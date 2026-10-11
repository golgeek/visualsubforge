// Diagnostic only: capture one explicitly supplied test-player PID, never a
// global mix or microphone. Public CoreAudio process taps require macOS 14.2+.
// The private aggregate is never selected as a system input/output device.
#import <CoreAudio/CoreAudio.h>
#import <CoreAudio/CATapDescription.h>
#import <CoreAudio/AudioHardwareTapping.h>
#import <Foundation/Foundation.h>
#include <atomic>
#include <chrono>
#include <cmath>
#include <cstdio>
#include <csignal>
#include <cstring>
#include <stdexcept>
#include <string>
#include <thread>
#include <vector>
#include <mach/mach_time.h>

static volatile sig_atomic_t interrupted = 0;
static void interrupt(int) { interrupted = 1; }
static void check(OSStatus status, const char *operation) {
    if (status != noErr)
        throw std::runtime_error(std::string(operation) + ": " + std::to_string(status));
}
struct Block {
    uint64_t hostTime;
    double sampleTime;
    uint32_t flags;
    size_t firstFrame, frames;
};
struct Capture {
    AudioObjectID tap = kAudioObjectUnknown, aggregate = kAudioObjectUnknown;
    AudioDeviceIOProcID io = nullptr;
    bool started = false;
    AudioStreamBasicDescription format{};
    std::vector<float> samples;
    std::vector<Block> blocks;
    size_t frames = 0, blockCount = 0;
    std::atomic<bool> overflow{false}, badLayout{false};

    void stop() {
        if (started) { AudioDeviceStop(aggregate, io); started = false; }
        // Destroy the callback before reading its buffers or releasing its state.
        if (io) { AudioDeviceDestroyIOProcID(aggregate, io); io = nullptr; }
    }
    ~Capture() {
        stop();
        if (aggregate) AudioHardwareDestroyAggregateDevice(aggregate);
        if (tap) AudioHardwareDestroyProcessTap(tap);
    }
    Capture() = default;
    Capture(const Capture &) = delete;
    Capture &operator=(const Capture &) = delete;
};

static OSStatus receive(AudioObjectID, const AudioTimeStamp *, const AudioBufferList *input,
                        const AudioTimeStamp *timestamp, AudioBufferList *,
                        const AudioTimeStamp *, void *context) {
    auto &c = *static_cast<Capture *>(context);
    if (!input || !input->mNumberBuffers) return noErr;
    const bool planar = (c.format.mFormatFlags & kAudioFormatFlagIsNonInterleaved) != 0;
    const unsigned channels = c.format.mChannelsPerFrame;
    if (input->mNumberBuffers != (planar ? channels : 1)) {
        c.badLayout.store(true, std::memory_order_relaxed); return noErr;
    }
    const auto &first = input->mBuffers[0];
    const size_t count = first.mDataByteSize / (sizeof(float) * (planar ? 1 : channels));
    if (!count) return noErr;
    if (c.frames + count > c.samples.size() / channels || c.blockCount == c.blocks.size()) {
        c.overflow.store(true, std::memory_order_relaxed); return noErr;
    }
    for (unsigned b = 0; b < input->mNumberBuffers; ++b) {
        const auto &buffer = input->mBuffers[b];
        if (!buffer.mData || buffer.mNumberChannels != (planar ? 1 : channels) ||
            buffer.mDataByteSize != first.mDataByteSize) {
            c.badLayout.store(true, std::memory_order_relaxed); return noErr;
        }
    }
    float *out = c.samples.data() + c.frames * channels;
    if (planar) {
        for (size_t i = 0; i < count; ++i)
            for (unsigned ch = 0; ch < channels; ++ch)
                out[i * channels + ch] = static_cast<const float *>(input->mBuffers[ch].mData)[i];
    } else {
        std::memcpy(out, first.mData, first.mDataByteSize);
    }
    c.blocks[c.blockCount++] = {timestamp->mHostTime, timestamp->mSampleTime,
                               timestamp->mFlags, c.frames, count};
    c.frames += count;
    return noErr;
}

