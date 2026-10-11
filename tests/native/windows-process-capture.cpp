// Isolated Windows process-tree loopback diagnostic. No microphone or whole-mix fallback.
// CLI: PID SECONDS OUTPUT.f32 READY_FILE. Windows 10 build 20348+ / Windows 11.
// Activation ABI: Microsoft's audioclientactivationparams.h (win32metadata).
// Packet timestamps: IAudioCaptureClient::GetBuffer, in 100 ns QPC units.
#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include <audioclient.h>
#include <mmdeviceapi.h>
#include <wrl/client.h>
#include <atomic>
#include <cstdio>
#include <cstring>
#include <cwchar>
#include <sstream>
#include <stdexcept>
#include <string>
#include <vector>
using Microsoft::WRL::ComPtr;

static void require(HRESULT hr, const char* operation) {
    if (FAILED(hr)) {
        char message[160];
        std::snprintf(message, sizeof(message), "%s: HRESULT 0x%08lx", operation,
                      static_cast<unsigned long>(hr));
        throw std::runtime_error(message);
    }
}
struct Handle {
    HANDLE value;
    explicit Handle(HANDLE h, const char* operation = "create native handle") : value(h) {
        if (!h || h == INVALID_HANDLE_VALUE)
            throw std::runtime_error(std::string(operation) + ": Win32 error " + std::to_string(GetLastError()));
    }
    ~Handle() { CloseHandle(value); }
    Handle(const Handle&) = delete;
    Handle& operator=(const Handle&) = delete;
};

