//! For an edited review, checked rows describe the original document and the
//! accepted candidate. Unchecked rows preview ticking that row in this selection.
//! Only endpoints written by the resulting checkbox closure need replaying.
use super::*;

pub(super) struct SelectionEffects<'a> {
    proposal: &'a Proposal,
    selected: &'a HashSet<u32>,
    current: Vec<Endpoints>,
    original: Vec<Endpoints>,
    order: Vec<usize>,
    positions: Vec<usize>,
    units: HashMap<u32, &'a Unit>,
    children: HashMap<u32, Vec<u32>>,
    writers: Vec<Vec<(u32, &'a Change)>>,
}

impl<'a> SelectionEffects<'a> {
    pub(super) fn new(
        proposal: &'a Proposal,
        selected: &'a HashSet<u32>,
        candidate: &Document,
    ) -> Self {
        let original = endpoints(&proposal.document);
        let mut order: Vec<_> = (0..original.len()).collect();
        order.sort_unstable_by_key(|&i| (original[i].start_ms, i));
        let mut positions = vec![0; order.len()];
        for (position, &index) in order.iter().enumerate() {
            positions[index] = position;
        }
        let mut children = HashMap::<_, Vec<_>>::new();
        let mut writers = vec![Vec::new(); order.len()];
        for unit in &proposal.units {
            for &parent in &unit.depends_on {
                children.entry(parent).or_default().push(unit.index);
            }
            for change in &unit.endpoints {
                writers[change.index as usize].push((unit.index, change));
            }
        }
        Self {
            proposal,
            selected,
            current: endpoints(candidate),
            original,
            order,
            positions,
            children,
            writers,
            units: proposal.units.iter().map(|u| (u.index, u)).collect(),
        }
    }

    fn tick(&self, id: u32) -> HashMap<u32, bool> {
        let mut overrides = HashMap::new();
        let mut stack = vec![(id, true)];
        while let Some((id, on)) = stack.pop() {
            if overrides
                .get(&id)
                .copied()
                .unwrap_or_else(|| self.selected.contains(&id))
                == on
            {
                continue;
            }
            overrides.insert(id, on);
            if on {
                let unit = self.units[&id];
                stack.extend(unit.depends_on.iter().map(|&p| (p, true)));
                stack.extend(unit.conflicts_with.iter().map(|&c| (c, false)));
            } else if let Some(children) = self.children.get(&id) {
                stack.extend(children.iter().map(|&c| (c, false)));
            }
        }
        overrides
    }

    fn rate_duration(&self, index: usize, point: &impl Fn(usize) -> Endpoints) -> u64 {
        let pos = self.positions[index];
        let mut first = pos;
        if self.proposal.before.profile.speed_basis == SpeedBasis::VssOverlapV1 {
            while first > 0
                && point(self.order[first - 1]).end_ms > point(self.order[first]).start_ms
            {
                first -= 1;
            }
        }
        let mut prior = None;
        let mut duration = 0;
        for &cue in &self.order[first..=pos] {
            let p = point(cue);
            let start =
                metrics::reading_floor(self.proposal.before.profile.speed_basis, p.start_ms, prior)
                    .min(p.end_ms);
            duration = p.end_ms - start;
            prior = Some(metrics::ReadingPrior {
                start,
                end: p.end_ms,
                count: self.proposal.before.metrics[cue].characters,
            });
        }
        duration
    }

    pub(super) fn project(&self, source: &Unit, report: &Report) -> Unit {
        let checked = self.selected.contains(&source.index);
        let toggles = if checked {
            HashMap::new()
        } else {
            self.tick(source.index)
        };
        let before = |i: usize| {
            if checked {
                self.original[i]
            } else {
                self.current[i]
            }
        };
        let after = |i: usize| {
            if !self.writers[i]
                .iter()
                .any(|(id, _)| toggles.contains_key(id))
            {
                return self.current[i];
            }
            let mut point = self.original[i];
            for &(id, change) in &self.writers[i] {
                if toggles
                    .get(&id)
                    .copied()
                    .unwrap_or_else(|| self.selected.contains(&id))
                {
                    if change.before.start_ms != change.after.start_ms {
                        point.start_ms = change.after.start_ms;
                    }
                    if change.before.end_ms != change.after.end_ms {
                        point.end_ms = change.after.end_ms;
                    }
                }
            }
            point
        };
        let mut unit = source.clone();
        unit.effects_text.clear();
        unit.cut_timecodes.clear();
        for claim in &unit.claims {
            if let Some(next) = claim.related {
                let gap = |a: Endpoints, b: Endpoints| b.start_ms as i64 - a.end_ms as i64;
                let segment = effects::spacing(
                    gap(before(claim.index), before(next)),
                    gap(after(claim.index), after(next)),
                );
                if !unit.effects_text.contains(&segment) {
                    unit.effects_text.push(segment);
                }
            }
        }
        for change in &mut unit.endpoints {
            let index = change.index as usize;
            change.before = before(index);
            change.after = after(index);
            for (rule, label, a, b) in [
                (
                    Rule::SceneStart,
                    "starts",
                    change.before.start_ms,
                    change.after.start_ms,
                ),
                (
                    Rule::SceneEnd,
                    "ends",
                    change.before.end_ms,
                    change.after.end_ms,
                ),
            ] {
                if unit
                    .claims
                    .iter()
                    .any(|c| c.index == index && c.rule == rule)
                    && a != b
                    && let Some((segment, cut)) = effects::shot(
                        label,
                        a,
                        b,
                        &self.proposal.scenes,
                        &self.proposal.before.profile,
                    )
                {
                    unit.effects_text.push(segment);
                    if !unit.cut_timecodes.contains(&cut) {
                        unit.cut_timecodes.push(cut);
                    }
                }
            }
            if unit.claims.iter().any(|c| {
                c.index == index && matches!(c.rule, Rule::DurationShort | Rule::DurationLong)
            }) {
                let old = change.before.end_ms - change.before.start_ms;
                let new = change.after.end_ms - change.after.start_ms;
                unit.effects_text
                    .push(EffectSegment::plain(format!("duration {old} → {new} ms")));
            }
        }
        for effect in &mut unit.effects {
            let index = effect.index as usize;
            let current = &report.metrics[index];
            let original = &self.proposal.before.metrics[index];
            let values = |row: &crate::quality::CueMetrics| match if effect.rule == Rule::CpsHigh {
                &row.cps
            } else {
                &row.rs
            } {
                metrics::Rate::Finite { value } => Some(*value),
                _ => None,
            };
            effect.before = values(if checked { original } else { current });
            effect.after = if checked {
                values(current)
            } else {
                let (cps, rs, _) = metrics::rates(
                    original.characters,
                    self.rate_duration(index, &after),
                    original.empty,
                );
                match if effect.rule == Rule::CpsHigh {
                    cps
                } else {
                    rs
                } {
                    metrics::Rate::Finite { value } => Some(value),
                    _ => None,
                }
            };
        }
        unit.effects_text.extend(
            unit.effects
                .iter()
                .filter_map(|e| effects::speed(e, unit.index)),
        );
        unit
    }
}
