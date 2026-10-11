// Standalone process-only capture reference: no mpv, FFmpeg, UI, or clock mapper.
// CLI: ordinary|refresh|loop-clear CAPTURE_READY_FILE NEW_REPORT.json
// One MTA thread owns every WASAPI interface. Audio samples and trace storage are
// allocated before Start; no file IO/logging occurs during playback. Stop runs
// before interface release, including error cleanup. Device/QPC estimates are
// observations, not verified speaker presentation.
#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include <audioclient.h>
#include <mmdeviceapi.h>
#include <mmreg.h>
#include <ks.h>
#include <ksmedia.h>
#include <wrl/client.h>
#include <cmath>
#include <cstdio>
#include <cstring>
#include <cwchar>
#include <iomanip>
#include <sstream>
#include <stdexcept>
#include <string>
#include <vector>
using Microsoft::WRL::ComPtr;
constexpr UINT32 rate = 48000, source_end = 4 * rate;
constexpr UINT32 loop_a = 48624, loop_b = 64128;

static void check(HRESULT hr, const char *operation) {
    if (FAILED(hr)) {
        char text[160];
        std::snprintf(text, sizeof(text), "%s: HRESULT 0x%08lx", operation,
                      static_cast<unsigned long>(hr));
        throw std::runtime_error(text);
    }
}
struct Handle {
    HANDLE value;
    explicit Handle(HANDLE v) : value(v) {
        if (!v || v == INVALID_HANDLE_VALUE) throw std::runtime_error("native handle creation failed");
    }
    ~Handle() { CloseHandle(value); }
    Handle(const Handle &) = delete;
    Handle &operator=(const Handle &) = delete;
};
static UINT64 qpc() {
    LARGE_INTEGER v{};
    if (!QueryPerformanceCounter(&v)) throw std::runtime_error("QPC failed");
    return v.QuadPart;
}
struct Stop {
    IAudioClient *client;
    ~Stop() { client->Stop(); }
};
struct Sample {
    UINT64 before, after, position, clock_qpc, written;
    UINT32 padding, epoch;
    HRESULT status;
};
struct Epoch { UINT32 index, source_start; UINT64 qpc_before, qpc_after; };
struct Report {
    UINT64 qpc_frequency = 0, clock_frequency = 0, started = 0, ended = 0;
    UINT32 buffer_frames = 0;
    REFERENCE_TIME default_period = 0, minimum_period = 0;
    std::vector<Sample> samples;
    std::vector<Epoch> epochs;
};

