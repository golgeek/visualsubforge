//! Representative 10k font inventory/proposal/history/serialization gate.
use sha2::{Digest, Sha256};
use std::{
    io::{self, Write},
    time::{Duration, Instant},
};
use subtitle_core::Editor;
use vsf_application::{
    authoring::attachments::{self, FontEdit},
    subtitle_file,
};
const TTF: &[u8] = include_bytes!("../../../tests/fixtures/subtitles/fixture-font.ttf");
const OTF: &[u8] = include_bytes!("../../../tests/fixtures/subtitles/fixture-font-cff.otf");
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
        let attached = attachments::propose(
            &parsed.document,
            &parsed.source,
            FontEdit::Put {
                replace: None,
                name: "Fixture.ttf",
                bytes: TTF,
            },
            || Ok(()),
        )?;
        let id = attached.summary.after.as_ref().unwrap().id;
        for name in ["inspect", "add", "replace", "remove"] {
            let source = if name == "add" {
                &parsed.source
            } else {
                attached.source()
            };
            let original = source.serialize(&parsed.document)?;
            println!("phase {extension}-{name}");
            io::stdout().flush()?;
            let mut times = vec![];
            for run in 0..21 {
                let mut editor = Editor::with_properties(parsed.document.clone(), source.clone());
                let began = Instant::now();
                if name == "inspect" {
                    let inventory = attachments::inspect(editor.properties(), || Ok(()))?;
                    assert_eq!(inventory.total, 1);
                    assert_eq!(inventory.entries[0].decoded_bytes, Some(TTF.len() as u32));
                    if run > 0 {
                        times.push(began.elapsed().as_secs_f64() * 1000.);
                    }
                    continue;
                }
                let edit = match name {
                    "add" => FontEdit::Put {
                        replace: None,
                        name: "Fixture.ttf",
                        bytes: TTF,
                    },
                    "replace" => FontEdit::Put {
                        replace: Some(id),
                        name: "Changed.otf",
                        bytes: OTF,
                    },
                    _ => FontEdit::Remove { id },
                };
                let proposal =
                    attachments::propose(editor.document(), editor.properties(), edit, || Ok(()))?;
                assert!(proposal.changed);
                let (document, source) = proposal.into_parts();
                editor.apply_document(0, document, source, subtitle_file::validate_document)?;
                let output = editor.properties().serialize(editor.document())?;
                let elapsed = began.elapsed().as_secs_f64() * 1000.;
                assert_eq!(editor.document(), &parsed.document);
                assert_eq!(output.contains("fontname:"), name != "remove");
                editor.undo(1)?;
                assert_eq!(editor.properties().serialize(editor.document())?, original);
                editor.redo(2)?;
                assert_eq!(editor.properties().serialize(editor.document())?, output);
                if run > 0 {
                    times.push(elapsed);
                }
            }
            times.sort_by(f64::total_cmp);
            workloads.push(serde_json::json!({"format": extension, "operation": name, "cues": 10000, "corpus_sha256": format!("{:x}", Sha256::digest(&bytes)), "input_sha256": format!("{:x}", Sha256::digest(original.as_bytes())), "bytes": original.len(), "font_bytes": TTF.len(), "replacement_bytes": OTF.len(), "runs": 20, "p95_ms": times[18], "maximum_ms": times[19]}));
        }
    }
    let passed = workloads
        .iter()
        .all(|v| v["p95_ms"].as_f64().unwrap() <= 500.);
    println!(
        "{}",
        serde_json::json!({"cpu_budget_ms": 500, "workloads": workloads, "passed": passed})
    );
    if !passed {
        return Err("Font proposal/history/serialization CPU budget exceeded".into());
    }
    Ok(())
}
