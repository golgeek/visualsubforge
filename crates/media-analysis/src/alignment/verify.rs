//! Verify additional intervals against decoded image samples. The timing model
//! always comes from distinctive cut anchors; samples never infer a frame rate.
use super::{Alignment, Segment, TIMING_TOLERANCE_US};
mod edges;
use crate::{
    Cancellation, Error, Result,
    scenes::visual::{Anchor, MAX_SAMPLES, SampleWindows, distance, has_detail},
};
use std::{
    collections::BTreeSet,
    time::{Duration, Instant},
};

const SAMPLE_RADIUS_US: u64 = 300_000;
const MAX_SAMPLE_GAP_US: u64 = 650_000;
const MAX_IMAGE_ERROR: f64 = 6.;
const IMAGE_MARGIN: f64 = 0.5;
const MIN_RUN_US: u64 = 2_000_000;
pub const LOOKAROUND_US: u64 = 4_300_000;

#[derive(Clone, Copy)]
pub struct VerificationSamples<'a> {
    pub source: &'a [Anchor],
    pub target: &'a [Anchor],
    pub target_windows: Option<&'a SampleWindows>,
    pub ends: Option<VideoEnds<'a>>,
}

/// Final decoded frames from both videos; duration metadata is not a substitute.
#[derive(Clone, Copy)]
pub struct VideoEnds<'a> {
    pub source: &'a Anchor,
    pub target: &'a Anchor,
}

/// Additional target-frame observations for currently unsupported intervals.
/// Timing still comes from cut anchors. These requests are evidence to verify,
/// never a declaration that the corresponding source interval is supported.
pub fn target_sample_times(
    alignment: &Alignment,
    source_samples: &[Anchor],
    cancellation: &Cancellation,
    timeout: Duration,
) -> Result<Vec<u64>> {
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
    validate_model(alignment)?;
    if source_samples.len() > MAX_SAMPLES
        || source_samples
            .windows(2)
            .any(|p| p[0].time_us >= p[1].time_us)
        || source_samples
            .last()
            .is_some_and(|s| s.time_us > 14_400_000_000)
    {
        return Err(Error::Message("Invalid source sampling times".into()));
    }
    if alignment.reordered {
        return Ok(Vec::new());
    }
    let mut times = BTreeSet::new();
    for (index, seed) in alignment.segments.iter().enumerate() {
        check()?;
        let (lower, upper) = neighborhood(&alignment.segments, index);
        for sample in &source_samples[source_samples.partition_point(|s| s.time_us < lower)
            ..source_samples.partition_point(|s| s.time_us <= upper)]
        {
            check()?;
            if seed.map_us(sample.time_us).is_some() {
                continue;
            }
            if let Some(time) = project(seed, sample.time_us) {
                times.insert(time);
                if times.len() > MAX_SAMPLES {
                    return Err(Error::Message(
                        "Target sampling exceeds 30,000 requested times".into(),
                    ));
                }
            }
        }
    }
    Ok(times.into_iter().collect())
}

fn validate_model(alignment: &Alignment) -> Result<()> {
    if alignment.segments.len() > MAX_SAMPLES
        || alignment.segments.iter().any(|s| {
            s.source_start_us >= s.source_end_us
                || s.target_start_us >= s.target_end_us
                || s.source_end_us > 14_400_000_000
                || s.target_end_us > 14_400_000_000
                || !s.max_residual_us.is_finite()
        })
        || alignment.segments.windows(2).any(|p| {
            p[0].source_end_us > p[1].source_start_us || p[0].target_end_us > p[1].target_start_us
        })
    {
        return Err(Error::Message("Invalid verification timing model".into()));
    }
    Ok(())
}

fn neighborhood(seeds: &[Segment], index: usize) -> (u64, u64) {
    let seed = &seeds[index];
    // Different cut-derived models establish a discontinuity, but coarse
    // samples cannot locate it. Keep the entire gap unknown; weak extensions
    // must not overlap and erase trusted cut evidence in the conflict filter.
    let lower = index
        .checked_sub(1)
        .map(|i| {
            if same_map(&seeds[i], seed) {
                seeds[i].source_end_us
            } else {
                seed.source_start_us
            }
        })
        .unwrap_or(0);
    let upper = seeds
        .get(index + 1)
        .map(|next| {
            if same_map(seed, next) {
                next.source_start_us
            } else {
                seed.source_end_us
            }
        })
        .unwrap_or(u64::MAX);
    (lower, upper)
}

