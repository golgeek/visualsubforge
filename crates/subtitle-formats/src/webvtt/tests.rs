use super::*;
use subtitle_core::{Edit, Editor};

const FIXTURE: &str = include_str!("../../../../tests/fixtures/subtitles/styles.vtt");
fn ms(value: u64) -> Milliseconds {
    Milliseconds::new(value).unwrap()
}
fn editor(input: &str) -> Editor<Source> {
    let parsed = parse(input).unwrap();
    Editor::with_properties(parsed.document, parsed.source)
}
fn output(editor: &Editor<Source>) -> String {
    let text = serialize(editor.document(), editor.properties()).unwrap();
    assert_eq!(
        serialized_len(editor.document(), editor.properties()).unwrap(),
        text.len()
    );
    text
}
fn apply(editor: &mut Editor<Source>, edit: Edit) -> Result<()> {
    editor
        .apply_edit_with_properties(editor.revision(), edit, |document, source| {
            serialized_len(document, source).map(|_| ())
        })
        .map(|_| ())
}

#[test]
fn preserves_complete_source_and_format_specific_metadata() {
    let p = parse(FIXTURE).unwrap();
    assert_eq!(p.document.cues().len(), 2);
    assert_eq!(p.document.cues()[0].start(), ms(1001));
    assert_eq!(
        metadata(&p.document.cues()[0]).unwrap().identifier,
        "intro,one"
    );
    assert!(
        metadata(&p.document.cues()[0])
            .unwrap()
            .settings
            .contains("position:20%,line-left")
    );
    assert_eq!(p.source.blocks(BlockKind::Style).count(), 1);
    assert_eq!(p.source.blocks(BlockKind::Region).count(), 1);
    assert_eq!(p.source.blocks(BlockKind::Note).count(), 2);
    assert!(
        p.source
            .blocks(BlockKind::Style)
            .next()
            .unwrap()
            .contains("::cue(.accent)")
    );
    assert_eq!(serialize(&p.document, &p.source).unwrap(), FIXTURE);
    assert_eq!(
        serialized_len(&p.document, &p.source).unwrap(),
        FIXTURE.len()
    );
    assert!(Arc::ptr_eq(&p.source.0, &p.source.clone().0));
    assert!(p.document.cues()[1].text().contains("<00:02.500>"));
    assert!(
        metadata(&p.document.cues()[1])
            .unwrap()
            .settings
            .contains("future-setting:keep")
    );
}

#[test]
fn exact_millisecond_clock_and_invalid_or_overflowing_times() {
    for (input, value) in [
        ("00:00.001", 1),
        ("59:59.999", 3_599_999),
        ("01:00:00.000", 3_600_000),
        ("9999:59:59.999", Milliseconds::MAX),
        ("00000001:00:00.001", 3_600_001),
    ] {
        assert_eq!(parse_time(input).unwrap(), ms(value));
        assert_eq!(parse_time(&format_time(ms(value))).unwrap(), ms(value));
    }
    for bad in [
        "0:00.000",
        "00:0.000",
        "00:00.00",
        "00:00,000",
        "-00:00.001",
        "60:00.000",
        "00:60.000",
        "0:00:00.000",
        "00:00:60.000",
        "10000:00:00.000",
        "18446744073709551615:00:00.000",
        "18446744073709551616:00:00.000",
        "00:00.0000",
        "00:00.000 ",
        "00:00:00:00.000",
        "00:\u{00a0}0.000",
        "００:00.000",
    ] {
        assert!(parse_time(bad).is_err(), "accepted {bad:?}");
    }
}

#[test]
fn bom_line_endings_whitespace_and_eof_are_preserved() {
    for input in [
        "WEBVTT",
        "WEBVTT\n",
        "WEBVTT\r\n\r\n",
        "WEBVTT title --> other\n\n",
        "\u{feff}WEBVTT title\r\r",
        "WEBVTT\n\n00:00.001 --> 00:00.002",
        "WEBVTT\n\n00:00.001 --> 00:00.002\n",
        "WEBVTT\n\n00:00.001 --> 00:00.002\n\n",
        "WEBVTT\n\n00:00.001 --> 00:00.002\ntext",
        "\u{feff}WEBVTT mixed\r\n\nNOTE a\rb\r\n\r00:01.001\t--> \t00:03.999 \talign:start \t\r\n世界\rline\n \r\n\n",
    ] {
        assert_eq!(output(&editor(input)), input);
    }
    for newline in ["\n", "\r\n", "\r"] {
        let input = format!("\u{feff}{}", FIXTURE.replace('\n', newline));
        let mut e = editor(&input);
        let cue = e.document().cues()[0].clone();
        apply(
            &mut e,
            Edit::Text {
                id: cue.id(),
                start: ms(1002),
                end: cue.end(),
                text: cue.text().into(),
            },
        )
        .unwrap();
        assert_eq!(output(&e), input.replacen("00:01.001", "00:00:01.002", 1));
        e.undo(e.revision()).unwrap();
        assert_eq!(output(&e), input);
        assert!(!e.dirty());
    }
}

