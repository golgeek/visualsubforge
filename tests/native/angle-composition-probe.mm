// Isolated feasibility probe, not the application adapter. Public AppKit,
// WKWebView and ANGLE/Metal only. No decoded frames cross the WebKit bridge.
#import <AppKit/AppKit.h>
#import <QuartzCore/CAMetalLayer.h>
#import <WebKit/WebKit.h>
#include <EGL/egl.h>
#include <EGL/eglext.h>
#include <GLES3/gl3.h>
#include <mpv/client.h>
#include <mpv/render_gl.h>
#include <atomic>
#include <chrono>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <deque>
#include <future>
#include <mutex>
#include <stdexcept>
#include <string>
#include <thread>
#include <vector>

struct Playback {
    // Control owns ordinary mpv calls; rendering owns EGL and mpv_render_* calls.
    // Main owns views and tries the drawable lock; control never takes it.
    std::mutex drawable, commands;
    std::deque<std::vector<std::string>> pending;
    std::atomic<bool> stop{false}, dirty{true}, pageReady{false}, loaded{false};
    void enqueue(std::vector<std::string> command) {
        std::lock_guard lock(commands);
        if (pending.size() < 64) pending.push_back(std::move(command));
    }
};
static Playback playback;
static std::atomic<int> exitStatus{0};
static void check(bool success, const char *operation) {
    if (!success) throw std::runtime_error(operation);
}
static void mpvCheck(int result, const char *operation) {
    if (result < 0) throw std::runtime_error(std::string(operation) + ": " + mpv_error_string(result));
}
static void send(mpv_handle *core, const std::vector<std::string> &command) {
    std::vector<const char *> argv;
    for (const auto &part : command) argv.push_back(part.c_str());
    argv.push_back(nullptr);
    mpvCheck(mpv_command(core, argv.data()), command[0].c_str());
}

