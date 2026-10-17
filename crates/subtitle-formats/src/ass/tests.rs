use super::*;
use subtitle_core::{Edit, Editor};
const HEADER: &str = "[Script Info]\nScriptType: v4.00+\nPlayResX: 1920\nPlayResY: 1080\nWrapStyle: 2\nUnknown-Key: keep : everything\n; comment\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, Unknown\nStyle: Default,Arial,42,preserve\nStyle: Title,Hiragino Sans,64,more\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Extra, Text\n";
const FIRST: &str = "Dialogue: 3, 0:00:01.20 ,0:00:03.45,Default,Actor,0010,0020,0030,,custom,{\\an8\\c&H00FFFF&}日本語 😀\\Ntwo, commas, stay\n";
const SECOND: &str = "Dialogue: 0,0:00:04.00,0:00:06.00,Title,,0,0,0,,unknown,second\n";
fn source() -> String {
    format!(
        "{HEADER}{FIRST}Comment: 0,not parsed and preserved\n{SECOND}[Unknown Section]\nopaque, bytes : retained\n[Fonts]\nfontname: fake.ttf\n!encodedFakeFont!\n"
    )
}
fn ms(t: u64) -> Milliseconds {
    Milliseconds::new(t).unwrap()
}
#[test]
fn noop_preserves_all_bytes_bom_endings_unknowns_and_variants() {
    let lf = source();
    let mixed = format!(
        "\u{feff}{}",
        lf.replace("ScriptType: v4.00+\n", "ScriptType: v4.00+\r\n")
    );
    let ssa = lf
        .replace("v4.00+", "v4.00")
        .replace("[V4+ Styles]", "[V4 Styles]")
        .replace("Format: Layer,", "Format: Marked,")
        .replace("Dialogue: 3,", "Dialogue: Marked=0,")
        .replace("Dialogue: 0,", "Dialogue: Marked=1,");
    for input in [
        lf.clone(),
        lf.replace('\n', "\r\n"),
        lf.trim_end_matches('\n').into(),
        mixed,
        ssa,
    ] {
        let parsed = parse(&input).unwrap();
        assert_eq!(parsed.document.cues().len(), 2);
        assert_eq!(serialize(&parsed.document, &parsed.source).unwrap(), input);
        assert_eq!(
            serialized_len(&parsed.document, &parsed.source).unwrap(),
            input.len()
        );
        assert_eq!(parsed.source.styles()[1].name, "Title");
        assert!(
            parsed
                .source
                .script_info()
                .contains(&("PlayResX".into(), "1920".into()))
        );
        assert_eq!(
            parsed.document.cues()[0].metadata().unwrap().get("Extra"),
            Some("custom")
        );
    }
}
#[test]
fn changed_fields_only_and_undo_restore_original_bytes() {
    let input = source();
    let parsed = parse(&input).unwrap();
    let mut editor = Editor::new(parsed.document);
    editor
        .edit(
            0,
            CueId(1),
            ms(1250),
            ms(3450),
            "{\\i1}new\\Ntext, with comma".into(),
        )
        .unwrap();
    let expected = input.replace(" 0:00:01.20 ", " 0:00:01.25 ").replace(
        "{\\an8\\c&H00FFFF&}日本語 😀\\Ntwo, commas, stay",
        "{\\i1}new\\Ntext, with comma",
    );
    assert_eq!(
        serialize(editor.document(), &parsed.source).unwrap(),
        expected
    );
    editor
        .apply_edit(
            1,
            Edit::Style {
                id: CueId(1),
                name: "Title".into(),
            },
            |_| Ok(()),
        )
        .unwrap();
    let styled = expected.replace(",Default,Actor,", ",Title,Actor,");
    assert_eq!(
        serialize(editor.document(), &parsed.source).unwrap(),
        styled
    );
    editor.undo(2).unwrap();
    assert_eq!(
        serialize(editor.document(), &parsed.source).unwrap(),
        expected
    );
    editor.undo(3).unwrap();
    assert_eq!(serialize(editor.document(), &parsed.source).unwrap(), input);
    editor.redo(4).unwrap();
    editor.redo(5).unwrap();
    assert_eq!(
        serialize(editor.document(), &parsed.source).unwrap(),
        styled
    );
}
#[test]
fn reordered_case_insensitive_fields_and_multiple_formats_keep_mapping() {
    let input = "[Script Info]\nScriptType: v4.00+\n[V4+ Styles]\nFormat: Fontsize, Name, Fontname\nStyle: 36,Default,Arial\n[Events]\nFormat: End, sTyLe, Custom, START, Text\nDialogue: 0:00:02.00,Default,X,0:00:01.00,{\\r}a,b\nFormat: Style, Start, End, Text\nDialogue: Default,0:00:04.00,0:00:05.00,next";
    let parsed = parse(input).unwrap();
    assert_eq!(parsed.document.cues()[0].start().get(), 1000);
    assert_eq!(parsed.document.cues()[0].text(), "{\\r}a,b");
    assert_eq!(serialize(&parsed.document, &parsed.source).unwrap(), input);
    let mut editor = Editor::new(parsed.document);
    editor
        .edit(0, CueId(1), ms(1230), ms(2100), "changed,comma".into())
        .unwrap();
    assert_eq!(
        serialize(editor.document(), &parsed.source).unwrap(),
        input.replace(
            "0:00:02.00,Default,X,0:00:01.00,{\\r}a,b",
            "0:00:02.10,Default,X,0:00:01.23,changed,comma"
        )
    );
    editor.split(1, CueId(1), ms(1500)).unwrap();
    // Insertion under a different layout must not silently drop Custom.
    assert!(
        serialize(editor.document(), &parsed.source)
            .unwrap_err()
            .contains("metadata cannot")
    );
}
#[test]
fn create_delete_split_preserve_metadata_opaque_records_and_empty_documents() {
    let input = source();
    let parsed = parse(&input).unwrap();
    let mut editor = Editor::new(parsed.document);
    editor
        .apply_edit(
            0,
            Edit::Create {
                after: Some(CueId(1)),
                start: ms(3500),
                end: ms(3900),
                text: "new".into(),
                metadata: Some(parsed.source.default_metadata().unwrap()),
            },
            |_| Ok(()),
        )
        .unwrap();
    let output = serialize(editor.document(), &parsed.source).unwrap();
    let again = parse(&output).unwrap();
    assert_eq!(
        again
            .document
            .cues()
            .iter()
            .map(Cue::text)
            .collect::<Vec<_>>(),
        vec![editor.document().cues()[0].text(), "new", "second"]
    );
    assert!(output.contains("Comment: 0,not parsed and preserved\n"));
    assert!(output.ends_with("fontname: fake.ttf\n!encodedFakeFont!\n"));
    editor.undo(1).unwrap();
    assert_eq!(serialize(editor.document(), &parsed.source).unwrap(), input);
    let new = editor.split(2, CueId(1), ms(2000)).unwrap();
    assert_eq!(
        editor.document().cues()[1].metadata(),
        editor.document().cues()[0].metadata()
    );
    let split = parse(&serialize(editor.document(), &parsed.source).unwrap()).unwrap();
    assert_eq!(split.document.cues().len(), 3);
    editor.delete(3, new).unwrap();
    editor.delete(4, CueId(1)).unwrap();
    editor.delete(5, CueId(2)).unwrap();
    let empty = serialize(editor.document(), &parsed.source).unwrap();
    assert_eq!(parse(&empty).unwrap().document.cues().len(), 0);
    assert!(empty.contains("Comment:") && empty.contains("[Fonts]"));
    let parsed_empty = parse(HEADER).unwrap();
    let mut fresh = Editor::new(parsed_empty.document);
    fresh
        .apply_edit(
            0,
            Edit::Create {
                after: None,
                start: ms(0),
                end: ms(1000),
                text: "first".into(),
                metadata: Some(parsed_empty.source.default_metadata().unwrap()),
            },
            |_| Ok(()),
        )
        .unwrap();
    assert_eq!(
        parse(&serialize(fresh.document(), &parsed_empty.source).unwrap())
            .unwrap()
            .document
            .cues()
            .len(),
        1
    );
}
#[test]
fn malformed_ambiguous_and_unsupported_sources_fail_explicitly() {
    let valid = format!("{HEADER}{FIRST}");
    for input in [
        valid.replace("v4.00+", "v4.00++"),
        valid.replace("v4.00+", "v3.00"),
        valid.replace("[V4+ Styles]", "[V4 Styles]"),
        valid.replace("Text\nDialogue", "Text, Extra2\nDialogue"),
        valid.replace("Start, End", "Start, START"),
        valid.replace("0:00:03.45", "0:00:03.456"),
        valid.replace("0:00:03.45", "0:60:03.45"),
        valid.replace("0:00:03.45", "0:00:01.00"),
        valid.replace("Dialogue:", "Dialogue"),
        valid.replace("Style: Title", "Style: Default"),
        valid.replace("ScriptType:", "Unknown:"),
        format!("{valid}\0"),
        format!("{valid}[Events]\n"),
        valid.replace("[Events]", "[Even\rts]"),
    ] {
        assert!(parse(&input).is_err(), "accepted malformed source: {input}");
    }
}
#[test]
fn exact_time_grid_and_failed_validation_preserve_saved_state() {
    assert_eq!(parse_time("9999:59:59.99").unwrap().get(), 35_999_999_990);
    assert_eq!(format_time(ms(1230)).unwrap(), "0:00:01.23");
    assert!(format_time(ms(1231)).is_err());
    let parsed = parse(&source()).unwrap();
    let mut editor = Editor::new(parsed.document.clone());
    let result = editor.apply_edit(
        0,
        Edit::Text {
            id: CueId(1),
            start: ms(1201),
            end: ms(3450),
            text: "x".into(),
        },
        |d| serialized_len(d, &parsed.source).map(|_| ()),
    );
    assert!(result.err().unwrap().contains("10 ms increments"));
    assert_eq!(editor.document(), &parsed.document);
    assert_eq!(editor.revision(), 0);
    assert!(!editor.dirty() && !editor.can_undo());
    assert!(parsed.source.validate_style("missing").is_err());
    assert!(parsed.source.validate_style("Title").is_ok());
}
