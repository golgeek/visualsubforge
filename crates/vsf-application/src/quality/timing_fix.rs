//! One immutable, format-aware timing review. Candidate generation is local;
//! publication and acceptance always check the resulting subtitle document.
mod dependencies;
mod effects;
mod manual;
mod precision;
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
    #[serde(skip)]
    claims: Vec<Claim>,
    pub depends_on: Vec<u32>,
    pub conflicts_with: Vec<u32>,
    pub effects_text: Vec<EffectSegment>,
    pub cut_timecodes: Vec<String>,
    pub index: u32,
    pub endpoints: Vec<Change>,
    pub fixed: Vec<Rule>,
    pub partial: bool,
    pub effects: Vec<Effect>,
    pub trade_off: bool,
}
/// An index refers to the immutable captured document throughout a review.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Claim {
    index: usize,
    rule: Rule,
    related: Option<usize>,
}

fn unkept_claims(after: &Report, units: &[Unit], selected: &HashSet<u32>) -> HashSet<usize> {
    let remaining: HashSet<_> = after
        .findings
        .iter()
        .map(|finding| {
            (
                finding.cue_index as usize,
                finding.rule,
                finding.related_cue_id.as_deref(),
            )
        })
        .collect();
    units
        .iter()
        .filter(|unit| selected.contains(&unit.index))
        .flat_map(|unit| &unit.claims)
        .filter(|claim| {
            let related = claim.related.map(|i| after.metrics[i].cue_id.as_str());
            remaining.contains(&(claim.index, claim.rule, related))
        })
        .map(|claim| claim.index)
        .collect()
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

pub const WITHHELD_ALL_MESSAGE: &str =
    "Timing changes were withheld because the safety check could not finish. Try a smaller scope.";

pub struct Proposal {
    pub withheld_all: bool,
    #[cfg(test)]
    evaluation_time: [std::time::Duration; 3],
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
        self.validate_selection(selected, true, &mut checkpoint)
    }
    fn validate_selection(
        &self,
        selected: &[u32],
        verify_claims: bool,
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
        if verify_claims {
            affected.extend(unkept_claims(&after, &self.units, &chosen));
        }
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

struct PlanningInputs<'a> {
    document: &'a Document,
    before: &'a Report,
    context: Evaluation<'a>,
    scenes: &'a Scenes,
    request: &'a Request,
    scope: &'a HashSet<CueId>,
    windows: &'a Windows,
}
struct Walk {
    units: Vec<Unit>,
    partial_d: HashMap<usize, (Endpoints, u64)>,
    blocked: Vec<Option<&'static str>>,
}
struct Retained {
    withheld_all: bool,
    #[cfg(test)]
    evaluation_time: [std::time::Duration; 3],
    after: Report,
    withheld: HashMap<usize, u32>,
    withheld_facts: HashMap<u32, String>,
    #[cfg(test)]
    evaluations: usize,
}
fn targets(document: &Document, before: &Report, families: Families) -> HashSet<CueId> {
    let mut targets = HashSet::new();
    for finding in &before.findings {
        if family(finding.rule, families) {
            targets.insert(document.cues()[finding.cue_index as usize].id());
            if let Some(id) = finding.related_cue_id.as_ref().and_then(|s| s.parse().ok()) {
                targets.insert(CueId(id));
            }
        }
    }
    targets
}
impl PlanningInputs<'_> {
    fn walk(
        &self,
        order: &[usize],
        groups: &[usize],
        ambiguous: &[bool],
        checkpoint: &mut impl FnMut() -> Result<()>,
    ) -> Result<Walk> {
        let Self {
            document,
            before,
            context,
            scenes,
            request,
            scope,
            windows,
        } = *self;
        let targets = targets(document, before, request.families);
        let mut points = endpoints(document);
        let mut units = Vec::new();
        let mut partial_timings = HashMap::new();
        let mut prior = None;
        let mut frontier = 0;
        let mut locked = false;
        let mut provenance = dependencies::Provenance::new(points.len());
        let mut alternatives = dependencies::PartialReads::new(points.len());
        let blocked = |index: usize| {
            if !scope.contains(&document.cues()[index].id()) {
                Some("Outside the selected scope.")
            } else if matches!(
                windows.coverage(),
                SceneCoverage::Unavailable | SceneCoverage::Partial
            ) {
                Some("Finish the scene scan to fix this cue.")
            } else if ambiguous[groups[index]] {
                Some("Equal starts or nested cues need manual timing.")
            } else if crate::subtitle_markup::retiming_is_ambiguous(
                &document.cues()[index],
                context.format,
            ) {
                Some("Timed markup or an event effect needs manual timing.")
            } else {
                None
            }
        };
        let blocked_reasons: Vec<_> = (0..points.len()).map(blocked).collect();
        for (position, &index) in order.iter().enumerate() {
            checkpoint()?;
            let next = order.get(position + 1).copied();
            let mut lock_next = false;
            if targets.contains(&document.cues()[index].id()) && blocked_reasons[index].is_none() {
                let search = search::Search {
                    index,
                    next,
                    remaining_order: &order[position..],
                    previous: position.checked_sub(1).map(|p| order[p]),
                    following: order
                        .get(position + 2)
                        .map(|&neighbour| points[neighbour].start_ms),
                    prior,
                    frontier,
                    locked,
                    can_delay: next.is_some_and(|neighbour| blocked_reasons[neighbour].is_none()),
                    context,
                    request,
                    before,
                    points: &points,
                    windows,
                    scenes,
                };
                if let Some((mut unit, effective)) = search.choose() {
                    unit.depends_on =
                        provenance.prerequisites(index, next, order.get(position + 2).copied());
                    unit.conflicts_with =
                        alternatives.read_by(index, next, order.get(position + 2).copied());
                    if unit.partial {
                        alternatives.record(&unit);
                        let at = unit
                            .endpoints
                            .iter()
                            .find(|endpoint| endpoint.index as usize == index)
                            .map_or(points[index], |endpoint| endpoint.after);
                        partial_timings.insert(index, (at, effective));
                    } else {
                        provenance.applied(&unit);
                        for change in &unit.endpoints {
                            points[change.index as usize] = change.after;
                        }
                        lock_next = unit
                            .endpoints
                            .iter()
                            .any(|change| Some(change.index as usize) == next);
                    }
                    units.push(unit);
                }
            }
            locked = lock_next;
            let planned = points[index];
            let overlap_chain = context.profile.speed_basis == SpeedBasis::VssOverlapV1
                && position > 0
                && (prior.is_some_and(|previous: metrics::ReadingPrior| {
                    previous.end > planned.start_ms
                }) || document.cues()[order[position - 1]].end()
                    > document.cues()[index].start());
            provenance.advance(index, planned.end_ms >= frontier, overlap_chain);
            alternatives.advance(index, planned.end_ms >= frontier, overlap_chain);
            let floor =
                metrics::reading_floor(context.profile.speed_basis, planned.start_ms, prior);
            prior = Some(metrics::ReadingPrior {
                start: floor.min(planned.end_ms),
                end: planned.end_ms,
                count: before.metrics[index].characters,
            });
            frontier = frontier.max(planned.end_ms);
        }
        if context.profile.speed_basis == SpeedBasis::VssOverlapV1 {
            dependencies::couple_readers(document, &points, order, &mut units);
        }
        Ok(Walk {
            units,
            partial_d: partial_timings,
            blocked: blocked_reasons,
        })
    }
    fn retain_safe(
        &self,
        units: &mut Vec<Unit>,
        groups: &[usize],
        candidate_limit: usize,
        checkpoint: &mut impl FnMut() -> Result<()>,
    ) -> Result<Retained> {
        let Self {
            document,
            before,
            context,
            scenes,
            ..
        } = *self;
        let evaluate =
            |units: &[Unit], checkpoint: &mut dyn FnMut() -> Result<()>| -> Result<Report> {
                let chosen = units
                    .iter()
                    .filter(|u| !u.partial)
                    .map(|u| u.index)
                    .collect();
                let edit = retime(document, units, &chosen)?;
                let plan = timing_selection::prepare_shift(
                    document,
                    edit,
                    Selection::default(),
                    &mut *checkpoint,
                )?;
                evaluate_with_scenes(&plan.candidate, context, scenes, |_, _| checkpoint())
            };
        let mut withheld = HashMap::new();
        let mut withheld_facts = HashMap::new();
        let mut evaluations = 0;
        let mut withheld_all = false;
        #[cfg(test)]
        let mut evaluation_time = [std::time::Duration::ZERO; 3];
        let after = loop {
            #[cfg(test)]
            let started = std::time::Instant::now();
            let candidate = evaluate(units, checkpoint)?;
            #[cfg(test)]
            {
                evaluation_time[evaluations] = started.elapsed();
            }
            evaluations += 1;
            let chosen = units
                .iter()
                .filter(|unit| !unit.partial)
                .map(|unit| unit.index)
                .collect();
            let mut unsafe_cues = unsafe_indices(before, &candidate, units, &chosen);
            unsafe_cues.extend(unkept_claims(&candidate, units, &chosen));
            if unsafe_cues.is_empty() {
                break candidate;
            }
            let old = timing_validation::timing_findings(before);
            let mut affected_groups = HashMap::new();
            for finding in &candidate.findings {
                let index = finding.cue_index as usize;
                if unsafe_cues.contains(&index) && timing_validation::worsened(finding, &old) {
                    affected_groups
                        .entry(groups[index])
                        .or_insert((index, manual::family_name(finding.rule)));
                }
            }
            for unit in units.iter() {
                if let Some(&(index, name)) = unit
                    .endpoints
                    .iter()
                    .find_map(|change| affected_groups.get(&groups[change.index as usize]))
                {
                    withheld_facts.insert(
                        unit.index,
                        format!("Cue {} would have a worse {name} finding.", index + 1),
                    );
                }
            }
            let count = units.len();
            dependencies::withhold(units, groups, &unsafe_cues, &mut withheld);
            // The original is a known-safe fallback and consumes no candidate pass.
            if units.len() == count || evaluations >= candidate_limit {
                withheld_all = true;
                let root = *unsafe_cues.iter().min().expect("unsafe set is nonempty") as u32;
                for unit in units.drain(..) {
                    withheld.insert(unit.index as usize, root);
                    for change in unit.endpoints {
                        withheld.insert(change.index as usize, root);
                    }
                }
                break before.clone();
            }
        };
        Ok(Retained {
            withheld_all,
            #[cfg(test)]
            evaluation_time,
            after,
            withheld,
            withheld_facts,
            #[cfg(test)]
            evaluations,
        })
    }
    fn manual_rows(
        &self,
        walk: &Walk,
        retained: &Retained,
        checkpoint: &mut impl FnMut() -> Result<()>,
    ) -> Result<Vec<Manual>> {
        let Self {
            document,
            request,
            scope,
            windows,
            context,
            ..
        } = *self;
        let Retained {
            after, withheld, ..
        } = retained;
        let positions: HashMap<_, _> = document
            .cues()
            .iter()
            .enumerate()
            .map(|(index, c)| (c.id(), index))
            .collect();
        let partial_units: HashSet<_> = walk
            .units
            .iter()
            .filter(|u| u.partial)
            .map(|u| u.index as usize)
            .collect();
        let mut unresolved = vec![Vec::new(); document.cues().len()];
        for finding in &after.findings {
            if family(finding.rule, request.families) {
                let index = finding.cue_index as usize;
                let dest = if scope.contains(&document.cues()[index].id()) {
                    Some(index)
                } else {
                    finding
                        .related_cue_id
                        .as_ref()
                        .and_then(|s| s.parse().ok())
                        .and_then(|id| scope.contains(&CueId(id)).then_some(id))
                        .and_then(|id| positions.get(&CueId(id)).copied())
                };
                if let Some(destination) = dest {
                    unresolved[destination].push(finding.rule);
                }
            }
        }
        let mut manual = Vec::new();
        for (index, rules) in unresolved
            .into_iter()
            .enumerate()
            .filter(|(_, r)| !r.is_empty())
        {
            checkpoint()?;
            let metric = &after.metrics[index];
            let partial = partial_units.contains(&index);
            let (at, effective) = walk
                .partial_d
                .get(&index)
                .copied()
                .filter(|_| partial)
                .unwrap_or((
                    Endpoints {
                        start_ms: metric.start_ms as u64,
                        end_ms: metric.end_ms as u64,
                    },
                    metric.effective_duration_ms as u64,
                ));
            let budget = rules
                .iter()
                .any(|r| matches!(r, Rule::CpsHigh | Rule::RsHigh | Rule::SpeedUnavailable))
                .then(|| budget(metric, at, effective, partial, context.profile));
            let next = metric
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
            let mut row = Manual::new(index, rules, budget);
            if retained.withheld_all && withheld.contains_key(&index) {
                row.lead = WITHHELD_ALL_MESSAGE.into();
                manual.push(row);
                continue;
            }
            row.explain(&manual::ReasonContext {
                families: request.families,
                blocked: walk.blocked[index],
                withheld: withheld.get(&index).copied(),
                next,
                at,
                metric,
                profile: context.profile,
                windows,
                tolerance: request.options.start_tolerance_ms,
            });
            if let Some(fact) = withheld
                .get(&index)
                .and_then(|root| retained.withheld_facts.get(root))
            {
                row.facts.push(fact.clone());
            }
            manual.push(row);
        }
        Ok(manual)
    }
}

