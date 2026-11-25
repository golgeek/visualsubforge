//! File-order projections built once for an immutable accepted report.
use super::*;
use crate::quality_check::{IssueIndex, RuleCues};
use engine::{Rule, Severity};

#[derive(Default)]
struct FilterRows {
    findings: Vec<u32>,
    cues: Vec<u32>,
}

pub(super) struct ReportIndex {
    filters: HashMap<(Option<Rule>, Option<Severity>), FilterRows>,
    by_id: HashMap<String, usize>,
    by_cue: Vec<Vec<u32>>,
    issues: IssueIndex,
    pub(super) rule_counts: Vec<RuleCues>,
    timing_count: u32,
}

impl ReportIndex {
    pub(super) fn build(
        report: &Report,
        mut checkpoint: impl FnMut() -> Result<()>,
    ) -> Result<Self> {
        let by_id: HashMap<_, _> = report
            .metrics
            .iter()
            .enumerate()
            .map(|(i, m)| (m.cue_id.clone(), i))
            .collect();
        let mut by_cue = vec![Vec::new(); report.metrics.len()];
        let mut severity = vec![None; report.metrics.len()];
        let mut filters = HashMap::<_, FilterRows>::new();
        let mut timing_count = 0;
        for (i, f) in report.findings.iter().enumerate() {
            if i % 256 == 0 {
                checkpoint()?;
            }
            timing_count += u32::from(engine::timing_fix::is_timing_finding(f.rule));
            let primary = by_id[&f.cue_id];
            let related = f
                .related_cue_id
                .as_ref()
                .map(|id| by_id[id])
                .filter(|i| *i != primary);
            for cue in std::iter::once(primary).chain(related) {
                by_cue[cue].push(i as u32);
                if severity[cue] != Some(Severity::Violation) {
                    severity[cue] = Some(f.severity);
                }
            }
            for rule in [None, Some(f.rule)] {
                for level in [None, Some(f.severity)] {
                    let rows = filters.entry((rule, level)).or_default();
                    rows.findings.push(i as u32);
                    rows.cues.push(primary as u32);
                    if let Some(cue) = related {
                        rows.cues.push(cue as u32);
                    }
                }
            }
        }
        let mut rule_counts = Vec::new();
        for (&(rule, level), rows) in &mut filters {
            checkpoint()?;
            rows.cues.sort_unstable();
            rows.cues.dedup();
            if let Some(rule) = rule
                && level.is_none()
            {
                rule_counts.push(RuleCues {
                    rule,
                    cues: rows.cues.len() as u32,
                });
            }
        }
        rule_counts.sort_by_key(|c| c.rule as u8);
        let mut issues = IssueIndex::default();
        for (row, level) in report.metrics.iter().zip(severity) {
            match level {
                Some(Severity::Violation) => issues.violations.push(row.cue_id.clone()),
                Some(Severity::Advisory) => issues.advisories.push(row.cue_id.clone()),
                None => {}
            }
        }
        Ok(Self {
            filters,
            by_id,
            by_cue,
            issues,
            rule_counts,
            timing_count,
        })
    }

    pub(super) fn project(&self, report: &Report, request: &Request, view: &mut View) {
        view.timing_findings_count = self.timing_count;
        view.rule_cues = self.rule_counts.clone();
        if request
            .index_since
            .is_some_and(|known| Some(known) != view.report_id)
        {
            view.issue_index = Some(self.issues.clone());
        }
        let selected = request
            .selected
            .and_then(|id| self.by_id.get(&id.get().to_string()).copied());
        if let Some(i) = selected {
            let row = &report.metrics[i];
            view.selected = Some(row.into());
            view.selected_issues = Some(cue_issues(
                self.by_cue[i].iter().map(|n| &report.findings[*n as usize]),
                &row.cue_id,
                request.selected_offset,
            ));
        }
        let Some(rows) = self.filters.get(&(request.rule, request.severity)) else {
            view.offset = 0;
            return;
        };
        view.filtered_count = rows.findings.len() as u32;
        view.filtered_cues = rows.cues.len() as u32;
        view.offset = page_offset(request.offset, view.filtered_count, request.limit);
        view.findings = rows
            .findings
            .iter()
            .skip(view.offset as usize)
            .take(request.limit as usize)
            .map(|i| report.findings[*i as usize].clone())
            .collect();
        let next = selected.map_or(0, |i| rows.cues.partition_point(|&c| c as usize <= i));
        let prev = selected.map_or(rows.cues.len(), |i| {
            rows.cues.partition_point(|&c| (c as usize) < i)
        });
        let id = |i: &u32| report.metrics[*i as usize].cue_id.clone();
        view.next_cue = rows.cues.get(next).or(rows.cues.first()).map(id);
        view.previous_cue = prev
            .checked_sub(1)
            .and_then(|i| rows.cues.get(i))
            .or(rows.cues.last())
            .map(id);
    }
}
