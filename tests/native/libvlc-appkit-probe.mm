// Isolated public-API libVLC 4 embedding experiment. AppKit owns the window/view;
// one worker owns libVLC handles/commands. VLC owns all decoding/rendering.
// No OpenGL/Metal calls, frame callbacks, private APIs or video IPC in this host.
#import <AppKit/AppKit.h>
#import <QuartzCore/QuartzCore.h>
#include <vlc/vlc.h>
#include <atomic>
#include <chrono>
#include <cstdlib>
#include <cstdio>
#include <deque>
#include <fstream>
#include <mutex>
#include <string>
#include <thread>
#include <vector>

struct Shared {
    std::mutex mutex;
    std::deque<int> commands;
    std::atomic<bool> stop{false}, done{false}, failed{false};
    std::atomic<int> state{libvlc_Stopped}, nextStatus{999}, prevStatus{999};
    std::atomic<unsigned> vouts{0}, revision{0};
    std::atomic<int64_t> time{-1};
    std::atomic<int64_t> duration{0};
    void enqueue(int command) {
        std::lock_guard lock(mutex);
        if (commands.size() < 16) commands.push_back(command);
        else { failed = true; std::fprintf(stderr, "COMMAND QUEUE FULL\n"); }
    }
};
static Shared shared;
static std::thread worker;
static std::string mediaPath, evidencePath;
static std::string diagnosticMode;
static NSArray *wavePeaks;
static void stateChanged(void *, libvlc_state_t state) { shared.state = state; }
static void voutChanged(void *, unsigned count) { shared.vouts = count; }
static void nextFrame(void *, int status) { shared.nextStatus = status; }
static void prevFrame(void *, int status) { shared.prevStatus = status; }
// Diagnostic callbacks only record values; never reenter the media player.
static void timeUpdated(void *, const libvlc_media_player_time_point_t *point) {
    std::printf("CLOCK_UPDATE ts=%lld date=%lld rate=%.3f\n",
        (long long)point->ts_us, (long long)point->system_date_us, point->rate);
}
static void timePaused(void *, libvlc_time_t date) {
    std::printf("CLOCK_PAUSE date=%lld\n", (long long)date);
}
static void timeSeek(void *, const libvlc_media_player_time_point_t *point) {
    std::printf("CLOCK_SEEK active=%d target=%lld\n", point != nullptr,
        point ? (long long)point->ts_us : -1LL);
}
static void check(int result, const char *operation) {
    std::printf("COMMAND %s result=%d\n", operation, result);
    if (result != 0) shared.failed = true;
}
static void addPreview(libvlc_media_player_t *player) {
    const unsigned revision = ++shared.revision;
    const std::string path = evidencePath + "/preview-" + std::to_string(revision) + ".srt";
    std::ofstream file(path);
    file << "1\n00:00:00,000 --> 00:00:10,000\n"
         << (revision == 1 ? "Original café — 日本語" : "Updated café — 日本語")
         << "\nPreview revision " << revision << "\n\n";
    file.close();
    if (!file) { shared.failed = true; return; }
    NSString *url = [[NSURL fileURLWithPath:[NSString stringWithUTF8String:path.c_str()]] absoluteString];
    check(libvlc_media_player_add_slave(player, libvlc_media_slave_type_subtitle, url.UTF8String, true), "preview");
}
static void seekDiagnostic(libvlc_media_player_t *player) {
    // Native transport operations run on the existing control worker. The UI
    // remains live; this does not synthesize input or resize windows.
    const auto pausedDeadline = std::chrono::steady_clock::now() + std::chrono::seconds(5);
    while (!shared.stop && shared.state != libvlc_Paused && std::chrono::steady_clock::now() < pausedDeadline)
        std::this_thread::sleep_for(std::chrono::milliseconds(10));
    if (shared.state != libvlc_Paused) { shared.failed = true; shared.stop = true; return; }
    unsigned snapshotIndex = 0;
    for (const int64_t target : {1500000, 500000, 2500000}) {
        if (shared.stop) break;
        std::this_thread::sleep_for(std::chrono::milliseconds(750));
        const int64_t before = libvlc_media_player_get_time(player);
        shared.nextStatus = 999;
        check(libvlc_media_player_set_time(player, target, false), "diagnostic paused seek");
        unsigned settled = 0;
        for (int sample = 0; sample < 100 && !shared.stop; ++sample) {
            shared.time = libvlc_media_player_get_time(player);
            shared.duration = libvlc_media_player_get_length(player);
            const bool near = std::llabs(shared.time.load() - target) <= 40000;
            settled = near && shared.state == libvlc_Paused ? settled + 1 : 0;
            std::printf("SEEK_SAMPLE target=%lld sample=%d time=%lld state=%d next=%d\n",
                (long long)target, sample, (long long)shared.time.load(), shared.state.load(), shared.nextStatus.load());
            std::this_thread::sleep_for(std::chrono::milliseconds(20));
        }
        const bool passed = settled >= 10;
        shared.failed = shared.failed || !passed;
        std::printf("SEEK_RESULT mode=%s target=%lld before=%lld after=%lld settled=%u passed=%d\n",
            diagnosticMode.c_str(), (long long)target, (long long)before, (long long)shared.time.load(), settled, passed);
        // Engine snapshots for an independent frame oracle, not screenshots or
        // a measurement of physical presentation. No decoded image crosses IPC.
        const std::string path = evidencePath + "/frame-" + std::to_string(++snapshotIndex) + ".png";
        const int snapshotResult = libvlc_video_take_snapshot(player, 0, path.c_str(), 0, 0);
        check(snapshotResult, "diagnostic snapshot");
        // Success only acknowledges the request; the runner verifies the file.
    }
    shared.stop = true;
}
static void control(void *view) {
    @autoreleasepool {
        std::vector<const char *> args = {"--ignore-config", "--no-video-title-show", "--verbose=2"};
        if (diagnosticMode == "seek-video") args.push_back("--no-audio");
        // The pinned VLC 4 source gives its software decoder the ffmpeg
        // shortcut; avcodec-hw is obsolete. Verify actual selection in logs.
        if (diagnosticMode == "seek-software") args.push_back("--codec=ffmpeg,none");
        if (diagnosticMode == "seek-dummy") {
            args.push_back("--codec=ffmpeg,none");
            args.push_back("--vout=dummy");
        }
        libvlc_instance_t *instance = libvlc_new((int)args.size(), args.data());
        libvlc_media_player_t *player = nullptr;
        libvlc_media_t *media = nullptr;
        bool watching = false;
        if (instance) {
            static const libvlc_media_player_cbs callbacks = {
                .version = 0, .on_state_changed = stateChanged,
                .on_next_frame_status = nextFrame, .on_prev_frame_status = prevFrame,
                .on_vout_changed = voutChanged
            };
            player = libvlc_media_player_new(instance, &callbacks, nullptr);
            media = libvlc_media_new_path(mediaPath.c_str());
        }
        if (!instance || !player || !media) { shared.failed = true; shared.stop = true; }
        if (!shared.stop && !diagnosticMode.empty()) {
            static const libvlc_media_player_watch_time_cbs callbacks = {
                .version = 0, .on_update = timeUpdated, .on_paused = timePaused, .on_seek = timeSeek
            };
            watching = libvlc_media_player_watch_time(player, 10000, &callbacks, nullptr) == 0;
            if (!watching) { shared.failed = true; shared.stop = true; }
        }
        if (!shared.stop) {
            libvlc_media_player_set_nsobject(player, view);
            libvlc_media_player_set_media(player, media);
            if (diagnosticMode.empty() || diagnosticMode == "seek-subtitles") addPreview(player);
            check(libvlc_media_player_play(player), "play");
            auto deadline = std::chrono::steady_clock::now() + std::chrono::seconds(10);
            while (!shared.stop && std::chrono::steady_clock::now() < deadline &&
                   (shared.vouts == 0 || libvlc_media_player_get_time(player) < 200000))
                std::this_thread::sleep_for(std::chrono::milliseconds(10));
            if (shared.vouts == 0) { shared.failed = true; shared.stop = true; }
            if (!shared.stop) {
                libvlc_media_player_set_pause(player, 1);
                std::puts("READY video output initialized; pause requested");
            }
        }
        if (!shared.stop && !diagnosticMode.empty()) seekDiagnostic(player);
        while (!shared.stop) {
            int command = 0;
            {
                std::lock_guard lock(shared.mutex);
                if (!shared.commands.empty()) { command = shared.commands.front(); shared.commands.pop_front(); }
            }
            switch (command) {
                case 1: libvlc_media_player_pause(player); std::puts("COMMAND toggle pause"); break;
                case 2: check(libvlc_media_player_set_time(player, 1500000, false), "seek 1.5s"); break;
                case 3: shared.nextStatus = 999; libvlc_media_player_next_frame(player); std::puts("COMMAND next frame"); break;
                case 4: shared.prevStatus = 999; libvlc_media_player_previous_frame(player); std::puts("COMMAND previous frame"); break;
                case 5: addPreview(player); break;
                default: break;
            }
            shared.time = libvlc_media_player_get_time(player);
            shared.duration = libvlc_media_player_get_length(player);
            std::this_thread::sleep_for(std::chrono::milliseconds(20));
        }
        if (player) {
            if (libvlc_media_player_stop_async(player) == 0) {
                const auto deadline = std::chrono::steady_clock::now() + std::chrono::seconds(10);
                while (shared.state != libvlc_Stopped && std::chrono::steady_clock::now() < deadline)
                    std::this_thread::sleep_for(std::chrono::milliseconds(10));
                if (shared.state != libvlc_Stopped) shared.failed = true;
            }
            if (watching) libvlc_media_player_unwatch_time(player);
            libvlc_media_player_release(player);
        }
        if (media) libvlc_media_release(media);
        if (instance) libvlc_release(instance);
        std::puts("CORE RELEASED");
        shared.done = true;
    }
}

