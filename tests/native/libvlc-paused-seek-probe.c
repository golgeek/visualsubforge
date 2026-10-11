/* Public libVLC 4 paused-seek reproducer, without AppKit or a visible renderer.
 * One thread owns the player; callbacks only log/copy data. No media calls are
 * made from callbacks. Dummy output is diagnostic, never a rendering candidate.
 * Build against the matching pinned SDK recorded in docs/libvlc-feasibility.md.
 */
#include <vlc/vlc.h>
#include <stdatomic.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

static _Atomic int state = libvlc_Stopped;
static void on_state(void *opaque, libvlc_state_t value) {
    (void)opaque;
    atomic_store(&state, value);
}
static void on_next(void *opaque, int status) {
    (void)opaque;
    printf("NEXT_FRAME status=%d\n", status);
}
static void on_update(void *opaque, const libvlc_media_player_time_point_t *point) {
    (void)opaque;
    printf("CLOCK_UPDATE ts=%lld date=%lld\n",
           (long long)point->ts_us, (long long)point->system_date_us);
}
static void on_pause(void *opaque, libvlc_time_t date) {
    (void)opaque;
    printf("CLOCK_PAUSE date=%lld\n", (long long)date);
}
static void on_seek(void *opaque, const libvlc_media_player_time_point_t *point) {
    (void)opaque;
    printf("CLOCK_SEEK active=%d target=%lld\n", point != NULL,
           point ? (long long)point->ts_us : -1LL);
}
static int64_t now_us(void) {
    struct timespec now;
    clock_gettime(CLOCK_MONOTONIC, &now);
    return (int64_t)now.tv_sec * 1000000 + now.tv_nsec / 1000;
}
static void sleep_ms(long milliseconds) {
    struct timespec delay = {.tv_sec = milliseconds / 1000,
                            .tv_nsec = milliseconds % 1000 * 1000000};
    while (nanosleep(&delay, &delay) != 0) {}
}
static bool wait_state(int expected) {
    const int64_t deadline = now_us() + 5000000;
    while (atomic_load(&state) != expected && now_us() < deadline) sleep_ms(10);
    return atomic_load(&state) == expected;
}
static bool snapshot(libvlc_media_player_t *player, const char *directory, unsigned index) {
    if (!directory) return true;
    char path[4096];
    const int count = snprintf(path, sizeof(path), "%s/frame-%u.png", directory, index);
    if (count < 0 || (size_t)count >= sizeof(path)) return false;
    const int result = libvlc_video_take_snapshot(player, 0, path, 0, 0);
    printf("SNAPSHOT index=%u result=%d time=%lld\n", index, result,
           (long long)libvlc_media_player_get_time(player));
    return result == 0; // The runner separately verifies image files and contents.
}
int main(int argc, char **argv) {
    if (argc < 2 || argc > 4 || (argc >= 3 && strcmp(argv[2], "step-after-seek") != 0 &&
        strcmp(argv[2], "resume-after-seek") != 0 && strcmp(argv[2], "plain") != 0)) {
        fprintf(stderr, "usage: probe VIDEO [plain|step-after-seek|resume-after-seek] [SNAPSHOT_DIR]\n"); return 2;
    }
    const bool step_after_seek = argc >= 3 && strcmp(argv[2], "step-after-seek") == 0;
    const bool resume_after_seek = argc >= 3 && strcmp(argv[2], "resume-after-seek") == 0;
    const char *snapshot_dir = argc == 4 ? argv[3] : NULL;
    setvbuf(stdout, NULL, _IOLBF, 0);
    printf("VERSION %s\n", libvlc_get_version());
    const char *args[] = {"--ignore-config", "--verbose=2", "--no-video-title-show",
                          "--codec=ffmpeg,none", "--vout=dummy"};
    const struct libvlc_media_player_cbs callbacks = {
        .version = 0, .on_state_changed = on_state, .on_next_frame_status = on_next
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
    if (libvlc_media_player_play(player) != 0) { failed = true; goto cleanup; }
    const int64_t deadline = now_us() + 10000000;
    while (libvlc_media_player_get_time(player) < 200000 && now_us() < deadline) sleep_ms(10);
    if (libvlc_media_player_get_time(player) < 200000) { failed = true; goto cleanup; }
    libvlc_media_player_set_pause(player, true);
    if (!wait_state(libvlc_Paused)) { failed = true; goto cleanup; }
    failed |= !snapshot(player, snapshot_dir, 0);
    const libvlc_time_t targets[] = {1500000, 500000, 2500000};
    for (unsigned i = 0; i < sizeof(targets) / sizeof(targets[0]); ++i) {
        sleep_ms(750);
        const libvlc_time_t target = targets[i];
        const int result = libvlc_media_player_set_time(player, target, false);
        // Diagnostic only: tests whether an explicit output advance unblocks
        // the paused seek. It may advance past the target and is not a fix.
        if (step_after_seek) libvlc_media_player_next_frame(player);
        unsigned settled = 0;
        libvlc_time_t time = -1;
        for (unsigned sample = 0; sample < 100; ++sample) {
            time = libvlc_media_player_get_time(player);
            const bool near = llabs(time - target) <= 40000;
            settled = near && atomic_load(&state) == libvlc_Paused ? settled + 1 : 0;
            printf("SEEK_SAMPLE target=%lld sample=%u time=%lld state=%d\n",
                   (long long)target, sample, (long long)time, atomic_load(&state));
            sleep_ms(20);
        }
        const bool passed = result == 0 && settled >= 10;
        printf("SEEK_RESULT target=%lld after=%lld settled=%u passed=%d\n",
               (long long)target, (long long)time, settled, passed);
        failed |= !passed;
        failed |= !snapshot(player, snapshot_dir, i + 1);
        if (resume_after_seek) {
            // A transport/liveness check, not a device clock or audio oracle.
            const libvlc_time_t before = libvlc_media_player_get_time(player);
            const int64_t resume_start = now_us();
            libvlc_media_player_set_pause(player, false);
            const bool playing = wait_state(libvlc_Playing);
            const int64_t resume_deadline = now_us() + 2000000;
            libvlc_time_t after = before;
            while (playing && after < before + 200000 && now_us() < resume_deadline) {
                sleep_ms(20);
                after = libvlc_media_player_get_time(player);
            }
            libvlc_media_player_set_pause(player, true);
            const bool paused = wait_state(libvlc_Paused);
            const int64_t resume_elapsed = now_us() - resume_start;
            // Reject gross clock jumps. This generous 250 ms allowance is a
            // failure detector, not an accepted editing precision budget.
            const bool resumed = playing && paused && before >= 0 &&
                                 after >= before + 200000 && after <= before + 1500000 &&
                                 after - before <= resume_elapsed + 250000;
            printf("RESUME_RESULT before=%lld after=%lld elapsed_us=%lld paused=%d passed=%d\n",
                   (long long)before, (long long)after, (long long)resume_elapsed, paused, resumed);
            failed |= !resumed;
        }
    }
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
