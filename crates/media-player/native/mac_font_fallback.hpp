#pragma once
#import <Foundation/Foundation.h>
#import <CoreText/CoreText.h>
#include <mpv/client.h>
#include <unistd.h>
#include <cstdlib>
#include <string>

// A last-resort font file for libass, used only after named/system fallbacks.
// The control owner creates this before mpv initialization and destroys it after
// render/core teardown, including failed initialization. No font is installed or
// copied. The private directory contains only a symlink to an installed OS font.
class MacFontFallback {
    std::string directory;
public:
    MacFontFallback() = default;
    MacFontFallback(const MacFontFallback &) = delete;
    MacFontFallback &operator=(const MacFontFallback &) = delete;
    ~MacFontFallback() {
        if (!directory.empty()) {
            ::unlink((directory + "/subfont.ttf").c_str());
            ::rmdir(directory.c_str());
        }
    }
    const char *path() const { return directory.c_str(); }
    bool prepare() {
        @autoreleasepool {
            CTFontRef font = CTFontCreateWithName(CFSTR("HiraginoSansGB-W3"), 16, nullptr);
            if (!font) return false;
            NSString *name = CFBridgingRelease(CTFontCopyPostScriptName(font));
            NSURL *url = CFBridgingRelease(CTFontCopyAttribute(font, kCTFontURLAttribute));
            CFRelease(font);
            if (![name hasPrefix:@"HiraginoSansGB-"] || !url.isFileURL ||
                ![NSFileManager.defaultManager isReadableFileAtPath:url.path]) return false;
            std::string pattern = [[NSTemporaryDirectory()
                stringByAppendingPathComponent:@"vsf-preview-font-XXXXXX"] fileSystemRepresentation];
            if (!::mkdtemp(pattern.data())) return false; // atomic, mode0700
            directory = std::move(pattern);
            return ::symlink(url.fileSystemRepresentation,
                             (directory + "/subfont.ttf").c_str()) == 0;
        }
    }
    int configure(mpv_handle *core) const {
        // config=no disables even subfont.ttf lookup. The pinned mpv path policy
        // confines ALL config lookup (including system paths and MPV_HOME) to
        // this private directory when config-dir is set. It contains no config
        // or scripts; disable_scripts remains mandatory. No user config is read.
        int result = mpv_set_option_string(core, "config-dir", path());
        if (result < 0) return result;
        return mpv_set_option_string(core, "config", "yes");
    }
};
