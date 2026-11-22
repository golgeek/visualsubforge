//! One immutable, format-aware timing review. Candidate generation is local;
//! publication and acceptance always check the resulting subtitle document.
mod search;
use super::{
    Error, Evaluation, Report, Result, Rule, evaluate_with_scenes, gap, metrics,
    profile::SpeedBasis,
    scene::{SceneCoverage, Scenes, Windows},
    timing_selection::{self, Selection},
    timing_validation,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use subtitle_core::{CueId, CueTiming, Document, Edit, Milliseconds};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Families {
    pub gaps: bool,
    pub speed: bool,
    pub shots: bool,
}
impl Default for Families {
    fn default() -> Self {
        Self {
            gaps: true,
            speed: true,
            shots: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum Priority {
    #[default]
    ReadingSpeed,
    Spacing,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum SceneEdges {
    #[default]
    Inward,
    Nearest,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Options {
    pub priority: Priority,
    pub start_tolerance_ms: u32,
    pub scene_edges: SceneEdges,
}
impl Options {
    pub fn validate(self) -> Result<()> {
        if self.start_tolerance_ms > 1000 {
            return Err(Error::Invalid("Start tolerance must be 0–1000 ms".into()));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Default)]
pub struct Request {
    pub scope: Option<Vec<CueId>>,
    pub families: Families,
    pub options: Options,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Endpoints {
    pub start_ms: u64,
    pub end_ms: u64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Change {
    pub index: u32,
    pub before: Endpoints,
    pub after: Endpoints,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Effect {
    pub index: u32,
    pub rule: Rule,
    pub before: Option<f64>,
    pub after: Option<f64>,
    pub limit: u32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Unit {
    pub index: u32,
    pub endpoints: Vec<Change>,
    pub fixed: Vec<Rule>,
    pub partial: bool,
    pub effects: Vec<Effect>,
    pub trade_off: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Budget {
    pub effective_ms: u64,
    pub max_counted: Option<u32>,
    pub shorten_by: Option<u32>,
    pub estimate: bool,
    pub at: Endpoints,
    pub at_partial: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Manual {
    pub index: u32,
    pub reason: String,
    pub rules: Vec<Rule>,
    pub budget: Option<Budget>,
}

fn family(rule: Rule, f: Families) -> bool {
    match rule {
        Rule::Gap | Rule::Overlap => f.gaps,
        Rule::DurationShort
        | Rule::DurationLong
        | Rule::CpsHigh
        | Rule::CpsLow
        | Rule::RsHigh
        | Rule::RsLow
        | Rule::SpeedUnavailable => f.speed,
        Rule::SceneStart | Rule::SceneEnd => f.shots,
        _ => false,
    }
}

pub struct Proposal {
    document: Document,
    before: Report,
    after: Report,
    units: Vec<Unit>,
    manual: Vec<Manual>,
    scope: HashSet<CueId>,
    request: Request,
    format: subtitle_formats::Format,
    scenes: Scenes,
    lexicon: Option<crate::language::Lexicon>,
}
impl Proposal {
    pub fn before(&self) -> &Report {
        &self.before
    }
    pub fn after(&self) -> &Report {
        &self.after
    }
    pub fn units(&self) -> &[Unit] {
        &self.units
    }
    pub fn manual(&self) -> &[Manual] {
        &self.manual
    }
    pub fn in_scope(&self, id: CueId) -> bool {
        self.scope.contains(&id)
    }
    pub fn default_units(&self) -> Vec<u32> {
        self.units
            .iter()
            .filter(|u| !u.partial)
            .map(|u| u.index)
            .collect()
    }
}

fn endpoints(document: &Document) -> Vec<Endpoints> {
    document
        .cues()
        .iter()
        .map(|c| Endpoints {
            start_ms: c.start().get(),
            end_ms: c.end().get(),
        })
        .collect()
}

fn retime(document: &Document, units: &[Unit], selected: &HashSet<u32>) -> Result<Edit> {
    let mut points = endpoints(document);
    for unit in units.iter().filter(|u| selected.contains(&u.index)) {
        for e in &unit.endpoints {
            let dest = &mut points[e.index as usize];
            if e.before.start_ms != e.after.start_ms {
                dest.start_ms = e.after.start_ms;
            }
            if e.before.end_ms != e.after.end_ms {
                dest.end_ms = e.after.end_ms;
            }
        }
    }
    let timings = document
        .cues()
        .iter()
        .zip(points)
        .map(|(cue, p)| {
            Ok(CueTiming {
                id: cue.id(),
                start: Milliseconds::new(p.start_ms).map_err(|e| Error::Invalid(e.into()))?,
                end: Milliseconds::new(p.end_ms).map_err(|e| Error::Invalid(e.into()))?,
            })
        })
        .collect::<Result<_>>()?;
    Ok(Edit::Retime { timings })
}

fn budget(
    row: &super::CueMetrics,
    at: Endpoints,
    effective: u64,
    partial: bool,
    profile: &super::Profile,
) -> Budget {
    let speed = &profile.rules.speed;
    let mut maximum = speed
        .maximum_cps
        .map(|n| effective.saturating_mul(n.into()) / 1000);
    if let Some(limit) = speed.maximum_rs_exclusive {
        maximum = effective.checked_sub(501).map(|_| {
            let rs = ((effective - 500).saturating_mul(limit.into()) - 1) / 1000;
            maximum.map_or(rs, |cps| cps.min(rs))
        });
    }
    let max_counted = maximum.map(|n| n.min(u32::MAX.into()) as u32);
    Budget {
        effective_ms: effective,
        max_counted,
        shorten_by: max_counted.map(|n| row.characters.saturating_sub(n)),
        estimate: profile.speed_basis == SpeedBasis::VssOverlapV1
            && row.gap_ms.is_some_and(|g| g < 0.),
        at,
        at_partial: partial,
    }
}

// Compare final checker findings, including untouched cues down an overlap
// chain. Only an explicit spacing trade on that cue permits a higher rate.
fn unsafe_indices(
    before: &Report,
    after: &Report,
    units: &[Unit],
    selected: &HashSet<u32>,
) -> HashSet<usize> {
    let old = timing_validation::timing_findings(before);
    let trades: HashSet<_> = units
        .iter()
        .filter(|u| u.trade_off && selected.contains(&u.index))
        .flat_map(|u| {
            u.effects
                .iter()
                .filter(|e| matches!(e.rule, Rule::CpsHigh | Rule::RsHigh) && e.after > e.before)
                .map(|e| (e.index, e.rule))
        })
        .collect();
    after
        .findings
        .iter()
        .filter(|f| {
            timing_validation::worsened(f, &old) && !trades.contains(&(f.cue_index, f.rule))
        })
        .map(|f| f.cue_index as usize)
        .collect()
}

impl Proposal {
    /// Selection refers to unit indices, so a linked change has one checkbox.
    pub fn select(
        &self,
        selected: &[u32],
        mut checkpoint: impl FnMut() -> Result<()>,
    ) -> Result<timing_selection::Plan> {
        let chosen: HashSet<_> = selected.iter().copied().collect();
        if chosen.len() != selected.len()
            || self
                .units
                .iter()
                .filter(|u| chosen.contains(&u.index))
                .count()
                != chosen.len()
        {
            return Err(Error::Invalid("Unknown or duplicate timing unit".into()));
        }
        let context = Evaluation {
            profile: &self.before.profile,
            format: self.format,
            lexicon: self.lexicon.as_ref(),
        };
        let scenes = &self.scenes;
        let edit = retime(&self.document, &self.units, &chosen)?;
        let mut plan = timing_selection::prepare_shift(
            &self.document,
            edit,
            Selection::default(),
            &mut checkpoint,
        )?;
        let after = evaluate_with_scenes(&plan.candidate, context, scenes, |_, _| checkpoint())?;
        if !unsafe_indices(&self.before, &after, &self.units, &chosen).is_empty()
            || !selection_geometry(
                &self.document,
                &plan.candidate,
                context,
                self.request.options,
                scenes,
                &mut checkpoint,
            )?
        {
            plan.error = Some("These timing units depend on other checked changes.".into());
        }
        plan.after = Some(after);
        Ok(plan)
    }
}

fn selection_geometry(
    before: &Document,
    after: &Document,
    context: Evaluation<'_>,
    options: Options,
    scenes: &Scenes,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<bool> {
    let windows = Windows::for_format(context.profile, scenes, context.format, &mut *checkpoint)?;
    let mut order: Vec<_> = (0..before.cues().len()).collect();
    order.sort_by_key(|&i| (before.cues()[i].start(), i));
    let mut prior_start = None;
    let mut frontier = 0;
    for (pos, &i) in order.iter().enumerate() {
        checkpoint()?;
        let old = &before.cues()[i];
        let new = &after.cues()[i];
        let (s, e) = (new.start().get(), new.end().get());
        let (os, oe) = (old.start().get(), old.end().get());
        if prior_start.is_some_and(|p| p > (s, i))
            || (s != os
                && (windows.containing(s).is_some()
                    || s.abs_diff(os) > options.start_tolerance_ms.into()
                        && !windows.containing(os).is_some_and(|w| w.1 == s as i64)))
            || (e != oe && windows.containing(e).is_some())
        {
            return Ok(false);
        }
        if s < os && s < frontier {
            return Ok(false);
        }
        if let Some(&next) = order.get(pos + 1) {
            let old_gap = before.cues()[next].start().get() as i64 - oe as i64;
            let new_gap = after.cues()[next].start().get() as i64 - e as i64;
            if new_gap < old_gap.min(context.profile.timing.cue_gap_ms.into())
                || e > oe && new_gap < 0
            {
                return Ok(false);
            }
        }
        frontier = frontier.max(e);
        prior_start = Some((s, i));
    }
    Ok(true)
}

pub fn propose(
    document: Document,
    context: Evaluation<'_>,
    scenes: &Scenes,
    request: Request,
    mut checkpoint: impl FnMut() -> Result<()>,
) -> Result<Proposal> {
    request.options.validate()?;
    let before = evaluate_with_scenes(&document, context, scenes, |_, _| checkpoint())?;
    let windows = Windows::for_format(context.profile, scenes, context.format, &mut checkpoint)?;
    let all: HashSet<_> = document.cues().iter().map(|c| c.id()).collect();
    let scope = if let Some(ids) = &request.scope {
        let scoped: HashSet<_> = ids.iter().copied().collect();
        if ids.len() != scoped.len() || !scoped.is_subset(&all) {
            return Err(Error::Invalid(
                "Scope contains an unknown or duplicate cue".into(),
            ));
        }
        scoped
    } else {
        all
    };
    let mut order: Vec<_> = (0..document.cues().len()).collect();
    order.sort_by_key(|&i| (document.cues()[i].start(), i));
    let (groups, ambiguous) = gap::overlap_groups(&document, &order, &mut checkpoint)?;
    let mut targets = HashSet::new();
    for f in &before.findings {
        if family(f.rule, request.families) {
            targets.insert(document.cues()[f.cue_index as usize].id());
            if let Some(id) = f.related_cue_id.as_ref().and_then(|s| s.parse().ok()) {
                targets.insert(CueId(id));
            }
        }
    }
    let mut points = endpoints(&document);
    let mut units = Vec::new();
    let mut reasons = HashMap::new();
    let mut partial_d = HashMap::new();
    let mut prior = None;
    let mut frontier = 0;
    let mut locked = false;
    let blocked = |i: usize| {
        if !scope.contains(&document.cues()[i].id()) {
            Some("outside the selected scope")
        } else if matches!(
            windows.coverage(),
            SceneCoverage::Unavailable | SceneCoverage::Partial
        ) {
            Some("complete scene data is required")
        } else if ambiguous[groups[i]] {
            Some("equal starts or nested intervals require manual review")
        } else if crate::subtitle_markup::retiming_is_ambiguous(&document.cues()[i], context.format)
        {
            Some("timing-sensitive markup or an event effect requires manual review")
        } else {
            None
        }
    };
    let movable: Vec<_> = (0..points.len()).map(blocked).collect();
    for (pos, &i) in order.iter().enumerate() {
        checkpoint()?;
        let next = order.get(pos + 1).copied();
        let mut lock_next = false;
        if targets.contains(&document.cues()[i].id()) {
            if let Some(why) = movable[i] {
                reasons.insert(i, why.to_owned());
            } else {
                let search = search::Search {
                    index: i,
                    next,
                    following: order.get(pos + 2).map(|&j| points[j].start_ms),
                    prior,
                    frontier,
                    locked,
                    can_delay: next.is_some_and(|j| movable[j].is_none()),
                    context,
                    request: &request,
                    before: &before,
                    points: &points,
                    windows: &windows,
                };
                if let Some((unit, effective)) = search.choose() {
                    if unit.partial {
                        let at = unit
                            .endpoints
                            .iter()
                            .find(|e| e.index as usize == i)
                            .map_or(points[i], |e| e.after);
                        partial_d.insert(i, (at, effective));
                    } else {
                        for c in &unit.endpoints {
                            points[c.index as usize] = c.after;
                        }
                        lock_next = unit
                            .endpoints
                            .iter()
                            .any(|c| Some(c.index as usize) == next);
                    }
                    units.push(unit);
                }
                if let Some(j) = next {
                    reasons.insert(
                        i,
                        format!(
                            "Cue {} limits available time at {} ms{}.",
                            j + 1,
                            points[j].start_ms,
                            movable[j].map_or(String::new(), |r| format!(" ({r})"))
                        ),
                    );
                }
            }
        }
        locked = lock_next;
        let p = points[i];
        let floor = metrics::reading_floor(context.profile.speed_basis, p.start_ms, prior);
        prior = Some(metrics::ReadingPrior {
            start: floor.min(p.end_ms),
            end: p.end_ms,
            count: before.metrics[i].characters,
        });
        frontier = frontier.max(p.end_ms);
    }
    let evaluate = |units: &[Unit], checkpoint: &mut dyn FnMut() -> Result<()>| -> Result<Report> {
        let chosen = units
            .iter()
            .filter(|u| !u.partial)
            .map(|u| u.index)
            .collect();
        let edit = retime(&document, units, &chosen)?;
        let plan = timing_selection::prepare_shift(
            &document,
            edit,
            Selection::default(),
            &mut *checkpoint,
        )?;
        evaluate_with_scenes(&plan.candidate, context, scenes, |_, _| checkpoint())
    };
    let mut after = evaluate(&units, &mut checkpoint)?;
    let chosen = units
        .iter()
        .filter(|u| !u.partial)
        .map(|u| u.index)
        .collect();
    let unsafe_cues = unsafe_indices(&before, &after, &units, &chosen);
    if !unsafe_cues.is_empty() {
        let bad: HashSet<_> = unsafe_cues.iter().map(|&i| groups[i]).collect();
        units.retain(|u| {
            !u.endpoints
                .iter()
                .any(|e| bad.contains(&groups[e.index as usize]))
        });
        after = evaluate(&units, &mut checkpoint)?;
        if !unsafe_indices(&before, &after, &units, &chosen).is_empty() {
            return Err(Error::Invalid(
                "Coupled reading starts prevent a safe timing proposal".into(),
            ));
        }
    }
    let positions: HashMap<_, _> = document
        .cues()
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id(), i))
        .collect();
    let partial_units: HashSet<_> = units
        .iter()
        .filter(|u| u.partial)
        .map(|u| u.index as usize)
        .collect();
    let mut unresolved = vec![Vec::new(); document.cues().len()];
    for f in &after.findings {
        if family(f.rule, request.families) {
            let i = f.cue_index as usize;
            let dest = if scope.contains(&document.cues()[i].id()) {
                Some(i)
            } else {
                f.related_cue_id
                    .as_ref()
                    .and_then(|s| s.parse().ok())
                    .and_then(|id| scope.contains(&CueId(id)).then_some(id))
                    .and_then(|id| positions.get(&CueId(id)).copied())
            };
            if let Some(j) = dest {
                unresolved[j].push(f.rule);
            }
        }
    }
    let mut manual = Vec::new();
    for (i, rules) in unresolved
        .into_iter()
        .enumerate()
        .filter(|(_, r)| !r.is_empty())
    {
        checkpoint()?;
        let m = &after.metrics[i];
        let partial = partial_units.contains(&i);
        let (at, d) = partial_d.get(&i).copied().filter(|_| partial).unwrap_or((
            Endpoints {
                start_ms: m.start_ms as u64,
                end_ms: m.end_ms as u64,
            },
            m.effective_duration_ms as u64,
        ));
        let budget = rules
            .iter()
            .any(|r| matches!(r, Rule::CpsHigh | Rule::RsHigh | Rule::SpeedUnavailable))
            .then(|| budget(m, at, d, partial, context.profile));
        let mut detail = Vec::new();
        if let Some(reason) = reasons.remove(&i) {
            detail.push(reason);
        }
        if movable[i].is_none() {
            for endpoint in [m.start_ms, m.end_ms] {
                if let Some((left, right)) = windows.containing(endpoint as u64) {
                    detail.push(format!(
                        "Endpoint {} ms is inside the exclusion window ({left}, {right}) ms.",
                        endpoint as u64
                    ));
                }
            }
            if let Some(next) = m
                .next_cue_id
                .as_ref()
                .and_then(|id| id.parse().ok())
                .and_then(|id| positions.get(&CueId(id)))
            {
                let next_start = after.metrics[*next].start_ms as u64;
                let cap = next_start.saturating_sub(context.profile.timing.cue_gap_ms.into());
                if rules.contains(&Rule::DurationShort)
                    && next_start
                        < at.start_ms + u64::from(context.profile.rules.duration.minimum_ms)
                {
                    detail.push(format!("Minimum duration cannot fit before cue {} without an overlap at this start.",next+1));
                }
                if rules.iter().any(|r| matches!(r, Rule::Gap | Rule::Overlap))
                    && cap > m.reading_start_ms as u64
                {
                    let d = cap - m.reading_start_ms as u64;
                    let (cps, rs, _) = metrics::rates(m.characters, d, m.empty);
                    for (label, old, new, limit, exclusive) in [
                        (
                            "CPS",
                            &m.cps,
                            cps,
                            context.profile.rules.speed.maximum_cps,
                            false,
                        ),
                        (
                            "RS",
                            &m.rs,
                            rs,
                            context.profile.rules.speed.maximum_rs_exclusive,
                            true,
                        ),
                    ] {
                        if context.profile.rules.speed.enabled
                            && let (Some(old), Some(new), Some(limit)) =
                                (old.value(), new.value(), limit)
                            && new > old
                            && (new > f64::from(limit) || exclusive && new == f64::from(limit))
                        {
                            detail.push(format!("Moving the end to {cap} ms would raise {label} from {old:.2} to {new:.2} (limit {limit})."));
                        }
                    }
                }
                if cap < at.end_ms && !scope.contains(&document.cues()[*next].id()) {
                    detail.push(format!(
                        "Cue {} is outside the scope and cannot move.",
                        next + 1
                    ));
                } else if cap < at.end_ms
                    && at.end_ms - cap > u64::from(request.options.start_tolerance_ms)
                {
                    detail.push(format!("Keeping this end requires delaying cue {} by {} ms, beyond the {} ms tolerance.",next+1,at.end_ms-cap,request.options.start_tolerance_ms));
                }
            }
        }
        if let Some(b) = &budget {
            match (b.max_counted,b.shorten_by) {
                (Some(n),Some(by))=>detail.push(format!("Shorten by {by} counted characters (maximum {n}).")),
                _=>detail.push("A text budget is unavailable while reading time is within the 500 ms allowance.".into())
            }
        }
        if detail.is_empty() {
            detail.push(
                "No permitted endpoint satisfies the enabled duration, speed and spacing limits."
                    .into(),
            );
        }
        let reason = detail.join(" ");
        manual.push(Manual {
            index: i as u32,
            reason,
            rules,
            budget,
        });
    }
    Ok(Proposal {
        document,
        before,
        after,
        units,
        manual,
        scope,
        request,
        format: context.format,
        scenes: scenes.clone(),
        lexicon: context.lexicon.cloned(),
    })
}

#[cfg(test)]
mod tests;
