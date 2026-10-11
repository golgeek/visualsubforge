//! Bounded, pure scene-correspondence experiment. No IO or subtitle mutation.
//!
//! Mutual distinctive appearance matches are anchors, not sufficient by
//! themselves to infer a continuous map. Runs must preserve scene order and
//! agree on local timing. Unmatched scene boundaries always break a run.
pub mod verify;
use crate::{
    Cancellation, Error, Result,
    scenes::visual::{Anchor, MAX_ANCHORS, distance, has_detail},
};
use serde::Serialize;
use std::time::{Duration, Instant};

const MAX_COMPARISONS: usize = 25_000_000;
const MAX_ERROR: f64 = 12.;
const MIN_MARGIN: f64 = 6.;
const TIMING_TOLERANCE_US: f64 = 50_000.;

#[derive(Debug, Clone, Serialize)]
pub struct Segment {
    pub source_start_us: u64,
    pub source_end_us: u64,
    pub target_start_us: u64,
    pub target_end_us: u64,
    pub anchors: usize,
    pub max_residual_us: f64,
}

impl Segment {
    /// Used for measuring the experiment; the application will own final cue
    /// rounding and range validation. Never extrapolates outside this segment.
    pub fn map_us(&self, source: u64) -> Option<f64> {
        if source < self.source_start_us
            || source > self.source_end_us
            || self.source_end_us <= self.source_start_us
            || self.target_end_us <= self.target_start_us
        {
            return None;
        }
        Some(
            self.target_start_us as f64
                + (source - self.source_start_us) as f64
                    * (self.target_end_us - self.target_start_us) as f64
                    / (self.source_end_us - self.source_start_us) as f64,
        )
    }
}

#[derive(Debug, Serialize)]
pub struct Alignment {
    pub segments: Vec<Segment>,
    pub distinctive_matches: usize,
    pub ambiguous_source_anchors: usize,
    pub reordered: bool,
}

#[derive(Clone, Copy)]
struct Nearest {
    index: usize,
    best: f64,
    second: f64,
}
impl Nearest {
    fn empty() -> Self {
        Self {
            index: 0,
            best: f64::INFINITY,
            second: f64::INFINITY,
        }
    }
    fn consider(&mut self, index: usize, distance: f64) {
        if distance < self.best {
            self.second = self.best;
            self.best = distance;
            self.index = index;
        } else if distance < self.second {
            self.second = distance;
        }
    }
    fn distinctive(&self) -> bool {
        self.best <= MAX_ERROR && self.second - self.best >= MIN_MARGIN
    }
}

pub fn align(
    source: &[Anchor],
    target: &[Anchor],
    cancellation: &Cancellation,
    timeout: Duration,
) -> Result<Alignment> {
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
    for anchors in [source, target] {
        if anchors.len() > MAX_ANCHORS
            || anchors.windows(2).any(|p| p[0].time_us >= p[1].time_us)
            || anchors.iter().any(|a| a.time_us > 14_400_000_000)
        {
            return Err(Error::Message(
                "Invalid or excessive alignment anchors".into(),
            ));
        }
    }
    if source.len().saturating_mul(target.len()) > MAX_COMPARISONS {
        return Err(Error::Message(
            "Alignment exceeds 25 million comparisons".into(),
        ));
    }
    let mut forward = vec![Nearest::empty(); source.len()];
    let mut reverse = vec![Nearest::empty(); target.len()];
    let target_detail: Vec<_> = target.iter().map(has_detail).collect();
    for (i, a) in source.iter().enumerate() {
        check()?;
        if !has_detail(a) {
            continue;
        }
        for (j, b) in target.iter().enumerate() {
            if j % 256 == 0 {
                check()?;
            }
            if !target_detail[j] {
                continue;
            }
            let error = distance(a, b);
            forward[i].consider(j, error);
            reverse[j].consider(i, error);
        }
    }
    let matches: Vec<_> = forward
        .iter()
        .enumerate()
        .filter_map(|(i, near)| {
            (near.distinctive()
                && reverse[near.index].distinctive()
                && reverse[near.index].index == i)
                .then_some((i, near.index))
        })
        .collect();
    let mut result = Alignment {
        segments: vec![],
        distinctive_matches: matches.len(),
        ambiguous_source_anchors: forward
            .iter()
            .filter(|n| n.best <= MAX_ERROR && !n.distinctive())
            .count(),
        reordered: matches.windows(2).any(|p| p[0].1 >= p[1].1),
    };
    // Do not hide contradictory content order by selecting a convenient subset.
    if result.reordered {
        return Ok(result);
    }
    let mut first = 0;
    let mut fit_checks = 0;
    while first < matches.len() {
        check()?;
        let mut end = first + 1;
        while end < matches.len()
            && matches[end].0 == matches[end - 1].0 + 1
            && matches[end].1 == matches[end - 1].1 + 1
        {
            end += 1;
        }
        fit_runs(
            &matches[first..end],
            source,
            target,
            &mut result.segments,
            &check,
            &mut fit_checks,
        )?;
        first = end;
    }
    check()?;
    Ok(result)
}

