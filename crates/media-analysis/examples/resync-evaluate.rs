//! Evaluate the experiment against independently generated edit-list truth.
//! `cargo run -p media-analysis --example resync-evaluate -- <fixture directory>`
use media_analysis::{Cancellation, Tools, alignment, scenes::visual};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf, time::Duration};

#[derive(Deserialize)]
struct Truth {
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    source: String,
    target: String,
    expected: String,
    segments: Vec<Range>,
    boundary_tolerance_us: u64,
}
#[derive(Deserialize)]
struct Range {
    source_start_us: u64,
    source_end_us: u64,
    target_start_us: u64,
    target_end_us: u64,
}
#[derive(Serialize)]
struct Measurement {
    case: String,
    source_anchors: usize,
    target_anchors: usize,
    covered_interior_fraction: f64,
    worst_error_us: f64,
    median_error_us: f64,
    spans_unmatched_content: bool,
    passed: bool,
    alignment: alignment::Alignment,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| "tests/fixtures/generated/resync".into());
    let truth: Truth = serde_json::from_slice(&std::fs::read(path.join("truth.json"))?)?;
    let tools = Tools {
        ffmpeg: std::env::var_os("VSF_FFMPEG")
            .map(PathBuf::from)
            .unwrap_or_else(|| "ffmpeg".into()),
        ffprobe: std::env::var_os("VSF_FFPROBE")
            .map(PathBuf::from)
            .unwrap_or_else(|| "ffprobe".into()),
    };
    let cancellation = Cancellation::default();
    let mut signatures = BTreeMap::new();
    for case in &truth.cases {
        for name in [&case.source, &case.target] {
            if !signatures.contains_key(name) {
                eprintln!("Analyzing {name}");
                signatures.insert(
                    name.clone(),
                    visual::extract(
                        &tools,
                        &path.join(name),
                        &cancellation,
                        Duration::from_secs(180),
                    )?,
                );
            }
        }
    }
    let mut measurements = vec![];
    for case in truth.cases {
        let source = &signatures[&case.source];
        let target = &signatures[&case.target];
        let result = alignment::align(source, target, &cancellation, Duration::from_secs(30))?;
        let mut total = 0;
        let mut errors = vec![];
        for expected in &case.segments {
            for time in ((expected.source_start_us + 1_000_000)
                ..expected.source_end_us.saturating_sub(1_000_000))
                .step_by(250_000)
            {
                total += 1;
                if let Some(actual) = result
                    .segments
                    .iter()
                    .find_map(|segment| segment.map_us(time))
                {
                    let predicted = expected.target_start_us as f64
                        + (time - expected.source_start_us) as f64
                            * (expected.target_end_us - expected.target_start_us) as f64
                            / (expected.source_end_us - expected.source_start_us) as f64;
                    errors.push((actual - predicted).abs());
                }
            }
        }
        let spans_unmatched_content = result.segments.iter().any(|s| {
            !case.segments.iter().any(|r| {
                s.source_start_us >= r.source_start_us
                    && s.source_end_us <= r.source_end_us
                    && s.target_start_us.saturating_add(case.boundary_tolerance_us)
                        >= r.target_start_us
                    && s.target_end_us <= r.target_end_us + case.boundary_tolerance_us
            })
        });
        errors.sort_by(f64::total_cmp);
        let worst_error_us = errors.last().copied().unwrap_or(0.);
        let median_error_us = errors.get(errors.len() / 2).copied().unwrap_or(0.);
        let coverage = if total == 0 {
            0.
        } else {
            errors.len() as f64 / total as f64
        };
        let passed = match case.expected.as_str() {
            "abstain" => result.segments.is_empty(),
            "reject_reordered" => result.reordered && result.segments.is_empty(),
            "map_supported_interiors" => {
                coverage >= 0.9
                    && !spans_unmatched_content
                    && worst_error_us <= case.boundary_tolerance_us as f64
            }
            other => return Err(format!("Unknown expectation {other}").into()),
        };
        eprintln!(
            "{}: coverage {:.1}%, worst error {:.3} ms, {}",
            case.name,
            coverage * 100.,
            worst_error_us / 1_000.,
            if passed { "PASS" } else { "FAIL" }
        );
        measurements.push(Measurement {
            case: case.name,
            source_anchors: source.len(),
            target_anchors: target.len(),
            covered_interior_fraction: coverage,
            worst_error_us,
            median_error_us,
            spans_unmatched_content,
            passed,
            alignment: result,
        });
    }
    serde_json::to_writer_pretty(std::io::stdout().lock(), &measurements)?;
    if measurements.iter().any(|m| !m.passed) {
        std::process::exit(2);
    }
    Ok(())
}
