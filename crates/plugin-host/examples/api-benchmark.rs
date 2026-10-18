//! Full pure plugin API/history workload through a fresh real helper per run.
//! No desktop owner, recovery IO or native workflow is exercised here.
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::{self, Write},
    path::Path,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};
use subtitle_core::Editor;
use vsf_application::{
    plugins::{Api, Capture, Context, Key, Manifest, Mode, Package, Preferences, Request},
    subtitle_file,
};

fn exercise(
    bytes: &[u8],
    extension: &str,
    api: Api,
    mode: Mode,
    worker: &Path,
) -> Result<(), String> {
    let parsed = subtitle_file::parse_document(bytes, Some(extension))?;
    assert_eq!(parsed.document.cues().len(), 10_000);
    let mut editor = Editor::with_properties(parsed.document.clone(), parsed.source.clone());
    let request = Request {
        key: Key {
            generation: 1,
            revision: 0,
            context: 1,
        },
        mode,
        scope: parsed.document.cues().iter().map(|c| c.id()).collect(),
        settings: BTreeMap::new(),
        context: Context {
            selection: vec![],
            cursor: None,
            preferences: Preferences {
                minimum_blank_ms: 80,
                minimum_duration_ms: 500,
                maximum_duration_ms: 6000,
                cps_target: 20.0,
                rs_target_index: None,
            },
            scenes: None,
        },
    };
    let shift = if extension == "vtt" { 0 } else { 10 };
    let script = match api {
        Api::Vsf1 => format!(
            "VSFPlugin={{check(c,h){{if(h.getCue(c.id).text!==c.text)throw Error('snapshot');return 'append'}},fix(c,h){{h.update(c.id,{{text:c.text+'!',startMs:c.startMs+{shift},endMs:c.endMs+{shift}}})}}}};"
        ),
        Api::Vss1 => format!(
            "VSSPlugin={{Name:'Benchmark',HasError(c){{if(VSSCore.GetSubAt(c.Index-1)!==c)throw Error('identity');return 'append'}},FixError(c){{c.Text+='!';c.Start+={shift};c.Stop+={shift};}}}};"
        ),
    };
    let package = Package {
        manifest: Manifest {
            version: 1,
            id: "vsf.benchmark".into(),
            name: "Benchmark".into(),
            api,
            entry: "main.js".into(),
            requires: vec!["rules".into(), "fixes".into()],
            settings: BTreeMap::new(),
        },
        files: BTreeMap::from([("main.js".into(), script)]),
    };
    let capture = Capture::new(
        parsed.document.clone(),
        parsed.source.clone(),
        package,
        request.clone(),
        || Ok(()),
    )?;
    let execution = capture.execution(|| Ok(()))?;
    let result = vsf_plugin_host::run(
        worker,
        &[],
        &vsf_plugin_host::Request::new(execution.script, execution.input),
        &AtomicBool::new(false),
    )
    .map_err(|e| e.to_string())?;
    let proposal = capture.validate(result, || Ok(()))?;
    assert_eq!(proposal.findings().len(), 10_000);
    for (index, finding) in proposal.findings().iter().enumerate() {
        assert_eq!(
            finding.cue_id,
            parsed.document.cues()[index].id().0.to_string()
        );
        assert_eq!(finding.message, "append");
    }
    if mode == Mode::Check {
        assert!(proposal.rows().is_empty());
        assert!(!editor.dirty());
        assert!(!editor.can_undo());
    } else {
        assert_eq!(proposal.rows().len(), 10_000);
        for (index, row) in proposal.rows().iter().enumerate() {
            assert_eq!(row.cue_index, index);
            assert!(!row.outside_scope);
            // The dense SRT fixture contains karaoke overrides, which the
            // existing text projection requires users to review even when the
            // unchanged override tokens remain present. Other fixtures are
            // unambiguous for this append-only text change.
            assert_eq!(row.warning.is_some(), extension == "srt");
            assert_eq!(row.after.text, format!("{}!", row.before.text));
            assert_eq!(row.after.start.get(), row.before.start.get() + shift);
            assert_eq!(row.after.end.get(), row.before.end.get() + shift);
        }
        let edit = proposal.edit(editor.document(), editor.properties(), &request)?;
        editor.apply_edit_with_properties(0, edit, subtitle_file::validate_document)?;
        let saved = editor.properties().serialize(editor.document())?;
        let reopened = subtitle_file::parse_document(saved.as_bytes(), Some(extension))?;
        assert_eq!(&reopened.document, editor.document());
        assert_eq!(editor.properties(), &parsed.source);
        for (before, after) in parsed.document.cues().iter().zip(editor.document().cues()) {
            assert_eq!(before.metadata(), after.metadata());
            assert_eq!(before.id(), after.id());
        }
        editor.undo(1)?;
        assert_eq!(editor.document(), &parsed.document);
        assert!(!editor.dirty());
        assert!(!editor.can_undo());
        assert_eq!(
            editor.properties().serialize(editor.document())?.as_bytes(),
            bytes
        );
        editor.redo(2)?;
        assert_eq!(editor.properties().serialize(editor.document())?, saved);
        let mut stale = request.clone();
        stale.key.revision = editor.revision();
        assert!(
            proposal
                .edit(editor.document(), editor.properties(), &stale)
                .is_err()
        );
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/generated");
    let worker = std::env::current_exe()?
        .parent()
        .ok_or("Missing examples directory")?
        .parent()
        .ok_or("Missing release directory")?
        .join(format!("vsf-plugin-worker{}", std::env::consts::EXE_SUFFIX));
    let dense=(1..=10_000).map(|i|format!("{i}\n00:00:01,000 --> 00:00:05,000\n{{\\an8\\k20}}<i>Été 世界 😀 {i}</i>\nSecond line\n\n")).collect::<String>().into_bytes();
    let mut fixtures = vec![("dense-srt", "srt", dense)];
    for (name, extension, path) in [
        ("ass", "ass", "ass-ssa/10000.ass"),
        ("ssa", "ssa", "ass-ssa/10000.ssa"),
        ("styled-vtt", "vtt", "webvtt/10000-styled.vtt"),
        ("timed-vtt", "vtt", "webvtt/10000-timed.vtt"),
    ] {
        fixtures.push((name, extension, std::fs::read(root.join(path))?));
    }
    println!(
        "fixture_ready 10000 cues, {} input bytes",
        fixtures.iter().map(|f| f.2.len()).sum::<usize>()
    );
    io::stdout().flush()?;
    std::thread::sleep(Duration::from_millis(500));
    let mut reports = vec![];
    for (name, extension, bytes) in fixtures {
        for api in [Api::Vsf1, Api::Vss1] {
            for mode in [Mode::Check, Mode::Fix] {
                let label = format!("{name}-{api:?}-{mode:?}");
                println!("phase {label}");
                io::stdout().flush()?;
                let mut times = vec![];
                for iteration in 0..21 {
                    let start = Instant::now();
                    exercise(&bytes, extension, api, mode, &worker)?;
                    if iteration > 0 {
                        times.push(start.elapsed().as_secs_f64() * 1000.0);
                    }
                }
                times.sort_by(f64::total_cmp);
                reports.push(json!({"name":label,"input_sha256":format!("{:x}",Sha256::digest(&bytes)),"input_bytes":bytes.len(),"cues":10000,"iterations":20,"warmups":1,"p95_ms":times[18],"max_ms":times[19],"passed":times[18]<=500.0}));
            }
        }
    }
    let passed = reports.iter().all(|r| r["passed"] == true);
    println!(
        "{}",
        json!({"schema":1,"scope":"Input parse, capture/package validation, real helper modern/VSS API, all findings/edits/metadata validation; fix includes acceptance, serialize/reparse, one Undo/Redo, stale refusal. No desktop/IO/recovery/native acceptance.","workloads":reports,"p95_budget_ms":500,"passed":passed})
    );
    if !passed {
        return Err("Plugin API CPU budget failed".into());
    }
    Ok(())
}