@interface WaveView : NSView
@end
@implementation WaveView
- (void)drawRect:(NSRect)dirty {
    (void)dirty;
    [[NSColor colorWithWhite:0.12 alpha:1] setFill]; NSRectFill(self.bounds);
    [[NSColor systemTealColor] setStroke];
    NSBezierPath *path = [NSBezierPath bezierPath];
    for (NSUInteger i = 0; i < wavePeaks.count; ++i) {
        NSArray *pair = wavePeaks[i];
        const CGFloat x = i * self.bounds.size.width / wavePeaks.count;
        [path moveToPoint:NSMakePoint(x, (1 + [pair[0] doubleValue]) * self.bounds.size.height / 2)];
        [path lineToPoint:NSMakePoint(x, (1 + [pair[1] doubleValue]) * self.bounds.size.height / 2)];
    }
    [path stroke];
    if (shared.time >= 0 && shared.duration > 0) {
        [[NSColor systemOrangeColor] setFill];
        NSRectFill(NSMakeRect(shared.time / (double)shared.duration * self.bounds.size.width, 0, 2, self.bounds.size.height));
    }
}
@end

@interface Probe : NSObject <NSApplicationDelegate, NSWindowDelegate>
@property(strong) NSWindow *window;
@property(strong) NSView *video;
@property(strong) WaveView *wave;
@property(strong) NSTextField *status;
@property(strong) NSTimer *timer;
@end
@implementation Probe
- (void)applicationDidFinishLaunching:(NSNotification *)notification {
    (void)notification;
    self.window = [[NSWindow alloc] initWithContentRect:NSMakeRect(100, 100, 900, 660)
        styleMask:NSWindowStyleMaskTitled|NSWindowStyleMaskClosable|NSWindowStyleMaskResizable|NSWindowStyleMaskMiniaturizable
        backing:NSBackingStoreBuffered defer:NO];
    self.window.title = @"VSF · libVLC 4 native embedding";
    self.window.minSize = NSMakeSize(800, 440);
    self.window.delegate = self;
    self.window.releasedWhenClosed = NO;
    NSView *content = self.window.contentView;
    NSView *container = [[NSView alloc] initWithFrame:NSMakeRect(16, 180, 868, 464)];
    container.autoresizingMask = NSViewWidthSizable|NSViewHeightSizable;
    [content addSubview:container];
    // This pinned VLC window module copies drawable.frame into its child frame.
    // A zero-origin drawable keeps placement in the outer AppKit container.
    self.video = [[NSView alloc] initWithFrame:container.bounds];
    self.video.autoresizingMask = NSViewWidthSizable|NSViewHeightSizable;
    self.video.wantsLayer = YES;
    self.video.layer.masksToBounds = YES;
    [container addSubview:self.video];
    self.wave = [[WaveView alloc] initWithFrame:NSMakeRect(16, 90, 868, 70)];
    self.wave.autoresizingMask = NSViewWidthSizable;
    [content addSubview:self.wave];
    self.status = [NSTextField labelWithString:@"Initializing native video…"];
    self.status.frame = NSMakeRect(16, 60, 868, 22);
    self.status.autoresizingMask = NSViewWidthSizable;
    [content addSubview:self.status];
    NSArray *titles = @[@"Play / Pause", @"Seek 1.5s", @"Next frame", @"Previous frame", @"Update subtitle", @"Modal", @"Close"];
    CGFloat x = 16;
    for (NSUInteger i = 0; i < titles.count; ++i) {
        NSButton *button = [NSButton buttonWithTitle:titles[i] target:self action:@selector(action:)];
        button.tag = i + 1;
        button.frame = NSMakeRect(x, 16, i == 6 ? 60 : 110, 30);
        [content addSubview:button]; x += i == 6 ? 64 : 114;
    }
    [self.window makeKeyAndOrderFront:nil];
    [NSApp activateIgnoringOtherApps:YES];
    // The delegate retains the view until the worker releases its VLC core.
    worker = std::thread(control, (__bridge void *)self.video);
    self.timer = [NSTimer scheduledTimerWithTimeInterval:0.1 target:self selector:@selector(poll:) userInfo:nil repeats:YES];
}
- (void)action:(NSButton *)sender {
    if (sender.tag <= 5) shared.enqueue((int)sender.tag);
    else if (sender.tag == 6) {
        NSAlert *alert = [[NSAlert alloc] init];
        alert.messageText = @"Native modal over VLC video";
        alert.informativeText = @"Close this sheet to continue the composition check.";
        [alert beginSheetModalForWindow:self.window completionHandler:nil];
    } else shared.stop = true;
}
- (void)poll:(NSTimer *)timer {
    static unsigned loggedRevision = 0;
    if (shared.revision && loggedRevision != shared.revision && self.video.subviews.count) {
        loggedRevision = shared.revision;
        std::printf("HOST frame=%s bounds=%s layer=%s\n", NSStringFromRect(self.video.frame).UTF8String,
            NSStringFromRect(self.video.bounds).UTF8String, NSStringFromClass(self.video.layer.class).UTF8String);
        for (NSView *view in self.video.subviews)
            std::printf("CHILD class=%s frame=%s bounds=%s layer=%s\n", NSStringFromClass(view.class).UTF8String,
                NSStringFromRect(view.frame).UTF8String, NSStringFromRect(view.bounds).UTF8String,
                NSStringFromClass(view.layer.class).UTF8String);
    }
    self.status.stringValue = [NSString stringWithFormat:@"Time %.3f s · state %d · vouts %u · revision %u · next %d · previous %d",
        shared.time.load()/1000000.0, shared.state.load(), shared.vouts.load(), shared.revision.load(), shared.nextStatus.load(), shared.prevStatus.load()];
    [self.wave setNeedsDisplay:YES];
    if (shared.done) {
        worker.join(); // done is published after all native cleanup; main never waits on active work.
        [timer invalidate];
        [self.video removeFromSuperview];
        std::printf("VIEW REMOVED failed=%d\n", shared.failed.load());
        // Return through main so diagnostic failures have a nonzero exit.
        // All native resources have already been released and the worker joined.
        [NSApp stop:nil];
        // stop: changes the run-loop flag but a timer callback does not itself
        // wake nextEventMatchingMask:. An application-defined event lets it return.
        [NSApp postEvent:[NSEvent otherEventWithType:NSEventTypeApplicationDefined
            location:NSZeroPoint modifierFlags:0 timestamp:0 windowNumber:0
            context:nil subtype:0 data1:0 data2:0] atStart:YES];
    }
}
- (void)windowDidResize:(NSNotification *)notification {
    (void)notification;
    std::printf("RESIZE video=%.0fx%.0f scale=%.1f\n", self.video.bounds.size.width, self.video.bounds.size.height, self.window.backingScaleFactor);
}
- (BOOL)windowShouldClose:(NSWindow *)sender { (void)sender; shared.stop = true; return NO; }
- (NSApplicationTerminateReply)applicationShouldTerminate:(NSApplication *)sender {
    (void)sender;
    if (shared.done) return NSTerminateNow;
    shared.stop = true; return NSTerminateCancel;
}
@end
int main(int argc, char **argv) {
    if (argc != 4 && argc != 5) { std::fprintf(stderr, "usage: probe VIDEO EVIDENCE_DIR PEAKS_JSON [seek-av|seek-video|seek-subtitles|seek-software|seek-dummy]\n"); return 2; }
    if (argc == 5) {
        diagnosticMode = argv[4];
        if (diagnosticMode != "seek-av" && diagnosticMode != "seek-video" && diagnosticMode != "seek-subtitles" && diagnosticMode != "seek-software" && diagnosticMode != "seek-dummy") return 2;
    }
    setvbuf(stdout, nullptr, _IOLBF, 0);
    mediaPath = argv[1]; evidencePath = argv[2];
    @autoreleasepool {
        // Diagnostic input read before the application event loop starts.
        NSData *data = [NSData dataWithContentsOfFile:[NSString stringWithUTF8String:argv[3]]];
        wavePeaks = data ? [NSJSONSerialization JSONObjectWithData:data options:0 error:nil] : nil;
        if (![wavePeaks isKindOfClass:[NSArray class]] || !wavePeaks.count) return 2;
        [NSApplication sharedApplication];
        [NSApp setActivationPolicy:NSApplicationActivationPolicyRegular];
        Probe *delegate = [[Probe alloc] init];
        NSApp.delegate = delegate;
        [NSApp run];
    }
    return shared.failed ? 1 : 0;
}
