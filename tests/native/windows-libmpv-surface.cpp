// Isolated in-process HWND/libmpv boundary; no WebView or application state.
// Main owns HWND lifetime/geometry. One worker owns every mpv call and DLL.
// WM_CLOSE requests shutdown; HWNDs survive until core/DLL teardown completes.
#include <windows.h>
#include <mpv/client.h>
#include <atomic>
#include <chrono>
#include <cstdio>
#include <exception>
#include <functional>
#include <initializer_list>
#include <stdexcept>
#include <string>
#include <thread>

constexpr UINT finished_message = WM_APP + 1;
constexpr UINT resize_message = WM_APP + 2;
struct State {
    HWND window = nullptr, video = nullptr;
    std::atomic<bool> stopping{false};
    std::atomic<unsigned> resize_ack{0};
    int result = 1; // Published by the finished message; read after worker join.
};

static LRESULT CALLBACK window_proc(HWND window, UINT msg, WPARAM wp, LPARAM lp) {
    auto *state = reinterpret_cast<State *>(GetWindowLongPtrW(window, GWLP_USERDATA));
    if (msg == WM_NCCREATE) {
        state = static_cast<State *>(reinterpret_cast<CREATESTRUCTW *>(lp)->lpCreateParams);
        SetWindowLongPtrW(window, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(state));
    }
    if (state) {
        if (msg == WM_SIZE && state->video) {
            MoveWindow(state->video, 0, 0, LOWORD(lp), HIWORD(lp), TRUE);
            return 0;
        }
        if (msg == resize_message) {
            SetWindowPos(window, nullptr, 0, 0, static_cast<int>(wp), static_cast<int>(lp),
                         SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE);
            state->resize_ack.fetch_add(1);
            return 0;
        }
        if (msg == WM_CLOSE) {
            std::printf("WINDOW CLOSE REQUEST hwnd_alive=%d\n", IsWindow(state->video));
            state->stopping.store(true);
            return 0;
        }
        if (msg == finished_message) { DestroyWindow(window); return 0; }
        if (msg == WM_DESTROY) { PostQuitMessage(0); return 0; }
    }
    return DefWindowProcW(window, msg, wp, lp);
}

struct Api {
    HMODULE library = nullptr;
#define API(name) decltype(&mpv_##name) name = nullptr
    API(create); API(initialize); API(set_option_string); API(set_option);
    API(get_property); API(get_property_string); API(free); API(command);
    API(wait_event); API(terminate_destroy); API(request_log_messages); API(error_string);
#undef API
    void load(const wchar_t *path) {
        library = LoadLibraryExW(path, nullptr, LOAD_LIBRARY_SEARCH_DEFAULT_DIRS |
                                               LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR);
        if (!library) throw std::runtime_error("LoadLibraryEx failed: " + std::to_string(GetLastError()));
#define LOAD(name) name = reinterpret_cast<decltype(name)>(GetProcAddress(library, "mpv_" #name)); \
        if (!name) throw std::runtime_error("Missing mpv_" #name)
        LOAD(create); LOAD(initialize); LOAD(set_option_string); LOAD(set_option);
        LOAD(get_property); LOAD(get_property_string); LOAD(free); LOAD(command);
        LOAD(wait_event); LOAD(terminate_destroy); LOAD(request_log_messages); LOAD(error_string);
#undef LOAD
    }
};

static std::string utf8(const wchar_t *text) {
    int count = WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, text, -1, nullptr, 0, nullptr, nullptr);
    if (!count) throw std::runtime_error("Invalid UTF-16 path");
    std::string value(static_cast<size_t>(count), '\0');
    if (!WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, text, -1, value.data(), count, nullptr, nullptr))
        throw std::runtime_error("UTF-8 conversion failed");
    value.pop_back();
    return value;
}

