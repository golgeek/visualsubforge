use subtitle_core::{Cue, CueId, Document, Milliseconds};
use subtitle_formats::{Format, Source};
use vsf_application::{
    authoring::fonts,
    conversion::{
        self, ConversionFormat, ConversionOptions, ConversionPrecision, ConversionSeverity,
        Proposal,
    },
};

const ASS: &str = include_str!("../../../tests/fixtures/subtitles/styles.ass");
const SSA: &str = include_str!("../../../tests/fixtures/subtitles/styles.ssa");
const FONT: &[u8] = include_bytes!("../../../tests/fixtures/subtitles/fixture-font.ttf");
const SRT: &str = "007\n00:00:01,000 --> 00:00:02,000\n<i>世界</i>\nSecond line\n\n008\n00:00:02,010 --> 00:00:03,010\n{\\an8\\k10}Fansub\\Ntags\n\n";
fn propose(input: &str, target: ConversionFormat, precision: ConversionPrecision) -> Proposal {
    let parsed = subtitle_formats::parse(input, None).unwrap();
    let result = conversion::propose(
        &parsed.document,
        &parsed.source,
        ConversionOptions { target, precision },
        || Ok(()),
    )
    .unwrap();
    assert_eq!(parsed.source.serialize(&parsed.document).unwrap(), input);
    result
}
fn issue<'a>(proposal: &'a Proposal, code: &str) -> &'a conversion::ConversionIssue {
    proposal
        .report()
        .issues
        .iter()
        .find(|i| i.code == code)
        .unwrap_or_else(|| panic!("Missing {code}: {:?}", proposal.report()))
}
fn field<'a>(source: &'a Source, style: usize, field: &str) -> &'a str {
    source.ass().unwrap().styles()[style]
        .fields
        .iter()
        .find(|(n, _)| n == field)
        .unwrap()
        .1
        .trim()
}

#[test]
fn all_six_directions_have_parseable_separate_outputs_and_explicit_losses() {
    for (input, targets) in [
        (SRT, [ConversionFormat::Ass, ConversionFormat::Ssa]),
        (ASS, [ConversionFormat::Srt, ConversionFormat::Ssa]),
        (SSA, [ConversionFormat::Srt, ConversionFormat::Ass]),
    ] {
        let original = subtitle_formats::parse(input, None).unwrap();
        for target in targets {
            let proposal = propose(input, target, ConversionPrecision::Exact);
            assert!(!proposal.report().blocked(), "{:?}", proposal.report());
            let output = proposal.output().unwrap();
            assert_eq!(proposal.report().output_bytes, Some(output.len() as u32));
            let exported = subtitle_formats::parse(output, Some(target.format())).unwrap();
            assert_eq!(exported.document.cues().len(), 2);
            for (before, after) in original
                .document
                .cues()
                .iter()
                .zip(exported.document.cues())
            {
                assert_eq!((before.start(), before.end()), (after.start(), after.end()));
            }
            if original.source.format() == Format::Srt {
                assert_eq!(
                    exported.document.cues()[0].text(),
                    "{\\i1}世界{\\i0}\\NSecond line"
                );
                assert_eq!(
                    exported.document.cues()[1].text(),
                    original.document.cues()[1].text()
                );
                issue(&proposal, "default_style");
                issue(&proposal, "srt_overrides");
            } else {
                assert!(!output.contains("Opaque: exact bytes"));
                assert!(!output.contains("Comment:"));
                assert!(issue(&proposal, "other_records_removed").count >= 4);
                issue(&proposal, "event_fields_removed");
                if target == ConversionFormat::Srt {
                    assert_eq!(issue(&proposal, "styles_removed").count, 2);
                    assert_eq!(
                        exported.document.cues()[1].text(),
                        "{\\r}Second line\\N世界"
                    );
                    assert!(exported.document.cues()[0].metadata().is_none());
                } else {
                    assert_eq!(
                        field(&exported.source, 1, "Alignment"),
                        if target == ConversionFormat::Ass {
                            "8"
                        } else {
                            "6"
                        }
                    );
                    assert_eq!(
                        exported.document.cues()[0].metadata().unwrap().get("Style"),
                        Some("Title")
                    );
                    assert_eq!(exported.document.cues()[0].text(), "Red title");
                    assert!(output.contains("PlayResX: 640"));
                }
            }
        }
    }
}

