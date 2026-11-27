// Exercise source ownership without a decoder. Run on a GTK desktop, with
// VSF_RENDER_TRACE unset. The real WebKit child matches the Tauri topology.
#include <webkit2/webkit2.h>
#include "../../crates/media-player/native/player_linux.cpp"
#include <cassert>

int main(int argc, char **argv) {
    gtk_init(&argc, &argv);
    GtkWidget *window = gtk_window_new(GTK_WINDOW_TOPLEVEL);
    GtkWidget *box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);
    gtk_container_add(GTK_CONTAINER(window), box);
    gtk_container_add(GTK_CONTAINER(box), webkit_web_view_new());
    gtk_widget_show_all(window);
    void *surface = vsf_surface_create(box);
    assert(surface);
    GMainContext *context = g_main_context_default();
    auto pending = [&] { return g_main_context_find_source_by_user_data(context, surface); };
    assert(!pending()); // No timer follows the lifetime of the window.
    vsf_surface_layout(surface, 0, 0, 100, 100, 1);
    auto *source = pending();
    assert(source);
    assert(g_source_get_priority(source) == G_PRIORITY_HIGH_IDLE);
    vsf_surface_layout(surface, 0, 0, 200, 200, 1);
    assert(pending() == source);
    while (pending()) g_main_context_iteration(context, TRUE);
    vsf_surface_layout(surface, 0, 0, 200, 200, 0);
    assert(pending());
    vsf_surface_release(surface);
    assert(!pending());
    gtk_widget_destroy(window);
}
