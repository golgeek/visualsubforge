// Isolated GTK/WebKit/libmpv feasibility probe, not the production adapter.
// Worker: mpv control/core and logs. GTK thread: widgets, GL context/render API.
// Shutdown frees the render context before the worker destroys the core;
// the UI polls completion and never joins a live media worker.
#include <gtk/gtk.h>
#include <webkit2/webkit2.h>
#include <epoxy/gl.h>
#include <epoxy/egl.h>
#include <mpv/client.h>
#include <mpv/render_gl.h>
#include <math.h>
#include <stdio.h>

typedef struct { char *text; int generation; } Request;
typedef struct { double position; int pause, idle, generation; gboolean has_position; char *subtitle, *decoder; } State;
typedef struct {
    GtkWidget *window, *area;
    WebKitWebView *web;
    mpv_handle *core;
    mpv_render_context *renderer;
    GAsyncQueue *requests, *states, *notes;
    GThread *worker;
    const char *media, *subtitles;
    gint ready, stopping, render_gone, done, failed, pending, generation, queued;
    gint renders, smoke_done, blank_generation;
    // GTK-only presentation state, accepted from the worker's current generation.
    gboolean show_video;
    int display_generation;
    gboolean smoke;
} Probe;

static void free_state(State *s) {
    mpv_free(s->subtitle); mpv_free(s->decoder); g_free(s);
}

static void note(Probe *p, const char *text) { g_async_queue_push(p->notes, g_strdup(text)); }

static void request(Probe *p, const char *text) {
    if (g_atomic_int_get(&p->stopping)) return;
    if (g_atomic_int_get(&p->queued) >= 64) { note(p, "QUEUE FULL"); return; }
    if (!strcmp(text, "reload") || !strcmp(text, "unload")) g_atomic_int_inc(&p->generation);
    Request *r = g_new0(Request, 1);
    r->text = g_strdup(text); r->generation = g_atomic_int_get(&p->generation);
    g_atomic_int_inc(&p->queued); g_async_queue_push(p->requests, r);
}

static int command(Probe *p, const char **args) {
    int result = mpv_command(p->core, args);
    printf("COMMAND %s: %s\n", args[0], mpv_error_string(result));
    if (result < 0) g_atomic_int_set(&p->failed, 1);
    return result;
}

// Optional backend integration sequence. It uses the media request boundary;
// it does not synthesize clicks or establish keyboard/focus acceptance.
static void smoke_step(Probe *p, State *s, int *stage, gint64 *since, int *render_start) {
    if (!p->smoke || g_atomic_int_get(&p->smoke_done)) return;
    gint64 elapsed = g_get_monotonic_time() - *since;
    if (elapsed < 2000000) return;
    const char *sub = s->subtitle ? s->subtitle : "";
    int idle = 0;
    mpv_get_property(p->core, "idle-active", MPV_FORMAT_FLAG, &idle);
    gboolean drew = g_atomic_int_get(&p->renders) > *render_start;
    gboolean valid[] = {
        s->pause && fabs(s->position - 2) < .05 && strstr(sub, "café") && drew,
        s->pause && fabs(s->position - 2.7) < .05 && !*sub && drew,
        s->pause && fabs(s->position - 4) < .05 && strstr(sub, "Second cue") && drew,
        s->pause && fabs(s->position - 4) < .05 && strstr(sub, "Edited preview") && drew,
        !s->pause && fabs(s->position - 4) > .15 && drew,
        s->pause,
        idle && !*sub && g_atomic_int_get(&p->blank_generation) == s->generation,
        !idle && s->pause && fabs(s->position - 2) < .05 && strstr(sub, "café") && drew,
    };
    const char *next[] = {"seek:2.7", "seek:4", "edit", "toggle", "toggle", "unload", "reload"};
    gboolean position_valid = *stage == 6 ? !s->has_position : s->has_position;
    if (!valid[*stage] || !position_valid) {
        if (elapsed > 10000000) {
            printf("SMOKE FAIL stage=%d pos=%.6f pause=%d idle=%d renders=%d subtitle=%s\n",
                   *stage, s->position, s->pause, idle, g_atomic_int_get(&p->renders), sub);
            g_atomic_int_set(&p->failed, 1);
        }
        return;
    }
    printf("SMOKE PASS stage=%d pos=%.6f pause=%d idle=%d renders=%d decoder=%s subtitle=%s\n",
           *stage, s->position, s->pause, idle, g_atomic_int_get(&p->renders),
           s->decoder ? s->decoder : "", sub);
    if (*stage == 7) { g_atomic_int_set(&p->smoke_done, 1); return; }
    request(p, next[(*stage)++]); *since = g_get_monotonic_time();
    *render_start = g_atomic_int_get(&p->renders);
}

