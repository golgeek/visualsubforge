//! Exact integer-ms duration/speed repairs with fixed starts. All proposals are
//! immutable and validated as complete documents; no IO or desktop state.
use super::{
    Error, Profile, Report, Result, Rule, evaluate_with_scenes, gap, metrics,
    scene::{SceneCoverage, Scenes, Windows},
    timing_validation::{retimed_cue, timing_findings, worsened},
};
use serde::Serialize;
use subtitle_core::{CueId, CueTiming, Document, Edit, Milliseconds};
use subtitle_formats::Format;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Document,
    Cue(CueId),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum Skip {
    SceneDataUnavailable,
    AmbiguousOverlap,
    MarkupNeedsReview,
    ConflictingBounds,
    NeighborConstraint,
    FormatPrecision,
    SceneExclusion,
    CoupledConstraints,
    NoDirectChange,
}
impl Skip {
    pub fn explanation(self) -> &'static str {
        match self {
            Self::SceneDataUnavailable => {
                "Current completed scene data is required while scene buffers are enabled."
            }
            Self::AmbiguousOverlap => {
                "Equal starts or nested intervals make this overlapping group ambiguous."
            }
            Self::MarkupNeedsReview => {
                "Timing-sensitive ASS tags or event effects, WebVTT timestamp markers, ambiguous markup or a cue over 64 KiB require manual timing review."
            }
            Self::ConflictingBounds => {
                "The enabled duration and speed limits have no common integer-millisecond endpoint within the time range."
            }
            Self::NeighborConstraint => {
                "The next cue and minimum gap leave no endpoint satisfying all enabled duration and speed limits."
            }
            Self::FormatPrecision => {
                "No endpoint on the ASS/SSA 10 ms grid satisfies all enabled duration, speed and neighbor limits."
            }
            Self::SceneExclusion => {
                "Every otherwise permitted endpoint lies inside a scene exclusion window."
            }
            Self::CoupledConstraints => {
                "This group's coupled timing changes cannot be validated without adding or worsening timing findings; review it manually."
            }
            Self::NoDirectChange => {
                "Another retained repair resolves this cue's duration and speed findings; its endpoint is unchanged."
            }
        }
    }
}
#[derive(Clone, Debug)]
pub struct Row {
    pub cue_index: usize,
    pub new_end: Option<Milliseconds>,
    pub skipped: Option<Skip>,
}
pub struct Proposal {
    document: Document,
    scope: Scope,
    before: Report,
    after: Option<Report>,
    rows: Vec<Row>,
}
impl Proposal {
    pub fn before(&self) -> &Report {
        &self.before
    }
    pub fn after(&self) -> &Report {
        self.after.as_ref().unwrap_or(&self.before)
    }
    pub fn rows(&self) -> &[Row] {
        &self.rows
    }
    pub fn scope(&self) -> Scope {
        self.scope
    }
    pub fn coverage(&self) -> SceneCoverage {
        self.before.scene_coverage
    }
    pub fn changed(&self) -> usize {
        self.rows.iter().filter(|r| r.new_end.is_some()).count()
    }
    pub fn edit(&self, current: &Document) -> Result<Edit> {
        if current != &self.document {
            return Err(Error::Invalid(
                "Subtitles changed after duration preview; create a new proposal".into(),
            ));
        }
        if !self.coverage().available() {
            return Err(Error::Invalid(
                "Duration repair requires current completed scenes when scene buffers are enabled"
                    .into(),
            ));
        }
        if self.changed() == 0 {
            return Err(Error::Invalid(
                "There are no safe duration or speed changes to apply".into(),
            ));
        }
        let mut timings: Vec<_> = current
            .cues()
            .iter()
            .map(|c| CueTiming {
                id: c.id(),
                start: c.start(),
                end: c.end(),
            })
            .collect();
        for row in &self.rows {
            if let Some(end) = row.new_end {
                timings[row.cue_index].end = end;
            }
        }
        Ok(Edit::Retime { timings })
    }
}
fn target(rule: Rule) -> bool {
    matches!(
        rule,
        Rule::DurationShort
            | Rule::DurationLong
            | Rule::CpsLow
            | Rule::CpsHigh
            | Rule::RsLow
            | Rule::RsHigh
            | Rule::SpeedUnavailable
    )
}

