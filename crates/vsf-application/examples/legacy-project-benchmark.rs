//! Read-only legacy project inspection, including actual bounded file reads and
//! full subtitle parsing/size validation. Generated inputs precede RSS baseline.
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{self, Write},
    path::Path,
    time::{Duration, Instant},
};
use vsf_application::legacy_project::{self, Overrides};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/generated");
    let temp = tempfile::tempdir()?;
    fs::write(
        temp.path().join("clip.mkv"),
        b"independently authored media identity placeholder",
    )?;
    let mut fixtures = vec![];
    for (kind, relative, extension) in [
        ("plain", "webvtt/10000-plain.vtt", "vtt"),
        ("styled", "webvtt/10000-styled.vtt", "vtt"),
        ("timed", "webvtt/10000-timed.vtt", "vtt"),
        ("ass", "ass-ssa/10000.ass", "ass"),
    ] {
        let original = fs::read(root.join(relative))?;
        let subtitles = temp.path().join(format!("{kind}.{extension}"));
        fs::write(&subtitles, &original)?;
        let source = temp.path().join(format!("{kind}.vssprj"));
        let text = format!(
            "[VisualSubsync]\r\nVideoSource=clip.mkv\r\nSubtitlesFile={kind}.{extension}\r\nFocusedTimeMs=1501\r\nWAVDisplayerPositionStartMs=1000\r\nWAVDisplayerPositionStopMs=16000\r\n"
        );
        let mut project_bytes = vec![0xff, 0xfe];
        for word in text.encode_utf16() {
            project_bytes.extend(word.to_le_bytes());
        }
        fs::write(&source, &project_bytes)?;
        fixtures.push((kind, source, subtitles, original, project_bytes));
    }
    println!(
        "fixture_ready pid={} cues=10000 input_bytes={}",
        std::process::id(),
        fixtures
            .iter()
            .map(|f| f.3.len() + f.4.len())
            .sum::<usize>()
    );
    io::stdout().flush()?;
    std::thread::sleep(Duration::from_millis(500));
    let mut workloads = vec![];
    for (kind, source, subtitles, original, project_bytes) in fixtures {
        println!("phase {kind}");
        io::stdout().flush()?;
        let mut times = vec![];
        for run in 0..21 {
            let start = Instant::now();
            let result = legacy_project::inspect(&source, &Overrides::default(), || Ok(()))?;
            assert!(result.ready());
            assert_eq!(result.subtitle_cues, Some(10_000));
            assert_eq!(result.source_bytes, project_bytes);
            assert_eq!(result.subtitle_bytes.as_ref(), Some(&original));
            assert_eq!(result.project.view.focused_ms, Some(1501));
            legacy_project::verify_text_inputs(&result, || Ok(()))?;
            assert_eq!(fs::read(&source)?, project_bytes);
            assert_eq!(fs::read(&subtitles)?, original);
            let elapsed = start.elapsed().as_secs_f64() * 1000.;
            if run > 0 {
                times.push(elapsed);
            }
        }
        times.sort_by(f64::total_cmp);
        workloads.push(serde_json::json!({
            "workload": kind, "cues": 10_000, "subtitle_bytes": original.len(),
            "sha256": format!("{:x}", Sha256::digest(&original)),
            "project_sha256": format!("{:x}", Sha256::digest(&project_bytes)),
            "runs": 20, "p95_ms": times[18], "maximum_ms": times[19],
        }));
    }
    let passed = workloads
        .iter()
        .all(|w| w["p95_ms"].as_f64().is_some_and(|ms| ms <= 500.));
    println!(
        "{}",
        serde_json::json!({"cpu_budget_ms": 500, "workloads": workloads, "passed": passed, "scope": "Application inspection and input revalidation with IO, exact input preservation; no desktop, media decoding, publication or history mutation"})
    );
    if !passed {
        return Err("Legacy project inspection exceeded the declared budget".into());
    }
    Ok(())
}
