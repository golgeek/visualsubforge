//! Short synthetic SDR measurement, not a sustained-playback acceptance test.
use crate::media::{Action, Media};
use crate::media_smoke::wait;
use std::time::{Duration, Instant};
use tauri::Manager;

pub fn start(app: tauri::AppHandle, directory: String) {
    tauri::async_runtime::spawn_blocking(move || {
        let result = run(&app.state::<Media>(), &directory);
        let mut success = result.is_ok();
        match result {
            Ok(report) => {
                let path = std::path::Path::new(&directory).join("playback-results.txt");
                match std::fs::write(path, &report) {
                    Ok(()) => eprintln!("VSF PLAYBACK BENCH: PASS\n{report}"),
                    Err(e) => {
                        success = false;
                        eprintln!("VSF PLAYBACK BENCH: report write failed: {e}");
                    }
                }
            }
            Err(e) => eprintln!("VSF PLAYBACK BENCH: FAIL: {e}"),
        }
        crate::diagnostic_exit(&app, success);
    });
}
fn run(media: &Media, directory: &str) -> Result<String, String> {
    let mut report = String::from(
        "Debug build. Seek completion = seeking=false plus target position; NOT display latency.\n",
    );
    for file in ["h264-1080p.mp4", "hevc-4k.mp4"] {
        for hardware in [false, true] {
            let path = std::path::Path::new(directory).join(file);
            media.request(
                Action::Open {
                    path: path.to_string_lossy().into_owned(),
                    hardware,
                },
                0,
            )?;
            let session = wait(media, |s| s.loaded && s.rendered_frames > 0)?.session;
            media.request(Action::Seek { seconds: 0.0 }, session)?;
            let decoder = wait(media, |s| {
                s.loaded
                    && s.position_seconds.is_some_and(|t| t < 0.034)
                    && s.hardware_decoder.as_deref().is_some_and(|d| {
                        if hardware {
                            d != "no" && !d.is_empty()
                        } else {
                            d == "no"
                        }
                    })
            })?
            .hardware_decoder;
            let mut seeks = Vec::new();
            for seconds in [1.0, 8.0, 5.0, 6.0, 2.0, 10.0, 0.0] {
                let start = Instant::now();
                media.request(Action::Seek { seconds }, session)?;
                wait(media, |s| {
                    s.position_seconds
                        .is_some_and(|t| (t - seconds).abs() < 0.025)
                })?;
                seeks.push(start.elapsed().as_secs_f64() * 1000.0);
            }
            eprintln!("VSF PLAYBACK: {file} hardware={hardware} seek pre-cut");
            // Step across the synthetic color cut at 6 seconds, then back.
            media.request(
                Action::Seek {
                    seconds: 6.0 - 1.0 / 30.0,
                },
                session,
            )?;
            wait(media, |s| {
                s.position_seconds
                    .is_some_and(|t| (t - (6.0 - 1.0 / 30.0)).abs() < 0.01)
            })?;
            eprintln!("VSF PLAYBACK: step forward");
            media.request(Action::Step { backwards: false }, session)?;
            wait(media, |s| {
                s.position_seconds.is_some_and(|t| (t - 6.0).abs() < 0.01)
            })?;
            eprintln!("VSF PLAYBACK: step backward");
            media.request(Action::Step { backwards: true }, session)?;
            wait(media, |s| {
                s.position_seconds
                    .is_some_and(|t| (t - (6.0 - 1.0 / 30.0)).abs() < 0.01)
            })?;
            media.request(Action::Seek { seconds: 0.0 }, session)?;
            let before = wait(media, |s| s.position_seconds.is_some_and(|t| t < 0.034))?;
            media.request(Action::Pause { paused: false }, session)?;
            let start = Instant::now();
            while start.elapsed() < Duration::from_secs(10) {
                let status = media.request(Action::Status, session)?;
                if status.hardware_decoder != decoder {
                    return Err(format!(
                        "{file}: decoder changed during playback: {:?}",
                        status.hardware_decoder
                    ));
                }
                if let Some(error) = status.error {
                    return Err(error);
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            let after = media.request(Action::Pause { paused: true }, session)?;
            if after.position_seconds.unwrap_or(0.0) < 9.0
                || after.rendered_frames <= before.rendered_frames
            {
                return Err(format!("{file}: playback failed to advance"));
            }
            let line = format!(
                "{file} requested_hardware={hardware} actual={decoder:?} seek_ms={seeks:.1?} playback_position={:?} swaps={} decoder_drops={:?} output_drops={:?}\n",
                after.position_seconds,
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
            eprintln!("VSF PLAYBACK: {line}");
            report.push_str(&line);
        }
    }
    Ok(report)
}
