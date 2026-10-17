//! Whole-document checks/repairs with 10,000 real candidates and 10,000 cuts.
use std::{
    io::{self, Write},
    time::{Duration, Instant},
};
use subtitle_core::{Cue, CueId, Document, Milliseconds};
use vsf_application::quality::{
    Profile, evaluate_with_scenes,
    scene::{self, Scenes},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let source = "{\\c&H00FFFF&}<i>This is a subtitle line with Unicode 日本語 😀.</i>\nAnother line to wrap.";
    let document = Document::new(
        (0..10_000u64)
            .map(|i| {
                Cue::new(
                    CueId(i + 1),
                    Milliseconds::new(i * 1400 + 950).unwrap(),
                    Milliseconds::new(i * 1400 + 1850).unwrap(),
                    source.into(),
                )
                .unwrap()
            })
            .collect(),
    )?;
    let scenes = Scenes::Complete(
        (0..10_000)
            .map(|i| Milliseconds::new(i * 1400 + 1000).unwrap())
            .collect(),
    );
    let mut profile =
        Profile::default().custom("scene-benchmark".into(), "Scene benchmark workload".into());
    // Deliberately permissive stress policy keeps all 10,000 trims actionable.
    // Default limits and refusal behavior have separate correctness fixtures.
    profile.rules.duration.minimum_ms = 500;
    profile.rules.speed.maximum_cps = Some(150);
    profile.rules.speed.maximum_rs_exclusive = Some(500);
    println!(
        "fixture_ready pid={} cues=10000 cuts=10000 text_bytes={}",
        std::process::id(),
        source.len() * 10_000
    );
    io::stdout().flush()?;
    std::thread::sleep(Duration::from_millis(500));
    let mut results = vec![];
    for repair in [false, true] {
        let mut times = vec![];
        let mut changed = 0;
        for run in 0..21 {
            let start = Instant::now();
            if repair {
                let proposal = scene::propose(document.clone(), &profile, &scenes, || Ok(()))?;
                changed = proposal.changed();
                if changed != 10_000 {
                    return Err("Scene benchmark must exercise 10000 actual timing changes".into());
                }
            } else {
                let report = evaluate_with_scenes(&document, &profile, &scenes, |_, _| Ok(()))?;
                if report
                    .findings
                    .iter()
                    .filter(|f| matches!(f.rule, vsf_application::quality::Rule::SceneStart))
                    .count()
                    != 10_000
                {
                    return Err("Scene benchmark must check 10000 scene findings".into());
                }
            }
            if run > 0 {
                times.push(start.elapsed().as_secs_f64() * 1000.);
            }
        }
        times.sort_by(f64::total_cmp);
        results.push(serde_json::json!({"operation":if repair {"repair"} else {"check"},"runs":20,"p95_ms":times[18],"maximum_ms":times[19],"changed":changed}));
    }
    let passed = results.iter().all(|r| r["p95_ms"].as_f64().unwrap() < 500.);
    println!(
        "{}",
        serde_json::json!({"cues":10000,"cuts":10000,"cpu_budget_ms":500,"workloads":results,"passed":passed,"profile":profile})
    );
    if !passed {
        return Err("Scene benchmark exceeded declared CPU budget".into());
    }
    Ok(())
}