static void render(CAMetalLayer *layer, mpv_handle *core, std::promise<bool> ready) {
    @autoreleasepool {
        struct FrameTiming {
            double lockMs, updateMs, renderMs, clearMs, swapMs, reportMs;
            int64_t targetRaw, clockNs;
            uint64_t flags;
            int width, height, skipped;
        };
        // Opt-in diagnostic: bounded storage, no per-frame allocation or output.
        // Dump only after rendering stops; these are CPU call durations, not GPU timestamps.
        const bool timingEnabled = std::getenv("VSF_ANGLE_TIMING") != nullptr;
        constexpr size_t timingCapacity = 16384;
        std::vector<FrameTiming> timings;
        if (timingEnabled) timings.reserve(timingCapacity);
        size_t droppedTimings = 0;
        auto milliseconds = [](auto from, auto to) {
            return std::chrono::duration<double, std::milli>(to - from).count();
        };
        EGLDisplay display = EGL_NO_DISPLAY;
        EGLContext context = EGL_NO_CONTEXT;
        EGLSurface surface = EGL_NO_SURFACE;
        bool signalled = false;
        mpv_render_context *renderer = nullptr;
        try {
            auto platform = reinterpret_cast<PFNEGLGETPLATFORMDISPLAYEXTPROC>(eglGetProcAddress("eglGetPlatformDisplayEXT"));
            check(platform != nullptr, "ANGLE platform display entry point");
            const EGLint attributes[] = {EGL_PLATFORM_ANGLE_TYPE_ANGLE, EGL_PLATFORM_ANGLE_TYPE_METAL_ANGLE, EGL_NONE};
            display = platform(EGL_PLATFORM_ANGLE_ANGLE, nullptr, attributes);
            check(display != EGL_NO_DISPLAY && eglInitialize(display, nullptr, nullptr), "initialize ANGLE Metal display");
            check(eglBindAPI(EGL_OPENGL_ES_API), "bind OpenGL ES");
            const EGLint configAttributes[] = {EGL_SURFACE_TYPE, EGL_WINDOW_BIT, EGL_RENDERABLE_TYPE,
                EGL_OPENGL_ES3_BIT, EGL_RED_SIZE, 8, EGL_GREEN_SIZE, 8, EGL_BLUE_SIZE, 8, EGL_ALPHA_SIZE, 8, EGL_NONE};
            EGLConfig config;
            EGLint count = 0;
            check(eglChooseConfig(display, configAttributes, &config, 1, &count) && count == 1, "choose EGL config");
            const EGLint contextAttributes[] = {EGL_CONTEXT_CLIENT_VERSION, 3, EGL_NONE};
            context = eglCreateContext(display, config, EGL_NO_CONTEXT, contextAttributes);
            check(context != EGL_NO_CONTEXT, "create EGL context");
            {
                std::lock_guard lock(playback.drawable);
                surface = eglCreateWindowSurface(display, config, (__bridge EGLNativeWindowType)layer, nullptr);
                check(surface != EGL_NO_SURFACE && eglMakeCurrent(display, surface, surface, context), "attach EGL surface");
            }
            std::fprintf(stderr, "ANGLE renderer: %s; version: %s\n", glGetString(GL_RENDERER), glGetString(GL_VERSION));
            mpv_opengl_init_params gl = {[](void *, const char *name) -> void * {
                return reinterpret_cast<void *>(eglGetProcAddress(name));
            }, nullptr};
            mpv_render_param init[] = {{MPV_RENDER_PARAM_API_TYPE, (void *)MPV_RENDER_API_TYPE_OPENGL},
                {MPV_RENDER_PARAM_OPENGL_INIT_PARAMS, &gl}, {MPV_RENDER_PARAM_INVALID, nullptr}};
            mpvCheck(mpv_render_context_create(&renderer, core, init), "create mpv render context");
            mpv_render_context_set_update_callback(renderer, [](void *) { playback.dirty.store(true); }, nullptr);
            ready.set_value(true);
            signalled = true;
            uint64_t frames = 0;
            while (!playback.stop) {
                if (playback.dirty.exchange(false)) {
                    auto waiting = std::chrono::steady_clock::now();
                    std::lock_guard lock(playback.drawable);
                    auto began = std::chrono::steady_clock::now();
                    mpv_render_context_update(renderer);
                    auto updated = std::chrono::steady_clock::now();
                    // Querying EGL before its next drawable is acquired can
                    // return the previous size. Main publishes the intended
                    // pixel size under this lock; ANGLE acquires it on draw.
                    mpv_opengl_fbo fbo = {0, static_cast<int>(layer.drawableSize.width),
                        static_cast<int>(layer.drawableSize.height), 0};
                    int flip = 1;
                    int skip = playback.loaded ? 0 : 1;
                    mpv_render_param params[] = {{MPV_RENDER_PARAM_OPENGL_FBO, &fbo},
                        {MPV_RENDER_PARAM_FLIP_Y, &flip}, {MPV_RENDER_PARAM_SKIP_RENDERING, &skip},
                        {MPV_RENDER_PARAM_INVALID, nullptr}};
                    // Even while blanked, consume pending mpv frames so its VO
                    // does not wait for a presentation we have silently skipped.
                    mpv_render_frame_info frameInfo{};
                    int64_t clockNs = 0;
                    if (timingEnabled) {
                        mpvCheck(mpv_render_context_get_info(renderer,
                            {MPV_RENDER_PARAM_NEXT_FRAME_INFO, &frameInfo}), "read next frame timing");
                        // Retain raw target values: mpv 0.41.0 returns nanoseconds here,
                        // although its installed header documents microseconds. Do not
                        // make a scheduling decision from this diagnostic observation.
                        // The client API permits this clock query on render threads.
                        clockNs = mpv_get_time_ns(core);
                    }
                    auto rendering = std::chrono::steady_clock::now();
                    mpvCheck(mpv_render_context_render(renderer, params), "render video");
                    auto rendered = std::chrono::steady_clock::now();
                    if (skip) {
                        glBindFramebuffer(GL_FRAMEBUFFER, 0);
                        glDisable(GL_SCISSOR_TEST);
                        glColorMask(GL_TRUE, GL_TRUE, GL_TRUE, GL_TRUE);
                        glClearColor(0, 0, 0, 1); glClear(GL_COLOR_BUFFER_BIT);
                    }
                    auto drawn = std::chrono::steady_clock::now();
                    check(eglSwapBuffers(display, surface), "present Metal surface");
                    auto presented = std::chrono::steady_clock::now();
                    double drawMs = std::chrono::duration<double, std::milli>(drawn - began).count();
                    double swapMs = std::chrono::duration<double, std::milli>(presented - drawn).count();
                    if (drawMs + swapMs >= 100)
                        std::fprintf(stderr, "SLOW FRAME draw=%.1fms swap=%.1fms skip=%d\n", drawMs, swapMs, skip);
                    auto reporting = std::chrono::steady_clock::now();
                    mpv_render_context_report_swap(renderer);
                    auto reported = std::chrono::steady_clock::now();
                    if (timingEnabled) {
                        if (timings.size() < timingCapacity)
                            timings.push_back({milliseconds(waiting, began), milliseconds(began, updated),
                                milliseconds(rendering, rendered), milliseconds(rendered, drawn),
                                milliseconds(drawn, presented), milliseconds(reporting, reported),
                                frameInfo.target_time, clockNs, frameInfo.flags,
                                fbo.w, fbo.h, skip});
                        else
                            ++droppedTimings;
                    }
                    ++frames;
                }
                std::this_thread::sleep_for(std::chrono::milliseconds(4));
            }
            std::fprintf(stderr, "RENDERED %llu frames\n", static_cast<unsigned long long>(frames));
        } catch (const std::exception &error) {
            exitStatus = 1;
            playback.stop = true;
            if (!signalled) ready.set_value(false);
            std::fprintf(stderr, "RENDER ERROR: %s (EGL 0x%x)\n", error.what(), eglGetError());
        }
        // Teardown stays on this worker with its EGL context current. Only after
        // render/EGL teardown finishes may control destroy the core and stop UI.
        {
            std::lock_guard lock(playback.drawable);
            if (renderer) {
                mpv_render_context_set_update_callback(renderer, nullptr, nullptr);
                mpv_render_context_free(renderer);
            }
            if (display != EGL_NO_DISPLAY) {
                eglMakeCurrent(display, EGL_NO_SURFACE, EGL_NO_SURFACE, EGL_NO_CONTEXT);
                if (surface != EGL_NO_SURFACE) eglDestroySurface(display, surface);
                if (context != EGL_NO_CONTEXT) eglDestroyContext(display, context);
                eglTerminate(display);
            }
        }
        if (timingEnabled) {
            std::fprintf(stderr, "FRAME_TIMINGS count=%zu dropped=%zu\n", timings.size(), droppedTimings);
            for (size_t i = 0; i < timings.size(); ++i) {
                const auto &t = timings[i];
                std::fprintf(stderr, "FRAME_TIMING index=%zu lock=%.3f update=%.3f render=%.3f "
                    "clear=%.3f swap=%.3f report=%.3f target_raw=%lld clock_ns=%lld flags=%llu "
                    "width=%d height=%d skip=%d\n",
                    i, t.lockMs, t.updateMs, t.renderMs, t.clearMs, t.swapMs, t.reportMs,
                    static_cast<long long>(t.targetRaw), static_cast<long long>(t.clockNs),
                    static_cast<unsigned long long>(t.flags),
                    t.width, t.height, t.skipped);
            }
        }
        std::fprintf(stderr, "RENDER WORKER STOPPED\n");
    }
}

