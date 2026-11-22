//! The finite candidate set comes from the endpoints of the integer policy
//! intervals. Include both adjacent grid points and the outer edges of any
//! window covering a bound. No time in the interior of a window is admissible.
use super::*;
use crate::quality::{CueMetrics, Profile};
use metrics::ReadingPrior;

#[derive(Clone, Copy, Default)]
struct Violations {
    too_short: u64,
    too_long: u64,
    cps_low: u64,
    cps_high: u64,
    rs_low: u64,
    rs_high: u64,
    cps_unavailable: u64,
    rs_unavailable: u64,
    overlap: u64,
    gap: u64,
    scene_start: u64,
    scene_end: u64,
}
impl Violations {
    fn entries(self) -> [(Rule, u64); 12] {
        // Each unavailable metric counts independently in the local objective.
        [
            (Rule::DurationShort, self.too_short),
            (Rule::DurationLong, self.too_long),
            (Rule::CpsLow, self.cps_low),
            (Rule::CpsHigh, self.cps_high),
            (Rule::RsLow, self.rs_low),
            (Rule::RsHigh, self.rs_high),
            (Rule::SpeedUnavailable, self.cps_unavailable),
            (Rule::SpeedUnavailable, self.rs_unavailable),
            (Rule::Overlap, self.overlap),
            (Rule::Gap, self.gap),
            (Rule::SceneStart, self.scene_start),
            (Rule::SceneEnd, self.scene_end),
        ]
    }
}
#[derive(Clone, Copy)]
struct Bounds {
    actual_min: u64,
    actual_max: u64,
    speed: [Option<u64>; 4],
    cps: bool,
    rs: bool,
}
impl Bounds {
    fn new(metric: &CueMetrics, profile: &Profile) -> Self {
        let duration = &profile.rules.duration;
        let speed_rules = &profile.rules.speed;
        let characters_ms = u64::from(metric.characters) * 1000;
        // Integer division is deliberate: CPS is inclusive, RS excludes equality after its 500 ms allowance.
        let speed = if speed_rules.enabled && !metric.empty {
            [
                speed_rules
                    .minimum_cps
                    .map(|rate| characters_ms / u64::from(rate)),
                speed_rules
                    .maximum_cps
                    .map(|rate| characters_ms.div_ceil(rate.into())),
                speed_rules
                    .minimum_rs
                    .map(|rate| 500 + characters_ms / u64::from(rate)),
                speed_rules
                    .maximum_rs_exclusive
                    .map(|rate| 501 + characters_ms / u64::from(rate)),
            ]
        } else {
            [None; 4]
        };
        Self {
            actual_min: if duration.enabled {
                duration.minimum_ms.into()
            } else {
                1
            },
            actual_max: if duration.enabled {
                duration.maximum_ms.into()
            } else {
                Milliseconds::MAX
            },
            cps: speed[0].is_some() || speed[1].is_some(),
            rs: speed[2].is_some() || speed[3].is_some(),
            speed,
        }
    }
    fn durations(self) -> impl Iterator<Item = u64> {
        [
            Some(self.actual_min),
            Some(self.actual_max),
            self.cps.then_some(1),
            self.rs.then_some(501),
        ]
        .into_iter()
        .chain(self.speed)
        .flatten()
    }
}
#[derive(Clone, Copy)]
struct Trial {
    current: Endpoints,
    next: Option<Endpoints>,
}
#[derive(Clone, Copy)]
struct Score {
    severity: [Violations; 2],
    effective: [u64; 2],
    previous_gap: u64,
}
pub(super) struct Search<'a> {
    pub index: usize,
    pub next: Option<usize>,
    pub following: Option<u64>,
    pub prior: Option<ReadingPrior>,
    pub frontier: u64,
    pub locked: bool,
    pub can_delay: bool,
    pub context: Evaluation<'a>,
    pub request: &'a Request,
    pub before: &'a Report,
    pub points: &'a [Endpoints],
    pub windows: &'a Windows,
    pub scenes: &'a Scenes,
}

