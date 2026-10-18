use subtitle_core::{Cue, CueId, Document, Milliseconds};
use subtitle_formats::Format;
use vsf_application::{
    language::{Language, Lexicon, Settings, WordLists},
    quality::{self, Evaluation, Profile, Rule},
};

fn profile(language: Language) -> Profile {
    let mut p = Profile::default().custom("language-test".into(), "Language test".into());
    p.language_tools = Settings {
        dictionary: language,
        spelling: true,
        ..Settings::default()
    };
    p.timing.scene_before_ms = 0;
    p.timing.scene_after_ms = 0;
    p
}
fn document(texts: &[&str]) -> Document {
    Document::new(
        texts
            .iter()
            .enumerate()
            .map(|(i, text)| {
                Cue::new(
                    CueId(i as u64 + 1),
                    Milliseconds::new(i as u64 * 5000).unwrap(),
                    Milliseconds::new(i as u64 * 5000 + 4000).unwrap(),
                    (*text).into(),
                )
                .unwrap()
            })
            .collect(),
    )
    .unwrap()
}
fn check(
    texts: &[&str],
    p: &Profile,
    format: Format,
    lexicon: Option<&Lexicon>,
) -> quality::Report {
    quality::evaluate(
        &document(texts),
        Evaluation {
            profile: p,
            format,
            lexicon,
        },
        |_, _| Ok(()),
    )
    .unwrap()
}

#[test]
fn spelling_keeps_original_unicode_offsets_and_markup_boundaries() {
    let p = profile(Language::EnglishUs);
    let r = check(
        &["😀 <i>helo</i> world", "he<i>lo</i>", r"{\an8}helo\Nworld"],
        &p,
        Format::Srt,
        None,
    );
    let findings: Vec<_> = r
        .findings
        .iter()
        .filter(|f| f.rule == Rule::Spelling)
        .collect();
    assert_eq!(findings.len(), 3);
    let first = findings[0].language.as_ref().unwrap();
    assert_eq!(
        (first.start, first.end, first.word.as_str()),
        (6, 10, "helo")
    );
    assert!(first.manual_reason.is_none());
    assert!(
        findings[1]
            .language
            .as_ref()
            .unwrap()
            .manual_reason
            .is_some()
    );
    assert!(
        findings[2]
            .language
            .as_ref()
            .unwrap()
            .manual_reason
            .is_none()
    );
    assert_eq!(r.language_coverage.checked_words, 5);
    assert!(r.complete);
    assert_eq!(r.schema_version, 3);
    let french = check(
        &[
            "Les e\u{301}le\u{300}ves aiment l’amour et le cœur.",
            "bonjor",
        ],
        &profile(Language::French),
        Format::Srt,
        None,
    );
    let bad: Vec<_> = french
        .findings
        .iter()
        .filter(|f| f.rule == Rule::Spelling)
        .map(|f| f.language.as_ref().unwrap().word.as_str())
        .collect();
    assert_eq!(bad, ["bonjor"]);
}

#[test]
fn unsupported_text_has_explicit_incomplete_coverage_and_never_spells_drawing_commands() {
    for format in [Format::Srt, Format::Ass, Format::Ssa] {
        let huge = "x".repeat(65537);
        let r = check(
            &[
                r"{\p1}m 0 0 l 5 5",
                r"{\k20}helo",
                r"{\unknown}helo",
                &huge,
                "https://example.invalid/helo mail@invalid.test abcdefghijklmnopqrstuvwxyzabcdefg",
            ],
            &profile(Language::EnglishUs),
            format,
            None,
        );
        assert_eq!(r.language_coverage.skipped_cues, 4);
        assert!(r.language_coverage.skipped_words > 2);
        assert!(!r.complete);
        assert!(!r.findings.iter().any(|f| f.rule == Rule::Spelling));
        assert_eq!(
            r.findings
                .iter()
                .filter(|f| f.rule == Rule::LanguageCoverage)
                .count(),
            5
        );
    }
}

#[test]
fn per_language_personal_and_session_words_are_explicit_evaluation_inputs() {
    let words = WordLists {
        english_us: vec!["Shinjix".into(), "Fan-name".into()],
        ..WordLists::default()
    };
    let lexicon = Lexicon::new(&words, &WordLists::default()).unwrap();
    let p = profile(Language::EnglishUs);
    let r = check(&["Shinjix fan-name"], &p, Format::Srt, Some(&lexicon));
    assert!(r.language_coverage.personalized);
    assert_eq!(r.language_coverage.checked_words, 2);
    assert!(!r.findings.iter().any(|f| f.rule == Rule::Spelling));
    assert_eq!(
        check(&["Shinjix"], &p, Format::Srt, None)
            .findings
            .iter()
            .filter(|f| f.rule == Rule::Spelling)
            .count(),
        1
    );
    assert_eq!(
        check(
            &["Shinjix"],
            &profile(Language::French),
            Format::Srt,
            Some(&lexicon)
        )
        .findings
        .iter()
        .filter(|f| f.rule == Rule::Spelling)
        .count(),
        1
    );
}

