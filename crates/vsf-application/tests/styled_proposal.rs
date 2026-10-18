use media_analysis::alignment::{Alignment, Segment};
use std::time::Duration;
use subtitle_core::Editor;
use subtitle_formats::{Format, parse};
use vsf_application::resync::{
    Budget, Cancellation, CueOutcome, Proposal, SubtitleSnapshot, UnresolvedReason,
};

const ASS: &str = include_str!("../../../tests/fixtures/subtitles/styles.ass");
const SSA: &str = include_str!("../../../tests/fixtures/subtitles/styles.ssa");
const FONT: &str = include_str!("../../../tests/fixtures/subtitles/embedded-font.ass");

fn build(
    source: &str,
    format: Format,
    source_end: u64,
    target_start: u64,
    target_end: u64,
) -> Proposal {
    let parsed = parse(source, Some(format)).unwrap();
    Proposal::build(
        SubtitleSnapshot::new(parsed.document, parsed.source).unwrap(),
        Alignment {
            segments: vec![Segment {
                source_start_us: 0,
                source_end_us: source_end,
                target_start_us: target_start,
                target_end_us: target_end,
                anchors: 3,
                max_residual_us: 0.,
            }],
            distinctive_matches: 3,
            ambiguous_source_anchors: 0,
            reordered: false,
        },
        &Budget::new(&Cancellation::default(), Duration::from_secs(5)),
    )
    .unwrap()
}

fn script(format: Format, text: &str, effect: &str) -> String {
    match format {
        Format::Srt => format!("1\n00:00:01,000 --> 00:00:02,000\n{text}\n"),
        Format::WebVtt => format!("WEBVTT\n\n00:00:01.000 --> 00:00:02.000\n{text}\n"),
        Format::Ass | Format::Ssa => format!(
            "[Script Info]\nScriptType: v4.00{}\n[{} Styles]\nFormat: Name, Fontname\nStyle: Default,Arial\n[Events]\nFormat: Start, End, Style, Effect, Text\nDialogue: 0:00:01.00,0:00:02.00,Default,{effect},{text}\n",
            if format == Format::Ass { "+" } else { "" },
            if format == Format::Ass { "V4+" } else { "V4" },
        ),
    }
}

#[test]
fn webvtt_offsets_preserve_literal_ass_and_refuse_changed_absolute_markers() {
    for text in ["<v Alice>Bonjour</v>", r"{\k20}Literal\N braces"] {
        let source = script(Format::WebVtt, text, "");
        let p = build(&source, Format::WebVtt, 10_000_000, 1_000, 10_001_000);
        assert!(p.is_complete());
        assert_eq!(
            p.subtitles().unwrap(),
            source
                .replace("01.000", "01.001")
                .replace("02.000", "02.001")
        );
    }
    let source = script(Format::WebVtt, "<v Alice>A<00:01.500>B</v>", "");
    let identity = build(&source, Format::WebVtt, 10_000_000, 0, 10_000_000);
    assert!(identity.is_complete());
    assert_eq!(identity.subtitles().unwrap(), source);
    for (start, end) in [(1_000, 10_001_000), (0, 11_000_000)] {
        let p = build(&source, Format::WebVtt, 10_000_000, start, end);
        assert!(!p.is_complete());
        assert!(p.subtitles().is_none());
        assert!(matches!(
            p.cues()[0].outcome,
            CueOutcome::Unresolved {
                reason: UnresolvedReason::TimedEffectsNeedReview
            }
        ));
    }
}

#[test]
fn styled_offsets_preserve_all_source_records_and_one_atomic_undo() {
    for (format, source) in [(Format::Ass, ASS), (Format::Ssa, SSA), (Format::Ass, FONT)] {
        let proposal = build(source, format, 30_000_000, 500_000, 30_500_000);
        assert!(proposal.is_complete());
        assert_eq!(proposal.snapshot().format(), format);
        let parsed = parse(source, Some(format)).unwrap();
        let mut editor = Editor::new(parsed.document.clone());
        editor
            .apply_edit(0, proposal.edit(editor.document()).unwrap(), |_| Ok(()))
            .unwrap();
        for (original, mapped) in parsed.document.cues().iter().zip(editor.document().cues()) {
            assert_eq!(mapped.start().get(), original.start().get() + 500);
            assert_eq!(mapped.end().get(), original.end().get() + 500);
            assert_eq!(mapped.id(), original.id());
            assert_eq!(mapped.text(), original.text());
            assert_eq!(mapped.metadata(), original.metadata());
        }
        let output = proposal.subtitles().unwrap();
        // Replacing only independently calculated timestamp spellings produces
        // exactly the output, including comments, styles, attachments and EOLs.
        let expected: String = source
            .split_inclusive('\n')
            .map(|line| {
                if line.starts_with("Dialogue:") {
                    line.replace("0:00:01.00", "0:00:01.50")
                        .replace("0:00:03.00", "0:00:03.50")
                        .replace("0:00:04.00", "0:00:04.50")
                } else {
                    line.to_owned()
                }
            })
            .collect();
        assert_eq!(output, expected);
        let reopened = parse(output, Some(format)).unwrap();
        assert_eq!(reopened.document, *editor.document());
        assert_eq!(
            reopened.source.serialize(&reopened.document).unwrap(),
            output
        );
        editor.undo(1).unwrap();
        assert_eq!(parsed.source.serialize(editor.document()).unwrap(), source);
        assert!(!editor.can_undo());
        editor.redo(2).unwrap();
        assert_eq!(parsed.source.serialize(editor.document()).unwrap(), output);
    }
}

