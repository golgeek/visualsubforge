// Opt-in CPU call timing for the Mac adapter, owned only by its render worker.
// No per-frame allocation/IO. Destruction dumps after graphics and drawable-lock
// teardown. These records cannot identify subtitle pixels or display scanout.
#pragma once
#include <chrono>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <memory>
#include <new>

class MacRenderTrace {
    using Clock = std::chrono::steady_clock;
public:
    struct Frame {
        Clock::time_point waiting, began, updated, rendered, cleared, presented, reported;
        int width = 0, height = 0, skipped = 0, result = 0;
        bool swapped = false;
    };
    MacRenderTrace() {
        const char *option = std::getenv("VSF_RENDER_TRACE");
        if (!option || std::strcmp(option, "1") != 0) return;
        frames.reset(new (std::nothrow) Frame[capacity]);
        if (!frames) {
            std::fprintf(stderr, "VSF_RENDER_TRACE unavailable: allocation failed\n");
            return;
        }
        origin = Clock::now();
        unixOriginMs = std::chrono::duration<double, std::milli>(
            std::chrono::system_clock::now().time_since_epoch()).count();
    }
    Clock::time_point now() const {
        return frames ? Clock::now() : Clock::time_point{};
    }
    void append(const Frame &frame) {
        if (!frames) return;
        if (count < capacity) frames[count++] = frame;
        else ++dropped;
    }
    ~MacRenderTrace() {
        if (!frames) return;
        std::fprintf(stderr, "VSF_RENDER_TRACE count=%zu dropped=%zu\n", count, dropped);
        for (size_t i = 0; i < count; ++i) {
            const auto &f = frames[i];
            std::fprintf(stderr,
                "VSF_RENDER_FRAME index=%zu beginUnixMs=%.3f endUnixMs=%.3f "
                "lockMs=%.3f updateMs=%.3f renderMs=%.3f clearMs=%.3f presentMs=%.3f reportMs=%.3f "
                "width=%d height=%d skip=%d result=%d swapped=%d\n",
                i, unixOriginMs + ms(origin, f.began), unixOriginMs + ms(origin, f.reported),
                ms(f.waiting, f.began), ms(f.began, f.updated), ms(f.updated, f.rendered),
                ms(f.rendered, f.cleared), ms(f.cleared, f.presented),
                ms(f.presented, f.reported), f.width, f.height, f.skipped, f.result, f.swapped);
        }
    }
private:
    static double ms(Clock::time_point from, Clock::time_point to) {
        return std::chrono::duration<double, std::milli>(to - from).count();
    }
    static constexpr size_t capacity = 16384;
    std::unique_ptr<Frame[]> frames;
    size_t count = 0, dropped = 0;
    Clock::time_point origin{};
    double unixOriginMs = 0;
};
