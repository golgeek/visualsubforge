//! Immutable reference preparation, reading, matches and independent target
//! history. Fixture IO/generation precedes RSS sampling; parsed state is measured.
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{self, Write},
    path::Path,
    time::{Duration, Instant},
};
use subtitle_core::{Edit, Editor, Milliseconds};
use vsf_application::{
    subtitle_file,
    translation::{Offset, PageRequest, Reference},
};

fn page(offset: usize) -> PageRequest {
    PageRequest { offset, limit: 200 }
}

fn exercise(original: &[u8], extension: &str, dense: bool) -> Result<(), String> {
    let reference = Reference::parse(original.to_vec(), Some(extension), || Ok(()))?;
    assert_eq!(reference.cues().len(), 10_000);
    assert_eq!(reference.original_bytes(), original);
    assert_eq!(
        reference.fingerprint(),
        format!("{:x}", Sha256::digest(original))
    );
    for offset in (0..10_000).step_by(200) {
        let rows = reference.browse(Offset::default(), page(offset), || Ok(()))?;
        assert_eq!(rows.total, 10_000);
        assert_eq!(rows.rows.len(), 200);
        for (index, row) in rows.rows.into_iter().enumerate() {
            assert_eq!(row.source_index, offset + index);
            assert!(!reference.reading(row.source_index)?.text.is_empty());
        }
    }
    if dense {
        // All 10k cues overlap; check all 50 pages, rather than hiding truncated
        // results behind a short page or running 100m redundant identical pairs.
        for offset in (0..10_000).step_by(200) {
            let rows = reference.matches(
                Milliseconds::new(2000)?,
                Milliseconds::new(3000)?,
                Offset::new(-500)?,
                page(offset),
                || Ok(()),
            )?;
            assert_eq!(rows.total, 10_000);
            assert_eq!(rows.rows.len(), 200);
            for (index, row) in rows.rows.into_iter().enumerate() {
                assert_eq!(row.source_index, offset + index);
                assert_eq!(row.overlap_ms, Some(1000));
            }
        }
        let mut checkpoints = 0;
        let result = reference.matches(
            Milliseconds::new(2000)?,
            Milliseconds::new(3000)?,
            Offset::default(),
            page(0),
            || {
                checkpoints += 1;
                if checkpoints == 3 {
                    Err("cancelled".into())
                } else {
                    Ok(())
                }
            },
        );
        assert_eq!(result.unwrap_err(), "cancelled");
    } else {
        // These pinned fixtures are disjoint, so each accepted interval must
        // match exactly itself. This is a complete all-target-cue query pass.
        for (index, cue) in reference.cues().iter().enumerate() {
            let rows = reference.matches(
                cue.start(),
                cue.end(),
                Offset::default(),
                page(0),
                || Ok(()),
            )?;
            assert_eq!(rows.total, 1);
            assert_eq!(rows.rows[0].source_index, index);
            assert_eq!(
                rows.rows[0].overlap_ms,
                Some(cue.end().get() - cue.start().get())
            );
        }
    }
    let target = reference.copy_target();
    let mut editor = Editor::with_properties(target.document, target.source);
    let last = editor.document().cues().last().ok_or("missing last cue")?;
    let edit = Edit::Text {
        id: last.id(),
        start: last.start(),
        end: last.end(),
        text: format!("{} — traduit", last.text()),
    };
    editor.apply_edit_with_properties(0, edit, subtitle_file::validate_document)?;
    let output = editor.properties().serialize(editor.document())?;
    let reparsed = subtitle_file::parse_document(output.as_bytes(), Some(extension))?;
    assert_eq!(&reparsed.document, editor.document());
    drop(reparsed);
    editor.undo(editor.revision())?;
    assert!(!editor.dirty());
    assert_eq!(
        editor.properties().serialize(editor.document())?.as_bytes(),
        original
    );
    editor.redo(editor.revision())?;
    assert_eq!(editor.properties().serialize(editor.document())?, output);
    assert_eq!(reference.original_bytes(), original);
    assert_ne!(
        editor.document().cues()[9999].text(),
        reference.cues()[9999].text()
    );
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/generated");
    let mut fixtures = vec![];
    for (kind, relative, extension, expected_sha) in [
        (
            "plain",
            "webvtt/10000-plain.vtt",
            "vtt",
            "337cffd83d4ad10fc053bd565f96b4cbc77fd7c817db39fc40e6d254a5d09198",
        ),
        (
            "styled",
            "webvtt/10000-styled.vtt",
            "vtt",
            "783a7ad89406073e0cb6512c9ca9daaac9b7c09baeaa933dada2bd0e67aa339f",
        ),
        (
            "timed",
            "webvtt/10000-timed.vtt",
            "vtt",
            "46430c8e7b374e2bd6ccb3d4bd95cf3489d2571f00ff8c9120c9c59ce5e1c1e6",
        ),
        (
            "ass",
            "ass-ssa/10000.ass",
            "ass",
            "00e4deb1210c47e2943c30744da0e5d0512a57ba159b659d9dd16ef5d47c1c0c",
        ),
    ] {
        let bytes = fs::read(root.join(relative))?;
        assert_eq!(format!("{:x}", Sha256::digest(&bytes)), expected_sha);
        fixtures.push((kind, extension, bytes));
    }
    let dense = (1..=10_000)
        .map(|index| {
            format!(
                "{index}\n00:00:01,000 --> 00:00:05,000\n{{\\an8\\k20}}<i>Été 世界 {index}</i>\n\n"
            )
        })
        .collect::<String>()
        .into_bytes();
    fixtures.push(("dense", "srt", dense));
    println!(
        "fixture_ready pid={} cues=10000 input_bytes={}",
        std::process::id(),
        fixtures.iter().map(|f| f.2.len()).sum::<usize>()
    );
    io::stdout().flush()?;
    std::thread::sleep(Duration::from_millis(500));
    let mut workloads = vec![];
    for (kind, extension, original) in fixtures {
        println!("phase {kind}");
        io::stdout().flush()?;
        let mut times = vec![];
        for run in 0..21 {
            let start = Instant::now();
            exercise(&original, extension, kind == "dense")?;
            if run > 0 {
                times.push(start.elapsed().as_secs_f64() * 1000.);
            }
        }
        times.sort_by(f64::total_cmp);
        workloads.push(serde_json::json!({
            "workload": kind, "cues": 10_000, "bytes": original.len(),
            "sha256": format!("{:x}", Sha256::digest(&original)),
            "runs": 20, "p95_ms": times[18], "maximum_ms": times[19],
        }));
    }
    let passed = workloads
        .iter()
        .all(|w| w["p95_ms"].as_f64().is_some_and(|ms| ms <= 500.));
    println!(
        "{}",
        serde_json::json!({
            "cpu_budget_ms": 500, "workloads": workloads, "passed": passed,
            "scope": "Application reference preparation, all-cue reading/matching or dense paging/cancel, independent target edit/reopen/Undo/Redo; excludes desktop and project/recovery IO"
        })
    );
    if !passed {
        return Err("Translation foundation exceeded the declared CPU budget".into());
    }
    Ok(())
}
