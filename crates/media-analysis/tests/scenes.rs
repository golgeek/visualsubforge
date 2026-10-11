//! Run after python3 scripts/scene-fixtures.py with FFmpeg on PATH.
use media_analysis::{
    Cancellation, Error, Tools,
    scenes::{Detail, detect},
};
use std::{path::PathBuf, time::Duration};
fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/generated")
        .join(name)
}
fn tools() -> Tools {
    Tools {
        ffmpeg: std::env::var_os("VSF_FFMPEG")
            .unwrap_or_else(|| "ffmpeg".into())
            .into(),
        ffprobe: std::env::var_os("VSF_FFPROBE")
            .unwrap_or_else(|| "ffprobe".into())
            .into(),
    }
}
#[test]
#[ignore = "requires scene fixtures and FFmpeg"]
fn cfr_and_vfr_cuts_use_container_relative_presentation_timestamps() {
    for (name, detail) in [
        ("scene-cfr.mkv", Detail::Standard),
        ("scene-vfr.mkv", Detail::Standard),
        ("scene-cfr.mkv", Detail::Reduced),
        ("scene-vfr.mkv", Detail::Reduced),
    ] {
        let result = detect(
            &tools(),
            &fixture(name),
            &Cancellation::default(),
            Duration::from_secs(30),
            detail,
        )
        .unwrap();
        assert_eq!(result.seconds, vec![1.0, 2.0], "{name}");
    }
}
#[test]
#[ignore = "requires scene fixtures and FFmpeg"]
fn cancelled_and_expired_detection_never_returns_partial_cuts() {
    let token = Cancellation::default();
    token.cancel();
    assert!(matches!(
        detect(
            &tools(),
            &fixture("scene-cfr.mkv"),
            &token,
            Duration::from_secs(30),
            Detail::Reduced,
        ),
        Err(Error::Cancelled)
    ));
    assert!(matches!(
        detect(
            &tools(),
            &fixture("scene-cfr.mkv"),
            &Cancellation::default(),
            Duration::ZERO,
            Detail::Reduced,
        ),
        Err(Error::Timeout)
    ));
}

#[test]
#[ignore = "requires scene fixtures and FFmpeg"]
fn flat_video_has_no_cuts_and_audio_only_media_is_rejected() {
    assert!(
        detect(
            &tools(),
            &fixture("scene-flat.mkv"),
            &Cancellation::default(),
            Duration::from_secs(30),
            Detail::Reduced,
        )
        .unwrap()
        .seconds
        .is_empty()
    );
    assert!(
        detect(
            &tools(),
            &fixture("scene-audio.wav"),
            &Cancellation::default(),
            Duration::from_secs(30),
            Detail::Reduced,
        )
        .unwrap_err()
        .to_string()
        .contains("no video track")
    );
}

#[test]
#[ignore = "requires scene fixtures and FFmpeg"]
fn fast_keyframes_use_presentation_origin_and_differ_from_visual_cuts() {
    for name in ["scene-cfr.mkv", "scene-vfr.mkv"] {
        let result = media_analysis::scenes::keyframes(
            &tools(),
            &fixture(name),
            &Cancellation::default(),
            Duration::from_secs(30),
        )
        .unwrap();
        assert_eq!(result.seconds.first(), Some(&0.));
        assert!(result.seconds.windows(2).all(|p| p[0] < p[1]));
        assert!(result.seconds.last().unwrap() < &3.);
    }
    assert!(
        media_analysis::scenes::keyframes(
            &tools(),
            &fixture("scene-audio.wav"),
            &Cancellation::default(),
            Duration::from_secs(30)
        )
        .is_err()
    );
    let token = Cancellation::default();
    token.cancel();
    assert!(matches!(
        media_analysis::scenes::keyframes(
            &tools(),
            &fixture("scene-cfr.mkv"),
            &token,
            Duration::from_secs(30)
        ),
        Err(Error::Cancelled)
    ));
}

#[test]
#[ignore = "requires scene fixtures and FFmpeg"]
fn low_resolution_scan_finds_non_keyframe_cuts_and_ignores_continuous_motion() {
    let path = fixture("scene-fixed-gop.mkv");
    let keyframes = media_analysis::scenes::keyframes(
        &tools(),
        &path,
        &Cancellation::default(),
        Duration::from_secs(30),
    )
    .unwrap();
    assert_eq!(keyframes.seconds, vec![0.]);
    let cuts = detect(
        &tools(),
        &path,
        &Cancellation::default(),
        Duration::from_secs(30),
        Detail::Reduced,
    )
    .unwrap();
    assert_eq!(cuts.seconds.len(), 2, "{:?}", cuts.seconds);
    for (actual, expected) in cuts.seconds.iter().zip([1.12, 2.24]) {
        assert!(
            (actual - expected).abs() < 0.000001,
            "{actual} != {expected}"
        );
    }
    let motion = detect(
        &tools(),
        &fixture("scene-motion-portrait.mkv"),
        &Cancellation::default(),
        Duration::from_secs(30),
        Detail::Reduced,
    )
    .unwrap();
    assert!(motion.seconds.is_empty(), "{:?}", motion.seconds);
}

#[test]
#[ignore = "requires scene fixtures and FFmpeg"]
fn pipeline_verifies_intra_candidates_and_fills_fixed_gop_and_other_codec_gaps() {
    use media_analysis::scenes::{Phase, scan};
    for name in [
        "scene-intra-cfr.mkv",
        "scene-intra-vfr.mkv",
        "scene-fixed-gop.mkv",
        "scene-cfr.mkv",
        "scene-motion-portrait.mkv",
    ] {
        let mut quick = vec![];
        let mut progress = vec![];
        let result = scan(
            &tools(),
            &fixture(name),
            &Cancellation::default(),
            Duration::from_secs(30),
            Detail::Standard,
            |update| match update.phase {
                Phase::Verifying => quick = update.cuts,
                Phase::Full => progress.push(update.progress),
                _ => {}
            },
        )
        .unwrap();
        assert_eq!(progress.first(), Some(&0.));
        assert_eq!(progress.last(), Some(&1.));
        assert!(progress.windows(2).all(|w| w[0] <= w[1]));
        if name.contains("intra") {
            assert_eq!(quick, vec![1., 2.], "{name}");
            assert_eq!(result.seconds, quick);
        } else {
            assert!(quick.is_empty(), "{name}");
            if name.contains("motion") {
                assert!(result.seconds.is_empty());
            } else {
                assert_eq!(result.seconds.len(), 2, "{name}");
            }
        }
    }
}

#[test]
#[ignore = "requires scene fixtures and FFmpeg"]
fn pipeline_cancellation_between_phases_never_reports_completion() {
    use media_analysis::scenes::{Phase, scan};
    for cancel_phase in ["verifying", "full"] {
        let token = Cancellation::default();
        let result = scan(
            &tools(),
            &fixture("scene-intra-vfr.mkv"),
            &token,
            Duration::from_secs(30),
            Detail::Standard,
            |update| {
                if (cancel_phase == "verifying" && matches!(update.phase, Phase::Verifying))
                    || (cancel_phase == "full" && matches!(update.phase, Phase::Full))
                {
                    token.cancel();
                }
            },
        );
        assert!(matches!(result, Err(Error::Cancelled)));
    }
}
