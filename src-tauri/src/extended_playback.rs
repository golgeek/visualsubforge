//! Longer-file, fallback and container checks on the actual desktop media boundary.
use crate::media::{Action, Media};
use crate::media_smoke::wait;
use std::time::{Duration, Instant};
use tauri::Manager;

pub fn start(app: tauri::AppHandle, directory: String) {
    tauri::async_runtime::spawn_blocking(move || {
        let mut report =
            String::from("Longer desktop playback; swaps are not presentation timestamps.\n");
        let result = run(&app.state::<Media>(), &directory, &mut report);
        report.push_str(&format!("result={result:?}\n"));
        let saved = std::fs::write(
            std::path::Path::new(&directory).join(
                if std::env::var_os("VSF_EXTENDED_FILTER").is_some() {
                    "extended-filter-results.txt"
                } else {
                    "extended-results.txt"
                },
            ),
            &report,
        );
        eprintln!("VSF EXTENDED: {report} report_write={saved:?}");
        crate::diagnostic_exit(&app, result.is_ok() && saved.is_ok());
    });
}
fn run(media: &Media, directory: &str, report: &mut String) -> Result<(), String> {
    let filter = std::env::var("VSF_EXTENDED_FILTER").ok();
    let mut matched = false;
    // Known rates belong to these fixture contracts, not arbitrary user media.
    for (file, hardware, expected_hardware, fps, play_seconds) in [
        ("fallback-ffv1.mkv", true, false, 24.0, 20),
        ("long-gop-h264.mkv", true, true, 25.0, 170),
        ("long-gop-h264.mkv", false, false, 25.0, 30),
        ("hevc-main10-pq.mkv", true, true, 30.0, 60),
        ("hevc-main10-pq.mkv", false, false, 30.0, 60),
        ("big-buck-bunny.mp4", true, true, 24.0, 580),
        ("big-buck-bunny.mp4", false, false, 24.0, 30),
        ("big-buck-bunny-1080.mov", true, true, 24.0, 60),
        ("big-buck-bunny-1080.mov", false, false, 24.0, 60),
    ] {
        if filter.as_deref().is_some_and(|name| name != file) {
            continue;
        }
        matched = true;
        let session = media
            .request(
                Action::Open {
                    path: std::path::Path::new(directory)
                        .join(file)
                        .to_string_lossy()
                        .into_owned(),
                    hardware,
                },
                0,
            )?
            .session;
        let loaded = wait(media, |s| {
            s.session == session && s.loaded && s.rendered_frames > 0
        })?;
        let decoder = loaded
            .hardware_decoder
            .as_deref()
            .ok_or("No decoder reported")?;
        if (decoder != "no" && !decoder.is_empty()) != expected_hardware {
            return Err(format!("{file}: unexpected decoder {decoder}"));
        }
        let duration = loaded.duration_seconds.ok_or("No duration")?;
        let mut seek_ms = Vec::new();
        for seconds in [duration - 2.0, duration / 2.0, 1.0, 0.0] {
            let start = Instant::now();
            media.request(Action::Seek { seconds }, session)?;
            let at = wait(media, |s| {
                s.position_seconds
                    .is_some_and(|t| (t - seconds).abs() <= 1.0 / fps + 0.005)
            })?;
            seek_ms.push(start.elapsed().as_secs_f64() * 1000.0);
            let position = at.position_seconds.ok_or("Missing seek position")?;
            media.request(Action::Step { backwards: false }, session)?;
            wait(media, |s| {
                s.position_seconds
                    .is_some_and(|t| (t - position - 1.0 / fps).abs() < 0.01)
            })?;
            media.request(Action::Step { backwards: true }, session)?;
            wait(media, |s| {
                s.position_seconds
                    .is_some_and(|t| (t - position).abs() < 0.01)
            })?;
        }
        let before = media.request(Action::Status, session)?;
        media.request(Action::Pause { paused: false }, session)?;
        let start = Instant::now();
        let mut previous = 0.0;
        let mut changed = Instant::now();
        let mut max_avsync = 0.0_f64;
        let mut av_samples = 0;
        let mut progress = 30;
        while start.elapsed() < Duration::from_secs(play_seconds) {
            let state = media.request(Action::Status, session)?;
            if state.session != session
                || !state.loaded
                || state.error.is_some()
                || state.hardware_decoder != loaded.hardware_decoder
            {
                return Err(format!("{file}: playback state changed: {:?}", state.error));
            }
            let position = state.position_seconds.ok_or("Missing position")?;
            if position > previous + 0.005 {
                changed = Instant::now();
            }
            if changed.elapsed() > Duration::from_secs(2) {
                return Err(format!("{file}: stalled"));
            }
            previous = position;
            if let Some(av) = state.av_sync_seconds {
                max_avsync = max_avsync.max(av.abs());
                av_samples += 1;
            }
            if start.elapsed().as_secs() >= progress {
                eprintln!(
                    "VSF EXTENDED: {file} hardware={hardware} elapsed={} position={position:.3}",
                    start.elapsed().as_secs()
                );
                progress += 30;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let after = media.request(Action::Pause { paused: true }, session)?;
        if after.position_seconds.unwrap_or(0.0) < play_seconds as f64 - 1.0
            || after.rendered_frames <= before.rendered_frames
        {
            return Err(format!("{file}: insufficient playback progress"));
        }
        let line = format!(
            "{file} requested_hardware={hardware} actual={decoder} duration={duration:.3} seek_ms={seek_ms:.1?} play_s={play_seconds} swaps={} decoder_drops={:?} output_drops={:?} max_abs_avsync_s={max_avsync:.4} av_samples={av_samples}\n",
            after.rendered_frames - before.rendered_frames,
            after
                .decoder_drops
                .zip(before.decoder_drops)
                .map(|(a, b)| a - b),
            after
                .output_drops
                .zip(before.output_drops)
                .map(|(a, b)| a - b)
        );
        eprint!("VSF EXTENDED: {line}");
        report.push_str(&line);
        media.request(Action::Close, session)?;
    }
    if !matched {
        return Err("No extended fixture matched the filter".into());
    }
    Ok(())
}
