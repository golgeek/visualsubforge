//! Verify additional intervals against decoded image samples. The timing model
//! always comes from distinctive cut anchors; samples never infer a frame rate.
use super::{Alignment, Segment, TIMING_TOLERANCE_US};
use crate::{
    Cancellation, Error, Result,
    scenes::visual::{Anchor, MAX_SAMPLES, distance, has_detail},
};
use std::time::{Duration, Instant};

const SAMPLE_RADIUS_US: u64 = 300_000;
const MAX_SAMPLE_GAP_US: u64 = 650_000;
const MAX_IMAGE_ERROR: f64 = 6.;
const MIN_RUN_US: u64 = 2_000_000;

pub fn refine(
    alignment: &mut Alignment,
    source_cuts: &[Anchor],
    target_cuts: &[Anchor],
    source_samples: &[Anchor],
    target_samples: &[Anchor],
    cancellation: &Cancellation,
    timeout: Duration,
) -> Result<()> {
    let started = Instant::now();
    let check = || {
        if cancellation.is_cancelled() {
            Err(Error::Cancelled)
        } else if started.elapsed() >= timeout {
            Err(Error::Timeout)
        } else {
            Ok(())
        }
    };
    check()?;
    for samples in [source_samples, target_samples, source_cuts, target_cuts] {
        if samples.len() > MAX_SAMPLES
            || samples.windows(2).any(|p| p[0].time_us >= p[1].time_us)
            || samples.iter().any(|s| s.time_us > 14_400_000_000)
        {
            return Err(Error::Message(
                "Invalid or excessive verification samples".into(),
            ));
        }
    }
    if alignment.reordered || alignment.segments.is_empty() {
        return Ok(());
    }
    if alignment.segments.iter().any(|s| {
        s.source_start_us >= s.source_end_us
            || s.target_start_us >= s.target_end_us
            || s.source_end_us > 14_400_000_000
            || s.target_end_us > 14_400_000_000
            || !s.max_residual_us.is_finite()
    }) || alignment.segments.windows(2).any(|p| {
        p[0].source_end_us > p[1].source_start_us || p[0].target_end_us > p[1].target_start_us
    }) {
        return Err(Error::Message("Invalid verification timing model".into()));
    }
    let seeds = &alignment.segments;
    let mut additions = Vec::new();
    for (index, seed) in seeds.iter().enumerate() {
        check()?;
        // A distinctive neighboring block is contradictory evidence for this
        // model outside its neighborhood. Do not carry an offset past that block.
        let lower = index
            .checked_sub(1)
            .map(|i| seeds[i].source_end_us)
            .unwrap_or(0);
        let upper = seeds
            .get(index + 1)
            .map(|s| s.source_start_us)
            .unwrap_or(u64::MAX);
        let samples = &source_samples[source_samples.partition_point(|s| s.time_us < lower)
            ..source_samples.partition_point(|s| s.time_us <= upper)];
        let mut run: Option<(u64, u64)> = None;
        for (sample_index, sample) in samples.iter().enumerate() {
            check()?;
            let source_time = sample.time_us;
            let predicted = project(seed, source_time);
            let agrees = temporally_distinct(samples, sample_index)
                && predicted.is_some_and(|time| image_agrees(sample, target_samples, time));
            let continuous = run.is_none_or(|(_, previous)| {
                source_time - previous <= MAX_SAMPLE_GAP_US
                    && cuts_agree(seed, previous, source_time, source_cuts, target_cuts)
            });
            if !agrees || !continuous {
                finish(seed, run.take(), &mut additions);
            }
            if agrees {
                run = Some((run.map(|r| r.0).unwrap_or(source_time), source_time));
            }
        }
        finish(seed, run, &mut additions);
    }
    check()?;
    // Work on a separate vector so failure/cancellation cannot publish half of a
    // refinement. Only merge overlapping intervals with the same timing model.
    let mut segments = alignment.segments.clone();
    segments.extend(additions);
    segments.sort_by_key(|s| s.source_start_us);
    let mut merged: Vec<Segment> = Vec::new();
    for segment in segments {
        check()?;
        if let Some(previous) = merged.last_mut()
            && segment.source_start_us <= previous.source_end_us
            && same_map(previous, &segment)
        {
            if segment.source_end_us > previous.source_end_us {
                previous.source_end_us = segment.source_end_us;
                previous.target_end_us = segment.target_end_us;
            }
            previous.max_residual_us = previous.max_residual_us.max(segment.max_residual_us);
            previous.anchors = previous.anchors.max(segment.anchors);
        } else {
            merged.push(segment);
        }
    }
    // Conflicting verified models remain unresolved rather than being selected
    // by iteration order. Remove their overlapping intervals entirely.
    let mut accepted = Vec::new();
    let mut group = Vec::<Segment>::new();
    let (mut source_end, mut target_end) = (0, 0);
    for segment in merged {
        check()?;
        if segment.source_start_us >= source_end && segment.target_start_us >= target_end {
            if group.len() == 1 {
                accepted.append(&mut group);
            }
            group.clear();
            source_end = 0;
            target_end = 0;
        }
        source_end = source_end.max(segment.source_end_us);
        target_end = target_end.max(segment.target_end_us);
        group.push(segment);
    }
    if group.len() == 1 {
        accepted.append(&mut group);
    }
    check()?;
    alignment.segments = accepted;
    Ok(())
}