static gpointer worker(gpointer data) {
    Probe *p = data;
    p->core = mpv_create();
    if (!p->core) { g_atomic_int_set(&p->failed, 1); goto finished; }
    const char *options[][2] = {
        {"config", "no"}, {"vo", "libmpv"}, {"hwdec", "auto-copy"},
        {"idle", "yes"}, {"keep-open", "yes"}, {"pause", "yes"},
        {"mute", "yes"}, {"start", "2"}, {"loop-file", "inf"},
    };
    for (gsize i = 0; i < G_N_ELEMENTS(options); i++)
        if (mpv_set_option_string(p->core, options[i][0], options[i][1]) < 0)
            g_atomic_int_set(&p->failed, 1);
    if (mpv_initialize(p->core) < 0 || g_atomic_int_get(&p->failed)) {
        g_atomic_int_set(&p->failed, 1); goto destroy;
    }
    mpv_request_log_messages(p->core, "warn");
    printf("MPV %s\n", mpv_client_name(p->core));
    g_atomic_int_set(&p->ready, 1); // Publishes the initialized core to GTK.
    int player_generation = 0, edits = 0;
    gboolean pending_subtitle = FALSE;
    int smoke_stage = 0, render_start = 0;
    gint64 smoke_since = g_get_monotonic_time();
    while (!g_atomic_int_get(&p->stopping)) {
        char *line;
        while ((line = g_async_queue_try_pop(p->notes))) { puts(line); g_free(line); }
        Request *r = g_async_queue_timeout_pop(p->requests, 50000);
        if (r) {
            g_atomic_int_add(&p->queued, -1);
            const char *action = r->text;
            printf("REQUEST %s generation=%d\n", action, r->generation);
            if (r->generation != g_atomic_int_get(&p->generation)) puts("STALE REQUEST");
            else if (!strcmp(action, "toggle")) command(p, (const char *[]){"cycle", "pause", NULL});
            else if (g_str_has_prefix(action, "seek:")) {
                char *end = NULL; double seconds = g_ascii_strtod(action + 5, &end);
                if (end && !*end && isfinite(seconds) && seconds >= 0 && seconds <= 6)
                    command(p, (const char *[]){"seek", action + 5, "absolute+exact", NULL});
            } else if (!strcmp(action, "reload")) {
                pending_subtitle = command(p, (const char *[]){"loadfile", p->media, "replace", NULL}) >= 0;
                player_generation = r->generation;
            } else if (!strcmp(action, "unload")) {
                pending_subtitle = FALSE;
                command(p, (const char *[]){"stop", NULL}); player_generation = r->generation;
            } else if (!strcmp(action, "edit")) {
                char *subtitle = g_strdup_printf("memory://1\n00:00:00,000 --> 00:00:06,000\nEdited preview %d: café — 日本語\nSecond line\n", ++edits);
                command(p, (const char *[]){"sub-remove", NULL});
                command(p, (const char *[]){"sub-add", subtitle, "select", NULL});
                g_free(subtitle);
            }
            g_free(r->text); g_free(r);
        }
        mpv_event *event;
        while ((event = mpv_wait_event(p->core, 0))->event_id != MPV_EVENT_NONE) {
            // loadfile acknowledges before decoding/file initialization ends.
            // Adding an external subtitle before FILE_LOADED races that setup.
            if (event->event_id == MPV_EVENT_FILE_LOADED && pending_subtitle) {
                pending_subtitle = FALSE;
                command(p, (const char *[]){"sub-add", p->subtitles, "select", NULL});
            }
            if (event->event_id == MPV_EVENT_LOG_MESSAGE) {
                mpv_event_log_message *log = event->data;
                printf("MPV [%s] %s", log->prefix, log->text);
            }
            if (event->event_id == MPV_EVENT_END_FILE &&
                ((mpv_event_end_file *)event->data)->reason == MPV_END_FILE_REASON_ERROR) {
                mpv_event_end_file *end = event->data;
                // Freeing an active render context deliberately disables video.
                // The worker can receive that event after GTK starts shutdown.
                if (g_atomic_int_get(&p->stopping) && end->error == MPV_ERROR_VO_INIT_FAILED) {
                    puts("SHUTDOWN video disabled after render-context removal");
                } else {
                    printf("MEDIA ERROR %s\n", mpv_error_string(end->error));
                    g_atomic_int_set(&p->failed, 1);
                }
            }
        }
        State *s = g_new0(State, 1); s->generation = player_generation;
        s->has_position = mpv_get_property(p->core, "time-pos", MPV_FORMAT_DOUBLE, &s->position) >= 0
                          && isfinite(s->position);
        mpv_get_property(p->core, "pause", MPV_FORMAT_FLAG, &s->pause);
        s->idle = 1;
        mpv_get_property(p->core, "idle-active", MPV_FORMAT_FLAG, &s->idle);
        s->subtitle = mpv_get_property_string(p->core, "sub-text");
        s->decoder = mpv_get_property_string(p->core, "hwdec-current");
        smoke_step(p, s, &smoke_stage, &smoke_since, &render_start);
        State *old;
        while ((old = g_async_queue_try_pop(p->states))) free_state(old);
        g_async_queue_push(p->states, s);
    }
    while (!g_atomic_int_get(&p->render_gone)) g_usleep(1000);
destroy:
    mpv_terminate_destroy(p->core); p->core = NULL;
finished:
    puts("CORE DESTROYED"); g_atomic_int_set(&p->done, 1);
    return NULL;
}

