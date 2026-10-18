//! Opt-in tests require synthetic fixtures and FFmpeg, never a codec pack or Tauri.
use media_analysis::{Cancellation, Error, Tools, analyze};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

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
fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/generated")
        .join(name)
}
#[test]
#[ignore = "requires generated fixtures and FFmpeg; see tests/fixtures/README.md"]
fn offset_and_selected_track_match_project_time_for_cfr_and_vfr() {
    for file in ["cfr-offset.mkv", "vfr-offset.mkv"] {
        for (ordinal, pulse, amplitude) in [(0, 125, 0.75), (1, 200, -0.5)] {
            let waveform = analyze(
                &tools(),
                &fixture(file),
                ordinal,
                &Cancellation::default(),
                Duration::from_secs(30),
            )
            .unwrap();
            assert_eq!(waveform.audio_stream_index, ordinal + 1);
            assert_eq!(waveform.sample_count, 32_000);
            let first = waveform
                .peaks
                .iter()
                .position(|p| p.min.abs().max(p.max.abs()) > 0.1)
                .unwrap();
            assert_eq!(first, pulse, "{file}, audio {ordinal}");
            let value = if amplitude > 0.0 {
                waveform.peaks[first].max
            } else {
                waveform.peaks[first].min
            };
            assert!((value - amplitude).abs() < 0.001);
        }
    }
}
#[test]
#[ignore = "requires generated fixtures and FFmpeg; see tests/fixtures/README.md"]
fn pcm_wav_and_broadcast_wav_use_verified_sample_zero() {
    for file in ["zero-start.wav", "bwf-zero-start.wav"] {
        let waveform = analyze(
            &tools(),
            &fixture(file),
            0,
            &Cancellation::default(),
            Duration::from_secs(30),
        )
        .unwrap();
        assert_eq!(waveform.container_origin_seconds, 0.0, "{file}");
        assert_eq!(waveform.audio_stream_index, 0, "{file}");
        assert_eq!(waveform.sample_count, 32_000, "{file}");
        assert_eq!(waveform.peaks.len(), 400, "{file}");
        let first = waveform.peaks.iter().position(|p| p.max > 0.1);
        assert_eq!(first, Some(125), "{file}");
        assert!((waveform.peaks[125].max - 0.75).abs() < 0.001, "{file}");
    }
}

#[test]
#[ignore = "requires generated fixtures and FFmpeg; see tests/fixtures/README.md"]
fn long_timeline_has_no_accumulated_bucket_drift() {
    let start = Instant::now();
    let waveform = analyze(
        &tools(),
        &fixture("long.flac"),
        0,
        &Cancellation::default(),
        Duration::from_secs(120),
    )
    .unwrap();
    assert_eq!(waveform.sample_count, 57_600_000);
    assert_eq!(waveform.peaks.len(), 720_000);
    assert_eq!(
        waveform.peaks.iter().position(|p| p.max > 0.1),
        Some(719_900)
    );
    eprintln!(
        "Two-hour waveform: {:.3}s, {} peaks",
        start.elapsed().as_secs_f64(),
        waveform.peaks.len()
    );
}
#[test]
#[ignore = "requires generated fixtures and FFmpeg; see tests/fixtures/README.md"]
fn missing_track_and_invalid_media_return_useful_errors() {
    let error = analyze(
        &tools(),
        &fixture("cfr-offset.mkv"),
        9,
        &Cancellation::default(),
        Duration::from_secs(30),
    )
    .unwrap_err();
    assert!(error.to_string().contains("Audio track 9 does not exist"));
    let error = analyze(
        &tools(),
        &fixture("generation.json"),
        0,
        &Cancellation::default(),
        Duration::from_secs(30),
    )
    .unwrap_err();
    assert!(error.to_string().contains("ffprobe"));
}
#[test]
#[ignore = "requires generated fixtures and FFmpeg; see tests/fixtures/README.md"]
fn cancelled_and_timed_out_jobs_return_no_waveform() {
    let cancellation = Cancellation::default();
    cancellation.cancel();
    assert!(matches!(
        analyze(
            &tools(),
            &fixture("long.flac"),
            0,
            &cancellation,
            Duration::from_secs(30)
        ),
        Err(Error::Cancelled)
    ));
    assert!(matches!(
        analyze(
            &tools(),
            &fixture("long.flac"),
            0,
            &Cancellation::default(),
            Duration::ZERO
        ),
        Err(Error::Timeout)
    ));
    let cancellation = Cancellation::default();
    let copy = cancellation.clone();
    let start = Instant::now();
    let worker = std::thread::spawn(move || {
        analyze(
            &tools(),
            &fixture("long.flac"),
            0,
            &copy,
            Duration::from_secs(30),
        )
    });
    std::thread::sleep(Duration::from_millis(100));
    cancellation.cancel();
    assert!(matches!(worker.join().unwrap(), Err(Error::Cancelled)));
    assert!(
        start.elapsed() < Duration::from_secs(5),
        "Cancellation/cleanup stalled"
    );
}
