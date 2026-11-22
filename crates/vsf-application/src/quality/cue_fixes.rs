//! Conservative, selected-cue repairs composed into one immutable candidate.
//! No split, inferred spelling choice, media IO or accepted history lives here.
use super::{
    Error, Evaluation, Finding, Report, Result, Rule, evaluate_with_scenes, scene, timing_fix,
    timing_validation,
};
use crate::text::{self, Operation};
use std::collections::HashMap;
use subtitle_core::{Cue, CueId, Document, Edit};

pub struct Proposal {
    before: Document,
    after: Document,
    pub cue_id: CueId,
    pub fixed: u32,
    pub remaining: u32,
}
impl Proposal {
    pub fn edit(&self, current: &Document) -> Result<Edit> {
        if current != &self.before {
            return Err(Error::Invalid(
                "Subtitles changed; check safe fixes again".into(),
            ));
        }
        if self.fixed == 0 {
            return Err(Error::Invalid("No safe cue fixes are available".into()));
        }
        let before = current.cues();
        let after = self.after.cues();
        let index = after
            .iter()
            .position(|cue| cue.id() == self.cue_id)
            .ok_or_else(|| Error::Invalid("Selected cue is missing from the repair".into()))?;
        let selected = &after[index];
        if before.len() != after.len()
            || before[..index] != after[..index]
            || before[index + 1..] != after[index + 1..]
            || before[index].id() != selected.id()
            || before[index].metadata() != selected.metadata()
        {
            return Err(Error::Invalid(
                "Safe fix must only change the selected cue's text and timing".into(),
            ));
        }
        Ok(Edit::Text {
            id: selected.id(),
            start: selected.start(),
            end: selected.end(),
            text: selected.text().into(),
        })
    }
}
fn belongs(finding: &Finding, id: CueId) -> bool {
    let id = id.0.to_string();
    finding.cue_id == id || finding.related_cue_id.as_deref() == Some(id.as_str())
}
fn issue_counts(report: &Report, id: CueId) -> HashMap<Rule, u32> {
    let mut counts = HashMap::new();
    for finding in report.findings.iter().filter(|f| belongs(f, id)) {
        match finding.rule {
            Rule::CpsHigh | Rule::RsHigh => {
                counts.insert(Rule::CpsHigh, 1);
            }
            Rule::CpsLow | Rule::RsLow => {
                counts.insert(Rule::CpsLow, 1);
            }
            rule => *counts.entry(rule).or_default() += 1,
        }
    }
    counts
}
fn selected_edit(document: &Document, id: CueId, edit: Edit) -> Result<Document> {
    let original = document
        .cues()
        .iter()
        .find(|cue| cue.id() == id)
        .ok_or_else(|| Error::Invalid("Selected cue no longer exists".into()))?;
    let (start, end, text) = match edit {
        Edit::TextBatch { texts } => {
            let value = texts
                .into_iter()
                .find(|text| text.id == id)
                .ok_or_else(|| Error::Invalid("Missing text repair".into()))?;
            (original.start(), original.end(), value.text)
        }
        Edit::Retime { timings } => {
            let value = timings
                .into_iter()
                .find(|timing| timing.id == id)
                .ok_or_else(|| Error::Invalid("Missing timing repair".into()))?;
            (value.start, value.end, original.text().into())
        }
        _ => return Err(Error::Invalid("Unsupported automatic cue repair".into())),
    };
    let mut replacement =
        Cue::new(id, start, end, text).map_err(|e| Error::Invalid(e.to_string()))?;
    if let Some(metadata) = original.metadata() {
        replacement = replacement.with_metadata(metadata.clone());
    }
    Document::new(
        document
            .cues()
            .iter()
            .map(|cue| {
                if cue.id() == id {
                    replacement.clone()
                } else {
                    cue.clone()
                }
            })
            .collect(),
    )
    .map_err(|e| Error::Invalid(e.to_string()))
}
fn safe(before: &Report, after: &Report) -> bool {
    let old = timing_validation::timing_findings(before);
    if after
        .findings
        .iter()
        .any(|finding| timing_validation::worsened(finding, &old))
    {
        return false;
    }
    // Language offsets can move after an earlier correction. Count by cue/rule,
    // while timing comparisons above also check magnitude and neighbors.
    let mut counts = HashMap::new();
    for f in &before.findings {
        *counts
            .entry((f.cue_id.as_str(), f.rule, f.related_cue_id.as_deref()))
            .or_insert(0usize) += 1;
    }
    for f in &after.findings {
        let Some(count) = counts.get_mut(&(f.cue_id.as_str(), f.rule, f.related_cue_id.as_deref()))
        else {
            return false;
        };
        if *count == 0 {
            return false;
        }
        *count -= 1;
    }
    true
}

