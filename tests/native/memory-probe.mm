// Standalone task-003b diagnostic using the production native adapter, without
// Rust/Tauri/WebKit. Main owns AppKit; one worker owns all player operations.
#include "../../crates/media-player/native/player.mm"
#include <libproc.h>
#include <malloc/malloc.h>
#include <functional>
#include <vector>
#include <stdexcept>
#include <string>
#include <cmath>

static void sample(const char *phase, int cycle) {
    rusage_info_v4 usage{};
    if (proc_pid_rusage(getpid(), RUSAGE_INFO_V4, (rusage_info_t *)&usage) != 0)
        throw std::runtime_error("proc_pid_rusage failed");
    malloc_statistics_t heap{};
    malloc_zone_statistics(nullptr, &heap);
    printf("phase=%s cycle=%d footprint=%llu resident=%llu heap_live=%zu heap_reserved=%zu blocks=%u\n",
        phase, cycle, usage.ri_phys_footprint, usage.ri_resident_size,
        heap.size_in_use, heap.size_allocated, heap.blocks_in_use);
    fflush(stdout);
}
static void command(void *player, std::initializer_list<const char *> args) {
    std::vector<const char *> list(args);
    list.push_back(nullptr);
    int result = vsf_command(player, list.data());
    if (result < 0) throw std::runtime_error(vsf_error(result));
}
static void until(void *player, const std::function<bool()> &done) {
    auto deadline = std::chrono::steady_clock::now() + std::chrono::seconds(15);
    for (;;) {
        int error = vsf_event_error(player);
        if (error < 0) throw std::runtime_error(vsf_error(error));
        if (done()) return;
        if (std::chrono::steady_clock::now() > deadline) throw std::runtime_error("state wait timed out");
        std::this_thread::sleep_for(std::chrono::milliseconds(20));
    }
}
int main(int argc, char **argv) {
    if (argc != 5 || (std::string(argv[2]) != "recreate" && std::string(argv[2]) != "reuse") ||
        (std::string(argv[3]) != "no" && std::string(argv[3]) != "auto-copy")) {
        fprintf(stderr, "usage: memory-probe ABSOLUTE_FIXTURE recreate|reuse no|auto-copy CYCLES\n");
        return 2;
    }
    int cycles;
    try { cycles = std::stoi(argv[4]); } catch (...) { return 2; }
    if (cycles < 1 || cycles > 500) return 2;
    bool reuse = std::string(argv[2]) == "reuse";
    bool recycleSurface = std::getenv("VSF_PROBE_RECYCLE_SURFACE") != nullptr;
    bool cyclePool = std::getenv("VSF_PROBE_CYCLE_POOL") != nullptr;
    const char *alternate = std::getenv("VSF_PROBE_ALTERNATE");
    if (reuse && recycleSurface) {
        fprintf(stderr, "surface recycling requires recreate mode\n");
        return 2;
    }
    int exitCode = 0;
    @autoreleasepool {
        [NSApplication sharedApplication];
        [NSApp setActivationPolicy:NSApplicationActivationPolicyRegular];
        NSWindow *window = [[NSWindow alloc] initWithContentRect:NSMakeRect(0, 0, 960, 540)
            styleMask:NSWindowStyleMaskTitled backing:NSBackingStoreBuffered defer:NO];
        window.title = @"VSF native memory diagnostic";
        [window center];
        [window makeKeyAndOrderFront:nil];
        void *surface = vsf_surface_create((__bridge void *)window.contentView);
        if (!surface) return 1;
        vsf_surface_layout(surface, 0, 0, 960, 540, 1);
        std::thread worker([&] {
            void *player = nullptr;
            try {
                printf("pid=%d mode=%s decoder=%s cycles=%d bytes; native-only\n", getpid(), argv[2], argv[3], cycles);
                printf("recycle_surface=%d cycle_autorelease_pool=%d\n", recycleSurface, cyclePool);
                if (alternate) printf("alternate_fixture=%s (even cycles)\n", alternate);
                sample("initial", 0);
                for (int i = 1; i <= cycles; ++i) {
                    auto runCycle = [&] {
                        if (!player) {
                            int error = 0;
                            player = vsf_player_create(surface, &error);
                            if (!player) throw std::runtime_error(vsf_error(error));
                        }
                        if (i == 1) {
                            if (std::isfinite(vsf_number(player, "vsf-nonexistent-property")))
                                throw std::runtime_error("missing property returned a numeric value");
                            command(player, {"set", "audio-delay", "-1"});
                            if (vsf_number(player, "audio-delay") != -1)
                                throw std::runtime_error("signed property was not preserved");
                            command(player, {"set", "audio-delay", "0"});
                        }
                        if (const char *threads = std::getenv("VSF_PROBE_THREADS"))
                            command(player, {"set", "vd-lavc-threads", threads});
                        uint64_t previous = vsf_frames(player);
                        command(player, {"set", "pause", "yes"});
                        command(player, {"set", "hwdec", argv[3]});
                        command(player, {"loadfile", alternate && i % 2 == 0 ? alternate : argv[1], "replace"});
                        until(player, [&] { return vsf_number(player, "duration") > 0 && vsf_frames(player) > previous; });
                        command(player, {"seek", "5", "absolute+exact"});
                        until(player, [&] { return !vsf_flag(player, "seeking") && std::abs(vsf_number(player, "time-pos") - 5) < .02; });
                        char *decoder = vsf_text(player, "hwdec-current");
                        bool correct = decoder && (std::string(argv[3]) == "no" ? std::string(decoder) == "no" : std::string(decoder) != "no" && *decoder);
                        if (i == 1 && decoder) printf("actual_decoder=%s\n", decoder);
                        vsf_text_free(decoder);
                        if (!correct) throw std::runtime_error("decoder mismatch");
                        if (std::getenv("VSF_PROBE_PLAY")) {
                            command(player, {"seek", "0", "absolute+exact"});
                            until(player, [&] { return !vsf_flag(player, "seeking") && vsf_number(player, "time-pos") < .02; });
                            double decoderDrops = vsf_number(player, "decoder-frame-drop-count");
                            double outputDrops = vsf_number(player, "frame-drop-count");
                            uint64_t swaps = vsf_frames(player);
                            command(player, {"set", "pause", "no"});
                            std::this_thread::sleep_for(std::chrono::seconds(10));
                            command(player, {"set", "pause", "yes"});
                            double position = vsf_number(player, "time-pos");
                            printf("playback_10s position=%.3f swaps=%llu decoder_drops=%.0f output_drops=%.0f\n",
                                position, (unsigned long long)(vsf_frames(player) - swaps),
                                vsf_number(player, "decoder-frame-drop-count") - decoderDrops,
                                vsf_number(player, "frame-drop-count") - outputDrops);
                            if (!std::isfinite(position) || position < 9 || vsf_frames(player) <= swaps)
                                throw std::runtime_error("playback did not progress");
                        }
                        if (reuse) {
                            command(player, {"stop"});
                            until(player, [&] { return vsf_flag(player, "idle-active") != 0; });
                        } else {
                            vsf_player_destroy(player);
                            player = nullptr;
                        }
                        if (recycleSurface) {
                            // Diagnostic only: compare a new GL context per open
                            // with the production surface's window-long lifetime.
                            vsf_surface_release(surface);
                            surface = nullptr;
                            __block void *nextSurface = nullptr;
                            dispatch_sync(dispatch_get_main_queue(), ^{
                                nextSurface = vsf_surface_create((__bridge void *)window.contentView);
                            });
                            surface = nextSurface;
                            if (!surface) throw std::runtime_error("surface recreation failed");
                            vsf_surface_layout(surface, 0, 0, 960, 540, 1);
                        }
                    };
                    if (cyclePool) {
                        @autoreleasepool { runCycle(); }
                    } else {
                        runCycle();
                    }
                    // Sample after the optional pool has drained.
                    if (i == 2 || i % 10 == 0 || i == cycles) {
                        std::this_thread::sleep_for(std::chrono::seconds(2));
                        sample("closed", i);
                    }
                }
                if (player) { vsf_player_destroy(player); player = nullptr; }
                std::this_thread::sleep_for(std::chrono::seconds(2));
                sample("destroyed", cycles);
                if (std::getenv("VSF_PROBE_RELIEVE")) {
                    // Diagnostic intervention only: asks malloc to release free
                    // pages. This is not an application memory-management policy.
                    size_t relieved = malloc_zone_pressure_relief(nullptr, 0);
                    printf("allocator_relief_requested returned_bytes=%zu\n", relieved);
                    std::this_thread::sleep_for(std::chrono::seconds(2));
                    sample("relieved", cycles);
                }

            } catch (const std::exception &error) {
                fprintf(stderr, "FAIL: %s\n", error.what());
                if (player) vsf_player_destroy(player);
                exitCode = 1;
            }
            if (surface) vsf_surface_release(surface); // acknowledged main-queue cleanup
            if (exitCode == 0) {
                try {
                    std::this_thread::sleep_for(std::chrono::seconds(2));
                    sample("surface_released", cycles);
                    puts("PASS");
                } catch (const std::exception &error) {
                    fprintf(stderr, "FAIL: %s\n", error.what());
                    exitCode = 1;
                }
            }
            dispatch_async(dispatch_get_main_queue(), ^{
                [NSApp stop:nil];
                [NSApp postEvent:[NSEvent otherEventWithType:NSEventTypeApplicationDefined
                    location:NSZeroPoint modifierFlags:0 timestamp:0 windowNumber:0 context:nil
                    subtype:0 data1:0 data2:0] atStart:NO];
            });
        });
        [NSApp run];
        worker.join(); // only after main acknowledged teardown and worker stopped the app
    }
    return exitCode;
}