static void run(State &state, const wchar_t *dll, const wchar_t *deps,
                const wchar_t *media_path, const wchar_t *subtitle_path,
                bool interactive, bool close_smoke) {
    Api api;
    mpv_handle *core = nullptr;
    DLL_DIRECTORY_COOKIE directory = nullptr;
    const auto handler = std::get_terminate();
    try {
        directory = AddDllDirectory(deps);
        if (!directory) throw std::runtime_error("AddDllDirectory failed");
        api.load(dll);
        wchar_t loaded[32768] = {};
        HMODULE ggml = GetModuleHandleW(L"ggml-base-0.dll");
        if (ggml && !GetModuleFileNameW(ggml, loaded, 32768)) throw std::runtime_error("GGML path failed");
        std::printf("HOST pid=%lu worker=%lu HWND=%p GGML=%ls handler_unchanged=%d\n",
                    GetCurrentProcessId(), GetCurrentThreadId(), state.video, loaded,
                    handler == std::get_terminate());
        if (handler != std::get_terminate()) throw std::runtime_error("Unexpected terminate-handler replacement");
        core = api.create();
        if (!core) throw std::runtime_error("mpv_create failed");
        auto checked = [&](int result) {
            if (result < 0) throw std::runtime_error(api.error_string(result));
        };
        const char *options[][2] = {{"config","no"},{"terminal","no"},{"idle","yes"},
            {"keep-open","yes"},{"pause","yes"},{"mute","yes"},{"vo","gpu"},
            {"gpu-api","d3d11"},{"gpu-context","d3d11"},{"hwdec","auto-copy"},
            {"osc","no"},{"input-default-bindings","no"},{"input-vo-keyboard","no"}};
        for (auto &option : options) {
            int result = api.set_option_string(core, option[0], option[1]);
            if (result < 0) throw std::runtime_error(std::string("Option ") + option[0] + ": " + api.error_string(result));
        }
        int64_t wid = reinterpret_cast<intptr_t>(state.video);
        checked(api.set_option(core, "wid", MPV_FORMAT_INT64, &wid));
        auto subtitles = utf8(subtitle_path), media = utf8(media_path);
        checked(api.request_log_messages(core, "v"));
        checked(api.initialize(core));
        auto command = [&](std::initializer_list<const char *> args) {
            const char *values[8] = {};
            if (args.size() >= 8) throw std::runtime_error("Command too large");
            size_t i = 0; for (auto arg : args) values[i++] = arg;
            int result = api.command(core, values);
            if (result < 0) throw std::runtime_error(std::string("Command ") + values[0] + ": " + api.error_string(result));
        };
        auto text = [&](const char *name) {
            char *raw = api.get_property_string(core, name);
            std::string value = raw ? raw : "";
            if (raw) api.free(raw);
            return value;
        };
        auto flag = [&](const char *name) {
            int value = 0;
            return api.get_property(core, name, MPV_FORMAT_FLAG, &value) >= 0 && value != 0;
        };
        auto position = [&] {
            double value = 0;
            checked(api.get_property(core,"time-pos",MPV_FORMAT_DOUBLE,&value));
            return value;
        };
        unsigned loaded_events = 0, stopped_events = 0;
        auto pump = [&](double seconds, const std::function<bool()> &condition) {
            auto end = std::chrono::steady_clock::now() + std::chrono::duration<double>(seconds);
            while (std::chrono::steady_clock::now() < end) {
                if (state.stopping.load()) throw std::runtime_error("User close before sequence completion");
                mpv_event *event = api.wait_event(core, .03);
                if (event->event_id == MPV_EVENT_LOG_MESSAGE) {
                    auto *log = static_cast<mpv_event_log_message *>(event->data);
                    std::printf("MPV [%s] %s", log->prefix, log->text);
                } else if (event->event_id == MPV_EVENT_END_FILE) {
                    auto *end_file = static_cast<mpv_event_end_file *>(event->data);
                    if (end_file->reason == MPV_END_FILE_REASON_STOP) ++stopped_events;
                    if (end_file->reason == MPV_END_FILE_REASON_ERROR)
                        throw std::runtime_error(std::string("Media error: ") + api.error_string(end_file->error));
                } else if (event->event_id == MPV_EVENT_FILE_LOADED) {
                    ++loaded_events;
                    std::printf("FILE LOADED generation=%u\n",loaded_events);
                } else if (event->event_id == MPV_EVENT_SHUTDOWN) {
                    throw std::runtime_error("Unexpected mpv shutdown");
                }
                if (condition()) return;
            }
            throw std::runtime_error("Stage deadline expired");
        };
        auto delay = [&](double seconds) {
            auto end = std::chrono::steady_clock::now() + std::chrono::duration<double>(seconds);
            pump(seconds + 2, [&] { return std::chrono::steady_clock::now() >= end; });
        };
        auto stage = [&](const char *name) {
            std::printf("STAGE %s pause=%d idle=%d position=%s subtitle=%s decoder=%s\n",name,
                flag("pause"),flag("idle-active"),text("time-pos").c_str(),
                text("sub-text").c_str(),text("hwdec-current").c_str());
        };
        auto seek = [&](const char *position) { command({"seek",position,"absolute+exact"}); };
        unsigned previous_load = loaded_events;
        command({"loadfile",media.c_str(),"replace"});
        pump(10,[&] { return loaded_events > previous_load && flag("vo-configured"); });
        command({"sub-add",subtitles.c_str(),"select"});
        seek("1");
        pump(5,[&] { return text("sub-text").find("Initial") != std::string::npos; });
        stage("initial"); delay(.4);
        seek("2.5");
        pump(5,[&] { return !flag("seeking") && text("sub-text").empty(); });
        stage("gap");
        seek("3.5");
        pump(5,[&] { return text("sub-text").find("Second cue") != std::string::npos; });
        stage("second");
        command({"sub-add","memory://1\n00:00:00,000 --> 00:00:06,000\nEdited café — 日本語\nSecond line\n","select"});
        pump(5,[&] { return text("sub-text").find("Edited") != std::string::npos; });
        stage("edited"); delay(.4);
        unsigned old_ack = state.resize_ack.load();
        if (!PostMessageW(state.window,resize_message,760,500)) throw std::runtime_error("Resize request failed");
        pump(5,[&] { return state.resize_ack.load() != old_ack; });
        RECT parent{}, child{};
        if (!GetClientRect(state.window,&parent) || !GetClientRect(state.video,&child) ||
            parent.right != child.right || parent.bottom != child.bottom)
            throw std::runtime_error("Child bounds mismatch");
        std::printf("RESIZE client=%ldx%ld child=%ldx%ld\n",parent.right,parent.bottom,child.right,child.bottom);
        double before_play = position();
        command({"set","pause","no"}); delay(.6); stage("playing");
        if (flag("pause") || position() < before_play + .1) throw std::runtime_error("Expected advancing playback");
        command({"set","pause","yes"}); stage("paused");
        if (!flag("pause")) throw std::runtime_error("Expected pause");
        unsigned previous_stop = stopped_events;
        command({"stop"});
        pump(5,[&] { return stopped_events > previous_stop && flag("idle-active"); }); stage("unloaded");
        previous_load = loaded_events;
        command({"loadfile",media.c_str(),"replace"});
        pump(10,[&] { return loaded_events > previous_load && flag("vo-configured") && !flag("idle-active"); });
        command({"sub-add",subtitles.c_str(),"select"});
        command({"set","pause","yes"}); seek("1");
        pump(5,[&] { return text("sub-text").find("Initial") != std::string::npos; });
        stage("reloaded"); delay(.4);
        std::printf("SEQUENCE PASS\n");
        if (interactive || close_smoke) {
            command({"set","loop-file","inf"});
            command({"set","pause","no"});
            if (close_smoke) {
                delay(.3);
                std::printf("SMOKE POST WM_CLOSE\n");
                if (!PostMessageW(state.window,WM_CLOSE,0,0))
                    throw std::runtime_error("Close request failed");
            }
            std::printf("INTERACTIVE READY\n");
            auto end = std::chrono::steady_clock::now() + std::chrono::seconds(close_smoke ? 5 : 120);
            while (!state.stopping.load() && std::chrono::steady_clock::now() < end) {
                // Consume messages during the visible run, without blocking the UI.
                mpv_event *event = api.wait_event(core,.05);
                if (event->event_id == MPV_EVENT_LOG_MESSAGE) {
                    auto *log = static_cast<mpv_event_log_message *>(event->data);
                    std::printf("MPV [%s] %s",log->prefix,log->text);
                } else if (event->event_id == MPV_EVENT_END_FILE &&
                           static_cast<mpv_event_end_file *>(event->data)->reason == MPV_END_FILE_REASON_ERROR) {
                    throw std::runtime_error("Interactive media error");
                }
            }
            std::printf("INTERACTIVE END reason=%s\n",state.stopping.load() ? "window-close" : "deadline");
            if (close_smoke && !state.stopping.load())
                throw std::runtime_error("Close request not acknowledged");
        }
        state.result = 0;
    } catch (const std::exception &error) {
        std::printf("ERROR %s\n",error.what()); state.result = 1;
    }
    if (core) { api.terminate_destroy(core); std::printf("CORE DESTROYED hwnd_alive=%d\n",IsWindow(state.video)); }
    if (api.library && !FreeLibrary(api.library)) { state.result=1; std::printf("DLL UNLOAD ERROR\n"); }
    if (directory) RemoveDllDirectory(directory);
    if (handler != std::get_terminate()) { state.result=1; std::printf("HANDLER CHANGED\n"); }
    std::printf("WORKER CLEANUP result=%d hwnd_alive=%d handler_unchanged=%d\n",
                state.result,IsWindow(state.video),handler==std::get_terminate());
    PostMessageW(state.window,finished_message,0,0);
}

