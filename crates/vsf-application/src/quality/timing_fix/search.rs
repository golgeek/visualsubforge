//! The finite candidate set comes from the endpoints of the integer policy
//! intervals. Include both adjacent grid points and the outer edges of any
//! window covering a bound. No time in the interior of a window is admissible.
use super::*;
use crate::quality::{CueMetrics, Profile};
use metrics::ReadingPrior;

const RULES: [Rule; 12] = [
    Rule::DurationShort,
    Rule::DurationLong,
    Rule::CpsLow,
    Rule::CpsHigh,
    Rule::RsLow,
    Rule::RsHigh,
    Rule::SpeedUnavailable,
    Rule::SpeedUnavailable,
    Rule::Overlap,
    Rule::Gap,
    Rule::SceneStart,
    Rule::SceneEnd,
];
type Violations = [u64; 12];
#[derive(Clone, Copy)]
struct Bounds {
    actual_min: u64,
    actual_max: u64,
    speed: [Option<u64>; 4],
    cps: bool,
    rs: bool,
}
impl Bounds {
    fn new(m: &CueMetrics, p: &Profile) -> Self {
        let d = &p.rules.duration;
        let s = &p.rules.speed;
        let a = u64::from(m.characters) * 1000;
        let speed = if s.enabled && !m.empty {
            [
                s.minimum_cps.map(|x| a / u64::from(x)),
                s.maximum_cps.map(|x| a.div_ceil(x.into())),
                s.minimum_rs.map(|x| 500 + a / u64::from(x)),
                s.maximum_rs_exclusive.map(|x| 501 + a / u64::from(x)),
            ]
        } else {
            [None; 4]
        };
        Self {
            actual_min: if d.enabled { d.minimum_ms.into() } else { 1 },
            actual_max: if d.enabled {
                d.maximum_ms.into()
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
    p: Endpoints,
    d: u64,
    next: Option<u64>,
    bounds: Bounds,
    profile: &Profile,
    windows: &Windows,
) -> Violations {
    let duration = p.end_ms - p.start_ms;
    let mut v = [0; 12];
    v[0] = bounds.actual_min.saturating_sub(duration);
    v[1] = duration.saturating_sub(bounds.actual_max);
    for (i, b) in bounds.speed.iter().enumerate() {
        if let Some(b) = b
            && d > if i < 2 { 0 } else { 500 }
        {
            v[2 + i] = if i % 2 == 0 {
                d.saturating_sub(*b)
            } else {
                b.saturating_sub(d)
            };
        }
    }
    v[6] = u64::from(bounds.cps && d == 0);
    v[7] = u64::from(bounds.rs && d <= 500);
    if profile.rules.gaps
        && let Some(next) = next
    {
        v[8] = p.end_ms.saturating_sub(next);
        if p.end_ms <= next {
            v[9] = (p.end_ms + u64::from(profile.timing.cue_gap_ms)).saturating_sub(next);
        }
    }
    v[10] = u64::from(windows.containing(p.start_ms).is_some());
    v[11] = u64::from(windows.containing(p.end_ms).is_some());
    v
}

impl Search<'_> {
    fn score(&self, t: Trial, bounds: [Bounds; 2]) -> Score {
        let profile = self.context.profile;
        let cur = t.current;
        let floor = metrics::reading_floor(profile.speed_basis, cur.start_ms, self.prior);
        let effective = cur.end_ms.saturating_sub(floor);
        let a = violations(
            cur,
            effective,
            t.next.map(|n| n.start_ms),
            bounds[0],
            profile,
            self.windows,
        );
        let mut score = Score {
            severity: [a, [0; 12]],
            effective: [effective, 0],
            previous_gap: 0,
        };
        if let Some(prior) = self.prior
            && profile.rules.gaps
        {
            score.previous_gap =
                (prior.end + u64::from(profile.timing.cue_gap_ms)).saturating_sub(cur.start_ms);
        }
        if let Some(next) = t.next {
            let prior = ReadingPrior {
                start: floor.min(cur.end_ms),
                end: cur.end_ms,
                count: self.before.metrics[self.index].characters,
            };
            let d = next.end_ms.saturating_sub(metrics::reading_floor(
                profile.speed_basis,
                next.start_ms,
                Some(prior),
            ));
            score.severity[1] =
                violations(next, d, self.following, bounds[1], profile, self.windows);
            score.effective[1] = d;
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
        let f = self.request.families;
        let mut fixed = [0; 4];
        let mut reduction = 0;
        let mut trade = false;
        if score.previous_gap > base.previous_gap {
            return None;
        }
        if f.gaps && base.previous_gap > 0 && score.previous_gap == 0 {
            fixed[2] += 1;
        }
        for (b, s) in base.severity.iter().zip(score.severity) {
            for k in 0..12 {
                if s[k] > b[k] {
                    if matches!(k, 3 | 5)
                        && self.request.options.priority == Priority::Spacing
                        && f.speed
                        && (f.gaps || f.shots)
                    {
                        trade = true;
                    } else {
                        return None;
                    }
                }
                if !family(RULES[k], f) {
                    continue;
                }
                let group = match k {
                    0..2 => 0,
                    2..8 => 1,
                    8..10 => 2,
                    _ => 3,
                };
                fixed[group] += i16::from(b[k] > 0 && s[k] == 0) - i16::from(b[k] == 0 && s[k] > 0);
                if k < 6 {
                    reduction += b[k].saturating_sub(s[k]);
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
        let p = self.context.profile;
        let i = self.index;
        let orig = self.points[i];
        let next = self.next.map(|j| self.points[j]);
        let b = Bounds::new(&self.before.metrics[i], p);
        let nb = self
            .next
            .map_or(b, |j| Bounds::new(&self.before.metrics[j], p));
        let base = self.score(
            Trial {
                current: orig,
                next,
            },
            [b, nb],
        );
        let tolerance = u64::from(self.request.options.start_tolerance_ms);
        let gap = u64::from(p.timing.cue_gap_ms);
        let start_window = self.windows.containing(orig.start_ms);
        let end_window = self.windows.containing(orig.end_ms);
        let inward_start = start_window
            .filter(|_| self.request.families.shots)
            .map(|w| w.1 as u64);
        let earliest_start = if self.prior.is_some() {
            self.frontier + gap
        } else {
            0
        };
        let mut starts = vec![orig.start_ms];
        if !self.locked && (tolerance > 0 || inward_start.is_some()) {
            let mut raw = vec![
                orig.start_ms,
                orig.start_ms.saturating_sub(tolerance),
                orig.start_ms + tolerance,
                earliest_start,
            ];
            let mut end_limits = vec![
                Some(orig.end_ms),
                next.and_then(|n| n.start_ms.checked_sub(gap)),
                next.and_then(|n| (n.start_ms + tolerance).checked_sub(gap)),
            ];
            if let Some(n) = next
                && self.can_delay
            {
                end_limits.extend(nb.durations().map(|d| n.end_ms.checked_sub(d + gap)));
            }
            for end in end_limits.into_iter().flatten() {
                raw.extend(b.durations().filter_map(|d| end.checked_sub(d)));
            }
            starts.extend(self.grid(&raw));
        }
        starts.sort_unstable();
        starts.dedup();
        let mut best = None;
        let mut best_key = None;
        for s in starts {
            if s != orig.start_ms
                && (self.windows.containing(s).is_some()
                    || (s.abs_diff(orig.start_ms) > tolerance && Some(s) != inward_start)
                    || (s < orig.start_ms && s < earliest_start)
                    || (start_window.is_some()
                        && self.request.options.scene_edges == SceneEdges::Inward
                        && s < orig.start_ms))
            {
                continue;
            }
            let floor = metrics::reading_floor(p.speed_basis, s, self.prior);
            let mut raw = vec![
                orig.end_ms,
                s + self.context.format.time_step_ms(),
                s + b.actual_min,
                s + b.actual_max,
            ];
            raw.extend(b.speed.into_iter().flatten().map(|d| floor + d));
            if b.cps {
                raw.push(floor + 1);
            }
            if b.rs {
                raw.push(floor + 501);
            }
            if let Some(n) = next {
                raw.push(n.start_ms.saturating_sub(gap));
                if p.speed_basis == SpeedBasis::VssOverlapV1 && orig.end_ms > n.start_ms {
                    raw.push(n.start_ms);
                    raw.extend(nb.durations().filter_map(|d| n.end_ms.checked_sub(d)));
                }
                if self.can_delay && tolerance > 0 {
                    raw.push((n.start_ms + tolerance).saturating_sub(gap));
                    raw.extend(nb.durations().filter_map(|d| n.end_ms.checked_sub(d + gap)));
                }
            }
            let mut ends = self.grid(&raw);
            if !ends.contains(&orig.end_ms) {
                ends.push(orig.end_ms);
            }
            for e in ends {
                if e <= s
                    || e != orig.end_ms
                        && (self.windows.containing(e).is_some()
                            || end_window.is_some()
                                && self.request.families.shots
                                && self.request.options.scene_edges == SceneEdges::Inward
                                && e > orig.end_ms)
                {
                    continue;
                }
                let current = Endpoints {
                    start_ms: s,
                    end_ms: e,
                };
                let mut delays = vec![next.map_or(0, |n| n.start_ms)];
                if let Some(n) = next
                    && self.can_delay
                    && tolerance > 0
                {
                    delays.extend(self.grid(&[e + gap, n.start_ms + tolerance]));
                    delays.sort_unstable();
                    delays.dedup();
                }
                for start in delays {
                    if let Some(n) = next
                        && (start < n.start_ms
                            || start - n.start_ms > tolerance
                            || start >= n.end_ms
                            || start != n.start_ms
                                && (self.windows.containing(start).is_some()
                                    || self.following.is_some_and(|f| start >= f))
                            || s >= start
                            || (e > orig.end_ms && e + gap > start)
                            || (e as i64 - start as i64)
                                > (orig.end_ms as i64 - n.start_ms as i64).max(-(gap as i64)))
                    {
                        continue;
                    }
                    let t = Trial {
                        current,
                        next: next.map(|n| Endpoints {
                            start_ms: start,
                            ..n
                        }),
                    };
                    let sc = self.score(t, [b, nb]);
                    let Some((fixed, reduction, trade)) = self.rank(base, sc) else {
                        continue;
                    };
                    let partial = fixed == [0; 4];
                    if fixed < [0; 4] || partial && reduction == 0 {
                        continue;
                    }
                    let start_move =
                        s.abs_diff(orig.start_ms) + next.map_or(0, |n| start - n.start_ms);
                    let key = (
                        fixed,
                        if partial { reduction } else { 0 },
                        std::cmp::Reverse((
                            start_move,
                            e.abs_diff(orig.end_ms),
                            e - s,
                            s,
                            e,
                            start,
                        )),
                    );
                    if best_key.is_none_or(|old| key > old) {
                        best_key = Some(key);
                        best = Some((t, sc, partial, trade));
                    }
                }
            }
        }
        let (t, score, partial, trade_off) = best?;
        let mut endpoints = vec![];
        let mut fixed = vec![];
        let mut effects = vec![];
        if self.request.families.gaps && base.previous_gap > 0 && score.previous_gap == 0 {
            fixed.push(Rule::Gap);
        }
        for (n, (idx, from, to)) in [
            (Some(i), Some(orig), Some(t.current)),
            (self.next, next, t.next),
        ]
        .into_iter()
        .enumerate()
        {
            let (Some(idx), Some(from), Some(to)) = (idx, from, to) else {
                continue;
            };
            if from != to {
                endpoints.push(Change {
                    index: idx as u32,
                    before: from,
                    after: to,
                });
            }
            for (k, &rule) in RULES.iter().enumerate() {
                if base.severity[n][k] > 0
                    && score.severity[n][k] == 0
                    && family(rule, self.request.families)
                    && !fixed.contains(&rule)
                {
                    fixed.push(rule);
                }
            }
            let m = &self.before.metrics[idx];
            let speed = &p.rules.speed;
            for (allowance, limit, rule) in [
                (0, speed.maximum_cps, Rule::CpsHigh),
                (500, speed.maximum_rs_exclusive, Rule::RsHigh),
            ] {
                if speed.enabled
                    && !m.empty
                    && let Some(limit) = limit
                    && base.effective[n] != score.effective[n]
                {
                    let rate = |d: u64| {
                        d.checked_sub(allowance)
                            .filter(|d| *d > 0)
                            .map(|d| f64::from(m.characters) * 1000. / d as f64)
                    };
                    effects.push(Effect {
                        index: idx as u32,
                        rule,
                        before: rate(base.effective[n]),
                        after: rate(score.effective[n]),
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
                        orig.start_ms as i64 - prior.end as i64,
                        t.current.start_ms as i64 - prior.end as i64,
                    )
                })
            } else {
                next.zip(t.next).map(|(old, new)| {
                    (
                        old.start_ms as i64 - orig.end_ms as i64,
                        new.start_ms as i64 - t.current.end_ms as i64,
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
                        effects::shot(label, before, after, self.scenes, p)
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
                .filter_map(|effect| effects::speed(effect, i as u32)),
        );
        Some((
            Unit {
                depends_on: Vec::new(),
                effects_text,
                cut_timecodes,
                index: i as u32,
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
