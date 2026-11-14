//! Conservative, selected-cue repairs composed into one immutable candidate.
//! No split, inferred spelling choice, media IO or accepted history lives here.
use super::{
    Error, Evaluation, Finding, Report, Result, Rule, duration, evaluate_with_scenes, gap, scene,
    timing_validation,
};
use crate::text::{self, Operation};
use std::collections::HashMap;
use subtitle_core::{Cue, CueId, CueUpdate, Document, Edit};

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
        Ok(Edit::UpdateBatch {
            cues: self
                .after
                .cues()
                .iter()
                .map(|cue| CueUpdate {
                    id: cue.id(),
                    start: cue.start(),
                    end: cue.end(),
                    text: cue.text().into(),
                })
                .collect(),
        })
    }
}
fn belongs(finding: &Finding, id: CueId) -> bool {
    let id = id.0.to_string();
    finding.cue_id == id || finding.related_cue_id.as_deref() == Some(id.as_str())
}
fn issue_count(report: &Report, id: CueId) -> u32 {
    let mut fast = false;
    let mut slow = false;
    report
        .findings
        .iter()
        .filter(|f| belongs(f, id))
        .filter(|f| match f.rule {
            Rule::CpsHigh | Rule::RsHigh => {
                let first = !fast;
                fast = true;
                first
            }
            Rule::CpsLow | Rule::RsLow => {
                let first = !slow;
                slow = true;
                first
            }
            _ => true,
        })
        .count() as u32
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

pub fn propose(
    document: Document,
    context: Evaluation<'_>,
    scenes: &scene::Scenes,
    id: CueId,
    mut checkpoint: impl FnMut() -> Result<()>,
) -> Result<Proposal> {
    if !document.cues().iter().any(|cue| cue.id() == id) {
        return Err(Error::Invalid("Selected cue no longer exists".into()));
    }
    let mut report = evaluate_with_scenes(&document, context, scenes, |_, _| checkpoint())?;
    let original_count = issue_count(&report, id);
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
    let mut operations: Vec<Option<Operation>> = corrections
        .into_iter()
        .map(|(_, operation)| Some(operation))
        .collect();
    operations.push(Some(Operation::Reflow));
    operations.extend([None, None, None]);
    let text_count = operations.len() - 3;
    for (index, operation) in operations.into_iter().enumerate() {
        checkpoint()?;
        let text_operation = operation.is_some();
        let proposed = match operation {
            Some(operation) => text::propose_for_format(
                candidate.clone(),
                context.format,
                context,
                &operation,
                &mut checkpoint,
            )
            .and_then(|plan| plan.edit(&candidate, None)),
            None if index == text_count => duration::propose_for_format(
                candidate.clone(),
                context.format,
                context,
                scenes,
                duration::Scope::Cue(id),
                &mut checkpoint,
            )
            .and_then(|plan| plan.edit(&candidate)),
            None if index == text_count + 1 => scene::propose_for_format(
                candidate.clone(),
                context.format,
                context,
                scenes,
                &mut checkpoint,
            )
            .and_then(|plan| plan.edit(&candidate)),
            None => gap::propose_for_format(
                candidate.clone(),
                context.format,
                context,
                scenes,
                &mut checkpoint,
            )
            // Never silently acknowledge missing media/scene coverage.
            .and_then(|plan| plan.edit(&candidate, false)),
        };
        let edit = match proposed {
            Ok(edit) => edit,
            Err(Error::Invalid(_)) => continue,
            Err(error) => return Err(error),
        };
        let mut next = selected_edit(&candidate, id, edit)?;
        if next == candidate {
            continue;
        }
        if text_operation {
            // Reflow changes the source-counting metric in legacy profiles.
            // Validate its coupled duration adjustment as one candidate instead
            // of rejecting a temporary speed increase before timing is repaired.
            match duration::propose_for_format(
                next.clone(),
                context.format,
                context,
                scenes,
                duration::Scope::Cue(id),
                &mut checkpoint,
            )
            .and_then(|plan| plan.edit(&next))
            {
                Ok(edit) => next = selected_edit(&next, id, edit)?,
                Err(Error::Invalid(_)) => {}
                Err(error) => return Err(error),
            }
        }
        let next_report = evaluate_with_scenes(&next, context, scenes, |_, _| checkpoint())?;
        if issue_count(&next_report, id) < issue_count(&report, id) && safe(&report, &next_report) {
            candidate = next;
            report = next_report;
        }
    }
    let remaining = issue_count(&report, id);
    Ok(Proposal {
        before: document,
        after: candidate,
        cue_id: id,
        fixed: original_count - remaining,
        remaining,
    })
}

#[cfg(test)]
mod tests;
