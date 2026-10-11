// Isolated libass lifetime experiment; no application, GPU or media-clock integration.
// One thread owns the library, renderer and tracks. Image pointers are borrowed
// until the next render call; inspect them before rendering again or freeing a track.
#include <ass/ass.h>
#include <chrono>
#include <cstdint>
#include <cstdio>
#include <memory>
#include <stdexcept>
#include <string>
#include <vector>

using Clock = std::chrono::steady_clock;
using Library = std::unique_ptr<ASS_Library, decltype(&ass_library_done)>;
using Renderer = std::unique_ptr<ASS_Renderer, decltype(&ass_renderer_done)>;
using Track = std::unique_ptr<ASS_Track, decltype(&ass_free_track)>;

// Installed fonts in the two measured environments; this is not a product font policy.
#if defined(__APPLE__)
static constexpr const char *font_preset = "macos";
static constexpr const char *fonts[] = {"Hiragino Sans", "HiraginoSans-W4"};
#else
static constexpr const char *font_preset = "noto";
static constexpr const char *fonts[] = {"Noto Sans", "NotoSans-Regular"};
#endif

static void require(bool value, const char *message) {
    if (!value) throw std::runtime_error(message);
}
static double elapsed(Clock::time_point from, Clock::time_point to) {
    return std::chrono::duration<double, std::milli>(to - from).count();
}

struct Context {
    // Reverse destruction order: renderer first, then its library.
    Library library{ass_library_init(), ass_library_done};
    Renderer renderer{nullptr, ass_renderer_done};
    explicit Context(const char *font) {
        require(bool(library), "create ASS library");
        renderer.reset(ass_renderer_init(library.get()));
        require(bool(renderer), "create ASS renderer");
        ass_set_fonts(renderer.get(), nullptr, font, ASS_FONTPROVIDER_AUTODETECT, nullptr, 1);
        ass_set_storage_size(renderer.get(), 640, 360);
    }
};

struct ImageSummary { uint64_t hash = 14695981039346656037ULL; unsigned images = 0; };
static ImageSummary summarize(ASS_Image *image) {
    ImageSummary result;
    auto byte = [&](uint8_t value) { result.hash = (result.hash ^ value) * 1099511628211ULL; };
    auto number = [&](uint32_t value) {
        for (unsigned i = 0; i < 4; ++i) byte(static_cast<uint8_t>(value >> (8 * i)));
    };
    for (; image; image = image->next) {
        ++result.images;
        number(image->w); number(image->h); number(image->dst_x); number(image->dst_y);
        number(image->color);
        // Row padding is not part of the visible bitmap and may be uninitialized.
        for (int y = 0; y < image->h; ++y)
            for (int x = 0; x < image->w; ++x) byte(image->bitmap[y * image->stride + x]);
    }
    return result;
}

static std::string document(const char *font, unsigned revision) {
    return std::string("[Script Info]\nScriptType: v4.00+\nPlayResX: 640\nPlayResY: 360\n") +
        "[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, "
        "OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, "
        "Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n"
        "Style: Default," + font + ",24,&H00FFFFFF,&H000000FF,&H00000000,&H80000000,"
        "0,0,0,0,100,100,0,0,1,1,0,2,20,20,20,1\n"
        "[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n"
        "Dialogue: 0,0:00:01.00,0:00:04.00,Default,,0,0,0,,Revision " +
        std::to_string(revision) + ": café — 日本語\\NSecond line\n";
}

int main(int argc, char **argv) {
    try {
        require(argc == 1 || (argc == 2 && std::string(argv[1]) == "--reuse-first"),
                "usage: libass-revision-probe [--reuse-first]");
        const bool reuse_first = argc == 2;
        std::printf("{\"libass_version\":%d,\"iterations_per_mode\":32,\"reuse_first\":%s,\"font_preset\":\"%s\"}\n",
                    ass_library_version(), reuse_first ? "true" : "false", font_preset);
        for (const char *font : fonts) {
            std::vector<ImageSummary> reference;
            // Compare both lifetime modes in either order to expose cache/order sensitivity.
            for (bool reuse : {reuse_first, !reuse_first}) {
                std::unique_ptr<Context> context;
                for (unsigned revision = 0; revision < 32; ++revision) {
                    const int width = (revision / 8) % 2 ? 1280 : 640;
                    auto began = Clock::now();
                    if (!context) context = std::make_unique<Context>(font);
                    ass_set_frame_size(context->renderer.get(), width, width * 9 / 16);
                    auto configured = Clock::now();
                    std::string text = document(font, revision);
                    Track track(ass_read_memory(context->library.get(), text.data(), text.size(), nullptr),
                                ass_free_track);
                    require(bool(track), "parse diagnostic ASS document");
                    auto parsed = Clock::now();
                    int changed = 0;
                    ASS_Image *images = ass_render_frame(context->renderer.get(), track.get(), 2000, &changed);
                    auto rendered = Clock::now();
                    ImageSummary summary = summarize(images);
                    require(summary.images > 0, "active cue produced no bitmap");
                    if (reuse != reuse_first) {
                        require(summary.hash == reference[revision].hash &&
                                summary.images == reference[revision].images, "lifetime modes produced different bitmaps");
                    } else reference.push_back(summary);
                    require(!ass_render_frame(context->renderer.get(), track.get(), 999, nullptr), "stale cue before start");
                    require(ass_render_frame(context->renderer.get(), track.get(), 1000, nullptr), "cue missing at start");
                    require(ass_render_frame(context->renderer.get(), track.get(), 3999, nullptr), "cue missing before end");
                    require(!ass_render_frame(context->renderer.get(), track.get(), 4000, nullptr), "stale cue at end");
                    ImageSummary restored = summarize(ass_render_frame(context->renderer.get(), track.get(), 2000, nullptr));
                    require(restored.hash == summary.hash, "seek back changed bitmap");
                    std::printf("{\"font\":\"%s\",\"reuse\":%s,\"revision\":%u,\"width\":%d,"
                        "\"setup_ms\":%.6f,\"parse_ms\":%.6f,\"render_ms\":%.6f,\"total_ms\":%.6f,"
                        "\"images\":%u,\"hash\":\"%016llx\",\"boundary_checks\":true}\n",
                        font, reuse ? "true" : "false", revision, width,
                        elapsed(began, configured), elapsed(configured, parsed), elapsed(parsed, rendered),
                        elapsed(began, rendered), summary.images, static_cast<unsigned long long>(summary.hash));
                    // Destroy the old track before destroying its library, or replacing the document.
                    track.reset();
                    if (!reuse) context.reset();
                }
            }
        }
        // Retain complete evidence even if LeakSanitizer terminates during process exit.
        require(std::fflush(stdout) == 0, "flush diagnostic output");
        return 0;
    } catch (const std::exception &error) {
        std::fprintf(stderr, "FAIL: %s\n", error.what());
        return 1;
    }
}
