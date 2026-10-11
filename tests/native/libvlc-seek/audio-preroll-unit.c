/* Isolated regression for the experimental VLC input-core PCM trimming helper.
 * Compile with matching core flags and src/input includes, then link that core.
 * Checks payload sample identity, bounds, nearest-sample ties-forward rounding
 * and frame allocation ownership; not device presentation or concurrency.
 */
#include "audio_preroll.h"
#include <assert.h>
#include <limits.h>
#include <stdio.h>
static vlc_frame_t *make(unsigned frames)
{
    vlc_frame_t *f = vlc_frame_Alloc(frames * 4);
    assert(f);
    for (unsigned i = 0; i < frames; ++i) {
        ((int16_t *)f->p_buffer)[i*2] = i;
        ((int16_t *)f->p_buffer)[i*2+1] = -((int)i);
    }
    f->i_nb_samples = frames; f->i_pts = f->i_dts = 1450667;
    f->i_length = vlc_tick_from_samples(frames, 48000);
    return f;
}
int main(void)
{
    audio_sample_format_t fmt = {.i_format=VLC_CODEC_S16N, .i_rate=48000,
                                 .i_frame_length=1, .i_bytes_per_frame=4};
    vlc_frame_t *f = make(4096); uint8_t *allocation = f->p_start;
    assert(vlc_audio_preroll_Trim(f, &fmt, 1500000));
    assert(f->i_pts == 1500000 && f->i_dts == 1500000);
    assert(f->i_nb_samples == 1728 && f->i_buffer == 1728*4);
    assert(((int16_t *)f->p_buffer)[0] == 2368 && ((int16_t *)f->p_buffer)[1] == -2368);
    assert(f->p_start == allocation && f->i_length == 36000);
    vlc_frame_Release(f);
    f=make(48); assert(!vlc_audio_preroll_Trim(f, &fmt, f->i_pts+1000)); vlc_frame_Release(f);
    f=make(48); assert(vlc_audio_preroll_Trim(f, &fmt, f->i_pts+1));
    assert(f->i_nb_samples==48 && ((int16_t *)f->p_buffer)[0]==0);vlc_frame_Release(f);
    f=make(48); uint8_t *original=f->p_buffer;
    assert(vlc_audio_preroll_Trim(f, &fmt, f->i_pts) && f->p_buffer==original);
    assert(vlc_audio_preroll_Trim(f, &fmt, VLC_TICK_INVALID));
    assert(!vlc_audio_preroll_Trim(f, &fmt, VLC_TICK_MAX));vlc_frame_Release(f);
    f=make(48);fmt.i_format=VLC_CODEC_A52;
    assert(vlc_audio_preroll_Trim(f, &fmt, f->i_pts+1) && f->i_nb_samples==48);vlc_frame_Release(f);
    fmt.i_format=VLC_CODEC_S16N;
    f=make(48);f->i_buffer=4;assert(!vlc_audio_preroll_Trim(f,&fmt,f->i_pts+1));vlc_frame_Release(f);
    f=make(48);f->i_pts=VLC_TICK_MAX-11;f->i_dts=VLC_TICK_MAX-11;
    assert(!vlc_audio_preroll_Trim(f,&fmt,VLC_TICK_MAX));vlc_frame_Release(f);
    f=make(48);f->i_dts=VLC_TICK_MAX;assert(!vlc_audio_preroll_Trim(f,&fmt,f->i_pts+11));vlc_frame_Release(f);
    f=make(48);f->i_pts=VLC_TICK_MIN;assert(!vlc_audio_preroll_Trim(f,&fmt,1));vlc_frame_Release(f);
    f=make(48);fmt.i_rate=44100;assert(vlc_audio_preroll_Trim(f,&fmt,f->i_pts+23));
    assert(f->i_nb_samples==47 && ((int16_t *)f->p_buffer)[0]==1);vlc_frame_Release(f);
    fmt.i_rate=1000;
    f=make(48);assert(vlc_audio_preroll_Trim(f,&fmt,f->i_pts+499));assert(f->i_nb_samples==48);vlc_frame_Release(f);
    f=make(48);assert(vlc_audio_preroll_Trim(f,&fmt,f->i_pts+500));assert(f->i_nb_samples==47);vlc_frame_Release(f);
    puts("PCM sample identity, stereo stride, boundary exclusion, nearest rounding, no-op, extreme target and passthrough policy pass");
    return 0;
}
