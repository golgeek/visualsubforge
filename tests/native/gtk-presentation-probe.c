// Bounded GTK presentation isolation: no mpv, Tauri, decoder, audio or document.
// GTK main owns every widget/GL call. Optional WebKit tests overlay composition.
#include <gtk/gtk.h>
#include <webkit2/webkit2.h>
#include <epoxy/gl.h>
#include <stdio.h>
#include <string.h>

typedef struct {
    GtkWidget *window, *area;
    gint64 started, last_tick, max_tick_gap, max_render;
    unsigned renders, requests;
    gboolean failed, announced;
} Probe;

static gboolean draw(GtkGLArea *area, GdkGLContext *context, gpointer data) {
    (void)context;
    Probe *p = data;
    gint64 start = g_get_monotonic_time();
    if (!p->announced) {
        GtkAllocation allocation;
        gtk_widget_get_allocation(GTK_WIDGET(area), &allocation);
        printf("GL %s / %s scale=%d\n", glGetString(GL_VERSION), glGetString(GL_RENDERER),
               gtk_widget_get_scale_factor(GTK_WIDGET(area)));
        printf("LAYOUT window=%dx%d area=%d,%d,%d,%d mapped=%d\n",
               gtk_widget_get_allocated_width(p->window), gtk_widget_get_allocated_height(p->window),
               allocation.x, allocation.y, allocation.width, allocation.height,
               gtk_widget_get_mapped(GTK_WIDGET(area)));
        // Borrowed GDK objects, queried on GTK main after mapping. Record the
        // actual scale/work area: GDK_SCALE need not affect Wayland's scale.
        GdkDisplay *display = gtk_widget_get_display(p->window);
        GdkMonitor *monitor = gdk_display_get_monitor_at_window(
            display, gtk_widget_get_window(p->window));
        if (monitor) {
            GdkRectangle geometry, workarea;
            gdk_monitor_get_geometry(monitor, &geometry);
            gdk_monitor_get_workarea(monitor, &workarea);
            printf("MONITOR backend=%s geometry=%d,%d,%d,%d workarea=%d,%d,%d,%d "
                   "scale=%d window_size_fits=%d\n",
                   G_OBJECT_TYPE_NAME(display), geometry.x, geometry.y,
                   geometry.width, geometry.height, workarea.x, workarea.y,
                   workarea.width, workarea.height, gdk_monitor_get_scale_factor(monitor),
                   gtk_widget_get_allocated_width(p->window) <= workarea.width &&
                   gtk_widget_get_allocated_height(p->window) <= workarea.height);
        } else {
            printf("MONITOR unavailable\n");
        }
        p->announced = TRUE;
    }
    glClearColor((p->renders % 60) / 60.0f, .3f, .6f, 1);
    glClear(GL_COLOR_BUFFER_BIT);
    if (glGetError() != GL_NO_ERROR) p->failed = TRUE;
    p->renders++;
    p->max_render = MAX(p->max_render, g_get_monotonic_time() - start);
    return TRUE;
}

static gboolean tick(gpointer data) {
    Probe *p = data;
    gint64 now = g_get_monotonic_time();
    if (p->last_tick) p->max_tick_gap = MAX(p->max_tick_gap, now - p->last_tick);
    p->last_tick = now;
    if (now - p->started >= 8000000) {
        GError *error = gtk_gl_area_get_error(GTK_GL_AREA(p->area));
        if (error) { fprintf(stderr,"GL ERROR: %s\n",error->message); p->failed=TRUE; }
        printf("RESULT elapsed_us=%lld renders=%u requests=%u max_tick_gap_us=%lld max_render_us=%lld\n",
               (long long)(now-p->started),p->renders,p->requests,
               (long long)p->max_tick_gap,(long long)p->max_render);
        gtk_widget_destroy(p->window);
        gtk_main_quit();
        return G_SOURCE_REMOVE;
    }
    p->requests++;
    gtk_gl_area_queue_render(GTK_GL_AREA(p->area));
    return G_SOURCE_CONTINUE;
}

static gboolean position(GtkOverlay *overlay, GtkWidget *child, GdkRectangle *rect, gpointer data) {
    (void)child; (void)data;
    // Keep the control visible even when scaling reduces the logical desktop.
    // A fixed x=700 placed the original scale-2 control outside its allocation.
    int width = gtk_widget_get_allocated_width(GTK_WIDGET(overlay));
    int height = gtk_widget_get_allocated_height(GTK_WIDGET(overlay));
    *rect=(GdkRectangle){10,10,MAX(1,MIN(373,width-20)),MAX(1,MIN(209,height-20))};
    return TRUE;
}

int main(int argc, char **argv) {
    if (argc < 2 || argc > 4 || (strcmp(argv[1],"plain") && strcmp(argv[1],"web"))) return 2;
    if (argc >= 3 && strcmp(argv[2],"minimum")) return 2;
    if (argc == 4 && (strcmp(argv[3],"software-web") || strcmp(argv[1],"web"))) return 2;
    gboolean minimum=argc >= 3, software_web=argc == 4;
    gboolean web=!strcmp(argv[1],"web");
    gtk_init(&argc,&argv);
    Probe p={0};
    p.window=gtk_window_new(GTK_WINDOW_TOPLEVEL);
    gtk_window_set_title(GTK_WINDOW(p.window),"VSF GTK presentation isolation");
    gtk_window_set_default_size(GTK_WINDOW(p.window),1100,720);
    if (minimum) {
        GdkGeometry geometry={0};
        geometry.min_width=800; geometry.min_height=600;
        gtk_window_set_geometry_hints(GTK_WINDOW(p.window),NULL,&geometry,GDK_HINT_MIN_SIZE);
    }
    GtkWidget *overlay=gtk_overlay_new();
    gtk_container_add(GTK_CONTAINER(p.window),overlay);
    GtkWidget *content=web ? webkit_web_view_new() : gtk_label_new("GTK-only presentation control");
    gtk_container_add(GTK_CONTAINER(overlay),content);
    if (web) {
        WebKitSettings *settings=webkit_web_view_get_settings(WEBKIT_WEB_VIEW(content));
        if (software_web) webkit_settings_set_hardware_acceleration_policy(settings,WEBKIT_HARDWARE_ACCELERATION_POLICY_NEVER);
        printf("WEBKIT %u.%u.%u acceleration_policy=%d\n",webkit_get_major_version(),
               webkit_get_minor_version(),webkit_get_micro_version(),
               webkit_settings_get_hardware_acceleration_policy(settings));
    }
    if (web) webkit_web_view_load_html(WEBKIT_WEB_VIEW(content),"<body style='background:#17232d;color:white'><h1>Static WebKit control</h1><input value='No mpv or Tauri'></body>",NULL);
    p.area=gtk_gl_area_new();
    gtk_gl_area_set_required_version(GTK_GL_AREA(p.area),3,2);
    gtk_gl_area_set_auto_render(GTK_GL_AREA(p.area),FALSE);
    gtk_overlay_add_overlay(GTK_OVERLAY(overlay),p.area);
    gtk_overlay_set_overlay_pass_through(GTK_OVERLAY(overlay),p.area,TRUE);
    g_signal_connect(overlay,"get-child-position",G_CALLBACK(position),&p);
    g_signal_connect(p.area,"render",G_CALLBACK(draw),&p);
    gtk_widget_show_all(p.window);
    p.started=g_get_monotonic_time();
    g_timeout_add(16,tick,&p);
    gtk_main();
    return p.failed || !p.renders ? 1 : 0;
}
