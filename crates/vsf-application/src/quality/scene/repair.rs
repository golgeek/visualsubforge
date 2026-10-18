//! Conservative inward trims. Original overlap groups bound coupled speed effects;
//! shrinking intervals cannot connect previously independent groups. At most three
//! full evaluations (baseline, candidate, retained plan) establish safety.
use super::{SceneCoverage, Scenes, Windows};
use crate::quality::timing_validation::{retimed_cue, timing_findings, worsened};
use crate::quality::{Error, Profile, Report, Result, evaluate_with_scenes, gap};
use serde::Serialize;
use subtitle_core::{CueTiming, Document, Edit, Milliseconds};
use subtitle_formats::Format;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum Skip {
    AmbiguousOverlap,
    MarkupNeedsReview,
    WouldCollapse,
    DurationConstraint,
    TimingWouldWorsen,
}
impl Skip {
    pub fn explanation(self) -> &'static str {
        match self {
            Self::AmbiguousOverlap => {
                "Nested intervals or equal starts make this overlap group ambiguous."
            }
            Self::MarkupNeedsReview => {
                "Timing-sensitive ASS tags or event effects, ambiguous markup or a cue over 64 KiB require manual timing review."
            }
            Self::WouldCollapse => {
                "The scene buffers leave no positive cue duration within the original interval at this format's precision."
            }
            Self::DurationConstraint => {
                "The proposed interval would be outside the enabled duration limits."
            }
            Self::TimingWouldWorsen => {
                "This overlap group's changes would add or worsen a duration, speed, overlap, gap or scene finding."
            }
        }
    }
}
#[derive(Clone, Debug)]
pub struct Row {
    pub cue_index: usize,
    pub new_start: Option<Milliseconds>,
    pub new_end: Option<Milliseconds>,
    pub skipped: Option<Skip>,
}
pub struct Proposal {
    document: Document,
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
    pub fn coverage(&self) -> SceneCoverage {
        self.before.scene_coverage
    }
    pub fn changed(&self) -> usize {
        self.rows.iter().filter(|r| r.new_start.is_some()).count()
    }
    pub fn edit(&self, current: &Document) -> Result<Edit> {
        if current != &self.document {
            return Err(Error::Invalid(
                "Subtitles changed after scene preview; create a new proposal".into(),
            ));
        }
        if !matches!(
            self.coverage(),
            SceneCoverage::Checked | SceneCoverage::Reduced
        ) {
            return Err(Error::Invalid(
                "Scene repair requires current completed scene data".into(),
            ));
        }
        if self.changed() == 0 {
            return Err(Error::Invalid(
                "There are no safe scene changes to apply".into(),
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
            if let (Some(start), Some(end)) = (row.new_start, row.new_end) {
                timings[row.cue_index].start = start;
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

pub fn propose_for_format<'a>(
    document: Document,
    format: Format,
    profile: impl Into<crate::quality::Evaluation<'a>>,
    scenes: &Scenes,
    mut checkpoint: impl FnMut() -> Result<()>,
) -> Result<Proposal> {
    let context = profile.into().with_format(format);
    let profile = context.profile;
    let before = evaluate_with_scenes(&document, context, scenes, |_, _| checkpoint())?;
    let windows = Windows::for_format(profile, scenes, format, &mut checkpoint)?;
    let cues = document.cues();
    let mut order: Vec<_> = (0..cues.len()).collect();
    order.sort_by_key(|&i| (cues[i].start(), i));
    let (groups, ambiguous) = gap::overlap_groups(&document, &order, &mut checkpoint)?;
    let mut rows = Vec::new();
    for (i, cue) in cues.iter().enumerate() {
        checkpoint()?;
        let start_window = windows.containing(cue.start().get());
        let end_window = windows.containing(cue.end().get());
        if start_window.is_none() && end_window.is_none() {
            continue;
        }
        let start = start_window.map_or(cue.start().get() as i64, |(_, right)| right);
        let end = end_window.map_or(cue.end().get() as i64, |(left, _)| left);
        let reason = if ambiguous[groups[i]] {
            Some(Skip::AmbiguousOverlap)
        } else if crate::subtitle_markup::retiming_is_ambiguous(cue, format) {
            Some(Skip::MarkupNeedsReview)
        } else if start >= end || start < 0 || end > Milliseconds::MAX as i64 {
            Some(Skip::WouldCollapse)
        } else if profile.rules.duration.enabled
            && (end - start < i64::from(profile.rules.duration.minimum_ms)
                || end - start > i64::from(profile.rules.duration.maximum_ms))
        {
            Some(Skip::DurationConstraint)
        } else {
            None
        };
        rows.push(Row {
            cue_index: i,
            new_start: if reason.is_none() {
                Some(Milliseconds::new(start as u64).map_err(Error::Invalid)?)
            } else {
                None
            },
            new_end: if reason.is_none() {
                Some(Milliseconds::new(end as u64).map_err(Error::Invalid)?)
            } else {
                None
            },
            skipped: reason,
        });
    }
    let mut after = None;
    if rows.iter().any(|r| r.new_start.is_some()) {
        let candidate = retimed(&document, &rows, &mut checkpoint)?;
        let report = evaluate_with_scenes(&candidate, context, scenes, |_, _| checkpoint())?;
        let old = timing_findings(&before);
        let mut rejected = vec![false; ambiguous.len()];
        for finding in &report.findings {
            checkpoint()?;
            if worsened(finding, &old) {
                rejected[groups[finding.cue_index as usize]] = true;
            }
        }
        let mut withheld = false;
        for row in &mut rows {
            if row.new_start.is_some() && rejected[groups[row.cue_index]] {
                row.new_start = None;
                row.new_end = None;
                row.skipped = Some(Skip::TimingWouldWorsen);
                withheld = true;
            }
        }
        drop(candidate);
        if !withheld {
            after = Some(report);
        } else if rows.iter().any(|r| r.new_start.is_some()) {
            drop(report);
            let retained = retimed(&document, &rows, &mut checkpoint)?;
            let report = evaluate_with_scenes(&retained, context, scenes, |_, _| checkpoint())?;
            for finding in &report.findings {
                checkpoint()?;
                if worsened(finding, &old) {
                    return Err(Error::Invalid("Coupled timing changes failed final scene validation; no proposal was published".into()));
                }
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
    })
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
        let row = if changes.peek().is_some_and(|r| r.cue_index == i) {
            changes.next()
        } else {
            None
        };
        cues.push(retimed_cue(
            cue,
            row.and_then(|r| r.new_start).unwrap_or(cue.start()),
            row.and_then(|r| r.new_end).unwrap_or(cue.end()),
        )?);
    }
    Document::new(cues).map_err(Error::Invalid)
}