#[test]
fn exact_precision_is_default_and_rounding_never_silently_damages_timed_effects() {
    let input = SRT.replace("01,000", "01,004").replace("02,000", "02,006");
    let exact = propose(&input, ConversionFormat::Ass, ConversionPrecision::Exact);
    assert!(exact.output().is_none());
    assert_eq!(issue(&exact, "precision_required").count, 1);
    let nearest = propose(&input, ConversionFormat::Ass, ConversionPrecision::Nearest);
    assert_eq!(issue(&nearest, "rounded").count, 1);
    assert!(nearest.output().unwrap().contains("0:00:01.00,0:00:02.01"));
    for tags in [
        r"{\k100}",
        r"{\t(0,900,\fs40)}",
        r"{\move(0,0,10,10)}",
        r"{\unknown1}",
        "<b>unclosed",
    ] {
        let input = format!("1\n00:00:01,004 --> 00:00:02,006\n{tags}text\n\n");
        let proposal = propose(&input, ConversionFormat::Ssa, ConversionPrecision::Nearest);
        assert!(proposal.output().is_none());
        issue(&proposal, "timed_effect");
        let shifted = propose(
            &input.replace("02,006", "02,004"),
            ConversionFormat::Ass,
            ConversionPrecision::Nearest,
        );
        assert!(shifted.output().is_some()); // same duration, tags retained
    }
    for times in [
        "00:00:01,001 --> 00:00:01,004",
        "9999:59:59,990 --> 9999:59:59,999",
    ] {
        let input = format!("1\n{times}\nx\n\n");
        let proposal = propose(&input, ConversionFormat::Ass, ConversionPrecision::Nearest);
        assert!(proposal.output().is_none());
        issue(&proposal, "invalid_rounding");
    }
    let tie = propose(
        "1\n00:00:01,005 --> 00:00:02,005\nx\n\n",
        ConversionFormat::Ass,
        ConversionPrecision::Nearest,
    );
    assert!(tie.output().unwrap().contains("0:00:01.01,0:00:02.01"));
}

#[test]
fn srt_retains_drawing_and_karaoke_syntax_protects_literal_html_and_reports_comments() {
    let input = ASS.replace(
        "Red title",
        r"{note\an8}<b>literal</b>{\p1}m 0 0 l 10 0 10 10{\p0}\N{\k10}世界",
    );
    let proposal = propose(&input, ConversionFormat::Srt, ConversionPrecision::Exact);
    let output = proposal.output().unwrap();
    assert!(output.contains(
        "{\\an8}<\u{2060}b>literal<\u{2060}/b>{\\p1}m 0 0 l 10 0 10 10{\\p0}\\N{\\k10}世界"
    ));
    assert!(!output.contains("note"));
    assert_eq!(issue(&proposal, "ass_comments").count, 1);
    issue(&proposal, "literal_angles");
    issue(&proposal, "fansub_srt");
    for bad in [r"{\incomplete", r"{\b1{\i1}nested}"] {
        let proposal = propose(
            &ASS.replace("Red title", bad),
            ConversionFormat::Srt,
            ConversionPrecision::Exact,
        );
        assert!(proposal.output().is_none());
        assert!(proposal.report().blocked());
    }
    let escaped = propose(
        &ASS.replace("Red title", r"\{literal\}"),
        ConversionFormat::Srt,
        ConversionPrecision::Exact,
    );
    assert!(escaped.output().unwrap().contains(r"{\r}\{literal\}"));
}

#[test]
fn style_mapping_preserves_names_positions_and_ssa_color_semantics_and_reports_loss() {
    let input = SSA
        .replace("255,255,0,0,-1", "255,255,19,197121,-1")
        .replace("25,0,1", "25,64,1");
    let proposal = propose(&input, ConversionFormat::Ass, ConversionPrecision::Exact);
    let parsed = subtitle_formats::parse(proposal.output().unwrap(), None).unwrap();
    for (name, value) in [
        ("PrimaryColour", "&H400000FF"),
        ("SecondaryColour", "&H400000FF"),
        ("OutlineColour", "&H40030201"),
        ("BackColour", "&H80030201"),
        ("Alignment", "8"),
    ] {
        assert_eq!(field(&parsed.source, 1, name), value);
    }
    assert_eq!(issue(&proposal, "style_fields_removed").count, 2); // both TertiaryColour fields
    let roundtrip = propose(
        proposal.output().unwrap(),
        ConversionFormat::Ssa,
        ConversionPrecision::Exact,
    );
    assert!(
        !roundtrip
            .report()
            .issues
            .iter()
            .any(|i| i.code == "ssa_color_loss")
    );
    let converted = subtitle_formats::parse(roundtrip.output().unwrap(), None).unwrap();
    assert_eq!(field(&converted.source, 1, "AlphaLevel"), "64");
    assert_eq!(field(&converted.source, 1, "BackColour"), "197121");
    let ass = propose(ASS, ConversionFormat::Ssa, ConversionPrecision::Exact);
    assert_eq!(issue(&ass, "ssa_color_loss").count, 2); // original shadow alpha 0 becomes 128
    for alignment in 1..=9 {
        let input = ASS.replace(",8,10,10,25,", &format!(",{alignment},10,10,25,"));
        let ssa = propose(&input, ConversionFormat::Ssa, ConversionPrecision::Exact);
        let ass = propose(
            ssa.output().unwrap(),
            ConversionFormat::Ass,
            ConversionPrecision::Exact,
        );
        let parsed = subtitle_formats::parse(ass.output().unwrap(), None).unwrap();
        assert_eq!(field(&parsed.source, 1, "Alignment"), alignment.to_string());
    }
    for bad in [
        ASS.replace("&H00FFFFFF", "bad-color"),
        ASS.replace(",8,10,10,25,", ",77,10,10,25,"),
    ] {
        let proposal = propose(&bad, ConversionFormat::Ssa, ConversionPrecision::Exact);
        assert!(proposal.output().is_none());
        issue(&proposal, "style_mapping");
    }
}