#[test]
fn editorial_rules_are_conservative_optional_and_source_mapped() {
    let mut p = profile(Language::French);
    p.language_tools.spelling = false;
    p.language_tools.typography = true;
    p.language_tools.capitalization = true;
    p.language_tools.dialogue_markers = true;
    let r = check(
        &[
            "<i>bonjour</i>... l'amour !",
            "encore",
            "—salut? oui",
            "-42",
            "https://example.com/a...b?q=hello",
        ],
        &p,
        Format::Srt,
        None,
    );
    let fixes: Vec<_> = r
        .findings
        .iter()
        .filter_map(|f| {
            f.language.as_ref().map(|d| {
                (
                    f.cue_index,
                    f.rule,
                    d.word.as_str(),
                    d.replacement.as_deref(),
                )
            })
        })
        .collect();
    for expected in [
        (0, Rule::Capitalization, "b", Some("B")),
        (0, Rule::Typography, "...", Some("…")),
        (0, Rule::Typography, "'", Some("’")),
        (0, Rule::Typography, " ", Some("\u{a0}")),
        (1, Rule::Capitalization, "e", Some("E")),
        (2, Rule::DialogueMarker, "—", Some("— ")),
        (2, Rule::Capitalization, "o", Some("O")),
    ] {
        assert!(fixes.contains(&expected), "missing {expected:?}: {fixes:?}");
    }
    assert!(!fixes.iter().any(|f| f.0 >= 3));
    assert!(
        !fixes
            .iter()
            .any(|f| f.0 == 2 && f.1 == Rule::Capitalization && f.2 == "s")
    );
    // Every cue is not treated as a new sentence, and periods are deliberately
    // ambiguous (abbreviations/continuations) in this initial conservative rule.
    let r = check(
        &["Hello.", "continuation", "question?", "answer"],
        &p,
        Format::Srt,
        None,
    );
    assert_eq!(
        r.findings
            .iter()
            .filter(|f| f.rule == Rule::Capitalization)
            .map(|f| f.cue_index)
            .collect::<Vec<_>>(),
        [3]
    );
}

#[test]
fn old_profiles_remain_disabled_and_unchanged_on_serialization() {
    let original = Profile::default();
    let json = serde_json::to_value(&original).unwrap();
    assert!(json.get("languageTools").is_none());
    let parsed = quality::parse_profile(&serde_json::to_vec(&json).unwrap()).unwrap();
    assert_eq!(parsed, original);
    assert!(!parsed.language_tools.enabled());
    assert_eq!(
        parsed.fingerprint().unwrap(),
        original.fingerprint().unwrap()
    );
    assert!(
        !check(&["helo"], &parsed, Format::Srt, None)
            .language_coverage
            .enabled
    );
    let p = profile(Language::French);
    assert_eq!(
        quality::parse_profile(&serde_json::to_vec(&p).unwrap()).unwrap(),
        p
    );
    let mut invalid = original;
    invalid.language_tools.spelling = true;
    assert!(invalid.validate().is_err(), "builtins remain immutable");
}

#[test]
fn url_punctuation_cannot_capitalize_following_cues() {
    let mut p = profile(Language::EnglishUs);
    p.language_tools.spelling = false;
    p.language_tools.capitalization = true;
    let r = check(
        &["https://example.test/?", "continuation"],
        &p,
        Format::Srt,
        None,
    );
    assert!(!r.findings.iter().any(|f| f.rule == Rule::Capitalization));
}

#[test]
fn cancellation_is_checked_during_language_work_and_dense_findings_are_bounded() {
    let p = profile(Language::EnglishUs);
    let d = document(&[&"helo ".repeat(10_000)]);
    let mut calls = 0;
    let result = quality::evaluate(&d, &p, |_, _| {
        calls += 1;
        if calls > 20 {
            Err(quality::Error::Cancelled)
        } else {
            Ok(())
        }
    });
    assert_eq!(result.unwrap_err(), quality::Error::Cancelled);
    let d = document(
        &vec!["helo ".repeat(1000); 101]
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
    );
    assert!(matches!(
        quality::evaluate(&d, &p, |_, _| Ok(())),
        Err(quality::Error::Limit(_))
    ));
}