fn violations(
    timing: Endpoints,
    effective_ms: u64,
    next: Option<u64>,
    bounds: Bounds,
    profile: &Profile,
    windows: &Windows,
) -> Violations {
    let duration = timing.end_ms - timing.start_ms;
    let [min_cps, max_cps, min_rs, max_rs] = bounds.speed;
    let below_rate = |bound: Option<u64>, allowance| {
        bound
            .filter(|_| effective_ms > allowance)
            .map_or(0, |bound| effective_ms.saturating_sub(bound))
    };
    let above_rate = |bound: Option<u64>, allowance| {
        bound
            .filter(|_| effective_ms > allowance)
            .map_or(0, |bound| bound.saturating_sub(effective_ms))
    };
    let next = next.filter(|_| profile.rules.gaps);
    Violations {
        too_short: bounds.actual_min.saturating_sub(duration),
        too_long: duration.saturating_sub(bounds.actual_max),
        cps_low: below_rate(min_cps, 0),
        cps_high: above_rate(max_cps, 0),
        rs_low: below_rate(min_rs, 500),
        rs_high: above_rate(max_rs, 500),
        cps_unavailable: u64::from(bounds.cps && effective_ms == 0),
        rs_unavailable: u64::from(bounds.rs && effective_ms <= 500),
        overlap: next.map_or(0, |start| timing.end_ms.saturating_sub(start)),
        gap: next
            .filter(|&start| timing.end_ms <= start)
            .map_or(0, |start| {
                (timing.end_ms + u64::from(profile.timing.cue_gap_ms)).saturating_sub(start)
            }),
        scene_start: u64::from(windows.containing(timing.start_ms).is_some()),
        scene_end: u64::from(windows.containing(timing.end_ms).is_some()),
    }
}