static void control(CAMetalLayer *layer, WKWebView *web, std::string media, std::string subtitles) {
    @autoreleasepool {
        mpv_handle *core = nullptr;
        std::thread renderer;
        try {
            core = mpv_create();
            check(core != nullptr, "create mpv");
            for (auto option : std::vector<std::pair<const char *, const char *>>{
                {"config", "no"}, {"vo", "libmpv"}, {"hwdec", "auto-copy"}, {"vd-lavc-threads", "4"},
                {"clipboard-backends", ""}, {"pause", "yes"}, {"idle", "yes"}, {"keep-open", "yes"},
                {"loop-file", "inf"}, {"mute", "yes"}, {"osc", "no"}, {"input-default-bindings", "no"}})
                mpvCheck(mpv_set_option_string(core, option.first, option.second), option.first);
            if (const char *provider = std::getenv("VSF_ANGLE_FONT_PROVIDER"))
                mpvCheck(mpv_set_option_string(core, "sub-font-provider", provider), "subtitle font provider");
            if (const char *font = std::getenv("VSF_ANGLE_FONT"))
                mpvCheck(mpv_set_option_string(core, "sub-font", font), "subtitle font");
            if (const char *ipc = std::getenv("VSF_ANGLE_IPC"))
                mpvCheck(mpv_set_option_string(core, "input-ipc-server", ipc), "diagnostic IPC");
            mpvCheck(mpv_initialize(core), "initialize mpv");
            mpv_request_log_messages(core, std::getenv("VSF_ANGLE_VERBOSE") ? "v" : "warn");
            std::promise<bool> ready;
            auto initialized = ready.get_future();
            renderer = std::thread(render, layer, core, std::move(ready));
            check(initialized.get(), "initialize render worker");
            send(core, {"loadfile", media});
            auto nextStatus = std::chrono::steady_clock::now();
            auto deadline = std::chrono::steady_clock::now() + std::chrono::minutes(5);
            while (!playback.stop && std::chrono::steady_clock::now() < deadline) {
                @autoreleasepool {
                    std::deque<std::vector<std::string>> commands;
                    { std::lock_guard lock(playback.commands); commands.swap(playback.pending); }
                    for (const auto &command : commands) {
                        try {
                            if (command[0] == "preview-edit") {
                                int64_t previous = 0;
                                mpv_get_property(core, "sid", MPV_FORMAT_INT64, &previous);
                                // Diagnostic literal only. A production implementation
                                // must serialize Rust's accepted document revision.
                                send(core, {"sub-add", "memory://1\n00:00:00,000 --> 00:00:04,000\n"
                                    "Edited while paused: café — 日本語\nSecond line\n", "select", "VSF edited preview"});
                                if (previous > 0) send(core, {"sub-remove", std::to_string(previous)});
                            } else if (command[0] == "reload-media") {
                                send(core, {"loadfile", media, "replace"});
                            } else {
                                send(core, command);
                                if (command[0] == "stop") { playback.loaded = false; playback.dirty = true; }
                            }
                            std::fprintf(stderr, "COMMAND %s %s OK\n", command[0].c_str(),
                                         command.size() > 1 ? command[1].c_str() : "");
                        }
                        catch (const std::exception &error) { std::fprintf(stderr, "COMMAND ERROR %s\n", error.what()); }
                    }
                    while (auto event = mpv_wait_event(core, 0)) {
                        if (event->event_id == MPV_EVENT_NONE) break;
                        if (event->event_id == MPV_EVENT_FILE_LOADED) {
                            playback.loaded = true;
                            send(core, {"sub-add", subtitles, "select"});
                            std::fprintf(stderr, "FILE_LOADED\n");
                        }
                        if (event->event_id == MPV_EVENT_LOG_MESSAGE) {
                            auto message = static_cast<mpv_event_log_message *>(event->data);
                            std::fprintf(stderr, "mpv/%s: %s", message->prefix, message->text);
                        }
                        if (event->event_id == MPV_EVENT_END_FILE) {
                            auto end = static_cast<mpv_event_end_file *>(event->data);
                            if (end->error < 0) mpvCheck(end->error, "playback");
                        }
                    }
                    auto now = std::chrono::steady_clock::now();
                    if (playback.pageReady && now >= nextStatus) {
                        nextStatus = now + std::chrono::milliseconds(80);
                        double position = 0;
                        int paused = 1;
                        bool available = playback.loaded && mpv_get_property(core, "time-pos", MPV_FORMAT_DOUBLE, &position) >= 0
                            && std::isfinite(position);
                        mpv_get_property(core, "pause", MPV_FORMAT_FLAG, &paused);
                        char *decoder = mpv_get_property_string(core, "hwdec-current");
                        NSString *decoderText = decoder ? [NSString stringWithUTF8String:decoder] : @"unavailable";
                        mpv_free(decoder);
                        NSDictionary *status = @{@"loaded": @(playback.loaded.load()), @"paused": @(paused != 0),
                            @"position": available ? @(position) : NSNull.null,
                            @"decoder": decoderText ?: @"unavailable"};
                        NSData *data = [NSJSONSerialization dataWithJSONObject:status options:0 error:nil];
                        NSString *json = [[NSString alloc] initWithData:data encoding:NSUTF8StringEncoding];
                        // Only scalar status crosses the bridge. WebKit calls remain
                        // on main; queued updates are ignored once shutdown starts.
                        dispatch_async(dispatch_get_main_queue(), ^{
                            if (!playback.stop) [web evaluateJavaScript:
                                [NSString stringWithFormat:@"window.vsfStatus(%@)", json] completionHandler:nil];
                        });
                    }
                } // autoreleasepool
                std::this_thread::sleep_for(std::chrono::milliseconds(4));
            }
            std::fprintf(stderr, "CONTROL STOP %s\n", playback.stop ? "requested" : "deadline");
        } catch (const std::exception &error) {
            exitStatus = 1;
            std::fprintf(stderr, "CONTROL ERROR: %s\n", error.what());
        }
        playback.stop = true;
        // Main retains the drawable and keeps servicing events. Control joins
        // rendering before destroying the core; rendering never waits on control.
        if (renderer.joinable()) renderer.join();
        if (core) mpv_terminate_destroy(core);
        std::fprintf(stderr, "CONTROL WORKER STOPPED\n");
        dispatch_async(dispatch_get_main_queue(), ^{
            [NSApp stop:nil];
            [NSApp postEvent:[NSEvent otherEventWithType:NSEventTypeApplicationDefined
                location:NSZeroPoint modifierFlags:0 timestamp:0 windowNumber:0
                context:nil subtype:0 data1:0 data2:0] atStart:NO];
        });
    }
}

