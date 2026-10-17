//! Repeatable text-proposal workloads; RSS is sampled externally.
use std::{
    io::{self, Write},
    time::{Duration, Instant},
};
use subtitle_core::{Cue, CueId, Document, Milliseconds};
use vsf_application::{
    quality::Profile,
    text::{self, Operation},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let source = "{\\c&H00FFFF&}<i>This is a subtitle line with Unicode 日本語 😀.</i>\nAnother line to wrap.";
    let document = Document::new(
        (0..10_000u64)
            .map(|i| {
                Cue::new(
                    CueId(i + 1),
                    Milliseconds::new(i * 4100).unwrap(),
                    Milliseconds::new(i * 4100 + 4000).unwrap(),
                    source.into(),
                )
                .unwrap()
            })
            .collect(),
    )?;
    println!(
        "fixture_ready pid={} cues=10000 text_bytes={}",
        std::process::id(),
        source.len() * 10_000
    );
    io::stdout().flush()?;
    std::thread::sleep(Duration::from_millis(500));
    let mut results = vec![];
    for operation in [
        Operation::Find {
            query: "line".into(),
            case_sensitive: false,
        },
        Operation::Replace {
            query: "line".into(),
            replacement: "sentence".into(),
            case_sensitive: false,
        },
        Operation::JoinLines,
        Operation::Reflow,
    ] {
        let mut profile = Profile::default().custom("benchmark".into(), "Benchmark".into());
        if matches!(operation, Operation::JoinLines) {
            profile.rules.lines.maximum_characters = 128;
            profile.rules.lines.single_line_preferred = 128;
        }
        text::propose(document.clone(), &profile, &operation, || Ok(()))?;
        let mut times = vec![];
        let mut changed = 0;
        for _ in 0..20 {
            let start = Instant::now();
            let proposal = text::propose(document.clone(), &profile, &operation, || Ok(()))?;
            changed = proposal.changed();
            times.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        times.sort_by(f64::total_cmp);
        if !matches!(operation, Operation::Find { .. }) && changed != 10_000 {
            return Err("Benchmark must exercise 10,000 actual changes".into());
        }
        results.push(serde_json::json!({"operation": operation, "runs": 20, "p95_ms": times[18], "maximum_ms": times[19], "changed": changed}));
    }
    let passed = results
        .iter()
        .all(|r| r["p95_ms"].as_f64().unwrap() < 500.0);
    println!(
        "{}",
        serde_json::json!({"cues":10000,"cpu_budget_ms":500,"workloads":results,"passed":passed})
    );
    if !passed {
        return Err("Text benchmark exceeded predeclared CPU budget".into());
    }
    Ok(())
}
