//! Repeatable 10,000-cue CPU/report-size check. Memory is sampled externally.
use std::{
    io::{self, Write},
    time::{Duration, Instant},
};
use subtitle_core::{Cue, CueId, Document, Milliseconds};
use vsf_application::quality::{self, Profile};

#[derive(Default)]
struct CountBytes(usize);
impl Write for CountBytes {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0 += bytes.len();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let text = "<i>This is a representative subtitle line.</i>\nAnother line with punctuation and Unicode: 日本語 😀.";
    let cues = (0..10000u64)
        .map(|i| {
            Cue::new(
                CueId(i + 1),
                Milliseconds::new(i * 4100)?,
                Milliseconds::new(i * 4100 + 4000)?,
                text.into(),
            )
        })
        .collect::<Result<Vec<_>, String>>()?;
    let document = Document::new(cues)?;
    let profile = Profile::default();
    println!(
        "fixture_ready pid={} cues=10000 text_bytes={}",
        std::process::id(),
        document
            .cues()
            .iter()
            .map(|c| c.text().len())
            .sum::<usize>()
    );
    // External RSS sampler obtains a document-only baseline before warmup.
    std::thread::sleep(Duration::from_millis(500));
    quality::evaluate(&document, &profile, |_, _| Ok(()))?;
    let mut times = Vec::new();
    let mut report_bytes = 0;
    for _ in 0..20 {
        let start = Instant::now();
        let report = quality::evaluate(&document, &profile, |_, _| Ok(()))?;
        times.push(start.elapsed().as_secs_f64() * 1000.0);
        // Match the CLI's streaming output. Desktop will serve bounded pages;
        // neither production caller needs a full serialized-report allocation.
        let mut count = CountBytes::default();
        serde_json::to_writer(&mut count, &report)?;
        report_bytes = count.0;
    }
    times.sort_by(f64::total_cmp);
    let p95 = times[18];
    println!(
        "{}",
        serde_json::json!({"runs":20,"cues":10000,"p95_ms":p95,"maximum_ms":times[19],"report_bytes":report_bytes,"cpu_budget_ms":500,"report_budget_bytes":16*1024*1024,"passed":p95<500.0 && report_bytes<16*1024*1024})
    );
    if p95 >= 500.0 || report_bytes >= 16 * 1024 * 1024 {
        return Err("Quality benchmark exceeded predeclared budget".into());
    }
    Ok(())
}
