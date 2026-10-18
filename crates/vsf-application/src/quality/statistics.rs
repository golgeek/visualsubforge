//! Read-only aggregation over full-document quality metrics. Scope changes the
//! report population, never a cue's reading context or chronological neighbors.
use super::metrics::{Rate, Rating, Unavailable};
use super::scene::SceneCoverage;
use super::{Error, Finding, Profile, Report as QualityReport, Result, Rule, Severity};
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Selection {
    pub range: Option<Range>,
    pub excluded: Vec<u32>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Range {
    pub first: u32,
    pub last: u32,
}
impl Selection {
    pub fn validate(&mut self, total: usize) -> Result<()> {
        if total > super::MAX_CUES || self.excluded.len() > super::MAX_CUES {
            return Err(Error::Limit(
                "Statistics selections are limited to 100000 cues".into(),
            ));
        }
        let range = self.range.unwrap_or(Range {
            first: 1,
            last: total as u32,
        });
        if self.range.is_some()
            && (range.first == 0 || range.first > range.last || range.last as usize > total)
        {
            return Err(Error::Invalid(
                "Cue range must be ordered, one-based and within the document".into(),
            ));
        }
        if self
            .excluded
            .iter()
            .any(|&n| n < range.first || n > range.last)
        {
            return Err(Error::Invalid(
                "Excluded cue numbers must be within the selected scope".into(),
            ));
        }
        self.excluded.sort_unstable();
        self.excluded.dedup();
        Ok(())
    }
    fn includes(&self, index: u32) -> bool {
        let n = index + 1;
        self.range.is_none_or(|r| n >= r.first && n <= r.last)
            && self.excluded.binary_search(&n).is_err()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Extreme {
    pub value: f64,
    pub cue_id: String,
    pub cue_index: u32,
    pub related_cue_id: Option<String>,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Aggregate {
    pub count: u32,
    pub minimum: Option<Extreme>,
    pub maximum: Option<Extreme>,
    pub mean: Option<f64>,
}
#[derive(Default)]
struct Accumulator {
    value: Aggregate,
    sum: f64,
    correction: f64,
}
impl Accumulator {
    fn add(&mut self, value: f64, row: &super::CueMetrics, related: Option<&str>) {
        let extreme = || Extreme {
            value,
            cue_id: row.cue_id.clone(),
            cue_index: row.cue_index,
            related_cue_id: related.map(str::to_owned),
        };
        if self.value.minimum.as_ref().is_none_or(|m| value < m.value) {
            self.value.minimum = Some(extreme());
        }
        if self.value.maximum.as_ref().is_none_or(|m| value > m.value) {
            self.value.maximum = Some(extreme());
        }
        // Compensated summation limits loss from adding many different rate magnitudes.
        let adjusted = value - self.correction;
        let next = self.sum + adjusted;
        self.correction = (next - self.sum) - adjusted;
        self.sum = next;
        self.value.count += 1;
    }
    fn finish(mut self) -> Aggregate {
        self.value.mean = (self.value.count > 0).then(|| self.sum / f64::from(self.value.count));
        self.value
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct UnavailableCounts {
    pub total: u32,
    pub empty_text: u32,
    pub invalid_duration: u32,
    pub reading_allowance: u32,
}
impl UnavailableCounts {
    fn add(&mut self, reason: Unavailable) {
        self.total += 1;
        match reason {
            Unavailable::EmptyText => self.empty_text += 1,
            Unavailable::InvalidDuration => self.invalid_duration += 1,
            Unavailable::ReadingAllowance => self.reading_allowance += 1,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct RatingCount {
    pub rating: Rating,
    pub count: u32,
    pub percentage: Option<f64>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct RuleCount {
    pub rule: Rule,
    pub count: u32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub schema_version: u32,
    pub selection: Selection,
    pub profile: Profile,
    pub profile_hash: String,
    pub complete: bool,
    pub scene_coverage: SceneCoverage,
    pub language_coverage: crate::language::Coverage,
    pub total_cues: u32,
    pub scope_cues: u32,
    pub outside_scope_cues: u32,
    pub excluded_cues: u32,
    pub included_cues: u32,
    pub empty_text_cues: u32,
    pub duration_ms: Aggregate,
    pub cps: Aggregate,
    pub rs: Aggregate,
    pub unavailable_cps: UnavailableCounts,
    pub unavailable_rs: UnavailableCounts,
    pub ratings: Vec<RatingCount>,
    pub line_count: Aggregate,
    pub longest_line: Aggregate,
    pub gaps_ms: Aggregate,
    pub excluded_neighbor_pairs: u32,
    pub negative_gaps: u32,
    pub touching_gaps: u32,
    pub positive_gaps: u32,
    /// Exact integer <= 100000*99999/2, safely represented by JSON numbers.
    pub overlapping_pairs: f64,
    pub overlapping_cues: u32,
    pub findings_count: u32,
    pub affected_cues: u32,
    pub violations: u32,
    pub advisories: u32,
    pub rules: Vec<RuleCount>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub summary: Summary,
    pub findings: Vec<Finding>,
}

/// Consume a complete shared report so the caller need not retain all per-cue
/// metrics and line vectors after aggregation. Checkpoints cover every cue/pair.
pub fn summarize(
    report: QualityReport,
    mut selection: Selection,
    mut checkpoint: impl FnMut() -> Result<()>,
) -> Result<Report> {
    selection.validate(report.metrics.len())?;
    checkpoint()?;
    let rows = &report.metrics;
    let included: HashSet<_> = rows
        .iter()
        .filter(|m| selection.includes(m.cue_index))
        .map(|m| m.cue_id.as_str())
        .collect();
    let mut duration = Accumulator::default();
    let mut cps = Accumulator::default();
    let mut rs = Accumulator::default();
    let mut lines = Accumulator::default();
    let mut longest = Accumulator::default();
    let mut gaps = Accumulator::default();
    let mut unavailable_cps = UnavailableCounts::default();
    let mut unavailable_rs = UnavailableCounts::default();
    let mut ratings: Vec<_> = Rating::ALL
        .into_iter()
        .map(|rating| RatingCount {
            rating,
            count: 0,
            percentage: None,
        })
        .collect();
    let (mut empty, mut negative, mut touching, mut positive) = (0, 0, 0, 0);
    let mut chronological = Vec::with_capacity(included.len());
    for row in rows {
        checkpoint()?;
        if !included.contains(row.cue_id.as_str()) {
            continue;
        }
        chronological.push(row);
        empty += u32::from(row.empty);
        duration.add(row.duration_ms, row, None);
        lines.add(row.line_characters.len() as f64, row, None);
        longest.add(
            f64::from(row.line_characters.iter().copied().max().unwrap_or(0)),
            row,
            None,
        );
        for (rate, accumulator, unavailable) in [
            (&row.cps, &mut cps, &mut unavailable_cps),
            (&row.rs, &mut rs, &mut unavailable_rs),
        ] {
            match rate {
                Rate::Finite { value } => accumulator.add(*value, row, None),
                Rate::Unavailable { reason } => unavailable.add(*reason),
            }
        }
        if let Some(rating) = row.rating
            && let Some(bucket) = ratings.iter_mut().find(|b| b.rating == rating)
        {
            bucket.count += 1;
        }
        if let (Some(gap), Some(next)) = (row.gap_ms, row.next_cue_id.as_deref())
            && included.contains(next)
        {
            gaps.add(gap, row, Some(next));
            if gap < 0. {
                negative += 1;
            } else if gap == 0. {
                touching += 1;
            } else {
                positive += 1;
            }
        }
    }
    // Strict interval intersection counts all pairs, including enclosing cues;
    // it is distinct from signed gaps between original consecutive neighbors.
    chronological.sort_by_key(|m| (m.start_ms as u64, m.cue_index));
    checkpoint()?;
    let mut active = BinaryHeap::new();
    let (mut overlapping_pairs, mut overlapping_cues) = (0u64, 0u32);
    for (i, row) in chronological.iter().enumerate() {
        checkpoint()?;
        let start = row.start_ms as u64;
        while active.peek().is_some_and(|&Reverse(end)| end <= start) {
            active.pop();
            checkpoint()?;
        }
        overlapping_pairs += active.len() as u64;
        if !active.is_empty()
            || chronological
                .get(i + 1)
                .is_some_and(|next| next.start_ms < row.end_ms)
        {
            overlapping_cues += 1;
        }
        active.push(Reverse(row.end_ms as u64));
    }
    // The input already supplies a bounded upper limit. Avoid repeated growing
    // large buffers when collecting scoped findings across report refreshes.
    let mut findings = Vec::with_capacity(report.findings.len());
    let mut affected = HashSet::new();
    let mut rules = HashMap::new();
    let (mut violations, mut advisories) = (0, 0);
    for finding in report.findings {
        checkpoint()?;
        if !included.contains(finding.cue_id.as_str()) {
            continue;
        }
        affected.insert(finding.cue_id.clone());
        if let Some(related) = &finding.related_cue_id
            && included.contains(related.as_str())
        {
            affected.insert(related.clone());
        }
        *rules.entry(finding.rule).or_insert(0) += 1;
        match finding.severity {
            Severity::Violation => violations += 1,
            Severity::Advisory => advisories += 1,
        }
        findings.push(finding);
    }
    for bucket in &mut ratings {
        bucket.percentage = (rs.value.count > 0)
            .then(|| f64::from(bucket.count) * 100. / f64::from(rs.value.count));
    }
    let total = rows.len() as u32;
    let scope = selection.range.map_or(total, |r| r.last - r.first + 1);
    let excluded_neighbor_pairs = total.saturating_sub(1) - gaps.value.count;
    let mut rules: Vec<_> = rules
        .into_iter()
        .map(|(rule, count)| RuleCount { rule, count })
        .collect();
    rules.sort_by_key(|r| r.rule as u8);
    checkpoint()?;
    Ok(Report {
        summary: Summary {
            schema_version: 1,
            selection: selection.clone(),
            profile: report.profile,
            profile_hash: report.profile_hash,
            complete: report.complete,
            scene_coverage: report.scene_coverage,
            language_coverage: report.language_coverage,
            total_cues: total,
            scope_cues: scope,
            outside_scope_cues: total - scope,
            excluded_cues: selection.excluded.len() as u32,
            included_cues: included.len() as u32,
            empty_text_cues: empty,
            duration_ms: duration.finish(),
            cps: cps.finish(),
            rs: rs.finish(),
            unavailable_cps,
            unavailable_rs,
            ratings,
            line_count: lines.finish(),
            longest_line: longest.finish(),
            gaps_ms: gaps.finish(),
            excluded_neighbor_pairs,
            negative_gaps: negative,
            touching_gaps: touching,
            positive_gaps: positive,
            overlapping_pairs: overlapping_pairs as f64,
            overlapping_cues,
            findings_count: findings.len() as u32,
            affected_cues: affected.len() as u32,
            violations,
            advisories,
            rules,
        },
        findings,
    })
}

pub fn evaluate<'a>(
    document: &subtitle_core::Document,
    profile: impl Into<super::Evaluation<'a>>,
    scenes: &super::scene::Scenes,
    mut selection: Selection,
    mut checkpoint: impl FnMut() -> Result<()>,
) -> Result<Report> {
    selection.validate(document.cues().len())?;
    let report = super::evaluate_with_scenes(document, profile, scenes, |_, _| checkpoint())?;
    summarize(report, selection, checkpoint)
}

#[cfg(test)]
mod tests;
