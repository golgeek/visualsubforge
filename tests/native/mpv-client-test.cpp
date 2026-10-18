// Deterministic client-policy tests. Fake mpv calls isolate queue ordering,
// bounds and error semantics; this does not validate a native surface or device.
#include "../../crates/media-player/native/mpv_client.hpp"
#include <cassert>
#include <cmath>
#include <cstring>
#include <string>
#include <utility>
#include <vector>

#ifdef NDEBUG
#error This diagnostic requires assertions enabled.
#endif

namespace {
std::vector<mpv_event> events;
size_t next_event = 0;
int wait_calls = 0;
mpv_event none{};
std::vector<std::string> disabled_scripts;
const char *rejected_option = nullptr;

mpv_event event(mpv_event_id id, void *data = nullptr) {
    mpv_event value{};
    value.event_id = id;
    value.data = data;
    return value;
}

void queue(std::vector<mpv_event> values) {
    events = std::move(values);
    next_event = 0;
    wait_calls = 0;
}
} // namespace

extern "C" int mpv_set_option_string(mpv_handle *, const char *name, const char *value) {
    assert(std::strcmp(value, "no") == 0);
    disabled_scripts.emplace_back(name);
    return rejected_option && std::strcmp(name, rejected_option) == 0
        ? MPV_ERROR_OPTION_NOT_FOUND : 0;
}

extern "C" mpv_event *mpv_wait_event(mpv_handle *, double timeout) {
    assert(timeout == 0); // The common policy must never block its owner.
    ++wait_calls;
    return next_event < events.size() ? &events[next_event++] : &none;
}

extern "C" int mpv_get_property(mpv_handle *, const char *name,
                                mpv_format format, void *data) {
    if (std::strcmp(name, "signed") == 0) {
        assert(format == MPV_FORMAT_DOUBLE);
        *static_cast<double *>(data) = -1;
        return 0;
    }
    if (std::strcmp(name, "enabled") == 0) {
        assert(format == MPV_FORMAT_FLAG);
        *static_cast<int *>(data) = 1;
        return 0;
    }
    return MPV_ERROR_PROPERTY_UNAVAILABLE;
}

int main() {
    assert(vsf_mpv::disable_scripts(nullptr) == 0);
    assert((disabled_scripts == std::vector<std::string>{"load-scripts", "osc", "ytdl",
        "load-stats-overlay", "load-console", "load-auto-profiles", "load-select",
        "load-positioning", "load-commands", "load-context-menu"}));
    disabled_scripts.clear();
    rejected_option = "load-stats-overlay";
    assert(vsf_mpv::disable_scripts(nullptr) == MPV_ERROR_OPTION_NOT_FOUND);
    assert(disabled_scripts.back() == rejected_option && disabled_scripts.size() == 4);

    assert(vsf_mpv::number(nullptr, "signed") == -1);
    assert(std::isnan(vsf_mpv::number(nullptr, "missing")));
    assert(vsf_mpv::flag(nullptr, "enabled") == 1);
    assert(vsf_mpv::flag(nullptr, "missing") == 0);

    int transition = 99;
    queue({});
    assert(vsf_mpv::poll_events(nullptr, MPV_ERROR_UNSUPPORTED, false,
                                &transition) == MPV_ERROR_UNSUPPORTED);
    assert(transition == 0 && wait_calls == 1);

    mpv_event_end_file ended{};
    ended.reason = MPV_END_FILE_REASON_EOF;
    queue({event(MPV_EVENT_END_FILE, &ended), event(MPV_EVENT_FILE_LOADED)});
    assert(vsf_mpv::poll_events(nullptr, 0, false, &transition) == 0);
    assert(transition == 1); // Last observed transition wins, in queue order.
    queue({event(MPV_EVENT_FILE_LOADED), event(MPV_EVENT_END_FILE, &ended)});
    assert(vsf_mpv::poll_events(nullptr, MPV_ERROR_UNSUPPORTED, false,
                                &transition) == MPV_ERROR_UNSUPPORTED);
    assert(transition == -1); // Ordinary EOF must not clear a render error.

    ended.reason = MPV_END_FILE_REASON_ERROR;
    ended.error = MPV_ERROR_LOADING_FAILED;
    queue({event(MPV_EVENT_END_FILE, &ended)});
    assert(vsf_mpv::poll_events(nullptr, MPV_ERROR_UNSUPPORTED, false,
                                &transition) == MPV_ERROR_LOADING_FAILED);

    std::vector<mpv_event> pending(64, event(MPV_EVENT_TICK));
    pending.push_back(event(MPV_EVENT_FILE_LOADED));
    queue(pending);
    assert(vsf_mpv::poll_events(nullptr, 0, false, &transition) == 0);
    assert(wait_calls == 64 && next_event == 64 && transition == 0);
    assert(vsf_mpv::poll_events(nullptr, 0, false, &transition) == 0);
    assert(next_event == 65 && transition == 1);

    queue(std::vector<mpv_event>(1023, event(MPV_EVENT_TICK)));
    assert(vsf_mpv::discard_events(nullptr) == 0 && wait_calls == 1024);
    queue(std::vector<mpv_event>(1025, event(MPV_EVENT_TICK)));
    assert(vsf_mpv::discard_events(nullptr) == MPV_ERROR_EVENT_QUEUE_FULL);
    assert(wait_calls == 1024 && next_event == 1024);
    assert(vsf_mpv::discard_events(nullptr) == 0 && next_event == 1025);

    mpv_event_log_message log{"test", "info", "diagnostic\n", MPV_LOG_LEVEL_INFO};
    queue({event(MPV_EVENT_LOG_MESSAGE, &log)});
    assert(vsf_mpv::poll_events(nullptr, 0, false, &transition) == 0);
    queue({event(MPV_EVENT_LOG_MESSAGE, &log)});
    assert(vsf_mpv::poll_events(nullptr, 0, true, &transition) == 0);
    vsf_mpv::FontDiagnostics fonts;
    mpv_event_log_message missing{"sub/ass", "warn",
        "fontselect: failed to find any fallback with glyph 0x4E16\n", MPV_LOG_LEVEL_WARN};
    queue({event(MPV_EVENT_LOG_MESSAGE, &missing), event(MPV_EVENT_LOG_MESSAGE, &missing)});
    assert(vsf_mpv::poll_events(nullptr, 0, false, &transition, &fonts) == 0);
    assert(std::strcmp(fonts.text(), "fontselect: failed to find any fallback with glyph 0x4E16\n") == 0);
    missing.log_level = MPV_LOG_LEVEL_V;
    missing.text = "Glyph not found, selecting one more font";
    fonts.observe(&missing); // A successful fallback search isn't a warning.
    assert(!std::strstr(fonts.text(), "selecting"));
    missing.log_level = MPV_LOG_LEVEL_WARN;
    missing.prefix = "osd/ass";
    fonts.observe(&missing); // Notices must belong to subtitles.
    assert(!std::strstr(fonts.text(), "selecting"));
    for (int i = 0; i < 20; ++i) fonts.add((std::to_string(i) + std::string(1000, 'x')).c_str());
    assert(std::strlen(fonts.text()) < 8 * 512 + 64);
    assert(std::strstr(fonts.text(), "Further font notices omitted.") != nullptr);
    fonts.clear();
    assert(std::strlen(fonts.text()) == 0);
    fonts.add("new\nsession\rnotice");
    assert(std::strcmp(fonts.text(), "new session notice\n") == 0);
    std::puts("PASS: common mpv property/event policy");
}
