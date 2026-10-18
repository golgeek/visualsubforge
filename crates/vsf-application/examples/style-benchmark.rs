//! Style create/edit/rename proposals plus atomic acceptance and serialization.
//! Original inputs are resident before RSS baseline; parsing/history are included
//! in memory accounting. Timings cover proposal, acceptance and serialization.
use sha2::{Digest, Sha256};
use std::{
    io::{self, Write},
    time::{Duration, Instant},
};
use subtitle_core::Editor;
use vsf_application::{
    authoring::styles::{self, StyleFieldValue, StyleOperation},
    subtitle_file,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/generated/ass-ssa");
    let fixtures = ["ass", "ssa"]
        .into_iter()
        .map(|extension| {
            Ok((
                extension,
                std::fs::read(root.join(format!("10000.{extension}")))?,
            ))
        })
        .collect::<Result<Vec<_>, std::io::Error>>()?;
    println!("fixture_ready pid={} cues=10000", std::process::id());
    io::stdout().flush()?;
    std::thread::sleep(Duration::from_millis(500));
    let mut workloads = vec![];
    for (extension, bytes) in fixtures {
        let parsed = subtitle_file::parse_document(&bytes, Some(extension))?;
        assert_eq!(parsed.document.cues().len(), 10000);
        for (name, operation) in [
            (
                "create",
                StyleOperation::Create {
                    name: "Copied".into(),
                    copy_from: Some("Title".into()),
                    fields: vec![],
                },
            ),
            (
                "edit",
                StyleOperation::Edit {
                    name: "Title".into(),
                    fields: vec![StyleFieldValue {
                        name: "Fontsize".into(),
                        value: "42".into(),
                    }],
                },
            ),
            (
                "rename",
                StyleOperation::Edit {
                    name: "Title".into(),
                    fields: vec![StyleFieldValue {
                        name: "Name".into(),
                        value: "Heading".into(),
                    }],
                },
            ),
        ] {
            println!("phase {extension}-{name}");
            io::stdout().flush()?;
            let mut times = vec![];
            for run in 0..21 {
                let mut editor =
                    Editor::with_properties(parsed.document.clone(), parsed.source.clone());
                let began = Instant::now();
                let proposal = styles::propose(
                    editor.document(),
                    editor.properties(),
                    &operation,
                    || Ok(()),
                )?;
                assert!(proposal.changed);
                assert_eq!(
                    proposal.summary.changed_cues,
                    if name == "rename" { 2500 } else { 0 }
                );
                let (document, source) = proposal.into_parts();
                editor.apply_document(0, document, source, subtitle_file::validate_document)?;
                let output = editor.properties().serialize(editor.document())?;
                let elapsed = began.elapsed().as_secs_f64() * 1000.;
                assert!(output.contains(if name == "create" {
                    "Style: Copied,"
                } else if name == "rename" {
                    "Style: Heading,"
                } else {
                    "Title,sans-serif,42"
                }));
                editor.undo(1)?;
                assert_eq!(
                    editor.properties().serialize(editor.document())?.as_bytes(),
                    bytes
                );
                editor.redo(2)?;
                assert_eq!(editor.properties().serialize(editor.document())?, output);
                if run > 0 {
                    times.push(elapsed);
                }
            }
            times.sort_by(f64::total_cmp);
            workloads.push(serde_json::json!({"format": extension, "operation": name, "cues": 10000, "bytes": bytes.len(), "sha256": format!("{:x}", Sha256::digest(&bytes)), "runs": 20, "p95_ms": times[18], "maximum_ms": times[19]}));
        }
    }
    let passed = workloads
        .iter()
        .all(|v| v["p95_ms"].as_f64().unwrap() < 500.);
    println!(
        "{}",
        serde_json::json!({"cpu_budget_ms": 500, "workloads": workloads, "passed": passed})
    );
    if !passed {
        return Err("Style proposal/history/serialize CPU budget exceeded".into());
    }
    Ok(())
}