#[test]
fn edited_text_times_and_properties_share_history_and_restore_bytes() {
    let mut e = editor(FIXTURE);
    let old = e.document().cues()[0].clone();
    apply(
        &mut e,
        Edit::Text {
            id: old.id(),
            start: old.start(),
            end: ms(4001),
            text: "<v Alice>été\n世界</v>".into(),
        },
    )
    .unwrap();
    let changed = output(&e);
    assert_eq!(
        changed,
        FIXTURE.replacen("00:03.999", "00:00:04.001", 1).replacen(
            old.text(),
            "<v Alice>été\n世界</v>",
            1
        )
    );
    let mut cues = e.document().cues().to_vec();
    cues[0] = cues[0].clone().with_metadata(
        cue_metadata("intro,renamed".into(), "line:20%,end align:left".into()).unwrap(),
    );
    e.apply_document(
        e.revision(),
        Document::new(cues).unwrap(),
        e.properties().clone(),
        |document, source| serialized_len(document, source).map(|_| ()),
    )
    .unwrap();
    let authored = output(&e);
    let reopened = parse(&authored).unwrap();
    assert_eq!(
        metadata(&reopened.document.cues()[0]).unwrap().identifier,
        "intro,renamed"
    );
    assert_eq!(
        metadata(&reopened.document.cues()[0]).unwrap().settings,
        "line:20%,end align:left"
    );
    assert_eq!(
        reopened.source.blocks(BlockKind::Style).collect::<Vec<_>>(),
        e.properties().blocks(BlockKind::Style).collect::<Vec<_>>()
    );
    e.undo(e.revision()).unwrap();
    assert_eq!(output(&e), changed);
    e.undo(e.revision()).unwrap();
    assert_eq!(output(&e), FIXTURE);
    assert!(!e.dirty());
    e.redo(e.revision()).unwrap();
    e.redo(e.revision()).unwrap();
    assert_eq!(output(&e), authored);
}

#[test]
fn insertion_deletion_and_empty_payload_edits_keep_block_boundaries() {
    let mut e = editor(FIXTURE);
    apply(
        &mut e,
        Edit::Create {
            after: Some(CueId(1)),
            start: ms(1500),
            end: ms(1701),
            text: "new".into(),
            metadata: Some(default_metadata().unwrap()),
        },
    )
    .unwrap();
    let added = output(&e);
    assert_eq!(parse(&added).unwrap().document.cues().len(), 3);
    apply(&mut e, Edit::Delete { id: CueId(1) }).unwrap();
    let deleted = output(&e);
    let p = parse(&deleted).unwrap();
    assert_eq!(p.document.cues().len(), 2);
    assert_eq!(p.source.blocks(BlockKind::Note).count(), 2);
    assert_eq!(
        p.source.blocks(BlockKind::Region).collect::<Vec<_>>(),
        e.properties().blocks(BlockKind::Region).collect::<Vec<_>>()
    );
    e.undo(e.revision()).unwrap();
    assert_eq!(output(&e), added);
    e.undo(e.revision()).unwrap();
    assert_eq!(output(&e), FIXTURE);
    for input in [
        "WEBVTT",
        "WEBVTT\r",
        "WEBVTT\n\nSTYLE\n::cue { color: red; }",
    ] {
        let mut e = editor(input);
        apply(
            &mut e,
            Edit::Create {
                after: None,
                start: ms(1),
                end: ms(2001),
                text: "{} literal ASS\\N".into(),
                metadata: Some(default_metadata().unwrap()),
            },
        )
        .unwrap();
        assert_eq!(
            parse(&output(&e)).unwrap().document.cues()[0].text(),
            "{} literal ASS\\N"
        );
        e.undo(e.revision()).unwrap();
        assert_eq!(output(&e), input);
    }
    for input in [
        "WEBVTT\n\n00:00.001 --> 00:02.001",
        "WEBVTT\n\n00:00.001 --> 00:02.001\n\n00:03.000 --> 00:04.000\nx",
    ] {
        let mut e = editor(input);
        apply(
            &mut e,
            Edit::Text {
                id: CueId(1),
                start: ms(1),
                end: ms(2001),
                text: "new\ntext".into(),
            },
        )
        .unwrap();
        assert_eq!(
            parse(&output(&e)).unwrap().document.cues()[0].text(),
            "new\ntext"
        );
        e.undo(e.revision()).unwrap();
        assert_eq!(output(&e), input);
    }
}

