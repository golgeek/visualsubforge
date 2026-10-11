// Render-worker-only EGL ownership. The main-thread Surface retains the layer
// until this object and the mpv render context have been destroyed.
#pragma once
#include <EGL/egl.h>
#include <EGL/eglext.h>
#include <GLES3/gl3.h>
#import <QuartzCore/CAMetalLayer.h>

class AngleContext {
    EGLDisplay display = EGL_NO_DISPLAY;
    EGLContext context = EGL_NO_CONTEXT;
    EGLSurface surface = EGL_NO_SURFACE;
    bool initialized = false;
public:
    AngleContext() = default;
    AngleContext(const AngleContext &) = delete;
    AngleContext &operator=(const AngleContext &) = delete;

    bool attach(CAMetalLayer *layer) {
        auto platform = reinterpret_cast<PFNEGLGETPLATFORMDISPLAYEXTPROC>(
            eglGetProcAddress("eglGetPlatformDisplayEXT"));
        if (!platform) return false;
        const EGLint attributes[] = {EGL_PLATFORM_ANGLE_TYPE_ANGLE,
            EGL_PLATFORM_ANGLE_TYPE_METAL_ANGLE, EGL_NONE};
        display = platform(EGL_PLATFORM_ANGLE_ANGLE, nullptr, attributes);
        if (display == EGL_NO_DISPLAY || !eglInitialize(display, nullptr, nullptr)) return false;
        initialized = true;
        if (!eglBindAPI(EGL_OPENGL_ES_API)) return false;
        const EGLint configAttributes[] = {EGL_SURFACE_TYPE, EGL_WINDOW_BIT,
            EGL_RENDERABLE_TYPE, EGL_OPENGL_ES3_BIT, EGL_RED_SIZE, 8,
            EGL_GREEN_SIZE, 8, EGL_BLUE_SIZE, 8, EGL_ALPHA_SIZE, 8, EGL_NONE};
        EGLConfig config;
        EGLint count = 0;
        if (!eglChooseConfig(display, configAttributes, &config, 1, &count) || count != 1) return false;
        const EGLint contextAttributes[] = {EGL_CONTEXT_CLIENT_VERSION, 3, EGL_NONE};
        context = eglCreateContext(display, config, EGL_NO_CONTEXT, contextAttributes);
        if (context == EGL_NO_CONTEXT) return false;
        surface = eglCreateWindowSurface(display, config, (__bridge EGLNativeWindowType)layer, nullptr);
        if (surface == EGL_NO_SURFACE || !eglMakeCurrent(display, surface, surface, context)) return false;
        return eglSwapInterval(display, 1) == EGL_TRUE;
    }
    static void *resolve(void *, const char *name) {
        return reinterpret_cast<void *>(eglGetProcAddress(name));
    }
    bool present() { return eglSwapBuffers(display, surface) == EGL_TRUE; }
    ~AngleContext() {
        if (!initialized) return;
        eglMakeCurrent(display, EGL_NO_SURFACE, EGL_NO_SURFACE, EGL_NO_CONTEXT);
        if (surface != EGL_NO_SURFACE) eglDestroySurface(display, surface);
        if (context != EGL_NO_CONTEXT) eglDestroyContext(display, context);
        eglTerminate(display);
    }
};
