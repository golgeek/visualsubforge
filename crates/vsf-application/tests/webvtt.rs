use subtitle_core::{CueId, Edit, Editor, Milliseconds};
use subtitle_formats::{Format, Parsed};
use vsf_application::{batch_timing, quality, subtitle_file, subtitle_operations, text};

const FIXTURE: &str = include_str!("../../../tests/fixtures/subtitles/styles.vtt");
fn parsed(text: &str) -> Parsed {
    subtitle_file::parse_document(text.as_bytes(), Some("vtt")).unwrap()
}
fn one(text: &str) -> Parsed {
    parsed(&format!(
        "WEBVTT\n\n00:00:01.001 --> 00:00:04.999\n{text}\n"
    ))
}
fn proposal(p: &Parsed, operation: text::Operation) -> text::Proposal {
    text::propose_for_format(
        p.document.clone(),
        Format::WebVtt,
        &quality::Profile::default(),
        &operation,
        || Ok(()),
    )
    .unwrap()
}
fn replace(query: &str, replacement: &str) -> text::Operation {
    text::Operation::Replace {
        query: query.into(),
        replacement: replacement.into(),
        case_sensitive: true,
    }
}

#[test]
fn detection_bounded_input_and_same_format_save_never_convert_implicitly() {
    let p = parsed(FIXTURE);
    assert_eq!(p.source.format(), Format::WebVtt);
    assert_eq!(p.source.format().time_step_ms(), 1);
    assert_eq!(
        subtitle_file::parse_document(FIXTURE.as_bytes(), None).unwrap(),
        p
    );
    assert_eq!(p.source.serialize(&p.document).unwrap(), FIXTURE);
    assert!(subtitle_file::parse_document(FIXTURE.as_bytes(), Some("srt")).is_err());
    assert!(
        subtitle_file::parse_document(b"1\n00:00:01,000 --> 00:00:02,000\nx", Some("vtt")).is_err()
    );
    assert!(subtitle_file::parse_document(&[0xff], Some("vtt")).is_err());
    assert!(subtitle_file::parse_document(&vec![b'a'; 16 * 1024 * 1024 + 1], Some("vtt")).is_err());
    assert!(p.source.validate_save_extension(Some("VTT")).is_ok());
    for ext in [None, Some("srt"), Some("ass"), Some("ssa"), Some("txt")] {
        assert!(p.source.validate_save_extension(ext).is_err());
    }
    assert_eq!(
        p.source
            .prepare_text("{\\b1}literal\\N\nnext".into())
            .unwrap(),
        "{\\b1}literal\\N\nnext"
    );
    assert!(p.source.prepare_text("one\n\ntwo".into()).is_err());
    let error = vsf_application::conversion::propose(
        &p.document,
        &p.source,
        vsf_application::conversion::ConversionOptions {
            target: vsf_application::conversion::ConversionFormat::Srt,
            precision: vsf_application::conversion::ConversionPrecision::Exact,
        },
        || Ok(()),
    )
    .err()
    .unwrap();
    assert!(error.contains("WebVTT conversion"));
}

#[test]
fn decoded_text_counts_exclude_annotations_and_keep_literal_ass() {
    let source = "<v WrongVoice><lang fr><c.accent>Été &amp; 世界</c></lang></v> <ruby>字<rt>じ</ruby>\n{\\k20}\\N😀";
    let expected = ["Été & 世界 字", "{\\k20}\\N😀"];
    let m = quality::metrics::text_for_format(source, Format::WebVtt, || Ok(())).unwrap();
    let lengths: Vec<u32> = expected
        .iter()
        .map(|s| s.encode_utf16().count() as u32)
        .collect();
    assert_eq!(m.lines, lengths);
    assert_eq!(m.count, lengths.iter().sum::<u32>() + 2);
    let entities = "&eacute;&#233;&#x1F600;&NotEqualTilde;&nbsp;&lrm;&rlm;&#128;&#0;&#xD800;";
    let expected = "éé😀\u{2242}\u{338}\u{a0}\u{200e}\u{200f}€��";
    let m = quality::metrics::text_for_format(entities, Format::WebVtt, || Ok(())).unwrap();
    assert_eq!(m.count, expected.encode_utf16().count() as u32);
    let p = one(source);
    let report = quality::evaluate(
        &p.document,
        quality::Evaluation::from(&quality::Profile::default()).with_format(Format::WebVtt),
        |_, _| Ok(()),
    )
    .unwrap();
    assert_eq!(
        report.text_interpretation.as_deref(),
        Some("webvtt_utf16_v1")
    );
    assert_eq!(
        report.metrics[0].characters,
        lengths.iter().sum::<u32>() + 2
    );
}