@interface Probe : NSObject <NSWindowDelegate, WKScriptMessageHandler>
@property NSWindow *window;
@property WKWebView *web;
@property NSView *video;
@property CAMetalLayer *metal;
@property NSRect desired;
@end
@implementation Probe
- (BOOL)windowShouldClose:(NSWindow *)sender { (void)sender; std::fprintf(stderr, "CLOSE SOURCE native window\n"); playback.stop = true; return NO; }
- (void)layout {
    if (self.desired.size.width <= 0 || self.desired.size.height <= 0) return;
    std::unique_lock lock(playback.drawable, std::try_to_lock);
    if (!lock.owns_lock()) return; // Main stays responsive; timer retries latest geometry.
    NSRect rect = self.desired;
    rect.origin.y = self.window.contentView.bounds.size.height - NSMaxY(rect);
    double scale = self.window.backingScaleFactor;
    if (!NSEqualRects(self.video.frame, rect) || self.metal.contentsScale != scale) {
        self.video.frame = rect;
        self.metal.frame = self.video.bounds;
        self.metal.contentsScale = scale;
        self.metal.drawableSize = CGSizeMake(rect.size.width * scale, rect.size.height * scale);
        std::fprintf(stderr, "LAYOUT %.0f %.0f %.0f %.0f scale=%.1f\n", rect.origin.x,
            rect.origin.y, rect.size.width, rect.size.height, scale);
        playback.dirty = true;
    }
}
- (void)userContentController:(WKUserContentController *)controller didReceiveScriptMessage:(WKScriptMessage *)message {
    (void)controller;
    if (![message.body isKindOfClass:NSDictionary.class]) return;
    NSDictionary *body = message.body;
    NSString *op = body[@"op"];
    if ([op isEqual:@"layout"]) {
        for (NSString *key in @[@"x", @"y", @"width", @"height"])
            if (![body[key] isKindOfClass:NSNumber.class]) return;
        self.desired = NSMakeRect([body[@"x"] doubleValue], [body[@"y"] doubleValue],
                                  MAX(1, [body[@"width"] doubleValue]), MAX(1, [body[@"height"] doubleValue]));
        [self layout];
    } else if ([op isEqual:@"ready"]) playback.pageReady = true;
    else if ([op isEqual:@"play"]) playback.enqueue({"cycle", "pause"});
    else if ([op isEqual:@"seek"] && [body[@"seconds"] isKindOfClass:NSNumber.class]) {
        double seconds = [body[@"seconds"] doubleValue];
        if (std::isfinite(seconds) && seconds >= 0 && seconds <= 6.01)
            playback.enqueue({"seek", std::to_string(seconds), "absolute+exact"});
    }
    else if ([op isEqual:@"stop"]) playback.enqueue({"stop"});
    else if ([op isEqual:@"reload"]) playback.enqueue({"reload-media"});
    else if ([op isEqual:@"edit"]) playback.enqueue({"preview-edit"});
    else if ([op isEqual:@"subtitle"]) playback.enqueue({"cycle", "sub-visibility"});
    else if ([op isEqual:@"overlay"]) self.video.hidden = [body[@"visible"] boolValue];
}
@end