fn fit_runs(
    matches: &[(usize, usize)],
    source: &[Anchor],
    target: &[Anchor],
    segments: &mut Vec<Segment>,
    check: &impl Fn() -> Result<()>,
    fit_checks: &mut usize,
) -> Result<()> {
    let mut first = 0;
    while first + 2 < matches.len() {
        let mut end = first + 2;
        let mut accepted = None;
        while end < matches.len() {
            check()?;
            let (a, b) = (matches[first], matches[end]);
            let mut segment = Segment {
                source_start_us: source[a.0].time_us,
                source_end_us: source[b.0].time_us,
                target_start_us: target[a.1].time_us,
                target_end_us: target[b.1].time_us,
                anchors: end - first + 1,
                max_residual_us: 0.,
            };
            let scale = (segment.target_end_us - segment.target_start_us) as f64
                / (segment.source_end_us - segment.source_start_us) as f64;
            for &(i, j) in &matches[first + 1..end] {
                *fit_checks += 1;
                if *fit_checks > MAX_COMPARISONS {
                    return Err(Error::Message(
                        "Alignment exceeds 25 million fit checks".into(),
                    ));
                }
                if (*fit_checks).is_multiple_of(256) {
                    check()?;
                }
                let prediction = segment.map_us(source[i].time_us).unwrap_or(f64::INFINITY);
                segment.max_residual_us = segment
                    .max_residual_us
                    .max((prediction - target[j].time_us as f64).abs());
            }
            if !(0.8..=1.25).contains(&scale) || segment.max_residual_us > TIMING_TOLERANCE_US {
                break;
            }
            accepted = Some(segment);
            end += 1;
        }
        if let Some(mut segment) = accepted {
            extend_last_shot(&mut segment, matches[end - 1], source, target);
            segments.push(segment);
            first = end - 1;
        } else {
            first += 1;
        }
    }
    Ok(())
}