static void *gl_proc(void *unused, const char *name) {
    (void)unused; return (void *)eglGetProcAddress(name);
}

static void frame_ready(void *data) {
    Probe *p = data; g_atomic_int_set(&p->pending, 1);
}

static gboolean render(GtkGLArea *area, GdkGLContext *context, gpointer data) {
    (void)context; Probe *p = data;
    if (!g_atomic_int_get(&p->ready) || g_atomic_int_get(&p->stopping)) return TRUE;
    if (!p->renderer) {
        mpv_opengl_init_params gl = { .get_proc_address = gl_proc };
        mpv_render_param params[] = {
            {MPV_RENDER_PARAM_API_TYPE, MPV_RENDER_API_TYPE_OPENGL},
            {MPV_RENDER_PARAM_OPENGL_INIT_PARAMS, &gl}, {0, NULL}
        };
        int result = mpv_render_context_create(&p->renderer, p->core, params);
        if (result < 0) {
            note(p, mpv_error_string(result)); g_atomic_int_set(&p->failed, 1); return TRUE;
        }
        char *line = g_strdup_printf("GL %s / %s / scale=%d", glGetString(GL_VERSION),
                                    glGetString(GL_RENDERER), gtk_widget_get_scale_factor(GTK_WIDGET(area)));
        note(p, line); g_free(line);
        mpv_render_context_set_update_callback(p->renderer, frame_ready, p);
        request(p, "reload");
    }
    mpv_render_context_update(p->renderer);
    // Renderer initialization can change the bound FBO. Rebind GTK's target
    // instead of assuming it survived mpv's GL state changes.
    gtk_gl_area_attach_buffers(area);
    GLint framebuffer; glGetIntegerv(GL_DRAW_FRAMEBUFFER_BINDING, &framebuffer);
    int scale = gtk_widget_get_scale_factor(GTK_WIDGET(area));
    mpv_opengl_fbo fbo = {framebuffer, gtk_widget_get_allocated_width(GTK_WIDGET(area)) * scale,
                         gtk_widget_get_allocated_height(GTK_WIDGET(area)) * scale, 0};
    int flip = 1;
    mpv_render_param params[] = {{MPV_RENDER_PARAM_OPENGL_FBO, &fbo},
                                {MPV_RENDER_PARAM_FLIP_Y, &flip}, {0, NULL}};
    if (mpv_render_context_render(p->renderer, params) < 0) g_atomic_int_set(&p->failed, 1);
    if (!p->show_video) {
        // mpv can retain its last frame after stop. Explicitly blank GTK's
        // drawable after rendering, while still servicing mpv render updates.
        glBindFramebuffer(GL_FRAMEBUFFER, framebuffer);
        glDisable(GL_SCISSOR_TEST);
        glColorMask(GL_TRUE, GL_TRUE, GL_TRUE, GL_TRUE);
        glClearColor(0, 0, 0, 1);
        glClear(GL_COLOR_BUFFER_BIT);
        if (p->smoke) {
            unsigned char pixel[4];
            glReadPixels(fbo.w / 2, fbo.h / 2, 1, 1, GL_RGBA, GL_UNSIGNED_BYTE, pixel);
            if (glGetError() != GL_NO_ERROR || pixel[0] || pixel[1] || pixel[2] || pixel[3] != 255)
                g_atomic_int_set(&p->failed, 1);
        }
        g_atomic_int_set(&p->blank_generation, p->display_generation);
    }
    g_atomic_int_inc(&p->renders);
    return TRUE;
}

