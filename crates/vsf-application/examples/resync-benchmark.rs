//! Format-preserving 10k proposal, accepted edit, serialization and Undo cost.
use media_analysis::alignment::{Alignment, Segment};
use sha2::{Digest, Sha256};
use std::{
    io::{self, Write},
    time::{Duration, Instant},
};
use subtitle_core::{CueTiming, Edit, Editor, Milliseconds};
use vsf_application::{
    resync::{Budget, Cancellation, Proposal, SubtitleSnapshot},
    subtitle_file,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/generated/ass-ssa");
    let fixtures = ["ass", "ssa"]
        .into_iter()
        .map(|extension| {
            let bytes = std::fs::read(root.join(format!("10000.{extension}")))?;
            let parsed =
                subtitle_file::parse_document(&bytes, Some(extension)).map_err(io::Error::other)?;
            // Task016's original spans 4h10m, beyond the established resync cap.
            // Derive a 2h05m timing-only variant before observing RSS/CPU; keep every
            // cue and all source records, text, tags and metadata. Original stays on disk.
            let mut editor = Editor::new(parsed.document);
            let timings = editor
                .document()
                .cues()
                .iter()
                .map(|cue| CueTiming {
                    id: cue.id(),
                    start: Milliseconds::new(cue.start().get() / 2).unwrap(),
                    end: Milliseconds::new(cue.end().get() / 2).unwrap(),
                })
                .collect();
            editor
                .apply_edit(0, Edit::Retime { timings }, |_| Ok(()))
                .map_err(io::Error::other)?;
            let derived = parsed
                .source
                .serialize(editor.document())
                .map_err(io::Error::other)?;
            let parsed = subtitle_file::parse_document(derived.as_bytes(), Some(extension))
                .map_err(io::Error::other)?;
            Ok((extension, bytes, derived.into_bytes(), parsed))
        })
        .collect::<Result<Vec<_>, io::Error>>()?;
    println!("fixture_ready pid={} cues=10000", std::process::id());
    io::stdout().flush()?;
    std::thread::sleep(Duration::from_millis(500));
    let mut workloads = vec![];
    for (extension, parent_bytes, original, parsed) in fixtures {
        let mut times = vec![];
        for run in 0..21 {
            let start = Instant::now();
            let cancel = Cancellation::default();
            let p = Proposal::build(
                SubtitleSnapshot::new(parsed.document.clone(), parsed.source.clone())?,
                Alignment {
                    segments: vec![Segment {
                        source_start_us: 0,
                        source_end_us: 14_399_000_000,
                        target_start_us: 120_000,
                        target_end_us: 14_399_120_000,
                        anchors: 3,
                        max_residual_us: 0.,
                    }],
                    distinctive_matches: 3,
                    ambiguous_source_anchors: 0,
                    reordered: false,
                },
                &Budget::new(&cancel, Duration::from_secs(5)),
            )?;
            let mut editor = Editor::new(parsed.document.clone());
            editor.apply_edit(0, p.edit(editor.document())?, |d| {
                subtitle_file::validate_document(d, &parsed.source)
            })?;
            let output = parsed.source.serialize(editor.document())?;
            editor.undo(1)?;
            let elapsed = start.elapsed().as_secs_f64() * 1000.;
            assert_eq!(Some(output.as_str()), p.subtitles());
            assert_ne!(output.as_bytes(), &original);
            assert_eq!(
                parsed.source.serialize(editor.document())?.as_bytes(),
                &original
            );
            if run > 0 {
                times.push(elapsed);
            }
        }
        times.sort_by(f64::total_cmp);
        workloads.push(serde_json::json!({"format":extension,"cues":10000,"timing_variant":"source endpoints divided by two; all cues and text retained", "parent_sha256":format!("{:x}",Sha256::digest(&parent_bytes)), "sha256":format!("{:x}",Sha256::digest(&original)),"runs":20,"p95_ms":times[18],"maximum_ms":times[19]}));
    }
    let passed = workloads
        .iter()
        .all(|w| w["p95_ms"].as_f64().unwrap() < 500.);
    println!(
        "{}",
        serde_json::json!({"cpu_budget_ms":500,"workloads":workloads,"passed":passed})
    );
    if !passed {
        return Err("Resync proposal exceeded declared CPU budget".into());
    }
    Ok(())
}
