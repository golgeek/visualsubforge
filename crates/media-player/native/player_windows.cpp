// Main owns HWND creation/layout/removal. One Rust control worker owns mpv;
// mpv owns its internal D3D11 renderer. Rust's Surface Arc outlives Playback.
// The Tauri exit gate retains the parent while worker teardown acknowledges
// main-thread child removal. Never join that worker from the UI thread.
#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include "mpv_client.hpp"
#include <algorithm>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <memory>
#include <mutex>

namespace {
constexpr UINT layout_message = WM_APP + 1;
constexpr UINT release_message = WM_APP + 2;
constexpr UINT occlusion_message = WM_APP + 3;
constexpr wchar_t surface_class[] = L"VisualSubForgeVideoClip";
struct Bounds {
    double x = 0, y = 0, width = 1, height = 1;
    bool visible = false, presenting = false;
};
struct Surface {
    HWND parent = nullptr, clip = nullptr, video = nullptr;
    DWORD ui_thread = 0;
    std::mutex mutex; // Protects desired bounds only; no Win32/mpv calls held.
    Bounds desired;
    bool queued = false;
    bool occluded = false; // Same mutex; layout/presentation never resets it.
};
struct Playback {
    mpv_handle *core = nullptr;
    Surface *surface = nullptr;
};

void apply_layout(Surface &surface) {
    Bounds bounds;
    bool occluded;
    {
        std::lock_guard lock(surface.mutex);
        bounds = surface.desired;
        surface.queued = false;
        occluded = surface.occluded;
    }
    RECT client{};
    if (!GetClientRect(surface.parent, &client)) return;
    // Web content uses logical pixels at the application's default page zoom.
    // Per-window DPI follows monitor scale; custom webview zoom is not supported.
    const double scale = GetDpiForWindow(surface.parent) / 96.0;
    const int x = static_cast<int>(std::lround(bounds.x * scale));
    const int y = static_cast<int>(std::lround(bounds.y * scale));
    const int width = std::max(1, static_cast<int>(std::lround(bounds.width * scale)));
    const int height = std::max(1, static_cast<int>(std::lround(bounds.height * scale)));
    const int left = std::max(0, x), top = std::max(0, y);
    const int right = std::min(static_cast<int>(client.right), x + width);
    const int bottom = std::min(static_cast<int>(client.bottom), y + height);
    const bool visible = !occluded && bounds.visible && bounds.presenting && right > left && bottom > top;
    if (!visible) {
        ShowWindow(surface.clip, SW_HIDE);
        return;
    }
    // Crop the child rather than shrinking its picture when partially scrolled.
    SetWindowPos(surface.clip, HWND_TOP, left, top, right - left, bottom - top,
                 SWP_NOACTIVATE);
    SetWindowPos(surface.video, nullptr, x - left, y - top, width, height,
                 SWP_NOACTIVATE | SWP_NOZORDER);
    ShowWindow(surface.clip, SW_SHOWNOACTIVATE);
}

LRESULT CALLBACK surface_proc(HWND window, UINT message, WPARAM wp, LPARAM lp) {
    auto *surface = reinterpret_cast<Surface *>(GetWindowLongPtrW(window, GWLP_USERDATA));
    if (message == WM_NCCREATE) {
        surface = static_cast<Surface *>(reinterpret_cast<CREATESTRUCTW *>(lp)->lpCreateParams);
        SetWindowLongPtrW(window, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(surface));
    }
    if (surface) {
        if (message == layout_message) { apply_layout(*surface); return 0; }
        if (message == occlusion_message) { apply_layout(*surface); return 1; }
        if (message == release_message) { DestroyWindow(window); return 0; }
        if (message == WM_MOUSEACTIVATE) return MA_NOACTIVATE;
        if (message == WM_NCDESTROY) SetWindowLongPtrW(window, GWLP_USERDATA, 0);
    }
    return DefWindowProcW(window, message, wp, lp);
}

// Caller holds the short state lock. At most one outstanding layout message;
// DestroyWindow purges queued messages before Rust releases Surface memory.
void queue_layout(Surface &surface) {
    if (!surface.queued)
        surface.queued = PostMessageW(surface.clip, layout_message, 0, 0) != 0;
}
}

extern "C" void *vsf_surface_create(void *parent_pointer) try {
    HWND parent = static_cast<HWND>(parent_pointer);
    if (!IsWindow(parent) || GetWindowThreadProcessId(parent, nullptr) != GetCurrentThreadId())
        return nullptr;
    WNDCLASSW wc{};
    wc.lpfnWndProc = surface_proc;
    wc.hInstance = GetModuleHandleW(nullptr);
    wc.lpszClassName = surface_class;
    if (!RegisterClassW(&wc) && GetLastError() != ERROR_CLASS_ALREADY_EXISTS) return nullptr;
    auto surface = std::make_unique<Surface>();
    surface->parent = parent;
    surface->ui_thread = GetCurrentThreadId();
    surface->clip = CreateWindowExW(WS_EX_NOACTIVATE, surface_class, L"",
        WS_CHILD | WS_CLIPCHILDREN | WS_CLIPSIBLINGS, 0, 0, 1, 1,
        parent, nullptr, wc.hInstance, surface.get());
    if (!surface->clip) return nullptr;
    surface->video = CreateWindowExW(WS_EX_NOACTIVATE, L"STATIC", L"",
        WS_CHILD | WS_VISIBLE | WS_CLIPCHILDREN | SS_BLACKRECT, 0, 0, 1, 1,
        surface->clip, nullptr, wc.hInstance, nullptr);
    if (!surface->video) { DestroyWindow(surface->clip); return nullptr; }
    return surface.release();
} catch (...) { return nullptr; }

