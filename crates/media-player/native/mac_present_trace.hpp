// Opt-in Metal presentation feedback. ANGLE still owns and presents drawables.
// Acquisition/dump run only on the render worker. Each asynchronous callback
// writes one distinct slot, then publishes it with release/acquire ordering.
// Shared state outlives the layer and late callbacks; it owns no native object.
#pragma once
#import <QuartzCore/CAMetalLayer.h>
#import <QuartzCore/CABase.h>
#include <atomic>
#include <chrono>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <memory>
#include <new>

class MacPresentTrace {
public:
    struct Frame {
        double acquired = 0, presented = 0, callback = 0;
        unsigned long drawable = 0;
        std::atomic<bool> completed{false};
    };
    static constexpr size_t capacity = 16384;
    std::unique_ptr<Frame[]> frames{new (std::nothrow) Frame[capacity]};
    size_t count = 0, overflow = 0; // Render worker only.
    const double hostBefore = CACurrentMediaTime();
    const double unixMs = std::chrono::duration<double, std::milli>(
        std::chrono::system_clock::now().time_since_epoch()).count();
    const double hostAfter = CACurrentMediaTime();

    void dump() const {
        const double endBefore = CACurrentMediaTime();
        const double endUnixMs = std::chrono::duration<double, std::milli>(
            std::chrono::system_clock::now().time_since_epoch()).count();
        const double endAfter = CACurrentMediaTime();
        std::fprintf(stderr, "VSF_METAL_CLOCK unixMs=%.3f hostBefore=%.9f hostAfter=%.9f\n",
            unixMs, hostBefore, hostAfter);
        std::fprintf(stderr, "VSF_METAL_CLOCK_END unixMs=%.3f hostBefore=%.9f hostAfter=%.9f\n",
            endUnixMs, endBefore, endAfter);
        size_t pending = 0;
        for (size_t i = 0; i < count; ++i) {
            const auto &frame = frames[i];
            bool complete = frame.completed.load(std::memory_order_acquire);
            if (!complete) ++pending;
            // A pending callback may still write its two doubles. Never read
            // them until the release store, and never wait for presentation.
            std::fprintf(stderr,
                "VSF_METAL_PRESENT index=%zu drawable=%lu acquired=%.9f "
                "completed=%d presented=%.9f callback=%.9f\n",
                i, frame.drawable, frame.acquired, complete,
                complete ? frame.presented : 0, complete ? frame.callback : 0);
        }
        std::fprintf(stderr, "VSF_METAL_TRACE count=%zu pending=%zu overflow=%zu\n",
            count, pending, overflow);
    }
};

@interface VSFTracedMetalLayer : CAMetalLayer {
@public
    std::shared_ptr<MacPresentTrace> presentationTrace;
}
@end

@implementation VSFTracedMetalLayer
- (id<CAMetalDrawable>)nextDrawable {
    id<CAMetalDrawable> drawable = [super nextDrawable];
    auto state = presentationTrace;
    if (!drawable || !state) return drawable;
    if (state->count == MacPresentTrace::capacity) {
        ++state->overflow;
        return drawable;
    }
    size_t index = state->count++;
    auto &frame = state->frames[index];
    frame.acquired = CACurrentMediaTime();
    frame.drawable = drawable.drawableID;
    // Capture only C++ storage; no cycle through the drawable, layer or player.
    [drawable addPresentedHandler:^(id<MTLDrawable> completed) {
        auto &slot = state->frames[index];
        slot.presented = completed.presentedTime;
        slot.callback = CACurrentMediaTime();
        slot.completed.store(true, std::memory_order_release);
    }];
    return drawable;
}
@end

static CAMetalLayer *makeVideoMetalLayer() {
    const char *option = std::getenv("VSF_PRESENT_TRACE");
    if (option && std::strcmp(option, "1") == 0) {
        try {
            auto state = std::make_shared<MacPresentTrace>();
            if (state->frames) {
                VSFTracedMetalLayer *layer = [VSFTracedMetalLayer layer];
                layer->presentationTrace = std::move(state);
                return layer;
            }
        } catch (const std::bad_alloc &) {
            // Diagnostic allocation failure must not prevent ordinary playback.
        }
    }
    return [CAMetalLayer layer];
}

// Declare before the worker's autorelease/graphics/drawable-lock scope. Dumping
// then occurs off the UI thread, after native graphics and lock teardown. Late
// callbacks remain safe but explicitly pending in this snapshot, never passes.
struct MacPresentTraceDump {
    std::shared_ptr<MacPresentTrace> state;
    explicit MacPresentTraceDump(CAMetalLayer *layer) {
        if ([layer isKindOfClass:VSFTracedMetalLayer.class])
            state = ((VSFTracedMetalLayer *)layer)->presentationTrace;
    }
    ~MacPresentTraceDump() { if (state) state->dump(); }
};
