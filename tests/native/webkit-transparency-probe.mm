// Isolated comparison: public under-page styling versus current private KVC.
#import <AppKit/AppKit.h>
#import <WebKit/WebKit.h>
int main() {
    @autoreleasepool {
        [NSApplication sharedApplication];
        [NSApp setActivationPolicy:NSApplicationActivationPolicyRegular];
        NSWindow *window = [[NSWindow alloc] initWithContentRect:NSMakeRect(0,0,900,400)
            styleMask:NSWindowStyleMaskTitled|NSWindowStyleMaskClosable backing:NSBackingStoreBuffered defer:NO];
        window.title = @"VSF transparency comparison";
        window.contentView.wantsLayer = YES;
        window.contentView.layer.backgroundColor = NSColor.systemGreenColor.CGColor;
        for (int i=0; i<2; ++i) {
            WKWebView *web = [[WKWebView alloc] initWithFrame:NSMakeRect(i*450,0,450,400)];
            web.underPageBackgroundColor = NSColor.clearColor;
            if (i) [web setValue:@NO forKey:@"drawsBackground"];
            [window.contentView addSubview:web];
            NSString *html = [NSString stringWithFormat:@"<html style='background:transparent'><body style='background:transparent;font:24px system-ui;color:black'><h2>%@</h2><p>Green means native background is visible.</p><button>HTML overlay</button></body></html>",i?@"Private KVC + public":@"Public API only"];
            [web loadHTMLString:html baseURL:nil];
        }
        [window center]; [window makeKeyAndOrderFront:nil];
        // Bounded diagnostic lifetime; all AppKit resources remain on main.
        dispatch_after(dispatch_time(DISPATCH_TIME_NOW,120*NSEC_PER_SEC),dispatch_get_main_queue(),^{ [NSApp terminate:nil]; });
        [NSApp run];
    }
}
