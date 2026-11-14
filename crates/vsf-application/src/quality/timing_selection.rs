//! Select from immutable timing proposals on their existing worker. A subset of
//! individually safe rows can be unsafe together: repairs recheck the complete
//! candidate before publication. Explicit shifts retain their overlap policy.
use super::{
    Error, Evaluation, MAX_CUES, Report, Result, evaluate_with_scenes, scene, timing_validation,
};
use std::collections::{HashMap, HashSet};
use subtitle_core::{CueId, CueTiming, Document, Edit};

#[derive(Clone, Default)]
pub struct Selection {
    /// None means the whole document; Some(empty) is an empty scope.
    pub scope: Option<Vec<CueId>>,
    pub excluded: Vec<CueId>,
}

pub struct Plan {
    before: Document,
    candidate: Document,
    scope: Option<HashSet<CueId>>,
    excluded: HashSet<CueId>,
    after: Option<Report>,
    pub changed: usize,
    /// Keep the rows reviewable when a combination needs another selection.
    pub error: Option<String>,
}
impl Plan {
    pub fn in_scope(&self, id: CueId) -> bool {
        self.scope.as_ref().is_none_or(|scope| scope.contains(&id))
    }
    pub fn selected(&self, id: CueId) -> bool {
        self.in_scope(id) && !self.excluded.contains(&id)
    }
    pub fn in_scope_at(&self, index: usize) -> bool {
        self.before
            .cues()
            .get(index)
            .is_some_and(|cue| self.in_scope(cue.id()))
    }
    pub fn selected_at(&self, index: usize) -> bool {
        self.before
            .cues()
            .get(index)
            .is_some_and(|cue| self.selected(cue.id()))
    }
    pub fn after<'a>(&'a self, fallback: &'a Report) -> &'a Report {
        self.after.as_ref().unwrap_or(fallback)
    }
    pub fn edit(&self, current: &Document) -> Result<Edit> {
        if current != &self.before {
            return Err(Error::Invalid(
                "Subtitles changed; preview the selection again".into(),
            ));
        }
        if let Some(error) = &self.error {
            return Err(Error::Invalid(error.clone()));
        }
        if self.changed == 0 {
            return Err(Error::Invalid("Select at least one timing change".into()));
        }
        Ok(Edit::Retime {
            timings: self
                .candidate
                .cues()
                .iter()
                .map(|cue| CueTiming {
                    id: cue.id(),
                    start: cue.start(),
                    end: cue.end(),
                })
                .collect(),
        })
    }
}

fn ids(values: Vec<CueId>, known: &HashSet<CueId>) -> Result<HashSet<CueId>> {
    if values.len() > MAX_CUES {
        return Err(Error::Limit("Timing selection exceeds 100,000 cues".into()));
    }
    let mut result = HashSet::with_capacity(values.len());
    for id in values {
        if !known.contains(&id) || !result.insert(id) {
            return Err(Error::Invalid(
                "Timing selection contains an unknown or duplicate cue".into(),
            ));
        }
    }
    Ok(result)
}

/// Explicit uniform shifts may intentionally create overlaps. This only selects
/// endpoints already calculated by the batch planner; it does not recalculate
/// shifts or silently clamp times. Source validation still runs at acceptance.
pub fn prepare_shift(
    document: &Document,
    edit: Edit,
    selection: Selection,
    mut checkpoint: impl FnMut() -> Result<()>,
) -> Result<Plan> {
    checkpoint()?;
    let known: HashSet<_> = document.cues().iter().map(|cue| cue.id()).collect();
    let scope = selection
        .scope
        .map(|scope| ids(scope, &known))
        .transpose()?;
    let excluded = ids(selection.excluded, &known)?;
    let Edit::Retime { timings } = edit else {
        return Err(Error::Invalid(
            "Only timing proposals support timing selection".into(),
        ));
    };
    let mut proposed = HashMap::with_capacity(timings.len());
    for timing in timings {
        if !known.contains(&timing.id) || proposed.insert(timing.id, timing).is_some() {
            return Err(Error::Invalid(
                "Timing proposal contains an unknown or duplicate cue".into(),
            ));
        }
    }
    let mut changed = 0;
    let mut cues = Vec::with_capacity(document.cues().len());
    for cue in document.cues() {
        checkpoint()?;
        let timing = proposed.get(&cue.id()).filter(|_| {
            scope.as_ref().is_none_or(|ids| ids.contains(&cue.id()))
                && !excluded.contains(&cue.id())
        });
        let (start, end) = timing.map_or((cue.start(), cue.end()), |t| (t.start, t.end));
        changed += usize::from(start != cue.start() || end != cue.end());
        cues.push(timing_validation::retimed_cue(cue, start, end)?);
    }
    let candidate = Document::new(cues).map_err(|e| Error::Invalid(e.to_string()))?;
    Ok(Plan {
        before: document.clone(),
        candidate,
        scope,
        excluded,
        after: None,
        changed,
        error: None,
    })
}

/// Run outside accepted-state locks, with the proposal's captured policy, scenes
/// and cancellation/deadline callback. Never assume arbitrary subsets are safe.
pub fn prepare_repair(
    document: &Document,
    before: &Report,
    edit: Edit,
    selection: Selection,
    context: Evaluation<'_>,
    scenes: &scene::Scenes,
    mut checkpoint: impl FnMut() -> Result<()>,
) -> Result<Plan> {
    let mut plan = prepare_shift(document, edit, selection, &mut checkpoint)?;
    let after = evaluate_with_scenes(&plan.candidate, context, scenes, |_, _| checkpoint())?;
    let old = timing_validation::timing_findings(before);
    if after
        .findings
        .iter()
        .any(|finding| timing_validation::worsened(finding, &old))
    {
        plan.error = Some("These timing changes depend on other cues. Restore the coupled changes or choose a different scope.".into());
    }
    plan.after = Some(after);
    Ok(plan)
}

#[cfg(test)]
mod tests;
