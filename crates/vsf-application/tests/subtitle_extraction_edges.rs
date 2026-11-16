use media_analysis::{Cancellation, Tools};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
use vsf_application::subtitle_extraction;
#[test]
#[ignore = "requires synthetic media-fixtures.py corpus and FFmpeg"]
fn generated_edge_tracks_are_not_dropped_or_guessed() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/generated/subtitle-extraction");
    let tools = Tools::discover().unwrap();
    let token = Cancellation::default();
    let extract = |name: &str| {
        subtitle_extraction::extract(&tools, &root.join(name), 2, &token, Duration::from_secs(30))
    };
    let repaired = extract("no-duration.mkv").unwrap();
    assert_eq!(repaired.repaired_cues, vec![1, 2]);
    assert_eq!(repaired.parsed.document.cues()[0].end().get(), 3000);
    assert_eq!(repaired.parsed.document.cues()[1].end().get(), 8000);
    assert!(
        extract("empty.mkv")
            .unwrap()
            .parsed
            .document
            .cues()
            .is_empty()
    );
    assert!(
        extract("invalid-utf8.mkv")
            .err()
            .unwrap()
            .contains("isn't valid UTF-8")
    );
}
#[test]
#[ignore = "requires generated 10k extraction corpus and release profile for timing"]
fn ten_thousand_extracted_cues_keep_existing_cpu_budget() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/generated/subtitle-extraction/ten-thousand.mkv");
    let tools = Tools::discover().unwrap();
    let cancel = Cancellation::default();
    let mut samples = Vec::new();
    for iteration in 0..21 {
        let extracted = media_analysis::subtitle_tracks::extract(
            &tools,
            &path,
            0,
            &cancel,
            Duration::from_secs(30),
        )
        .unwrap();
        let started = Instant::now();
        let value = subtitle_extraction::prepare(&path, extracted).unwrap();
        assert_eq!(value.parsed.document.cues().len(), 10_000);
        let mut editor =
            subtitle_core::Editor::with_properties(value.parsed.document, value.parsed.source);
        let first = &editor.document().cues()[0];
        let original = editor.properties().serialize(editor.document()).unwrap();
        let edit = subtitle_core::Edit::Text {
            id: first.id(),
            start: first.start(),
            end: first.end(),
            text: "Edited".into(),
        };
        editor
            .apply_edit_with_properties(0, edit, vsf_application::subtitle_file::validate_document)
            .unwrap();
        let output = editor.properties().serialize(editor.document()).unwrap();
        vsf_application::subtitle_file::parse_document(output.as_bytes(), Some("srt")).unwrap();
        editor.undo(editor.revision()).unwrap();
        assert_eq!(
            editor.properties().serialize(editor.document()).unwrap(),
            original
        );
        if iteration > 0 {
            samples.push(started.elapsed().as_secs_f64() * 1000.);
        }
    }
    samples.sort_by(f64::total_cmp);
    eprintln!(
        "10k extraction parse/edit/serialize/reopen/Undo: p95={:.3}ms max={:.3}ms",
        samples[18], samples[19]
    );
    assert!(samples[18] < 500., "unchanged 500ms CPU budget");
}
