//! Opt-in desktop stress measurements. Native calls are not forcibly interruptible.
use crate::media::{Action, Media, Status};
use crate::media_smoke::wait;
use std::time::{Duration, Instant};
use tauri::Manager;

pub fn start(app: tauri::AppHandle, directory: String) {
    tauri::async_runtime::spawn_blocking(move || {
        let mut report = String::from(
            "Debug desktop run; latencies are API completion, not physical presentation.\n",
        );
        let result = run(&app.state::<Media>(), &directory, &mut report);
        report.push_str(&format!("result={result:?}\n"));
        let output = std::path::Path::new(&directory).join("soak-results.txt");
        let saved = std::fs::write(output, &report);
        if let Err(error) = &saved {
            eprintln!("VSF SOAK: report write failed: {error}");
        }
        eprintln!("VSF SOAK: {report}");
        crate::diagnostic_exit(&app, result.is_ok() && saved.is_ok());
    });
}
fn opened(media: &Media, path: &str, hardware: bool) -> Result<Status, String> {
    let requested = media
        .request(
            Action::Open {
                path: path.into(),
                hardware,
            },
            0,
        )?
        .session;
    wait(media, |s| {
        s.session == requested && s.loaded && s.rendered_frames > 0
    })
}
fn summary(name: &str, samples: &[f64]) -> String {
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    let rank = (95 * sorted.len()).div_ceil(100) - 1;
    format!(
        "{name}: n={} min_ms={:.3} median_ms={:.3} p95_ms={:.3} max_ms={:.3}\n",
        sorted.len(),
        sorted[0],
        sorted[sorted.len() / 2],
        sorted[rank],
        sorted[sorted.len() - 1]
    )
}
fn run(media: &Media, directory: &str, report: &mut String) -> Result<(), String> {
    // VM rendering/lifecycle coverage can be requested without claiming a
    // hardware-decoding pass. The default still requires both decoder modes.
    let software_only = std::env::var("VSF_MEDIA_SOAK_SOFTWARE_ONLY").as_deref() == Ok("1");
    report.push_str(&format!(
        "software_only={software_only}; hardware coverage={}\n",
        if software_only { "skipped" } else { "required" }
    ));
    let file = |name| {
        std::path::Path::new(directory)
            .join(name)
            .to_string_lossy()
            .into_owned()
    };
    let hevc = file("hevc-4k.mp4");
    let h264 = file("h264-1080p.mp4");
    for hardware in [false, true].into_iter().filter(|h| !software_only || !h) {
        let session = opened(media, &hevc, hardware)?.session;
        media.request(Action::Seek { seconds: 1.0 }, session)?;
        let before = wait(media, |s| {
            s.position_seconds.is_some_and(|t| (t - 1.0).abs() < 0.02)
                && s.hardware_decoder.as_deref().is_some_and(|d| {
                    if hardware {
                        d != "no" && !d.is_empty()
                    } else {
                        d == "no"
                    }
                })
        })?;
        media.request(
            Action::Loop {
                start: 1.0,
                end: 11.0,
            },
            session,
        )?;
        media.request(Action::Pause { paused: false }, session)?;
        let start = Instant::now();
        let mut last_change = Instant::now();
        let mut previous = 1.0;
        let mut wraps = 0;
        let mut requests = Vec::new();
        let mut next_progress = 15;
        while start.elapsed() < Duration::from_secs(60) {
            let request = Instant::now();
            let status = media.request(Action::Status, session)?;
            requests.push(request.elapsed().as_secs_f64() * 1000.0);
            if status.session != session
                || !status.loaded
                || status.hardware_decoder != before.hardware_decoder
            {
                return Err(format!(
                    "Soak state changed: session={} loaded={} decoder={:?} error={:?}",
                    status.session, status.loaded, status.hardware_decoder, status.error
                ));
            }
            if let Some(error) = status.error {
                return Err(error);
            }
            let position = status.position_seconds.ok_or("Missing playback position")?;
            if previous > position + 0.5 {
                wraps += 1;
            }
            if (position - previous).abs() > 0.005 {
                last_change = Instant::now();
            }
            if last_change.elapsed() > Duration::from_secs(2) {
                return Err("Playback stalled for two seconds".into());
            }
            previous = position;
            if start.elapsed().as_secs() >= next_progress {
                eprintln!(
                    "VSF SOAK: hardware={hardware} elapsed={}s wraps={wraps} rendered_frames={}",
                    start.elapsed().as_secs(),
                    status.rendered_frames - before.rendered_frames
                );
                next_progress += 15;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let after = media.request(Action::Pause { paused: true }, session)?;
        media.request(Action::ClearLoop, session)?;
        if wraps < 4 || after.rendered_frames <= before.rendered_frames {
            return Err("Repeated playback did not progress through four loops".into());
        }
        let line = format!(
            "HEVC 4K hardware={hardware} decoder={:?} elapsed_s={:.2} wraps={wraps} rendered_frames={} swap_counter_available={} decoder_drops={:?} output_drops={:?}\n{}",
            after.hardware_decoder,
            start.elapsed().as_secs_f64(),
            after.rendered_frames - before.rendered_frames,
            after.swap_counter_available,
            after
                .decoder_drops
                .zip(before.decoder_drops)
                .map(|(a, b)| a - b),
            after
                .output_drops
                .zip(before.output_drops)
                .map(|(a, b)| a - b),
            summary("status", &requests)
        );
        eprintln!("VSF SOAK: {line}");
        report.push_str(&line);
    }
    let mut replacements = Vec::new();
    let mut closes = Vec::new();
    for _ in 0..30 {
        let old = opened(media, &h264, !software_only)?.session;
        let start = Instant::now();
        let current = opened(media, &hevc, !software_only)?;
        replacements.push(start.elapsed().as_secs_f64() * 1000.0);
        if media.request(Action::Seek { seconds: 1.0 }, old).is_ok() {
            return Err("Stale session accepted after replacement".into());
        }
        let start = Instant::now();
        let closed = media.request(Action::Close, current.session)?;
        closes.push(start.elapsed().as_secs_f64() * 1000.0);
        if closed.loaded || closed.error.is_some() {
            return Err("Close did not reset state".into());
        }
    }
    report.push_str(&summary(
        "replace_H264_with_HEVC_to_loaded_render",
        &replacements,
    ));
    report.push_str(&summary("close", &closes));
    let mut cancellations = Vec::new();
    let mut superseded = 0;
    for _ in 0..30 {
        std::thread::scope(|scope| -> Result<(), String> {
            let previous = media.latest_generation();
            let opening = scope.spawn(|| {
                media.request(
                    Action::Open {
                        path: hevc.clone(),
                        hardware: !software_only,
                    },
                    0,
                )
            });
            let started = Instant::now();
            while media.latest_generation() == previous {
                if started.elapsed() > Duration::from_secs(2) {
                    return Err("Open did not reserve a session".into());
                }
                std::thread::yield_now();
            }
            let start = Instant::now();
            let closed = media.request(Action::Close, 0)?;
            cancellations.push(start.elapsed().as_secs_f64() * 1000.0);
            match opening.join().map_err(|_| "Open test thread panicked")? {
                Err(e) if e == "Superseded media session" => superseded += 1,
                Err(e) => return Err(e),
                Ok(_) => {} // Open can finish before Close; still must not resurrect it.
            }
            let after = media.request(Action::Status, 0)?;
            if after.loaded || after.session != closed.session || after.error.is_some() {
                return Err("Cancelled open resurfaced after Close".into());
            }
            Ok(())
        })?;
    }
    report.push_str(&summary(
        "close_after_open_generation_reserved",
        &cancellations,
    ));
    report.push_str(&format!("superseded_opens={superseded}/30; reservation does not prove native initialization had begun.\n"));
    opened(media, &hevc, !software_only)?; // leave a usable paused fixture
    Ok(())
}