#[test]
fn attachments_are_preserved_between_scripts_or_reported_lost_in_srt() {
    for base in [ASS, SSA] {
        let input = format!("\u{feff}{base}[Graphics]\nfilename: drawing.bmp\nOPAQUE!\n[Fonts]\nfontname: Original.ttf\n{}\n", fonts::encode(FONT, || Ok(())).unwrap()).replace('\n', "\r\n");
        let target = if base == ASS {
            ConversionFormat::Ssa
        } else {
            ConversionFormat::Ass
        };
        let proposal = propose(&input, target, ConversionPrecision::Exact);
        let parsed = subtitle_formats::parse(proposal.output().unwrap(), None).unwrap();
        let script = parsed.source.ass().unwrap();
        let inventory = script.attachments(200);
        assert_eq!(inventory.total, 2);
        assert_eq!(issue(&proposal, "attachments_retained").count, 2);
        assert_eq!(
            fonts::decode(script, inventory.entries[1].id, || Ok(())).unwrap(),
            FONT
        );
        assert!(
            proposal
                .output()
                .unwrap()
                .contains("filename: drawing.bmp\nOPAQUE!")
        );
        let srt = propose(&input, ConversionFormat::Srt, ConversionPrecision::Exact);
        assert_eq!(issue(&srt, "attachments_removed").count, 2);
        assert!(!srt.output().unwrap().contains("fontname:"));
        let ambiguous = propose(
            &format!("{input}[Unknown]\r\nopaque\r\n"),
            target,
            ConversionPrecision::Exact,
        );
        assert!(ambiguous.output().is_none());
        issue(&ambiguous, "attachment_boundary");
    }
}

#[test]
fn cancellation_limits_and_report_samples_are_bounded_without_partial_outputs() {
    let parsed = subtitle_formats::parse(ASS, None).unwrap();
    let options = ConversionOptions {
        target: ConversionFormat::Ssa,
        precision: ConversionPrecision::Exact,
    };
    let mut calls = 0;
    conversion::propose(&parsed.document, &parsed.source, options, || {
        calls += 1;
        Ok(())
    })
    .unwrap();
    for stop in 1..=calls {
        let mut at = 0;
        let error = conversion::propose(&parsed.document, &parsed.source, options, || {
            at += 1;
            if at == stop {
                Err("cancel-once".into())
            } else {
                Ok(())
            }
        })
        .err()
        .unwrap();
        assert_eq!(error, "cancel-once");
    }
    assert!(
        conversion::propose(
            &parsed.document,
            &parsed.source,
            ConversionOptions {
                target: ConversionFormat::Ass,
                precision: ConversionPrecision::Exact
            },
            || Ok(())
        )
        .is_err()
    );
    let cue = |id, text| {
        Cue::new(
            CueId(id),
            Milliseconds::new(1001).unwrap(),
            Milliseconds::new(2001).unwrap(),
            text,
        )
        .unwrap()
    };
    let srt = subtitle_formats::parse("", Some(Format::Srt))
        .unwrap()
        .source;
    let document = Document::new((1..=10_000).map(|id| cue(id, "x".into())).collect()).unwrap();
    let proposal = conversion::propose(&document, &srt, options, || Ok(())).unwrap();
    let precision = issue(&proposal, "precision_required");
    assert_eq!(precision.count, 10_000);
    assert_eq!(precision.examples.len(), 8);
    assert_eq!(precision.examples[0].cue_id.as_deref(), Some("1"));
    assert_eq!(precision.severity, ConversionSeverity::Blocked);
    let oversized = Document::new(vec![cue(1, "x".repeat(65537))]).unwrap();
    let proposal = conversion::propose(&oversized, &srt, options, || Ok(())).unwrap();
    assert!(proposal.output().is_none());
    issue(&proposal, "cue_limit");
    let empty = Document::new(vec![]).unwrap();
    assert!(
        conversion::propose(&empty, &srt, options, || Ok(()))
            .unwrap()
            .output()
            .is_some()
    );
}