// MinGW currently lacks audioclientactivationparams.h. This is the documented
// 12-byte ABI, with explicit 32-bit enum fields: activation=process loopback,
// mode=include target tree. No alternative capture scope is offered.
struct ProcessActivation { UINT32 type = 1; DWORD pid; UINT32 mode = 0; };
static_assert(sizeof(ProcessActivation) == 12);
class Activation final : public IActivateAudioInterfaceCompletionHandler, public IAgileObject {
    std::atomic<ULONG> refs{1};
public:
    Handle done{CreateEventW(nullptr, TRUE, FALSE, nullptr)};
    ProcessActivation parameters;
    HRESULT result = E_PENDING;
    ComPtr<IAudioClient> client;
    explicit Activation(DWORD pid) : parameters{1, pid, 0} {}
    HRESULT STDMETHODCALLTYPE QueryInterface(REFIID id, void** out) override {
        if (!out) return E_POINTER;
        *out = nullptr;
        if (id == __uuidof(IUnknown) || id == __uuidof(IActivateAudioInterfaceCompletionHandler))
            *out = static_cast<IActivateAudioInterfaceCompletionHandler*>(this);
        else if (id == __uuidof(IAgileObject)) *out = static_cast<IAgileObject*>(this);
        else return E_NOINTERFACE;
        AddRef();
        return S_OK;
    }
    ULONG STDMETHODCALLTYPE AddRef() override { return ++refs; }
    ULONG STDMETHODCALLTYPE Release() override {
        ULONG remaining = --refs;
        if (!remaining) delete this;
        return remaining;
    }
    HRESULT STDMETHODCALLTYPE ActivateCompleted(IActivateAudioInterfaceAsyncOperation* operation) override {
        ComPtr<IUnknown> object;
        HRESULT activated = E_UNEXPECTED;
        result = operation->GetActivateResult(&activated, &object);
        if (SUCCEEDED(result)) result = activated;
        if (SUCCEEDED(result)) result = object.As(&client);
        // Publish the result/client once. The MTA owner reads only after this
        // event; Windows retains the callback while activation is outstanding.
        SetEvent(done.value);
        return S_OK;
    }
};
struct Packet { UINT64 offset, device_frame, qpc_100ns; UINT32 frames; DWORD flags; };
struct StopCapture {
    IAudioClient* client;
    ~StopCapture() { if (client) client->Stop(); }
};
static void write_all(HANDLE file, const void* data, size_t bytes) {
    DWORD written = 0;
    if (bytes > MAXDWORD || !WriteFile(file, data, static_cast<DWORD>(bytes), &written, nullptr)
        || written != bytes) throw std::runtime_error("capture file write failed");
}
static unsigned long number(const wchar_t* text) {
    wchar_t* end = nullptr;
    unsigned long value = std::wcstoul(text, &end, 10);
    if (!*text || *end || *text == L'-') throw std::runtime_error("invalid integer argument");
    return value;
}
static void capture(DWORD pid, unsigned seconds, const wchar_t* output, const wchar_t* ready) {
    Handle target{OpenProcess(SYNCHRONIZE, FALSE, pid), "open target process"};
    if (WaitForSingleObject(target.value, 0) != WAIT_TIMEOUT)
        throw std::runtime_error("target process has exited");
    // Exclusive creation protects previous evidence. Files may remain empty on failure.
    Handle pcm_file{CreateFileW(output, GENERIC_WRITE, 0, nullptr, CREATE_NEW, FILE_ATTRIBUTE_NORMAL, nullptr), "create PCM output"};
    std::wstring meta_path = std::wstring(output) + L".json";
    Handle meta_file{CreateFileW(meta_path.c_str(), GENERIC_WRITE, 0, nullptr, CREATE_NEW, FILE_ATTRIBUTE_NORMAL, nullptr), "create capture metadata"};
    ComPtr<Activation> activation;
    activation.Attach(new Activation(pid));
    PROPVARIANT params{};
    params.vt = VT_BLOB;
    params.blob.cbSize = sizeof(activation->parameters);
    params.blob.pBlobData = reinterpret_cast<BYTE*>(&activation->parameters);
    ComPtr<IActivateAudioInterfaceAsyncOperation> operation;
    require(ActivateAudioInterfaceAsync(L"VAD\\Process_Loopback", __uuidof(IAudioClient),
                                       &params, activation.Get(), &operation), "activate process capture");
    if (WaitForSingleObject(activation->done.value, 5000) != WAIT_OBJECT_0) {
        // Activation cannot be cancelled safely. End this isolated diagnostic
        // process without destroying memory still reachable by its callback.
        std::fprintf(stderr, "capture activation timed out\n");
        ExitProcess(1);
    }
    require(activation->result, "activation result");
    ComPtr<IAudioClient> client = activation->client;
    WAVEFORMATEX format{};
    format.wFormatTag = WAVE_FORMAT_IEEE_FLOAT;
    format.nChannels = 2;
    format.nSamplesPerSec = 48000;
    format.wBitsPerSample = 32;
    format.nBlockAlign = 8;
    format.nAvgBytesPerSec = 384000;
    require(client->Initialize(AUDCLNT_SHAREMODE_SHARED,
        AUDCLNT_STREAMFLAGS_LOOPBACK | AUDCLNT_STREAMFLAGS_EVENTCALLBACK | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
        0, 0, &format, nullptr), "initialize capture");
    Handle available{CreateEventW(nullptr, FALSE, FALSE, nullptr)};
    require(client->SetEventHandle(available.value), "capture event");
    ComPtr<IAudioCaptureClient> reader;
    require(client->GetService(__uuidof(IAudioCaptureClient), reinterpret_cast<void**>(reader.GetAddressOf())), "capture service");
    const size_t capacity_frames = (seconds + 1) * 48000;
    std::vector<float> pcm(capacity_frames * 2);
    std::vector<Packet> packets;
    packets.reserve((seconds + 1) * 1000);
    UINT64 total_frames = 0;
    require(client->Start(), "start capture");
    {
        StopCapture stop{client.Get()};
        Handle ready_file{CreateFileW(ready, GENERIC_WRITE, FILE_SHARE_READ, nullptr, CREATE_NEW, FILE_ATTRIBUTE_NORMAL, nullptr)};
        const char message[] = "READY windows-process-loopback\n";
        write_all(ready_file.value, message, sizeof(message) - 1);
        const ULONGLONG deadline = GetTickCount64() + seconds * 1000;
        while (GetTickCount64() < deadline) {
            DWORD wait = WaitForSingleObject(available.value, 100);
            if (wait == WAIT_FAILED) throw std::runtime_error("capture event wait failed");
            UINT32 pending = 0;
            require(reader->GetNextPacketSize(&pending), "packet size");
            while (pending) {
                BYTE* data = nullptr;
                UINT32 frames = 0;
                DWORD flags = 0;
                UINT64 position = 0, qpc = 0;
                require(reader->GetBuffer(&data, &frames, &flags, &position, &qpc), "capture buffer");
                if (total_frames + frames > capacity_frames || packets.size() == packets.capacity()) {
                    reader->ReleaseBuffer(frames);
                    throw std::runtime_error("capture exceeded preallocated bounds");
                }
                if (!(flags & AUDCLNT_BUFFERFLAGS_SILENT)) {
                    if (!data && frames) {
                        reader->ReleaseBuffer(frames);
                        throw std::runtime_error("non-silent packet has no data");
                    }
                    std::memcpy(pcm.data() + total_frames * 2, data, frames * 8);
                }
                // Silent frames remain explicitly zero in the preallocated buffer.
                packets.push_back({total_frames, position, qpc, frames, flags});
                total_frames += frames;
                require(reader->ReleaseBuffer(frames), "release packet");
                require(reader->GetNextPacketSize(&pending), "next packet size");
            }
        }
        require(client->Stop(), "stop capture");
        stop.client = nullptr;
    }
    if (!total_frames) throw std::runtime_error("capture produced no frames");
    write_all(pcm_file.value, pcm.data(), static_cast<size_t>(total_frames) * 8);
    std::ostringstream meta;
    meta << "{\"capture_kind\":\"windows-process-loopback\",\"player_pid\":" << pid
         << ",\"sample_rate\":48000,\"channels\":2,\"format\":\"float32le\",\"frames\":" << total_frames
         << ",\"requested_seconds\":" << seconds << ",\"packets\":[";
    for (size_t i = 0; i < packets.size(); ++i) {
        const auto& p = packets[i];
        if (i) meta << ',';
        meta << "{\"frame_offset\":" << p.offset << ",\"device_frame\":" << p.device_frame
             << ",\"qpc_100ns\":" << p.qpc_100ns << ",\"frames\":" << p.frames << ",\"flags\":" << p.flags << '}';
    }
    meta << "]}\n";
    auto text = meta.str();
    write_all(meta_file.value, text.data(), text.size());
    std::printf("CAPTURE COMPLETE frames=%llu packets=%zu\n", static_cast<unsigned long long>(total_frames), packets.size());
}
int wmain(int argc, wchar_t** argv) {
    if (argc == 2 && std::wcscmp(argv[1], L"--clock") == 0) {
        LARGE_INTEGER counter{}, frequency{};
        if (!QueryPerformanceFrequency(&frequency) || !QueryPerformanceCounter(&counter)) return 1;
        std::printf("{\"ticks\":%lld,\"frequency\":%lld}\n", counter.QuadPart, frequency.QuadPart);
        return 0;
    }
    HRESULT initialized = CoInitializeEx(nullptr, COINIT_MULTITHREADED);
    int result = 1;
    try {
        require(initialized, "COM initialization");
        if (argc != 5) throw std::runtime_error("usage: PID SECONDS OUTPUT.f32 READY_FILE");
        auto pid = number(argv[1]);
        auto seconds = number(argv[2]);
        if (pid <= 4 || seconds < 1 || seconds > 60) throw std::runtime_error("invalid PID or duration outside [1,60]");
        capture(pid, seconds, argv[3], argv[4]);
        result = 0;
    } catch (const std::exception& error) {
        std::fprintf(stderr, "CAPTURE FAILED: %s\n", error.what());
    }
    if (SUCCEEDED(initialized)) CoUninitialize();
    return result;
}
