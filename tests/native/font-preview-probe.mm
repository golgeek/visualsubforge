// Software composition through real libmpv and the production fallback helper.
// Input is a production serializer URL. No UI/surface/device acceptance implied.
#include "../../crates/media-player/native/mac_font_fallback.hpp"
#include "../../crates/media-player/native/mpv_client.hpp"
#include <chrono>
#include <fstream>
#include <iostream>
#include <memory>
#include <stdexcept>

static void checked(int value) {
    if (value < 0) throw std::runtime_error(mpv_error_string(value));
}
int main(int argc, char **argv) try {
    if (argc != 4) throw std::runtime_error("Usage: probe baseline|fallback|embedded input.url output-directory");
    const std::string mode = argv[1];
    std::ifstream input(argv[2]);
    const std::string url((std::istreambuf_iterator<char>(input)), {});
    if (url.rfind("hex://", 0) != 0) throw std::runtime_error("Expected production preview URL");
    MacFontFallback fallback;
    std::unique_ptr<mpv_handle, decltype(&mpv_terminate_destroy)> core(mpv_create(), mpv_terminate_destroy);
    if (!core) throw std::runtime_error("mpv_create failed");
    const char *options[][2] = {{"config", "no"}, {"terminal", "no"}, {"vo", "image"},
        {"vo-image-format", "png"}, {"vo-image-outdir", argv[3]}, {"ao", "null"},
        {"sub-ass-override", "no"}, {"sid", "auto"}, {"start", "1.5"},
        {"frames", "1"}, {"clipboard-backends", ""}};
    for (const auto &option : options) {
        const int result = mpv_set_option_string(core.get(), option[0], option[1]);
        if (result < 0) throw std::runtime_error(std::string(option[0]) + ": " + mpv_error_string(result));
    }
    // A path-list string would split the hex:// URL on ':' on POSIX.
    mpv_node entry{};
    entry.format = MPV_FORMAT_STRING;
    entry.u.string = const_cast<char *>(url.c_str());
    mpv_node_list list{1, &entry, nullptr};
    mpv_node files{};
    files.format = MPV_FORMAT_NODE_ARRAY;
    files.u.list = &list;
    checked(mpv_set_option(core.get(), "sub-files", MPV_FORMAT_NODE, &files));
    checked(vsf_mpv::disable_scripts(core.get()));
    if (mode == "fallback") {
        if (!fallback.prepare()) throw std::runtime_error("Installed fallback unavailable");
        checked(fallback.configure(core.get()));
        std::cout << "private-resource=" << fallback.path() << '\n';
    } else if (mode == "embedded") {
        checked(mpv_set_option_string(core.get(), "sub-font-provider", "none"));
    } else if (mode != "baseline") {
        throw std::runtime_error("Unknown mode");
    }
    checked(vsf_mpv::configure_font_logging(core.get(), false));
    checked(mpv_initialize(core.get()));
    const char *load[] = {"loadfile", "av://lavfi:color=c=black:s=768x576:d=6", nullptr};
    checked(mpv_command(core.get(), load));
    vsf_mpv::FontDiagnostics notices;
    const auto deadline = std::chrono::steady_clock::now() + std::chrono::seconds(30);
    bool finished = false;
    while (!finished && std::chrono::steady_clock::now() < deadline) {
        const auto *event = mpv_wait_event(core.get(), 0.05);
        if (event->event_id == MPV_EVENT_LOG_MESSAGE) {
            const auto *log = static_cast<mpv_event_log_message *>(event->data);
            notices.observe(log);
            if (std::strncmp(log->prefix, "sub", 3) == 0)
                std::cout << log->text;
        }
        if (event->event_id == MPV_EVENT_END_FILE) {
            const auto *end = static_cast<mpv_event_end_file *>(event->data);
            if (end->reason == MPV_END_FILE_REASON_ERROR) checked(end->error);
            finished = true;
        }
    }
    if (!finished) throw std::runtime_error("Render timeout");
    std::cout << "font-notices:\n" << notices.text();
    return 0;
} catch (const std::exception &error) {
    std::cerr << error.what() << '\n';
    return 1;
}
