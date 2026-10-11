//! Opt-in application regression against the explicit queued-clock capability.
use crate::media::{Action, Media, Status};
use crate::media_smoke::wait;
use std::time::{Duration, Instant};
use tauri::Manager;

pub fn start(app: tauri::AppHandle, path: String) {
    tauri::async_runtime::spawn_blocking(move || {
        let result = run(&app.state::<Media>(), &path);
        match &result {
            Ok(()) => eprintln!("VSF AUDIO SMOKE: PASS (transport/clock; not acoustic latency)"),
            Err(error) => eprintln!("VSF AUDIO SMOKE: FAIL: {error}"),
        }
        crate::diagnostic_exit(&app, result.is_ok());
    });
}

fn require(condition: bool, message: &str) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}

fn seek_paused(media: &Media, session: u64, target: f64) -> Result<Status, String> {
    media.request(Action::Pause { paused: true }, session)?;
    media.request(Action::Seek { seconds: target }, session)?;
    wait(media, |s| {
        s.paused
            && s.position_seconds
                .is_some_and(|p| (p - target).abs() < 0.001)
    })
}

fn run(media: &Media, path: &str) -> Result<(), String> {
    media.request(
        Action::Open {
            path: path.into(),
            hardware: false,
        },
        0,
    )?;
    let loaded = wait(media, |s| s.loaded)?;
    require(
        loaded.queued_audio_clock,
        "Load the pinned patched libmpv; queued-clock capability is absent",
    )?;
    require(
        loaded
            .duration_seconds
            .is_some_and(|d| (d - 7200.0).abs() < 0.01),
        "Use the generated two-hour long.flac fixture",
    )?;
    let session = loaded.session;
    let (a, b) = (7198.930, 7199.253);
    seek_paused(media, session, a)?;
    media.request(Action::Loop { start: a, end: b }, session)?;
    seek_paused(media, session, a)?;
    media.request(Action::Pause { paused: false }, session)?;
    let started = Instant::now();
    let mut previous = a;
    let mut wraps = 0;
    let mut samples = 0;
    let mut missing = 0;
    let mut low = b;
    let mut high = a;
    while started.elapsed() < Duration::from_secs(3) {
        let status = media.request(Action::Status, session)?;
        if let Some(position) = status.position_seconds {
            require(
                (a - 0.000125..=b + 0.000125).contains(&position),
                &format!(
                    "Audio clock escaped the short loop at {position:.6}, seeking={}",
                    status.seeking
                ),
            )?;
            if position < previous - 0.1 {
                wraps += 1;
            }
            previous = position;
            low = low.min(position);
            high = high.max(position);
            samples += 1;
        } else {
            missing += 1;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    require(
        wraps >= 6 && samples > 80 && missing < samples / 5,
        "Loop clock did not progress reliably",
    )?;
    eprintln!(
        "VSF AUDIO SMOKE: {wraps} wraps, {samples} positions, {missing} unavailable; range {low:.6}..{high:.6}"
    );

    media.request(Action::Pause { paused: true }, session)?;
    let paused = wait(media, |s| s.paused && s.position_seconds.is_some())?;
    let position = paused.position_seconds.ok_or("Paused clock unavailable")?;
    for _ in 0..15 {
        std::thread::sleep(Duration::from_millis(20));
        let next = media.request(Action::Status, session)?;
        require(
            next.position_seconds
                .is_some_and(|p| (p - position).abs() < 0.000125),
            "Paused audio clock moved",
        )?;
    }
    seek_paused(media, session, a - 0.4)?;
    seek_paused(media, session, b + 0.1)?;
    seek_paused(media, session, a + 0.1)?;
    let cleared = media.request(Action::ClearLoop, session)?;
    require(cleared.paused, "Clearing a paused loop resumed playback")?;
    media.request(Action::Pause { paused: false }, session)?;
    wait(media, |s| s.position_seconds.is_some_and(|p| p > b + 0.2))?;
    eprintln!("VSF AUDIO SMOKE: pause, outside-range seeks and paused clear passed");

    seek_paused(media, session, a)?;
    media.request(Action::Loop { start: a, end: b }, session)?;
    seek_paused(media, session, a)?;
    media.request(Action::Pause { paused: false }, session)?;
    std::thread::sleep(Duration::from_millis(700));
    let cleared = media.request(Action::ClearLoop, session)?;
    require(
        !cleared.paused,
        "Clearing a playing loop left playback paused",
    )?;
    wait(media, |s| s.position_seconds.is_some_and(|p| p > b + 0.2))?;
    wait(media, |s| {
        s.paused && s.position_seconds.is_some_and(|p| p > 7199.8)
    })?;
    eprintln!("VSF AUDIO SMOKE: playing clear and EOF resting cursor passed");
    let closed = media.request(Action::Close, session)?;
    require(
        !closed.loaded && closed.position_seconds.is_none(),
        "Close retained a clock",
    )?;
    Ok(())
}