int wmain(int argc, wchar_t **argv) {
    if (argc!=6 && argc!=7) return 2;
    bool interactive=argc==7 && std::wstring(argv[6])==L"--interactive";
    bool close_smoke=argc==7 && std::wstring(argv[6])==L"--close-smoke";
    if (argc==7 && !interactive && !close_smoke) return 2;
    if (std::wstring(argv[5])!=L"--run") return 2;
    setvbuf(stdout,nullptr,_IOLBF,0);
    if (!SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_DEFAULT_DIRS)) return 1;
    SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    HINSTANCE instance=GetModuleHandleW(nullptr);
    WNDCLASSW wc{};wc.lpfnWndProc=window_proc;wc.hInstance=instance;
    wc.lpszClassName=L"VSFInProcessSurface";wc.hCursor=LoadCursorW(nullptr,IDC_ARROW);
    if (!RegisterClassW(&wc)) return 1;
    State state;
    state.window=CreateWindowExW(0,wc.lpszClassName,L"VSF in-process libmpv boundary probe",
        WS_OVERLAPPEDWINDOW|WS_CLIPCHILDREN,CW_USEDEFAULT,CW_USEDEFAULT,1000,700,nullptr,nullptr,instance,&state);
    if (!state.window) return 1;
    state.video=CreateWindowExW(0,L"STATIC",L"",WS_CHILD|WS_VISIBLE|WS_CLIPCHILDREN|SS_BLACKRECT,
        0,0,980,660,state.window,nullptr,instance,nullptr);
    if (!state.video) { DestroyWindow(state.window); return 1; }
    RECT size{};GetClientRect(state.window,&size);
    MoveWindow(state.video,0,0,size.right,size.bottom,TRUE);
    std::thread worker;
    try { worker=std::thread(run,std::ref(state),argv[1],argv[2],argv[3],argv[4],interactive,close_smoke); }
    catch (...) { DestroyWindow(state.window); return 1; }
    ShowWindow(state.window,SW_SHOW);
    MSG msg{};int status;
    while ((status=GetMessageW(&msg,nullptr,0,0))>0) { TranslateMessage(&msg);DispatchMessageW(&msg); }
    // Normal WM_QUIT follows worker native cleanup, never an early WM_CLOSE.
    worker.join();
    return status<0 ? 1 : state.result;
}