fn extend_last_shot(
    segment: &mut Segment,
    last: (usize, usize),
    source: &[Anchor],
    target: &[Anchor],
) {
    // The last matched image starts a shot, it does not end the retained content.
    // Extend only to the next independently observed cut in BOTH videos, when
    // that outgoing shot's duration agrees with the established scale to 1 ms.
    // This is not extrapolation past an unknown tail or across an unmatched shot.
    let (Some(source_end), Some(target_end)) = (source.get(last.0 + 1), target.get(last.1 + 1))
    else {
        return;
    };
    let scale = (segment.target_end_us - segment.target_start_us) as f64
        / (segment.source_end_us - segment.source_start_us) as f64;
    let predicted =
        segment.target_end_us as f64 + (source_end.time_us - segment.source_end_us) as f64 * scale;
    let adjustment = (predicted - target_end.time_us as f64).abs();
    if adjustment <= 1_000. && segment.max_residual_us + adjustment <= TIMING_TOLERANCE_US {
        // Changing the fitted endpoint can move prior predictions by at most
        // this amount. Retain a conservative upper bound for diagnostics.
        segment.max_residual_us += adjustment;
        segment.source_end_us = source_end.time_us;
        segment.target_end_us = target_end.time_us;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anchors(times: &[u64], ids: &[u32]) -> Vec<Anchor> {
        times
            .iter()
            .zip(ids)
            .map(|(&time_us, &id)| {
                let mut state = id + 1;
                let mut pixels = [0; 256];
                for p in &mut pixels {
                    state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                    *p = (state >> 24) as u8;
                }
                Anchor { time_us, pixels }
            })
            .collect()
    }
    fn run(a: &[Anchor], b: &[Anchor]) -> Alignment {
        align(a, b, &Cancellation::default(), Duration::from_secs(2)).unwrap()
    }
    #[test]
    fn offset_and_speed_come_from_content_timestamps() {
        let a = anchors(&[0, 1_000_000, 2_000_000, 3_000_000], &[1, 2, 3, 4]);
        let b = anchors(&[5_000_000, 5_960_000, 6_920_000, 7_880_000], &[1, 2, 3, 4]);
        let result = run(&a, &b);
        assert_eq!(result.segments.len(), 1);
        assert_eq!(result.segments[0].map_us(1_250_000), Some(6_200_000.));
        assert_eq!(result.segments[0].map_us(3_000_001), None);
    }
    #[test]
    fn gaps_never_become_interpolation_spans() {
        let times = [
            0, 1_000_000, 2_000_000, 3_000_000, 4_000_000, 5_000_000, 6_000_000,
        ];
        let a = anchors(&times, &[1, 2, 3, 4, 5, 6, 7]);
        let b = anchors(&times, &[1, 2, 3, 99, 5, 6, 7]);
        let result = run(&a, &b);
        assert_eq!(result.segments.len(), 2);
        assert_eq!(result.segments[0].source_end_us, 3_000_000);
        assert!(
            result
                .segments
                .iter()
                .all(|s| s.map_us(3_500_000).is_none())
        );
    }
    #[test]
    fn repeated_unrelated_and_reordered_content_cannot_look_complete() {
        let a = anchors(&[0, 1, 2, 3], &[1, 2, 3, 4]);
        let unrelated = anchors(&[0, 1, 2, 3], &[40, 41, 42, 43]);
        assert!(run(&a, &unrelated).segments.is_empty());
        let repeated = anchors(&[0, 1, 2, 3], &[1, 1, 1, 1]);
        assert!(run(&repeated, &repeated).segments.is_empty());
        let reordered = anchors(&[0, 1, 2, 3], &[4, 1, 2, 3]);
        assert!(run(&a, &reordered).reordered);
        assert!(run(&a, &reordered).segments.is_empty());
    }
    #[test]
    fn cancellation_limits_and_bad_timestamp_inputs_fail() {
        let a = anchors(&[0, 1, 2], &[1, 2, 3]);
        let cancellation = Cancellation::default();
        cancellation.cancel();
        assert!(matches!(
            align(&a, &a, &cancellation, Duration::from_secs(1)),
            Err(Error::Cancelled)
        ));
        assert!(matches!(
            align(&a, &a, &Cancellation::default(), Duration::ZERO),
            Err(Error::Timeout)
        ));
        let bad = anchors(&[1, 1, 2], &[1, 2, 3]);
        assert!(align(&bad, &a, &Cancellation::default(), Duration::from_secs(1)).is_err());
        let excessive: Vec<_> = (0..5_001)
            .map(|time_us| Anchor {
                time_us,
                pixels: [0; 256],
            })
            .collect();
        assert!(
            align(
                &excessive,
                &excessive,
                &Cancellation::default(),
                Duration::from_secs(1)
            )
            .unwrap_err()
            .to_string()
            .contains("25 million")
        );
    }
    #[test]
    fn timing_discontinuity_splits_runs_even_without_an_extra_scene() {
        let a = anchors(
            &[0, 1_000_000, 2_000_000, 3_000_000, 4_000_000, 5_000_000],
            &[1, 2, 3, 4, 5, 6],
        );
        let b = anchors(
            &[0, 1_000_000, 2_000_000, 5_000_000, 6_000_000, 7_000_000],
            &[1, 2, 3, 4, 5, 6],
        );
        let result = run(&a, &b);
        assert_eq!(result.segments.len(), 2);
        assert_eq!(result.segments[0].source_end_us, 2_000_000);
        assert_eq!(result.segments[1].source_start_us, 3_000_000);
        assert!(
            result
                .segments
                .iter()
                .all(|s| s.map_us(2_500_000).is_none())
        );
    }
}
