//! Exercise the public command, including its exit status and file publication.
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};
use tempfile::{TempDir, tempdir};

struct Example {
    dir: TempDir,
    source: PathBuf,
    cuts: PathBuf,
}
impl Example {
    fn new() -> Self {
        let dir = tempdir().unwrap();
        let source = dir.path().join("input.srt");
        fs::write(&source,"\u{feff}1\r\n00:00:01,000 --> 00:00:03,000\r\nOne\r\n\r\n2\r\n00:00:03,030 --> 00:00:05,000\r\nTwo\r\n").unwrap();
        let cuts = dir.path().join("cuts.json");
        fs::write(&cuts, r#"{"version":1,"cutsMs":[10000,6000,6000]}"#).unwrap();
        Self { dir, source, cuts }
    }
    fn run(&self, extra: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_vsf"))
            .args([
                "fix",
                "timing",
                "--subtitles",
                self.source.to_str().unwrap(),
                "--profile",
                "vsf-default",
            ])
            .args(extra)
            .output()
            .unwrap()
    }
    fn review(&self, extra: &[&str]) -> Value {
        let mut args = vec!["--json", "--scenes", self.cuts.to_str().unwrap()];
        args.extend_from_slice(extra);
        let result = self.run(&args);
        assert!(
            matches!(result.status.code(), Some(0 | 1)),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        serde_json::from_slice(&result.stdout).unwrap()
    }
    fn publish(&self, id: &str, out: &Path) -> Output {
        self.run(&[
            "--scenes",
            self.cuts.to_str().unwrap(),
            "--accept",
            id,
            "--output",
            out.to_str().unwrap(),
        ])
    }
}
fn code(output: Output, expected: i32) {
    assert_eq!(
        output.status.code(),
        Some(expected),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
fn preview_hash_scope_accept_and_new_file_guards() {
    let ex = Example::new();
    let original = fs::read(&ex.source).unwrap();
    let first = ex.review(&[]);
    assert_eq!(first["schemaVersion"], 1);
    assert_eq!(first, ex.review(&[]));
    assert_eq!(first["coverage"], "checked");
    assert!(
        first["changes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["selected"] == true)
    );
    assert_eq!(fs::read_dir(ex.dir.path()).unwrap().count(), 2);
    let id = first["reviewId"].as_str().unwrap();
    assert_eq!(id.len(), 64);
    let scoped = ex.review(&[
        "--fix",
        "gaps",
        "--cue",
        "2",
        "--start-tolerance-ms",
        "120",
        "--scene-edges",
        "nearest",
        "--priority",
        "spacing",
    ]);
    assert_ne!(scoped["reviewId"], id);
    assert_eq!(scoped["families"]["speed"], false);
    assert_eq!(scoped["cues"], serde_json::json!([2]));
    assert!(scoped["changes"].as_array().unwrap().iter().all(|r| {
        r["unit"]["endpoints"]
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e["index"] == 1)
    }));
    let output = ex.dir.path().join("fixed.srt");
    code(ex.publish(&"0".repeat(64), &output), 6);
    assert!(!output.exists());
    code(ex.publish(id, &ex.source), 2);
    code(ex.publish(id, &output.with_extension("ass")), 2);
    code(ex.publish(id, &output), 0);
    let result = fs::read(&output).unwrap();
    assert_ne!(result, original);
    assert!(result.starts_with(&[0xef, 0xbb, 0xbf]));
    assert!(result.windows(2).any(|w| w == b"\r\n"));
    code(ex.publish(id, &output), 2);
    assert_eq!(fs::read(&output).unwrap(), result);
    assert_eq!(fs::read(&ex.source).unwrap(), original);
    fs::write(&ex.cuts, br#"{"version":1,"cutsMs":[]}"#).unwrap();
    code(ex.publish(id, &ex.dir.path().join("changed.srt")), 6);
}
#[test]
fn strict_scenes_explicit_coverage_and_invalid_inputs() {
    let ex = Example::new();
    code(ex.run(&[]), 2);
    let no = ex.run(&["--without-scenes", "--json"]);
    assert_eq!(no.status.code(), Some(3));
    let report: Value = serde_json::from_slice(&no.stdout).unwrap();
    assert_eq!(report["coverage"], "noMedia");
    let cuts = ex.cuts.to_str().unwrap();
    for input in [
        r#"{"version":2,"cutsMs":[]}"#,
        r#"{"version":1,"cutsMs":[-1]}"#,
        r#"{"version":1,"cutsMs":[1.5]}"#,
        r#"{"version":1,"cutsMs":[],"extra":1}"#,
        r#"{"version":1}"#,
        r#"{"version":1,"version":1,"cutsMs":[]}"#,
    ] {
        fs::write(&ex.cuts, input).unwrap();
        code(ex.run(&["--scenes", cuts]), 2);
    }
    fs::write(
        &ex.cuts,
        serde_json::to_vec(&serde_json::json!({"version":1,"cutsMs":vec![1;100001]})).unwrap(),
    )
    .unwrap();
    code(ex.run(&["--scenes", cuts]), 2);
    for args in [
        vec!["--without-scenes", "--scenes", cuts],
        vec!["--without-scenes", "--start-tolerance-ms", "1001"],
        vec!["--without-scenes", "--fix", "bogus"],
        vec!["--without-scenes", "--cue", "0"],
        vec!["--without-scenes", "--cue", "3"],
        vec!["--without-scenes", "--cue", "1,1"],
        vec!["--output", "unreviewed.srt"],
    ] {
        code(ex.run(&args), 2);
    }
    code(
        ex.run(&[
            "--scenes",
            ex.dir.path().join("missing.json").to_str().unwrap(),
        ]),
        5,
    );
}
#[test]
fn remaining_findings_return_one_and_print_manual_budget() {
    let ex = Example::new();
    let s = format!(
        "1\n00:00:01,000 --> 00:00:02,000\n{}\n\n2\n00:00:02,080 --> 00:00:04,000\nNext\n",
        "w".repeat(125)
    );
    fs::write(&ex.source, s).unwrap();
    let output = ex.run(&["--fix", "speed", "--scenes", ex.cuts.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(
        text.contains("Review ID") && text.contains("Shorten by") && text.contains("Budget at")
    );
}