#[test]
fn search_replace_and_undo_preserve_markup_identifiers_settings_and_blocks() {
    let p = parsed(FIXTURE);
    let plan = proposal(&p, replace("Été", "Hiver"));
    assert_eq!(plan.changed(), 1);
    let source = p.source.clone();
    let mut editor = Editor::with_properties(p.document, source);
    editor
        .apply_edit_with_properties(
            0,
            plan.edit(editor.document(), None).unwrap(),
            subtitle_file::validate_document,
        )
        .unwrap();
    let changed = editor.properties().serialize(editor.document()).unwrap();
    assert_eq!(changed, FIXTURE.replace("Été", "Hiver"));
    assert!(plan.edit(editor.document(), None).is_err());
    assert_eq!(parsed(&changed).document, *editor.document());
    editor.undo(editor.revision()).unwrap();
    assert_eq!(
        editor.properties().serialize(editor.document()).unwrap(),
        FIXTURE
    );
    editor.redo(editor.revision()).unwrap();
    assert_eq!(
        editor.properties().serialize(editor.document()).unwrap(),
        changed
    );
    for query in ["Alice", "fr", "accent", "じ", "00:02.500"] {
        assert_eq!(
            proposal(
                &parsed(FIXTURE),
                text::Operation::Find {
                    query: query.into(),
                    case_sensitive: true
                }
            )
            .matches(),
            0
        );
    }
    let p = one("&amp; &NotEqualTilde; word");
    for query in ["&", "\u{2242}"] {
        let r = proposal(&p, replace(query, "changed"));
        assert_eq!(r.changed(), 0);
        assert!(r.rows()[0].matches[0].skipped.is_some());
    }
    assert_eq!(proposal(&p, replace("word", "&amp;")).changed(), 0);
}

#[test]
fn formatting_and_reflow_respect_webvtt_context_and_unicode_offsets() {
    let source = "<v Alice>😀 hello {\\b1}\\N</v>";
    let start = source[..source.find("hello").unwrap()]
        .encode_utf16()
        .count() as u32;
    let formatted =
        text::format_webvtt_selection(source, start, start + 5, text::Format::Bold).unwrap();
    assert_eq!(formatted.text, source.replace("hello", "<b>hello</b>"));
    let restored = text::format_webvtt_selection(
        &formatted.text,
        formatted.start,
        formatted.end,
        text::Format::Bold,
    )
    .unwrap();
    assert_eq!(restored.text, source);
    assert!(text::format_webvtt_selection("😀", 1, 2, text::Format::Italic).is_err());
    assert!(text::format_webvtt_selection("&amp;", 1, 4, text::Format::Italic).is_err());
    let joined = proposal(&one("<v Alice>one\ntwo"), text::Operation::JoinLines);
    assert_eq!(joined.rows()[0].after.as_deref(), Some("<v Alice>one two"));
    let positioned = parsed("WEBVTT\n\n00:01.000 --> 00:03.000 line:10%,start\none\ntwo");
    assert_eq!(
        proposal(&positioned, text::Operation::JoinLines).changed(),
        0
    );
    for source in [
        "<unknown>one</unknown>",
        "<b>one",
        "<v>one</v>",
        "one &not-an-entity;",
    ] {
        let r = proposal(&one(source), replace("one", "two"));
        assert_eq!(r.changed(), 0);
        assert!(r.rows()[0].matches[0].skipped.is_some());
    }
}