fn temporally_distinct(samples: &[Anchor], index: usize) -> bool {
    let sample = &samples[index];
    let previous = index.checked_sub(1).map(|i| &samples[i]);
    let next = samples.get(index + 1);
    (previous.is_some() || next.is_some())
        && [previous, next].into_iter().flatten().all(|neighbor| {
            sample.time_us.abs_diff(neighbor.time_us) <= MAX_SAMPLE_GAP_US
                && distance(sample, neighbor) >= 0.25
        })
}

fn project(segment: &Segment, time: u64) -> Option<u64> {
    let value = segment.target_start_us as f64
        + (time as f64 - segment.source_start_us as f64)
            * (segment.target_end_us - segment.target_start_us) as f64
            / (segment.source_end_us - segment.source_start_us) as f64;
    (value.is_finite() && (0. ..=14_400_000_000.).contains(&value)).then_some(value.round() as u64)
}

fn image_agrees(sample: &Anchor, targets: &[Anchor], time: u64) -> bool {
    if !has_detail(sample) {
        return false;
    }
    let best_near = |time: u64| {
        let first = targets.partition_point(|t| t.time_us < time.saturating_sub(SAMPLE_RADIUS_US));
        targets[first..]
            .iter()
            .take_while(|t| t.time_us <= time.saturating_add(SAMPLE_RADIUS_US))
            .map(|target| distance(sample, target))
            .fold(f64::INFINITY, f64::min)
    };
    let best = best_near(time);
    if best > MAX_IMAGE_ERROR {
        return false;
    }
    // A static or locally repeating shot can look right at the wrong time.
    // Require temporal distinction from alternative offsets as well as spatial
    // detail. This does not replace the distinctive cut-derived timing model.
    let alternative = [-4_000_000i64, -2_000_000, 2_000_000, 4_000_000]
        .iter()
        .filter_map(|offset| time.checked_add_signed(*offset))
        .map(best_near)
        .fold(f64::INFINITY, f64::min);
    alternative - best >= 0.5
}

fn cuts_agree(model: &Segment, start: u64, end: u64, source: &[Anchor], target: &[Anchor]) -> bool {
    let (Some(target_start), Some(target_end)) = (project(model, start), project(model, end))
    else {
        return false;
    };
    let tolerance = TIMING_TOLERANCE_US as u64;
    let nearby = |times: &[Anchor], time: u64| {
        let i = times.partition_point(|a| a.time_us < time.saturating_sub(tolerance));
        times
            .get(i)
            .is_some_and(|a| a.time_us.abs_diff(time) <= tolerance)
    };
    source[source.partition_point(|a| a.time_us <= start)..]
        .iter()
        .take_while(|a| a.time_us <= end)
        .all(|a| project(model, a.time_us).is_some_and(|t| nearby(target, t)))
        && target[target.partition_point(|a| a.time_us <= target_start)..]
            .iter()
            .take_while(|a| a.time_us <= target_end)
            .all(|a| {
                let source_time = model.source_start_us as f64
                    + (a.time_us as f64 - model.target_start_us as f64)
                        * (model.source_end_us - model.source_start_us) as f64
                        / (model.target_end_us - model.target_start_us) as f64;
                source_time >= 0. && nearby(source, source_time.round() as u64)
            })
}

