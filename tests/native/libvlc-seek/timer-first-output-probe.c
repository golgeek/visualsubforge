/* Regression for the pinned private VLC timer implementation, not VSF runtime
 * logic. Compile beside timer-under-test.c with matching core compiler flags
 * and src/player includes, then link that core. Define FIRST_OUTPUT_MAPPING
 * only for the candidate helper signature. No media or threads are started.
 * BestSource normally records last_ts after accepting a point; this focused
 * helper test supplies that state explicitly. Real player tests cover callers.
 */
#include "timer-under-test.c"
static bool query(vlc_player_t *player, vlc_tick_t date, vlc_tick_t expected)
{
    bool seeking = false; vlc_tick_t actual = VLC_TICK_INVALID;
    int ret = vlc_player_GetTimerPoint(player, &seeking, date, &actual, NULL);
    printf("QUERY date=%lld expected=%lld actual=%lld ret=%d\n",
           (long long)date, (long long)expected, (long long)actual, ret);
    return ret == 0 && actual == expected;
}
static void supply(vlc_player_t *player, bool input, vlc_tick_t ts, vlc_tick_t date)
{
#ifdef FIRST_OUTPUT_MAPPING
    vlc_player_UpdateTimerSource(player, &player->timer.best_source, 1.0, ts, date, input);
#else
    (void)input;
    vlc_player_UpdateTimerSource(player, &player->timer.best_source, 1.0, ts, date);
#endif
}
int main(void)
{
    bool passed = true;
    vlc_player_t player = {0}; vlc_player_InitTimer(&player);
    player.timer.input_length = 10000000;
    /* The first output point at startup must still be frozen. */
    supply(&player, false, 1000001, 9000000);
    passed &= query(&player, 9200000, 1000001);
    /* BestSource sets last_ts after accepting each point. */
    player.timer.last_ts = 1000001;
    vlc_player_UpdateTimerEvent(&player, NULL, VLC_PLAYER_TIMER_EVENT_DISCONTINUITY, VLC_TICK_INVALID);
    supply(&player, true, 1500001, 10000000);
    passed &= query(&player, 10200000, 1500001);
    vlc_player_UpdateTimerEvent(&player, NULL, VLC_PLAYER_TIMER_EVENT_DISCONTINUITY, VLC_TICK_INVALID);
    supply(&player, false, 1500001, 10000000);
    passed &= query(&player, 10200000, 1700001);
    vlc_player_UpdateTimerEvent(&player, NULL, VLC_PLAYER_TIMER_EVENT_DISCONTINUITY, VLC_TICK_INVALID);
    supply(&player, false, 2000001, 20000000);
    passed &= query(&player, 20200000, 2200001);
    vlc_player_UpdateTimerEvent(&player, NULL, VLC_PLAYER_TIMER_EVENT_DISCONTINUITY, VLC_TICK_INVALID);
    supply(&player, false, 2500001, VLC_TICK_MAX);
    passed &= query(&player, 30200000, 2500001);
    vlc_player_DestroyTimer(&player);
    return passed ? 0 : 1;
}