pub fn refine(
    alignment: &mut Alignment,
    source_cuts: &[Anchor],
    target_cuts: &[Anchor],
    samples: VerificationSamples<'_>,
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
    let source_samples = samples.source;
    for (samples, limit) in [
        (source_samples, MAX_SAMPLES),
        (samples.target, MAX_SAMPLES),
        (
            samples.target_windows.map_or(&[], SampleWindows::frames),
            MAX_SAMPLES,
        ),
        (source_cuts, MAX_SAMPLES),
        (target_cuts, MAX_SAMPLES),
    ] {
        if samples.len() > limit
            || samples.windows(2).any(|p| p[0].time_us >= p[1].time_us)
            || samples.iter().any(|s| s.time_us > 14_400_000_000)
        {
            return Err(Error::Message(
                "Invalid or excessive verification samples".into(),
            ));
        }
    }
    if let Some(ends) = samples.ends {
        for (end, coarse) in [(ends.source, samples.source), (ends.target, samples.target)] {
            if end.time_us > 14_400_000_000
                || coarse.last().is_some_and(|s| s.time_us > end.time_us)
            {
                return Err(Error::Message("Invalid decoded video end".into()));
            }
        }
        if samples.target_windows.is_some_and(|w| {
            w.frames()
                .last()
                .is_some_and(|s| s.time_us > ends.target.time_us)
        }) {
            return Err(Error::Message(
                "Decoded end precedes observed target frames".into(),
            ));
        }
    }
    if alignment.reordered || alignment.segments.is_empty() {
        return Ok(());
    }
    validate_model(alignment)?;
    let seeds = &alignment.segments;
    let mut additions = Vec::new();
    for (index, seed) in seeds.iter().enumerate() {
        check()?;
        let (lower, upper) = neighborhood(seeds, index);
        let local_samples = &source_samples[source_samples.partition_point(|s| s.time_us < lower)
            ..source_samples.partition_point(|s| s.time_us <= upper)];
        let mut run: Option<Run> = None;
        for (sample_index, sample) in local_samples.iter().enumerate() {
            check()?;
            let source_time = sample.time_us;
            let predicted = project(seed, source_time);
            let mut evidence = predicted.map_or(ImageEvidence::Mismatch, |time| {
                image_evidence(sample, samples, time)
            });
            if evidence == ImageEvidence::Distinct
                && !temporally_distinct(local_samples, sample_index)
            {
                evidence = ImageEvidence::Ambiguous;
            }
            let continuous = run.as_ref().is_none_or(|run| {
                source_time - run.last_seen <= MAX_SAMPLE_GAP_US
                    && cuts_agree(seed, run.last_seen, source_time, source_cuts, target_cuts)
            });
            if !continuous {
                finish(seed, run.take(), &mut additions);
            }
            match evidence {
                ImageEvidence::Distinct => {
                    let run = run.get_or_insert(Run {
                        start: source_time,
                        last_verified: source_time,
                        last_seen: source_time,
                        pending_ambiguity: false,
                    });
                    run.last_verified = source_time;
                    run.last_seen = source_time;
                    run.pending_ambiguity = false;
                }
                ImageEvidence::Ambiguous => {
                    // One matching but non-distinct observation can lie between
                    // distinctive observations. It cannot start/end an extension
                    // or bridge a second ambiguity, missing frame or changed cut.
                    if let Some(run) = run.as_mut().filter(|run| !run.pending_ambiguity) {
                        run.last_seen = source_time;
                        run.pending_ambiguity = true;
                    } else {
                        finish(seed, run.take(), &mut additions);
                    }
                }
                ImageEvidence::Mismatch => finish(seed, run.take(), &mut additions),
            }
        }
        finish(seed, run, &mut additions);
    }
    additions.extend(edges::verify(
        seeds,
        source_cuts,
        target_cuts,
        samples,
        &check,
    )?);
    check()?;
    // Work on a separate vector so failure/cancellation cannot publish half of a
    // refinement. Only merge overlapping intervals with the same timing model.
    let mut segments: Vec<_> = seeds
        .iter()
        .map(|model| VerifiedRange {
            model,
            start: model.source_start_us,
            end: model.source_end_us,
        })
        .collect();
    segments.extend(additions);
    segments.sort_by_key(|s| s.start);
    let mut merged: Vec<VerifiedRange<'_>> = Vec::new();
    for segment in segments {
        check()?;
        if let Some(previous) = merged.last_mut()
            && segment.start <= previous.end
            && maps_agree(
                previous.model,
                segment.model,
                previous.start,
                previous.end.max(segment.end),
            )
        {
            // Keep one original fit, preferring the model with more supporting
            // cuts. Mixing two models' endpoints invents a different slope;
            // repeated merges can then disagree with their own cut evidence.
            if segment.model.anchors > previous.model.anchors {
                previous.model = segment.model;
            }
            previous.end = previous.end.max(segment.end);
        } else {
            merged.push(segment);
        }
    }
    // Conflicting verified models remain unresolved rather than being selected
    // by iteration order. Remove their overlapping intervals entirely.
    let mut accepted = Vec::new();
    let mut group = Vec::<Segment>::new();
    let (mut source_end, mut target_end) = (0, 0);
    for range in merged {
        check()?;
        let segment = range.segment().ok_or_else(|| {
            Error::Message("Verified interval exceeds the timing model's range".into())
        })?;
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

#[derive(Clone, Copy, PartialEq)]
enum ImageEvidence {
    Mismatch,
    Ambiguous,
    Distinct,
}

fn image_evidence(sample: &Anchor, samples: VerificationSamples<'_>, time: u64) -> ImageEvidence {
    let end = time.saturating_add(LOOKAROUND_US).min(14_400_000_000);
    let end = samples
        .ends
        .map_or(end, |ends| end.min(ends.target.time_us));
    let dense = samples
        .target_windows
        .filter(|windows| windows.covers_interval(time.saturating_sub(LOOKAROUND_US), end));
    let targets = dense.map_or(samples.target, SampleWindows::frames);
    // Never extrapolate beyond the observed presentation timestamps.
    if targets.first().is_none_or(|s| time < s.time_us)
        || targets.last().is_none_or(|s| time > s.time_us)
    {
        return ImageEvidence::Mismatch;
    }
    if !has_detail(sample) {
        return ImageEvidence::Mismatch;
    }
    let best_near = |time: u64| {
        let first = targets.partition_point(|t| t.time_us < time.saturating_sub(SAMPLE_RADIUS_US));
        targets[first..]
            .iter()
            .take_while(|t| t.time_us <= time.saturating_add(SAMPLE_RADIUS_US))
            .map(|target| distance(sample, target))
            .fold(f64::INFINITY, f64::min)
    };
    // The predicted time chooses the frame. Choosing the best-looking frame
    // anywhere in a 300 ms window can silently hide an edit shorter than that
    // window once dense samples are available.
    let next = targets.partition_point(|target| target.time_us < time);
    let nearest = [
        next.checked_sub(1).and_then(|i| targets.get(i)),
        targets.get(next),
    ]
    .into_iter()
    .flatten()
    .min_by_key(|target| target.time_us.abs_diff(time));
    let Some(nearest) = nearest.filter(|target| target.time_us.abs_diff(time) <= SAMPLE_RADIUS_US)
    else {
        return ImageEvidence::Mismatch;
    };
    let best = distance(sample, nearest);
    if best > MAX_IMAGE_ERROR {
        return ImageEvidence::Mismatch;
    }
    // A static or locally repeating shot can look right at the wrong time.
    // Dense observations around the model also supply alternative offsets.
    // Repeated motion can otherwise look unique on a coarse half-second grid.
    // This does not replace the distinctive cut-derived timing model.
    let alternative = [-4_000_000i64, -2_000_000, 2_000_000, 4_000_000]
        .iter()
        .filter_map(|offset| time.checked_add_signed(*offset))
        .map(best_near)
        .fold(f64::INFINITY, f64::min);
    let distinct = if dense.is_some() {
        alternative - best >= IMAGE_MARGIN
    } else {
        // Coarse observations cannot rule out an equally plausible frame
        // between samples. Keep the stricter fallback until it is observed.
        alternative > MAX_IMAGE_ERROR
    };
    if best - alternative > IMAGE_MARGIN {
        ImageEvidence::Mismatch
    } else if distinct {
        ImageEvidence::Distinct
    } else {
        ImageEvidence::Ambiguous
    }
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

struct Run {
    start: u64,
    last_verified: u64,
    last_seen: u64,
    pending_ambiguity: bool,
}

/// An observed extent keeps the original cut fit, even when its endpoints are
/// later widened or rounded. Sampling never creates a new timing model.
struct VerifiedRange<'a> {
    model: &'a Segment,
    start: u64,
    end: u64,
}

impl VerifiedRange<'_> {
    fn segment(&self) -> Option<Segment> {
        Some(Segment {
            source_start_us: self.start,
            source_end_us: self.end,
            target_start_us: project(self.model, self.start)?,
            target_end_us: project(self.model, self.end)?,
            anchors: self.model.anchors,
            max_residual_us: self.model.max_residual_us,
        })
    }
}

fn finish<'a>(seed: &'a Segment, run: Option<Run>, output: &mut Vec<VerifiedRange<'a>>) {
    if let Some(Run {
        start,
        last_verified: end,
        ..
    }) = run
        && end - start >= MIN_RUN_US
    {
        output.push(VerifiedRange {
            model: seed,
            start,
            end,
        });
    }
}

