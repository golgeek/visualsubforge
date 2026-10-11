/* Public libVLC 4 step/resume diagnostic. One control thread owns all handles;
 * callbacks publish atomic observations only. Plain/cycle never seek, so the
 * original core's paused-seek failure cannot confound the baseline comparison.
 * Restore is a separate diagnostic that seeks back after the completed cycle.
 * Queued requests the same seek and resumes immediately, without settling.
 * Use the binary-marker fixture and independently check every saved image.
 */
#include <vlc/vlc.h>
#include <limits.h>
#include <stdatomic.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

static _Atomic int state = libvlc_Stopped, next_status = INT_MIN, previous_status = INT_MIN;
static _Atomic unsigned presented_sequence = 0, seek_ends = 0;
static _Atomic int64_t presented_pts = -1;
static int64_t now_us(void) {
    struct timespec value;
    clock_gettime(CLOCK_MONOTONIC, &value);
    return (int64_t)value.tv_sec * 1000000 + value.tv_nsec / 1000;
}
static void sleep_ms(long ms) {
    struct timespec delay = {.tv_sec = ms / 1000, .tv_nsec = ms % 1000 * 1000000};
    while (nanosleep(&delay, &delay) != 0) {}
}
static void on_state(void *opaque, libvlc_state_t value) {
    (void)opaque;
    atomic_store(&state, value);
    printf("STATE value=%d now=%lld\n", value, (long long)now_us());
}
static void on_next(void *opaque, int status) {
    (void)opaque;
    atomic_store(&next_status, status);
    printf("NEXT status=%d\n", status);
}
static void on_previous(void *opaque, int status) {
    (void)opaque;
    atomic_store(&previous_status, status);
    printf("PREVIOUS status=%d\n", status);
}
static void on_update(void *opaque, const libvlc_media_player_time_point_t *point) {
    (void)opaque;
    if (point->system_date_us == INT64_MAX) {
        atomic_store(&presented_pts, point->ts_us);
        atomic_fetch_add(&presented_sequence, 1);
        printf("PAUSED_POINT pts=%lld\n", (long long)point->ts_us);
    }
}
static void on_pause(void *opaque, libvlc_time_t date) { (void)opaque; (void)date; }
static void on_seek(void *opaque, const libvlc_media_player_time_point_t *point) {
    (void)opaque;
    if (!point) atomic_fetch_add(&seek_ends, 1);
    printf("SEEK active=%d\n", point != NULL);
}
static bool wait_state(int expected) {
    const int64_t deadline = now_us() + 5000000;
    while (atomic_load(&state) != expected && now_us() < deadline) sleep_ms(10);
    return atomic_load(&state) == expected;
}
static bool step(libvlc_media_player_t *player, bool forward) {
    _Atomic int *status = forward ? &next_status : &previous_status;
    atomic_store(status, INT_MIN);
    const unsigned sequence = atomic_load(&presented_sequence);
    if (forward) libvlc_media_player_next_frame(player);
    else libvlc_media_player_previous_frame(player);
    const int64_t deadline = now_us() + 5000000;
    while (now_us() < deadline &&
           (atomic_load(status) == INT_MIN || atomic_load(&presented_sequence) == sequence))
        sleep_ms(10);
    const bool passed = atomic_load(status) == 0 &&
                        atomic_load(&presented_sequence) > sequence &&
                        atomic_load(&state) == libvlc_Paused;
    printf("STEP forward=%d status=%d point=%lld delivered=%d passed=%d\n", forward,
           atomic_load(status), (long long)atomic_load(&presented_pts),
           atomic_load(&presented_sequence) > sequence, passed);
    return passed; // The image oracle independently checks which frame arrived.
}
static bool snapshot(libvlc_media_player_t *player, const char *directory, const char *name) {
    char path[4096];
    int count = snprintf(path, sizeof(path), "%s/%s.png", directory, name);
    if (count < 0 || (size_t)count >= sizeof(path)) return false;
    int result = libvlc_video_take_snapshot(player, 0, path, 0, 0);
    printf("SNAPSHOT name=%s result=%d time=%lld\n", name, result,
           (long long)libvlc_media_player_get_time(player));
    return result == 0;
}
int main(int argc, char **argv) {
    if (argc != 4 || (strcmp(argv[3], "plain") && strcmp(argv[3], "cycle") &&
                      strcmp(argv[3], "restore") && strcmp(argv[3], "queued"))) {
        fprintf(stderr, "usage: probe VIDEO EXISTING_OUTPUT_DIR plain|cycle|restore|queued\n"); return 2;
    }
    const bool cycle = strcmp(argv[3], "plain") != 0;
    const bool restore = strcmp(argv[3], "restore") == 0;
    const bool queued = strcmp(argv[3], "queued") == 0;
    setvbuf(stdout, NULL, _IOLBF, 0);
    const char *args[] = {"--ignore-config", "--verbose=2", "--no-video-title-show",
                          "--codec=ffmpeg,none", "--vout=dummy"};
    const struct libvlc_media_player_cbs callbacks = {
        .version = 0, .on_state_changed = on_state,
        .on_next_frame_status = on_next, .on_prev_frame_status = on_previous
    };
    const struct libvlc_media_player_watch_time_cbs time_callbacks = {
        .version = 0, .on_update = on_update, .on_paused = on_pause, .on_seek = on_seek
    };
    libvlc_instance_t *instance = libvlc_new(sizeof(args) / sizeof(args[0]), args);
    if (!instance) return 2;
    libvlc_media_player_t *player = libvlc_media_player_new(instance, &callbacks, NULL);
    libvlc_media_t *media = libvlc_media_new_path(argv[1]);
    bool failed = !player || !media, watching = false;
    if (failed) goto cleanup;
    watching = libvlc_media_player_watch_time(player, 10000, &time_callbacks, NULL) == 0;
    if (!watching) { failed = true; goto cleanup; }
    libvlc_media_player_set_media(player, media);
    if (libvlc_media_player_play(player)) { failed = true; goto cleanup; }
    const int64_t deadline = now_us() + 10000000;
    while (libvlc_media_player_get_time(player) < 1500000 && now_us() < deadline) sleep_ms(10);
    if (libvlc_media_player_get_time(player) < 1500000) { failed = true; goto cleanup; }
    libvlc_media_player_set_pause(player, true);
    if (!wait_state(libvlc_Paused)) { failed = true; goto cleanup; }
    sleep_ms(100);
    failed |= !snapshot(player, argv[2], "initial");
    if (cycle) {
        if (!step(player, true)) { failed = true; goto cleanup; }
        failed |= !snapshot(player, argv[2], "next");
        if (!step(player, false)) { failed = true; goto cleanup; }
        failed |= !snapshot(player, argv[2], "previous");
    }
    const libvlc_time_t before = libvlc_media_player_get_time(player);
    if (restore) {
        const unsigned sequence = atomic_load(&seek_ends);
        if (libvlc_media_player_set_time(player, before, false) != 0) {
            failed = true; goto cleanup;
        }
        const int64_t seek_deadline = now_us() + 5000000;
        while (atomic_load(&seek_ends) == sequence && now_us() < seek_deadline) sleep_ms(10);
        const bool completed = atomic_load(&seek_ends) > sequence;
        // Seek-end acknowledges processing, not picture delivery. The saved
        // image is checked independently after this bounded settling interval.
        sleep_ms(100);
        failed |= !snapshot(player, argv[2], "restored");
        const libvlc_time_t restored = libvlc_media_player_get_time(player);
        const bool passed = completed && llabs(restored - before) <= 2;
        printf("RESTORE_RESULT before=%lld after=%lld completed=%d passed=%d\n",
               (long long)before, (long long)restored, completed, passed);
        failed |= !passed;
        if (!completed) goto cleanup;
    }
    const unsigned queued_seek_before = atomic_load(&seek_ends);
    const int64_t start = now_us();
    if (queued) {
        const int result = libvlc_media_player_set_time(player, before, false);
        printf("QUEUED_RESTORE target=%lld result=%d\n", (long long)before, result);
        if (result != 0) { failed = true; goto cleanup; }
    }
    libvlc_media_player_set_pause(player, false);
    const bool playing = wait_state(libvlc_Playing);
    sleep_ms(200);
    libvlc_media_player_set_pause(player, true);
    const bool paused = wait_state(libvlc_Paused);
    const int64_t elapsed = now_us() - start;
    const libvlc_time_t after = libvlc_media_player_get_time(player);
    if (queued) {
        const unsigned completed = atomic_load(&seek_ends) - queued_seek_before;
        printf("QUEUED_RESTORE_DONE count=%u\n", completed);
        failed |= completed == 0;
    }
    const bool passed = playing && paused && before >= 0 && after > before &&
                        after - before <= elapsed + 250000;
    printf("RESUME_OBSERVATION cycle=%d before=%lld after=%lld elapsed_us=%lld paused=%d passed=%d\n",
           cycle, (long long)before, (long long)after, (long long)elapsed, paused, passed);
    failed |= !passed;
    failed |= !snapshot(player, argv[2], "resumed");
cleanup:
    if (player) {
        if (libvlc_media_player_stop_async(player) == 0 && !wait_state(libvlc_Stopped)) failed = true;
        if (watching) libvlc_media_player_unwatch_time(player);
        libvlc_media_player_release(player);
    }
    if (media) libvlc_media_release(media);
    libvlc_release(instance);
    printf("CORE RELEASED failed=%d\n", failed);
    return failed ? 1 : 0;
}