#[test]
fn styled_mapping_rounds_directly_to_centiseconds_and_refuses_collapsed_ranges() {
    for format in [Format::Ass, Format::Ssa] {
        let source = script(format, r"{\an8\b1}Static", "");
        // 1004.9ms rounds to 1000ms directly; via whole milliseconds it would
        // incorrectly become 1010ms. The end rounds independently to 2010ms.
        let p = build(&source, format, 10_000_000, 0, 10_049_000);
        assert!(matches!(
            p.cues()[0].outcome,
            CueOutcome::Mapped {
                start_ms: 1000,
                end_ms: 2010,
                ..
            }
        ));
        let p = build(&source, format, 10_000_000, 0, 10_050_000);
        assert!(matches!(
            p.cues()[0].outcome,
            CueOutcome::Mapped {
                start_ms: 1010,
                end_ms: 2010,
                ..
            }
        ));
        let p = build(&source, format, 10_000_000, 0, 10_000);
        assert!(matches!(
            p.cues()[0].outcome,
            CueOutcome::Unresolved {
                reason: UnresolvedReason::InvalidMappedRange
            }
        ));
        assert!(p.subtitles().is_none());
    }
    let p = build(
        &script(Format::Srt, "Static", ""),
        Format::Srt,
        10_000_000,
        0,
        10_049_000,
    );
    assert!(matches!(
        p.cues()[0].outcome,
        CueOutcome::Mapped {
            start_ms: 1005,
            end_ms: 2010,
            ..
        }
    ));
}

#[test]
fn timed_or_ambiguous_tags_move_intact_but_cannot_silently_change_duration() {
    for format in [Format::Srt, Format::Ass, Format::Ssa] {
        for text in [
            r"{\k20}Hi",
            r"{\K20}Hi",
            r"{\kf20}Hi",
            r"{\ko20}Hi",
            r"{\kt20}Hi",
            r"{\move(0,0,100,100)}Hi",
            r"{\move(0,0,100,100,50,500)}Hi",
            r"{\t(\fs20)}Hi",
            r"{\t(2,\fs20)}Hi",
            r"{\t(10,500,\fs20)}Hi",
            r"{\t(10,500,2,\fs20)}Hi",
            r"{\fad(50,100)}Hi",
            r"{\fade(255,0,255,0,100,900,1000)}Hi",
            r"{\future(20)}Hi",
            r"{\move(0,0,100",
            r"{\p1}m 0 0 l 5 5",
        ] {
            let source = script(format, text, "");
            let moved = build(&source, format, 10_000_000, 500_000, 10_500_000);
            assert!(moved.is_complete(), "{format:?} {text}");
            assert!(moved.subtitles().unwrap().contains(text));
            let scaled = build(&source, format, 10_000_000, 0, 9_600_000);
            assert!(
                matches!(
                    scaled.cues()[0].outcome,
                    CueOutcome::Unresolved {
                        reason: UnresolvedReason::TimedEffectsNeedReview
                    }
                ),
                "{format:?} {text}"
            );
            assert!(scaled.subtitles().is_none());
            assert!(scaled.subtitles_sha256().is_none());
            assert!(scaled.edit(scaled.snapshot().document()).is_err());
        }
        let source = script(format, &"x".repeat(65_537), "");
        assert!(!build(&source, format, 10_000_000, 0, 9_600_000).is_complete());
    }
    for format in [Format::Ass, Format::Ssa] {
        let source = script(format, "Hi", "Banner;10;0;0");
        assert!(build(&source, format, 10_000_000, 500_000, 10_500_000).is_complete());
        assert!(!build(&source, format, 10_000_000, 0, 9_600_000).is_complete());
    }
}

#[test]
fn mixed_safe_and_timed_cues_refuse_whole_document_acceptance() {
    let source = "1\n00:00:01,000 --> 00:00:02,000\n{\\an8}Static\n\n2\n00:00:03,000 --> 00:00:04,000\n{\\k20}Timed\n";
    let p = build(source, Format::Srt, 10_000_000, 0, 9_600_000);
    assert!(matches!(p.cues()[0].outcome, CueOutcome::Mapped { .. }));
    assert!(matches!(
        p.cues()[1].outcome,
        CueOutcome::Unresolved {
            reason: UnresolvedReason::TimedEffectsNeedReview
        }
    ));
    assert!(p.subtitles().is_none());
    assert!(p.edit(p.snapshot().document()).is_err());
}
