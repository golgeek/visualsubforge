// macOS's predefined Quit item and Dock Quit send terminate: directly to
// NSApplication. Tao 0.37 has no applicationShouldTerminate: implementation.
// Add that public delegate callback once, routing requests through our guarded
// Tauri exit path. No delegate replacement, swizzling or native media dependency.
#import <AppKit/AppKit.h>
#import <objc/runtime.h>
#include <atomic>
#include <string>

using QuitCallback = void (*)();
static std::atomic<QuitCallback> quit_callback{nullptr};

static NSApplicationTerminateReply should_terminate(id, SEL, NSApplication *) {
    if (auto callback = quit_callback.load()) callback();
    // The application's asynchronous Save/Discard/Cancel flow eventually uses
    // AppHandle::exit, not NSApplication::terminate:, after native cleanup.
    return NSTerminateCancel;
}

extern "C" bool vsf_install_quit_guard(QuitCallback callback) {
    if (![NSThread isMainThread] || !NSApp.delegate || !callback) return false;
    Class cls = [NSApp.delegate class];
    SEL selector = @selector(applicationShouldTerminate:);
    // Fail explicitly if a future framework adds its own termination policy;
    // silently replacing it would violate framework ownership.
    if (class_getInstanceMethod(cls, selector)) return false;
    std::string encoding = std::string(@encode(NSApplicationTerminateReply)) + "@:@";
    quit_callback.store(callback);
    if (!class_addMethod(cls, selector, reinterpret_cast<IMP>(should_terminate), encoding.c_str())) {
        quit_callback.store(nullptr);
        return false;
    }
    return true;
}