static gboolean close_probe(gpointer data) {
    Probe *p = data;
    if (g_atomic_int_exchange(&p->stopping, 1)) return G_SOURCE_REMOVE;
    if (p->renderer) {
        gtk_gl_area_make_current(GTK_GL_AREA(p->area));
        mpv_render_context_set_update_callback(p->renderer, NULL, NULL);
        mpv_render_context_free(p->renderer); p->renderer = NULL;
    }
    g_atomic_int_set(&p->render_gone, 1);
    return G_SOURCE_REMOVE;
}

static gboolean delete_event(GtkWidget *widget, GdkEvent *event, gpointer data) {
    (void)widget; (void)event; close_probe(data); return TRUE;
}

static gboolean tick(gpointer data) {
    Probe *p = data;
    GError *gl_error = gtk_gl_area_get_error(GTK_GL_AREA(p->area));
    if (gl_error && !g_atomic_int_get(&p->failed)) {
        note(p, gl_error->message); g_atomic_int_set(&p->failed, 1);
    }
    if (g_atomic_int_get(&p->failed) || g_atomic_int_get(&p->smoke_done)) close_probe(p);
    if (g_atomic_int_get(&p->done)) {
        close_probe(p); gtk_widget_destroy(p->window); gtk_main_quit(); return G_SOURCE_REMOVE;
    }
    if (g_atomic_int_get(&p->stopping)) return G_SOURCE_CONTINUE;
    if (g_atomic_int_exchange(&p->pending, 0) || (g_atomic_int_get(&p->ready) && !p->renderer))
        gtk_gl_area_queue_render(GTK_GL_AREA(p->area));
    State *s = g_async_queue_try_pop(p->states);
    if (s) {
        if (s->generation == g_atomic_int_get(&p->generation)) {
            gboolean show_video = !s->idle;
            if (show_video != p->show_video || s->generation != p->display_generation) {
                p->show_video = show_video;
                p->display_generation = s->generation;
                gtk_gl_area_queue_render(GTK_GL_AREA(p->area));
            }
            char number[G_ASCII_DTOSTR_BUF_SIZE]; g_ascii_dtostr(number, sizeof number, s->position);
            char *sub = g_uri_escape_string(s->subtitle ? s->subtitle : "", NULL, FALSE);
            char *decoder = g_uri_escape_string(s->decoder ? s->decoder : "", NULL, FALSE);
            char *js = g_strdup_printf("window.updateState?.({position:%s,pause:%s,subtitle:decodeURIComponent('%s'),decoder:decodeURIComponent('%s'),generation:%d})",
                                       number, s->pause ? "true" : "false", sub, decoder, s->generation);
            webkit_web_view_evaluate_javascript(p->web, js, -1, NULL, NULL, NULL, NULL, NULL);
            g_free(js); g_free(sub); g_free(decoder);
        }
        free_state(s);
    }
    return G_SOURCE_CONTINUE;
}

static void dialog_response(GtkDialog *dialog, int response, gpointer data) {
    (void)response; (void)data; gtk_widget_destroy(GTK_WIDGET(dialog));
}

// Observe routing without consuming input or recording text entered in the probe.
static gboolean input_event(GtkWidget *widget, GdkEvent *event, gpointer data) {
    (void)widget;
    const char *kind = event->type == GDK_BUTTON_PRESS ? "button-press" :
                       event->type == GDK_KEY_PRESS ? "key-press" : NULL;
    if (kind) printf("INPUT %s %s\n", (const char *)data, kind);
    return FALSE;
}

