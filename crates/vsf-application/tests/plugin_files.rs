//! Filesystem/publication tests inject a wire reply; CLI tests run the real engine.
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use vsf_application::{
    plugin_files::{self, Budget, Error, LoadedPackage, Options},
    plugins::{Execution, Mode},
    save_lease,
};

const SRT: &str = "1\n00:00:01,000 --> 00:00:02,000\n{\\i1}Été 😀{\\i0}  \n\n";
fn setup(root: &Path) -> (PathBuf, PathBuf) {
    let source = root.join("source.srt");
    fs::write(&source, SRT).unwrap();
    let package = root.join("package");
    fs::create_dir(&package).unwrap();
    let manifest = package.join("manifest.json");
    fs::write(
        &manifest,
        include_str!("../../../examples/plugins/trim-trailing/manifest.json"),
    )
    .unwrap();
    fs::write(
        package.join("main.js"),
        include_str!("../../../examples/plugins/trim-trailing/main.js"),
    )
    .unwrap();
    (source, manifest)
}
fn options() -> Options {
    Options {
        mode: Mode::Fix,
        scope: vec![],
        selection: vec![],
        settings: BTreeMap::new(),
        context_file: None,
    }
}
fn reply(execution: Execution) -> plugin_files::Result<Value> {
    let i = execution.input;
    let c = &i["cues"][0];
    Ok(
        json!({"version":1,"key":i["key"],"fingerprint":i["fingerprint"],"name":"test","canFix":true,"settings":{},"findings":[{"cueId":c["id"],"message":"trailing"}],"logs":[],"edits":[{"cueId":c["id"],"startMs":c["startMs"],"endMs":c["endMs"],"text":c["text"].as_str().unwrap().trim_end()}]}),
    )
}

#[test]
fn local_package_is_bounded_captured_and_verified_with_relative_helpers() {
    let root = tempfile::tempdir().unwrap();
    let (_, manifest) = setup(root.path());
    let package = manifest.parent().unwrap();
    fs::create_dir(package.join("common")).unwrap();
    fs::write(package.join("common/helper.js"), "var helper=1;").unwrap();
    fs::write(package.join("README.txt"), "ignored").unwrap();
    let cancel = AtomicBool::new(false);
    let budget = Budget::new(&cancel, Duration::from_secs(10));
    let loaded = LoadedPackage::read(&manifest, &budget).unwrap();
    assert_eq!(loaded.package().files.len(), 2);
    assert_eq!(loaded.package().files["common/helper.js"], "var helper=1;");
    assert_eq!(loaded.files().len(), 3);
    loaded.verify(&budget).unwrap();
    fs::write(package.join("common/helper.js"), "var helper=2;").unwrap();
    assert!(matches!(loaded.verify(&budget), Err(Error::Changed(_))));
    fs::write(package.join("common/helper.js"), "var helper=1;").unwrap();
    loaded.verify(&budget).unwrap();
    fs::write(package.join("extra.js"), "// newly captured dependency").unwrap();
    assert!(matches!(loaded.verify(&budget), Err(Error::Changed(_))));
    fs::remove_file(package.join("extra.js")).unwrap();
    let mut bytes = fs::read(&manifest).unwrap();
    bytes.push(b' ');
    fs::write(&manifest, bytes).unwrap();
    assert!(matches!(loaded.verify(&budget), Err(Error::Changed(_))));
    cancel.store(true, Ordering::Release);
    assert!(matches!(
        LoadedPackage::read(&manifest, &budget),
        Err(Error::Cancelled)
    ));
}

#[test]
fn package_file_counts_size_encoding_and_missing_entry_fail_without_execution() {
    for case in ["missing", "big", "utf8", "count", "manifest", "deep"] {
        let root = tempfile::tempdir().unwrap();
        let (_, manifest) = setup(root.path());
        let package = manifest.parent().unwrap();
        match case {
            "missing" => fs::remove_file(package.join("main.js")).unwrap(),
            "big" => fs::write(package.join("main.js"), vec![b'x'; 1024 * 1024 + 1]).unwrap(),
            "utf8" => fs::write(package.join("main.js"), [255]).unwrap(),
            "count" => {
                for i in 0..64 {
                    fs::write(package.join(format!("extra-{i}.js")), "").unwrap();
                }
            }
            "manifest" => fs::write(&manifest, "{\"version\":99}").unwrap(),
            "deep" => {
                let mut dir = package.to_owned();
                for _ in 0..17 {
                    dir.push("d");
                    fs::create_dir(&dir).unwrap();
                }
            }
            _ => unreachable!(),
        }
        let cancel = AtomicBool::new(false);
        let budget = Budget::new(&cancel, Duration::from_secs(10));
        assert!(LoadedPackage::read(&manifest, &budget).is_err(), "{case}");
    }
}

#[cfg(unix)]
#[test]
fn symlink_packages_and_special_files_are_refused_without_opening_them() {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir().unwrap();
    let (source, manifest) = setup(root.path());
    let package = manifest.parent().unwrap();
    let cancel = AtomicBool::new(false);
    let budget = Budget::new(&cancel, Duration::from_secs(10));
    symlink(&source, package.join("outside.js")).unwrap();
    assert!(LoadedPackage::read(&manifest, &budget).is_err());
    fs::remove_file(package.join("outside.js")).unwrap();
    symlink(&manifest, root.path().join("manifest-alias.json")).unwrap();
    assert!(LoadedPackage::read(&root.path().join("manifest-alias.json"), &budget).is_err());
    let fifo = package.join("blocked.js");
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success()
    );
    assert!(LoadedPackage::read(&manifest, &budget).is_err());
}

