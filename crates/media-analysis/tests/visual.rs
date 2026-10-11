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