int main(int argc, char **argv) {
    @autoreleasepool {
        try {
            if (argc != 4) throw std::runtime_error("usage: process-audio-capture PID SECONDS OUTPUT.f32");
            const int pid = std::stoi(argv[1]);
            const double seconds = std::stod(argv[2]);
            if (pid <= 0 || !std::isfinite(seconds) || seconds <= 0 || seconds > 30)
                throw std::runtime_error("positive PID and duration in (0, 30] required");
            Capture c;
            std::fprintf(stderr, "PHASE find process\n");
            AudioObjectID process = kAudioObjectUnknown;
            AudioObjectPropertyAddress address{kAudioHardwarePropertyTranslatePIDToProcessObject,
                                                kAudioObjectPropertyScopeGlobal, kAudioObjectPropertyElementMain};
            UInt32 size = sizeof(process);
            check(AudioObjectGetPropertyData(kAudioObjectSystemObject, &address, sizeof(pid), &pid,
                                             &size, &process), "find audio process");
            if (!process) throw std::runtime_error("PID has no CoreAudio process object");
            CATapDescription *description = [[CATapDescription alloc] initStereoMixdownOfProcesses:@[@(process)]];
            description.name = @"VSF test-player audio diagnostic";
            description.privateTap = YES;
            description.muteBehavior = CATapUnmuted;
            std::fprintf(stderr, "PHASE create tap (macOS may request audio-capture permission)\n");
            check(AudioHardwareCreateProcessTap(description, &c.tap), "create process tap");
            std::fprintf(stderr, "PHASE read tap format\n");
            address.mSelector = kAudioTapPropertyFormat;
            size = sizeof(c.format);
            check(AudioObjectGetPropertyData(c.tap, &address, 0, nullptr, &size, &c.format), "read tap format");
            if (c.format.mFormatID != kAudioFormatLinearPCM || c.format.mBitsPerChannel != 32 ||
                !(c.format.mFormatFlags & kAudioFormatFlagIsFloat) ||
                (c.format.mFormatFlags & kAudioFormatFlagIsBigEndian) ||
                c.format.mChannelsPerFrame != 2 || c.format.mSampleRate < 8000 ||
                c.format.mSampleRate > 192000)
                throw std::runtime_error("unsupported tap format");
            c.samples.resize(static_cast<size_t>((seconds + 1) * c.format.mSampleRate) * 2);
            c.blocks.resize(20000);
            NSDictionary *settings = @{
                @kAudioAggregateDeviceNameKey: @"VSF private diagnostic tap",
                @kAudioAggregateDeviceUIDKey: [[NSUUID UUID] UUIDString],
                @kAudioAggregateDeviceIsPrivateKey: @YES,
                @kAudioAggregateDeviceTapAutoStartKey: @NO,
                @kAudioAggregateDeviceTapListKey: @[@{
                    @kAudioSubTapUIDKey: description.UUID.UUIDString,
                    @kAudioSubTapDriftCompensationKey: @YES
                }]
            };
            check(AudioHardwareCreateAggregateDevice((__bridge CFDictionaryRef)settings, &c.aggregate), "create private aggregate");
            std::fprintf(stderr, "PHASE start private aggregate\n");
            check(AudioDeviceCreateIOProcID(c.aggregate, receive, &c, &c.io), "create callback");
            signal(SIGINT, interrupt);
            signal(SIGTERM, interrupt);
            check(AudioDeviceStart(c.aggregate, c.io), "start capture");
            c.started = true;
            std::printf("READY pid=%d rate=%.0f channels=2\n", pid, c.format.mSampleRate);
            std::fflush(stdout);
            auto end = std::chrono::steady_clock::now() + std::chrono::duration<double>(seconds);
            while (!interrupted && std::chrono::steady_clock::now() < end)
                std::this_thread::sleep_for(std::chrono::milliseconds(20));
            c.stop();
            if (c.overflow || c.badLayout || !c.frames)
                throw std::runtime_error("capture overflow, unsupported buffers, or no callbacks");
            // File IO occurs only after all realtime callbacks have stopped.
            FILE *file = std::fopen(argv[3], "wbx");
            if (!file) throw std::runtime_error("cannot create capture output (must not exist)");
            const size_t written = std::fwrite(c.samples.data(), sizeof(float), c.frames * 2, file);
            const int closeStatus = std::fclose(file);
            if (written != c.frames * 2 || closeStatus) throw std::runtime_error("capture write failed");
            NSMutableArray *blocks = [NSMutableArray arrayWithCapacity:c.blockCount];
            for (size_t i = 0; i < c.blockCount; ++i) {
                const auto &b = c.blocks[i];
                [blocks addObject:@{@"host_ticks": @(b.hostTime), @"sample_time": @(b.sampleTime),
                                    @"flags": @(b.flags), @"first_frame": @(b.firstFrame), @"frames": @(b.frames)}];
            }
            NSDictionary *metadata = @{@"pid": @(pid), @"sample_rate": @(c.format.mSampleRate),
                                       @"channels": @2, @"frames": @(c.frames), @"blocks": blocks};
            mach_timebase_info_data_t timebase{};
            mach_timebase_info(&timebase);
            NSMutableDictionary *timedMetadata = [metadata mutableCopy];
            timedMetadata[@"host_time_numer"] = @(timebase.numer);
            timedMetadata[@"host_time_denom"] = @(timebase.denom);
            NSData *json = [NSJSONSerialization dataWithJSONObject:timedMetadata options:0 error:nil];
            NSString *metadataPath = [[NSString stringWithUTF8String:argv[3]] stringByAppendingString:@".json"];
            if (![json writeToFile:metadataPath options:NSDataWritingWithoutOverwriting error:nil])
                throw std::runtime_error("cannot create capture metadata");
            std::printf("CAPTURED frames=%zu blocks=%zu\n", c.frames, c.blockCount);
            return 0;
        } catch (const std::exception &error) {
            std::fprintf(stderr, "%s\n", error.what()); return 1;
        }
    }
}
