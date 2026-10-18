//! Conservative, immutable minimum-gap proposals. No IO, desktop or native state.
//! Connected overlap groups bound coupled speed effects without a document check
//! for each candidate. At most three full shared evaluations are required.
pub use super::scene::{SceneCoverage, Scenes};
use super::{
    Error, Profile, Report, Result, Rule, evaluate_with_scenes, timing_validation::retimed_cue,
};
use serde::Serialize;
use std::collections::HashMap;
use subtitle_core::{CueTiming, Document, Edit, Milliseconds};
use subtitle_formats::Format;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum Skip {
    AmbiguousOverlap,
    MarkupNeedsReview,
    WouldCollapse,
    MinimumDuration,
    SceneDataUnavailable,
    SceneExclusion,
    SpeedWouldWorsen,
}
impl Skip {
    pub fn explanation(self) -> &'static str {
        match self {
            Self::AmbiguousOverlap => {
                "Equal starts or nested intervals make this overlapping group ambiguous."
            }
            Self::MarkupNeedsReview => {
                "Timing-sensitive ASS tags or event effects, ambiguous markup or a cue over 64 KiB require manual timing review."
            }
            Self::WouldCollapse => "The required gap would leave no positive cue duration.",
            Self::MinimumDuration => "Shortening would violate the enabled minimum duration.",
            Self::SceneDataUnavailable => {
                "Current complete scene data is required while media is attached."
            }
            Self::SceneExclusion => {
                "The proposed endpoint is inside a configured scene exclusion zone."
            }
            Self::SpeedWouldWorsen => {
                "This overlapping group's changes would add or worsen a reading-speed finding."
            }
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub cue_index: usize,
    pub next_cue_index: usize,
    pub old_end: Milliseconds,
    pub new_end: Option<Milliseconds>,
    pub skipped: Option<Skip>,
}
/// Only this module can construct/alter a proposal. The desktop additionally
/// guards document revision, policy epoch and captured media/scene context.
pub struct Proposal {
    document: Document,
    before: Report,
    after: Option<Report>,
    rows: Vec<Row>,
    coverage: SceneCoverage,
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
    pub fn coverage(&self) -> SceneCoverage {
        self.coverage
    }
    pub fn changed(&self) -> usize {
        self.rows.iter().filter(|r| r.new_end.is_some()).count()
    }
    pub fn edit(&self, current: &Document, acknowledge_no_media: bool) -> Result<Edit> {
        if current != &self.document {
            return Err(Error::Invalid(
                "Subtitles changed after gap preview; create a new proposal".into(),
            ));
        }
        if self.changed() == 0 {
            return Err(Error::Invalid(
                "There are no safe gap changes to apply".into(),
            ));
        }
        if self.coverage == SceneCoverage::NoMedia && !acknowledge_no_media {
            return Err(Error::Invalid(
                "Acknowledge that scene constraints were not checked without media".into(),
            ));
        }
        let mut timings: Vec<_> = current
            .cues()
            .iter()
            .map(|cue| CueTiming {
                id: cue.id(),
                start: cue.start(),
                end: cue.end(),
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

pub fn propose(
    document: Document,
    profile: &Profile,
    scenes: &Scenes,
    checkpoint: impl FnMut() -> Result<()>,
) -> Result<Proposal> {
    propose_for_format(document, Format::Srt, profile, scenes, checkpoint)
}

pub fn propose_for_format(
    document: Document,
    format: Format,
    profile: &Profile,
    scenes: &Scenes,
    mut checkpoint: impl FnMut() -> Result<()>,
) -> Result<Proposal> {
    let before = evaluate_with_scenes(&document, profile, scenes, |_, _| checkpoint())?;
    let windows = super::scene::Windows::for_format(profile, scenes, format, &mut checkpoint)?;
    let coverage = windows.coverage();
    let cues = document.cues();
    let mut order: Vec<usize> = (0..cues.len()).collect();
    order.sort_by_key(|&i| (cues[i].start(), i));
    checkpoint()?;
    let (groups, ambiguous) = overlap_groups(&document, &order, &mut checkpoint)?;
    let mut rows = Vec::new();
    let gap = u64::from(profile.timing.cue_gap_ms);
    for pair in order.windows(2) {
        checkpoint()?;
        let (i, next) = (pair[0], pair[1]);
        let cue = &cues[i];
        let next_start = cues[next].start().get();
        if next_start >= cue.end().get() && next_start - cue.end().get() >= gap {
            continue;
        }
        let step = format.time_step_ms();
        let end = next_start.checked_sub(gap).map(|end| end / step * step);
        let reason = if ambiguous[groups[i]] {
            Some(Skip::AmbiguousOverlap)
        } else if crate::subtitle_markup::retiming_is_ambiguous(cue, format) {
            Some(Skip::MarkupNeedsReview)
        } else if end.is_none_or(|end| end <= cue.start().get()) {
            Some(Skip::WouldCollapse)
        } else if profile.rules.duration.enabled
            && end.is_some_and(|end| {
                end - cue.start().get() < u64::from(profile.rules.duration.minimum_ms)
            })
        {
            Some(Skip::MinimumDuration)
        } else if matches!(
            coverage,
            SceneCoverage::Unavailable | SceneCoverage::Partial
        ) {
            Some(Skip::SceneDataUnavailable)
        } else if end.is_some_and(|end| windows.containing(end).is_some()) {
            Some(Skip::SceneExclusion)
        } else {
            None
        };
        rows.push(Row {
            cue_index: i,
            next_cue_index: next,
            old_end: cue.end(),
            new_end: if reason.is_none() {
                Some(
                    Milliseconds::new(
                        end.ok_or_else(|| Error::Invalid("Missing gap endpoint".into()))?,
                    )
                    .map_err(Error::Invalid)?,
                )
            } else {
                None
            },
            skipped: reason,
        });
    }
    rows.sort_by_key(|r| r.cue_index);
    checkpoint()?;
    let mut after = None;
    if rows.iter().any(|r| r.new_end.is_some()) {
        let candidate = retimed(&document, &rows, &mut checkpoint)?;
        let report = evaluate_with_scenes(&candidate, profile, scenes, |_, _| checkpoint())?;
        let mut rejected = vec![false; ambiguous.len()];
        let old = speed_findings(&before);
        for finding in &report.findings {
            checkpoint()?;
            if is_speed(finding.rule) && worsened(finding, &old) {
                rejected[groups[finding.cue_index as usize]] = true;
            }
        }
        let mut withheld = false;
        for row in &mut rows {
            if row.new_end.is_some() && rejected[groups[row.cue_index]] {
                row.new_end = None;
                row.skipped = Some(Skip::SpeedWouldWorsen);
                withheld = true;
            }
        }
        drop(candidate);
        if !withheld {
            after = Some(report);
        } else if rows.iter().any(|r| r.new_end.is_some()) {
            // Overlap groups are independent: shortening an end cannot add an
            // overlap with another group. Recheck the actual retained plan.
            drop(report);
            let candidate = retimed(&document, &rows, &mut checkpoint)?;
            let report = evaluate_with_scenes(&candidate, profile, scenes, |_, _| checkpoint())?;
            if report
                .findings
                .iter()
                .any(|f| is_speed(f.rule) && worsened(f, &old))
            {
                return Err(Error::Invalid(
                    "Gap safety check found coupled speed changes; no proposal was published"
                        .into(),
                ));
            }
            after = Some(report);
        }
    }
    checkpoint()?;
    Ok(Proposal {
        document,
        before,
        after,
        rows,
        coverage,
    })
}

pub(super) fn overlap_groups(
    document: &Document,
    order: &[usize],
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<(Vec<usize>, Vec<bool>)> {
    let cues = document.cues();
    let mut groups = vec![0; cues.len()];
    let mut ambiguous = Vec::new();
    let mut frontier = 0;
    let mut last_start = None;
    for &i in order {
        checkpoint()?;
        let cue = &cues[i];
        if ambiguous.is_empty() || cue.start().get() >= frontier {
            ambiguous.push(false);
            frontier = cue.end().get();
        } else {
            let group = ambiguous.last_mut().expect("a current group exists");
            *group |= cue.end().get() <= frontier || last_start == Some(cue.start());
            frontier = frontier.max(cue.end().get());
        }
        groups[i] = ambiguous.len() - 1;
        last_start = Some(cue.start());
    }
    Ok((groups, ambiguous))
}
fn retimed(
    document: &Document,
    rows: &[Row],
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<Document> {
    let mut changes = rows.iter().peekable();
    let mut cues = Vec::with_capacity(document.cues().len());
    for (index, cue) in document.cues().iter().enumerate() {
        checkpoint()?;
        let end = if changes.peek().is_some_and(|r| r.cue_index == index) {
            changes.next().and_then(|r| r.new_end).unwrap_or(cue.end())
        } else {
            cue.end()
        };
        cues.push(retimed_cue(cue, cue.start(), end)?);
    }
    Document::new(cues).map_err(Error::Invalid)
}
fn is_speed(rule: Rule) -> bool {
    matches!(
        rule,
        Rule::CpsLow | Rule::CpsHigh | Rule::RsLow | Rule::RsHigh | Rule::SpeedUnavailable
    )
}
type Speeds<'a> = HashMap<(u32, Rule, &'a str), Option<f64>>;
fn speed_findings(report: &Report) -> Speeds<'_> {
    report
        .findings
        .iter()
        .filter(|f| is_speed(f.rule))
        .map(|f| ((f.cue_index, f.rule, f.expected.as_str()), f.measured))
        .collect()
}
fn worsened(finding: &super::Finding, old: &Speeds<'_>) -> bool {
    let Some(before) = old.get(&(finding.cue_index, finding.rule, finding.expected.as_str()))
    else {
        return true;
    };
    match (before, finding.measured) {
        (Some(old), Some(new)) => match finding.rule {
            Rule::CpsLow | Rule::RsLow => new < *old,
            Rule::CpsHigh | Rule::RsHigh => new > *old,
            _ => false,
        },
        (None, None) => false,
        _ => true,
    }
}

#[cfg(test)]
mod tests;
