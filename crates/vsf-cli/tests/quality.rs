use std::{
    fs,
    process::{Command, Output},
};
use vsf_application::quality::{self, Profile};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_vsf"))
        .args(args)
        // The checker must never discover/decode media.
        .env("VSF_FFMPEG", "/missing-ffmpeg")
        .env("VSF_FFPROBE", "/missing-ffprobe")
        .output()
        .unwrap()
}

#[test]
fn check_cli_matches_shared_report_preserves_inputs_and_distinguishes_partial_coverage() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("input.srt");
    let source = "\u{feff}007\r\n00:00:00,000 --> 00:00:02,500\r\n<i>Thirty five characters of dialogue.</i>\r\n\r\n";
    fs::write(&path, source).unwrap();
    let parsed = vsf_application::subtitle_file::parse(source.as_bytes()).unwrap();
    let expected = quality::evaluate(&parsed.document, &Profile::default(), |_, _| Ok(())).unwrap();
    assert_eq!(expected.metrics[0].characters, 35);
    assert!(expected.findings.is_empty());
    let output = run(&[
        "check",
        "--subtitles",
        path.to_str().unwrap(),
        "--profile",
        "vsf-default",
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
        serde_json::to_value(&expected).unwrap()
    );
    let output = run(&[
        "check",
        "--subtitles",
        path.to_str().unwrap(),
        "--profile",
        "sw3-uk-vo-subset",
    ]);
    assert_eq!(output.status.code(), Some(3));
    let human = String::from_utf8(output.stdout).unwrap();
    assert!(human.contains("0 findings"));
    assert!(human.contains("partial"));
    assert!(human.contains("Not checked:"));
    assert_eq!(fs::read(&path).unwrap(), source.as_bytes());
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    assert!(!expected.complete);
    assert_eq!(
        expected.scene_coverage,
        quality::scene::SceneCoverage::NoMedia
    );
    // Text-only checking must be explicit; missing media cannot silently pass
    // a policy whose nonzero scene buffers request endpoint checks.
    let mut text_only = Profile::default().custom("text-only".into(), "Text only".into());
    text_only.timing.scene_before_ms = 0;
    text_only.timing.scene_after_ms = 0;
    let preset = dir.path().join("text-only.vsfpreset");
    quality::write_profile(&preset, &text_only).unwrap();
    let output = run(&[
        "check",
        "--subtitles",
        path.to_str().unwrap(),
        "--profile",
        preset.to_str().unwrap(),
        "--json",
    ]);
    assert_eq!(output.status.code(), Some(0));
    let report: quality::Report = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report.schema_version, 2);
    assert!(report.complete);
    assert_eq!(
        report.scene_coverage,
        quality::scene::SceneCoverage::NotRequired
    );
    assert_eq!(fs::read(&path).unwrap(), source.as_bytes());
}

#[test]
fn imported_profiles_and_errors_have_documented_exit_codes_without_file_changes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("input.srt");
    let source = "1\n00:00:00,000 --> 00:00:00,500\nShort\n";
    fs::write(&path, source).unwrap();
    let profile_path = dir.path().join("custom.vsfpreset");
    let mut p = Profile::default().custom("custom".into(), "Custom example".into());
    p.rules.speed.enabled = false;
    p.timing.scene_before_ms = 0;
    p.timing.scene_after_ms = 0;
    quality::write_profile(&profile_path, &p).unwrap();
    let preset = fs::read(&profile_path).unwrap();
    let args = [
        "check",
        "--subtitles",
        path.to_str().unwrap(),
        "--profile",
        profile_path.to_str().unwrap(),
        "--json",
    ];
    let output = run(&args);
    assert_eq!(output.status.code(), Some(1));
    let report: quality::Report = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report.findings.len(), 1);
    assert_eq!(report.findings[0].rule, quality::Rule::DurationShort);
    assert_eq!(fs::read(&profile_path).unwrap(), preset);
    fs::write(&profile_path, b"{\"version\":999}").unwrap();
    let output = run(&args);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(!output.stderr.is_empty());
    assert_eq!(fs::read(&path).unwrap(), source.as_bytes());
    quality::write_profile(&profile_path, &p).unwrap();
    fs::write(&path, b"not SRT").unwrap();
    assert_eq!(run(&args).status.code(), Some(2));
    fs::remove_file(&path).unwrap();
    assert_eq!(run(&args).status.code(), Some(5));
    assert_eq!(
        run(&["check", "--subtitles", "missing.srt"]).status.code(),
        Some(2)
    );
    assert_eq!(run(&["check", "--help"]).status.code(), Some(0));
}
