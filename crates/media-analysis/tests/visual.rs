use media_analysis::{Cancellation, Error, Tools, alignment, scenes::visual};
use std::{path::PathBuf, time::Duration};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/generated/resync")
        .join(name)
}
fn tools() -> Tools {
    Tools {
        ffmpeg: std::env::var_os("VSF_FFMPEG")
            .map(PathBuf::from)
            .unwrap_or_else(|| "ffmpeg".into()),
        ffprobe: std::env::var_os("VSF_FFPROBE")
            .map(PathBuf::from)
            .unwrap_or_else(|| "ffprobe".into()),
    }
}

#[test]
#[ignore = "requires resync fixtures and FFmpeg"]
fn decoded_visual_anchors_preserve_pts_and_distinguish_content() {
    let cancellation = Cancellation::default();
    let extract = |name| {
        visual::extract(
            &tools(),
            &fixture(name),
            &cancellation,
            Duration::from_secs(30),
        )
        .unwrap()
    };
    let source = extract("source.mkv");
    assert_eq!(
        source.iter().map(|a| a.time_us).collect::<Vec<_>>(),
        (1..18).map(|i| i * 2_000_000).collect::<Vec<_>>()
    );
    for name in ["fps-only.mkv", "vfr-origin.mkv"] {
        let target = extract(name);
        assert_eq!(
            target.iter().map(|a| a.time_us).collect::<Vec<_>>(),
            source.iter().map(|a| a.time_us).collect::<Vec<_>>()
        );
        let result =
            alignment::align(&source, &target, &cancellation, Duration::from_secs(2)).unwrap();
        assert_eq!(result.segments.len(), 1);
        assert_eq!(result.segments[0].map_us(15_500_000), Some(15_500_000.));
    }
    let unrelated = extract("unrelated.mkv");
    // A timestamp-only matcher would see an exact match. Appearance rejects it.
    assert_eq!(
        unrelated.iter().map(|a| a.time_us).collect::<Vec<_>>(),
        source.iter().map(|a| a.time_us).collect::<Vec<_>>()
    );
    assert!(
        alignment::align(&source, &unrelated, &cancellation, Duration::from_secs(2))
            .unwrap()
            .segments
            .is_empty()
    );
}

#[test]
#[ignore = "requires resync fixtures and FFmpeg"]
fn extraction_cancellation_and_timeout_do_not_publish_partial_anchors() {
    let cancellation = Cancellation::default();
    cancellation.cancel();
    assert!(matches!(
        visual::extract(
            &tools(),
            &fixture("source.mkv"),
            &cancellation,
            Duration::from_secs(30)
        ),
        Err(Error::Cancelled)
    ));
    assert!(matches!(
        visual::extract(
            &tools(),
            &fixture("source.mkv"),
            &Cancellation::default(),
            Duration::ZERO
        ),
        Err(Error::Timeout)
    ));
}

#[test]
#[ignore = "requires resync fixtures and FFmpeg"]
fn sampled_frames_keep_actual_pts_and_zero_in_both_origins() {
    let cancellation = Cancellation::default();
    let source = visual::sample(
        &tools(),
        &fixture("source.mkv"),
        &cancellation,
        Duration::from_secs(30),
    )
    .unwrap();
    let vfr = visual::sample(
        &tools(),
        &fixture("vfr-origin.mkv"),
        &cancellation,
        Duration::from_secs(30),
    )
    .unwrap();
    assert_eq!(source.len(), 72);
    assert_eq!(
        source.iter().map(|s| s.time_us).collect::<Vec<_>>(),
        (0..72).map(|i| i * 500_000).collect::<Vec<_>>()
    );
    assert_eq!(
        source.iter().map(|s| s.time_us).collect::<Vec<_>>(),
        vfr.iter().map(|s| s.time_us).collect::<Vec<_>>()
    );
}

