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
    assert_eq!(first["withheldAll"], false);
    assert_eq!(first, ex.review(&[]));
    assert_eq!(first["coverage"], "checked");
    for row in first["changes"].as_array().unwrap() {
        assert!(row.get("ghosts").is_none());
        for key in ["effects", "effectsText", "dependsOn", "conflictsWith"] {
            assert!(row[key].is_array(), "missing {key}");
        }
    }
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
        r["endpoints"]
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
    assert!(result.ends_with(b"\r\n\r\n"));
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
        text.contains("Review ID")
            && text.contains("Shorten by")
            && text.contains("Scene coverage: complete")
    );
}

#[test]
fn complete_scene_input_uses_the_full_file_budget_and_empty_preview_returns_zero() {
    let ex = Example::new();
    let mut file = vec![b' '; 5 * 1024 * 1024];
    file.extend_from_slice(br#"{"version":1,"cutsMs":[]}"#);
    fs::write(&ex.cuts, file).unwrap();
    let output = ex.run(&[
        "--scenes",
        ex.cuts.to_str().unwrap(),
        "--fix",
        "shots",
        "--json",
    ]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(report["changes"].as_array().unwrap().is_empty());
}

#[test]
fn episode_names_the_overlap_and_keeps_independent_repairs_separate() {
    // Construct SRT from the same synthetic cues as the Rust-generated dock review.
    let episode: Value = serde_json::from_str(include_str!(
        "../../../tests/frontend/scene-quality/timing-episode.json"
    ))
    .unwrap();
    let ex = Example::new();
    let mut srt = String::new();
    let time = |ms: u64| {
        format!(
            "{:02}:{:02}:{:02},{:03}",
            ms / 3_600_000,
            ms / 60_000 % 60,
            ms / 1000 % 60,
            ms % 1000
        )
    };
    for cue in episode["cues"].as_array().unwrap() {
        use std::fmt::Write;
        writeln!(
            srt,
            "{}\n{} --> {}\n{}\n",
            cue["id"],
            time(cue["startMs"].as_u64().unwrap()),
            time(cue["endMs"].as_u64().unwrap()),
            cue["text"].as_str().unwrap()
        )
        .unwrap();
    }
    fs::write(&ex.source, srt).unwrap();
    fs::write(&ex.cuts, br#"{"version":1,"cutsMs":[12000,17990]}"#).unwrap();
    let output = ex.run(&[
        "--scenes",
        ex.cuts.to_str().unwrap(),
        "--priority",
        "spacing",
        "--start-tolerance-ms",
        "300",
        "--scene-edges",
        "nearest",
    ]);
    assert_eq!(output.status.code(), Some(1));
    let text = String::from_utf8(output.stdout).unwrap();
    for phrase in [
        "Scene coverage: complete",
        "Trade-off",
        "gap 20 → 80 ms · CPS 38.7 → 40.3 (limit 20)",
        "cue 5 CPS",
        "Cue 1 needs manual work. Too fast: cue 2 starts at 00:00:02,520.",
        "Shorten by 30 counted characters (maximum 28).",
    ] {
        assert!(text.contains(phrase), "missing {phrase} in {text}");
    }
    for spurious in [
        "needs cue 3",
        "replaces the change to cue 10",
        "conflicts with cue 8 partial",
    ] {
        assert!(!text.contains(spurious), "{text}");
    }
    let default = ex.run(&["--scenes", ex.cuts.to_str().unwrap()]);
    assert_eq!(default.status.code(), Some(1));
    assert!(
        String::from_utf8(default.stdout).unwrap().contains(
            "Cue 4 needs manual work. Overlap not fixed: this cue would become too fast."
        )
    );
}

#[test]
fn unknown_profile_and_unreadable_existing_profile_have_distinct_errors() {
    let ex = Example::new();
    let run = |profile: &str| {
        Command::new(env!("CARGO_BIN_EXE_vsf"))
            .current_dir(ex.dir.path())
            .args([
                "fix",
                "timing",
                "--subtitles",
                ex.source.to_str().unwrap(),
                "--profile",
                profile,
                "--without-scenes",
            ])
            .output()
            .unwrap()
    };
    let missing = run("nope");
    assert_eq!(missing.status.code(), Some(2));
    assert!(
        String::from_utf8(missing.stderr)
            .unwrap()
            .contains("Unknown profile ID or file: nope")
    );
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("missing-target", ex.dir.path().join("existing-profile"))
            .unwrap();
        let unreadable = run("existing-profile");
        assert_eq!(unreadable.status.code(), Some(5));
        assert!(
            String::from_utf8(unreadable.stderr)
                .unwrap()
                .contains("Cannot read existing-profile")
        );
    }
}
