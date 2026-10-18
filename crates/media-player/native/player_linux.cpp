// GTK main owns widgets, the GL context and mpv's render API. The Rust control
// worker owns ordinary mpv calls. No main-thread call waits for that worker.
// Rust's Surface Arc and exit gate retain widgets until renderer/core teardown.
#include <gtk/gtk.h>
#include <epoxy/gl.h>
#include <epoxy/egl.h>
#include "mpv_client.hpp"
#include <mpv/render_gl.h>
#include <algorithm>
#include <atomic>
#include <cmath>
#include <condition_variable>
#include <cstdio>
#include <cstdlib>
#include <functional>
#include <memory>
#include <mutex>
#include <thread>

namespace {
struct Bounds {
    double x = 0, y = 0, width = 1, height = 1;
    bool visible = false, presenting = false, occluded = false;
};
struct Surface {
    GtkWidget *box = nullptr, *web = nullptr, *overlay = nullptr, *area = nullptr;
    int position = 0;
    gboolean expand = TRUE, fill = TRUE;
    guint padding = 0, timer = 0;
    GtkPackType pack = GTK_PACK_START;
    std::thread::id ui_thread = std::this_thread::get_id();
    std::mutex mutex; // Desired state only; never held during GTK/mpv calls.
    Bounds desired;
    std::atomic<bool> layout_dirty{true}, frame_dirty{false};
    std::atomic<uint64_t> frames{0};
    std::atomic<int> error{0};
    mpv_render_context *renderer = nullptr; // Main only, including free.
    Bounds applied; // Main only; used by overlay allocation.
    const bool trace = std::getenv("VSF_RENDER_TRACE") != nullptr;
    gint64 last_tick = 0, last_render = 0, last_report = 0;
    gint64 max_tick_gap = 0, max_render_time = 0;
    uint64_t queued_renders = 0, skipped_renders = 0;
};
struct Playback {
    mpv_handle *core = nullptr;
    Surface *surface = nullptr;
    vsf_mpv::FontDiagnostics fontDiagnostics;
};
struct MainCall {
    std::function<void()> action;
    std::mutex mutex;
    std::condition_variable condition;
    bool done = false;
};

// Explicitly attach a source: g_main_context_invoke can run immediately on a
// worker if it acquires an idle context, which would violate GTK affinity.
void on_main(Surface &surface, std::function<void()> action) {
    if (std::this_thread::get_id() == surface.ui_thread) { action(); return; }
    auto call = std::make_shared<MainCall>();
    call->action = std::move(action);
    auto *source = g_idle_source_new();
    g_source_set_callback(source, [](gpointer data) -> gboolean {
        auto call = *static_cast<std::shared_ptr<MainCall> *>(data);
        call->action();
        std::lock_guard lock(call->mutex);
        call->done = true;
        call->condition.notify_one();
        return G_SOURCE_REMOVE;
    }, new std::shared_ptr<MainCall>(call), [](gpointer data) {
        delete static_cast<std::shared_ptr<MainCall> *>(data);
    });
    g_source_attach(source, g_main_context_default());
    g_source_unref(source);
    std::unique_lock lock(call->mutex);
    call->condition.wait(lock, [&] { return call->done; });
}

void *gl_proc(void *, const char *name) { return reinterpret_cast<void *>(eglGetProcAddress(name)); }
void frame_ready(void *raw) { static_cast<Surface *>(raw)->frame_dirty.store(true); }

gboolean position_video(GtkOverlay *, GtkWidget *child, GdkRectangle *rect, gpointer raw) {
    auto &s = *static_cast<Surface *>(raw);
    if (child != s.area) return FALSE;
    const auto &b = s.applied;
    *rect = {static_cast<int>(std::lround(b.x)), static_cast<int>(std::lround(b.y)),
             std::max(1, static_cast<int>(std::lround(b.width))),
             std::max(1, static_cast<int>(std::lround(b.height)))};
    return TRUE;
}
void apply_layout(Surface &s) {
    { std::lock_guard lock(s.mutex); s.applied = s.desired; }
    const auto &b = s.applied;
    bool visible = b.visible && b.presenting && !b.occluded;
    gtk_widget_set_visible(s.area, visible);
    gtk_widget_queue_resize(s.overlay);
    if (visible) gtk_gl_area_queue_render(GTK_GL_AREA(s.area));
}
gboolean render(GtkGLArea *area, GdkGLContext *, gpointer raw) {
    auto &s = *static_cast<Surface *>(raw);
    if (!s.renderer) return TRUE;
    const gint64 started = s.trace ? g_get_monotonic_time() : 0;
    mpv_render_context_update(s.renderer);
    gtk_gl_area_attach_buffers(area);
    GLint framebuffer = 0;
    glGetIntegerv(GL_DRAW_FRAMEBUFFER_BINDING, &framebuffer);
    const int scale = gtk_widget_get_scale_factor(GTK_WIDGET(area));
    mpv_opengl_fbo fbo{framebuffer, gtk_widget_get_allocated_width(GTK_WIDGET(area)) * scale,
                      gtk_widget_get_allocated_height(GTK_WIDGET(area)) * scale, 0};
    int flip = 1, block = 0;
    mpv_render_param params[] = {{MPV_RENDER_PARAM_OPENGL_FBO, &fbo},
        {MPV_RENDER_PARAM_FLIP_Y, &flip}, {MPV_RENDER_PARAM_BLOCK_FOR_TARGET_TIME, &block},
        {MPV_RENDER_PARAM_INVALID, nullptr}};
    int result = mpv_render_context_render(s.renderer, params);
    if (result < 0) s.error.store(result);
    else s.frames.fetch_add(1);
    if (s.trace) {
        s.last_render = g_get_monotonic_time();
        s.max_render_time = std::max(s.max_render_time, s.last_render - started);
    }
    return TRUE;
}
gboolean tick(gpointer raw) {
    auto &s = *static_cast<Surface *>(raw);
    if (s.trace) {
        const gint64 now = g_get_monotonic_time();
        if (s.last_tick) s.max_tick_gap = std::max(s.max_tick_gap, now - s.last_tick);
        s.last_tick = now;
        if (s.renderer && now - s.last_report >= 500000) {
            GtkAllocation rect, clip;
            gtk_widget_get_allocation(s.area, &rect);
            gtk_widget_get_clip(s.area, &clip);
            std::fprintf(stderr, "VSF render trace t=%lld tick_gap_us=%lld render_us=%lld age_us=%lld frames=%llu queued=%llu skipped=%llu mapped=%d visible=%d rect=%d,%d,%d,%d clip=%d,%d,%d,%d\n",
                static_cast<long long>(now), static_cast<long long>(s.max_tick_gap),
                static_cast<long long>(s.max_render_time), static_cast<long long>(s.last_render ? now-s.last_render : -1),
                static_cast<unsigned long long>(s.frames.load()), static_cast<unsigned long long>(s.queued_renders),
                static_cast<unsigned long long>(s.skipped_renders), gtk_widget_get_mapped(s.area), gtk_widget_get_visible(s.area),
                rect.x, rect.y, rect.width, rect.height, clip.x, clip.y, clip.width, clip.height);
            s.last_report = now;
            s.max_tick_gap = 0; s.max_render_time = 0;
        }
    }
    if (s.layout_dirty.exchange(false)) apply_layout(s);
    if (s.renderer && s.frame_dirty.exchange(false)) {
        if (gtk_widget_get_mapped(s.area)) {
            ++s.queued_renders;
            gtk_gl_area_queue_render(GTK_GL_AREA(s.area));
        }
        else {
            // Hidden surfaces must still service render updates to avoid blocking
            // the core during HTML dialogs, clipping, unload or replacement.
            gtk_gl_area_make_current(GTK_GL_AREA(s.area));
            mpv_render_context_update(s.renderer);
            int skip = 1;
            mpv_render_param params[] = {{MPV_RENDER_PARAM_SKIP_RENDERING, &skip},
                                         {MPV_RENDER_PARAM_INVALID, nullptr}};
            int result = mpv_render_context_render(s.renderer, params);
            ++s.skipped_renders;
            if (result < 0) s.error.store(result);
        }
    }
    return G_SOURCE_CONTINUE;
}
}