#[test]
fn invalid_edits_roll_back_without_losing_redo_or_original_records() {
    let mut e = editor(FIXTURE);
    apply(&mut e, Edit::Delete { id: CueId(1) }).unwrap();
    e.undo(e.revision()).unwrap();
    let revision = e.revision();
    let cue = e.document().cues()[1].clone();
    for (start, end, text) in [
        (2500, 5000, cue.text()),
        (2000, 4000, cue.text()),
        (2000, 5000, "a\n\nb"),
        (2000, 5000, "text --> cue"),
        (2000, 5000, "<00:04.000>a<00:03.000>b"),
        (2000, 5000, "<00:04.000>a<00:04.000>b"),
    ] {
        assert!(
            apply(
                &mut e,
                Edit::Text {
                    id: cue.id(),
                    start: ms(start),
                    end: ms(end),
                    text: text.into()
                }
            )
            .is_err()
        );
        assert_eq!(e.revision(), revision);
        assert!(e.can_redo());
        assert!(!e.dirty());
        assert_eq!(output(&e), FIXTURE);
    }
    // Splitting a named cue must never silently duplicate its external identifier.
    assert!(
        apply(
            &mut e,
            Edit::Split {
                id: CueId(1),
                at: ms(1500)
            }
        )
        .is_err()
    );
    assert_eq!(e.revision(), revision);
    assert_eq!(output(&e), FIXTURE);
    let mut unnamed = editor("WEBVTT\n\n00:00.001 --> 00:02.001\nhello");
    apply(
        &mut unnamed,
        Edit::Split {
            id: CueId(1),
            at: ms(1001),
        },
    )
    .unwrap();
    assert_eq!(parse(&output(&unnamed)).unwrap().document.cues().len(), 2);
    unnamed.undo(unnamed.revision()).unwrap();
    assert_eq!(output(&unnamed), "WEBVTT\n\n00:00.001 --> 00:02.001\nhello");
}

#[test]
fn malformed_or_ambiguous_input_is_never_silently_dropped() {
    for input in [
        "",
        "WEBVTTX\n\n",
        " WEBVTT\n\n",
        "WEBVTT\0\n\n",
        "WEBVTT\n00:00.000 --> 00:01.000\nx",
        "WEBVTT\nX-TIMESTAMP-MAP=LOCAL:00:00.000,MPEGTS:900000\n\n",
        "WEBVTT\n\nCUSTOM\nmetadata",
        "WEBVTT\n\nid",
        "WEBVTT\n\n00:00.000-->00:01.000\nx",
        "WEBVTT\n\n00:00.000 -->00:01.000\nx",
        "WEBVTT\n\n00:01.000 --> 00:01.000\nx",
        "WEBVTT\n\n00:01.000 --> 00:02.000\nx --> y",
        "WEBVTT\n\nNOTE\nx --> y",
        "WEBVTT\n\n00:01.000 --> 00:02.000\nx\n\nSTYLE\n::cue {color:red}",
        "WEBVTT\n\na\n00:01.000 --> 00:02.000\nx\n\na\n00:03.000 --> 00:04.000\ny",
        "WEBVTT\n\n00:03.000 --> 00:04.000\nx\n\n00:01.000 --> 00:02.000\ny",
        "WEBVTT\n\n00:01.000 --> 00:02.000\n<00:00.500>x",
        "WEBVTT\n\n00:01.000 --> 00:02.000\n<00:01.5000>x",
    ] {
        assert!(parse(input).is_err(), "accepted {input:?}");
    }
    for (id, settings) in [
        ("NOTE", ""),
        ("NOTE hi", ""),
        ("STYLE\t", ""),
        ("x\ny", ""),
        ("x", "foo --> bar"),
        ("x", " line:1"),
        ("x", "line:1\nsize:20%"),
    ] {
        assert!(cue_metadata(id.into(), settings.into()).is_err());
    }
}

