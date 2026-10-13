#pragma once

#include <mpv/client.h>
#include <cstdio>
#include <limits>

// Shared client policy, called only by the player's control owner. These
// functions borrow the live core; they neither retain it nor touch a surface.
namespace vsf_mpv {

inline int disable_scripts(mpv_handle *core) {
    // VSF owns the UI and opens local files. mpv's built-in UI/network scripts
    // are independent of config=no and load-scripts=no. Disable them before
    // initialize: otherwise LuaJIT can execute unsigned code inside the signed
    // macOS app. Keep this list aligned with our pinned mpv scripting.c.
    const char *options[] = {"load-scripts", "osc", "ytdl", "load-stats-overlay",
        "load-console", "load-auto-profiles", "load-select", "load-positioning",
        "load-commands", "load-context-menu"};
    for (const char *option : options) {
        const int result = mpv_set_option_string(core, option, "no");
        if (result < 0) return result;
    }
    return 0;
}

inline int discard_events(mpv_handle *core) {
    // Only after stop reaches idle, before loading the next logical session.
    for (int i = 0; i < 1024; ++i) {
        if (mpv_wait_event(core, 0)->event_id == MPV_EVENT_NONE) return 0;
    }
    return MPV_ERROR_EVENT_QUEUE_FULL;
}

inline double number(mpv_handle *core, const char *name) {
    double value = 0;
    // Signed properties may legitimately equal -1. Keep errors unavailable.
    if (mpv_get_property(core, name, MPV_FORMAT_DOUBLE, &value) < 0)
        return std::numeric_limits<double>::quiet_NaN();
    return value;
}

inline int flag(mpv_handle *core, const char *name) {
    int value = 0;
    mpv_get_property(core, name, MPV_FORMAT_FLAG, &value);
    return value;
}

inline int poll_events(mpv_handle *core, int render_error, bool log_messages,
                       int *file_transition) {
    *file_transition = 0;
    int result = render_error;
    // Bound control work. Unconsumed events remain queued for the next poll.
    for (int i = 0; i < 64; ++i) {
        const mpv_event *event = mpv_wait_event(core, 0);
        if (event->event_id == MPV_EVENT_NONE) break;
        if (log_messages && event->event_id == MPV_EVENT_LOG_MESSAGE) {
            const auto *log = static_cast<mpv_event_log_message *>(event->data);
            std::fprintf(stderr, "VSF mpv [%s] %s", log->prefix, log->text);
        }
        if (event->event_id == MPV_EVENT_FILE_LOADED) *file_transition = 1;
        if (event->event_id == MPV_EVENT_END_FILE) {
            *file_transition = -1;
            const auto *end = static_cast<mpv_event_end_file *>(event->data);
            if (end->reason == MPV_END_FILE_REASON_ERROR) result = end->error;
        }
    }
    return result;
}

} // namespace vsf_mpv
