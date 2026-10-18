use super::*;
use subtitle_core::{CueId, Editor, Milliseconds};

#[test]
fn ass_search_protects_escapes_and_comments_but_treats_html_as_visible_text() {
    let doc = document(&[r"First\N世界{translator note}<i>literal</i>"]);
    let format = subtitle_formats::Format::Ass;
    for query in ["translator", "N"] {
        let p = propose_for_format(
            doc.clone(),
            format,
            &profile(),
            &replace(query, "other"),
            || Ok(()),
        )
        .unwrap();
        assert_eq!(p.matches(), 0);
    }
    let p = propose_for_format(
        doc.clone(),
        format,
        &profile(),
        &replace("literal", "visible"),
        || Ok(()),
    )
    .unwrap();
    assert_eq!(
        p.rows()[0].after.as_deref(),
        Some(r"First\N世界{translator note}<i>visible</i>")
    );
    let p = propose_for_format(
        doc.clone(),
        format,
        &profile(),
        &replace("First\n世界", "crossed"),
        || Ok(()),
    )
    .unwrap();
    assert!(p.rows()[0].matches[0].skipped.is_some());
    assert_eq!(p.changed(), 0);
    assert!(
        propose_for_format(
            doc.clone(),
            format,
            &profile(),
            &replace("First", "first\nsecond"),
            || Ok(())
        )
        .is_err()
    );
    let srt = propose(
        document(&[r"First\Nlast"]),
        &profile(),
        &replace("N", "n"),
        || Ok(()),
    )
    .unwrap();
    assert_eq!(srt.matches(), 1, "Plain SRT keeps literal escape policy");
}

#[test]
fn ass_line_repairs_preserve_overrides_metadata_and_undo() {
    let parsed = subtitle_formats::parse(
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/subtitles/styles.ass"
        )),
        Some(subtitle_formats::Format::Ass),
    )
    .unwrap();
    let source = parsed.source;
    let mut editor = Editor::new(parsed.document);
    editor
        .apply_edit(
            0,
            Edit::TextBatch {
                texts: vec![
                    CueText {
                        id: CueId(1),
                        text: r"{\i1}One two three four five six{\i0}".into(),
                    },
                    CueText {
                        id: CueId(2),
                        text: r"First\Nsecond".into(),
                    },
                ],
            },
            |document| crate::subtitle_file::validate_document(document, &source),
        )
        .unwrap();
    let before = source.serialize(editor.document()).unwrap();
    let metadata = editor.document().cues()[0].metadata().cloned();
    let mut rules = profile();
    rules.rules.lines.maximum_characters = 18;
    rules.rules.lines.single_line_preferred = 16;
    let p = propose_for_format(
        editor.document().clone(),
        source.format(),
        &rules,
        &Operation::Reflow,
        || Ok(()),
    )
    .unwrap();
    assert_eq!(
        p.rows()[0].after.as_deref(),
        Some(r"{\i1}One two three\Nfour five six{\i0}")
    );
    assert_eq!(p.rows()[1].after.as_deref(), Some("First second"));
    let edit = p.edit(editor.document(), None).unwrap();
    editor
        .apply_edit(1, edit, |document| {
            crate::subtitle_file::validate_document(document, &source)
        })
        .unwrap();
    assert_eq!(editor.document().cues()[0].metadata(), metadata.as_ref());
    assert!(
        source
            .serialize(editor.document())
            .unwrap()
            .contains("Comment:")
    );
    editor.undo(2).unwrap();
    assert_eq!(source.serialize(editor.document()).unwrap(), before);
    let p = propose_for_format(
        editor.document().clone(),
        source.format(),
        &rules,
        &Operation::JoinLines,
        || Ok(()),
    )
    .unwrap();
    assert_eq!(p.rows()[0].cue_index, 1);
    assert_eq!(p.rows()[0].after.as_deref(), Some("First second"));
}

#[test]
fn ass_formatting_restores_inherited_styles_and_toggles_only_its_exact_span() {
    let text = "😀 A<B>";
    let formatted = format_ass_selection(text, 3, 4, Format::Italic, false).unwrap();
    assert_eq!(formatted.text, "😀 {\\i1}A{\\i0}<B>");
    assert_eq!(
        format_ass_selection(
            &formatted.text,
            formatted.start,
            formatted.end,
            Format::Italic,
            false
        )
        .unwrap()
        .text,
        text
    );
    assert_eq!(
        format_ass_selection("Bold tail", 0, 4, Format::Bold, true)
            .unwrap()
            .text,
        "{\\b0}Bold{\\b1} tail"
    );
    for text in [r"{\rTitle}Hello", r"{\b700}Hello", r"{\k40}Hello"] {
        let start = text.find("Hello").unwrap() as u32;
        assert!(format_ass_selection(text, start, start + 5, Format::Bold, false).is_err());
    }
    assert!(format_ass_selection(r"A\N B", 1, 3, Format::Bold, false).is_err());
    assert!(format_ass_selection("😀", 1, 2, Format::Bold, false).is_err());
}

