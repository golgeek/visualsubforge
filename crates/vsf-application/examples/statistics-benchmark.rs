//! Declared full checker + statistics workloads; no precomputed/mocked metrics.
use std::{
    io::{self, Write},
    time::{Duration, Instant},
};
use subtitle_core::{Cue, CueId, Document, Milliseconds};
use vsf_application::quality::{
    Profile,
    scene::Scenes,
    statistics::{self, Range, Selection},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let text = "{\\c&H00FFFF&}<i>This is a subtitle line with Unicode 日本語 😀.</i>\nAnother line to wrap.";
    let document = Document::new(
        (0..10000u64)
            .map(|i| {
                Cue::new(
                    CueId(i + 1),
                    Milliseconds::new(i * 1400 + 650).unwrap(),
                    Milliseconds::new(i * 1400 + 650 + if i % 2 == 0 { 400 } else { 1300 })
                        .unwrap(),
                    text.into(),
                )
                .unwrap()
            })
            .collect(),
    )?;
    let scenes = Scenes::Complete(
        (1..=10000)
            .map(|i| Milliseconds::new(i * 1400).unwrap())
            .collect(),
    );
    let profile = Profile::default();
    println!(
        "fixture_ready pid={} cues=10000 cuts=10000 text_bytes={}",
        std::process::id(),
        text.len() * 10000
    );
    io::stdout().flush()?;
    std::thread::sleep(Duration::from_millis(500));
    let mut workloads = vec![];
    for (operation, selection, count, finite, pairs) in [
        ("document", Selection::default(), 10000, 5000, 9999),
        (
            "range_exclusions",
            Selection {
                range: Some(Range {
                    first: 1001,
                    last: 9000,
                }),
                excluded: (1001..=9000).filter(|n| n % 4 == 0).collect(),
            },
            6000,
            2000,
            4000,
        ),
    ] {
        let mut times = vec![];
        for run in 0..21 {
            let start = Instant::now();
            let report =
                statistics::evaluate(&document, &profile, &scenes, selection.clone(), || Ok(()))?;
            let s = &report.summary;
            if s.included_cues != count
                || s.rs.count != finite
                || s.unavailable_rs.total != count - finite
                || s.gaps_ms.count != pairs
                || s.overlapping_pairs != 0.
            {
                return Err("Unexpected statistics benchmark population".into());
            }
            if run > 0 {
                times.push(start.elapsed().as_secs_f64() * 1000.);
            }
        }
        times.sort_by(f64::total_cmp);
        workloads.push(serde_json::json!({"operation":operation,"runs":20,"p95_ms":times[18],"maximum_ms":times[19],"included":count}));
    }
    let passed = workloads
        .iter()
        .all(|w| w["p95_ms"].as_f64().unwrap() < 500.);
    println!(
        "{}",
        serde_json::json!({"cues":10000,"cuts":10000,"cpu_budget_ms":500,"workloads":workloads,"passed":passed,"profile":profile})
    );
    if !passed {
        return Err("Statistics benchmark exceeded declared CPU budget".into());
    }
    Ok(())
}