#[test]
#[ignore = "requires resync fixtures and FFmpeg"]
fn final_frame_uses_decoded_pts_including_the_container_origin() {
    let cancellation = Cancellation::default();
    let frame = visual::last_frame(
        &tools(),
        &fixture("source.mkv"),
        &cancellation,
        Duration::from_secs(30),
    )
    .unwrap()
    .unwrap();
    assert_eq!(frame.time_us, 35_958_000);
    // Some demuxers include the positive origin in format.duration. If an
    // end-relative seek misses the video, do not invent an end from metadata.
    let vfr = visual::last_frame(
        &tools(),
        &fixture("vfr-origin.mkv"),
        &cancellation,
        Duration::from_secs(30),
    )
    .unwrap();
    assert!(vfr.is_none_or(|frame| frame.time_us == 35_875_000));
}

#[test]
#[ignore = "requires FFmpeg"]
fn final_frame_handles_short_video_and_does_not_guess_from_a_longer_audio_track() {
    let directory = tempfile::tempdir().unwrap();
    let tools = tools();
    for audio in [false, true] {
        let path = directory.path().join(if audio {
            "long-audio.mkv"
        } else {
            "short-video.mkv"
        });
        let mut command = std::process::Command::new(&tools.ffmpeg);
        command.args([
            "-v",
            "error",
            "-nostdin",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=32x32:rate=10:duration=1",
        ]);
        if audio {
            command.args([
                "-f",
                "lavfi",
                "-i",
                "anullsrc=r=8000:cl=mono",
                "-map",
                "0:v:0",
                "-map",
                "1:a:0",
                "-c:a",
                "pcm_s16le",
                "-t",
                "8",
            ]);
        }
        let output = command
            .args([
                "-filter_threads",
                "1",
                "-c:v",
                "ffv1",
                "-threads",
                "2",
                "-fps_mode",
                "passthrough",
            ])
            .arg(&path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let frame = visual::last_frame(
            &tools,
            &path,
            &Cancellation::default(),
            Duration::from_secs(30),
        )
        .unwrap();
        if audio {
            assert!(
                frame.is_none(),
                "audio duration must not invent a video endpoint"
            );
        } else {
            assert_eq!(frame.unwrap().time_us, 900_000);
        }
    }
}

#[test]
#[ignore = "requires resync fixtures and FFmpeg"]
fn requested_neighborhoods_keep_actual_pts_coalesce_ranges_and_handle_eof() {
    let cancel = Cancellation::default();
    let tools = tools();
    let version = tools
        .versions(&cancel, Duration::from_secs(10))
        .unwrap()
        .ffmpeg;
    for (name, expected) in [
        (
            "source.mkv",
            vec![917_000, 958_000, 1_000_000, 1_042_000, 1_083_000],
        ),
        ("vfr-origin.mkv", vec![1_000_000]),
    ] {
        let frames = visual::sample_near(
            &tools,
            &fixture(name),
            &[1_000_000, 1_010_000],
            100_000,
            &version,
            &cancel,
            Duration::from_secs(30),
        )
        .unwrap();
        assert_eq!(
            frames
                .frames()
                .iter()
                .map(|f| f.time_us)
                .collect::<Vec<_>>(),
            expected,
            "{name}"
        );
    }
    assert!(
        visual::sample_near(
            &tools,
            &fixture("source.mkv"),
            &[37_000_000],
            100_000,
            &version,
            &cancel,
            Duration::from_secs(30)
        )
        .unwrap()
        .frames()
        .is_empty()
    );
    // The expression is far beyond Windows' command-line ceiling. Every frame
    // in each disjoint 200 ms interval must still be emitted exactly once.
    let requests: Vec<_> = (0..10_000).map(|i| i * 1_000_000).collect();
    let frames = visual::sample_near(
        &tools,
        &fixture("source.mkv"),
        &requests,
        100_000,
        &version,
        &cancel,
        Duration::from_secs(30),
    )
    .unwrap();
    assert_eq!(frames.frames().len(), 180);
    assert!(frames.frames().iter().all(|f| {
        let phase = f.time_us % 1_000_000;
        phase <= 100_000 || phase >= 900_000
    }));
}
