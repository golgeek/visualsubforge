//! 10,000 real extensions/trims with completed cut data and enabled speed limits.
use std::{
    io::{self, Write},
    time::{Duration, Instant},
};
use subtitle_core::{Cue, CueId, Document, Milliseconds};
use vsf_application::quality::{
    Profile,
    duration::{self, Scope},
    scene::Scenes,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let source = "{\\c&H00FFFF&}<i>This is a subtitle line with Unicode 日本語 😀.</i>\nAnother line to wrap.";
    let make = |length| -> Result<Document, String> {
        Document::new(
            (0..10_000u64)
                .map(|i| {
                    Cue::new(
                        CueId(i + 1),
                        Milliseconds::new(i * 1400 + 650).unwrap(),
                        Milliseconds::new(i * 1400 + 650 + length).unwrap(),
                        source.into(),
                    )
                    .unwrap()
                })
                .collect(),
        )
    };
    let document = make(400)?;
    let scenes = Scenes::Complete(
        (1..=10_000)
            .map(|i| Milliseconds::new(i * 1400).unwrap())
            .collect(),
    );
    let mut profile = Profile::default().custom(
        "duration-benchmark".into(),
        "Duration benchmark workload".into(),
    );
    profile.rules.duration.minimum_ms = 800;
    profile.rules.duration.maximum_ms = 1200;
    profile.rules.speed.minimum_cps = Some(60);
    profile.rules.speed.maximum_cps = Some(100);
    profile.rules.speed.maximum_rs_exclusive = Some(500);
    println!(
        "fixture_ready pid={} cues=10000 cuts=10000 text_bytes={}",
        std::process::id(),
        source.len() * 10_000
    );
    io::stdout().flush()?;
    std::thread::sleep(Duration::from_millis(500));
    let mut results = vec![];
    for (operation, document, expected) in [("extend", document, 880), ("trim", make(1300)?, 1116)]
    {
        let mut times = vec![];
        for run in 0..21 {
            let start = Instant::now();
            let proposed =
                duration::propose(document.clone(), &profile, &scenes, Scope::Document, || {
                    Ok(())
                })?;
            if proposed.changed() != 10000
                || proposed
                    .rows()
                    .iter()
                    .enumerate()
                    .any(|(i, r)| r.new_end.unwrap().get() != i as u64 * 1400 + 650 + expected)
            {
                return Err("Duration benchmark must exercise 10000 expected end changes".into());
            }
            if run > 0 {
                times.push(start.elapsed().as_secs_f64() * 1000.);
            }
        }
        times.sort_by(f64::total_cmp);
        results.push(serde_json::json!({"operation":operation,"runs":20,"p95_ms":times[18],"maximum_ms":times[19],"changed":10000}));
    }
    let passed = results.iter().all(|r| r["p95_ms"].as_f64().unwrap() < 500.);
    println!(
        "{}",
        serde_json::json!({"cues":10000,"cuts":10000,"cpu_budget_ms":500,"workloads":results,"passed":passed,"profile":profile})
    );
    if !passed {
        return Err("Duration benchmark exceeded declared CPU budget".into());
    }
    Ok(())
}