static void run(const wchar_t *mode, const wchar_t *ready, Report &report) {
    const bool loop_mode = std::wcscmp(mode, L"loop-clear") == 0;
    const bool refresh = std::wcscmp(mode, L"refresh") == 0;
    if (!loop_mode && !refresh && std::wcscmp(mode, L"ordinary"))
        throw std::runtime_error("unknown mode");
    LARGE_INTEGER frequency{};
    if (!QueryPerformanceFrequency(&frequency)) throw std::runtime_error("QPC frequency failed");
    report.qpc_frequency = frequency.QuadPart;
    report.samples.reserve(8192);
    report.epochs.reserve(2);
    std::vector<float> source(source_end * 2);
    double phase = 0;
    const double tau = 2 * std::acos(-1.0);
    for (UINT32 frame = 0; frame < source_end; ++frame) {
        double hz = frame < 72000 ? 440 : frame < 120000 ? 660 : 880;
        source[2 * frame] = source[2 * frame + 1] = static_cast<float>(.25 * std::sin(phase));
        phase = std::fmod(phase + tau * hz / rate, tau);
    }
    ComPtr<IMMDeviceEnumerator> enumerator;
    check(CoCreateInstance(__uuidof(MMDeviceEnumerator), nullptr, CLSCTX_ALL,
                           __uuidof(IMMDeviceEnumerator), reinterpret_cast<void **>(enumerator.GetAddressOf())), "enumerator");
    ComPtr<IMMDevice> device;
    check(enumerator->GetDefaultAudioEndpoint(eRender, eMultimedia, &device), "default render endpoint");
    ComPtr<IAudioClient> client;
    check(device->Activate(__uuidof(IAudioClient), CLSCTX_ALL, nullptr,
                           reinterpret_cast<void **>(client.GetAddressOf())), "activate render client");
    WAVEFORMATEXTENSIBLE format{};
    format.Format.wFormatTag = WAVE_FORMAT_EXTENSIBLE;
    format.Format.nChannels = 2;
    format.Format.nSamplesPerSec = rate;
    format.Format.wBitsPerSample = 32;
    format.Format.nBlockAlign = 8;
    format.Format.nAvgBytesPerSec = rate * 8;
    format.Format.cbSize = sizeof(WAVEFORMATEXTENSIBLE) - sizeof(WAVEFORMATEX);
    format.Samples.wValidBitsPerSample = 32;
    format.dwChannelMask = SPEAKER_FRONT_LEFT | SPEAKER_FRONT_RIGHT;
    // SDK DEFINE_WAVEFORMATEX_GUID(WAVE_FORMAT_IEEE_FLOAT), with explicit
    // aggregate braces; the MinGW named GUID is an unresolved external here.
    format.SubFormat = {WAVE_FORMAT_IEEE_FLOAT, 0, 0x0010,
                        {0x80, 0x00, 0x00, 0xaa, 0x00, 0x38, 0x9b, 0x71}};
    check(client->Initialize(AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
                             0, 0, &format.Format, nullptr), "initialize shared float48k stereo");
    Stop stop{client.Get()};
    check(client->GetBufferSize(&report.buffer_frames), "buffer size");
    check(client->GetDevicePeriod(&report.default_period, &report.minimum_period), "device period");
    Handle event(CreateEventW(nullptr, FALSE, FALSE, nullptr));
    check(client->SetEventHandle(event.value), "render event");
    ComPtr<IAudioRenderClient> writer;
    ComPtr<IAudioClock> clock;
    check(client->GetService(__uuidof(IAudioRenderClient), reinterpret_cast<void **>(writer.GetAddressOf())), "render service");
    check(client->GetService(__uuidof(IAudioClock), reinterpret_cast<void **>(clock.GetAddressOf())), "clock service");
    check(clock->GetFrequency(&report.clock_frequency), "clock frequency");
    if (!report.clock_frequency) throw std::runtime_error("zero device clock frequency");
    bool looping = loop_mode, transitioned = false;
    UINT32 source_frame = looping ? loop_a : 0, epoch = 0;
    UINT64 written = 0;
    auto fill = [&] {
        UINT32 padding = 0;
        check(client->GetCurrentPadding(&padding), "padding");
        if (padding > report.buffer_frames) throw std::runtime_error("padding exceeds buffer");
        UINT32 frames = report.buffer_frames - padding;
        if (!frames) return;
        BYTE *bytes = nullptr;
        check(writer->GetBuffer(frames, &bytes), "render buffer");
        auto samples = reinterpret_cast<float *>(bytes);
        for (UINT32 i = 0; i < frames; ++i) {
            if (looping && source_frame == loop_b) source_frame = loop_a;
            float value = source_frame < source_end ? source[2 * source_frame] : 0;
            samples[2 * i] = samples[2 * i + 1] = value;
            ++source_frame;
        }
        check(writer->ReleaseBuffer(frames, 0), "release render buffer");
        written += frames;
    };
    auto sample = [&] {
        Sample s{};
        s.epoch = epoch; s.written = written;
        s.before = qpc();
        s.status = clock->GetPosition(&s.position, &s.clock_qpc);
        s.after = qpc();
        check(s.status, "device clock position");
        check(client->GetCurrentPadding(&s.padding), "sample padding");
        if (report.samples.size() == report.samples.capacity()) throw std::runtime_error("trace capacity exceeded");
        report.samples.push_back(s);
        return s;
    };
    const auto ready_deadline = GetTickCount64() + 8000;
    bool ready_seen = false;
    while (GetTickCount64() < ready_deadline) {
        HANDLE file = CreateFileW(ready, GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_WRITE, nullptr,
                                  OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, nullptr);
        if (file != INVALID_HANDLE_VALUE) {
            char text[80]{}; DWORD count = 0;
            bool read = ReadFile(file, text, sizeof(text) - 1, &count, nullptr);
            CloseHandle(file);
            if (read && std::strcmp(text, "READY windows-process-loopback\n") == 0) { ready_seen = true; break; }
        }
        Sleep(5);
    }
    if (!ready_seen) throw std::runtime_error("capture ready timeout");
    fill();
    report.started = qpc();
    check(client->Start(), "start render");
    report.epochs.push_back({0, looping ? loop_a : 0, report.started, qpc()});
    const double duration = loop_mode ? 10 : 6;
    while ((qpc() - report.started) / static_cast<double>(report.qpc_frequency) < duration) {
        const Sample s = sample();
        double elapsed = (s.after - report.started) / static_cast<double>(report.qpc_frequency);
        if (!transitioned && (loop_mode || refresh) && elapsed >= (loop_mode ? 5 : 1)) {
            // Resolve a known source frame from this stream's device position.
            // There is no decoder queue: before reset, mapping is either linear
            // or the explicitly submitted fixed loop. Do not extrapolate QPC.
            UINT64 output_frame = static_cast<UINT64>(std::floor(
                static_cast<long double>(s.position) * rate / report.clock_frequency));
            UINT32 resolved = looping ? loop_a + output_frame % (loop_b - loop_a)
                                      : static_cast<UINT32>(output_frame);
            if (resolved >= source_end) throw std::runtime_error("transition beyond fixture");
            UINT64 before = qpc();
            check(client->Stop(), "stop for reset");
            check(client->Reset(), "reset");
            ++epoch; source_frame = resolved; written = 0; looping = false;
            fill();
            check(client->Start(), "restart");
            report.epochs.push_back({epoch, resolved, before, qpc()});
            transitioned = true;
            continue;
        }
        fill();
        DWORD waited = WaitForSingleObject(event.value, 100);
        if (waited != WAIT_OBJECT_0 && waited != WAIT_TIMEOUT) throw std::runtime_error("render event wait failed");
    }
    check(client->Stop(), "final stop");
    report.ended = qpc();
}

