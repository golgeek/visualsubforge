// Test-only DYLD interposition: native calls are forwarded except the requested
// initialization fault. Counters are atomic because native callbacks may use
// other threads. Nothing here is linked into the application or libmpv.
#include <AudioUnit/AudioUnit.h>
#include <CoreAudio/CoreAudio.h>
#include <dlfcn.h>
#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
static atomic_int created, disposed, failures, listeners_added,
    listeners_removed, dispose_errors;
static OSStatus trace_new(AudioComponent component,
                          AudioComponentInstance *unit)
{
    OSStatus r = AudioComponentInstanceNew(component, unit);
    if (r == noErr)
        atomic_fetch_add(&created, 1);
    return r;
}
static OSStatus trace_dispose(AudioComponentInstance unit)
{
    OSStatus r = AudioComponentInstanceDispose(unit);
    if (r == noErr)
        atomic_fetch_add(&disposed, 1);
    else {
        atomic_fetch_add(&dispose_errors, 1);
        fprintf(stderr, "DISPOSE_ERROR status=%d\n", (int)r);
    }
    return r;
}
static OSStatus trace_init(AudioUnit unit)
{
    const char *failure = getenv("VSF_AUDIO_FAILURE");
    if (failure && strcmp(failure, "failure") == 0) {
        atomic_fetch_add(&failures, 1);
        return kAudio_ParamError;
    }
    return AudioUnitInitialize(unit);
}
static OSStatus trace_property(AudioUnit unit, AudioUnitPropertyID property,
                               AudioUnitScope scope, AudioUnitElement element,
                               const void *data, UInt32 size)
{
    const char *failure = getenv("VSF_AUDIO_FAILURE");
    if (failure && strcmp(failure, "format-failure") == 0 &&
        property == kAudioUnitProperty_StreamFormat &&
        scope == kAudioUnitScope_Input) {
        atomic_fetch_add(&failures, 1);
        return kAudio_ParamError;
    }
    return AudioUnitSetProperty(unit, property, scope, element, data, size);
}
static int from_mpv(AudioObjectPropertyListenerProc callback)
{
    Dl_info info;
    return dladdr((const void *)callback, &info) && info.dli_fname &&
           strstr(info.dli_fname, "libmpv");
}
static OSStatus trace_add(AudioObjectID object,
                          const AudioObjectPropertyAddress *address,
                          AudioObjectPropertyListenerProc callback,
                          void *context)
{
    OSStatus r =
        AudioObjectAddPropertyListener(object, address, callback, context);
    if (r == noErr && from_mpv(callback))
        atomic_fetch_add(&listeners_added, 1);
    return r;
}
static OSStatus trace_remove(AudioObjectID object,
                             const AudioObjectPropertyAddress *address,
                             AudioObjectPropertyListenerProc callback,
                             void *context)
{
    OSStatus r =
        AudioObjectRemovePropertyListener(object, address, callback, context);
    if (r == noErr && from_mpv(callback))
        atomic_fetch_add(&listeners_removed, 1);
    return r;
}
#define INTERPOSE(replacement, original)                                       \
    __attribute__((used)) static struct {                                      \
        const void *replacement;                                               \
        const void *original;                                                  \
    } ip_##original __attribute__((section("__DATA,__interpose"))) = {         \
        (const void *)&replacement, (const void *)&original}
INTERPOSE(trace_new, AudioComponentInstanceNew);
INTERPOSE(trace_dispose, AudioComponentInstanceDispose);
INTERPOSE(trace_init, AudioUnitInitialize);
INTERPOSE(trace_property, AudioUnitSetProperty);
INTERPOSE(trace_add, AudioObjectAddPropertyListener);
INTERPOSE(trace_remove, AudioObjectRemovePropertyListener);
__attribute__((destructor)) static void report(void)
{
    fprintf(stderr, "DISPOSE_ERRORS count=%d\n", atomic_load(&dispose_errors));
    fprintf(stderr, "LISTENER_COUNTS added=%d removed=%d\n",
            atomic_load(&listeners_added), atomic_load(&listeners_removed));
    fprintf(stderr, "UNIT_COUNTS created=%d disposed=%d failures=%d\n",
            atomic_load(&created), atomic_load(&disposed),
            atomic_load(&failures));
}
