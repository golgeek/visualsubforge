//! One-shot playback. Only the media control worker reads or changes this target.
use media_player::Player;

#[derive(Default)]
pub struct SelectionPlayback {
    target: Option<Target>,
}

struct Target {
    session: u64,
    start: f64,
    end: f64,
    user_paused: bool,
}

fn exact_seek(player: &mut Player, seconds: f64) -> Result<(), String> {
    player.command(&["seek", &seconds.to_string(), "absolute+exact"])
}

impl SelectionPlayback {
    pub fn play(
        &mut self,
        player: &mut Player,
        session: u64,
        start: f64,
        end: f64,
        queued_audio: bool,
    ) -> Result<(), String> {
        let resume = self.target.as_ref().is_some_and(|target| {
            target.session == session
                && target.start == start
                && target.end == end
                && target.user_paused
        }) && player.paused()
            && !player.flag(c"eof-reached")
            && crate::media_transport::position(player, queued_audio)
                .is_some_and(|t| start < t && t < end);

        // mpv clips decoded audio and video at end. keep-open pauses after drain.
        player.command(&["set", "end", &end.to_string()])?;
        self.target = Some(Target {
            session,
            start,
            end,
            user_paused: false,
        });
        let result = (|| {
            if !resume {
                exact_seek(player, start)?;
            }
            player.command(&["set", "pause", "no"])
        })();
        if let Err(error) = result {
            self.cancel(player)
                .map_err(|cleanup| format!("{error}; clearing playback end: {cleanup}"))?;
            return Err(error);
        }
        Ok(())
    }

    pub fn pause(&mut self, player: &mut Player, paused: bool) -> Result<(), String> {
        if !paused {
            self.clear_selection(player, None)?;
        }
        let interrupted = paused && self.target.is_some() && !player.paused();
        player.command(&["set", "pause", if paused { "yes" } else { "no" }])?;
        if interrupted && let Some(target) = &mut self.target {
            target.user_paused = true;
        }
        Ok(())
    }

    pub fn seek(&mut self, player: &mut Player, seconds: f64) -> Result<(), String> {
        if self
            .target
            .as_ref()
            .is_some_and(|t| seconds < t.start || seconds >= t.end)
        {
            self.cancel(player)?;
        }
        exact_seek(player, seconds)
    }

    pub fn seek_to_cue(
        &mut self,
        player: &mut Player,
        session: u64,
        start: f64,
        end: f64,
        queued_audio: bool,
    ) -> Result<(), String> {
        if self.target.as_ref().is_some_and(|t| t.session == session) && !player.paused() {
            self.play(player, session, start, end, queued_audio)
        } else {
            self.cancel(player)?;
            exact_seek(player, start)
        }
    }

    pub fn cancel(&mut self, player: &mut Player) -> Result<(), String> {
        if self.target.is_some() {
            player.command(&["set", "end", "none"])?;
            self.target = None;
        }
        Ok(())
    }

    pub fn clear_selection(
        &mut self,
        player: &mut Player,
        seek: Option<f64>,
    ) -> Result<(), String> {
        // A clipped EOF leaves mpv parked on the last frame. Restore the normal
        // playback cursor at the cut before a subsequent unconstrained Play.
        let cursor = seek.or_else(|| {
            self.target.as_ref().map(|t| t.end).filter(|end| {
                player.flag(c"eof-reached")
                    && player
                        .number(c"duration")
                        .is_some_and(|duration| *end < duration)
            })
        });
        self.cancel(player)?;
        if let Some(seconds) = cursor {
            exact_seek(player, seconds)?;
        }
        Ok(())
    }
}