int main(int argc, char **argv) {
    if (argc != 3) { std::fprintf(stderr, "usage: angle-composition-probe VIDEO SUBTITLES\n"); return 2; }
    @autoreleasepool {
        // Read the small bundled diagnostic page before starting the UI loop.
        NSString *path = [NSBundle.mainBundle.resourcePath stringByAppendingPathComponent:@"composition.html"];
        NSError *error = nil;
        NSString *html = [NSString stringWithContentsOfFile:path encoding:NSUTF8StringEncoding error:&error];
        if (!html) { std::fprintf(stderr, "Cannot read diagnostic page: %s\n", error.localizedDescription.UTF8String); return 2; }
        [NSApplication sharedApplication];
        [NSApp setActivationPolicy:NSApplicationActivationPolicyRegular];
        Probe *probe = [Probe new];
        probe.window = [[NSWindow alloc] initWithContentRect:NSMakeRect(0, 0, 960, 720)
            styleMask:NSWindowStyleMaskTitled|NSWindowStyleMaskClosable|NSWindowStyleMaskResizable
            backing:NSBackingStoreBuffered defer:NO];
        probe.window.title = @"VSF ANGLE Metal composition probe";
        probe.window.delegate = probe;
        WKWebViewConfiguration *config = [WKWebViewConfiguration new];
        [config.userContentController addScriptMessageHandler:probe name:@"probe"];
        probe.web = [[WKWebView alloc] initWithFrame:probe.window.contentView.bounds configuration:config];
        probe.web.autoresizingMask = NSViewWidthSizable|NSViewHeightSizable;
        [probe.window.contentView addSubview:probe.web];
        probe.video = [[NSView alloc] initWithFrame:NSMakeRect(16, 160, 928, 460)];
        probe.video.wantsLayer = YES;
        probe.video.layer.masksToBounds = YES;
        probe.metal = [CAMetalLayer layer];
        probe.metal.frame = probe.video.bounds;
        [probe.video.layer addSublayer:probe.metal];
        [probe.window.contentView addSubview:probe.video positioned:NSWindowAbove relativeTo:probe.web];
        [probe.web loadHTMLString:html baseURL:nil];
        [probe.window center]; [probe.window makeKeyAndOrderFront:nil]; [NSApp activateIgnoringOtherApps:YES];
        NSTimer *timer = [NSTimer timerWithTimeInterval:0.03 repeats:YES block:^(NSTimer *) { [probe layout]; }];
        [[NSRunLoop mainRunLoop] addTimer:timer forMode:NSRunLoopCommonModes];
        std::thread worker(control, probe.metal, probe.web, std::string(argv[1]), std::string(argv[2]));
        // Worker signals normal termination after cleanup; main never joins a
        // live render thread while the app event loop is running.
        [NSApp run];
        playback.stop = true;
        worker.join();
        [timer invalidate];
        [config.userContentController removeScriptMessageHandlerForName:@"probe"];
    }
    return exitStatus;
}