extern "C" void vsf_surface_layout(void *raw, double x, double y, double width,
                                    double height, int visible) {
    auto &surface = *static_cast<Surface *>(raw);
    std::lock_guard lock(surface.mutex);
    const bool presenting = surface.desired.presenting;
    surface.desired = {x, y, width, height, visible != 0, presenting};
    queue_layout(surface);
}
extern "C" void vsf_surface_release(void *raw) {
    std::unique_ptr<Surface> surface(static_cast<Surface *>(raw));
    // Core destruction has already joined mpv's internal VO threads.
    if (GetCurrentThreadId() == surface->ui_thread) DestroyWindow(surface->clip);
    else SendMessageW(surface->clip, release_message, 0, 0);
    std::fprintf(stderr, "VSF Windows surface removed: child_alive=%d\n", IsWindow(surface->clip));
}
extern "C" int vsf_surface_occlude(void *raw, int value) {
    auto &surface = *static_cast<Surface *>(raw);
    {
        std::lock_guard lock(surface.mutex);
        surface.occluded = value != 0;
    }
    // No lock or mpv call while waiting on main. The controller serializes
    // occlusion changes; already-posted layout messages also respect this flag.
    if (SendMessageW(surface.clip, occlusion_message, 0, 0) != 1) return 0;
    const bool visible = IsWindowVisible(surface.clip) != 0;
    if (std::getenv("VSF_MEDIA_SMOKE"))
        std::fprintf(stderr, "VSF Windows occlusion: requested=%d visible=%d\n", value != 0, visible);
    return value == 0 || !visible;
}
extern "C" void *vsf_player_create(void *raw, int *error) try {
    auto player = std::make_unique<Playback>();
    player->surface = static_cast<Surface *>(raw);
    std::unique_ptr<mpv_handle, decltype(&mpv_terminate_destroy)> core(mpv_create(), mpv_terminate_destroy);
    if (!core) { *error = MPV_ERROR_NOMEM; return nullptr; }
    const char *options[][2] = {{"config", "no"}, {"terminal", "no"},
        {"vo", "gpu"}, {"gpu-api", "d3d11"}, {"gpu-context", "d3d11"},
        {"clipboard-backends", ""}, {"hwdec", "auto-copy"}, {"vd-lavc-threads", "4"},
        {"hr-seek-demuxer-offset", "1"}, {"pause", "yes"}, {"keep-open", "yes"},
        {"idle", "yes"}, {"osc", "no"}, {"input-default-bindings", "no"},
        {"input-vo-keyboard", "no"}};
    for (const auto &option : options) {
        *error = mpv_set_option_string(core.get(), option[0], option[1]);
        if (*error < 0) return nullptr;
    }
    int64_t wid = reinterpret_cast<intptr_t>(player->surface->video);
    *error = mpv_set_option(core.get(), "wid", MPV_FORMAT_INT64, &wid);
    if (*error < 0) return nullptr;
    if (std::getenv("VSF_MEDIA_SMOKE")) mpv_request_log_messages(core.get(), "v");
    *error = mpv_initialize(core.get());
    if (*error < 0) return nullptr;
    player->core = core.release();
    return player.release();
} catch (...) { *error = MPV_ERROR_NOMEM; return nullptr; }
extern "C" void vsf_player_destroy(void *raw) {
    std::unique_ptr<Playback> player(static_cast<Playback *>(raw));
    mpv_terminate_destroy(player->core);
    std::fprintf(stderr, "VSF Windows core destroyed: child_alive=%d\n", IsWindow(player->surface->video));
}
extern "C" int vsf_command(void *raw, const char **args) {
    return mpv_command(static_cast<Playback *>(raw)->core, args);
}
extern "C" void vsf_presenting(void *raw, int value) {
    auto &surface = *static_cast<Playback *>(raw)->surface;
    std::lock_guard lock(surface.mutex);
    surface.desired.presenting = value != 0;
    queue_layout(surface);
}
extern "C" int vsf_discard_events(void *raw) {
    return vsf_mpv::discard_events(static_cast<Playback *>(raw)->core);
}
extern "C" double vsf_number(void *raw, const char *name) {
    return vsf_mpv::number(static_cast<Playback *>(raw)->core, name);
}
extern "C" int vsf_flag(void *raw, const char *name) {
    return vsf_mpv::flag(static_cast<Playback *>(raw)->core, name);
}
// Internal D3D11 swaps are not counted by this adapter. The typed status marks
// this counter unavailable; vo-configured is separate and is not pixel evidence.
extern "C" uint64_t vsf_frames(void *) { return 0; }
extern "C" int vsf_event_error(void *raw, int *file_transition) {
    return vsf_mpv::poll_events(static_cast<Playback *>(raw)->core, 0,
                                true, file_transition);
}
extern "C" char *vsf_text(void *raw, const char *name) {
    return mpv_get_property_string(static_cast<Playback *>(raw)->core, name);
}
extern "C" void vsf_text_free(char *text) { mpv_free(text); }
extern "C" const char *vsf_error(int code) { return mpv_error_string(code); }
