//! Compare whole-document timing findings without treating old unrelated findings as new.
use super::{Finding, Report, Rule};
use std::collections::HashMap;

fn timing(rule: Rule) -> bool {
    !matches!(rule, Rule::Empty | Rule::LineCount | Rule::LineLength)
}
pub(super) type Findings<'a> = HashMap<(u32, Rule, Option<&'a str>, &'a str), Option<f64>>;
pub(super) fn timing_findings(report: &Report) -> Findings<'_> {
    report
        .findings
        .iter()
        .filter(|f| timing(f.rule))
        .map(|f| {
            (
                (
                    f.cue_index,
                    f.rule,
                    f.related_cue_id.as_deref(),
                    f.expected.as_str(),
                ),
                f.measured,
            )
        })
        .collect()
}
pub(super) fn worsened(finding: &Finding, old: &Findings<'_>) -> bool {
    if !timing(finding.rule) {
        return false;
    }
    let Some(before) = old.get(&(
        finding.cue_index,
        finding.rule,
        finding.related_cue_id.as_deref(),
        finding.expected.as_str(),
    )) else {
        return true;
    };
    match (before, finding.measured) {
        (Some(old), Some(new)) => match finding.rule {
            Rule::DurationShort | Rule::CpsLow | Rule::RsLow | Rule::Gap | Rule::Overlap => {
                new < *old
            }
            Rule::DurationLong | Rule::CpsHigh | Rule::RsHigh => new > *old,
            Rule::SceneStart | Rule::SceneEnd => new != *old,
            _ => false,
        },
        (None, None) => false,
        _ => true,
    }
}
