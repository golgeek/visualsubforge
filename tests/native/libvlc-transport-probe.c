/* Isolated libVLC 4 public-API transport/capture diagnostic. Not application code.
 * One control thread owns handles and writes logs. Native callbacks only copy
 * clock snapshots or publish state. Callback storage outlives unwatch/release.
 * Times passed to libVLC 4 are microseconds. Requires the matching SDK headers. */
#define _POSIX_C_SOURCE 200809L
#include <vlc/vlc.h>
#include <inttypes.h>
#include <pthread.h>
#include <stdatomic.h>
#include <stdio.h>
#include <string.h>
#include <time.h>

struct clock_state {
    pthread_mutex_t mutex;
    libvlc_media_player_time_point_t point;
    bool valid, seeking;
    libvlc_time_t pause_date;
};
static void on_update(void *opaque, const libvlc_media_player_time_point_t *point) {
    struct clock_state *clock = opaque;
    pthread_mutex_lock(&clock->mutex);
    clock->point = *point;
    clock->valid = true;
    clock->pause_date = 0;
    pthread_mutex_unlock(&clock->mutex);
}
static void on_pause(void *opaque, libvlc_time_t date) {
    struct clock_state *clock = opaque;
    pthread_mutex_lock(&clock->mutex);
    clock->pause_date = date;
    if (date <= 0) clock->valid = false;
    pthread_mutex_unlock(&clock->mutex);
}
static void on_seek(void *opaque, const libvlc_media_player_time_point_t *point) {
    struct clock_state *clock = opaque;
    pthread_mutex_lock(&clock->mutex);
    clock->seeking = point != NULL;
    /* Wait for a measured post-seek point rather than extrapolating the old one. */
    clock->valid = false;
    pthread_mutex_unlock(&clock->mutex);
}
static libvlc_time_t read_clock(struct clock_state *clock) {
    pthread_mutex_lock(&clock->mutex);
    const libvlc_media_player_time_point_t point = clock->point;
    const bool valid = clock->valid && !clock->seeking;
    const libvlc_time_t paused = clock->pause_date;
    pthread_mutex_unlock(&clock->mutex);
    libvlc_time_t now; double position;
    if (!valid || libvlc_media_player_time_point_interpolate(
            &point, paused > 0 ? paused : libvlc_clock(), &now, &position) != 0)
        return -1;
    return now;
}
static void on_state(void *opaque, libvlc_state_t state) {
    atomic_store((_Atomic int *)opaque, state);
}
static int64_t monotonic_us(void) {
    struct timespec now;
    clock_gettime(CLOCK_MONOTONIC, &now);
    return (int64_t)now.tv_sec * 1000000 + now.tv_nsec / 1000;
}
static void tick(void) {
    const struct timespec delay = {.tv_nsec = 10000000};
    nanosleep(&delay, NULL);
}
static bool wait_state(_Atomic int *state, int wanted) {
    const int64_t deadline = monotonic_us() + 5000000;
    while (monotonic_us() < deadline) {
        if (atomic_load(state) == wanted) return true;
        tick();
    }
    return false;
}
int main(int argc, char **argv) {
    if ((argc != 3 && argc != 4) || (strcmp(argv[2], "ordinary") && strcmp(argv[2], "loop")) ||
        (argc == 4 && strcmp(argv[3], "cache100"))) {
        fprintf(stderr, "usage: libvlc-transport-probe FOUR_SECOND_WAV ordinary|loop [cache100]\n");
        return 2;
    }
    const bool loop = !strcmp(argv[2], "loop");
    setvbuf(stdout, NULL, _IOLBF, 0);
    printf("VERSION %s\n", libvlc_get_version());
    const char *options[] = {"--ignore-config", "--no-video", "--no-video-title-show", "--file-caching=100"};
    printf("CACHE %s\n", argc == 4 ? "100ms" : "default");
    libvlc_instance_t *instance = libvlc_new(argc == 4 ? 4 : 3, options);
    if (!instance) return 3;
    _Atomic int state = libvlc_Stopped;
    const struct libvlc_media_player_cbs callbacks = {.version = 0, .on_state_changed = on_state};
    struct clock_state clock = {.mutex = PTHREAD_MUTEX_INITIALIZER};
    const struct libvlc_media_player_watch_time_cbs time_callbacks = {
        .version = 0, .on_update = on_update, .on_paused = on_pause, .on_seek = on_seek
    };
    libvlc_media_player_t *player = libvlc_media_player_new(instance, &callbacks, &state);
    libvlc_media_t *media = libvlc_media_new_path(argv[1]);
    bool watching = false, failed = !player || !media;
    if (failed) goto cleanup;
    if (libvlc_media_player_watch_time(player, 10000, &time_callbacks, &clock)) {
        failed = true; goto cleanup;
    }
    watching = true;
    libvlc_media_player_set_media(player, media);
    if (libvlc_media_player_play(player) || !wait_state(&state, libvlc_Playing)) {
        failed = true; goto cleanup;
    }
    /* Warm the native audio output before a process tap attaches. */
    const int64_t deadline = monotonic_us() + 5000000;
    libvlc_time_t ready_time = -1;
    while (monotonic_us() < deadline) {
        ready_time = read_clock(&clock);
        if (ready_time >= 300000 && ready_time <= 1500000) break;
        tick();
    }
    if (ready_time < 300000 || ready_time > 1500000) { failed = true; goto cleanup; }
    libvlc_media_player_set_pause(player, 1);
    if (!wait_state(&state, libvlc_Paused)) { failed = true; goto cleanup; }
    printf("READY time=%"PRId64" length=%"PRId64"\n", ready_time, libvlc_media_player_get_length(player));
    if (getchar() != 'r') { failed = true; goto cleanup; }
    const int64_t began = monotonic_us();
    if (loop) {
        const int result = libvlc_media_player_set_abloop_time(player, 750000, 1073000);
        libvlc_time_t a = -1, b = -1; double ap = -1, bp = -1;
        const int enabled = libvlc_media_player_get_abloop(player, &a, &ap, &b, &bp);
        printf("LOOP result=%d state=%d a=%"PRId64" b=%"PRId64"\n", result, enabled, a, b);
        failed |= result != 0 || enabled != libvlc_abloop_b;
        /* Endpoint round-trip accuracy is assessed separately; retain raw values. */
    }
    const int seek = libvlc_media_player_set_time(player, 0, false);
    printf("SEEK result=%d\n", seek);
    failed |= seek != 0;
    libvlc_media_player_set_pause(player, 0);
    bool cleared = false;
    while (monotonic_us() - began < 6500000) {
        const int64_t elapsed = monotonic_us() - began;
        if (loop && !cleared && elapsed >= 5000000) {
            const int reset = libvlc_media_player_reset_abloop(player);
            printf("RESET elapsed=%"PRId64" result=%d\n", elapsed, reset);
            failed |= reset != 0;
            cleared = true;
        }
        printf("TIME elapsed=%"PRId64" mono=%"PRId64" watched=%"PRId64" coarse=%"PRId64" state=%d\n",
            elapsed, monotonic_us(), read_clock(&clock), libvlc_media_player_get_time(player), atomic_load(&state));
        tick();
    }
cleanup:
    if (player) {
        if (libvlc_media_player_stop_async(player) == 0)
            failed |= !wait_state(&state, libvlc_Stopped);
        if (watching) libvlc_media_player_unwatch_time(player);
        libvlc_media_player_release(player);
    }
    if (media) libvlc_media_release(media);
    libvlc_release(instance);
    pthread_mutex_destroy(&clock.mutex);
    printf("CLEANUP completed failed=%d\n", failed);
    return failed ? 1 : 0;
}
