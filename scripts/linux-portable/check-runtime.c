#include <dlfcn.h>
#include <stdio.h>

int main(int argc, char **argv) {
    if (argc < 2) {
        fputs("Missing runtime SONAME list\n", stderr);
        return 2;
    }
    int missing = 0;
    for (int i = 1; i < argc; ++i) {
        /* This short-lived helper leaves the GUI's loader namespace untouched. */
        if (!dlopen(argv[i], RTLD_NOW | RTLD_LOCAL)) {
            puts(argv[i]);
            missing = 1;
        }
    }
    return missing;
}