fn same_map(a: &Segment, b: &Segment) -> bool {
    maps_agree(
        a,
        b,
        a.source_start_us.min(b.source_start_us),
        a.source_end_us.max(b.source_end_us),
    )
}

fn maps_agree(a: &Segment, b: &Segment, start: u64, end: u64) -> bool {
    [start, end].iter().all(|&time| {
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
            VerificationSamples {
                source,
                target,
                target_windows: None,
                ends: None,
            },
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
    fn an_alternative_within_the_image_tolerance_keeps_extension_ambiguous() {
        let mut source = frames(0, None);
        for (index, frame) in source.iter_mut().enumerate() {
            for (pixel, value) in frame.pixels.iter_mut().enumerate() {
                *value = if pixel % 2 == 0 { 50 } else { 180 };
            }
            // A moving small object gives local temporal variation, but other
            // offsets still fit the image tolerance. A best match alone cannot
            // distinguish a short deletion hidden inside this kind of motion.
            frame.pixels[index * 4..index * 4 + 2].fill(255);
        }
        let mut target = source.clone();
        for frame in &mut target {
            frame.time_us += 1_000_000;
        }
        let mut result = seed();
        run(&mut result, &source, &target, &[]);
        assert_eq!(result.segments.len(), 1);
        assert_eq!(result.segments[0].source_start_us, 2_000_000);
        assert_eq!(result.segments[0].source_end_us, 4_000_000);
    }

    #[test]
    fn a_single_ambiguous_image_needs_distinct_neighbors_and_cannot_end_a_run() {
        let source = frames(0, None);
        let mut target = frames(1_000_000, None);
        target[10].pixels = source[2].pixels; // Ambiguous source 1s; mismatch at 5s.
        let mut result = seed();
        run(&mut result, &source, &target, &[]);
        assert_eq!(result.segments[0].source_start_us, 0);
        assert_eq!(result.segments[0].source_end_us, 4_500_000);

        // Two adjacent ambiguous images cannot join the isolated prefix to a
        // later run, even though each predicted image individually matches.
        target[9].pixels = source[1].pixels;
        result = seed();
        run(&mut result, &source, &target, &[]);
        assert!(
            result
                .segments
                .iter()
                .all(|s| s.source_start_us >= 1_500_000)
        );

        // An ambiguous final observation cannot become an accepted endpoint.
        let mut tail = frames(1_000_000, None);
        tail[11].pixels = source[19].pixels;
        result = seed();
        run(&mut result, &source, &tail, &[]);
        assert!(result.segments.iter().all(|s| s.source_end_us <= 9_000_000));
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
    fn different_offsets_do_not_extend_into_the_uncertain_gap_or_erase_cut_evidence() {
        let source = frames(0, None);
        let mut target = frames(1_000_000, None);
        for frame in target.iter_mut().skip(10) {
            frame.time_us -= 120_000;
        }
        let mut result = seed();
        result.segments.push(Segment {
            source_start_us: 6_000_000,
            source_end_us: 8_000_000,
            target_start_us: 6_880_000,
            target_end_us: 8_880_000,
            anchors: 3,
            max_residual_us: 0.,
        });
        run(&mut result, &source, &target, &[]);
        // A nearest image within the 300 ms sampling radius can agree with
        // either model. It does not locate this 120 ms edit between cut runs.
        assert!(
            result
                .segments
                .iter()
                .any(|s| s.map_us(2_000_000) == Some(3_000_000.)
                    && s.map_us(4_000_000) == Some(5_000_000.))
        );
        assert!(
            result
                .segments
                .iter()
                .any(|s| s.map_us(6_000_000) == Some(6_880_000.)
                    && s.map_us(8_000_000) == Some(8_880_000.))
        );
        assert!(
            result
                .segments
                .iter()
                .all(|s| s.source_end_us <= 4_000_000 || s.source_start_us >= 6_000_000)
        );
    }
    #[test]
    fn merging_extensions_keeps_a_cut_model_instead_of_blending_endpoint_slopes() {
        let second = Segment {
            source_start_us: 310_600_000,
            source_end_us: 409_080_000,
            target_start_us: 303_053_000,
            target_end_us: 405_739_000,
            anchors: 36,
            max_residual_us: 746.,
        };
        let expected = second.map_us(400_000_000).unwrap();
        let mut result = Alignment {
            segments: vec![
                Segment {
                    source_start_us: 215_560_000,
                    source_end_us: 308_240_000,
                    target_start_us: 203_954_000,
                    target_end_us: 300_592_000,
                    anchors: 16,
                    max_residual_us: 875.,
                },
                second.clone(),
            ],
            distinctive_matches: 52,
            ambiguous_source_anchors: 0,
            reordered: false,
        };
        let mut source = frames(0, None);
        for frame in &mut source {
            frame.time_us += 308_020_000;
        }
        let target: Vec<_> = source
            .iter()
            .map(|frame| Anchor {
                time_us: project(&second, frame.time_us).unwrap(),
                pixels: frame.pixels,
            })
            .collect();
        run(&mut result, &source, &target, &[]);
        // The two cut fits differ by less than 1 ms. Combining the start from
        // one short extension with the end from another invents a third slope,
        // which can conflict with and erase the much longer trusted interval.
        let actual = result
            .segments
            .iter()
            .find_map(|segment| segment.map_us(400_000_000))
            .expect("long cut-supported interval must survive sample refinement");
        assert!((actual - expected).abs() <= 1.);
        assert!(
            result
                .segments
                .iter()
                .any(|s| s.map_us(250_000_000).is_some())
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
    fn requested_target_frames_use_the_model_and_respect_uncertain_gaps() {
        let mut alignment = seed();
        alignment.segments.push(Segment {
            source_start_us: 6_000_000,
            source_end_us: 8_000_000,
            target_start_us: 6_880_000,
            target_end_us: 8_880_000,
            anchors: 3,
            max_residual_us: 0.,
        });
        let times = target_sample_times(
            &alignment,
            &frames(0, None),
            &Cancellation::default(),
            Duration::from_secs(1),
        )
        .unwrap();
        assert_eq!(
            times,
            [
                1_000_000, 1_500_000, 2_000_000, 2_500_000, 9_380_000, 9_880_000, 10_380_000
            ]
        );
        let cancel = Cancellation::default();
        cancel.cancel();
        assert!(matches!(
            target_sample_times(
                &alignment,
                &frames(0, None),
                &cancel,
                Duration::from_secs(1)
            ),
            Err(Error::Cancelled)
        ));
        alignment.segments.clear();
        assert!(
            target_sample_times(
                &alignment,
                &frames(0, None),
                &Cancellation::default(),
                Duration::from_secs(1)
            )
            .unwrap()
            .is_empty()
        );
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
                VerificationSamples {
                    source: &[],
                    target: &[],
                    target_windows: None,
                    ends: None,
                },
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
                VerificationSamples {
                    source: &[],
                    target: &[],
                    target_windows: None,
                    ends: None,
                },
                &Cancellation::default(),
                Duration::from_secs(1)
            )
            .is_err()
        );
    }
}
