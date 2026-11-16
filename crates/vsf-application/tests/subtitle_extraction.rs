use media_analysis::{Cancellation, Tools, subtitle_tracks};
use std::{path::PathBuf, time::Duration};
use vsf_application::subtitle_extraction;
#[test]
#[ignore = "requires synthetic scripts/media-fixtures.py corpus and FFmpeg"]
fn generated_tracks_preserve_project_timing_and_ass_order() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/generated/subtitle-extraction");
    let tools = Tools::discover().unwrap();
    let cancel = Cancellation::default();
    let timeout = Duration::from_secs(30);
    let path = root.join("offset.mkv");
    let listing = subtitle_tracks::list(&tools, &path, &cancel, timeout).unwrap();
    assert_eq!(listing.tracks.len(), 3);
    assert_eq!(listing.font_count, 1);
    for track in listing.tracks {
        let result =
            subtitle_extraction::extract(&tools, &path, track.index, &cancel, timeout).unwrap();
        let cues = result.parsed.document.cues();
        match track.codec.as_str() {
            "ass" => {
                assert_eq!(
                    cues.iter().map(|c| c.start().get()).collect::<Vec<_>>(),
                    vec![3000, 1000]
                );
                let text = result
                    .parsed
                    .source
                    .serialize(&result.parsed.document)
                    .unwrap();
                assert!(text.contains("Original extraction fixture header"));
                assert!(text.contains("Comment: 0,0:00:01.00,0:00:02.00"));
                assert!(text.contains("Style: Signs"));
            }
            "subrip" => {
                assert_eq!(cues[0].start().get(), 1000);
                assert_eq!(cues[0].text(), "First");
                assert_eq!(cues[1].end().get(), 4000);
            }
            "webvtt" => {
                assert_eq!(cues[0].start().get(), 1000);
                assert_eq!(cues[0].text(), "<v Test>First");
            }
            other => panic!("unexpected {other}"),
        }
    }
    let ssa =
        subtitle_extraction::extract(&tools, &root.join("ssa.mkv"), 0, &cancel, timeout).unwrap();
    assert_eq!(ssa.parsed.source.format(), subtitle_formats::Format::Ssa);
    assert_eq!(ssa.provenance.suggested_name, "ssa.ssa");
    let destination = tempfile::tempdir().unwrap();
    subtitle_extraction::publish(&ssa, &destination.path().join("copy.ssa"), &cancel).unwrap();
    let mp4 =
        subtitle_extraction::extract(&tools, &root.join("text.mp4"), 1, &cancel, timeout).unwrap();
    assert_eq!(mp4.parsed.document.cues()[0].start().get(), 1000);
    assert_eq!(mp4.provenance.codec, "mov_text");
    let many =
        subtitle_extraction::extract(&tools, &root.join("ten-thousand.mkv"), 0, &cancel, timeout)
            .unwrap();
    assert_eq!(many.parsed.document.cues().len(), 10_000);
}
