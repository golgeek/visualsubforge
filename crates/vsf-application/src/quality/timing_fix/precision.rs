//! Prove which candidate graph constraints matter to a chosen timing. Every
//! checker predicate belongs to a cue, its reading predecessors, or its next
//! start. Choices outside that neighbourhood cannot change the predicate.
use super::*;
use search::{Bounds, violations};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
enum Edge {
    Needs(usize, usize),
    Excludes(usize, usize),
}

impl Edge {
    fn ends(self) -> (usize, usize) {
        match self {
            Self::Needs(a, b) | Self::Excludes(a, b) => (a, b),
        }
    }
    fn forbids(self, member: impl Fn(usize) -> bool) -> bool {
        let (a, b) = self.ends();
        member(a)
            && match self {
                Self::Needs(..) => !member(b),
                Self::Excludes(..) => member(b),
            }
    }
}

struct Neighbourhood<'a> {
    inputs: &'a PlanningInputs<'a>,
    order: &'a [usize],
    position: usize,
    next: Option<usize>,
    bounds: Bounds,
    writers: &'a [usize],
    units: &'a [Unit],
}

impl Neighbourhood<'_> {
    fn state(&self, points: &[Endpoints]) -> Option<([(Rule, u64); 12], u64)> {
        let index = self.order[self.position];
        let current = points[self.position];
        let original = &self.inputs.document.cues()[index];
        let mut prior = None;
        let mut frontier = 0;
        for (&cue, &point) in self.order[..=self.position].iter().zip(points) {
            if point.start_ms >= point.end_ms {
                return None;
            }
            let floor = metrics::reading_floor(
                self.inputs.context.profile.speed_basis,
                point.start_ms,
                prior,
            )
            .min(point.end_ms);
            if cue == index {
                if current.start_ms < original.start().get() && current.start_ms < frontier {
                    return None;
                }
                let next_start = self.next.map(|_| points[self.position + 1].start_ms);
                if let Some(next) = self.next {
                    let old_gap = self.inputs.document.cues()[next].start().get() as i64
                        - original.end().get() as i64;
                    let gap = next_start.unwrap() as i64 - current.end_ms as i64;
                    if (current.start_ms, index) > (next_start.unwrap(), next)
                        || gap < old_gap.min(self.inputs.context.profile.timing.cue_gap_ms.into())
                        || (gap < 0 && current.end_ms > original.end().get())
                    {
                        return None;
                    }
                }
                let effective = current.end_ms - floor;
                return Some((
                    violations(
                        current,
                        effective,
                        next_start,
                        self.bounds,
                        self.inputs.context.profile,
                        self.inputs.windows,
                    )
                    .entries(),
                    effective,
                ));
            }
            prior = Some(metrics::ReadingPrior {
                start: floor,
                end: point.end_ms,
                count: self.inputs.before.metrics[cue].characters,
            });
            frontier = frontier.max(point.end_ms);
        }
        unreachable!("neighbourhood contains the checked cue")
    }

    fn apply(&self, writer: usize, points: &mut [Endpoints]) {
        for change in &self.units[writer].endpoints {
            if let Some(pos) = self.order.iter().position(|&i| i == change.index as usize) {
                if change.before.start_ms != change.after.start_ms {
                    points[pos].start_ms = change.after.start_ms;
                }
                if change.before.end_ms != change.after.end_ms {
                    points[pos].end_ms = change.after.end_ms;
                }
            }
        }
    }

    fn clauses(&self, original: &[Endpoints], edges: &[Edge], constraints: &mut Vec<Vec<Edge>>) {
        let index = self.order[self.position];
        let baseline = self
            .state(original)
            .expect("original geometry is admissible")
            .0;
        let mut planned = original.to_vec();
        let mut claims = vec![0u16; self.writers.len()];
        for (bit, &writer) in self.writers.iter().enumerate() {
            let before = self.state(&planned);
            let mut candidate = planned.clone();
            self.apply(writer, &mut candidate);
            if let (Some((before, _)), Some((after, _))) = (before, self.state(&candidate)) {
                for (k, (&(rule, old), &(_, new))) in before.iter().zip(&after).enumerate() {
                    if old > 0 && new == 0 && self.units[writer].fixed.contains(&rule) {
                        claims[bit] |= 1 << k;
                    }
                }
            }
            if !self.units[writer].partial {
                planned = candidate;
            }
        }
        let mut points = original.to_vec();
        for mask in 1..1usize << self.writers.len() {
            points.copy_from_slice(original);
            let mut required = 0;
            let mut ceilings = [0f64; 2];
            for (bit, &writer) in self
                .writers
                .iter()
                .enumerate()
                .filter(|(bit, _)| mask & (1 << bit) != 0)
            {
                self.apply(writer, &mut points);
                required |= claims[bit];
                if self.units[writer].trade_off {
                    for e in self.units[writer]
                        .effects
                        .iter()
                        .filter(|e| e.index as usize == index && e.after > e.before)
                    {
                        if let Some(rate) = e.after {
                            let slot = usize::from(e.rule == Rule::RsHigh);
                            ceilings[slot] = ceilings[slot].max(rate);
                        }
                    }
                }
            }
            let safe = self.state(&points).is_some_and(|(after, effective)| {
                after.iter().zip(&baseline).enumerate().all(
                    |(k, (&(rule, severity), &(_, old)))| {
                        if required & (1 << k) != 0 && severity > 0 {
                            return false;
                        }
                        if severity <= old {
                            return true;
                        }
                        if !matches!(rule, Rule::CpsHigh | Rule::RsHigh) {
                            return false;
                        }
                        let rs = rule == Rule::RsHigh;
                        let available = effective.saturating_sub(if rs { 500 } else { 0 });
                        available > 0
                            && f64::from(self.inputs.before.metrics[index].characters) * 1000.
                                / available as f64
                                <= ceilings[usize::from(rs)]
                    },
                )
            });
            if !safe {
                let forbids: Vec<_> = edges
                    .iter()
                    .copied()
                    .filter(|edge| {
                        edge.forbids(|w| {
                            self.writers
                                .binary_search(&w)
                                .is_ok_and(|bit| mask & (1 << bit) != 0)
                        })
                    })
                    .collect();
                if !forbids.is_empty() {
                    constraints.push(forbids);
                }
            }
        }
    }
}

