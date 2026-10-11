//! Desktop-only memory observations, not leak detection or a memory budget.
use crate::media::{Action, Media};
use crate::media_smoke::wait;
use std::time::Duration;
use tauri::Manager;

pub fn start(app: tauri::AppHandle, directory: String) {
    tauri::async_runtime::spawn_blocking(move || {
        let mut report = format!(
            "pid={} debug process RSS in KiB; excludes WebKit child processes; not physical footprint or leak detection.\n",
            std::process::id()
        );
        let result = run(&app.state::<Media>(), &directory, &mut report);
        report.push_str(&format!("result={result:?}\n"));
        let output = std::path::Path::new(&directory).join(format!(
            "memory-{}-results.txt",
            std::env::var("VSF_MEMORY_MODE").unwrap_or_else(|_| "both".into())
        ));
        let saved = std::fs::write(output, &report);
        if let Err(error) = &saved {
            eprintln!("VSF MEMORY: report write failed: {error}");
        }
        eprintln!("VSF MEMORY: {report}");
        crate::diagnostic_exit(&app, result.is_ok() && saved.is_ok());
    });
}

fn snapshot(label: &str, report: &mut String) -> Result<(), String> {
    // This diagnostic already runs on a blocking worker, never the UI thread.
    // Sample after the same delay at each closed checkpoint; allocator/driver
    // caches may remain resident, so RSS cannot identify leaked allocations.
    std::thread::sleep(Duration::from_secs(2));
    let output = std::process::Command::new("/bin/ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .map_err(|error| format!("Cannot sample RSS: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "RSS sampler failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let rss = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u64>()
        .map_err(|error| format!("Invalid RSS sample: {error}"))?;
    let line = format!("{label} rss_kib={rss}\n");
    eprint!("VSF MEMORY: {line}");
    report.push_str(&line);
    Ok(())
}

fn cycle(media: &Media, path: &str, hardware: bool) -> Result<(), String> {
    let session = media
        .request(
            Action::Open {
                path: path.into(),
                hardware,
            },
            0,
        )?
        .session;
    wait(media, |s| {
        s.session == session && s.loaded && s.rendered_frames > 0
    })?;
    media.request(Action::Seek { seconds: 5.0 }, session)?;
    wait(media, |s| {
        s.session == session
            && s.position_seconds.is_some_and(|t| (t - 5.0).abs() < 0.02)
            && s.hardware_decoder.as_deref().is_some_and(|decoder| {
                if hardware {
                    decoder != "no" && !decoder.is_empty()
                } else {
                    decoder == "no"
                }
            })
    })?;
    let closed = media.request(Action::Close, session)?;
    if closed.loaded || closed.error.is_some() {
        return Err("Close did not clear media state".into());
    }
    Ok(())
}

fn run(media: &Media, directory: &str, report: &mut String) -> Result<(), String> {
    let path = std::path::Path::new(directory).join("hevc-4k.mp4");
    let path = path.to_str().ok_or("Fixture path is not UTF-8")?;
    let alternate = std::env::var("VSF_MEMORY_ALTERNATE").ok();
    report.push_str(&format!("alternate_fixture={alternate:?}\n"));
    snapshot("initial_closed", report)?;
    let modes = match std::env::var("VSF_MEMORY_MODE").as_deref() {
        Ok("hardware") => vec![true],
        Ok("software") => vec![false],
        Err(_) => vec![false, true],
        _ => return Err("VSF_MEMORY_MODE must be hardware or software".into()),
    };
    let cycles = match std::env::var("VSF_MEMORY_CYCLES") {
        Ok(value) => value.parse::<u32>().map_err(|_| "Invalid cycle count")?,
        Err(_) => 50,
    };
    if !(1..=500).contains(&cycles) {
        return Err("Cycle count must be 1..500".into());
    }
    for hardware in modes {
        // Warm each decoder before comparing checkpoints within that mode.
        for count in 1..=2 {
            cycle(
                media,
                if count % 2 == 0 {
                    alternate.as_deref().unwrap_or(path)
                } else {
                    path
                },
                hardware,
            )?;
        }
        snapshot(&format!("hardware={hardware} warm_closed"), report)?;
        for count in 1..=cycles {
            cycle(
                media,
                if count % 2 == 0 {
                    alternate.as_deref().unwrap_or(path)
                } else {
                    path
                },
                hardware,
            )?;
            if count % 10 == 0 || count == cycles {
                snapshot(
                    &format!("hardware={hardware} cycles={count} closed"),
                    report,
                )?;
            }
        }
    }
    let summary = std::process::Command::new("/usr/bin/vmmap")
        .args(["-summary", &std::process::id().to_string()])
        .output()
        .map_err(|e| format!("vmmap failed: {e}"))?;
    report.push_str(&format!(
        "vmmap status={}\n{}\n{}\n",
        summary.status,
        String::from_utf8_lossy(&summary.stdout),
        String::from_utf8_lossy(&summary.stderr)
    ));
    // Leave media closed so an external allocation scan sees the same state.
    Ok(())
}
