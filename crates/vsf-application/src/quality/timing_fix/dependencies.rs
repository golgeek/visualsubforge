//! Provenance follows the same chronological reads as the candidate search.
use super::*;

#[derive(Clone, Copy, Default)]
struct Writers {
    start: Option<u32>,
    end: Option<u32>,
}

pub(super) struct Provenance {
    endpoints: Vec<Writers>,
    frontier: Option<u32>,
    reading: Vec<u32>,
}

impl Provenance {
    pub fn new(count: usize) -> Self {
        Self {
            endpoints: vec![Writers::default(); count],
            frontier: None,
            reading: Vec::new(),
        }
    }

    pub fn prerequisites(
        &self,
        current: usize,
        next: Option<usize>,
        following: Option<usize>,
    ) -> Vec<u32> {
        let mut ids = self.reading.clone();
        ids.extend(self.frontier);
        for index in Some(current).into_iter().chain(next) {
            let writers = self.endpoints[index];
            ids.extend(writers.start);
            ids.extend(writers.end);
        }
        ids.extend(following.and_then(|index| self.endpoints[index].start));
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    pub fn applied(&mut self, unit: &Unit) {
        for change in &unit.endpoints {
            let writers = &mut self.endpoints[change.index as usize];
            if change.before.start_ms != change.after.start_ms {
                writers.start = Some(unit.index);
            }
            if change.before.end_ms != change.after.end_ms {
                writers.end = Some(unit.index);
            }
        }
    }

    pub fn advance(&mut self, index: usize, reaches_frontier: bool, inherits_reading: bool) {
        let writers = self.endpoints[index];
        if reaches_frontier {
            self.frontier = writers.end;
        }
        if !inherits_reading {
            self.reading.clear();
        }
        self.reading.extend(writers.start);
        self.reading.extend(writers.end);
        self.reading.sort_unstable();
        self.reading.dedup();
    }
}

/// Units are stored in planning order, so all prerequisites precede their dependents.
pub(super) fn withhold(
    units: &mut Vec<Unit>,
    groups: &[usize],
    unsafe_cues: &HashSet<usize>,
    withheld: &mut HashMap<usize, u32>,
) {
    let bad_groups: HashSet<_> = unsafe_cues.iter().map(|&index| groups[index]).collect();
    let mut removed = HashMap::new();
    units.retain(|unit| {
        let root = unit
            .depends_on
            .iter()
            .find_map(|id| removed.get(id).copied())
            .or_else(|| {
                unit.endpoints
                    .iter()
                    .any(|change| bad_groups.contains(&groups[change.index as usize]))
                    .then_some(unit.index)
            });
        if let Some(root) = root {
            removed.insert(unit.index, root);
            withheld.insert(unit.index as usize, root);
            for change in &unit.endpoints {
                withheld.insert(change.index as usize, root);
            }
            false
        } else {
            true
        }
    });
}

impl Proposal {
    pub fn review_selection(
        &self,
        toggled: &[u32],
        omit_trades: bool,
        checkpoint: impl FnMut() -> Result<()>,
    ) -> Result<(Vec<u32>, timing_selection::Plan)> {
        let selected = self.selected_units(toggled, omit_trades)?;
        let plan = self.select(&selected, checkpoint)?;
        Ok((selected, plan))
    }

    /// Replay user actions in order. Repeating a checkbox reverses its current state.
    pub fn selected_units(&self, toggled: &[u32], omit_trades: bool) -> Result<Vec<u32>> {
        let mut selected = HashSet::new();
        let mut parents = HashMap::new();
        let mut children = HashMap::<u32, Vec<u32>>::new();
        for unit in &self.units {
            parents.insert(unit.index, unit.depends_on.as_slice());
            for &id in &unit.depends_on {
                children.entry(id).or_default().push(unit.index);
            }
            if !(unit.partial || omit_trades && unit.trade_off)
                && unit.depends_on.iter().all(|id| selected.contains(id))
            {
                selected.insert(unit.index);
            }
        }
        for &id in toggled {
            if !parents.contains_key(&id) {
                return Err(Error::Invalid("Unknown timing unit".into()));
            }
            let tick = !selected.contains(&id);
            let mut stack = vec![id];
            while let Some(unit) = stack.pop() {
                if tick {
                    if selected.insert(unit) {
                        stack.extend_from_slice(parents[&unit]);
                    }
                } else if selected.remove(&unit)
                    && let Some(dependents) = children.get(&unit)
                {
                    stack.extend_from_slice(dependents);
                }
            }
        }
        Ok(self
            .units
            .iter()
            .map(|unit| unit.index)
            .filter(|id| selected.contains(id))
            .collect())
    }
}