extern "C" void *vsf_surface_create(void *parent) try {
    auto *box = GTK_WIDGET(parent);
    if (!GTK_IS_BOX(box)) return nullptr;
    // Tauri's public default_vbox contains its menu and Wry's direct WebKit
    // child. Fail explicitly if that topology changes; never wrap the menu.
    GType web_type = g_type_from_name("WebKitWebView");
    GList *children = gtk_container_get_children(GTK_CONTAINER(box));
    GtkWidget *web = nullptr;
    for (GList *it = children; it; it = it->next) {
        if (web_type && g_type_is_a(G_OBJECT_TYPE(it->data), web_type)) {
            if (web) { g_list_free(children); return nullptr; }
            web = GTK_WIDGET(it->data);
        }
    }
    g_list_free(children);
    if (!web) return nullptr;
    auto s = std::make_unique<Surface>();
    s->box = box; s->web = web;
    gtk_container_child_get(GTK_CONTAINER(box), web, "position", &s->position, nullptr);
    gtk_box_query_child_packing(GTK_BOX(box), web, &s->expand, &s->fill, &s->padding, &s->pack);
    s->overlay = gtk_overlay_new(); s->area = gtk_gl_area_new();
    gtk_gl_area_set_required_version(GTK_GL_AREA(s->area), 3, 2);
    gtk_gl_area_set_auto_render(GTK_GL_AREA(s->area), FALSE);
    gtk_widget_set_can_focus(s->area, FALSE);
    g_object_ref(web);
    gtk_container_remove(GTK_CONTAINER(box), web);
    gtk_container_add(GTK_CONTAINER(s->overlay), web);
    g_object_unref(web);
    gtk_overlay_add_overlay(GTK_OVERLAY(s->overlay), s->area);
    // Keyboard and mouse remain with WebKit controls underneath the video pane.
    gtk_overlay_set_overlay_pass_through(GTK_OVERLAY(s->overlay), s->area, TRUE);
    gtk_box_pack_start(GTK_BOX(box), s->overlay, s->expand, s->fill, s->padding);
    gtk_box_set_child_packing(GTK_BOX(box), s->overlay, s->expand, s->fill, s->padding, s->pack);
    gtk_box_reorder_child(GTK_BOX(box), s->overlay, s->position);
    g_signal_connect(s->overlay, "get-child-position", G_CALLBACK(position_video), s.get());
    g_signal_connect(s->area, "render", G_CALLBACK(render), s.get());
    gtk_widget_show_all(s->overlay);
    gtk_widget_realize(s->area);
    gtk_widget_hide(s->area);
    s->timer = g_timeout_add(8, tick, s.get());
    return s.release();
} catch (...) { return nullptr; }
extern "C" void vsf_surface_layout(void *raw, double x, double y, double width, double height, int visible) {
    auto &s = *static_cast<Surface *>(raw);
    { std::lock_guard lock(s.mutex);
      s.desired.x=x; s.desired.y=y; s.desired.width=width; s.desired.height=height;
      s.desired.visible=visible != 0; }
    s.layout_dirty.store(true);
}
extern "C" int vsf_surface_occlude(void *raw, int value) {
    auto &s = *static_cast<Surface *>(raw);
    { std::lock_guard lock(s.mutex); s.desired.occluded=value != 0; }
    bool visible = false;
    on_main(s, [&] {
        apply_layout(s); visible = gtk_widget_get_visible(s.area);
        if (std::getenv("VSF_MEDIA_SMOKE"))
            std::fprintf(stderr,"VSF Linux occlusion: requested=%d visible=%d\n",value != 0,visible);
    });
    return value == 0 || !visible;
}
extern "C" void vsf_surface_release(void *raw) {
    std::unique_ptr<Surface> s(static_cast<Surface *>(raw));
    on_main(*s, [&] {
        g_source_remove(s->timer);
        // Renderer and core are already gone. Restore Wry's original parent
        // before destroying our overlay so its own shutdown retains ownership.
        g_object_ref(s->web);
        gtk_container_remove(GTK_CONTAINER(s->overlay), s->web);
        gtk_box_pack_start(GTK_BOX(s->box), s->web, s->expand, s->fill, s->padding);
        gtk_box_set_child_packing(GTK_BOX(s->box), s->web, s->expand, s->fill, s->padding, s->pack);
        gtk_box_reorder_child(GTK_BOX(s->box), s->web, s->position);
        g_object_unref(s->web);
        gtk_widget_destroy(s->overlay);
        std::fprintf(stderr,"VSF Linux surface removed\n");
    });
}
extern "C" void *vsf_player_create(void *raw, int *error) try {
    auto player = std::make_unique<Playback>();
    player->surface = static_cast<Surface *>(raw);
    std::unique_ptr<mpv_handle, decltype(&mpv_terminate_destroy)> core(mpv_create(), mpv_terminate_destroy);
    if (!core) { *error=MPV_ERROR_NOMEM; return nullptr; }
    const char *options[][2] = {{"config","no"},{"terminal","no"},{"vo","libmpv"},
        {"clipboard-backends",""},{"hwdec","auto-copy"},{"vd-lavc-threads","4"},
        {"hr-seek-demuxer-offset","1"},{"pause","yes"},{"keep-open","yes"},
        {"idle","yes"},{"input-default-bindings","no"},{"input-vo-keyboard","no"}};
    for (const auto &option: options) {
        *error=mpv_set_option_string(core.get(),option[0],option[1]);
        if (*error<0) return nullptr;
    }
    *error = vsf_mpv::configure_font_logging(core.get(), std::getenv("VSF_MEDIA_SMOKE") != nullptr);
    if (*error < 0) return nullptr;
    *error=vsf_mpv::disable_scripts(core.get());
    if (*error<0) return nullptr;
    *error=mpv_initialize(core.get());
    if (*error<0) return nullptr;
    auto &s=*player->surface;
    s.error.store(0);
    on_main(s,[&] {
        gtk_gl_area_make_current(GTK_GL_AREA(s.area));
        if (auto *gl_error=gtk_gl_area_get_error(GTK_GL_AREA(s.area))) {
            std::fprintf(stderr,"VSF Linux GL error: %s\n",gl_error->message);
            *error=MPV_ERROR_UNSUPPORTED; return;
        }
        mpv_opengl_init_params gl{gl_proc,nullptr};
        int advanced=1;
        mpv_render_param params[]={{MPV_RENDER_PARAM_API_TYPE,const_cast<char *>(MPV_RENDER_API_TYPE_OPENGL)},
            {MPV_RENDER_PARAM_OPENGL_INIT_PARAMS,&gl},{MPV_RENDER_PARAM_ADVANCED_CONTROL,&advanced},
            {MPV_RENDER_PARAM_INVALID,nullptr}};
        *error=mpv_render_context_create(&s.renderer,core.get(),params);
        if (*error<0) return;
        std::fprintf(stderr,"VSF Linux GL: %s / %s / scale=%d\n",glGetString(GL_VERSION),
                         glGetString(GL_RENDERER),gtk_widget_get_scale_factor(s.area));
        mpv_render_context_set_update_callback(s.renderer,frame_ready,&s);
    });
    if (*error<0) return nullptr;
    player->core=core.release();
    return player.release();
} catch (...) { *error=MPV_ERROR_NOMEM; return nullptr; }
extern "C" void vsf_player_destroy(void *raw) {
    std::unique_ptr<Playback> player(static_cast<Playback *>(raw));
    auto &s=*player->surface;
    on_main(s,[&] {
        gtk_gl_area_make_current(GTK_GL_AREA(s.area));
        mpv_render_context_set_update_callback(s.renderer,nullptr,nullptr);
        mpv_render_context_free(s.renderer); s.renderer=nullptr;
        s.frame_dirty.store(false);
        std::fprintf(stderr,"VSF Linux render context removed\n");
    });
    mpv_terminate_destroy(player->core);
    std::fprintf(stderr,"VSF Linux core destroyed\n");
}
extern "C" int vsf_command(void *raw,const char **args) { return mpv_command(static_cast<Playback *>(raw)->core,args); }
extern "C" void vsf_presenting(void *raw,int value) {
    auto &s=*static_cast<Playback *>(raw)->surface;
    { std::lock_guard lock(s.mutex); s.desired.presenting=value != 0; }
    s.layout_dirty.store(true);
}
extern "C" int vsf_discard_events(void *raw) {
    auto *player = static_cast<Playback *>(raw);
    const int result = vsf_mpv::discard_events(player->core);
    if (result >= 0) player->fontDiagnostics.clear();
    return result;
}
extern "C" double vsf_number(void *raw, const char *name) {
    return vsf_mpv::number(static_cast<Playback *>(raw)->core, name);
}
extern "C" int vsf_flag(void *raw, const char *name) {
    return vsf_mpv::flag(static_cast<Playback *>(raw)->core, name);
}
extern "C" uint64_t vsf_frames(void *raw) { return static_cast<Playback *>(raw)->surface->frames.load(); }
extern "C" int vsf_event_error(void *raw, int *file_transition) {
    auto *player = static_cast<Playback *>(raw);
    return vsf_mpv::poll_events(static_cast<Playback *>(raw)->core, player->surface->error.exchange(0),
                                std::getenv("VSF_MEDIA_SMOKE") != nullptr, file_transition, &player->fontDiagnostics);
}
extern "C" const char *vsf_font_diagnostics(void *raw) {
    return static_cast<Playback *>(raw)->fontDiagnostics.text();
}
extern "C" char *vsf_text(void *raw,const char *name) { return mpv_get_property_string(static_cast<Playback *>(raw)->core,name); }
extern "C" void vsf_text_free(char *text) { mpv_free(text); }
extern "C" const char *vsf_error(int code) { return mpv_error_string(code); }
