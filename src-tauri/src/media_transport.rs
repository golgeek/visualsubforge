//! Transport policy; called only by the media control worker.

/// A missing live audio clock must not be replaced by the smoothed transport
/// clock. Once output has drained, the engine's final transport position is a
/// useful resting cursor, not a claim that audio is still being presented.
pub fn select_position(
    queued_audio: bool,
    audio_only: bool,
    eof: bool,
    audio: Option<f64>,
    transport: Option<f64>,
) -> Option<f64> {
    if queued_audio && audio_only {
        audio.or(if eof { transport } else { None })
    } else {
        transport
    }
}

#[cfg(all(
    feature = "media-prototype",
    any(target_os = "macos", target_os = "windows", target_os = "linux")
))]
pub fn position(player: &mut media_player::Player, queued_audio: bool) -> Option<f64> {
    let moving_video =
        player.number(c"vid").is_some() && !player.flag(c"current-tracks/video/image");
    let audio_only = player.number(c"aid").is_some() && !moving_video;
    select_position(
        queued_audio,
        audio_only,
        player.flag(c"eof-reached"),
        player.number(c"audio-pts"),
        player.number(c"time-pos"),
    )
}

#[cfg(all(
    feature = "media-prototype",
    any(target_os = "macos", target_os = "windows", target_os = "linux")
))]
pub fn clear_loop(player: &mut media_player::Player, queued_audio: bool) -> Result<(), String> {
    let old_a = player
        .text(c"ab-loop-a")
        .ok_or("Loop start is unavailable")?;
    let old_b = player.text(c"ab-loop-b").ok_or("Loop end is unavailable")?;
    if !queued_audio || old_a == "no" || old_b == "no" {
        player.command(&["set", "ab-loop-a", "no"])?;
        return player.command(&["set", "ab-loop-b", "no"]);
    }

    // Freeze output before choosing a restart point. CoreAudio can retain
    // already-submitted PCM across reset: its opt-in continuation API maps the
    // end of that PCM, including the left side of an exact loop boundary.
    // It must never replace the audible clock used by position()/Follow.
    // Other outputs retain their existing policy until separately validated.
    let paused = player.paused();
    player.command(&["set", "pause", "yes"])?;
    let result = (|| {
        let submitted = player.text(c"vsf-audio-continuation-version").as_deref() == Some("1")
            && player.text(c"current-ao").as_deref() == Some("coreaudio");
        let target = if submitted {
            // Missing history is an error, not permission to fall back to the
            // earlier audible position and replay submitted audio.
            player
                .number(c"vsf-audio-continuation-pts")
                .ok_or("Audio continuation is unavailable; retry after the seek finishes")?
        } else {
            position(player, queued_audio)
                .ok_or("Playback clock is unavailable; retry after the seek finishes")?
        };
        player.command(&["set", "ab-loop-a", "no"])?;
        player.command(&["set", "ab-loop-b", "no"])?;
        player.command(&["seek", &target.to_string(), "absolute+exact"])
    })();
    if result.is_err() {
        // Best-effort rollback; if native commands fail, report it explicitly.
        for (name, value) in [("ab-loop-a", old_a.as_str()), ("ab-loop-b", old_b.as_str())] {
            if let Err(error) = player.command(&["set", name, value]) {
                return Err(format!(
                    "Could not restore {name}: {error}; playback remains paused"
                ));
            }
        }
    }
    player.command(&["set", "pause", if paused { "yes" } else { "no" }])?;
    result
}

#[cfg(test)]
mod tests {
    use super::select_position;

    #[test]
    fn audio_clock_keeps_discontinuities_and_unavailable_state() {
        assert_eq!(
            select_position(true, true, false, Some(7198.95), Some(7195.0)),
            Some(7198.95)
        );
        assert_eq!(select_position(true, true, false, None, Some(7195.0)), None);
        assert_eq!(
            select_position(true, true, false, Some(10.0), Some(20.0)),
            Some(10.0)
        );
        assert_eq!(
            select_position(true, true, true, None, Some(7200.0)),
            Some(7200.0)
        );
        assert_eq!(
            select_position(true, true, true, Some(7199.9), Some(7200.0)),
            Some(7199.9)
        );
    }

    #[test]
    fn video_and_unpatched_playback_keep_the_transport_clock() {
        assert_eq!(
            select_position(true, false, false, Some(1.5), Some(1.6)),
            Some(1.6)
        );
        assert_eq!(
            select_position(false, true, false, Some(1.5), Some(1.6)),
            Some(1.6)
        );
        assert_eq!(select_position(true, false, false, Some(1.5), None), None);
    }
}
