// Uses installed OS font metadata and a private temporary directory, no surface.
// Tests production helper ownership/isolation with stubbed mpv option calls.
#include "../../crates/media-player/native/mac_font_fallback.hpp"
#include <cassert>
#include <map>
#include <sys/stat.h>

#ifdef NDEBUG
#error This diagnostic requires assertions enabled.
#endif

static std::map<std::string, std::string> options;
extern "C" int mpv_set_option_string(mpv_handle *, const char *name, const char *value) {
    options[name] = value;
    return 0;
}
int main() {
    std::string previous;
    for (int i = 0; i < 20; ++i) {
        std::string path;
        {
            MacFontFallback font;
            assert(font.prepare());
            path = font.path();
            assert(path != previous);
            struct stat info{};
            assert(::stat(path.c_str(), &info) == 0 && (info.st_mode & 0777) == 0700);
            assert(::lstat((path + "/subfont.ttf").c_str(), &info) == 0 && S_ISLNK(info.st_mode));
            assert(font.configure(nullptr) == 0);
            assert(options["config-dir"] == path && options["config"] == "yes");
            // A second engine must have an independent resource lifetime.
            MacFontFallback other;
            assert(other.prepare() && path != other.path());
        }
        assert(::access(path.c_str(), F_OK) != 0);
        previous = path;
    }
    std::puts("PASS: Mac fallback resource isolation and cleanup (20 lifetimes)");
}