#[test]
fn spelling_and_corrections_use_visible_words_and_original_utf16_spans() {
    let p = one("<v MispelledSpeaker><lang en>😀 <b>helo</b> world</lang></v>");
    let mut profile = quality::Profile::default()
        .custom("webvtt-language-test".into(), "WebVTT language test".into());
    profile.language_tools = vsf_application::language::Settings {
        dictionary: vsf_application::language::Language::EnglishUs,
        spelling: true,
        ..Default::default()
    };
    let report = quality::evaluate(
        &p.document,
        quality::Evaluation::from(&profile).with_format(Format::WebVtt),
        |_, _| Ok(()),
    )
    .unwrap();
    let findings: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.rule == quality::Rule::Spelling)
        .collect();
    assert_eq!(findings.len(), 1);
    let detail = findings[0].language.as_ref().unwrap();
    assert_eq!(detail.word, "helo");
    let source = p.document.cues()[0].text();
    assert_eq!(
        detail.start,
        source[..source.find("helo").unwrap()]
            .encode_utf16()
            .count() as u32
    );
    let correction = text::propose_for_format(
        p.document.clone(),
        Format::WebVtt,
        &profile,
        &text::Operation::LanguageCorrection {
            cue_index: 0,
            start: detail.start,
            end: detail.end,
            original: "helo".into(),
            replacement: "hello".into(),
        },
        || Ok(()),
    )
    .unwrap();
    assert_eq!(
        correction.rows()[0].after.as_deref(),
        Some(source.replace("helo", "hello").as_str())
    );
}

#[test]
fn shifts_preserve_milliseconds_and_refuse_absolute_markers_or_reordering() {
    let p = parsed(
        "WEBVTT\n\n00:01.001 --> 00:02.999\n{\\k20}literal\\N\n\n00:03.001 --> 00:04.999\nsecond",
    );
    let request = |scope, offset| batch_timing::Request {
        scope,
        change: batch_timing::Change::Shift(offset),
    };
    let batch = batch_timing::propose(
        p.document.clone(),
        Format::WebVtt,
        request(batch_timing::Scope::All, 1),
        || Ok(()),
    )
    .unwrap();
    let mut editor = Editor::with_properties(p.document.clone(), p.source.clone());
    editor
        .apply_edit_with_properties(
            0,
            batch.edit(editor.document()).unwrap(),
            subtitle_file::validate_document,
        )
        .unwrap();
    assert_eq!(editor.document().cues()[0].start().get(), 1002);
    assert_eq!(
        editor.document().cues()[0].text(),
        p.document.cues()[0].text()
    );
    assert!(
        batch_timing::propose(
            p.document.clone(),
            Format::WebVtt,
            request(batch_timing::Scope::Selected(vec![CueId(1)]), 3000),
            || Ok(())
        )
        .is_err()
    );
    let timed = one("one <00:02.000>two");
    assert!(
        batch_timing::propose(
            timed.document.clone(),
            Format::WebVtt,
            request(batch_timing::Scope::All, 1),
            || Ok(())
        )
        .is_err()
    );
    assert!(
        subtitle_operations::validate_split(&timed.document.cues()[0], Format::WebVtt).is_err()
    );
    assert!(
        subtitle_operations::validate_split(&parsed(FIXTURE).document.cues()[0], Format::WebVtt)
            .is_err()
    );
    assert!(subtitle_operations::validate_split(&p.document.cues()[0], Format::WebVtt).is_ok());
    assert!(
        subtitle_operations::validate_merge(
            &one("<v Alice>one").document.cues()[0],
            &one("<v Bob>two").document.cues()[0],
            Format::WebVtt
        )
        .is_err()
    );
    assert!(
        subtitle_operations::validate_merge(
            &one("<v Alice>one</v>").document.cues()[0],
            &one("<v Bob>two</v>").document.cues()[0],
            Format::WebVtt
        )
        .is_ok()
    );
    editor.undo(editor.revision()).unwrap();
    assert_eq!(editor.document(), &p.document);
    assert!(
        editor
            .apply_edit_with_properties(
                editor.revision(),
                Edit::Text {
                    id: CueId(1),
                    start: Milliseconds::new(4000).unwrap(),
                    end: Milliseconds::new(5000).unwrap(),
                    text: "x".into()
                },
                subtitle_file::validate_document
            )
            .is_err()
    );
    assert!(editor.can_redo());
}

