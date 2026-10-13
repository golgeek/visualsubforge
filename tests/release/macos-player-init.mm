// Run against the packaged libmpv under the same hardened signing policy as VSF.
// No media, renderer or audio device is opened. This is not playback acceptance.
#import <AppKit/AppKit.h>
#include "../../crates/media-player/native/mpv_client.hpp"
#include <atomic>
#include <chrono>
#include <cstring>
#include <memory>
#include <thread>

static int initialize(bool original_policy) {
    std::unique_ptr<mpv_handle, decltype(&mpv_terminate_destroy)>
        core(mpv_create(), mpv_terminate_destroy);
    if (!core) return 1;
    const char *options[][2] = {{"config", "no"}, {"terminal", "no"},
        {"vo", "null"}, {"ao", "null"}, {"idle", "yes"},
        {"clipboard-backends", ""}, {"osc", "no"}};
    for (const auto &option : options) {
        const int result = mpv_set_option_string(core.get(), option[0], option[1]);
        if (result < 0) {
            std::fprintf(stderr, "Option %s: %s\n", option[0], mpv_error_string(result));
            return 1;
        }
    }
    if (!original_policy) {
        const int result = vsf_mpv::disable_scripts(core.get());
        if (result < 0) {
            std::fprintf(stderr, "Script policy: %s\n", mpv_error_string(result));
            return 1;
        }
    }
    if (mpv_initialize(core.get()) < 0) return 1;
    // The original defect runs asynchronously in a built-in script thread.
    // Keep the initialized core alive briefly before exercising its teardown.
    const auto until = std::chrono::steady_clock::now() + std::chrono::seconds(1);
    while (std::chrono::steady_clock::now() < until) mpv_wait_event(core.get(), 0.05);
    core.reset();
    std::puts("PASS: hardened packaged mpv initialization and teardown (no GPU/audio)");
    return 0;
}

int main(int argc, char **argv) {
    const bool original_policy = argc == 2 && std::strcmp(argv[1], "--original-policy") == 0;
    if (argc != 1 && !original_policy) return 2;
    @autoreleasepool {
        [NSApplication sharedApplication];
        std::atomic<bool> done{false};
        int result = 1;
        // Like VSF, initialize on a control worker while the main thread serves
        // mpv's synchronous Cocoa callbacks.
        std::thread control([&] {
            @autoreleasepool { result = initialize(original_policy); }
            done.store(true);
        });
        while (!done.load()) CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.05, true);
        control.join();
        return result;
    }
}
