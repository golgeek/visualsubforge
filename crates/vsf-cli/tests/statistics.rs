use std::{
    fs,
    process::{Command, Output},
};
use vsf_application::quality::{
    self, Profile,
    scene::Scenes,
    statistics::{self, Range, Selection},
};
fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_vsf"))
        .args(args)
        .env("VSF_FFMPEG", "/missing-tools")
        .env("VSF_FFPROBE", "/missing-tools")
        .output()
        .unwrap()
}
#[test]
fn json_matches_shared_scoped_report_and_preserves_bytes_without_media_tools() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("input.srt");
    let source = "\u{feff}007\r\n00:00:00,000 --> 00:00:02,500\r\n{\\i1}A😀\r\n\r\n10\r\n00:00:02,000 --> 00:00:02,400\r\n0\r\n\r\n12\r\n00:00:04,000 --> 00:00:06,000\r\n<i>Other text</i>\r\n\r\n";
    fs::write(&path, source).unwrap();
    let parsed = vsf_application::subtitle_file::parse(source.as_bytes()).unwrap();
    let expected = statistics::evaluate(
        &parsed.document,
        &Profile::default(),
        &Scenes::NoMedia,
        Selection {
            range: Some(Range { first: 1, last: 3 }),
            excluded: vec![3],
        },
        || Ok(()),
    )
    .unwrap();
    let output = run(&[
        "stats",
        "--subtitles",
        path.to_str().unwrap(),
        "--profile",
        "vsf-default",
        "--from-cue",
        "1",
        "--to-cue",
        "3",
        "--exclude",
        "3,3",
        "--json",
    ]);
    assert_eq!(
        output.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap(),
        serde_json::to_value(expected).unwrap()
    );
    assert_eq!(fs::read(&path).unwrap(), source.as_bytes());
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}
#[test]
fn human_output_empty_scope_errors_and_exit_codes_do_not_conflate_findings_with_failure() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("input.srt");
    let preset = dir.path().join("profile.vsfpreset");
    fs::write(&path, "1\n00:00:00,000 --> 00:00:00,500\n0\n").unwrap();
    let mut p = Profile::default().custom("stats-test".into(), "Stats test".into());
    p.timing.scene_before_ms = 0;
    p.timing.scene_after_ms = 0;
    quality::write_profile(&preset, &p).unwrap();
    let args = [
        "stats",
        "--subtitles",
        path.to_str().unwrap(),
        "--profile",
        preset.to_str().unwrap(),
    ];
    let output = run(&args);
    assert_eq!(output.status.code(), Some(0));
    let human = String::from_utf8(output.stdout).unwrap();
    assert!(human.contains("1 included / 1 total"));
    assert!(human.contains("Unavailable RS: 1"));
    assert!(human.contains("RS distribution denominator: 0"));
    assert!(human.contains("Quality findings: 2"));
    let mut excluded = args.to_vec();
    excluded.extend(["--exclude", "1", "--json"]);
    let output = run(&excluded);
    assert_eq!(output.status.code(), Some(0));
    let r: statistics::Report = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(r.summary.included_cues, 0);
    assert!(r.summary.rs.mean.is_none());
    for suffix in [
        vec!["--from-cue", "1"],
        vec!["--from-cue", "0", "--to-cue", "1"],
        vec!["--exclude", "2"],
        vec!["--from-cue", "2", "--to-cue", "1"],
    ] {
        let mut bad = args.to_vec();
        bad.extend(suffix);
        let result = run(&bad);
        assert_eq!(result.status.code(), Some(2));
        assert!(result.stdout.is_empty());
        assert!(!result.stderr.is_empty());
    }
    fs::write(&path, "").unwrap();
    let mut json = args.to_vec();
    json.push("--json");
    let output = run(&json);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let r: statistics::Report = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(r.summary.total_cues, 0);
    fs::write(&path, "invalid").unwrap();
    assert_eq!(run(&args).status.code(), Some(2));
    fs::remove_file(&path).unwrap();
    assert_eq!(run(&args).status.code(), Some(5));
    assert_eq!(run(&["stats", "--help"]).status.code(), Some(0));
}