impl Search<'_> {
    fn score(&self, trial: Trial, bounds: [Bounds; 2]) -> Score {
        let profile = self.context.profile;
        let current = trial.current;
        let floor = metrics::reading_floor(profile.speed_basis, current.start_ms, self.prior);
        let effective = current.end_ms.saturating_sub(floor);
        let severity = violations(
            current,
            effective,
            trial.next.map(|timing| timing.start_ms),
            bounds[0],
            profile,
            self.windows,
        );
        let mut score = Score {
            severity: [severity, Violations::default()],
            effective: [effective, 0],
            previous_gap: 0,
        };
        if let Some(prior) = self.prior
            && profile.rules.gaps
        {
            score.previous_gap =
                (prior.end + u64::from(profile.timing.cue_gap_ms)).saturating_sub(current.start_ms);
        }
        if let Some(next) = trial.next {
            let prior = ReadingPrior {
                start: floor.min(current.end_ms),
                end: current.end_ms,
                count: self.before.metrics[self.index].characters,
            };
            let next_effective = next.end_ms.saturating_sub(metrics::reading_floor(
                profile.speed_basis,
                next.start_ms,
                Some(prior),
            ));
            score.severity[1] = violations(
                next,
                next_effective,
                self.following,
                bounds[1],
                profile,
                self.windows,
            );
            score.effective[1] = next_effective;
        }
        score
    }
    fn grid(&self, raw: &[u64]) -> Vec<u64> {
        let step = self.context.format.time_step_ms();
        let mut result = Vec::with_capacity(2 * raw.len());
        for &n in raw {
            if n > Milliseconds::MAX {
                continue;
            }
            for v in [n / step * step, n.div_ceil(step) * step] {
                if let Some((l, r)) = self.windows.containing(v) {
                    if l >= 0 {
                        result.push(l as u64);
                    }
                    if r <= Milliseconds::MAX as i64 {
                        result.push(r as u64);
                    }
                } else if v <= Milliseconds::MAX {
                    result.push(v);
                }
            }
        }
        result.sort_unstable();
        result.dedup();
        result
    }
    fn rank(&self, base: Score, score: Score) -> Option<([i16; 4], u64, bool)> {
        let families = self.request.families;
        let mut fixed = [0; 4];
        let mut reduction = 0;
        let mut trade = false;
        if score.previous_gap > base.previous_gap {
            return None;
        }
        if families.gaps && base.previous_gap > 0 && score.previous_gap == 0 {
            fixed[2] += 1;
        }
        for (original, candidate) in base.severity.iter().zip(score.severity) {
            for ((rule, before), (_, after)) in
                original.entries().into_iter().zip(candidate.entries())
            {
                if after > before {
                    if matches!(rule, Rule::CpsHigh | Rule::RsHigh)
                        && self.request.options.priority == Priority::Spacing
                        && families.speed
                        && (families.gaps || families.shots)
                    {
                        trade = true;
                    } else {
                        return None;
                    }
                }
                if !family(rule, families) {
                    continue;
                }
                let group = match rule {
                    Rule::DurationShort | Rule::DurationLong => 0,
                    Rule::Gap | Rule::Overlap => 2,
                    Rule::SceneStart | Rule::SceneEnd => 3,
                    _ => 1,
                };
                fixed[group] +=
                    i16::from(before > 0 && after == 0) - i16::from(before == 0 && after > 0);
                if matches!(
                    rule,
                    Rule::DurationShort
                        | Rule::DurationLong
                        | Rule::CpsHigh
                        | Rule::CpsLow
                        | Rule::RsHigh
                        | Rule::RsLow
                ) {
                    reduction += before.saturating_sub(after);
                }
            }
        }
        if trade && fixed[2] <= 0 && fixed[3] <= 0 {
            return None;
        }
        if self.request.options.priority == Priority::Spacing {
            fixed = [fixed[0], fixed[2], fixed[3], fixed[1]];
        }
        Some((fixed, reduction, trade))
    }
    pub(super) fn choose(&self) -> Option<(Unit, u64)> {
        let profile = self.context.profile;
        let index = self.index;
        let original = self.points[index];
        let next = self.next.map(|next_index| self.points[next_index]);
        let bounds = Bounds::new(&self.before.metrics[index], profile);
        let next_bounds = self.next.map_or(bounds, |next_index| {
            Bounds::new(&self.before.metrics[next_index], profile)
        });
        let base = self.score(
            Trial {
                current: original,
                next,
            },
            [bounds, next_bounds],
        );
        let tolerance = u64::from(self.request.options.start_tolerance_ms);
        let gap = u64::from(profile.timing.cue_gap_ms);
        let start_window = self.windows.containing(original.start_ms);
        let end_window = self.windows.containing(original.end_ms);
        let inward_start = start_window
            .filter(|_| self.request.families.shots)
            .map(|w| w.1 as u64);
        let earliest_start = if self.prior.is_some() {
            self.frontier + gap
        } else {
            0
        };
        let mut starts = vec![original.start_ms];
        if !self.locked && (tolerance > 0 || inward_start.is_some()) {
            let mut raw = vec![
                original.start_ms,
                original.start_ms.saturating_sub(tolerance),
                original.start_ms + tolerance,
                earliest_start,
            ];
            // Intersect candidate ends with each duration boundary to find the least start movement.
            let mut end_limits = vec![
                Some(original.end_ms),
                next.and_then(|neighbour| neighbour.start_ms.checked_sub(gap)),
                next.and_then(|neighbour| (neighbour.start_ms + tolerance).checked_sub(gap)),
            ];
            if let Some(neighbour) = next
                && self.can_delay
            {
                // Project the following cue’s duration through the required gap.
                end_limits.extend(
                    next_bounds
                        .durations()
                        .map(|duration| neighbour.end_ms.checked_sub(duration + gap)),
                );
            }
            for end in end_limits.into_iter().flatten() {
                raw.extend(
                    bounds
                        .durations()
                        .filter_map(|duration| end.checked_sub(duration)),
                );
            }
            starts.extend(self.grid(&raw));
        }
        starts.sort_unstable();
        starts.dedup();
        let mut best = None;
        let mut best_key = None;
        for start_ms in starts {
            if start_ms != original.start_ms
                && (self.windows.containing(start_ms).is_some()
                    || (start_ms.abs_diff(original.start_ms) > tolerance
                        && Some(start_ms) != inward_start)
                    || (start_ms < original.start_ms && start_ms < earliest_start)
                    || (start_window.is_some()
                        && self.request.options.scene_edges == SceneEdges::Inward
                        && start_ms < original.start_ms))
            {
                continue;
            }
            let floor = metrics::reading_floor(profile.speed_basis, start_ms, self.prior);
            let mut raw = vec![
                original.end_ms,
                start_ms + self.context.format.time_step_ms(),
                start_ms + bounds.actual_min,
                start_ms + bounds.actual_max,
            ];
            raw.extend(
                bounds
                    .speed
                    .into_iter()
                    .flatten()
                    .map(|duration| floor + duration),
            );
            if bounds.cps {
                raw.push(floor + 1);
            }
            if bounds.rs {
                raw.push(floor + 501);
            }
            if let Some(neighbour) = next {
                raw.push(neighbour.start_ms.saturating_sub(gap));
                if profile.speed_basis == SpeedBasis::VssOverlapV1
                    && original.end_ms > neighbour.start_ms
                {
                    raw.push(neighbour.start_ms);
                    raw.extend(
                        next_bounds
                            .durations()
                            .filter_map(|duration| neighbour.end_ms.checked_sub(duration)),
                    );
                }
                if self.can_delay && tolerance > 0 {
                    raw.push((neighbour.start_ms + tolerance).saturating_sub(gap));
                    raw.extend(
                        next_bounds
                            .durations()
                            .filter_map(|duration| neighbour.end_ms.checked_sub(duration + gap)),
                    );
                }
            }
            let mut ends = self.grid(&raw);
            if !ends.contains(&original.end_ms) {
                ends.push(original.end_ms);
            }
            for end_ms in ends {
                if end_ms <= start_ms
                    || end_ms != original.end_ms
                        && (self.windows.containing(end_ms).is_some()
                            || end_window.is_some()
                                && self.request.families.shots
                                && self.request.options.scene_edges == SceneEdges::Inward
                                && end_ms > original.end_ms)
                {
                    continue;
                }
                let current = Endpoints { start_ms, end_ms };
                let mut delays = vec![next.map_or(0, |neighbour| neighbour.start_ms)];
                if let Some(neighbour) = next
                    && self.can_delay
                    && tolerance > 0
                {
                    delays.extend(self.grid(&[end_ms + gap, neighbour.start_ms + tolerance]));
                    delays.sort_unstable();
                    delays.dedup();
                }
                for start in delays {
                    if let Some(neighbour) = next
                        && (start < neighbour.start_ms
                            || start - neighbour.start_ms > tolerance
                            || start >= neighbour.end_ms
                            || start != neighbour.start_ms
                                && (self.windows.containing(start).is_some()
                                    || self.following.is_some_and(|f| start >= f))
                            || start_ms >= start
                            || (end_ms > original.end_ms && end_ms + gap > start)
                            || (end_ms as i64 - start as i64)
                                > (original.end_ms as i64 - neighbour.start_ms as i64)
                                    .max(-(gap as i64)))
                    {
                        continue;
                    }
                    let trial = Trial {
                        current,
                        next: next.map(|neighbour| Endpoints {
                            start_ms: start,
                            ..neighbour
                        }),
                    };
                    let score = self.score(trial, [bounds, next_bounds]);
                    let Some((fixed, reduction, trade)) = self.rank(base, score) else {
                        continue;
                    };
                    let partial = fixed == [0; 4];
                    if fixed < [0; 4] || partial && reduction == 0 {
                        continue;
                    }
                    let start_move = start_ms.abs_diff(original.start_ms)
                        + next.map_or(0, |neighbour| start - neighbour.start_ms);
                    let key = (
                        fixed,
                        if partial { reduction } else { 0 },
                        std::cmp::Reverse((
                            start_move,
                            end_ms.abs_diff(original.end_ms),
                            end_ms - start_ms,
                            start_ms,
                            end_ms,
                            start,
                        )),
                    );
                    if best_key.is_none_or(|old| key > old) {
                        best_key = Some(key);
                        best = Some((trial, score, partial, trade));
                    }
                }
            }
        }
        let (trial, score, partial, trade_off) = best?;
        let mut endpoints = vec![];
        let mut fixed = vec![];
        let mut effects = vec![];
        if self.request.families.gaps && base.previous_gap > 0 && score.previous_gap == 0 {
            fixed.push(Rule::Gap);
        }
        for (neighbour, (cue_index, from, to)) in [
            (Some(index), Some(original), Some(trial.current)),
            (self.next, next, trial.next),
        ]
        .into_iter()
        .enumerate()
        {
            let (Some(cue_index), Some(from), Some(to)) = (cue_index, from, to) else {
                continue;
            };
            if from != to {
                endpoints.push(Change {
                    index: cue_index as u32,
                    before: from,
                    after: to,
                });
            }
            for ((rule, initial), (_, planned)) in base.severity[neighbour]
                .entries()
                .into_iter()
                .zip(score.severity[neighbour].entries())
            {
                if initial > 0
                    && planned == 0
                    && family(rule, self.request.families)
                    && !fixed.contains(&rule)
                {
                    fixed.push(rule);
                }
            }
            let metric = &self.before.metrics[cue_index];
            let speed = &profile.rules.speed;
            for (allowance, limit, rule) in [
                (0, speed.maximum_cps, Rule::CpsHigh),
                (500, speed.maximum_rs_exclusive, Rule::RsHigh),
            ] {
                if speed.enabled
                    && !metric.empty
                    && let Some(limit) = limit
                    && base.effective[neighbour] != score.effective[neighbour]
                {
                    let rate = |duration: u64| {
                        duration
                            .checked_sub(allowance)
                            .filter(|duration| *duration > 0)
                            .map(|duration| f64::from(metric.characters) * 1000. / duration as f64)
                    };
                    effects.push(Effect {
                        index: cue_index as u32,
                        rule,
                        before: rate(base.effective[neighbour]),
                        after: rate(score.effective[neighbour]),
                        limit,
                    });
                }
            }
        }
        let mut effects_text = Vec::new();
        let mut cut_timecodes = Vec::new();
        if fixed
            .iter()
            .any(|rule| matches!(rule, Rule::Gap | Rule::Overlap))
        {
            let pair = if base.previous_gap > score.previous_gap {
                self.prior.map(|prior| {
                    (
                        original.start_ms as i64 - prior.end as i64,
                        trial.current.start_ms as i64 - prior.end as i64,
                    )
                })
            } else {
                next.zip(trial.next).map(|(old, new)| {
                    (
                        old.start_ms as i64 - original.end_ms as i64,
                        new.start_ms as i64 - trial.current.end_ms as i64,
                    )
                })
            };
            if let Some((before, after)) = pair {
                effects_text.push(effects::spacing(before, after));
            }
        }
        for change in &endpoints {
            for (rule, label, before, after) in [
                (
                    Rule::SceneStart,
                    "starts",
                    change.before.start_ms,
                    change.after.start_ms,
                ),
                (
                    Rule::SceneEnd,
                    "ends",
                    change.before.end_ms,
                    change.after.end_ms,
                ),
            ] {
                if fixed.contains(&rule)
                    && before != after
                    && self.windows.containing(before).is_some()
                    && let Some((segment, timecode)) =
                        effects::shot(label, before, after, self.scenes, profile)
                {
                    effects_text.push(segment);
                    if !cut_timecodes.contains(&timecode) {
                        cut_timecodes.push(timecode);
                    }
                }
            }
            if fixed
                .iter()
                .any(|rule| matches!(rule, Rule::DurationShort | Rule::DurationLong))
            {
                let before = change.before.end_ms - change.before.start_ms;
                let after = change.after.end_ms - change.after.start_ms;
                if before != after {
                    effects_text.push(EffectSegment::plain(format!(
                        "duration {before} → {after} ms"
                    )));
                }
            }
        }
        effects_text.extend(
            effects
                .iter()
                .filter_map(|effect| effects::speed(effect, index as u32)),
        );
        Some((
            Unit {
                depends_on: Vec::new(),
                effects_text,
                cut_timecodes,
                index: index as u32,
                endpoints,
                partial,
                trade_off,
                fixed,
                effects,
            },
            score.effective[0],
        ))
    }
}
