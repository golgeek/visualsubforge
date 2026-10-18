use subtitle_core::Editor;
use subtitle_formats::{Source, ass::Variant};
use vsf_application::authoring::styles::{
    self, StyleFieldValue as FieldValue, StyleOperation as Operation,
};

const ASS: &str = include_str!("../../../tests/fixtures/subtitles/styles.ass");
const SSA: &str = include_str!("../../../tests/fixtures/subtitles/styles.ssa");
fn field(name: &str, value: &str) -> FieldValue {
    FieldValue {
        name: name.into(),
        value: value.into(),
    }
}
fn edit(name: &str, changes: &[(&str, &str)]) -> Operation {
    Operation::Edit {
        name: name.into(),
        fields: changes.iter().map(|(n, v)| field(n, v)).collect(),
    }
}

#[test]
fn style_values_and_creation_preserve_cues_metadata_and_unknown_source() {
    for input in [ASS, SSA] {
        let parsed = subtitle_formats::parse(input, None).unwrap();
        let proposal = styles::propose(
            &parsed.document,
            &parsed.source,
            &edit("Title", &[("Fontsize", "42"), ("Fontname", "Test Family")]),
            || Ok(()),
        )
        .unwrap();
        assert!(proposal.changed);
        assert_eq!(proposal.summary.changed_cues, 0);
        assert_eq!(proposal.summary.affected_cues, 1);
        assert_eq!(proposal.document(), &parsed.document);
        assert_eq!(
            proposal.source().serialize(proposal.document()).unwrap(),
            input.replace(
                "Style: Title,sans-serif,36,",
                "Style: Title,Test Family,42,"
            )
        );
        let mut editor = Editor::with_properties(parsed.document.clone(), parsed.source.clone());
        let (document, source) = proposal.into_parts();
        editor
            .apply_document(
                0,
                document,
                source,
                vsf_application::subtitle_file::validate_document,
            )
            .unwrap();
        editor.undo(1).unwrap();
        assert_eq!(
            editor.properties().serialize(editor.document()).unwrap(),
            input
        );
        editor.redo(2).unwrap();
        let before_create = editor.properties().serialize(editor.document()).unwrap();
        let op = Operation::Create {
            name: "New 世界".into(),
            copy_from: None,
            fields: vec![field("Fontname", "New Font"), field("Bold", "-1")],
        };
        let created =
            styles::propose(editor.document(), editor.properties(), &op, || Ok(())).unwrap();
        assert!(created.summary.created);
        assert_eq!(
            created.summary.fields.len(),
            if parsed.source.ass().unwrap().variant() == Variant::Ass {
                23
            } else {
                18
            }
        );
        assert_eq!(created.document(), editor.document());
        let encoded = created.source().serialize(created.document()).unwrap();
        assert!(encoded.contains("New 世界,New Font,24,"));
        assert_eq!(
            subtitle_formats::parse(&encoded, None).unwrap().document,
            parsed.document
        );
        let (document, source) = created.into_parts();
        editor
            .apply_document(
                3,
                document,
                source,
                vsf_application::subtitle_file::validate_document,
            )
            .unwrap();
        editor.undo(4).unwrap();
        assert_eq!(
            editor.properties().serialize(editor.document()).unwrap(),
            before_create
        );
    }
}