#[test]
fn suggestions_and_exact_corrections_preserve_format_source_and_one_undo() {
    use subtitle_core::{CueText, Edit, Editor};
    use vsf_application::{
        subtitle_file,
        text::{self, Operation},
    };
    for (extension, bytes) in [
        (
            "srt",
            "1\n00:00:01,001 --> 00:00:03,999\n{\\an8}<i>😀 helo helo</i>\n\n",
        ),
        (
            "ass",
            include_str!("../../../tests/fixtures/subtitles/styles.ass"),
        ),
        (
            "ssa",
            include_str!("../../../tests/fixtures/subtitles/styles.ssa"),
        ),
    ] {
        let parsed = subtitle_file::parse_document(bytes.as_bytes(), Some(extension)).unwrap();
        let source = parsed.source;
        let format = source.format();
        let mut editor = Editor::new(parsed.document);
        if format != Format::Srt {
            let texts = editor
                .document()
                .cues()
                .iter()
                .map(|c| CueText {
                    id: c.id(),
                    text: r"{\i1}😀 helo helo{\i0}".into(),
                })
                .collect();
            editor
                .apply_edit(0, Edit::TextBatch { texts }, |d| {
                    source.serialize(d).map(|_| ())
                })
                .unwrap();
        }
        let baseline = editor.document().clone();
        let original = source.serialize(&baseline).unwrap();
        let p = profile(Language::EnglishUs);
        let context = Evaluation::from(&p).with_format(format);
        let report = quality::evaluate(&baseline, context, |_, _| Ok(())).unwrap();
        let finding = report
            .findings
            .iter()
            .find(|f| f.rule == Rule::Spelling)
            .unwrap();
        let d = finding.language.as_ref().unwrap();
        let suggest = Operation::LanguageSuggestions {
            cue_index: finding.cue_index,
            start: d.start,
            end: d.end,
            original: d.word.clone(),
        };
        let suggestions =
            text::propose_for_format(baseline.clone(), format, context, &suggest, || Ok(()))
                .unwrap();
        assert!(suggestions.suggestions().iter().any(|s| s == "hello"));
        assert_eq!(suggestions.changed(), 0);
        assert!(suggestions.edit(&baseline, None).is_err());
        let correction = |start, replacement: &str| Operation::LanguageCorrection {
            cue_index: finding.cue_index,
            start,
            end: d.end,
            original: d.word.clone(),
            replacement: replacement.into(),
        };
        for invalid in [
            correction(d.start + 1, "hello"),
            correction(d.start, "{\\b1}"),
            correction(d.start, "\nhello"),
            correction(d.start, ""),
        ] {
            assert!(
                text::propose_for_format(baseline.clone(), format, context, &invalid, || Ok(()))
                    .is_err()
            );
        }
        let plan = text::propose_for_format(
            baseline.clone(),
            format,
            context,
            &correction(d.start, "hello"),
            || Ok(()),
        )
        .unwrap();
        assert_eq!(plan.changed(), 1);
        assert_eq!(plan.before_findings(), report.findings.len());
        let edit = plan.edit(&baseline, Some((0, 0))).unwrap();
        let revision = editor.revision();
        editor
            .apply_edit(revision, edit, |d| source.serialize(d).map(|_| ()))
            .unwrap();
        let after_report = quality::evaluate(editor.document(), context, |_, _| Ok(())).unwrap();
        assert_eq!(plan.after_findings(), after_report.findings.len());
        assert_eq!(
            report
                .findings
                .iter()
                .filter(|f| f.rule == Rule::Spelling)
                .count()
                - after_report
                    .findings
                    .iter()
                    .filter(|f| f.rule == Rule::Spelling)
                    .count(),
            1
        );
        assert_eq!(
            source.serialize(editor.document()).unwrap(),
            original.replacen("helo", "hello", 1)
        );
        assert!(plan.edit(editor.document(), None).is_err());
        editor.undo(revision + 1).unwrap();
        assert_eq!(source.serialize(editor.document()).unwrap(), original);
        editor.redo(revision + 2).unwrap();
        assert_eq!(
            source.serialize(editor.document()).unwrap(),
            original.replacen("helo", "hello", 1)
        );
    }
}

#[test]
fn unsafe_fixes_and_changed_personal_context_are_rejected_and_review_counts_agree() {
    use vsf_application::{
        quality::statistics,
        text::{self, Operation},
    };
    let p = profile(Language::EnglishUs);
    let d = document(&["he<i>lo</i>", "Shinjix"]);
    let correction = Operation::LanguageCorrection {
        cue_index: 0,
        start: 0,
        end: 7,
        original: "helo".into(),
        replacement: "hello".into(),
    };
    assert!(text::propose(d.clone(), &p, &correction, || Ok(())).is_err());
    let words = WordLists {
        english_us: vec!["Shinjix".into()],
        ..WordLists::default()
    };
    let lexicon = Lexicon::new(&words, &WordLists::default()).unwrap();
    let context = Evaluation {
        profile: &p,
        format: Format::Srt,
        lexicon: Some(&lexicon),
    };
    let old_finding = Operation::LanguageCorrection {
        cue_index: 1,
        start: 0,
        end: 7,
        original: "Shinjix".into(),
        replacement: "Shinji".into(),
    };
    assert!(
        text::propose_for_format(d.clone(), Format::Srt, context, &old_finding, || Ok(())).is_err()
    );
    let report = quality::evaluate(&d, context, |_, _| Ok(())).unwrap();
    let plan = text::propose_for_format(
        d.clone(),
        Format::Srt,
        context,
        &Operation::Find {
            query: "Shinjix".into(),
            case_sensitive: true,
        },
        || Ok(()),
    )
    .unwrap();
    assert_eq!(plan.before_findings(), report.findings.len());
    let stats = statistics::evaluate(
        &d,
        context,
        &quality::scene::Scenes::NoMedia,
        statistics::Selection::default(),
        || Ok(()),
    )
    .unwrap();
    let expected =
        statistics::summarize(report, statistics::Selection::default(), || Ok(())).unwrap();
    assert_eq!(stats, expected);
}