fn document(texts: &[&str]) -> Document {
    Document::new(
        texts
            .iter()
            .enumerate()
            .map(|(i, text)| {
                Cue::new(
                    CueId(i as u64 + 1),
                    Milliseconds::new(i as u64 * 2000).unwrap(),
                    Milliseconds::new(i as u64 * 2000 + 1500).unwrap(),
                    (*text).into(),
                )
                .unwrap()
            })
            .collect(),
    )
    .unwrap()
}
fn profile() -> Profile {
    Profile::builtins()
        .remove(0)
        .custom("test-text".into(), "Text test".into())
}
fn replace(query: &str, replacement: &str) -> Operation {
    Operation::Replace {
        query: query.into(),
        replacement: replacement.into(),
        case_sensitive: false,
    }
}
fn preview(texts: &[&str], op: Operation) -> Proposal {
    propose(document(texts), &profile(), &op, || Ok(())).unwrap()
}

#[test]
fn unicode_literal_search_preserves_original_tags_times_and_history() {
    let original = document(&[
        r"{\an8\c&HFF0000&}<i>ÉTÉ 😀 été</i>",
        "日本語 Σςσ",
        r"C:\new\here",
    ]);
    let proposal = propose(
        original.clone(),
        &profile(),
        &replace("été", "été!"),
        || Ok(()),
    )
    .unwrap();
    assert_eq!(proposal.matches(), 2);
    let row = &proposal.rows()[0];
    assert_eq!(
        row.after.as_deref(),
        Some(r"{\an8\c&HFF0000&}<i>été! 😀 été!</i>")
    );
    let mut e = Editor::new(original.clone());
    e.apply_edit(0, proposal.edit(e.document(), None).unwrap(), |_| Ok(()))
        .unwrap();
    assert_eq!(e.document().cues()[1], original.cues()[1]);
    assert_eq!(e.document().cues()[0].start(), original.cues()[0].start());
    e.undo(1).unwrap();
    assert_eq!(e.document(), &original);
    e.redo(2).unwrap();
    assert!(proposal.edit(e.document(), None).is_err());
    let greek = preview(&["Σςσ"], replace("σ", "x"));
    assert_eq!(greek.rows()[0].after.as_deref(), Some("xxx"));
    // Unicode simple folding is intentionally not normalization or ß -> ss.
    assert_eq!(
        preview(&["Straße e\u{301}"], replace("ss", "X")).matches(),
        0
    );
    let literal = preview(&["a.* a[0]"], replace(".*", "$1"));
    assert_eq!(literal.rows()[0].after.as_deref(), Some("a$1 a[0]"));
}

#[test]
fn matches_across_tags_escapes_drawings_and_malformed_markup_are_visible_but_skipped() {
    let p = preview(
        &[
            "<b>he</b>llo",
            r"{\r}he\Nllo",
            r"{\k10}hello",
            r"{\p1}hello",
            "<i>hello",
            r"{\future9}hello",
        ],
        replace("hello", "hi"),
    );
    assert_eq!(p.matches(), 5);
    assert_eq!(p.changed(), 0);
    assert!(p.rows().iter().all(|r| r.matches[0].skipped.is_some()));
    let p = preview(&[r"{\r}he\Nllo"], replace("he\nllo", "hi"));
    assert_eq!(p.matches(), 1);
    assert_eq!(p.changed(), 0);
    let p = preview(&[r"{X\b1}hello"], replace("X", ""));
    assert_eq!(p.changed(), 0);
    assert!(p.rows()[0].matches[0].skipped.unwrap().contains("markup"));
    let p = preview(&["<b>hello</b> hello"], replace("hello", "hi"));
    assert_eq!(p.rows()[0].after.as_deref(), Some("<b>hi</b> hi"));
}

#[test]
fn search_offsets_are_source_utf16_and_plain_srt_backslashes_stay_literal() {
    let p = preview(
        &["<i>😀 日本</i>", r"C:\new\here", r"{\r}A\hB"],
        Operation::Find {
            query: "日本".into(),
            case_sensitive: true,
        },
    );
    assert_eq!(
        (p.rows()[0].matches[0].start, p.rows()[0].matches[0].end),
        (6, 8)
    );
    assert_eq!(p.changed(), 0);
    assert_eq!(
        preview(&[r"C:\new\here"], replace(r"\new", "new")).rows()[0]
            .after
            .as_deref(),
        Some(r"C:new\here")
    );
    assert_eq!(
        preview(&[r"{\r}A\hB"], replace("A\u{a0}B", "X")).changed(),
        0
    );
}

