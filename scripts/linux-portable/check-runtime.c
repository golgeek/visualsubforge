#include <dlfcn.h>
#include <stdio.h>

int main(void) {
    static const char *const libraries[] = {
        "libwebkit2gtk-4.1.so.0", "libgtk-3.so.0", "libva.so.2",
        "libva-drm.so.2", "libva-x11.so.2", NULL
    };
    int failed = 0;
    for (const char *const *name = libraries; *name; ++name) {
        void *handle = dlopen(*name, RTLD_NOW | RTLD_LOCAL);
        if (!handle) {
            fprintf(stderr, "VisualSubForge requires %s (%s)\n", *name, dlerror());
            failed = 1;
        }
        /* The process exits immediately. Keep GTK and its dependencies loaded
           while testing the remaining libraries. */
    }
    return failed;
}