#[test]
fn review_seals_inputs_context_raw_changes_and_publishes_only_to_new_paths() {
    let root = tempfile::tempdir().unwrap();
    let (source, manifest) = setup(root.path());
    let cancel = AtomicBool::new(false);
    let budget = Budget::new(&cancel, Duration::from_secs(10));
    let run = plugin_files::prepare(&source, &manifest, options(), &budget, reply).unwrap();
    assert_eq!(run.report().changes.len(), 1);
    assert_eq!(run.report().changes[0].before.text, "{\\i1}Été 😀{\\i0}  ");
    let json: Value = serde_json::from_slice(run.json()).unwrap();
    assert_eq!(json["reviewId"], run.report().review_id);
    assert_eq!(json["context"]["preferences"]["minimumDurationMs"], 800);
    let again = plugin_files::prepare(&source, &manifest, options(), &budget, reply).unwrap();
    assert_eq!(run.report().review_id, again.report().review_id);
    assert!(matches!(
        run.publish(&root.path().join("out.srt"), &"0".repeat(64), &budget),
        Err(Error::Changed(_))
    ));
    assert!(
        run.publish(&source, &run.report().review_id, &budget)
            .is_err()
    );
    assert!(
        run.publish(
            &root.path().join("out.ass"),
            &run.report().review_id,
            &budget
        )
        .is_err()
    );
    let output = root.path().join("out.srt");
    let path = run
        .publish(&output, &run.report().review_id, &budget)
        .unwrap();
    assert_eq!(path, output.canonicalize().unwrap());
    assert_eq!(fs::read_to_string(&source).unwrap(), SRT);
    assert_eq!(
        fs::read_to_string(&output).unwrap(),
        SRT.replace("}  ", "}")
    );
    assert!(
        run.publish(&output, &run.report().review_id, &budget)
            .is_err()
    );
    assert!(root.path().join(".out.srt.vsf-lock").is_file());
}

#[test]
fn changes_cancellation_and_competing_writers_preserve_sources_and_destinations() {
    for change in [
        "source",
        "package",
        "context",
        "cancel",
        "destination",
        "lock",
        "deleted",
    ] {
        let root = tempfile::tempdir().unwrap();
        let (source, manifest) = setup(root.path());
        let context = root.path().join("context.json");
        fs::write(&context, "{\"cursorMs\":0}").unwrap();
        let cancel = AtomicBool::new(false);
        let budget = Budget::new(&cancel, Duration::from_secs(10));
        let mut opts = options();
        opts.context_file = Some(context.clone());
        let run = plugin_files::prepare(&source, &manifest, opts, &budget, reply).unwrap();
        let output = root.path().join("output.srt");
        let mut lease = None;
        match change {
            "source" => fs::write(&source, SRT.replace("Été", "été")).unwrap(),
            "package" => fs::write(manifest.parent().unwrap().join("main.js"), "changed").unwrap(),
            "context" => fs::write(&context, "{\"cursorMs\":1}").unwrap(),
            "cancel" => cancel.store(true, Ordering::Release),
            "destination" => fs::write(&output, "existing").unwrap(),
            "lock" => lease = Some(save_lease::acquire(&output).unwrap()),
            "deleted" => fs::remove_file(&source).unwrap(),
            _ => unreachable!(),
        }
        assert!(
            run.publish(&output, &run.report().review_id, &budget)
                .is_err(),
            "{change}"
        );
        if change == "destination" {
            assert_eq!(fs::read_to_string(&output).unwrap(), "existing");
        } else {
            assert!(!output.exists());
        }
        if change == "source" {
            assert_eq!(
                fs::read_to_string(&source).unwrap(),
                SRT.replace("Été", "été")
            );
        } else if change != "deleted" {
            assert_eq!(fs::read_to_string(&source).unwrap(), SRT);
        }
        assert!(
            fs::read_dir(root.path()).unwrap().all(|e| !e
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".tmp"))
        );
        drop(lease);
    }
}

#[test]
fn edits_during_execution_invalid_scope_and_context_produce_no_report_or_output() {
    let root = tempfile::tempdir().unwrap();
    let (source, manifest) = setup(root.path());
    let cancel = AtomicBool::new(false);
    let budget = Budget::new(&cancel, Duration::from_secs(10));
    let result = plugin_files::prepare(&source, &manifest, options(), &budget, |e| {
        fs::write(&source, SRT.replace("Été", "new")).unwrap();
        reply(e)
    });
    assert!(matches!(result, Err(Error::Changed(_))));
    for scope in [vec![0], vec![2], vec![1, 1]] {
        let mut o = options();
        o.scope = scope;
        assert!(
            plugin_files::prepare(&source, &manifest, o, &budget, |_| panic!(
                "invalid scope must not execute"
            ))
            .is_err()
        );
    }
    let context = root.path().join("context.json");
    fs::write(&context, "{\"preferences\":{\"invented\":1}}").unwrap();
    let mut o = options();
    o.context_file = Some(context);
    assert!(
        plugin_files::prepare(&source, &manifest, o, &budget, |_| panic!(
            "invalid context must not execute"
        ))
        .is_err()
    );
}