#[test]
fn metadata_validation_rejects_boundary_injection_and_foreign_fields_atomically() {
    let mut e = editor(FIXTURE);
    apply(&mut e, Edit::Delete { id: CueId(1) }).unwrap();
    e.undo(e.revision()).unwrap();
    let revision = e.revision();
    for fields in [
        vec![("Style", "Default")],
        vec![("Identifier", "intro,one"), ("Settings", "")],
        vec![("Identifier", "NOTE unsafe"), ("Settings", "")],
        vec![("Identifier", "song"), ("Settings", "align:start -->")],
        vec![("Identifier", "song"), ("Settings", " align:start")],
        vec![
            ("Identifier", "song"),
            ("Settings", ""),
            ("Foreign", "value"),
        ],
    ] {
        let mut cues = e.document().cues().to_vec();
        cues[1] = cues[1].clone().with_metadata(
            CueMetadata::new(
                fields
                    .into_iter()
                    .map(|(name, value)| (name.into(), value.into()))
                    .collect(),
            )
            .unwrap(),
        );
        assert!(
            e.apply_document(
                revision,
                Document::new(cues).unwrap(),
                e.properties().clone(),
                |d, s| serialized_len(d, s).map(|_| ())
            )
            .is_err()
        );
        assert_eq!(e.revision(), revision);
        assert!(e.can_redo());
        assert!(!e.dirty());
        assert_eq!(output(&e), FIXTURE);
    }
    e.redo(revision).unwrap();
    assert_eq!(e.document().cues().len(), 1);
}

#[test]
fn ten_thousand_cues_keep_identifiers_settings_unicode_and_last_edit() {
    let mut raw = String::from("WEBVTT 10k\n\nSTYLE\n::cue { color: lime; }\n\n");
    for i in 0..10000 {
        raw.push_str(&format!(
            "cue,{i}\n{} --> {} line:10%,start unknown:keep\n<v Alice>世界 &amp; été {i}</v>\n\n",
            format_time(ms(i * 1500 + 1)),
            format_time(ms(i * 1500 + 1001))
        ));
    }
    let mut e = editor(&raw);
    assert_eq!(e.document().cues().len(), 10000);
    assert_eq!(output(&e), raw);
    let last = e.document().cues().last().unwrap().clone();
    apply(
        &mut e,
        Edit::Text {
            id: last.id(),
            start: last.start(),
            end: last.end(),
            text: "last — 😀".into(),
        },
    )
    .unwrap();
    let p = parse(&output(&e)).unwrap();
    assert_eq!(p.document.cues().len(), 10000);
    assert_eq!(p.document.cues().last().unwrap().text(), "last — 😀");
    assert_eq!(
        metadata(&p.document.cues()[9999]).unwrap().identifier,
        "cue,9999"
    );
    e.undo(e.revision()).unwrap();
    assert_eq!(output(&e), raw);
}

#[test]
fn ass_delimiter_validation_remains_at_the_format_boundary() {
    let input = include_str!("../../../../tests/fixtures/subtitles/styles.ass");
    let p = crate::ass::parse(input).unwrap();
    let mut e = Editor::with_properties(p.document, p.source);
    let mut cues = e.document().cues().to_vec();
    let mut fields = cues[0].metadata().unwrap().fields().to_vec();
    let actor = fields
        .iter_mut()
        .find(|(name, _)| name.eq_ignore_ascii_case("Name"))
        .unwrap();
    actor.1 = "Alice,Bob".into();
    cues[0] = cues[0]
        .clone()
        .with_metadata(CueMetadata::new(fields).unwrap());
    assert!(
        e.apply_document(
            0,
            Document::new(cues).unwrap(),
            e.properties().clone(),
            |d, s| crate::ass::serialized_len(d, s).map(|_| ())
        )
        .is_err()
    );
    assert_eq!(e.revision(), 0);
    assert!(!e.can_undo());
    assert!(!e.dirty());
    assert_eq!(
        crate::ass::serialize(e.document(), e.properties()).unwrap(),
        input
    );
}