#[test]
fn rename_updates_exact_dialogue_comment_and_reset_references_atomically() {
    let old_text = r"{\rTitle}世界{\rTitleSuffix}tail{\r} plain \rTitle and \{\rTitle}";
    let new_text = r"{\rCredits}世界{\rTitleSuffix}tail{\r} plain \rTitle and \{\rTitle}";
    for input in [ASS, SSA] {
        let original = format!(
            "\u{feff}{}",
            input
                .replace("Second line\\N世界", old_text)
                .replace("Comment: Default,", "Comment: Title,")
                .replace("Keep this comment", r"{\rTitle}Keep Title")
                .replace("Retain this", r"{\rTitle}Retain Title")
                .replace('\n', "\r\n")
        );
        let parsed = subtitle_formats::parse(&original, None).unwrap();
        let proposal = styles::propose(
            &parsed.document,
            &parsed.source,
            &edit("Title", &[("Name", "Credits")]),
            || Ok(()),
        )
        .unwrap();
        let expected = original
            .replace("Style: Title,", "Style: Credits,")
            .replace("Dialogue: Title,", "Dialogue: Credits,")
            .replace("Comment: Title,", "Comment: Credits,")
            .replace(",Title,", ",Credits,")
            .replace(old_text, new_text)
            .replace(r"{\rTitle}Keep Title", r"{\rCredits}Keep Title")
            .replace(r"{\rTitle}Retain Title", r"{\rCredits}Retain Title");
        assert_eq!(
            proposal.source().serialize(proposal.document()).unwrap(),
            expected
        );
        assert_eq!(proposal.summary.changed_cues, 2);
        assert_eq!(proposal.summary.changed_comments, 1);
        assert_eq!(proposal.summary.affected_cues, 2);
        assert_eq!(proposal.summary.reference_review_cues, 0);
        let mut editor = Editor::with_properties(parsed.document, parsed.source);
        let (document, source) = proposal.into_parts();
        editor
            .apply_document(
                0,
                document,
                source,
                vsf_application::subtitle_file::validate_document,
            )
            .unwrap();
        editor.undo(1).unwrap();
        assert_eq!(
            editor.properties().serialize(editor.document()).unwrap(),
            original
        );
        editor.redo(2).unwrap();
        assert_eq!(
            editor.properties().serialize(editor.document()).unwrap(),
            expected
        );
        let again = styles::propose(
            editor.document(),
            editor.properties(),
            &edit("Credits", &[("Name", "End Credits")]),
            || Ok(()),
        )
        .unwrap();
        assert_eq!(again.summary.changed_comments, 1);
        assert_eq!(
            again.source().serialize(again.document()).unwrap(),
            expected.replace("Credits", "End Credits")
        );
    }
}

#[test]
fn invalid_fields_conflicts_opaque_comments_and_ambiguous_resets_are_refused() {
    for input in [ASS, SSA] {
        let parsed = subtitle_formats::parse(input, None).unwrap();
        for op in [
            edit("Title", &[("Fontsize", "0")]),
            edit("Title", &[("Fontsize", "NaN")]),
            edit("Title", &[("Fontsize", "1001")]),
            edit("Title", &[("Name", "Default")]),
            edit("Default", &[("Name", "Renamed")]),
            edit("Title", &[("MarginL", "-1")]),
            edit("Title", &[("PrimaryColour", "&H100000000")]),
            edit("Title", &[("BorderStyle", "2")]),
            edit("Title", &[("Fontname", "bad,font")]),
            edit("Title", &[("Encoding", "256")]),
            edit("Title", &[("Bogus", "1")]),
            edit("Title", &[("Fontsize", "30"), ("fontsize", "40")]),
        ] {
            assert!(
                styles::propose(&parsed.document, &parsed.source, &op, || Ok(())).is_err(),
                "{op:?}"
            );
            assert_eq!(parsed.source.serialize(&parsed.document).unwrap(), input);
        }
    }
    for text in [
        r"{\t(0,100,\rTitle)}x",
        r"{\rTitle }x",
        r"{\rTitle",
        r"{nested{\rTitle}}",
        r"{\ rTitle}",
        r"{\r(Title)}",
    ] {
        let input = ASS.replace("Red title", text);
        let parsed = subtitle_formats::parse(&input, None).unwrap();
        assert!(
            styles::propose(
                &parsed.document,
                &parsed.source,
                &edit("Title", &[("Name", "New")]),
                || Ok(())
            )
            .is_err()
        );
        let safe = styles::propose(
            &parsed.document,
            &parsed.source,
            &edit("Title", &[("Fontsize", "42")]),
            || Ok(()),
        )
        .unwrap();
        assert_eq!(safe.summary.reference_review_cues, 1);
        assert_eq!(safe.document(), &parsed.document);
    }
    let input = ASS.replace(
        "Comment: Default,0:00:02.00,0:00:01.00,0,comment,0,0,0,,opaque,Keep this comment",
        "Comment: opaque,unparseable",
    );
    let parsed = subtitle_formats::parse(&input, None).unwrap();
    assert!(
        styles::propose(
            &parsed.document,
            &parsed.source,
            &edit("Title", &[("Name", "New")]),
            || Ok(())
        )
        .err()
        .unwrap()
        .contains("Comment")
    );
}