pub(super) fn refine(
    inputs: &PlanningInputs<'_>,
    order: &[usize],
    units: &mut [Unit],
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<()> {
    let original = endpoints(inputs.document);
    let by_id: HashMap<_, _> = units
        .iter()
        .enumerate()
        .map(|(i, u)| (u.index, i))
        .collect();
    let mut written = vec![Vec::new(); order.len()];
    let mut envelope = original.clone();
    let mut candidates = vec![Vec::new(); units.len()];
    for (writer, unit) in units.iter().enumerate() {
        for change in &unit.endpoints {
            let cue = change.index as usize;
            written[cue].push(writer);
            envelope[cue].start_ms = envelope[cue].start_ms.min(change.after.start_ms);
            envelope[cue].end_ms = envelope[cue].end_ms.max(change.after.end_ms);
        }
        for edge in unit
            .depends_on
            .iter()
            .map(|id| Edge::Needs(writer, by_id[id]))
            .chain(unit.conflicts_with.iter().map(|id| {
                let other = by_id[id];
                Edge::Excludes(writer.min(other), writer.max(other))
            }))
        {
            let (a, b) = edge.ends();
            candidates[a].push(edge);
            candidates[b].push(edge);
        }
    }
    let mut keep = HashSet::new();
    let mut constraints = Vec::new();
    let mut first = 0;
    let mut frontier = 0;
    for (pos, &cue) in order.iter().enumerate() {
        checkpoint()?;
        if envelope[cue].start_ms >= frontier {
            first = pos;
        }
        frontier = frontier.max(envelope[cue].end_ms);
        let next = order.get(pos + 1).copied();
        let end = pos + 1 + usize::from(next.is_some());
        // Bound both reading-chain traversal and subset enumeration. Dense
        // ambiguous neighbourhoods keep their existing conservative links.
        if pos - first > 24 {
            for &writer in &written[cue] {
                keep.extend(&candidates[writer]);
            }
            continue;
        }
        let local = &order[first..end];
        let mut writers: Vec<_> = local
            .iter()
            .flat_map(|&i| written[i].iter().copied())
            .collect();
        writers.sort_unstable();
        writers.dedup();
        if writers.len() < 2 {
            continue;
        }
        let mut edges: Vec<_> = writers
            .iter()
            .flat_map(|&w| candidates[w].iter().copied())
            .filter(|e| {
                let (a, b) = e.ends();
                writers.binary_search(&a).is_ok() && writers.binary_search(&b).is_ok()
            })
            .collect();
        edges.sort_unstable();
        edges.dedup();
        if edges.is_empty() {
            continue;
        }
        if writers.len() > 8 {
            keep.extend(edges);
            continue;
        }
        let original_points: Vec<_> = local.iter().map(|&i| original[i]).collect();
        Neighbourhood {
            inputs,
            order: local,
            position: pos - first,
            next,
            bounds: Bounds::new(&inputs.before.metrics[cue], inputs.context.profile),
            writers: &writers,
            units,
        }
        .clauses(&original_points, &edges, &mut constraints);
    }
    // Every inadmissible local assignment needs at least one blocking edge.
    // Prefer forced choices first; identical witnesses reuse the same edge.
    constraints.sort_by_key(Vec::len);
    for choices in constraints {
        if !choices.iter().any(|e| keep.contains(e)) {
            keep.insert(choices[0]);
        }
    }
    for (writer, unit) in units.iter_mut().enumerate() {
        unit.depends_on
            .retain(|id| keep.contains(&Edge::Needs(writer, by_id[id])));
        unit.conflicts_with.retain(|id| {
            let other = by_id[id];
            keep.contains(&Edge::Excludes(writer.min(other), writer.max(other)))
        });
    }
    Ok(())
}
