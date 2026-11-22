//! One immutable, format-aware timing review. Candidate generation is local;
//! publication and acceptance always check the resulting subtitle document.
mod dependencies;
mod effects;
mod manual;
pub use effects::EffectSegment;
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
    pub depends_on: Vec<u32>,
    pub effects_text: Vec<EffectSegment>,
    pub cut_timecodes: Vec<String>,
    pub index: u32,
    pub endpoints: Vec<Change>,
    pub fixed: Vec<Rule>,
    pub partial: bool,
    pub effects: Vec<Effect>,
    pub trade_off: bool,
}
impl Unit {
    pub fn label(&self) -> String {
        std::iter::once(self.index)
            .chain(
                self.endpoints
                    .iter()
                    .map(|point| point.index)
                    .filter(|&index| index != self.index),
            )
            .map(|index| (index + 1).to_string())
            .collect::<Vec<_>>()
            .join("→")
    }
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
    pub lead: String,
    pub facts: Vec<String>,
    pub blocking_cue: Option<u32>,
    pub budget_text: Option<String>,
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
    #[cfg(test)]
    evaluations: usize,
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
    let mut rates = HashMap::<(u32, Rule), f64>::new();
    if units
        .iter()
        .any(|u| u.trade_off && selected.contains(&u.index))
    {
        // A coupled subset can lose the spacing benefit or change an untouched
        // neighbour's reading start. Authorize only what the review showed.
        let remaining: HashSet<_> = after
            .findings
            .iter()
            .map(|f| (f.cue_id.as_str(), f.related_cue_id.as_deref(), f.rule))
            .collect();
        let mut resolved = HashSet::new();
        for f in &before.findings {
            if matches!(
                f.rule,
                Rule::Gap | Rule::Overlap | Rule::SceneStart | Rule::SceneEnd
            ) && !remaining.contains(&(f.cue_id.as_str(), f.related_cue_id.as_deref(), f.rule))
            {
                resolved.insert((f.cue_id.as_str(), f.rule));
                if let Some(id) = f.related_cue_id.as_deref() {
                    resolved.insert((id, f.rule));
                }
            }
        }
        for unit in units
            .iter()
            .filter(|u| u.trade_off && selected.contains(&u.index))
        {
            let retained = unit.endpoints.iter().any(|e| {
                unit.fixed.iter().any(|r| {
                    resolved.contains(&(before.metrics[e.index as usize].cue_id.as_str(), *r))
                })
            });
            if !retained {
                continue;
            }
            for e in &unit.effects {
                if matches!(e.rule, Rule::CpsHigh | Rule::RsHigh)
                    && e.after > e.before
                    && let Some(limit) = e.after
                {
                    let ceiling = rates.entry((e.index, e.rule)).or_insert(limit);
                    *ceiling = ceiling.max(limit);
                }
            }
        }
    }
    after
        .findings
        .iter()
        .filter(|f| {
            timing_validation::worsened(f, &old)
                && !rates
                    .get(&(f.cue_index, f.rule))
                    .is_some_and(|limit| f.measured.is_some_and(|rate| rate <= *limit))
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
        let mut affected = unsafe_indices(&self.before, &after, &self.units, &chosen);
        let geometry = selection_geometry(
            &self.document,
            &plan.candidate,
            context,
            self.request.options,
            scenes,
            &mut checkpoint,
        )?;
        affected.extend(geometry);
        if !affected.is_empty() {
            // A timing finding can belong to an unchanged reader or either
            // side of a gap. Resolve those neighbours back to review units.
            let positions: HashMap<_, _> = self
                .before
                .metrics
                .iter()
                .enumerate()
                .map(|(index, m)| (m.cue_id.as_str(), index))
                .collect();
            let mut nearby = affected.clone();
            for &index in &affected {
                if let Some(next) = self.before.metrics[index]
                    .next_cue_id
                    .as_deref()
                    .and_then(|id| positions.get(id))
                {
                    nearby.insert(*next);
                }
            }
            for (index, m) in self.before.metrics.iter().enumerate() {
                if m.next_cue_id
                    .as_deref()
                    .and_then(|id| positions.get(id))
                    .is_some_and(|next| affected.contains(next))
                {
                    nearby.insert(index);
                }
            }
            let mut labels = Vec::new();
            for unit in &self.units {
                if nearby.contains(&(unit.index as usize))
                    || unit
                        .endpoints
                        .iter()
                        .any(|e| nearby.contains(&(e.index as usize)))
                {
                    plan.selection_conflicts.push(unit.index);
                    labels.push(unit.label());
                }
            }
            let message = if labels.len() == 1 {
                format!("Cue {} conflicts with the selected timing.", labels[0])
            } else {
                format!("Cues {} depend on each other.", labels.join(" and "))
            };
            plan.error = Some(format!("{message} Restore the default selection to apply."));
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
) -> Result<Vec<usize>> {
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
            return Ok(vec![i]);
        }
        if s < os && s < frontier {
            return Ok(vec![i]);
        }
        if let Some(&next) = order.get(pos + 1) {
            let old_gap = before.cues()[next].start().get() as i64 - oe as i64;
            let new_gap = after.cues()[next].start().get() as i64 - e as i64;
            if new_gap < old_gap.min(context.profile.timing.cue_gap_ms.into())
                || e > oe && new_gap < 0
            {
                return Ok(vec![i, next]);
            }
        }
        frontier = frontier.max(e);
        prior_start = Some((s, i));
    }
    Ok(Vec::new())
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
    let mut partial_d = HashMap::new();
    let mut prior = None;
    let mut frontier = 0;
    let mut locked = false;
    let mut provenance = dependencies::Provenance::new(points.len());
    let blocked = |i: usize| {
        if !scope.contains(&document.cues()[i].id()) {
            Some("Outside the selected scope.")
        } else if matches!(
            windows.coverage(),
            SceneCoverage::Unavailable | SceneCoverage::Partial
        ) {
            Some("Finish the scene scan to fix this cue.")
        } else if ambiguous[groups[i]] {
            Some("Equal starts or nested cues need manual timing.")
        } else if crate::subtitle_markup::retiming_is_ambiguous(&document.cues()[i], context.format)
        {
            Some("Timed markup or an event effect needs manual timing.")
        } else {
            None
        }
    };
    let movable: Vec<_> = (0..points.len()).map(blocked).collect();
    for (pos, &i) in order.iter().enumerate() {
        checkpoint()?;
        let next = order.get(pos + 1).copied();
        let mut lock_next = false;
        if targets.contains(&document.cues()[i].id()) && movable[i].is_none() {
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
                scenes,
            };
            if let Some((mut unit, effective)) = search.choose() {
                unit.depends_on = provenance.prerequisites(i, next, order.get(pos + 2).copied());
                if unit.partial {
                    let at = unit
                        .endpoints
                        .iter()
                        .find(|e| e.index as usize == i)
                        .map_or(points[i], |e| e.after);
                    partial_d.insert(i, (at, effective));
                } else {
                    provenance.applied(&unit);
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
        }
        locked = lock_next;
        let p = points[i];
        provenance.advance(
            i,
            p.end_ms >= frontier,
            context.profile.speed_basis == SpeedBasis::VssOverlapV1
                && prior.is_some_and(|previous: metrics::ReadingPrior| previous.end > p.start_ms),
        );
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
    let mut withheld = HashMap::new();
    let mut evaluations = 1;
    let after = loop {
        let candidate = evaluate(&units, &mut checkpoint)?;
        evaluations += 1;
        let chosen = units
            .iter()
            .filter(|unit| !unit.partial)
            .map(|unit| unit.index)
            .collect();
        let unsafe_cues = unsafe_indices(&before, &candidate, &units, &chosen);
        if unsafe_cues.is_empty() {
            break candidate;
        }
        let count = units.len();
        dependencies::withhold(&mut units, &groups, &unsafe_cues, &mut withheld);
        // The contract caps full checks at three. The original has already
        // been evaluated and remains available if a second chain is unsafe.
        if units.len() == count || evaluations >= 3 {
            let root = *unsafe_cues.iter().min().expect("unsafe set is nonempty") as u32;
            for unit in units.drain(..) {
                withheld.insert(unit.index as usize, root);
            }
            break before.clone();
        }
    };
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
        let next = m
            .next_cue_id
            .as_ref()
            .and_then(|id| id.parse().ok())
            .and_then(|id| positions.get(&CueId(id)))
            .map(|&index| {
                (
                    index as u32,
                    after.metrics[index].start_ms as u64,
                    scope.contains(&document.cues()[index].id()),
                )
            });
        let mut row = Manual::new(i, rules, budget);
        row.explain(&manual::ReasonContext {
            blocked: movable[i],
            withheld: withheld.get(&i).copied(),
            next,
            at,
            metric: m,
            profile: context.profile,
            windows: &windows,
            tolerance: request.options.start_tolerance_ms,
        });
        manual.push(row);
    }
    Ok(Proposal {
        #[cfg(test)]
        evaluations,
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
