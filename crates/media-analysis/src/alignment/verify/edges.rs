//! Video-edge observations can bound an otherwise locally repeating shot.
//! They never locate an edit: disagreeing edge times leave the interval unknown.
use super::{
    Anchor, ImageEvidence, MAX_IMAGE_ERROR, MAX_SAMPLE_GAP_US, MIN_RUN_US, Result, Segment,
    VerificationSamples, VerifiedRange, cuts_agree, distance, has_detail, image_evidence, project,
    temporally_distinct,
};

pub(super) fn verify<'a>(
    seeds: &'a [Segment],
    source_cuts: &[Anchor],
    target_cuts: &[Anchor],
    samples: VerificationSamples<'_>,
    check: &impl Fn() -> Result<()>,
) -> Result<Vec<VerifiedRange<'a>>> {
    let (Some(ends), Some(source_first), Some(target_first), Some(first), Some(last)) = (
        samples.ends,
        samples.source.first(),
        samples.target.first(),
        seeds.first(),
        seeds.last(),
    ) else {
        return Ok(Vec::new());
    };
    let mut output = Vec::new();
    for (range, source_edge, target_edge) in [
        (
            VerifiedRange {
                model: first,
                start: source_first.time_us,
                end: first.source_start_us,
            },
            source_first,
            target_first,
        ),
        (
            VerifiedRange {
                model: last,
                start: last.source_end_us,
                end: ends.source.time_us,
            },
            ends.source,
            ends.target,
        ),
    ] {
        check()?;
        if range.end.saturating_sub(range.start) < MIN_RUN_US
            || range.model.anchors < 3
            || project(range.model, source_edge.time_us)
                .is_none_or(|time| time.abs_diff(target_edge.time_us) > 1_000)
            || !has_detail(source_edge)
            || distance(source_edge, target_edge) > MAX_IMAGE_ERROR
        {
            continue;
        }
        let mut previous = range.start;
        let mut matches = true;
        let begin = samples.source.partition_point(|s| s.time_us < range.start);
        for (index, frame) in samples
            .source
            .iter()
            .enumerate()
            .skip(begin)
            .take_while(|(_, frame)| frame.time_us <= range.end)
        {
            check()?;
            if frame.time_us - previous > MAX_SAMPLE_GAP_US
                || !temporally_distinct(samples.source, index)
                || !cuts_agree(
                    range.model,
                    previous,
                    frame.time_us,
                    source_cuts,
                    target_cuts,
                )
                || project(range.model, frame.time_us).is_none_or(|time| {
                    image_evidence(frame, samples, time) == ImageEvidence::Mismatch
                })
            {
                matches = false;
                break;
            }
            previous = frame.time_us;
        }
        // Both the complete observed interval and its final fraction need
        // evidence. Matching edge pictures alone do not prove continuity.
        if matches
            && range.end - previous <= MAX_SAMPLE_GAP_US
            && cuts_agree(range.model, previous, range.end, source_cuts, target_cuts)
            && project(range.model, source_edge.time_us).is_some_and(|time| {
                image_evidence(source_edge, samples, time) != ImageEvidence::Mismatch
            })
        {
            output.push(range);
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Cancellation, alignment::Alignment, alignment::verify::VideoEnds};
    use std::time::Duration;

    fn periodic_video(offset: u64) -> Vec<Anchor> {
        (0..25)
            .map(|index| Anchor {
                time_us: offset + index * 500_000,
                pixels: std::array::from_fn(|pixel| {
                    if pixel as u64 % 4 == index % 4 {
                        200
                    } else {
                        50
                    }
                }),
            })
            .collect()
    }

    fn refine_edges(
        source: &[Anchor],
        target: &[Anchor],
        ends: Option<VideoEnds<'_>>,
    ) -> Alignment {
        let mut alignment = Alignment {
            segments: vec![Segment {
                source_start_us: 2_000_000,
                source_end_us: 8_000_000,
                target_start_us: 3_000_000,
                target_end_us: 9_000_000,
                anchors: 4,
                max_residual_us: 0.,
            }],
            distinctive_matches: 4,
            ambiguous_source_anchors: 0,
            reordered: false,
        };
        super::super::refine(
            &mut alignment,
            &[],
            &[],
            VerificationSamples {
                source,
                target,
                target_windows: None,
                ends,
            },
            &Cancellation::default(),
            Duration::from_secs(1),
        )
        .unwrap();
        alignment
    }

    #[test]
    fn observed_edges_bound_repeating_motion_but_missing_edges_do_not() {
        let source = periodic_video(0);
        let target = periodic_video(1_000_000);
        let unknown = refine_edges(&source, &target, None);
        assert_eq!(unknown.segments.len(), 1);
        assert_eq!(unknown.segments[0].source_start_us, 2_000_000);
        assert_eq!(unknown.segments[0].source_end_us, 8_000_000);
        let bounded = refine_edges(
            &source,
            &target,
            Some(VideoEnds {
                source: source.last().unwrap(),
                target: target.last().unwrap(),
            }),
        );
        assert_eq!(bounded.segments.len(), 1);
        assert_eq!(bounded.segments[0].map_us(0), Some(1_000_000.));
        assert_eq!(bounded.segments[0].map_us(12_000_000), Some(13_000_000.));
        assert_eq!(bounded.segments[0].map_us(12_000_001), None);
    }

    #[test]
    fn matching_final_image_cannot_hide_a_whole_cycle_duration_change() {
        let source = periodic_video(0);
        let target = periodic_video(1_000_000);
        let mut later_end = target.last().unwrap().clone();
        later_end.time_us += 2_000_000;
        let result = refine_edges(
            &source,
            &target,
            Some(VideoEnds {
                source: source.last().unwrap(),
                target: &later_end,
            }),
        );
        assert_eq!(result.segments[0].source_start_us, 0);
        assert_eq!(result.segments[0].source_end_us, 8_000_000);
    }

    #[test]
    fn matching_edges_cannot_bridge_wrong_images_missing_samples_or_static_content() {
        let source = periodic_video(0);
        let target = periodic_video(1_000_000);
        let mut wrong = source.clone();
        wrong[1].pixels.reverse();
        let result = refine_edges(
            &wrong,
            &target,
            Some(VideoEnds {
                source: wrong.last().unwrap(),
                target: target.last().unwrap(),
            }),
        );
        assert_eq!(result.segments[0].source_start_us, 2_000_000);

        let mut missing = source.clone();
        missing.remove(20);
        let result = refine_edges(
            &missing,
            &target,
            Some(VideoEnds {
                source: missing.last().unwrap(),
                target: target.last().unwrap(),
            }),
        );
        assert_eq!(result.segments[0].source_end_us, 8_000_000);

        let mut still_source = source.clone();
        let mut still_target = target.clone();
        for frame in still_source.iter_mut().chain(still_target.iter_mut()) {
            frame.pixels = source[0].pixels;
        }
        let result = refine_edges(
            &still_source,
            &still_target,
            Some(VideoEnds {
                source: still_source.last().unwrap(),
                target: still_target.last().unwrap(),
            }),
        );
        assert_eq!(result.segments[0].source_start_us, 2_000_000);
        assert_eq!(result.segments[0].source_end_us, 8_000_000);
    }
}
