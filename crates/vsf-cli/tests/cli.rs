use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    time::{Duration, Instant},
};

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
        let started = Instant::now();
        let status = loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                break status;
            }
            assert!(
                started.elapsed() < Duration::from_secs(90),
                "CLI exceeded test deadline"
            );
            std::thread::sleep(Duration::from_millis(10));
        };
        assert!(self.stdout.as_file().metadata().unwrap().len() < 1_000_000);
        assert!(self.stderr.as_file().metadata().unwrap().len() < 1_000_000);
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
fn vsf() -> Command {
    Command::new(env!("CARGO_BIN_EXE_vsf"))
}
fn run(command: &mut Command) -> Output {
    Running::start(command).finish()
}
const SRT: &str = "7\n00:00:03,000 --> 00:00:04,000\n<i>世界</i>\n\n9\n00:00:04,100 --> 00:00:05,000\nAfter 100 ms\n";

fn setup(dir: &Path) -> Command {
    fs::write(dir.join("source.srt"), SRT).unwrap();
    fs::write(dir.join("source.mkv"), b"input video placeholder").unwrap();
    fs::write(dir.join("target.mkv"), b"target video placeholder").unwrap();
    let mut command = vsf();
    command.current_dir(dir).args([
        "resync",
        "--source-video",
        "source.mkv",
        "--source-subtitles",
        "source.srt",
        "--target-video",
        "target.mkv",
        "--report",
        "report.json",
    ]);
    command
}

#[test]
fn help_and_invalid_combinations_have_documented_streams_and_exit_status() {
    for args in [vec!["--help"], vec!["resync", "--help"], vec!["--version"]] {
        let output = run(vsf().args(args));
        assert!(output.status.success());
        assert!(!output.stdout.is_empty());
        assert!(output.stderr.is_empty());
    }
    let dir = tempfile::tempdir().unwrap();
    for options in [
        vec![],
        vec!["--analysis-only", "--output", "out.srt"],
        vec!["--analysis-only", "--timeout-seconds", "0"],
        vec!["--analysis-only", "--timeout-seconds", "14401"],
    ] {
        let output = run(setup(dir.path()).args(options));
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.is_empty());
        assert!(!dir.path().join("report.json").exists());
    }
}

#[test]
fn invalid_input_existing_outputs_and_missing_tools_preserve_files() {
    let dir = tempfile::tempdir().unwrap();
    let mut command = setup(dir.path());
    command.args(["--output", "out.srt"]);
    fs::write(dir.path().join("source.srt"), b"invalid subtitles").unwrap();
    assert_eq!(run(&mut command).status.code(), Some(2));
    fs::write(dir.path().join("source.srt"), SRT).unwrap();
    fs::write(dir.path().join("out.srt"), b"existing output").unwrap();
    assert_eq!(run(&mut command).status.code(), Some(2));
    assert_eq!(
        fs::read(dir.path().join("out.srt")).unwrap(),
        b"existing output"
    );
    let mut command = setup(dir.path());
    command.args(["--analysis-only"]);
    command.env("VSF_FFMPEG", dir.path().join("missing-ffmpeg"));
    let output = run(&mut command);
    assert_eq!(output.status.code(), Some(4));
    assert!(String::from_utf8_lossy(&output.stderr).contains("missing-ffmpeg"));
    assert!(output.stdout.is_empty());
    assert!(!dir.path().join("report.json").exists());
    assert_eq!(
        fs::read_to_string(dir.path().join("source.srt")).unwrap(),
        SRT
    );
}

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/generated/resync")
}

#[test]
#[ignore = "Requires python3 scripts/resync-fixtures.py and FFmpeg/ffprobe"]
fn real_binary_publishes_round_trip_srt_and_json_and_refuses_retry() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("original.srt");
    fs::write(&source, SRT).unwrap();
    let mut command = vsf();
    command
        .current_dir(dir.path())
        .args(["resync", "--source-video"])
        .arg(fixtures().join("source.mkv"))
        .arg("--target-video")
        .arg(fixtures().join("offset.mkv"))
        .arg("--source-subtitles")
        .arg(&source)
        .args([
            "--output",
            "result.srt",
            "--report",
            "report.json",
            "--json",
        ]);
    let output = run(&mut command);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let summary: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(summary["status"], "complete");
    assert_eq!(summary["unresolved_cues"], 0);
    let result = fs::read_to_string(dir.path().join("result.srt")).unwrap();
    let parsed = subtitle_formats::srt::parse(&result).unwrap();
    assert_eq!(
        (
            parsed.document.cues()[0].start().get(),
            parsed.document.cues()[0].end().get()
        ),
        (6000, 7000)
    );
    assert_eq!(parsed.document.cues()[0].text(), "<i>世界</i>");
    assert_eq!(
        subtitle_formats::srt::serialize(&parsed.document, &parsed.source).unwrap(),
        result
    );
    assert_eq!(fs::read_to_string(&source).unwrap(), SRT);
    let report = fs::read(dir.path().join("report.json")).unwrap();
    assert_eq!(run(&mut command).status.code(), Some(2));
    assert_eq!(fs::read(dir.path().join("report.json")).unwrap(), report);
    assert_eq!(
        fs::read_to_string(dir.path().join("result.srt")).unwrap(),
        result
    );
}

