// macOS experiment: legacy OpenGL or opt-in ANGLE/Metal. Main owns AppKit;
// the render worker owns current-context operations and joins before core
// destruction. No view draw/reshape callback makes either context current.
#import <AppKit/AppKit.h>
#import <QuartzCore/QuartzCore.h>
#include "mpv_client.hpp"
#include "mac_font_fallback.hpp"
#include "mac_render_trace.hpp"
#include <mpv/render.h>
#include <mpv/render_gl.h>
#ifdef VSF_USE_ANGLE
#include "mac_angle.hpp"
#include "mac_present_trace.hpp"
#else
#include <OpenGL/gl3.h>
#include <dlfcn.h>
#endif
#include <atomic>
#include <chrono>
#include <future>
#include <memory>
#include <thread>
#include <cstdlib>
#include <cstdio>
#include <mutex>

#ifdef VSF_USE_ANGLE
// The above-webview video container must not intercept waveform/editor input
// in the rest of the window. AppKit continues hit-testing the webview below it.
@interface VSFVideoContainer : NSView
@end
@implementation VSFVideoContainer
- (NSView *)hitTest:(NSPoint)point { (void)point; return nil; }
@end
#endif

struct Surface {
    NSView *view;
    NSView *clip;
#ifdef VSF_USE_ANGLE
    CAMetalLayer *metal;
    bool occluded = false; // Main-thread only, independent of drawable contention.
#else
    NSOpenGLContext *context;
#endif
    int width, height;
    std::mutex drawable;
    std::atomic<bool> resized{false};
    NSRect desired;
    bool visible = false, retryQueued = false, retired = false;
    id moveObserver, scaleObserver, resizeObserver, endResizeObserver;
};
struct Playback {
    MacFontFallback fontFallback;
    bool fontFallbackReady = false;
    vsf_mpv::FontDiagnostics fontDiagnostics;
    mpv_handle *core = nullptr; // Only control worker calls ordinary client APIs.
    std::shared_ptr<Surface> surface;
    std::atomic<bool> stopping{false}, dirty{true}, presenting{true};
    std::atomic<uint64_t> frames{0};
    std::atomic<int> renderError{0};
    std::thread renderer;
};
using SurfaceRef = std::shared_ptr<Surface>;
using PlayerRef = std::shared_ptr<Playback>;

// Main never waits for rendering. Coalesce layout changes and retry between
// frames. The same mutex guards all context accesses, including init/cleanup.
static void applyLayout(const SurfaceRef &surface) {
    if (surface->retired) return;
    std::unique_lock lock(surface->drawable, std::try_to_lock);
    if (!lock.owns_lock()) {
        if (!surface->retryQueued) {
            surface->retryQueued = true;
            std::weak_ptr<Surface> weak = surface;
            dispatch_after(dispatch_time(DISPATCH_TIME_NOW, 8 * NSEC_PER_MSEC), dispatch_get_main_queue(), ^{
                if (auto next = weak.lock()) {
                    next->retryQueued = false;
                    applyLayout(next);
                }
            });
        }
        return;
    }
    NSView *parent = surface->clip.superview;
    NSRect content = [parent convertRect:parent.window.contentLayoutRect fromView:nil];
    NSRect rect = surface->desired;
    surface->clip.frame = content;
    // Clip has an unflipped local coordinate system, independent of its parent.
    rect.origin.y = surface->clip.bounds.size.height - rect.origin.y - rect.size.height;
    surface->view.frame = rect;
    surface->view.hidden = !surface->visible;
#ifdef VSF_USE_ANGLE
    surface->view.hidden = !surface->visible || surface->occluded;
    surface->metal.frame = surface->view.bounds;
    surface->metal.contentsScale = surface->view.window.backingScaleFactor;
#else
    [surface->context update];
#endif
    NSRect pixels = [surface->view convertRectToBacking:surface->view.bounds];
    surface->width = (int)pixels.size.width;
    surface->height = (int)pixels.size.height;
#ifdef VSF_USE_ANGLE
    surface->metal.drawableSize = CGSizeMake(surface->width, surface->height);
#endif
    surface->resized.store(true);
}

extern "C" void vsf_surface_layout(void *raw, double x, double y, double width, double height, int visible) {
    auto surface = *static_cast<SurfaceRef *>(raw);
    dispatch_async(dispatch_get_main_queue(), ^{
        surface->desired = NSMakeRect(x, y, width, height);
        surface->visible = visible != 0;
        applyLayout(surface);
    });
}