pub fn propose(
    document: Document,
    profile: &Profile,
    scenes: &Scenes,
    scope: Scope,
    checkpoint: impl FnMut() -> Result<()>,
) -> Result<Proposal> {
    propose_for_format(document, Format::Srt, profile, scenes, scope, checkpoint)
}

pub fn propose_for_format<'a>(
    document: Document,
    format: Format,
    profile: impl Into<crate::quality::Evaluation<'a>>,
    scenes: &Scenes,
    scope: Scope,
    mut checkpoint: impl FnMut() -> Result<()>,
) -> Result<Proposal> {
    let context = profile.into().with_format(format);
    let profile = context.profile;
    let before = evaluate_with_scenes(&document, context, scenes, |_, _| checkpoint())?;
    let windows = Windows::for_format(profile, scenes, format, &mut checkpoint)?;
    let cues = document.cues();
    if let Scope::Cue(id) = scope
        && !cues.iter().any(|c| c.id() == id)
    {
        return Err(Error::Invalid("Selected cue no longer exists".into()));
    }
    let mut targets = vec![false; cues.len()];
    for finding in &before.findings {
        checkpoint()?;
        let i = finding.cue_index as usize;
        if target(finding.rule) && (scope == Scope::Document || scope == Scope::Cue(cues[i].id())) {
            targets[i] = true;
        }
    }
    let mut order: Vec<_> = (0..cues.len()).collect();
    order.sort_by_key(|&i| (cues[i].start(), i));
    let (groups, ambiguous) = gap::overlap_groups(&document, &order, &mut checkpoint)?;
    let mut rows = Vec::new();
    let mut prior = None;
    for (position, &i) in order.iter().enumerate() {
        checkpoint()?;
        let cue = &cues[i];
        let floor = metrics::reading_floor(profile.speed_basis, cue.start().get(), prior);
        let mut end = cue.end().get();
        if targets[i] {
            let choice = if !windows.coverage().available() {
                Err(Skip::SceneDataUnavailable)
            } else if ambiguous[groups[i]] {
                Err(Skip::AmbiguousOverlap)
            } else if crate::subtitle_markup::retiming_is_ambiguous(cue, format) {
                Err(Skip::MarkupNeedsReview)
            } else {
                let next_start = order.get(position + 1).map(|&j| cues[j].start().get());
                choose_end(
                    &before.metrics[i],
                    profile,
                    floor,
                    next_start,
                    &windows,
                    format.time_step_ms(),
                )
            };
            let (new_end, skipped) = match choice {
                Ok(value) if value != end => {
                    end = value;
                    (Some(Milliseconds::new(end).map_err(Error::Invalid)?), None)
                }
                Ok(_) => (None, Some(Skip::NoDirectChange)),
                Err(reason) => (None, Some(reason)),
            };
            rows.push(Row {
                cue_index: i,
                new_end,
                skipped,
            });
        }
        prior = Some(metrics::ReadingPrior {
            start: floor.min(end),
            end,
            count: before.metrics[i].characters,
        });
    }
    rows.sort_by_key(|r| r.cue_index);
    let mut after = None;
    if rows.iter().any(|r| r.new_end.is_some()) {
        let candidate = retimed(&document, &rows, &mut checkpoint)?;
        let report = evaluate_with_scenes(&candidate, context, scenes, |_, _| checkpoint())?;
        let old = timing_findings(&before);
        let changed: Vec<bool> = (0..cues.len())
            .map(|i| candidate.cues()[i].end() != cues[i].end())
            .collect();
        let mut rejected = vec![false; ambiguous.len()];
        for finding in &report.findings {
            checkpoint()?;
            let i = finding.cue_index as usize;
            if worsened(finding, &old) || (changed[i] && target(finding.rule)) {
                rejected[groups[i]] = true;
            }
        }
        let mut withheld = false;
        for row in &mut rows {
            if rejected[groups[row.cue_index]]
                && (row.new_end.is_some() || row.skipped == Some(Skip::NoDirectChange))
            {
                withheld |= row.new_end.is_some();
                row.new_end = None;
                row.skipped = Some(Skip::CoupledConstraints);
            }
        }
        drop(candidate);
        if !withheld {
            after = Some(report);
        } else if rows.iter().any(|r| r.new_end.is_some()) {
            drop(report);
            let candidate = retimed(&document, &rows, &mut checkpoint)?;
            let report = evaluate_with_scenes(&candidate, context, scenes, |_, _| checkpoint())?;
            for finding in &report.findings {
                checkpoint()?;
                let i = finding.cue_index as usize;
                if worsened(finding, &old)
                    || (candidate.cues()[i].end() != cues[i].end() && target(finding.rule))
                {
                    return Err(Error::Invalid("Coupled duration repairs failed final validation; no proposal was published".into()));
                }
            }
            after = Some(report);
        }
    }
    checkpoint()?;
    Ok(Proposal {
        document,
        scope,
        before,
        after,
        rows,
    })
}