#[test]
fn quality_timing_repairs_skip_absolute_markers_but_allow_literal_ass_text() {
    use quality::{duration, gap, scene};
    let mut profile =
        quality::Profile::default().custom("webvtt-repairs".into(), "WebVTT repairs".into());
    profile.rules.speed.enabled = false;
    profile.timing.scene_before_ms = 0;
    profile.timing.scene_after_ms = 0;
    for timed in [false, true] {
        let text = if timed {
            "one <00:02.000>two"
        } else {
            "{\\k20}literal\\N"
        };
        let p = parsed(&format!(
            "WEBVTT\n\n00:01.001 --> 00:09.999\n{text}\n\n00:10.001 --> 00:12.999\nsecond"
        ));
        let gaps = gap::propose_for_format(
            p.document.clone(),
            Format::WebVtt,
            &profile,
            &scene::Scenes::NoMedia,
            || Ok(()),
        )
        .unwrap();
        let duration = duration::propose_for_format(
            p.document,
            Format::WebVtt,
            &profile,
            &scene::Scenes::NoMedia,
            duration::Scope::Document,
            || Ok(()),
        )
        .unwrap();
        if timed {
            assert_eq!(gaps.rows()[0].skipped, Some(gap::Skip::MarkupNeedsReview));
            assert_eq!(
                duration.rows()[0].skipped,
                Some(duration::Skip::MarkupNeedsReview)
            );
            assert_eq!((gaps.changed(), duration.changed()), (0, 0));
        } else {
            assert_eq!((gaps.changed(), duration.changed()), (1, 1));
        }
        let mut scene_profile = profile.clone();
        scene_profile.timing.scene_before_ms = 120;
        scene_profile.timing.scene_after_ms = 120;
        let scenes = scene::propose_for_format(
            one(text).document,
            Format::WebVtt,
            &scene_profile,
            &scene::Scenes::Complete(vec![Milliseconds::new(5000).unwrap()]),
            || Ok(()),
        )
        .unwrap();
        if timed {
            assert_eq!(
                scenes.rows()[0].skipped,
                Some(scene::Skip::MarkupNeedsReview)
            );
            assert_eq!(scenes.changed(), 0);
        } else {
            assert_eq!(scenes.changed(), 1);
        }
    }
}

#[test]
fn large_cue_metrics_cancel_and_deep_markup_requires_manual_review() {
    let source = format!("<unknown {}>tail", "x".repeat(100_000));
    let mut calls = 0;
    let error = quality::metrics::text_for_format(&source, Format::WebVtt, || {
        calls += 1;
        if calls == 3 {
            Err(quality::Error::Limit("cancelled by test".into()))
        } else {
            Ok(())
        }
    })
    .unwrap_err();
    assert!(error.to_string().contains("cancelled by test"));
    assert_eq!(calls, 3);
    let deep = format!("{}word{}", "<b>".repeat(65), "</b>".repeat(65));
    assert_eq!(
        proposal(&one(&deep), replace("word", "changed")).changed(),
        0
    );
    let m =
        quality::metrics::text_for_format(&"😀".repeat(40_000), Format::WebVtt, || Ok(())).unwrap();
    assert_eq!(m.count, 80_000);
}