pub fn propose(
    document: Document,
    context: Evaluation<'_>,
    scenes: &Scenes,
    request: Request,
    checkpoint: impl FnMut() -> Result<()>,
) -> Result<Proposal> {
    propose_with_limit(document, context, scenes, request, 3, checkpoint)
}

fn propose_with_limit(
    document: Document,
    context: Evaluation<'_>,
    scenes: &Scenes,
    request: Request,
    candidate_limit: usize,
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
    let inputs = PlanningInputs {
        document: &document,
        before: &before,
        context,
        scenes,
        request: &request,
        scope: &scope,
        windows: &windows,
    };
    let mut walk = inputs.walk(&order, &groups, &ambiguous, &mut checkpoint)?;
    precision::refine(&inputs, &order, &mut walk.units, &mut checkpoint)?;
    let mut retained =
        inputs.retain_safe(&mut walk.units, &groups, candidate_limit, &mut checkpoint)?;
    dependencies::link_alternatives(&mut walk.units);
    dependencies::withhold_incompatible_partials(
        &mut walk.units,
        &mut retained.withheld,
        &mut checkpoint,
    )?;
    let manual = inputs.manual_rows(&walk, &retained, &mut checkpoint)?;
    let units = walk.units;
    let after = retained.after;
    Ok(Proposal {
        withheld_all: retained.withheld_all,
        #[cfg(test)]
        evaluation_time: retained.evaluation_time,
        #[cfg(test)]
        evaluations: retained.evaluations,
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
