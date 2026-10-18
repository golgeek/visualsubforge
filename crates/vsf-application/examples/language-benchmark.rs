//! Whole-document language analysis/proposals. Sampled RSS is measured externally.
use std::{
    io::{self, Write},
    time::{Duration, Instant},
};
use subtitle_core::{Cue, CueId, Document, Milliseconds};
use subtitle_formats::Format;
use vsf_application::{
    language::{self, Language, Lexicon, Settings},
    quality::{self, Evaluation, Profile, Rule},
    text::{self, Operation},
};

fn fixture(language: Language, format: Format) -> Document {
    Document::new(
        (0..10_000u64)
            .map(|i| {
                let base = if language == Language::EnglishUs {
                    "helo, the students don't know... 😀"
                } else {
                    "bonjor, les e\u{301}le\u{300}ves aiment l'amour ! 😀"
                };
                let mut line = format!("{base} Fanword{i}");
                if i % 10 < 2 {
                    line.push_str(&format!(" Fanword{}", 10000 + i));
                }
                let source = match i % 10 {
                    1 if format == Format::Srt => format!("<i>{line}</i>"),
                    1 => format!(r"{{\i1}}{line}{{\i0}}"),
                    2 => format!(r"{{\an8}}{line}\N—hello"),
                    3 => format!("—{line}"),
                    8 => format!(r"{{\k20}}{line}"),
                    9 => r"{\p1}m 0 0 l 5 5".into(),
                    _ => line,
                };
                Cue::new(
                    CueId(i + 1),
                    Milliseconds::new(i * 5100).unwrap(),
                    Milliseconds::new(i * 5100 + 5000).unwrap(),
                    source,
                )
                .unwrap()
            })
            .collect(),
    )
    .unwrap()
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let baseline = fixture(Language::EnglishUs, Format::Srt);
    println!(
        "fixture_ready pid={} cues=10000 distinct_unknown_words=10000",
        std::process::id()
    );
    io::stdout().flush()?;
    std::thread::sleep(Duration::from_millis(500));
    let lexicon = Lexicon::default();
    println!("phase cold");
    let cold = Instant::now();
    assert!(language::is_correct(
        Language::EnglishUs,
        "hello",
        &lexicon
    )?);
    assert!(language::is_correct(Language::French, "bonjour", &lexicon)?);
    let cold_ms = cold.elapsed().as_secs_f64() * 1000.0;
    let mut results = vec![];
    let mut maximum_suggestion_ms = 0f64;
    for language in [Language::EnglishUs, Language::French] {
        for format in [Format::Srt, Format::Ass, Format::Ssa] {
            println!("phase {}-{}-setup", language.name(), format.extension());
            let document = if language == Language::EnglishUs && format == Format::Srt {
                baseline.clone()
            } else {
                fixture(language, format)
            };
            let mut profile =
                Profile::default().custom("language-benchmark".into(), "Language benchmark".into());
            profile.language_tools = Settings {
                dictionary: language,
                spelling: true,
                typography: true,
                capitalization: true,
                dialogue_markers: true,
            };
            let context = Evaluation {
                profile: &profile,
                format,
                lexicon: Some(&lexicon),
            };
            let report = quality::evaluate(&document, context, |_, _| Ok(()))?;
            assert_eq!(report.language_coverage.skipped_cues, 2000);
            assert_eq!(
                report
                    .findings
                    .iter()
                    .filter(|f| f.rule == Rule::Spelling
                        && f.language
                            .as_ref()
                            .is_some_and(|d| d.word.starts_with("Fanword")))
                    .count(),
                10000
            );
            let finding = report
                .findings
                .iter()
                .find(|f| f.rule == Rule::Spelling)
                .unwrap();
            let detail = finding.language.as_ref().unwrap();
            let request = Operation::LanguageCorrection {
                cue_index: finding.cue_index,
                start: detail.start,
                end: detail.end,
                original: detail.word.clone(),
                replacement: if language == Language::EnglishUs {
                    "hello"
                } else {
                    "bonjour"
                }
                .into(),
            };
            let began = Instant::now();
            let suggestions = language::suggestions(language, &detail.word, || Ok(()))?;
            maximum_suggestion_ms =
                maximum_suggestion_ms.max(began.elapsed().as_secs_f64() * 1000.0);
            assert!(!suggestions.is_empty());
            drop(report);
            text::propose_for_format(document.clone(), format, context, &request, || Ok(()))?;
            for proposal in [false, true] {
                println!(
                    "phase {}-{}-{}",
                    language.name(),
                    format.extension(),
                    if proposal { "proposal" } else { "analysis" }
                );
                let mut times = vec![];
                for _ in 0..10 {
                    let began = Instant::now();
                    if proposal {
                        let result = text::propose_for_format(
                            document.clone(),
                            format,
                            context,
                            &request,
                            || Ok(()),
                        )?;
                        assert_eq!(result.changed(), 1);
                    } else {
                        let result = quality::evaluate(&document, context, |_, _| Ok(()))?;
                        assert!(result.findings.len() >= 10_000);
                    }
                    times.push(began.elapsed().as_secs_f64() * 1000.0);
                }
                times.sort_by(f64::total_cmp);
                results.push(serde_json::json!({"language": language, "format": format.extension(), "proposal": proposal, "runs": 10, "p95_ms": times[9]}));
            }
        }
    }
    let passed = cold_ms <= 2000.0
        && maximum_suggestion_ms <= 250.0
        && results
            .iter()
            .all(|r| r["p95_ms"].as_f64().unwrap() <= 500.0);
    println!(
        "{}",
        serde_json::json!({"passed": passed, "cues":10000, "cpu_budget_ms":500, "cold_ms":cold_ms, "maximum_suggestion_ms":maximum_suggestion_ms, "workloads":results})
    );
    if !passed {
        return Err("Language benchmark exceeded predeclared CPU budget".into());
    }
    Ok(())
}