#[test]
#[ignore = "Requires python3 scripts/resync-fixtures.py and FFmpeg/ffprobe"]
fn analysis_only_and_needs_review_do_not_create_subtitle_outputs() {
    for (srt, options, code, status) in [
        (SRT, vec!["--analysis-only"], 0, "analysis_only"),
        (
            "1\n00:00:12,200 --> 00:00:13,000\nDeleted ad\n",
            vec!["--output", "result.srt"],
            3,
            "needs_review",
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("source.srt"), srt).unwrap();
        let mut command = vsf();
        command
            .current_dir(dir.path())
            .args(["resync", "--source-video"])
            .arg(fixtures().join("source.mkv"))
            .arg("--target-video")
            .arg(fixtures().join("speed-ads.mkv"))
            .args([
                "--source-subtitles",
                "source.srt",
                "--report",
                "report.json",
                "--json",
            ])
            .args(options);
        let output = run(&mut command);
        assert_eq!(
            output.status.code(),
            Some(code),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let summary: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(summary["status"], status);
        assert!(summary["output"].is_null());
        assert!(!dir.path().join("result.srt").exists());
        assert!(dir.path().join("report.json").is_file());
        if status == "analysis_only" {
            let report: serde_json::Value =
                serde_json::from_slice(&fs::read(dir.path().join("report.json")).unwrap()).unwrap();
            assert_eq!(report["cues"][0]["outcome"]["end_ms"], 2920);
            assert_eq!(report["cues"][1]["outcome"]["start_ms"], 3016);
            assert_eq!(
                report["cues"][0]["following_gap"]["outcome"]["target_ms"],
                96
            );
        }
        assert_eq!(
            fs::read_to_string(dir.path().join("source.srt")).unwrap(),
            srt
        );
    }
}

#[cfg(unix)]
#[test]
fn ctrl_c_and_timeout_reap_the_active_tool_and_publish_nothing() {
    use std::os::unix::fs::PermissionsExt;
    for interrupt in [true, false] {
        let dir = tempfile::tempdir().unwrap();
        let mut command = setup(dir.path());
        let tool = dir.path().join("slow-tool");
        // exec retains this PID: there is one owned child, no detached shell
        // grandchild. The actual subprocess supervisor must kill and reap it.
        fs::write(
            &tool,
            "#!/bin/sh\nprintf '%s' \"$$\" > \"$VSF_TEST_CHILD_PID\"\nexec /bin/sleep 60\n",
        )
        .unwrap();
        fs::set_permissions(&tool, fs::Permissions::from_mode(0o700)).unwrap();
        let pid_path = dir.path().join("tool.pid");
        command.args([
            "--analysis-only",
            "--timeout-seconds",
            if interrupt { "30" } else { "1" },
        ]);
        command
            .env("VSF_FFMPEG", &tool)
            .env("VSF_TEST_CHILD_PID", &pid_path);
        let running = Running::start(&mut command);
        let started = Instant::now();
        let tool_pid = loop {
            if let Ok(pid) = fs::read_to_string(&pid_path)
                && pid.parse::<u32>().is_ok()
            {
                break pid;
            }
            assert!(
                started.elapsed() < Duration::from_secs(5),
                "Tool did not start"
            );
            std::thread::sleep(Duration::from_millis(10));
        };
        if interrupt {
            assert!(
                Command::new("kill")
                    .args(["-INT", &running.child.id().to_string()])
                    .status()
                    .unwrap()
                    .success()
            );
        }
        let output = running.finish();
        assert_eq!(
            output.status.code(),
            Some(if interrupt { 130 } else { 7 }),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(
            !Command::new("kill")
                .args(["-0", &tool_pid])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .unwrap()
                .success(),
            "Analysis child survived CLI exit"
        );
        assert!(output.stdout.is_empty());
        assert!(!dir.path().join("report.json").exists());
        assert_eq!(
            fs::read_to_string(dir.path().join("source.srt")).unwrap(),
            SRT
        );
    }
}