#ifdef VSF_USE_ANGLE
extern "C" int vsf_surface_occlude(void *raw, int value) {
    auto surface = *static_cast<SurfaceRef *>(raw);
    __block int acknowledged = 0;
    // Do not acquire drawable while synchronously awaiting main. Hiding is a
    // view operation; it need not wait for a render or a resize retry.
    dispatch_block_t change = ^{
        if (surface->retired) return;
        surface->occluded = value != 0;
        surface->view.hidden = surface->occluded || !surface->visible;
        acknowledged = !surface->occluded || surface->view.hidden;
        fprintf(stderr, "VSF Metal occlusion: requested=%d visible=%d\n", value, !surface->view.hidden);
    };
    if ([NSThread isMainThread]) change();
    else dispatch_sync(dispatch_get_main_queue(), change);
    return acknowledged;
}
#endif

static void updated(void *context) {
    // mpv callback performs no rendering, dispatch, or blocking operation.
    static_cast<Playback *>(context)->dirty.store(true);
}

extern "C" void *vsf_surface_create(void *parentPointer) try {
    if (![NSThread isMainThread] || !parentPointer) return nullptr;
    NSView *parent = (__bridge NSView *)parentPointer;
    auto surface = std::make_shared<Surface>();
    auto owner = std::make_unique<SurfaceRef>(surface);
#ifndef VSF_USE_ANGLE
    NSOpenGLPixelFormatAttribute attributes[] = {NSOpenGLPFAAccelerated, NSOpenGLPFADoubleBuffer,
        NSOpenGLPFAOpenGLProfile, NSOpenGLProfileVersion3_2Core, NSOpenGLPFAColorSize, 24, 0};
    NSOpenGLPixelFormat *format = [[NSOpenGLPixelFormat alloc] initWithAttributes:attributes];
    if (!format) return nullptr;
#endif
    NSView *view = [[NSView alloc] initWithFrame:NSMakeRect(24, 0, 640, 360)];
#ifdef VSF_USE_ANGLE
    surface->metal = makeVideoMetalLayer();
    view.wantsLayer = YES;
    view.layer.masksToBounds = YES;
    // AppKit owns its backing layer's position. Our Metal sublayer uses local
    // video bounds; replacing the backing layer would move video to (0, 0).
    [view.layer addSublayer:surface->metal];
#else
    NSOpenGLContext *context = [[NSOpenGLContext alloc] initWithFormat:format shareContext:nil];
    if (!context) return nullptr;
    view.wantsBestResolutionOpenGLSurface = YES;
#endif
    // Match the web content's origin, excluding the title bar. The content
    // view may extend underneath it; parent.bounds alone puts video too high.
    // Start hidden at a fallback size until the frontend supplies its bounds.
    NSRect bounds = [parent convertRect:parent.window.contentLayoutRect fromView:nil];
    view.frame = NSMakeRect(bounds.origin.x + 24,
        parent.isFlipped ? NSMinY(bounds) + 76 : NSMaxY(bounds) - 436, 640, 360);
    view.autoresizingMask = NSViewNotSizable;
    view.hidden = YES;
    surface->desired = NSMakeRect(24, 76, 640, 360);
#ifdef VSF_USE_ANGLE
    NSView *clip = [[VSFVideoContainer alloc] initWithFrame:bounds];
#else
    NSView *clip = [[NSView alloc] initWithFrame:bounds];
#endif
    clip.wantsLayer = YES;
    clip.layer.masksToBounds = YES;
#ifdef VSF_USE_ANGLE
    [parent addSubview:clip positioned:NSWindowAbove relativeTo:nil];
#else
    [parent addSubview:clip positioned:NSWindowBelow relativeTo:nil];
#endif
    [clip addSubview:view];
    surface->clip = clip;
    surface->view = view;
#ifndef VSF_USE_ANGLE
    [context setView:view];
    surface->context = context;
#endif
    NSRect pixels = [view convertRectToBacking:view.bounds];
    surface->width = (int)pixels.size.width;
    surface->height = (int)pixels.size.height;
#ifdef VSF_USE_ANGLE
    surface->metal.frame = view.bounds;
    surface->metal.contentsScale = parent.window.backingScaleFactor;
    surface->metal.drawableSize = CGSizeMake(surface->width, surface->height);
#endif
    std::weak_ptr<Surface> weak = surface;
    auto changed = ^(NSNotification *) { if (auto next = weak.lock()) applyLayout(next); };
    surface->moveObserver = [NSNotificationCenter.defaultCenter addObserverForName:NSWindowDidMoveNotification
        object:parent.window queue:NSOperationQueue.mainQueue usingBlock:changed];
    surface->scaleObserver = [NSNotificationCenter.defaultCenter addObserverForName:NSWindowDidChangeBackingPropertiesNotification
        object:parent.window queue:NSOperationQueue.mainQueue usingBlock:changed];
    // CSS bounds can remain unchanged while the native bottom-left origin
    // moves with window height. Update the drawable even without a new IPC
    // rectangle, and redraw once more after live-resize finishes.
    surface->resizeObserver = [NSNotificationCenter.defaultCenter addObserverForName:NSWindowDidResizeNotification
        object:parent.window queue:NSOperationQueue.mainQueue usingBlock:changed];
    surface->endResizeObserver = [NSNotificationCenter.defaultCenter addObserverForName:NSWindowDidEndLiveResizeNotification
        object:parent.window queue:NSOperationQueue.mainQueue usingBlock:changed];
    return owner.release();
} catch (...) { return nullptr; }
extern "C" void vsf_surface_release(void *raw) {
    auto surface = *static_cast<SurfaceRef *>(raw);
    delete static_cast<SurfaceRef *>(raw);
    // Renderer has joined. Acknowledge main-thread cleanup before the control
    // worker reports shutdown complete. Main never joins that worker.
    dispatch_block_t cleanup = ^{
        surface->retired = true;
        [NSNotificationCenter.defaultCenter removeObserver:surface->moveObserver];
        [NSNotificationCenter.defaultCenter removeObserver:surface->scaleObserver];
        [NSNotificationCenter.defaultCenter removeObserver:surface->resizeObserver];
        [NSNotificationCenter.defaultCenter removeObserver:surface->endResizeObserver];
#ifndef VSF_USE_ANGLE
        [surface->context clearDrawable];
#endif
        [surface->view removeFromSuperview];
        [surface->clip removeFromSuperview];
        // Release AppKit/GL objects here even if the final shared_ptr is dropped
        // later on the control thread or by an already-queued retired layout.
        surface->view = nil;
        surface->clip = nil;
#ifdef VSF_USE_ANGLE
        surface->metal = nil;
#else
        surface->context = nil;
#endif
        surface->moveObserver = nil;
        surface->scaleObserver = nil;
        surface->resizeObserver = nil;
        surface->endResizeObserver = nil;
    };
    if ([NSThread isMainThread]) cleanup();
    else dispatch_sync(dispatch_get_main_queue(), cleanup);
}

