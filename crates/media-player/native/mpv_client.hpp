#pragma once

#include <mpv/client.h>
#include <cstdio>
#include <cstring>
#include <limits>

// Shared client policy, called only by the player's control owner. These
// functions borrow the live core; they neither retain it nor touch a surface.
namespace vsf_mpv {

// Session notices, not a fatal playback error or a claim about the current cue.
// Only the control owner observes/reads them. No log pointer or heap allocation
// survives a poll; repeated broken-font messages cannot grow memory unboundedly.
class FontDiagnostics {
    char messages[8][512]{};
    char joined[8 * 512 + 64]{};
    unsigned count = 0;
    size_t used = 0;
    bool omitted = false;
public:
    void clear() { count = 0; used = 0; omitted = false; joined[0] = '\0'; }
    const char *text() const { return joined; }
    void add(const char *text) {
        if (!text) return;
        char bounded[512]{};
        size_t length = 0;
        while (text[length] && length < sizeof(bounded) - 1) {
            const unsigned char c = text[length];
            bounded[length++] = c < 32 || c == 127 ? ' ' : static_cast<char>(c);
        }
        while (length && bounded[length - 1] == ' ') bounded[--length] = '\0';
        if (!length) return;
        for (unsigned i = 0; i < count; ++i)
            if (std::strcmp(messages[i], bounded) == 0) return;
        if (count == 8) {
            if (!omitted) {
                std::strcpy(joined + used, "Further font notices omitted.\n");
                omitted = true;
            }
            return;
        }
        std::memcpy(messages[count++], bounded, length + 1);
        std::memcpy(joined + used, bounded, length);
        used += length;
        joined[used++] = '\n';
        joined[used] = '\0';
    }
    void observe(const mpv_event_log_message *log) {
        if (!log || !log->prefix || !log->text)
            return;
        if (std::strcmp(log->prefix, "sub/ass") != 0) return;
        // libass/mpv emits some real failures/substitutions at verbose level.
        // Ordinary "selecting one more font" is normal fallback, not a failure.
        const bool fallback = std::strstr(log->text, "Error opening font") ||
            std::strstr(log->text, "fontselect: failed") ||
            std::strstr(log->text, "fontselect: Using default font");
        const bool warning = log->log_level <= MPV_LOG_LEVEL_WARN &&
            (std::strstr(log->text, "font") || std::strstr(log->text, "Font") ||
             std::strstr(log->text, "glyph") || std::strstr(log->text, "Glyph"));
        if (fallback || warning)
            add(log->text);
    }
};

inline int configure_font_logging(mpv_handle *core, bool verbose) {
    // Follow the per-module level, even with terminal=no. Avoid verbose logs
    // from decoding/rendering while retaining libass's missing-family notices.
    const int result = mpv_set_option_string(core, "msg-level",
        verbose ? "all=v" : "all=warn,sub/ass=v");
    if (result < 0) return result;
    return mpv_request_log_messages(core, "terminal-default");
}

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
                       int *file_transition, FontDiagnostics *fonts = nullptr) {
    *file_transition = 0;
    int result = render_error;
    // Bound control work. Unconsumed events remain queued for the next poll.
    for (int i = 0; i < 64; ++i) {
        const mpv_event *event = mpv_wait_event(core, 0);
        if (event->event_id == MPV_EVENT_NONE) break;
        if (event->event_id == MPV_EVENT_LOG_MESSAGE) {
            const auto *log = static_cast<mpv_event_log_message *>(event->data);
            if (fonts) fonts->observe(log);
            if (log_messages)
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
