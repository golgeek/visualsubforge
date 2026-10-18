//! Run the shipping CLI, including its self-spawned JavaScript helper.
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    process::{Child, Command, Output, Stdio},
    time::{Duration, Instant},
};
use vsf_application::subtitle_file;

struct Running {
    child: Child,
    stdout: tempfile::NamedTempFile,
    stderr: tempfile::NamedTempFile,
}
impl Running {
    fn start(command: &mut Command) -> Self {
        let stdout = tempfile::NamedTempFile::new().unwrap();
        let stderr = tempfile::NamedTempFile::new().unwrap();
        let child = command
            .stdin(Stdio::null())
            .stdout(stdout.as_file().try_clone().unwrap())
            .stderr(stderr.as_file().try_clone().unwrap())
            .spawn()
            .unwrap();
        Self {
            child,
            stdout,
            stderr,
        }
    }
    fn finish(mut self) -> Output {
        let deadline = Instant::now() + Duration::from_secs(20);
        let status = loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                break status;
            }
            assert!(
                Instant::now() < deadline,
                "Plugin CLI exceeded test deadline"
            );
            std::thread::sleep(Duration::from_millis(10));
        };
        assert!(self.stdout.as_file().metadata().unwrap().len() < 4 * 1024 * 1024);
        assert!(self.stderr.as_file().metadata().unwrap().len() < 64 * 1024);
        Output {
            status,
            stdout: fs::read(self.stdout.path()).unwrap(),
            stderr: fs::read(self.stderr.path()).unwrap(),
        }
    }
}
impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
fn run(command: &mut Command) -> Output {
    Running::start(command).finish()
}
fn vsf() -> Command {
    Command::new(env!("CARGO_BIN_EXE_vsf"))
}
fn expect(output: Output, code: i32) -> Value {
    assert_eq!(
        output.status.code(),
        Some(code),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    if code == 0 || code == 1 {
        assert!(
            output.stderr.is_empty(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    } else {
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.is_empty());
        Value::Null
    }
}
const SRT: &str = "7\n00:00:01,000 --> 00:00:02,000\n{\\i1}Été 😀{\\i0}  \n\n9\n00:00:03,000 --> 00:00:04,000\n<i>世界</i>  \n\n";
struct Fixture {
    root: tempfile::TempDir,
    source: PathBuf,
    manifest: PathBuf,
    original: String,
}
impl Fixture {
    fn new(extension: &str, legacy: bool) -> Self {
        let root = tempfile::tempdir().unwrap();
        let original = match extension {
            "srt" => SRT.into(),
            "ass" => include_str!("../../../tests/fixtures/subtitles/resync.ass")
                .replace("}AB\n", "}AB  \n")
                .replace(",BA\n", ",BA  \n"),
            "ssa" => include_str!("../../../tests/fixtures/subtitles/resync.ssa")
                .replace("Red title\n", "Red title  \n")
                .replace("line\\N世界\n", "line\\N世界  \n"),
            "vtt" => include_str!("../../../tests/fixtures/subtitles/styles.vtt")
                .replace("— 😀\n", "— 😀  \n")
                .replace("</ruby>\n", "</ruby>  \n"),
            _ => unreachable!(),
        };
        let source = root.path().join(format!("captions Été 世界.{extension}"));
        fs::write(&source, &original).unwrap();
        let package = root.path().join("package 字幕");
        fs::create_dir(&package).unwrap();
        let manifest = package.join("manifest.json");
        let (m, s) = if legacy {
            (
                include_str!("../../../examples/plugins/vss-trim-trailing/manifest.json"),
                include_str!("../../../examples/plugins/vss-trim-trailing/main.js"),
            )
        } else {
            (
                include_str!("../../../examples/plugins/trim-trailing/manifest.json"),
                include_str!("../../../examples/plugins/trim-trailing/main.js"),
            )
        };
        fs::write(&manifest, m).unwrap();
        fs::write(package.join("main.js"), s).unwrap();
        Self {
            root,
            source,
            manifest,
            original,
        }
    }
    fn command(&self, action: &str) -> Command {
        let mut command = vsf();
        command
            .args(["plugin", action, "--subtitles"])
            .arg(&self.source)
            .arg("--package")
            .arg(&self.manifest)
            .arg("--json");
        command
    }
    fn script(&self, script: &str) {
        fs::write(self.manifest.parent().unwrap().join("main.js"), script).unwrap();
    }
    fn manifest(&self, api: &str, requires: &[&str]) {
        fs::write(&self.manifest, json!({"version":1,"id":"test.rule","name":"Test rule","api":api,"entry":"main.js","requires":requires}).to_string()).unwrap();
    }
    fn output(&self) -> PathBuf {
        self.root
            .path()
            .join("output")
            .with_extension(self.source.extension().unwrap())
    }
    fn unchanged(&self) {
        assert_eq!(fs::read_to_string(&self.source).unwrap(), self.original);
    }
}

#[test]
fn modern_and_vss_check_preview_apply_preserve_all_four_formats() {
    for legacy in [false, true] {
        for extension in ["srt", "ass", "ssa", "vtt"] {
            let f = Fixture::new(extension, legacy);
            let check = expect(run(&mut f.command("check")), 1);
            assert_eq!(check["findings"].as_array().unwrap().len(), 2);
            assert_eq!(check["changes"], json!([]));
            let preview = expect(run(&mut f.command("preview")), 0);
            assert_eq!(preview["changes"].as_array().unwrap().len(), 2);
            assert!(!f.output().exists());
            f.unchanged();
            let repeat = expect(run(&mut f.command("preview")), 0);
            assert_eq!(preview, repeat);
            let receipt = expect(
                run(f
                    .command("apply")
                    .arg("--accept")
                    .arg(preview["reviewId"].as_str().unwrap())
                    .arg("--output")
                    .arg(f.output())),
                0,
            );
            assert_eq!(receipt["status"], "published");
            assert_eq!(receipt["outputSha256"], preview["outputSha256"]);
            let output = fs::read(f.output()).unwrap();
            let before =
                subtitle_file::parse_document(f.original.as_bytes(), Some(extension)).unwrap();
            let after = subtitle_file::parse_document(&output, Some(extension)).unwrap();
            assert_eq!(before.document.cues().len(), after.document.cues().len());
            for (a, b) in before.document.cues().iter().zip(after.document.cues()) {
                assert_eq!(a.start(), b.start());
                assert_eq!(a.end(), b.end());
                assert_eq!(a.metadata(), b.metadata());
                assert_eq!(a.text().trim_end(), b.text());
            }
            if extension == "ass" || extension == "ssa" {
                assert_eq!(
                    String::from_utf8(output).unwrap(),
                    f.original.replace("  \n", "\n")
                );
            }
            f.unchanged();
            let mut clean = vsf();
            clean
                .args(["plugin", "check", "--subtitles"])
                .arg(f.output())
                .arg("--package")
                .arg(&f.manifest)
                .arg("--json");
            assert_eq!(expect(run(&mut clean), 0)["findings"], json!([]));
        }
    }
}

#[test]
fn input_package_context_scope_selection_and_settings_changes_invalidate_review() {
    for change in [
        "source",
        "script",
        "manifest",
        "helper",
        "context",
        "scope",
        "selection",
        "setting",
    ] {
        let f = Fixture::new("srt", false);
        let context = f.root.path().join("context.json");
        fs::write(&context, "{\"cursorMs\":0}").unwrap();
        let preview = expect(run(f.command("preview").arg("--context").arg(&context)), 0);
        let mut apply = f.command("apply");
        apply
            .arg("--context")
            .arg(&context)
            .arg("--accept")
            .arg(preview["reviewId"].as_str().unwrap())
            .arg("--output")
            .arg(f.output());
        match change {
            "source" => fs::write(&f.source, f.original.replace("Été", "été")).unwrap(),
            "script" => {
                let path = f.manifest.parent().unwrap().join("main.js");
                let script = fs::read_to_string(&path).unwrap();
                fs::write(path, format!("{script}\n// changed")).unwrap();
            }
            "manifest" => {
                let text = fs::read_to_string(&f.manifest).unwrap();
                fs::write(&f.manifest, format!("{text} ")).unwrap();
            }
            "helper" => fs::write(
                f.manifest.parent().unwrap().join("extra.js"),
                "// new helper",
            )
            .unwrap(),
            "context" => fs::write(&context, "{\"cursorMs\":1}").unwrap(),
            "scope" => {
                apply.args(["--cue", "1"]);
            }
            "selection" => {
                apply.args(["--select", "1"]);
            }
            "setting" => {
                apply.args(["--setting", "removeTabs=false"]);
            }
            _ => unreachable!(),
        }
        expect(run(&mut apply), 6);
        assert!(!f.output().exists(), "{change}");
        if change != "source" {
            f.unchanged();
        }
    }
}

#[test]
fn explicit_context_helpers_and_neighbor_changes_are_visible_in_review() {
    let f = Fixture::new("srt", true);
    f.manifest(
        "vss-1",
        &[
            "rules",
            "fixes",
            "scene-data",
            "core-read",
            "package-scripts",
            "stripped-text-write",
        ],
    );
    fs::write(
        f.manifest.parent().unwrap().join("helper.js"),
        "var Helper = {message: 'neighbor'};",
    )
    .unwrap();
    f.script(r#"LoadScript('helper.js'); globalThis.VSSPlugin={Name:'Neighbor rule',ParamMode:{Value:1}, HasError(c,p,n) {
        if (VSSCore.GetSelectedCount()!==1 || VSSCore.GetFirstSelected().Index!==2 || VSSCore.GetAudioCursorPosition()!==1234 || VSSCore.MinimumBlank!==80 || VSSCore.RsTargetIndex!==2 || SceneChange.GetNext(1000)!==1500 || SceneChange.StartOffset!==0 || this.ParamMode.Value!==3) throw Error('context mismatch');
        return Helper.message;
    }, FixError(c,p,n) { ScriptLog('changed neighbor'); n.StrippedText=n.StrippedText.trim(); }};"#);
    expect(run(f.command("preview").args(["--cue", "1"])), 2);
    let context = f.root.path().join("context.json");
    fs::write(&context, json!({"cursorMs":1234,"preferences":{"minimumBlankMs":80,"minimumDurationMs":800,"maximumDurationMs":7000,"cpsTarget":20,"rsTargetIndex":2},"scenes":{"cutsMs":[1500,2500],"startOffsetMs":0,"stopOffsetMs":0,"filterOffsetMs":0}}).to_string()).unwrap();
    let preview = expect(
        run(f
            .command("preview")
            .args([
                "--cue",
                "1",
                "--select",
                "2",
                "--setting",
                "ParamMode=3",
                "--context",
            ])
            .arg(context)),
        0,
    );
    assert_eq!(preview["changes"].as_array().unwrap().len(), 1);
    assert_eq!(preview["changes"][0]["cue"], 2);
    assert_eq!(preview["changes"][0]["outsideScope"], true);
    assert_eq!(preview["changes"][0]["before"]["text"], "<i>世界</i>  ");
    assert_eq!(preview["changes"][0]["after"]["text"], "世界");
    assert!(preview["changes"][0]["warning"].is_string());
    assert_eq!(preview["logs"], json!(["changed neighbor"]));
    assert_eq!(preview["settings"]["ParamMode"], 3);
    f.unchanged();
}

#[test]
fn script_errors_invalid_fixes_timeouts_and_unavailable_apis_never_publish() {
    for (extension, script, code) in [
        (
            "srt",
            "VSFPlugin={check(){return 'x'},fix(c,h){h.update(c.id,{text:'partial'});throw Error('rollback')}}",
            4,
        ),
        (
            "srt",
            "VSFPlugin={check(){return 'x'},fix(c,h){h.update(c.id,{endMs:c.startMs})}}",
            2,
        ),
        (
            "ass",
            "VSFPlugin={check(){return 'x'},fix(c,h){h.update(c.id,{startMs:c.startMs+1,endMs:c.endMs+1})}}",
            2,
        ),
        (
            "ass",
            "VSFPlugin={check(){return 'x'},fix(c,h){h.update(c.id,{endMs:c.endMs+100,text:'removed tags'})}}",
            2,
        ),
        (
            "vtt",
            "VSFPlugin={check(){return 'x'},fix(c,h){h.update(c.id,{startMs:c.startMs+100,endMs:c.endMs+100})}}",
            2,
        ),
        ("srt", "while(true){}", 7),
    ] {
        let f = Fixture::new(extension, false);
        f.script(script);
        expect(
            run(f
                .command("apply")
                .args([
                    "--execution-ms",
                    if script == "while(true){}" {
                        "50"
                    } else {
                        "1000"
                    },
                    "--accept",
                    &"0".repeat(64),
                    "--output",
                ])
                .arg(f.output())),
            code,
        );
        assert!(!f.output().exists());
        f.unchanged();
    }
    for expression in [
        "VSSCore.RsTargetIndex",
        "VSSCore.GetAudioCursorPosition()",
        "SceneChange.GetCount()",
        "VSSCore.Unsupported()",
    ] {
        let f = Fixture::new("srt", true);
        f.script(&format!(
            "VSSPlugin={{Name:'Unavailable API',HasError(){{return String({expression})}}}}"
        ));
        expect(run(&mut f.command("check")), 4);
        f.unchanged();
    }
}

#[test]
fn output_refusals_nondeterminism_and_bad_arguments_preserve_existing_files() {
    let f = Fixture::new("srt", false);
    let preview = expect(run(&mut f.command("preview")), 0);
    let review = preview["reviewId"].as_str().unwrap();
    fs::write(f.output(), "existing output").unwrap();
    for path in [&f.source, &f.output(), &f.root.path().join("wrong.ass")] {
        expect(
            run(f
                .command("apply")
                .args(["--accept", review, "--output"])
                .arg(path)),
            2,
        );
    }
    assert_eq!(fs::read_to_string(f.output()).unwrap(), "existing output");
    for args in [
        vec!["--cue", "0"],
        vec!["--cue", "3"],
        vec!["--cue", "1,1"],
        vec![
            "--setting",
            "removeTabs=false",
            "--setting",
            "removeTabs=true",
        ],
        vec!["--execution-ms", "10001"],
        vec!["--timeout-ms", "0"],
    ] {
        expect(run(f.command("preview").args(args)), 2);
    }
    expect(
        run(f.command("preview").args(["--setting", "removeTabs=1"])),
        4,
    );
    f.script("VSFPlugin={check(){return String(Math.random())},fix(c,h){h.update(c.id,{text:c.text.trimEnd()})}}");
    let p = expect(run(&mut f.command("preview")), 0);
    let output = f.root.path().join("random.srt");
    expect(
        run(f
            .command("apply")
            .arg("--accept")
            .arg(p["reviewId"].as_str().unwrap())
            .arg("--output")
            .arg(&output)),
        6,
    );
    assert!(!output.exists());
    f.unchanged();
}

#[test]
fn ordinary_help_and_hidden_worker_mode_are_separate() {
    for args in [
        vec!["--help"],
        vec!["plugin", "--help"],
        vec!["plugin", "preview", "--help"],
        vec!["plugin", "apply", "--help"],
    ] {
        let output = run(vsf().args(args));
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        assert!(!String::from_utf8_lossy(&output.stdout).contains("--vsf-plugin-worker"));
    }
    // A well-delivered worker reply contains the request error in its protocol;
    // only transport/entry failures use the helper process's failure exit code.
    let output = run(vsf().arg("--vsf-plugin-worker"));
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let reply: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(reply["result"]["Err"].is_object());
    let output = run(vsf().args(["--vsf-plugin-worker", "extra"]));
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(!output.stderr.is_empty());
}

#[cfg(unix)]
fn live_worker(parent: &mut Running) -> u32 {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        assert!(
            parent.child.try_wait().unwrap().is_none(),
            "CLI exited before helper observation"
        );
        let output = run(Command::new("ps").args(["-axo", "pid=,ppid=,command="]));
        assert!(output.status.success());
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            let mut fields = line.split_whitespace();
            let pid = fields.next().and_then(|s| s.parse::<u32>().ok());
            let ppid = fields.next().and_then(|s| s.parse::<u32>().ok());
            if ppid == Some(parent.child.id()) && line.contains("--vsf-plugin-worker") {
                return pid.unwrap();
            }
        }
        assert!(Instant::now() < deadline, "No helper observed");
        std::thread::sleep(Duration::from_millis(10));
    }
}
#[cfg(unix)]
#[test]
fn live_cli_cancellation_reaps_helper_and_changed_source_is_rejected() {
    for cancel in [true, false] {
        let f = Fixture::new("srt", false);
        f.script(if cancel { "while(true){}" } else { "let until=Date.now()+500;while(Date.now()<until){};VSFPlugin={check(){return 'x'},fix(c,h){h.update(c.id,{text:c.text.trimEnd()})}}" });
        let mut parent = Running::start(f.command("preview").args(["--execution-ms", "5000"]));
        let worker = live_worker(&mut parent);
        if cancel {
            assert!(
                run(Command::new("kill").args(["-INT", &parent.child.id().to_string()]))
                    .status
                    .success()
            );
        } else {
            fs::write(&f.source, f.original.replace("Été", "changed")).unwrap();
        }
        expect(parent.finish(), if cancel { 130 } else { 6 });
        let alive = run(Command::new("kill").args(["-0", &worker.to_string()]));
        assert!(
            !alive.status.success(),
            "Helper remains alive after CLI exit"
        );
        assert!(!f.output().exists());
        if cancel {
            f.unchanged();
        }
    }
}

#[cfg(unix)]
#[test]
fn alias_and_dangling_output_paths_are_never_overwritten() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new("srt", false);
    let p = expect(run(&mut f.command("preview")), 0);
    let review = p["reviewId"].as_str().unwrap();
    for alias in ["hard", "symbolic", "dangling"] {
        let path = f.root.path().join(format!("{alias}.srt"));
        match alias {
            "hard" => fs::hard_link(&f.source, &path).unwrap(),
            "symbolic" => symlink(&f.source, &path).unwrap(),
            "dangling" => symlink(f.root.path().join("missing"), &path).unwrap(),
            _ => unreachable!(),
        }
        expect(
            run(f
                .command("apply")
                .args(["--accept", review, "--output"])
                .arg(path)),
            2,
        );
        f.unchanged();
    }
}