int wmain(int argc, wchar_t **argv) {
    if (argc != 4) { std::fprintf(stderr, "usage: ordinary|refresh|loop-clear READY NEW_REPORT.json\n"); return 2; }
    try {
        Handle output(CreateFileW(argv[3], GENERIC_WRITE, 0, nullptr, CREATE_NEW, FILE_ATTRIBUTE_NORMAL, nullptr));
        HRESULT initialized = CoInitializeEx(nullptr, COINIT_MULTITHREADED);
        Report r;
        std::string error;
        try { check(initialized, "COM initialization"); run(argv[1], argv[2], r); }
        catch (const std::exception &e) { error = e.what(); }
        if (SUCCEEDED(initialized)) CoUninitialize();
        std::ostringstream json;
        json << "{\"ok\":" << (error.empty() ? "true" : "false")
             << ",\"sample_rate\":" << rate << ",\"qpc_frequency\":" << r.qpc_frequency
             << ",\"clock_frequency\":" << r.clock_frequency << ",\"buffer_frames\":" << r.buffer_frames
             << ",\"default_period_100ns\":" << r.default_period << ",\"minimum_period_100ns\":" << r.minimum_period
             << ",\"started_qpc\":" << r.started << ",\"ended_qpc\":" << r.ended << ",\"epochs\":[";
        for (size_t i = 0; i < r.epochs.size(); ++i) {
            const auto &e = r.epochs[i]; if (i) json << ',';
            json << "{\"index\":" << e.index << ",\"source_start\":" << e.source_start
                 << ",\"before_qpc\":" << e.qpc_before << ",\"after_qpc\":" << e.qpc_after << '}';
        }
        json << "],\"samples\":[";
        for (size_t i = 0; i < r.samples.size(); ++i) {
            const auto &s = r.samples[i]; if (i) json << ',';
            json << "{\"before_qpc\":" << s.before << ",\"after_qpc\":" << s.after
                 << ",\"position\":" << s.position << ",\"clock_qpc_100ns\":" << s.clock_qpc
                 << ",\"written\":" << s.written << ",\"padding\":" << s.padding
                 << ",\"epoch\":" << s.epoch << ",\"status\":" << s.status << '}';
        }
        json << "]}\n";
        std::string text = json.str(); DWORD count = 0;
        if (text.size() > MAXDWORD || !WriteFile(output.value, text.data(), static_cast<DWORD>(text.size()), &count, nullptr)
            || count != text.size()) throw std::runtime_error("report write failed");
        if (!error.empty()) { std::fprintf(stderr, "FAILED: %s\n", error.c_str()); return 1; }
        std::printf("COMPLETE samples=%zu epochs=%zu\n", r.samples.size(), r.epochs.size());
        return 0;
    } catch (const std::exception &error) {
        std::fprintf(stderr, "FAILED: %s\n", error.what()); return 1;
    }
}
