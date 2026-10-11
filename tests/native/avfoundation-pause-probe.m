// Bounded diagnostic: public AVFoundation only; no mpv, UI, or decoder.
#import <AVFoundation/AVFoundation.h>
#import <Foundation/Foundation.h>
#import <CoreMedia/CoreMedia.h>
#include <stdio.h>
#include <string.h>
#include <stdlib.h>

static int fail(const char *message, OSStatus status) {
    fprintf(stderr, "%s: %d\n", message, (int)status);
    return 1;
}

int main(int argc, const char **argv) {
    @autoreleasepool {
        if (argc < 4 || argc > 6)
            return fail("usage: probe stereo-s16-48000.raw rate|resume-anchor|both-anchor chunk-frames [default|varispeed|time-domain|spectral] [immediate|buffered]", -1);
        const char *policy = argv[2];
        const char *algorithm = argc >= 5 ? argv[4] : "default";
        const char *rate_policy = argc == 6 ? argv[5] : "immediate";
        if (strcmp(rate_policy,"immediate") && strcmp(rate_policy,"buffered"))
            return fail("invalid rate buffering policy",-1);
        if (strcmp(algorithm,"default") && strcmp(algorithm,"varispeed") &&
            strcmp(algorithm,"time-domain") && strcmp(algorithm,"spectral"))
            return fail("invalid time/pitch algorithm",-1);
        char *end=NULL;
        long chunk_frames=strtol(argv[3],&end,10);
        if (!end || *end || chunk_frames<48 || chunk_frames>48000)
            return fail("invalid chunk size",-1);
        if (strcmp(policy,"rate") && strcmp(policy,"resume-anchor") && strcmp(policy,"both-anchor"))
            return fail("invalid policy",-1);
        NSData *pcm = [NSData dataWithContentsOfFile:[NSString stringWithUTF8String:argv[1]]];
        if (!pcm || pcm.length == 0 || pcm.length % 4 || pcm.length > 4*48000*10)
            return fail("invalid bounded PCM", -1);
        setvbuf(stdout, NULL, _IOLBF, 0);
        AVSampleBufferAudioRenderer *renderer = [[AVSampleBufferAudioRenderer alloc] init];
        // Select before enqueue/start. This isolates renderer processing from
        // the synchronizer's unchanged 0/1 rate and pause scheduling policy.
        if (!strcmp(algorithm,"varispeed")) renderer.audioTimePitchAlgorithm = AVAudioTimePitchAlgorithmVarispeed;
        if (!strcmp(algorithm,"time-domain")) renderer.audioTimePitchAlgorithm = AVAudioTimePitchAlgorithmTimeDomain;
        if (!strcmp(algorithm,"spectral")) renderer.audioTimePitchAlgorithm = AVAudioTimePitchAlgorithmSpectral;
        AVSampleBufferRenderSynchronizer *sync = [[AVSampleBufferRenderSynchronizer alloc] init];
        [sync addRenderer:renderer];
        // Preserve the previous/mpv policy by default. The optional comparison
        // changes only buffering on rate transitions, before playback starts.
        sync.delaysRateChangeUntilHasSufficientMediaData = !strcmp(rate_policy,"buffered");
        [sync setRate:0 time:kCMTimeZero];
        AudioStreamBasicDescription asbd = {
            .mSampleRate=48000, .mFormatID=kAudioFormatLinearPCM,
            .mFormatFlags=kAudioFormatFlagIsSignedInteger|kAudioFormatFlagIsPacked,
            .mBytesPerPacket=4, .mFramesPerPacket=1, .mBytesPerFrame=4,
            .mChannelsPerFrame=2, .mBitsPerChannel=16
        };
        CMAudioFormatDescriptionRef format = NULL;
        OSStatus status = CMAudioFormatDescriptionCreate(NULL, &asbd, 0, NULL, 0, NULL, NULL, &format);
        if (status) return fail("format",status);
        // Prequeue a bounded fixture in fixed-size buffers while the timebase is stopped.
        // No request callbacks or borrowed pointers survive this loop.
        for (size_t offset=0; offset<pcm.length; offset+=(size_t)chunk_frames*4) {
            size_t bytes=MIN((size_t)chunk_frames*4,pcm.length-offset);
            CMBlockBufferRef block=NULL;
            CMSampleBufferRef sample=NULL;
            status=CMBlockBufferCreateWithMemoryBlock(NULL,NULL,bytes,kCFAllocatorDefault,NULL,0,bytes,0,&block);
            if (!status) status=CMBlockBufferReplaceDataBytes((const char *)pcm.bytes+offset,block,0,bytes);
            CMSampleTimingInfo timing={CMTimeMake(1,48000),CMTimeMake(offset/4,48000),kCMTimeInvalid};
            size_t sample_size=4;
            if (!status) status=CMSampleBufferCreateReady(NULL,block,format,bytes/4,1,&timing,1,&sample_size,&sample);
            if (status) {
                if (block) CFRelease(block);
                CFRelease(format);
                return fail("sample",status);
            }
            [renderer enqueueSampleBuffer:sample];
            CFRelease(sample);
            CFRelease(block);
        }
        CFRelease(format);
        if (renderer.status == AVQueuedSampleBufferRenderingStatusFailed)
            return fail("renderer failed",(OSStatus)renderer.error.code);
        printf("READY frames=%lu algorithm=%s buffered_rate_changes=%d\n",(unsigned long)(pcm.length/4),
            renderer.audioTimePitchAlgorithm.UTF8String, sync.delaysRateChangeUntilHasSufficientMediaData);
        int command;
        while ((command=getchar()) != EOF && command!='q') {
            if (command!='p' && command!='r') continue;
            bool paused=command=='p';
            CMTime time=sync.currentTime;
            double host_before=CMTimeGetSeconds(CMClockGetTime(CMClockGetHostTimeClock()));
            double rate_before=CMTimebaseGetRate(sync.timebase);
            bool anchor=!strcmp(policy,"both-anchor") || (!paused && !strcmp(policy,"resume-anchor"));
            if (anchor) [sync setRate:paused ? 0 : 1 time:time];
            else [sync setRate:paused ? 0 : 1];
            printf("{\"paused\":%s,\"host_before\":%.9f,\"host_after\":%.9f,\"time_before\":%.9f,\"time_after\":%.9f,\"rate_before\":%.9f,\"rate_after\":%.9f,\"status\":%ld}\n",
                paused ? "true":"false",host_before,
                CMTimeGetSeconds(CMClockGetTime(CMClockGetHostTimeClock())),
                CMTimeGetSeconds(time),CMTimeGetSeconds(sync.currentTime),
                rate_before,CMTimebaseGetRate(sync.timebase),(long)renderer.status);
        }
        [sync setRate:0];
        [renderer flush];
        return renderer.status == AVQueuedSampleBufferRenderingStatusFailed ? 1 : 0;
    }
}
