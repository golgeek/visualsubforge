//! Actual subprocess/decoding checks. Generate fixtures first; no native player.
use media_analysis::Tools;
use std::{fs, path::PathBuf, time::Duration};
use subtitle_formats::srt;
use vsf_application::resync::{
    self, Budget, Cancellation, Error, OutputPaths, Publication, Request, Stage, Subtitles,
};

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/generated/resync")
}
const SUBTITLES: &str = "7\n00:00:03,000 --> 00:00:04,000\n<i>世界</i>\n\n8\n00:00:04,100 --> 00:00:05,000\nAfter 100 ms\n\n9\n00:00:16,000 --> 00:00:17,000\nB\n";
fn request(path: PathBuf, target: &str) -> Request {
    Request {
        source_video: fixtures().join("source.mkv"),
        target_video: fixtures().join(target),
        subtitles: Subtitles::File(path),
    }
}

#[test]
#[ignore = "Requires python3 scripts/resync-fixtures.py and FFmpeg/ffprobe"]
fn shared_operation_maps_content_and_publishes_preserved_srt_and_report() {
    for (target, expected) in [
        ("offset.mkv", [(6000, 7000), (7100, 8000), (19000, 20000)]),
        ("fps-only.mkv", [(3000, 4000), (4100, 5000), (16000, 17000)]),
        (
            "speed-ads.mkv",
            [(1960, 2920), (3016, 3880), (14520, 15480)],
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.srt");
        fs::write(&source, SUBTITLES).unwrap();
        let cancel = Cancellation::default();
        let result = resync::analyze(
            &Tools::discover().unwrap(),
            request(source.clone(), target),
            &cancel,
            Duration::from_secs(90),
            |_| {},
        )
        .unwrap();
        assert!(result.proposal().is_complete(), "{target}");
        let paths = OutputPaths::new(
            &dir.path().join("report.json"),
            Some(&dir.path().join("target.srt")),
        )
        .unwrap();
        assert_eq!(
            result
                .publish(&paths, &Budget::new(&cancel, Duration::from_secs(30)))
                .unwrap(),
            Publication::Complete
        );
        assert_eq!(fs::read_to_string(&source).unwrap(), SUBTITLES);
        let output = fs::read_to_string(paths.subtitles().unwrap()).unwrap();
        let parsed = srt::parse(&output).unwrap();
        for (cue, (start, end)) in parsed.document.cues().iter().zip(expected) {
            assert_eq!(
                (cue.start().get(), cue.end().get()),
                (start, end),
                "{target}"
            );
        }
        assert_eq!(parsed.document.cues()[0].text(), "<i>世界</i>");
        assert!(output.starts_with("7\n"));
        let report: serde_json::Value =
            serde_json::from_slice(&fs::read(paths.report()).unwrap()).unwrap();
        assert_eq!(report["schema_version"], 3);
        assert_eq!(report["complete"], true);
        assert_eq!(report["cue_gap_policy"], "content_mapped_endpoints_v1");
        assert_eq!(
            report["proposed_subtitle_sha256"],
            result.proposal().subtitles_sha256().unwrap()
        );
        assert_eq!(report["cues"][0]["cue_id"], "1");
        assert_eq!(report["cues"][0]["following_gap"]["source_ms"], 100);
        assert_eq!(
            report["cues"][0]["following_gap"]["outcome"]["status"],
            "mapped"
        );
        if target == "speed-ads.mkv" {
            assert_eq!(
                report["cues"][0]["following_gap"]["outcome"]["target_ms"],
                96
            );
            assert_eq!(
                report["cues"][1]["following_gap"]["outcome"]["status"],
                "across_sections"
            );
        }
        assert!(
            report["tools"]["ffmpeg"]
                .as_str()
                .unwrap()
                .contains("ffmpeg version")
        );
        assert!(
            result
                .publish(&paths, &Budget::new(&cancel, Duration::from_secs(30)))
                .is_err()
        );
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 3);
    }
}

#[test]
#[ignore = "Requires python3 scripts/resync-fixtures.py and FFmpeg/ffprobe"]
fn unresolved_and_changed_inputs_cannot_publish_subtitles() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.srt");
    fs::write(&source, "1\n00:00:12,200 --> 00:00:13,000\nRemoved ad\n").unwrap();
    let cancel = Cancellation::default();
    let result = resync::analyze(
        &Tools::discover().unwrap(),
        request(source.clone(), "speed-ads.mkv"),
        &cancel,
        Duration::from_secs(90),
        |_| {},
    )
    .unwrap();
    let paths = OutputPaths::new(
        &dir.path().join("report.json"),
        Some(&dir.path().join("target.srt")),
    )
    .unwrap();
    assert_eq!(
        result
            .publish(&paths, &Budget::new(&cancel, Duration::from_secs(30)))
            .unwrap(),
        Publication::NeedsReview
    );
    assert!(paths.report().is_file());
    assert!(!paths.subtitles().unwrap().exists());
    let other = OutputPaths::new(&dir.path().join("other.json"), None).unwrap();
    fs::write(&source, SUBTITLES).unwrap();
    assert!(
        result
            .publish(&other, &Budget::new(&cancel, Duration::from_secs(30)))
            .is_err()
    );
    assert!(!other.report().exists());
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2); // source + first report
}

#[test]
#[ignore = "Requires python3 scripts/resync-fixtures.py and FFmpeg/ffprobe"]
fn cancelled_or_changed_input_job_never_returns_a_proposal() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.srt");
    fs::write(&source, SUBTITLES).unwrap();
    let cancel = Cancellation::default();
    let result = resync::analyze(
        &Tools::discover().unwrap(),
        request(source.clone(), "offset.mkv"),
        &cancel,
        Duration::from_secs(90),
        |stage| {
            if matches!(stage, Stage::TargetSamples) {
                cancel.cancel();
            }
        },
    );
    assert!(matches!(result, Err(Error::Cancelled)));
    let result = resync::analyze(
        &Tools::discover().unwrap(),
        request(source.clone(), "offset.mkv"),
        &Cancellation::default(),
        Duration::from_secs(90),
        |stage| {
            if matches!(stage, Stage::Propose) {
                fs::write(&source, SUBTITLES.replace("B\n", "C\n")).unwrap();
            }
        },
    );
    assert!(matches!(result, Err(Error::InputChanged(_))));
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}
