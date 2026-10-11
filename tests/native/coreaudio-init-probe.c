// One thread owns each libmpv core, opening a mono source while paused. Every
// iteration destroys the core before the next. The caller bounds the process;
// the native destructor itself cannot be forcibly interrupted by this harness.
#include <mpv/client.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
static double now(void)
{
    struct timespec t;
    clock_gettime(CLOCK_MONOTONIC, &t);
    return t.tv_sec + t.tv_nsec / 1e9;
}
int main(int argc, char **argv)
{
    if (argc != 3 || (strcmp(argv[2], "failure") != 0 &&
                      strcmp(argv[2], "format-failure") != 0 &&
                      strcmp(argv[2], "success") != 0))
        return 2;
    int fail = strcmp(argv[2], "success") != 0;
    for (int n = 0; n < (fail ? 10 : 2); n++) {
        mpv_handle *p = mpv_create();
        if (!p)
            return 2;
        if (mpv_set_option_string(p, "config", "no") < 0 ||
            mpv_set_option_string(p, "ao", "coreaudio") < 0 ||
            mpv_set_option_string(p, "audio-channels", "mono") < 0 ||
            mpv_set_option_string(p, "vid", "no") < 0 ||
            mpv_set_option_string(p, "idle", "yes") < 0 ||
            mpv_set_option_string(p, "pause", "yes") < 0 ||
            mpv_set_option_string(p, "clipboard-backends", "none") < 0 ||
            mpv_initialize(p) < 0) {
            mpv_terminate_destroy(p);
            return 2;
        }
        const char *load[] = {"loadfile", argv[1], NULL};
        if (mpv_command(p, load) < 0) {
            mpv_terminate_destroy(p);
            return 2;
        }
        int matched = 0;
        double deadline = now() + 5;
        while (now() < deadline) {
            mpv_event *event = mpv_wait_event(p, 0.05);
            if (event->event_id == MPV_EVENT_END_FILE) {
                mpv_event_end_file *end = event->data;
                fprintf(stderr, "END iteration=%d reason=%d error=%d\n", n,
                        end->reason, end->error);
                matched = fail && end->reason == MPV_END_FILE_REASON_ERROR &&
                          end->error == MPV_ERROR_AO_INIT_FAILED;
                break;
            }
            if (!fail) {
                char *ao = mpv_get_property_string(p, "current-ao");
                if (ao) {
                    matched = strcmp(ao, "coreaudio") == 0;
                    mpv_free(ao);
                    if (matched)
                        break;
                }
            }
        }
        mpv_terminate_destroy(p);
        if (!matched) {
            fprintf(stderr, "FAIL iteration=%d expected=%s\n", n, argv[2]);
            return 1;
        }
    }
    puts("EXPECTED_LIFECYCLE_PASS");
    return 0;
}
