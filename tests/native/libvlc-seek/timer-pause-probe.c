/* Isolated regression for the pinned VLC player timer, not a VSF runtime API.
 * Compile beside a matching upstream timer.c saved as timer-under-test.c, with
 * the core build's compiler flags and src/player include path. Link that build's
 * libvlc/libvlccore. Only timer-owned state is initialized; no media or threads
 * are started. The synthetic monotonic dates make the checks deterministic.
 */
#include "timer-under-test.c"
struct observation { unsigned count; struct vlc_player_timer_point point; };
static void observe(const struct vlc_player_timer_point *point, void *opaque)
{
    struct observation *o = opaque; ++o->count; o->point = *point;
}
static bool query(vlc_player_t *player, vlc_tick_t date, vlc_tick_t expected)
{
    bool seeking = false; vlc_tick_t actual = VLC_TICK_INVALID;
    int ret = vlc_player_GetTimerPoint(player, &seeking, date, &actual, NULL);
    printf("QUERY date=%lld expected=%lld actual=%lld ret=%d\n",
           (long long)date, (long long)expected, (long long)actual, ret);
    return ret == 0 && actual == expected;
}
int main(void)
{
    bool passed = true;
    for (unsigned iteration = 0; iteration < 3; ++iteration)
    {
        double rate = iteration == 0 ? 0.5 : iteration == 1 ? 1.0 : 2.0;
        vlc_player_t player = {0}; vlc_player_InitTimer(&player);
        struct observation observed = {0};
        const struct vlc_player_timer_cbs cbs = {.on_update = observe};
        vlc_player_timer_id *timer = vlc_player_AddTimer(&player, VLC_TICK_INVALID, &cbs, &observed);
        assert(timer != NULL);
        const vlc_tick_t origin = 10000000, timestamp = 1000001;
        struct vlc_player_timer_point point = {.rate=rate, .ts=timestamp, .length=10000000,
                                              .system_date=origin, .position=0.1};
        vlc_player_UpdateTimer(&player, NULL, false, &point, VLC_TICK_0, 0, 0, 0);
        point.ts += (vlc_tick_t)(100000 * rate); point.system_date += 100000;
        vlc_player_UpdateTimer(&player, NULL, false, &point, VLC_TICK_0, 0, 0, 0);
        vlc_player_UpdateTimerEvent(&player, NULL, VLC_PLAYER_TIMER_EVENT_PAUSED, origin+200000);
        passed &= query(&player, origin+5200000, timestamp+(vlc_tick_t)(200000*rate));
        unsigned before = observed.count;
        vlc_player_UpdateTimerEvent(&player, NULL, VLC_PLAYER_TIMER_EVENT_PLAYING, origin+5200000);
        passed &= query(&player, origin+5300000, timestamp+(vlc_tick_t)(300000*rate));
        vlc_tick_t watch_ts = VLC_TICK_INVALID;
        int ret = vlc_player_timer_point_Interpolate(&observed.point, origin+5300000, &watch_ts, NULL);
        bool notified = observed.count > before && ret == 0 && watch_ts == timestamp+(vlc_tick_t)(300000*rate);
        printf("WATCH rate=%.1f notified=%d time=%lld\n", rate, notified, (long long)watch_ts);
        passed &= notified;
        /* A repeated PLAYING event must not apply the pause twice. */
        vlc_player_UpdateTimerEvent(&player, NULL, VLC_PLAYER_TIMER_EVENT_PLAYING, origin+5300000);
        passed &= query(&player, origin+5400000, timestamp+(vlc_tick_t)(400000*rate));
        vlc_player_UpdateTimerEvent(&player, NULL, VLC_PLAYER_TIMER_EVENT_PAUSED, origin+5400000);
        passed &= query(&player, origin+6000000, timestamp+(vlc_tick_t)(400000*rate));
        vlc_player_RemoveTimer(&player, timer); vlc_player_DestroyTimer(&player);
    }
    return passed ? 0 : 1;
}