fn finish(seed: &Segment, run: Option<(u64, u64)>, output: &mut Vec<Segment>) {
    if let Some((start, end)) = run
        && end - start >= MIN_RUN_US
        && let (Some(target_start), Some(target_end)) = (project(seed, start), project(seed, end))
    {
        output.push(Segment {
            source_start_us: start,
            source_end_us: end,
            target_start_us: target_start,
            target_end_us: target_end,
            anchors: seed.anchors,
            max_residual_us: seed.max_residual_us,
        });
    }
}

fn same_map(a: &Segment, b: &Segment) -> bool {
    [b.source_start_us, b.source_end_us].iter().all(|&time| {
        project(a, time)
            .zip(project(b, time))
            .is_some_and(|(x, y)| x.abs_diff(y) <= 1_000)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frames(offset: u64, still_after: Option<usize>) -> Vec<Anchor> {
        (0..20)
            .map(|i| {
                let identity = still_after.map_or(i, |end| i.min(end));
                let mut state = identity as u32 + 1;
                let mut pixels = [0; 256];
                for pixel in &mut pixels {
                    state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                    *pixel = (state >> 24) as u8;
                }
                Anchor {
                    time_us: offset + i as u64 * 500_000,
                    pixels,
                }
            })
            .collect()
    }
    fn seed() -> Alignment {
        Alignment {
            segments: vec![Segment {
                source_start_us: 2_000_000,
                source_end_us: 4_000_000,
                target_start_us: 3_000_000,
                target_end_us: 5_000_000,
                anchors: 3,
                max_residual_us: 0.,
            }],
            distinctive_matches: 3,
            ambiguous_source_anchors: 0,
            reordered: false,
        }
    }
    fn run(
        alignment: &mut Alignment,
        source: &[Anchor],
        target: &[Anchor],
        target_cuts: &[Anchor],
    ) {
        refine(
            alignment,
            &[],
            target_cuts,
            source,
            target,
            &Cancellation::default(),
            Duration::from_secs(1),
        )
        .unwrap();
    }
    #[test]
    fn observed_motion_can_extend_a_model_but_never_beyond_samples() {
        let mut result = seed();
        run(&mut result, &frames(0, None), &frames(1_000_000, None), &[]);
        assert_eq!(result.segments.len(), 1);
        assert_eq!(result.segments[0].source_start_us, 0);
        assert_eq!(result.segments[0].source_end_us, 9_500_000);
        assert_eq!(result.segments[0].map_us(9_750_000), None);
    }
    #[test]
    fn static_tail_does_not_prove_timing_or_extend_the_model() {
        let mut result = seed();
        run(
            &mut result,
            &frames(0, Some(8)),
            &frames(1_000_000, Some(8)),
            &[],
        );
        assert!(result.segments.iter().all(|s| s.source_end_us <= 4_000_000));
    }
    #[test]
    fn unmatched_cuts_break_verified_intervals_even_between_matching_samples() {
        let mut result = seed();
        let cut = Anchor {
            time_us: 5_250_000,
            pixels: [0; 256],
        };
        run(
            &mut result,
            &frames(0, None),
            &frames(1_000_000, None),
            &[cut],
        );
        assert!(
            result
                .segments
                .iter()
                .all(|s| s.map_us(4_250_000).is_none())
        );
        assert!(
            result
                .segments
                .iter()
                .any(|s| s.source_start_us >= 4_500_000)
        );
    }
    #[test]
    fn absent_timing_model_cannot_be_invented_from_samples() {
        let mut result = seed();
        result.segments.clear();
        run(&mut result, &frames(0, None), &frames(1_000_000, None), &[]);
        assert!(result.segments.is_empty());
    }
    #[test]
    fn cancellation_and_invalid_models_preserve_the_original_result() {
        let mut result = seed();
        let before = serde_json::to_string(&result).unwrap();
        let cancellation = Cancellation::default();
        cancellation.cancel();
        assert!(matches!(
            refine(
                &mut result,
                &[],
                &[],
                &[],
                &[],
                &cancellation,
                Duration::from_secs(1)
            ),
            Err(Error::Cancelled)
        ));
        assert_eq!(serde_json::to_string(&result).unwrap(), before);
        result.segments[0].target_end_us = 0;
        assert!(
            refine(
                &mut result,
                &[],
                &[],
                &[],
                &[],
                &Cancellation::default(),
                Duration::from_secs(1)
            )
            .is_err()
        );
    }
}