fn scoped_timing(
    document: &Document,
    context: Evaluation<'_>,
    scenes: &scene::Scenes,
    id: CueId,
    options: timing_fix::Options,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<Document> {
    let request = timing_fix::Request {
        scope: Some(vec![id]),
        options,
        ..Default::default()
    };
    let proposal =
        timing_fix::propose(document.clone(), context, scenes, request, &mut *checkpoint)?;
    let selection = proposal.select(&proposal.default_units(), checkpoint)?;
    if let Some(error) = selection.error {
        return Err(Error::Invalid(error));
    }
    Ok(selection.candidate)
}

pub fn propose(
    document: Document,
    context: Evaluation<'_>,
    scenes: &scene::Scenes,
    id: CueId,
    options: timing_fix::Options,
    mut checkpoint: impl FnMut() -> Result<()>,
) -> Result<Proposal> {
    if !document.cues().iter().any(|cue| cue.id() == id) {
        return Err(Error::Invalid("Selected cue no longer exists".into()));
    }
    let mut report = evaluate_with_scenes(&document, context, scenes, |_, _| checkpoint())?;
    let original = issue_counts(&report, id);
    let mut candidate = document.clone();
    // Existing deterministic operations choose the first safe repair. Spelling
    // alternatives need human choice; only supplied unambiguous replacements run.
    let mut corrections: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.cue_id == id.0.to_string())
        .filter_map(|finding| {
            let detail = finding.language.as_ref()?;
            if detail.manual_reason.is_some() || finding.rule == Rule::Spelling {
                return None;
            }
            Some((
                detail.start,
                Operation::LanguageCorrection {
                    cue_index: finding.cue_index,
                    start: detail.start,
                    end: detail.end,
                    original: detail.word.clone(),
                    replacement: detail.replacement.clone()?,
                },
            ))
        })
        .collect();
    corrections.sort_by_key(|(start, _)| std::cmp::Reverse(*start));
    // Bound repeated full-document validation. Excess findings remain manual.
    corrections.truncate(200);
    let operations = corrections
        .into_iter()
        .map(|(_, op)| op)
        .chain([Operation::Reflow]);
    for operation in operations {
        checkpoint()?;
        let edit = match text::propose_for_format(
            candidate.clone(),
            context.format,
            context,
            &operation,
            &mut checkpoint,
        )
        .and_then(|p| p.edit(&candidate, None))
        {
            Ok(edit) => edit,
            Err(Error::Invalid(_)) => continue,
            Err(error) => return Err(error),
        };
        let mut next = selected_edit(&candidate, id, edit)?;
        if next == candidate {
            continue;
        }
        let mut next_report = evaluate_with_scenes(&next, context, scenes, |_, _| checkpoint())?;
        // A reflow can temporarily increase counted length. Validate a coupled
        // retime before deciding whether this text operation is safe.
        if !safe(&report, &next_report) {
            next = scoped_timing(&next, context, scenes, id, options, &mut checkpoint)?;
            next_report = evaluate_with_scenes(&next, context, scenes, |_, _| checkpoint())?;
        }
        if safe(&report, &next_report)
            && issue_counts(&next_report, id).values().sum::<u32>()
                < issue_counts(&report, id).values().sum()
        {
            candidate = next;
            report = next_report;
        }
    }
    let timed = scoped_timing(&candidate, context, scenes, id, options, &mut checkpoint)?;
    if timed != candidate {
        candidate = timed;
        report = evaluate_with_scenes(&candidate, context, scenes, |_, _| checkpoint())?;
    }
    let left = issue_counts(&report, id);
    let remaining = left.values().sum();
    let fixed = original
        .iter()
        .map(|(rule, count)| count.saturating_sub(*left.get(rule).unwrap_or(&0)))
        .sum();
    Ok(Proposal {
        before: document,
        after: candidate,
        cue_id: id,
        fixed,
        remaining,
    })
}

#[cfg(test)]
mod tests;