#[test]
fn legacy_event_stars_are_preserved_while_ambiguous_definition_aliases_are_refused() {
    let input = ASS
        .replace("Dialogue: Title,", "Dialogue: **Title,")
        .replace("Comment: Default,", "Comment: *Title,");
    let parsed = subtitle_formats::parse(&input, None).unwrap();
    let proposal = styles::propose(
        &parsed.document,
        &parsed.source,
        &edit("Title", &[("Name", "Credits")]),
        || Ok(()),
    )
    .unwrap();
    assert_eq!(
        proposal.source().serialize(proposal.document()).unwrap(),
        input
            .replace("Style: Title,", "Style: Credits,")
            .replace("**Title,", "**Credits,")
            .replace("*Title,", "*Credits,")
    );
    let aliased = ASS.replace("Style: Default,", "Style: *Title,");
    let parsed = subtitle_formats::parse(&aliased, None).unwrap();
    assert!(
        styles::propose(
            &parsed.document,
            &parsed.source,
            &edit("Title", &[("Name", "Credits")]),
            || Ok(())
        )
        .is_err()
    );
    assert!(
        styles::propose(
            &parsed.document,
            &parsed.source,
            &edit("*Title", &[("Name", "Credits")]),
            || Ok(())
        )
        .is_err()
    );
    for name in ["*New", "DEFAULT", "New(parenthesized)"] {
        let operation = Operation::Create {
            name: name.into(),
            copy_from: None,
            fields: Vec::new(),
        };
        assert!(styles::propose(&parsed.document, &parsed.source, &operation, || Ok(())).is_err());
    }
}

#[test]
fn style_limits_variant_fields_noop_and_cancellation_preserve_input() {
    let parsed = subtitle_formats::parse(ASS, None).unwrap();
    let noop = styles::propose(
        &parsed.document,
        &parsed.source,
        &edit("Title", &[("Fontsize", "36")]),
        || Ok(()),
    )
    .unwrap();
    assert!(!noop.changed);
    assert!(noop.summary.fields.is_empty());
    let mut checkpoints = 0;
    let result = styles::propose(
        &parsed.document,
        &parsed.source,
        &edit("Title", &[("Name", "New")]),
        || {
            checkpoints += 1;
            if checkpoints >= 4 {
                Err("cancelled".into())
            } else {
                Ok(())
            }
        },
    );
    assert!(result.is_err());
    assert_eq!(parsed.source.serialize(&parsed.document).unwrap(), ASS);
    let srt = subtitle_formats::parse("1\n00:00:01,000 --> 00:00:02,000\nplain\n\n", None).unwrap();
    assert!(matches!(srt.source, Source::Srt(_)));
    assert!(styles::propose(&srt.document, &srt.source, &edit("Title", &[]), || Ok(())).is_err());
    assert!(styles::supported_field(Variant::Ass, "OutlineColour"));
    assert!(!styles::supported_field(Variant::Ssa, "OutlineColour"));
    assert!(!styles::supported_field(Variant::Ass, "AlphaLevel"));
    let parsed = subtitle_formats::parse(SSA, None).unwrap();
    assert!(
        styles::propose(
            &parsed.document,
            &parsed.source,
            &edit("Title", &[("Alignment", "4")]),
            || Ok(())
        )
        .is_err()
    );
    assert!(
        styles::propose(
            &parsed.document,
            &parsed.source,
            &edit("Title", &[("Alignment", "10")]),
            || Ok(())
        )
        .is_ok()
    );
}
