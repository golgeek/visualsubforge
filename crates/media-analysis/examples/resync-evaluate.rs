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
    unsafe_after_us: Option<u64>,
    unsafe_before_us: Option<u64>,
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
    worst_endpoint_error_us: f64,
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
    let versions = tools.versions(&cancellation, Duration::from_secs(10))?;
    let mut signatures = BTreeMap::new();
    let mut samples = BTreeMap::new();
    let mut final_frames = BTreeMap::new();
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
                samples.insert(
                    name.clone(),
                    visual::sample(
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
        let mut result = alignment::align(source, target, &cancellation, Duration::from_secs(30))?;
        if !result.segments.is_empty() {
            for name in [&case.source, &case.target] {
                if !final_frames.contains_key(name) {
                    final_frames.insert(
                        name.clone(),
                        visual::last_frame(
                            &tools,
                            &path.join(name),
                            &cancellation,
                            Duration::from_secs(30),
                        )?,
                    );
                }
            }
        }
        let detailed = {
            let requests = alignment::verify::target_sample_times(
                &result,
                &samples[&case.source],
                &cancellation,
                Duration::from_secs(30),
            )?;
            eprintln!("{}: {} targeted frame requests", case.name, requests.len());
            visual::sample_near(
                &tools,
                &path.join(&case.target),
                &requests,
                alignment::verify::LOOKAROUND_US,
                &versions.ffmpeg,
                &cancellation,
                Duration::from_secs(180),
            )?
        };
        alignment::verify::refine(
            &mut result,
            source,
            target,
            alignment::verify::VerificationSamples {
                source: &samples[&case.source],
                target: &samples[&case.target],
                target_windows: Some(&detailed),
                ends: final_frames
                    .get(&case.source)
                    .and_then(Option::as_ref)
                    .zip(final_frames.get(&case.target).and_then(Option::as_ref))
                    .map(|(source, target)| alignment::verify::VideoEnds { source, target }),
            },
            &cancellation,
            Duration::from_secs(30),
        )?;
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
        let (spans_unmatched_content, worst_endpoint_error_us) = boundary_metrics(&case, &result);
        errors.sort_by(f64::total_cmp);
        let worst_error_us = errors.last().copied().unwrap_or(0.);
        let median_error_us = errors.get(errors.len() / 2).copied().unwrap_or(0.);
        let coverage = if total == 0 {
            0.
        } else {
            errors.len() as f64 / total as f64
        };
        let correct = !spans_unmatched_content
            && worst_error_us.max(worst_endpoint_error_us) <= case.boundary_tolerance_us as f64;
        let passed = correct
            && match case.expected.as_str() {
                "abstain" => result.segments.is_empty(),
                "reject_reordered" => result.reordered && result.segments.is_empty(),
                "reject_ambiguous_tail" => case
                    .unsafe_after_us
                    .is_some_and(|end| result.segments.iter().all(|s| s.source_end_us <= end)),
                "reject_ambiguous_head" => case.unsafe_before_us.is_some_and(|start| {
                    result.segments.iter().all(|s| s.source_start_us >= start)
                }),
                "map_supported_interiors" => coverage >= 0.9,
                // Distortions characterize unpromised coverage, not a relaxed
                // correctness gate. Every accepted interval must still agree with
                // independently recorded content/timing and avoid edit boundaries.
                "safe_partial_mapping" => true,
                other => return Err(format!("Unknown expectation {other}").into()),
            };
        eprintln!(
            "{}: coverage {:.1}%, worst interior/endpoint error {:.3}/{:.3} ms, {}",
            case.name,
            coverage * 100.,
            worst_error_us / 1_000.,
            worst_endpoint_error_us / 1_000.,
            if passed { "PASS" } else { "FAIL" }
        );
        measurements.push(Measurement {
            case: case.name,
            source_anchors: source.len(),
            target_anchors: target.len(),
            covered_interior_fraction: coverage,
            worst_error_us,
            median_error_us,
            worst_endpoint_error_us,
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

// The quarter-second interior grid measures coverage, but cannot detect an
// incorrect short accepted interval or an error confined to a boundary. Since
// both maps are affine, the largest error within a supported interval is at one
// of its endpoints. Check every interval, including those with no grid samples.
fn boundary_metrics(case: &Case, alignment: &alignment::Alignment) -> (bool, f64) {
    let mut unsupported = false;
    let mut worst: f64 = 0.;
    for segment in &alignment.segments {
        let truth = case.segments.iter().find(|truth| {
            segment.source_start_us >= truth.source_start_us
                && segment.source_end_us <= truth.source_end_us
                && segment
                    .target_start_us
                    .saturating_add(case.boundary_tolerance_us)
                    >= truth.target_start_us
                && segment.target_end_us
                    <= truth
                        .target_end_us
                        .saturating_add(case.boundary_tolerance_us)
        });
        let Some(truth) = truth else {
            unsupported = true;
            continue;
        };
        for (source, target) in [
            (segment.source_start_us, segment.target_start_us),
            (segment.source_end_us, segment.target_end_us),
        ] {
            let expected = truth.target_start_us as f64
                + (source - truth.source_start_us) as f64
                    * (truth.target_end_us - truth.target_start_us) as f64
                    / (truth.source_end_us - truth.source_start_us) as f64;
            worst = worst.max((target as f64 - expected).abs());
        }
    }
    (unsupported, worst)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn case() -> Case {
        Case {
            name: "oracle control".into(),
            source: String::new(),
            target: String::new(),
            expected: "safe_partial_mapping".into(),
            segments: vec![Range {
                source_start_us: 0,
                source_end_us: 1_000_000,
                target_start_us: 0,
                target_end_us: 1_000_000,
            }],
            boundary_tolerance_us: 10_000,
            unsafe_after_us: None,
            unsafe_before_us: None,
        }
    }

    fn result(start: u64, end: u64, target_start: u64, target_end: u64) -> alignment::Alignment {
        alignment::Alignment {
            segments: vec![alignment::Segment {
                source_start_us: start,
                source_end_us: end,
                target_start_us: target_start,
                target_end_us: target_end,
                anchors: 3,
                max_residual_us: 0.,
            }],
            distinctive_matches: 3,
            ambiguous_source_anchors: 0,
            reordered: false,
        }
    }

    #[test]
    fn incorrect_short_interval_cannot_hide_outside_the_interior_grid() {
        assert_eq!(
            boundary_metrics(&case(), &result(0, 500_000, 200_000, 700_000)),
            (false, 200_000.)
        );
    }

    #[test]
    fn scaling_checks_both_endpoints_against_independent_truth() {
        let mut case = case();
        case.segments[0].target_end_us = 2_000_000;
        assert_eq!(
            boundary_metrics(&case, &result(200_000, 400_000, 400_000, 800_000)),
            (false, 0.)
        );
        assert_eq!(
            boundary_metrics(&case, &result(200_000, 400_000, 400_000, 900_000)),
            (false, 100_000.)
        );
    }

    #[test]
    fn an_interval_cannot_bridge_a_removed_piece() {
        let mut case = case();
        case.segments[0].source_end_us = 400_000;
        case.segments[0].target_end_us = 400_000;
        case.segments.push(Range {
            source_start_us: 600_000,
            source_end_us: 1_000_000,
            target_start_us: 400_000,
            target_end_us: 800_000,
        });
        assert!(boundary_metrics(&case, &result(0, 1_000_000, 0, 800_000)).0);
    }
}