#[test]
fn export_reports_unsupported_html_and_refuses_oversize_output_or_partial_attachment_inventory() {
    let input = "1\n00:00:01,000 --> 00:00:02,000\n<font color=red>text</font>\n\n";
    let proposal = propose(input, ConversionFormat::Ass, ConversionPrecision::Exact);
    assert!(
        proposal
            .output()
            .unwrap()
            .contains("<font color=red>text</font>")
    );
    issue(&proposal, "literal_html");

    let mut many = ASS.to_owned();
    many.push_str("[Fonts]\n");
    for i in 0..201 {
        many.push_str(&format!("fontname: Font{i}.ttf\n!!!!\n"));
    }
    let proposal = propose(&many, ConversionFormat::Ssa, ConversionPrecision::Exact);
    assert!(proposal.output().is_none());
    issue(&proposal, "attachment_limit");
    let srt = propose(&many, ConversionFormat::Srt, ConversionPrecision::Exact);
    assert_eq!(issue(&srt, "attachments_removed").count, 201);
    assert!(srt.output().is_some());

    let source = subtitle_formats::parse("", Some(Format::Srt))
        .unwrap()
        .source;
    let document = Document::new(
        (1..=150)
            .map(|id| {
                Cue::new(
                    CueId(id),
                    Milliseconds::new(1000).unwrap(),
                    Milliseconds::new(2000).unwrap(),
                    "{".repeat(60_000),
                )
                .unwrap()
            })
            .collect(),
    )
    .unwrap();
    let proposal = conversion::propose(
        &document,
        &source,
        ConversionOptions {
            target: ConversionFormat::Ass,
            precision: ConversionPrecision::Exact,
        },
        || Ok(()),
    )
    .unwrap();
    assert!(proposal.output().is_none());
    issue(&proposal, "output_size");
}

#[test]
fn current_authored_source_overlays_are_used_without_mutating_history() {
    use subtitle_core::Editor;
    let parsed = subtitle_formats::parse(ASS, None).unwrap();
    let original = parsed.source.clone();
    let script = original.ass().unwrap();
    let modified = script
        .with_style(
            Some(1),
            script.styles()[1].with_field("Fontsize", "47").unwrap(),
        )
        .unwrap()
        .with_font(
            None,
            "Authored.ttf",
            &fonts::encode(FONT, || Ok(())).unwrap(),
        )
        .unwrap();
    let mut editor = Editor::with_properties(parsed.document.clone(), original.clone());
    editor
        .apply_document(
            0,
            parsed.document.clone(),
            Source::Ass(modified),
            vsf_application::subtitle_file::validate_document,
        )
        .unwrap();
    let before = editor.properties().serialize(editor.document()).unwrap();
    let proposal = conversion::propose(
        editor.document(),
        editor.properties(),
        ConversionOptions {
            target: ConversionFormat::Ssa,
            precision: ConversionPrecision::Exact,
        },
        || Ok(()),
    )
    .unwrap();
    let exported = subtitle_formats::parse(proposal.output().unwrap(), None).unwrap();
    assert_eq!(field(&exported.source, 1, "Fontsize"), "47");
    assert!(
        proposal
            .output()
            .unwrap()
            .contains("fontname: Authored.ttf")
    );
    assert_eq!(
        editor.properties().serialize(editor.document()).unwrap(),
        before
    );
    editor.undo(1).unwrap();
    assert_eq!(editor.properties(), &original);
    let cancelled = conversion::propose(
        editor.document(),
        editor.properties(),
        ConversionOptions {
            target: ConversionFormat::Ssa,
            precision: ConversionPrecision::Exact,
        },
        || Err("cancelled".into()),
    );
    assert!(cancelled.is_err());
    editor.redo(2).unwrap();
    assert_eq!(
        editor.properties().serialize(editor.document()).unwrap(),
        before
    );
}