fn choose_end(
    row: &super::CueMetrics,
    profile: &Profile,
    floor: u64,
    next_start: Option<u64>,
    windows: &Windows,
    step: u64,
) -> std::result::Result<u64, Skip> {
    let start = row.start_ms as u64;
    let old = row.end_ms as u64;
    let (mut low, mut high) = (start + 1, Milliseconds::MAX);
    let d = &profile.rules.duration;
    if d.enabled {
        low = low.max(start + u64::from(d.minimum_ms));
        high = high.min(start + u64::from(d.maximum_ms));
    }
    let s = &profile.rules.speed;
    if s.enabled && !row.empty {
        let a = u64::from(row.characters) * 1000;
        if s.minimum_cps.is_some() || s.maximum_cps.is_some() {
            low = low.max(floor + 1);
        }
        if s.minimum_rs.is_some() || s.maximum_rs_exclusive.is_some() {
            low = low.max(floor + 501);
        }
        if let Some(max) = s.maximum_cps {
            low = low.max(floor + a.div_ceil(u64::from(max)));
        }
        if let Some(min) = s.minimum_cps {
            high = high.min(floor + a / u64::from(min));
        }
        if let Some(max) = s.maximum_rs_exclusive {
            low = low.max(floor + 501 + a / u64::from(max));
        }
        if let Some(min) = s.minimum_rs {
            high = high.min(floor + 500 + a / u64::from(min));
        }
    }
    if low > high {
        return Err(Skip::ConflictingBounds);
    }
    // A preceding proposed repair can resolve this cue without another edit.
    if (low..=high).contains(&old) {
        return Ok(old);
    }
    if let Some(next) = next_start {
        high = high.min(
            next.checked_sub(u64::from(profile.timing.cue_gap_ms))
                .ok_or(Skip::NeighborConstraint)?,
        );
    }
    if low > high {
        return Err(Skip::NeighborConstraint);
    }
    low = low.div_ceil(step) * step;
    high = high / step * step;
    if low > high {
        return Err(Skip::FormatPrecision);
    }
    let nearest = old.clamp(low, high);
    let Some((left, right)) = windows.containing(nearest) else {
        return Ok(nearest);
    };
    [left, right]
        .into_iter()
        .filter_map(|v| u64::try_from(v).ok())
        .filter(|v| (low..=high).contains(v))
        .min_by_key(|v| (v.abs_diff(old), *v))
        .ok_or(Skip::SceneExclusion)
}
fn retimed(
    document: &Document,
    rows: &[Row],
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<Document> {
    let mut changes = rows.iter().peekable();
    let mut cues = Vec::with_capacity(document.cues().len());
    for (i, cue) in document.cues().iter().enumerate() {
        checkpoint()?;
        let end = if changes.peek().is_some_and(|r| r.cue_index == i) {
            changes.next().and_then(|r| r.new_end).unwrap_or(cue.end())
        } else {
            cue.end()
        };
        cues.push(retimed_cue(cue, cue.start(), end)?);
    }
    Document::new(cues).map_err(Error::Invalid)
}

#[cfg(test)]
mod tests;
