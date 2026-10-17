//! Bounded deterministic checks over an immutable accepted document.
use super::{Error, MAX_CUES, MAX_FINDINGS, Profile, Result, metrics, profile::SpeedBasis};
use metrics::{Rate, Rating, TextMetrics};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use subtitle_core::Document;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum Rule {
    Empty,
    DurationShort,
    DurationLong,
    CpsLow,
    CpsHigh,
    RsLow,
    RsHigh,
    SpeedUnavailable,
    LineCount,
    LineLength,
    Overlap,
    Gap,
    SceneStart,
    SceneEnd,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Violation,
    Advisory,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub rule: Rule,
    /// Decimal identity strings avoid JSON/JavaScript integer precision loss.
    pub cue_id: String,
    pub cue_index: u32,
    pub related_cue_id: Option<String>,
    pub severity: Severity,
    pub measured: Option<f64>,
    pub expected: String,
    pub explanation: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct CueMetrics {
    pub cue_id: String,
    pub cue_index: u32,
    // Domain timestamps are bounded to < 2^53. f64 here is an exact JSON projection.
    pub start_ms: f64,
    pub end_ms: f64,
    pub duration_ms: f64,
    pub reading_start_ms: f64,
    pub effective_duration_ms: f64,
    pub characters: u32,
    pub line_characters: Vec<u32>,
    pub empty: bool,
    pub cps: Rate,
    pub rs: Rate,
    pub rating: Option<Rating>,
    pub next_cue_id: Option<String>,
    pub gap_ms: Option<f64>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub schema_version: u32,
    pub profile: Profile,
    pub profile_hash: String,
    pub complete: bool,
    pub scene_coverage: super::scene::SceneCoverage,
    pub affected_cues: u32,
    pub metrics: Vec<CueMetrics>,
    pub findings: Vec<Finding>,
}

/// Full evaluation is bounded by cue/text/finding limits. Caller-supplied
/// checkpoints permit cancellation/progress without importing a job framework.
/// Output remains in document order; time neighbors are found chronologically.
pub fn evaluate(
    document: &Document,
    profile: &Profile,
    checkpoint: impl FnMut(usize, usize) -> Result<()>,
) -> Result<Report> {
    evaluate_with_scenes(
        document,
        profile,
        &super::scene::Scenes::NoMedia,
        checkpoint,
    )
}

pub fn evaluate_with_scenes(
    document: &Document,
    profile: &Profile,
    scenes: &super::scene::Scenes,
    mut checkpoint: impl FnMut(usize, usize) -> Result<()>,
) -> Result<Report> {
    profile.validate()?;
    let windows = super::scene::Windows::new(profile, scenes, || checkpoint(0, 0))?;
    let cues = document.cues();
    let total = cues.len();
    if total > MAX_CUES {
        return Err(Error::Limit(
            "Quality checks support at most 100000 cues".into(),
        ));
    }
    let bytes: usize = cues.iter().map(|c| c.text().len()).sum();
    if bytes > super::MAX_TEXT_BYTES {
        return Err(Error::Limit("Quality text exceeds 16 MiB".into()));
    }
    checkpoint(0, total)?;
    let mut texts = Vec::with_capacity(total);
    for (index, cue) in cues.iter().enumerate() {
        texts.push(metrics::text(cue.text(), || checkpoint(index, total))?);
    }
    let mut chronological: Vec<usize> = (0..total).collect();
    chronological.sort_by_key(|&i| (cues[i].start().get(), i));
    checkpoint(0, total)?;
    let mut starts = vec![0; total];
    let mut next = vec![None; total];
    // Furthest preceding endpoint supplies one representative nested-overlap
    // relationship per later cue. Enumerating all pairs could be quadratic.
    let mut overlaps = vec![None; total];
    let mut frontier: Option<usize> = None;
    for (position, &i) in chronological.iter().enumerate() {
        checkpoint(position, total)?;
        let cue = &cues[i];
        let start = cue.start().get();
        starts[i] = start;
        if profile.speed_basis == SpeedBasis::VssOverlapV1 && position > 0 {
            let previous = chronological[position - 1];
            let prior = &cues[previous];
            if prior.end().get() > start {
                starts[i] = (starts[previous] + metrics::reading_duration(texts[previous].count))
                    .min(prior.end().get())
                    .max(start)
                    .min(cue.end().get());
            }
        }
        next[i] = chronological.get(position + 1).copied();
        if let Some(prior) = frontier
            && cues[prior].end().get() > start
        {
            overlaps[i] = Some(prior);
        }
        if frontier.is_none_or(|prior| cue.end() > cues[prior].end()) {
            frontier = Some(i);
        }
    }
    let mut report = Report {
        schema_version: 2,
        profile: profile.clone(),
        profile_hash: profile.fingerprint()?,
        complete: profile.limitations.is_empty() && windows.coverage().available(),
        scene_coverage: windows.coverage(),
        affected_cues: 0,
        metrics: Vec::with_capacity(total),
        findings: Vec::new(),
    };
    for (i, cue) in cues.iter().enumerate() {
        checkpoint(i, total)?;
        let text = &texts[i];
        let duration = cue.end().get() - cue.start().get();
        let effective = cue.end().get() - starts[i];
        let (cps, rs, rating) = metrics::rates(text.count, effective, text.empty);
        let row = CueMetrics {
            cue_id: cue.id().0.to_string(),
            cue_index: i as u32,
            start_ms: cue.start().get() as f64,
            end_ms: cue.end().get() as f64,
            duration_ms: duration as f64,
            reading_start_ms: starts[i] as f64,
            effective_duration_ms: effective as f64,
            characters: text.count,
            line_characters: text.lines.clone(),
            empty: text.empty,
            cps,
            rs,
            rating,
            next_cue_id: next[i].map(|j| cues[j].id().0.to_string()),
            gap_ms: next[i].map(|j| (cues[j].start().get() as i64 - cue.end().get() as i64) as f64),
        };
        check_cue(&row, text, profile, &mut report.findings);
        windows.check(&row, &mut report.findings);
        if profile.rules.gaps {
            if let Some(prior) = overlaps[i] {
                report.findings.push(Finding {
                    rule: Rule::Overlap, cue_id: cues[prior].id().0.to_string(), cue_index: prior as u32,
                    related_cue_id: Some(row.cue_id.clone()), severity: Severity::Violation,
                    measured: Some((cue.start().get() as i64 - cues[prior].end().get() as i64) as f64),
                    expected: "Non-overlapping half-open cue intervals".into(),
                    explanation: "A preceding cue remains visible when this cue starts; this representative pair also detects nested overlaps.".into(),
                });
            }
            if let Some(gap) = row.gap_ms
                && gap >= 0.0
                && gap < f64::from(profile.timing.cue_gap_ms)
            {
                let mut finding = finding(
                    &row,
                    Rule::Gap,
                    Some(gap),
                    format!("At least {} ms", profile.timing.cue_gap_ms),
                    "Gap to the next chronological cue is below the profile minimum.",
                );
                finding.related_cue_id.clone_from(&row.next_cue_id);
                report.findings.push(finding);
            }
        }
        if report.findings.len() > MAX_FINDINGS {
            return Err(Error::Limit(
                "Quality findings exceed 100000; narrow the enabled rules".into(),
            ));
        }
        report.metrics.push(row);
    }
    report.findings.sort_by_key(|f| f.cue_index);
    let affected: HashSet<&str> = report
        .findings
        .iter()
        .flat_map(|f| std::iter::once(f.cue_id.as_str()).chain(f.related_cue_id.as_deref()))
        .collect();
    report.affected_cues = affected.len() as u32;
    checkpoint(total, total)?;
    Ok(report)
}

fn finding(
    row: &CueMetrics,
    rule: Rule,
    measured: Option<f64>,
    expected: String,
    explanation: &str,
) -> Finding {
    Finding {
        rule,
        cue_id: row.cue_id.clone(),
        cue_index: row.cue_index,
        related_cue_id: None,
        severity: Severity::Violation,
        measured,
        expected,
        explanation: explanation.into(),
    }
}

fn check_cue(row: &CueMetrics, text: &TextMetrics, profile: &Profile, out: &mut Vec<Finding>) {
    let rules = &profile.rules;
    if rules.empty && row.empty {
        out.push(finding(
            row,
            Rule::Empty,
            Some(0.0),
            "Nonempty text under the selected counting policy".into(),
            "The counted text is empty or whitespace only.",
        ));
    }
    if rules.duration.enabled {
        if row.duration_ms < f64::from(rules.duration.minimum_ms) {
            out.push(finding(
                row,
                Rule::DurationShort,
                Some(row.duration_ms),
                format!("At least {} ms", rules.duration.minimum_ms),
                "Actual cue duration is below the inclusive minimum.",
            ));
        }
        if row.duration_ms > f64::from(rules.duration.maximum_ms) {
            out.push(finding(
                row,
                Rule::DurationLong,
                Some(row.duration_ms),
                format!("At most {} ms", rules.duration.maximum_ms),
                "Actual cue duration exceeds the inclusive maximum.",
            ));
        }
    }
    let s = &rules.speed;
    if s.enabled && !text.empty {
        let numerator = u64::from(text.count) * 1000;
        let duration = row.effective_duration_ms as u64;
        for (rate, denominator, min, max, low, high, exclusive, unit) in [
            (
                &row.cps,
                duration,
                s.minimum_cps,
                s.maximum_cps,
                Rule::CpsLow,
                Rule::CpsHigh,
                false,
                "CPS",
            ),
            (
                &row.rs,
                duration.saturating_sub(500),
                s.minimum_rs,
                s.maximum_rs_exclusive,
                Rule::RsLow,
                Rule::RsHigh,
                true,
                "RS",
            ),
        ] {
            if min.is_none() && max.is_none() {
                continue;
            }
            let Some(value) = rate.value() else {
                out.push(finding(row, Rule::SpeedUnavailable, None, format!("Finite {unit}"), "Effective reading duration is too short for this metric; it is not a passing result."));
                continue;
            };
            if let Some(min) = min
                && numerator < u64::from(min) * denominator
            {
                out.push(finding(
                    row,
                    low,
                    Some(value),
                    format!("At least {min} {unit}"),
                    "Unrounded speed is below the inclusive minimum.",
                ));
            }
            if let Some(max) = max {
                let bound = u64::from(max) * denominator;
                if numerator > bound || (exclusive && numerator == bound) {
                    out.push(finding(
                        row,
                        high,
                        Some(value),
                        format!(
                            "{} {max} {unit}",
                            if exclusive { "Below" } else { "At most" }
                        ),
                        "Unrounded speed exceeds the profile's permitted range.",
                    ));
                }
            }
        }
    }
    if rules.lines.enabled {
        let l = &rules.lines;
        if row.line_characters.len() > l.maximum_lines as usize {
            out.push(finding(
                row,
                Rule::LineCount,
                Some(row.line_characters.len() as f64),
                format!("At most {} lines", l.maximum_lines),
                "The cue contains more lines than permitted.",
            ));
        }
        let longest = row.line_characters.iter().copied().max().unwrap_or(0);
        if longest > l.maximum_characters {
            out.push(finding(
                row,
                Rule::LineLength,
                Some(f64::from(longest)),
                format!("At most {} UTF-16 units per line", l.maximum_characters),
                "The longest stripped line exceeds the strict character limit.",
            ));
        } else if row.line_characters.len() == 1 && longest > l.single_line_preferred {
            let mut f = finding(
                row,
                Rule::LineLength,
                Some(f64::from(longest)),
                format!("Prefer at most {} UTF-16 units", l.single_line_preferred),
                "The single line exceeds the preferred limit but remains within the strict limit.",
            );
            f.severity = Severity::Advisory;
            out.push(f);
        }
    }
}