#[test]
fn reflow_changes_only_whitespace_preserving_tag_bytes_and_profile_limits() {
    let mut profile = profile();
    profile.rules.lines.maximum_characters = 18;
    profile.rules.lines.single_line_preferred = 16;
    let doc = document(&[
        "<i>One two three four five six</i>",
        "First\nsecond",
        r"{\b1}One two three four five six",
        "- One\n- Two",
        r"{\pos(2,3)}First\nsecond",
        r"{\r}First\Nsecond",
    ]);
    let p = propose(doc, &profile, &Operation::Reflow, || Ok(())).unwrap();
    assert_eq!(
        p.rows()[0].after.as_deref(),
        Some("<i>One two three\nfour five six</i>")
    );
    assert_eq!(p.rows()[1].after.as_deref(), Some("First second"));
    assert_eq!(
        p.rows()[2].after.as_deref(),
        Some("{\\b1}One two three\nfour five six")
    );
    assert!(
        p.rows()[3..]
            .iter()
            .all(|r| r.after.is_none() && r.skipped.is_some())
    );
    let p = preview(&["<b>First</b>\n<i>second</i>"], Operation::JoinLines);
    assert_eq!(
        p.rows()[0].after.as_deref(),
        Some("<b>First</b> <i>second</i>")
    );
    let p = preview(&["<b>First\n</b> second"], Operation::JoinLines);
    assert!(p.rows()[0].skipped.unwrap().contains("formatting"));
}

#[test]
fn reflow_does_not_split_words_nonbreaking_spaces_unicode_or_dialogue() {
    let mut profile = profile();
    profile.rules.lines.maximum_characters = 8;
    profile.rules.lines.single_line_preferred = 8;
    let p = propose(
        document(&[
            "é😀 ab cd",
            "long\u{a0}word x",
            "a\n\nb",
            "\tIndented",
            "— Dialogue",
            " leading text",
        ]),
        &profile,
        &Operation::Reflow,
        || Ok(()),
    )
    .unwrap();
    assert_eq!(p.rows()[0].after.as_deref(), Some("é😀\nab cd"));
    assert!(
        p.rows()[1..]
            .iter()
            .all(|r| r.after.is_none() && r.skipped.is_some())
    );
}

#[test]
fn formatting_toggles_plain_selected_source_and_refuses_ambiguous_boundaries() {
    let v = format_selection("😀 日本語", 3, 6, Format::Italic).unwrap();
    assert_eq!(v.text, "😀 <i>日本語</i>");
    assert_eq!((v.start, v.end), (6, 9));
    let back = format_selection(&v.text, v.start, v.end, Format::Italic).unwrap();
    assert_eq!(back.text, "😀 日本語");
    assert_eq!((back.start, back.end), (3, 6));
    assert!(format_selection("😀 日本語", 1, 4, Format::Bold).is_err());
    assert!(format_selection("<i>Hello</i>", 1, 7, Format::Bold).is_err());
    assert!(format_selection(r"{\b1}Hello", 5, 10, Format::Bold).is_err());
    assert!(format_selection(r"{\k5}Hello", 5, 10, Format::Italic).is_err());
    assert_eq!(
        format_selection(r"{\an8}Hello", 6, 11, Format::Bold)
            .unwrap()
            .text,
        r"{\an8}<b>Hello</b>"
    );
}

#[test]
fn cancelled_oversized_noop_and_expanding_requests_do_not_publish_partial_proposals() {
    let mut calls = 0;
    let result = propose(
        document(&["hello", "hello"]),
        &profile(),
        &replace("hello", "hi"),
        || {
            calls += 1;
            if calls > 4 {
                Err(Error::Cancelled)
            } else {
                Ok(())
            }
        },
    );
    assert!(matches!(result, Err(Error::Cancelled)));
    let large = "a".repeat(MAX_CUE_BYTES + 1);
    let p = preview(&[&large, "hello"], replace("hello", "hi"));
    assert!(p.rows()[0].skipped.is_some());
    assert_eq!(p.changed(), 1);
    assert!(
        preview(&["same"], replace("same", "same"))
            .edit(&document(&["same"]), None)
            .is_err()
    );
    for op in [
        replace("", "a"),
        replace("a", "<i>"),
        replace("a", "{\\b1}"),
        replace("a", "\r"),
    ] {
        assert!(propose(document(&["a"]), &profile(), &op, || Ok(())).is_err());
    }
    let input = "a".repeat(5000);
    assert!(
        propose(
            document(&[&input]),
            &profile(),
            &replace("a", &"b".repeat(MAX_INPUT_BYTES)),
            || Ok(())
        )
        .is_err()
    );
}

#[test]
fn malformed_markup_remains_searchable_and_long_words_are_explained() {
    assert!(format_selection(r"{<b>\b1</b>}", 4, 7, Format::Bold).is_err());
    let p = preview(
        &["<b>hello"],
        Operation::Find {
            query: "<b>".into(),
            case_sensitive: true,
        },
    );
    assert_eq!(p.matches(), 1);
    assert!(p.rows()[0].matches[0].skipped.is_some());
    let p = preview(&[&"x".repeat(100)], Operation::Reflow);
    assert!(p.rows()[0].skipped.unwrap().contains("word boundary"));
    let p = preview(
        &[&"<".repeat(MAX_CUE_BYTES)],
        Operation::Find {
            query: "<".into(),
            case_sensitive: true,
        },
    );
    assert_eq!(p.matches(), MAX_CUE_BYTES);
}
