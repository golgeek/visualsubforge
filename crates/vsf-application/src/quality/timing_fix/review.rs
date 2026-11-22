//! Rust-owned projection shared by the desktop and headless review.
use super::*;

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Row {
    pub cue_id: String,
    pub cue_index: u32,
    pub cue_label: String,
    pub selected: bool,
    pub unit: Unit,
    pub delta: String,
    pub metric_effect: String,
    pub ghosts: Vec<Ghost>,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Ghost {
    pub cue_id: String,
    pub before: Endpoints,
    pub after: Endpoints,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ManualRow {
    pub cue_id: String,
    pub cue_number: u32,
    pub detail: Manual,
}
#[derive(Clone, Copy, Default, Debug, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Counts {
    pub gaps: u32,
    pub speed: u32,
    pub shots: u32,
    pub total: u32,
}
impl Counts {
    fn new(report: &Report, scope: &HashSet<CueId>) -> Self {
        let mut counts = Self::default();
        for f in &report.findings {
            let in_scope = |id: &str| id.parse().ok().is_some_and(|i| scope.contains(&CueId(i)));
            if !in_scope(&f.cue_id) && !f.related_cue_id.as_deref().is_some_and(in_scope) {
                continue;
            }
            match f.rule {
                Rule::Gap | Rule::Overlap => counts.gaps += 1,
                Rule::SceneStart | Rule::SceneEnd => counts.shots += 1,
                r if family(r, Families::default()) => counts.speed += 1,
                _ => continue,
            }
            counts.total += 1;
        }
        counts
    }
}
pub struct Review {
    pub changes: Vec<Row>,
    pub manual: Vec<ManualRow>,
    pub before: Counts,
    pub after: Counts,
    pub selected: u32,
    pub trades: u32,
    pub plan: timing_selection::Plan,
}
impl Proposal {
    pub fn review(
        &self,
        toggled: &[u32],
        omit_trades: bool,
        mut checkpoint: impl FnMut() -> Result<()>,
    ) -> Result<Review> {
        let toggles: HashSet<_> = toggled.iter().copied().collect();
        if toggles.len() != toggled.len()
            || self
                .units
                .iter()
                .filter(|u| toggles.contains(&u.index))
                .count()
                != toggles.len()
        {
            return Err(Error::Invalid(
                "Unknown or repeated checkbox identity".into(),
            ));
        }
        let mut selected = Vec::new();
        let mut changes = Vec::new();
        let mut trades = 0;
        for u in &self.units {
            checkpoint()?;
            let checked = (!(u.partial || omit_trades && u.trade_off)) ^ toggles.contains(&u.index);
            if checked {
                selected.push(u.index);
                trades += u32::from(u.trade_off);
            }
            let mut indices = vec![u.index];
            let mut deltas = Vec::new();
            for c in &u.endpoints {
                if c.index != u.index {
                    indices.push(c.index);
                }
                for (label, old, new) in [
                    ("start", c.before.start_ms, c.after.start_ms),
                    ("end", c.before.end_ms, c.after.end_ms),
                ] {
                    if old != new {
                        let prefix = if c.index != u.index {
                            format!("cue {} ", c.index + 1)
                        } else {
                            String::new()
                        };
                        deltas.push(format!("{prefix}{label} {:+} ms", new as i64 - old as i64));
                    }
                }
            }
            let i = u.index as usize;
            let mut metrics = Vec::new();
            for e in &u.effects {
                let name = if e.rule == Rule::CpsHigh { "CPS" } else { "RS" };
                let show = |n: Option<f64>| {
                    n.map_or_else(|| "unavailable".to_string(), |n| format!("{n:.1}"))
                };
                metrics.push(format!("{name} {} → {}", show(e.before), show(e.after)));
            }
            if metrics.is_empty() {
                for c in &u.endpoints {
                    metrics.push(format!(
                        "Duration {} → {} ms",
                        c.before.end_ms - c.before.start_ms,
                        c.after.end_ms - c.after.start_ms
                    ));
                }
            }
            changes.push(Row {
                cue_id: self.before.metrics[i].cue_id.clone(),
                cue_index: u.index,
                cue_label: indices
                    .iter()
                    .map(|v| (v + 1).to_string())
                    .collect::<Vec<_>>()
                    .join("→"),
                selected: checked,
                unit: u.clone(),
                delta: deltas.join(", "),
                metric_effect: metrics.join(" · "),
                ghosts: Vec::new(),
            });
        }
        let plan = self.select(&selected, &mut checkpoint)?;
        let chosen: HashSet<_> = selected.iter().copied().collect();
        for row in &mut changes {
            if row.selected {
                row.ghosts = row
                    .unit
                    .endpoints
                    .iter()
                    .map(|e| {
                        let cue = &plan.candidate.cues()[e.index as usize];
                        let original = &self.document.cues()[e.index as usize];
                        Ghost {
                            cue_id: cue.id().0.to_string(),
                            before: Endpoints {
                                start_ms: original.start().get(),
                                end_ms: original.end().get(),
                            },
                            after: Endpoints {
                                start_ms: cue.start().get(),
                                end_ms: cue.end().get(),
                            },
                        }
                    })
                    .collect();
            }
        }
        let report = plan.after(self.after());
        let positions = self
            .document
            .cues()
            .iter()
            .enumerate()
            .map(|(i, c)| (c.id(), i))
            .collect();
        let pending = unresolved_by_cue(
            report,
            &self.document,
            &self.scope,
            &positions,
            self.request.families,
        );
        let originals: HashMap<_, _> = self.manual.iter().map(|m| (m.index as usize, m)).collect();
        let mut manual = Vec::new();
        for (i, rules) in pending
            .into_iter()
            .enumerate()
            .filter(|(_, r)| !r.is_empty())
        {
            checkpoint()?;
            let m = &report.metrics[i];
            let mut detail = originals
                .get(&i)
                .map(|r| (*r).clone())
                .unwrap_or_else(|| Manual {
                    index: i as u32,
                    reason: "The change for this finding is unticked.".into(),
                    rules: vec![],
                    budget: None,
                });
            if let Some(old) = &detail.budget {
                detail.reason = detail
                    .reason
                    .trim_end_matches(&old.description())
                    .trim_end()
                    .to_owned();
            }
            detail.rules = rules;
            if detail
                .rules
                .iter()
                .any(|r| matches!(r, Rule::CpsHigh | Rule::RsHigh | Rule::SpeedUnavailable))
            {
                if !detail.budget.as_ref().is_some_and(|b| b.at_partial)
                    || chosen.contains(&(i as u32))
                {
                    detail.budget = Some(budget(
                        m,
                        Endpoints {
                            start_ms: m.start_ms as u64,
                            end_ms: m.end_ms as u64,
                        },
                        m.effective_duration_ms as u64,
                        false,
                        &self.before.profile,
                    ));
                }
            } else {
                detail.budget = None;
            }
            if let Some(b) = &detail.budget {
                detail.reason = format!("{} {}", detail.reason, b.description())
                    .trim()
                    .to_owned();
            }
            manual.push(ManualRow {
                cue_id: m.cue_id.clone(),
                cue_number: i as u32 + 1,
                detail,
            });
        }
        Ok(Review {
            changes,
            manual,
            before: Counts::new(&self.before, &self.scope),
            after: Counts::new(report, &self.scope),
            selected: selected.len() as u32,
            trades,
            plan,
        })
    }
}