extern "C" void *vsf_player_create(void *rawSurface, int *error) try {
    auto player = std::make_shared<Playback>();
    player->surface = *static_cast<SurfaceRef *>(rawSurface);
    player->core = mpv_create();
    if (!player->core) { *error = MPV_ERROR_NOMEM; return nullptr; }
    std::unique_ptr<mpv_handle, decltype(&mpv_terminate_destroy)> core(player->core, mpv_terminate_destroy);
    auto owner = std::make_unique<PlayerRef>(player);
    // HEVC/MP4 can land on a later display-time keyframe when seeking by DTS.
    // Decode from earlier in the stream so exact seeks can reach pre-cut frames.
    // This conservative experiment setting trades seek speed for correctness.
    // VSF owns clipboard interaction in the webview. Disable mpv's unused
    // native backend (0.41.0's mac backend leaks a Clipboard per core teardown).
    const char *options[][2] = {{"config", "no"}, {"terminal", "no"}, {"vo", "libmpv"},
        {"clipboard-backends", ""},
        {"hwdec", "auto-copy"}, {"vd-lavc-threads", "4"}, {"hr-seek-demuxer-offset", "1"}, {"pause", "yes"}, {"keep-open", "yes"}, {"idle", "yes"},
        {"input-default-bindings", "no"}, {"input-vo-keyboard", "no"}};
    for (auto &option : options) {
        *error = mpv_set_option_string(player->core, option[0], option[1]);
        if (*error < 0) { return nullptr; }
    }
    *error = vsf_mpv::disable_scripts(player->core);
    if (*error < 0) return nullptr;
    player->fontFallbackReady = player->fontFallback.prepare();
    if (player->fontFallbackReady) {
        *error = player->fontFallback.configure(player->core);
        if (*error < 0) return nullptr;
    } else {
        player->fontDiagnostics.add("Mac CJK fallback font unavailable; install a suitable font or use an available document font.");
    }
    *error = vsf_mpv::configure_font_logging(player->core, std::getenv("VSF_MEDIA_SMOKE") != nullptr);
    if (*error < 0) return nullptr;
    *error = mpv_initialize(player->core);
    if (*error < 0) { return nullptr; }
    std::promise<int> ready;
    auto initialized = ready.get_future();
    player->renderer = std::thread([player, ready = std::move(ready)]() mutable {
        MacRenderTrace trace;
#ifdef VSF_USE_ANGLE
        MacPresentTraceDump presentation(player->surface->metal);
#endif
        @autoreleasepool {
            std::unique_lock lock(player->surface->drawable);
#ifdef VSF_USE_ANGLE
            AngleContext graphics;
            if (!graphics.attach(player->surface->metal)) {
                fprintf(stderr, "VSF ANGLE initialization failed: EGL 0x%x\n", eglGetError());
                ready.set_value(MPV_ERROR_UNSUPPORTED);
                return; // EGL RAII cleanup completes before the worker is joined.
            }
            fprintf(stderr, "VSF ANGLE renderer: %s; version: %s\n", glGetString(GL_RENDERER), glGetString(GL_VERSION));
#else
            [player->surface->context makeCurrentContext];
            fprintf(stderr, "VSF OpenGL renderer: %s; version: %s\n", glGetString(GL_RENDERER), glGetString(GL_VERSION));
            GLint interval = 1;
            [player->surface->context setValues:&interval forParameter:NSOpenGLContextParameterSwapInterval];
#endif
            mpv_render_context *render = nullptr;
#ifdef VSF_USE_ANGLE
            mpv_opengl_init_params gl = { AngleContext::resolve, nullptr };
#else
            mpv_opengl_init_params gl = { [](void *, const char *name) -> void * { return dlsym(RTLD_DEFAULT, name); }, nullptr };
#endif
            mpv_render_param init[] = {{MPV_RENDER_PARAM_API_TYPE, (void *)MPV_RENDER_API_TYPE_OPENGL},
                {MPV_RENDER_PARAM_OPENGL_INIT_PARAMS, &gl}, {MPV_RENDER_PARAM_INVALID, nullptr}};
            int result = mpv_render_context_create(&render, player->core, init);
            if (result < 0) {
#ifndef VSF_USE_ANGLE
                [NSOpenGLContext clearCurrentContext];
#endif
                ready.set_value(result);
                return;
            }
            mpv_render_context_set_update_callback(render, updated, player.get());
            ready.set_value(0);
            lock.unlock();
            while (!player->stopping.load()) {
                @autoreleasepool {
                auto waiting = trace.now();
                lock.lock();
                bool resized = player->surface->resized.exchange(false);
                if (player->dirty.exchange(false) || resized) {
                    MacRenderTrace::Frame frame{};
                    frame.waiting = waiting;
                    frame.began = trace.now();
                    mpv_render_context_update(render);
                    mpv_opengl_fbo fbo = {0, player->surface->width, player->surface->height, 0};
                    int flip = 1;
                    int skip = player->presenting.load() ? 0 : 1;
                    mpv_render_param params[] = {{MPV_RENDER_PARAM_OPENGL_FBO, &fbo},
                        {MPV_RENDER_PARAM_SKIP_RENDERING, &skip},
                        {MPV_RENDER_PARAM_FLIP_Y, &flip}, {MPV_RENDER_PARAM_INVALID, nullptr}};
                    // Service pending frames even when the session is closed.
                    frame.updated = trace.now();
                    result = mpv_render_context_render(render, params);
                    frame.rendered = trace.now();
                    if (skip && result >= 0) {
                        // stop leaves mpv's last image cached. A closed session
                        // must also stay black after drawable resize/redraw.
                        glBindFramebuffer(GL_FRAMEBUFFER, 0);
                        glDisable(GL_SCISSOR_TEST);
                        glColorMask(GL_TRUE, GL_TRUE, GL_TRUE, GL_TRUE);
                        glClearColor(0, 0, 0, 1);
                        glClear(GL_COLOR_BUFFER_BIT);
                    }
                    frame.cleared = trace.now();
                    frame.width = fbo.w;
                    frame.height = fbo.h;
                    frame.skipped = skip;
                    frame.result = result;
                    if (result >= 0 && !player->stopping.load()) {
#ifdef VSF_USE_ANGLE
                        if (!graphics.present()) {
                            frame.presented = frame.reported = trace.now();
                            frame.result = MPV_ERROR_GENERIC;
                            trace.append(frame);
                            player->renderError.store(MPV_ERROR_GENERIC);
                            break;
                        }
#else
                        [player->surface->context flushBuffer];
#endif
                        frame.presented = trace.now();
                        frame.swapped = true;
                        mpv_render_context_report_swap(render);
                        player->frames.fetch_add(1);
                    } else {
                        frame.presented = trace.now();
                    }
                    frame.reported = trace.now();
                    trace.append(frame);
                    if (result < 0) {
                        player->renderError.store(result);
                        break;
                    }
                }
                lock.unlock();
                std::this_thread::sleep_for(std::chrono::milliseconds(4));
                }
            }
            if (!lock.owns_lock()) lock.lock();
            mpv_render_context_set_update_callback(render, nullptr, nullptr);
            mpv_render_context_free(render);
            glClearColor(0, 0, 0, 1); glClear(GL_COLOR_BUFFER_BIT);
#ifdef VSF_USE_ANGLE
            graphics.present();
#else
            [player->surface->context flushBuffer];
            [NSOpenGLContext clearCurrentContext];
#endif
        }
    });
    *error = initialized.get();
    if (*error < 0) { player->renderer.join(); return nullptr; }
    core.release(); // returned owner now controls ordered render/core cleanup
    return owner.release();
} catch (...) { *error = MPV_ERROR_NOMEM; return nullptr; }
extern "C" void vsf_player_destroy(void *raw) {
    if (!raw) return;
    auto player = *static_cast<PlayerRef *>(raw);
    delete static_cast<PlayerRef *>(raw);
    player->stopping.store(true);
    player->renderer.join(); // Called on control worker, never on main queue.
    mpv_terminate_destroy(player->core);

}
extern "C" int vsf_command(void *raw, const char **args) {
    return mpv_command((*static_cast<PlayerRef *>(raw))->core, args);
}
extern "C" void vsf_presenting(void *raw, int value) {
    auto player = *static_cast<PlayerRef *>(raw);
    // No GL on the control thread: it requests a draw on the renderer.
    player->presenting.store(value != 0);
    player->dirty.store(true);
}
extern "C" int vsf_discard_events(void *raw) {
    auto player = *static_cast<PlayerRef *>(raw);
    const int result = vsf_mpv::discard_events(player->core);
    if (result >= 0) {
        player->fontDiagnostics.clear();
        if (!player->fontFallbackReady)
            player->fontDiagnostics.add("Mac CJK fallback font unavailable; install a suitable font or use an available document font.");
    }
    return result;
}
extern "C" double vsf_number(void *raw, const char *name) {
    return vsf_mpv::number((*static_cast<PlayerRef *>(raw))->core, name);
}
extern "C" int vsf_flag(void *raw, const char *name) {
    return vsf_mpv::flag((*static_cast<PlayerRef *>(raw))->core, name);
}
extern "C" uint64_t vsf_frames(void *raw) { return (*static_cast<PlayerRef *>(raw))->frames.load(); }
extern "C" int vsf_event_error(void *raw, int *file_transition) {
    auto player = *static_cast<PlayerRef *>(raw);
    return vsf_mpv::poll_events((*static_cast<PlayerRef *>(raw))->core, player->renderError.exchange(0),
                                std::getenv("VSF_MEDIA_SMOKE") != nullptr, file_transition, &player->fontDiagnostics);
}
extern "C" const char *vsf_font_diagnostics(void *raw) {
    return (*static_cast<PlayerRef *>(raw))->fontDiagnostics.text();
}
extern "C" const char *vsf_error(int code) { return mpv_error_string(code); }

extern "C" char *vsf_text(void *raw, const char *name) {
    return mpv_get_property_string((*static_cast<PlayerRef *>(raw))->core, name);
}
extern "C" void vsf_text_free(char *text) { mpv_free(text); }
