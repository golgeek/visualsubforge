//! Full parse + accepted style/text/time edit validation + source serialization.
//! Input bytes are loaded before RSS baseline; no parsed document is prebuilt.
use sha2::{Digest, Sha256};
use std::{
    io::{self, Write},
    time::{Duration, Instant},
};
use subtitle_core::{CueId, Edit, Editor, Milliseconds};
use vsf_application::subtitle_file;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/generated/ass-ssa");
    let fixtures = ["ass", "ssa"]
        .into_iter()
        .map(|extension| {
            let bytes = std::fs::read(root.join(format!("10000.{extension}")))?;
            Ok((extension, bytes))
        })
        .collect::<Result<Vec<_>, std::io::Error>>()?;
    println!(
        "fixture_ready pid={} cues=10000 input_bytes={}",
        std::process::id(),
        fixtures.iter().map(|(_, bytes)| bytes.len()).sum::<usize>()
    );
    io::stdout().flush()?;
    std::thread::sleep(Duration::from_millis(500));
    let mut workloads = vec![];
    for (extension, original) in fixtures {
        let mut times = vec![];
        for run in 0..21 {
            let start = Instant::now();
            let parsed = subtitle_file::parse_document(&original, Some(extension))?;
            assert_eq!(parsed.document.cues().len(), 10000);
            let source = parsed.source;
            let mut editor = Editor::new(parsed.document);
            editor.apply_edit(
                0,
                Edit::StyledText {
                    id: CueId(1),
                    start: Milliseconds::new(10)?,
                    end: Milliseconds::new(1000)?,
                    text: "{\\i1}Edited 世界\\NSecond line".into(),
                    style: "Title".into(),
                },
                |document| subtitle_file::validate_document(document, &source),
            )?;
            let output = source.serialize(editor.document())?;
            let elapsed = start.elapsed().as_secs_f64() * 1000.;
            assert!(output.contains("Edited 世界"));
            editor.undo(1)?;
            assert_eq!(source.serialize(editor.document())?.as_bytes(), &original);
            if run > 0 {
                times.push(elapsed);
            }
        }
        times.sort_by(f64::total_cmp);
        workloads.push(serde_json::json!({
            "format": extension, "cues": 10000, "bytes": original.len(),
            "sha256": format!("{:x}", Sha256::digest(&original)),
            "runs": 20, "p95_ms": times[18], "maximum_ms": times[19],
        }));
    }
    let passed = workloads
        .iter()
        .all(|workload| workload["p95_ms"].as_f64().unwrap() < 500.);
    println!(
        "{}",
        serde_json::json!({"cpu_budget_ms":500, "workloads":workloads, "passed":passed})
    );
    if !passed {
        return Err("ASS/SSA parse/edit/serialize exceeded declared CPU budget".into());
    }
    Ok(())
}
