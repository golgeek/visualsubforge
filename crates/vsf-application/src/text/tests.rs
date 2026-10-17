use super::*;
use subtitle_core::{CueId, Editor, Milliseconds};

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
