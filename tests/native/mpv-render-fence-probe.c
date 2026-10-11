// Linux render-API resource regression; no window, media, GTK or Tauri.
// One thread owns the current EGL context and mpv renderer. Renderer/core
// destruction precedes EGL destruction. The process watchdog belongs to caller.
#include <epoxy/egl.h>
#include <epoxy/gl.h>
#include <mpv/client.h>
#include <mpv/render_gl.h>
#include <dirent.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

static void *get_proc(void *unused, const char *name)
{
    (void)unused;
    return (void *)eglGetProcAddress(name);
}

static int sync_file_count(void)
{
    DIR *directory = opendir("/proc/self/fd");
    if (!directory)
        return -1;
    int count = 0;
    struct dirent *entry;
    while ((entry = readdir(directory))) {
        if (entry->d_name[0] == '.')
            continue;
        char path[512], target[512];
        snprintf(path, sizeof(path), "/proc/self/fd/%s", entry->d_name);
        ssize_t length = readlink(path, target, sizeof(target) - 1);
        if (length < 0)
            continue; // A background thread can close a descriptor while read.
        target[length] = '\0';
        count += strcmp(target, "anon_inode:sync_file") == 0;
    }
    closedir(directory);
    return count;
}

int main(void)
{
    int result = 2; // Setup/driver errors are distinct from detected growth.
    EGLDisplay display = eglGetDisplay(EGL_DEFAULT_DISPLAY);
    EGLContext context = EGL_NO_CONTEXT;
    EGLSurface surface = EGL_NO_SURFACE;
    mpv_handle *core = NULL;
    mpv_render_context *renderer = NULL;
    EGLint major, minor, configs;
    EGLConfig config;
    const EGLint attributes[] = {
        EGL_SURFACE_TYPE, EGL_PBUFFER_BIT,
        EGL_RENDERABLE_TYPE, EGL_OPENGL_BIT,
        EGL_RED_SIZE, 8, EGL_GREEN_SIZE, 8, EGL_BLUE_SIZE, 8, EGL_NONE
    };
    const EGLint dimensions[] = {EGL_WIDTH, 160, EGL_HEIGHT, 90, EGL_NONE};
    if (!eglInitialize(display, &major, &minor))
        return 2;
    if (!eglBindAPI(EGL_OPENGL_API) ||
        !eglChooseConfig(display, attributes, &config, 1, &configs) || configs != 1)
        goto cleanup;
    surface = eglCreatePbufferSurface(display, config, dimensions);
    context = eglCreateContext(display, config, EGL_NO_CONTEXT, NULL);
    if (surface == EGL_NO_SURFACE || context == EGL_NO_CONTEXT ||
        !eglMakeCurrent(display, surface, surface, context))
        goto cleanup;
    printf("GL %s / %s\n", glGetString(GL_VERSION), glGetString(GL_RENDERER));
    core = mpv_create();
    if (!core || mpv_set_option_string(core, "config", "no") < 0 ||
        mpv_set_option_string(core, "vo", "libmpv") < 0 || mpv_initialize(core) < 0)
        goto cleanup;
    mpv_opengl_init_params init = {get_proc, NULL};
    mpv_render_param options[] = {
        {MPV_RENDER_PARAM_API_TYPE, MPV_RENDER_API_TYPE_OPENGL},
        {MPV_RENDER_PARAM_OPENGL_INIT_PARAMS, &init},
        {MPV_RENDER_PARAM_INVALID, NULL}
    };
    if (mpv_render_context_create(&renderer, core, options) < 0)
        goto cleanup;
    mpv_opengl_fbo fbo = {0, 160, 90, 0};
    int block = 0;
    mpv_render_param params[] = {
        {MPV_RENDER_PARAM_OPENGL_FBO, &fbo},
        {MPV_RENDER_PARAM_BLOCK_FOR_TARGET_TIME, &block},
        {MPV_RENDER_PARAM_INVALID, NULL}
    };
    int baseline = sync_file_count();
    if (baseline < 0)
        goto cleanup;
    printf("BASELINE sync_files=%d\n", baseline);
    for (int frame = 1; frame <= 2400; ++frame) {
        if (mpv_render_context_render(renderer, params) < 0)
            goto cleanup;
        if (frame % 100 != 0)
            continue;
        int count = sync_file_count();
        if (count < 0)
            goto cleanup;
        printf("SAMPLE frame=%d sync_files=%d\n", frame, count);
        fflush(stdout);
        // Allow a small bounded driver queue. Stop far below the default FD
        // limit if the historical one-fence-per-call regression returns.
        if (count > baseline + 16) {
            fprintf(stderr, "FAIL: GPU synchronization descriptors accumulate\n");
            result = 1;
            goto cleanup;
        }
    }
    puts("PASS: no accumulating sync_file descriptors over 2400 render calls");
    result = 0;
cleanup:
    if (renderer)
        mpv_render_context_free(renderer);
    if (core)
        mpv_terminate_destroy(core);
    eglMakeCurrent(display, EGL_NO_SURFACE, EGL_NO_SURFACE, EGL_NO_CONTEXT);
    if (context != EGL_NO_CONTEXT)
        eglDestroyContext(display, context);
    if (surface != EGL_NO_SURFACE)
        eglDestroySurface(display, surface);
    eglTerminate(display);
    if (result == 2)
        fprintf(stderr, "ERROR: could not complete EGL/libmpv probe\n");
    return result;
}
