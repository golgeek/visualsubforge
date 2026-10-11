//! Opt-in desktop checks. Needs a real window/GPU; never a headless CI claim.
use crate::media::{Action, Media, Status};
use std::time::{Duration, Instant};
use tauri::Manager;

pub fn start(app: tauri::AppHandle, path: String) {
    tauri::async_runtime::spawn_blocking(move || {
        let result = run(&app.state::<Media>(), &path);
        let success = result.is_ok();
        match result {
            Ok(()) => eprintln!(
                "VSF GPU SMOKE: PASS (command/property checks; visual confirmation still required)"
            ),
            Err(error) => eprintln!("VSF GPU SMOKE: FAIL: {error}"),
        }
        crate::diagnostic_exit(&app, success);
    });
}
pub(super) fn wait(media: &Media, predicate: impl Fn(&Status) -> bool) -> Result<Status, String> {
    let start = Instant::now();
    loop {
        let status = media.request(Action::Status, 0)?;
        if let Some(error) = &status.error {
            return Err(error.clone());
        }
        if !status.seeking && predicate(&status) {
            return Ok(status);
        }
        if start.elapsed() > Duration::from_secs(10) {
            return Err(format!(
                "Timed out: session={} loaded={} paused={} position={:?} swaps={}",
                status.session,
                status.loaded,
                status.paused,
                status.position_seconds,
                status.rendered_frames
            ));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
fn require(condition: bool, message: &str) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}
// Direct Windows VO does not expose our render-API swap counter. This checks
// configuration there; visible pixels remain a separate desktop acceptance gate.
fn video_ready(status: &Status) -> bool {
    status.loaded
        && if status.swap_counter_available {
            status.rendered_frames > 0
        } else {
            status.video_output_configured
                && (!cfg!(target_os = "linux") || status.rendered_frames > 0)
        }
}
fn run(media: &Media, path: &str) -> Result<(), String> {
    let start = Instant::now();
    media.request(
        Action::Open {
            path: path.into(),
            hardware: true,
        },
        0,
    )?;
    let first = wait(media, video_ready)?;
    eprintln!(
        "VSF GPU SMOKE: open-to-video-ready {:.3}s",
        start.elapsed().as_secs_f64()
    );
    let session = first.session;
    media.request(Action::Seek { seconds: 2.0 }, session)?;
    wait(media, |s| {
        s.position_seconds.is_some_and(|t| (t - 2.0).abs() < 0.025)
    })?;
    media.request(Action::Step { backwards: false }, session)?;
    // VFR fixture switches to 10 fps at two seconds.
    wait(media, |s| {
        s.position_seconds.is_some_and(|t| (t - 2.1).abs() < 0.025)
    })?;
    media.request(Action::Step { backwards: true }, session)?;
    wait(media, |s| {
        s.position_seconds.is_some_and(|t| (t - 2.0).abs() < 0.025)
    })?;
    for seconds in [0.4, 3.0, 1.2, 2.5, 0.0] {
        media.request(Action::Seek { seconds }, session)?;
        wait(media, |s| {
            s.position_seconds
                .is_some_and(|t| (t - seconds).abs() < 0.025)
        })?;
    }
    media.request(Action::Track { id: 2 }, session)?;
    wait(media, |s| s.audio_track == Some(2.0))?;
    media.request(
        Action::Loop {
            start: 1.0,
            end: 1.5,
        },
        session,
    )?;
    media.request(Action::Seek { seconds: 1.0 }, session)?;
    media.request(Action::Pause { paused: false }, session)?;
    let start = Instant::now();
    let mut previous = 0.0;
    let mut wraps = 0;
    while start.elapsed() < Duration::from_secs(2) {
        let status = media.request(Action::Status, session)?;
        if let Some(position) = status.position_seconds {
            if previous > position + 0.2 {
                wraps += 1;
            }
            previous = position;
        }
        std::thread::sleep(Duration::from_millis(30));
    }
    require(wraps >= 2, "Selection loop did not wrap twice")?;
    media.request(Action::Pause { paused: true }, session)?;
    media.request(Action::ClearLoop, session)?;
    // Reuse must isolate per-file track, loop and pause state, not just generations.
    for _ in 0..3 {
        let current = media.request(Action::Status, 0)?;
        if current.loaded {
            media.request(Action::Track { id: 2 }, current.session)?;
            media.request(
                Action::Loop {
                    start: 1.0,
                    end: 1.5,
                },
                current.session,
            )?;
            media.request(Action::Pause { paused: false }, current.session)?;
        }
        media.request(
            Action::Open {
                path: path.into(),
                hardware: true,
            },
            0,
        )?;
        let replacement = wait(media, video_ready)?;
        require(replacement.paused, "Replacement inherited playing state")?;
        require(
            replacement.audio_track == Some(1.0),
            "Replacement inherited track choice",
        )?;
        require(
            replacement.session > session,
            "Replacement did not change session",
        )?;
        require(
            media
                .request(Action::Seek { seconds: 0.0 }, session)
                .is_err(),
            "Stale command was accepted",
        )?;
        wait(media, |s| s.session == replacement.session && s.loaded)?;
        media.request(Action::Seek { seconds: 2.5 }, replacement.session)?;
        wait(media, |s| {
            s.position_seconds.is_some_and(|t| (t - 2.5).abs() < 0.025)
        })?;
        media.request(Action::Pause { paused: false }, replacement.session)?;
        wait(media, |s| s.position_seconds.is_some_and(|t| t > 2.7))?;
        let closed = media.request(Action::Close, replacement.session)?;
        require(
            !closed.loaded && closed.position_seconds.is_none() && closed.rendered_frames == 0,
            "Close retained media state",
        )?;
        require(
            media
                .request(Action::Pause { paused: false }, closed.session)
                .is_err(),
            "Closed engine accepted a playback command",
        )?;
    }
    require(
        media
            .request(
                Action::Open {
                    path: format!("{path}.missing"),
                    hardware: true,
                },
                0,
            )
            .is_err(),
        "Missing file was accepted",
    )?;
    // A real file that fails asynchronously in mpv must not poison the next load.
    let invalid =
        std::env::temp_dir().join(format!("vsf-invalid-media-{}.txt", std::process::id()));
    std::fs::write(&invalid, "not a media file").map_err(|e| e.to_string())?;
    let invalid_result = (|| {
        media.request(
            Action::Open {
                path: invalid.to_string_lossy().into_owned(),
                hardware: true,
            },
            0,
        )?;
        let start = Instant::now();
        loop {
            if media.request(Action::Status, 0)?.error.is_some() {
                break Ok(());
            }
            if start.elapsed() > Duration::from_secs(10) {
                break Err("Invalid media did not report an error".to_string());
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    })();
    let _ = std::fs::remove_file(invalid);
    invalid_result?;
    media.request(
        Action::Open {
            path: path.into(),
            hardware: true,
        },
        0,
    )?;
    let recovered = wait(media, video_ready)?;
    // Run after the replacement sequence so the frontend has supplied layout.
    // Windows logs actual child visibility after each main-thread acknowledgement;
    // inspect those values separately from the command/property PASS below.
    media.request(Action::Occlude { occluded: true }, recovered.session)?;
    media.request(Action::Occlude { occluded: false }, recovered.session)?;
    Ok(())
}
