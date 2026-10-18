//! Uniform proposal + acceptance validation + Undo on unchanged Task016 fixtures.
use sha2::{Digest, Sha256};
use std::{
    io::{self, Write},
    time::{Duration, Instant},
};
use subtitle_core::Editor;
use vsf_application::{
    batch_timing::{self, Change, Request, Scope},
    subtitle_file,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/generated/ass-ssa");
    let fixtures = ["ass", "ssa"]
        .into_iter()
        .map(|extension| {
            let bytes = std::fs::read(root.join(format!("10000.{extension}")))?;
            let parsed = subtitle_file::parse_document(&bytes, Some(extension))
                .map_err(std::io::Error::other)?;
            Ok((extension, bytes, parsed))
        })
        .collect::<Result<Vec<_>, std::io::Error>>()?;
    println!("fixture_ready pid={} cues=10000", std::process::id());
    io::stdout().flush()?;
    std::thread::sleep(Duration::from_millis(500));
    let mut workloads = vec![];
    for (extension, original, parsed) in fixtures {
        for selected in [false, true] {
            let scope = if selected {
                Scope::Selected(
                    parsed
                        .document
                        .cues()
                        .iter()
                        .step_by(2)
                        .map(|c| c.id())
                        .collect(),
                )
            } else {
                Scope::All
            };
            let mut times = vec![];
            for run in 0..21 {
                let start = Instant::now();
                let p = batch_timing::propose(
                    parsed.document.clone(),
                    parsed.source.format(),
                    Request {
                        scope: scope.clone(),
                        change: Change::Shift(120),
                    },
                    || Ok(()),
                )?;
                let mut editor = Editor::new(parsed.document.clone());
                editor.apply_edit(0, p.edit(editor.document())?, |d| {
                    subtitle_file::validate_document(d, &parsed.source)
                })?;
                let output = parsed.source.serialize(editor.document())?;
                editor.undo(1)?;
                let elapsed = start.elapsed().as_secs_f64() * 1000.;
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
            workloads.push(serde_json::json!({"format":extension, "scope":if selected {"disjoint-half"} else {"all"}, "cues":10000, "sha256":format!("{:x}", Sha256::digest(&original)), "runs":20, "p95_ms":times[18], "maximum_ms":times[19]}));
        }
    }
    let passed = workloads
        .iter()
        .all(|w| w["p95_ms"].as_f64().unwrap() < 500.);
    println!(
        "{}",
        serde_json::json!({"cpu_budget_ms":500, "workloads":workloads, "passed":passed})
    );
    if !passed {
        return Err("Batch proposal exceeded declared CPU budget".into());
    }
    Ok(())
}