static void message(WebKitUserContentManager *manager, WebKitJavascriptResult *result, gpointer data) {
    (void)manager; Probe *p = data;
    JSCValue *value = webkit_javascript_result_get_js_value(result);
    if (!jsc_value_is_string(value)) return;
    char *text = jsc_value_to_string(value);
    printf("WEB MESSAGE received\n");
    if (!strcmp(text, "close")) close_probe(p);
    else if (!strcmp(text, "modal")) {
        GtkWidget *dialog = gtk_dialog_new_with_buttons("VSF native modal", GTK_WINDOW(p->window),
            GTK_DIALOG_MODAL | GTK_DIALOG_DESTROY_WITH_PARENT, "Dismiss", GTK_RESPONSE_CLOSE, NULL);
        gtk_container_add(GTK_CONTAINER(gtk_dialog_get_content_area(GTK_DIALOG(dialog))), gtk_entry_new());
        g_signal_connect(dialog, "response", G_CALLBACK(dialog_response), NULL); gtk_widget_show_all(dialog);
    } else request(p, text);
    g_free(text);
}

int main(int argc, char **argv) {
    gboolean smoke = argc == 5 && !strcmp(argv[4], "--smoke");
    if (argc != 4 && !smoke) { fprintf(stderr, "usage: %s media subtitles html [--smoke]\n", argv[0]); return 2; }
    argc = 4;
    char *html = NULL; GError *error = NULL;
    if (!g_file_get_contents(argv[3], &html, NULL, &error)) {
        fprintf(stderr, "%s\n", error->message); g_error_free(error); return 2;
    }
    setvbuf(stdout, NULL, _IOLBF, 0);
    gtk_init(&argc, &argv);
    GdkDisplay *display = gdk_display_get_default();
    printf("DISPLAY backend=%s name=%s\n", G_OBJECT_TYPE_NAME(display),
           gdk_display_get_name(display));
    Probe p = {.media = argv[1], .subtitles = argv[2], .smoke = smoke};
    p.requests = g_async_queue_new(); p.states = g_async_queue_new(); p.notes = g_async_queue_new_full(g_free);
    p.window = gtk_window_new(GTK_WINDOW_TOPLEVEL);
    gtk_window_set_title(GTK_WINDOW(p.window), "VSF Linux Composition Probe");
    gtk_window_set_default_size(GTK_WINDOW(p.window), 1000, 820);
    GtkWidget *panes = gtk_paned_new(GTK_ORIENTATION_VERTICAL);
    gtk_container_add(GTK_CONTAINER(p.window), panes);
    p.area = gtk_gl_area_new(); gtk_gl_area_set_required_version(GTK_GL_AREA(p.area), 3, 2);
    gtk_gl_area_set_auto_render(GTK_GL_AREA(p.area), FALSE);
    gtk_widget_set_size_request(p.area, 100, 100);
    p.web = WEBKIT_WEB_VIEW(webkit_web_view_new());
    g_signal_connect(p.window, "event", G_CALLBACK(input_event), "window");
    g_signal_connect(p.area, "event", G_CALLBACK(input_event), "video");
    g_signal_connect(p.web, "event", G_CALLBACK(input_event), "webview");
    WebKitUserContentManager *manager = webkit_web_view_get_user_content_manager(p.web);
    g_signal_connect(manager, "script-message-received::vsf", G_CALLBACK(message), &p);
    webkit_user_content_manager_register_script_message_handler(manager, "vsf");
    webkit_web_view_load_html(p.web, html, "about:blank"); g_free(html);
    gtk_paned_pack1(GTK_PANED(panes), p.area, TRUE, FALSE);
    gtk_paned_pack2(GTK_PANED(panes), GTK_WIDGET(p.web), TRUE, FALSE);
    gtk_paned_set_position(GTK_PANED(panes), 380);
    g_signal_connect(p.area, "render", G_CALLBACK(render), &p);
    g_signal_connect(p.window, "delete-event", G_CALLBACK(delete_event), &p);
    p.worker = g_thread_new("mpv-control", worker, &p);
    gtk_widget_show_all(p.window);
    g_timeout_add(16, tick, &p); g_timeout_add_seconds(300, close_probe, &p);
    gtk_main(); g_thread_join(p.worker); // Already finished: tick observed done.
    Request *r; while ((r = g_async_queue_try_pop(p.requests))) { g_free(r->text); g_free(r); }
    State *s; while ((s = g_async_queue_try_pop(p.states))) free_state(s);
    g_async_queue_unref(p.requests); g_async_queue_unref(p.states); g_async_queue_unref(p.notes);
    printf("STOPPED renders=%d failed=%d smoke_done=%d\n", g_atomic_int_get(&p.renders),
           g_atomic_int_get(&p.failed), g_atomic_int_get(&p.smoke_done));
    return g_atomic_int_get(&p.failed) || (p.smoke && !g_atomic_int_get(&p.smoke_done)) ? 1 : 0;
}
