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

/// A partial writes no planned endpoint. Keep *all* alternative writers
/// on a read endpoint, since choosing either changes the search's assumptions.
pub(super) struct PartialReads {
    endpoints: Vec<[Vec<u32>; 2]>,
    frontier: Vec<u32>,
    reading: Vec<u32>,
}
impl PartialReads {
    pub fn new(count: usize) -> Self {
        Self {
            endpoints: vec![Default::default(); count],
            frontier: vec![],
            reading: vec![],
        }
    }
    pub fn record(&mut self, unit: &Unit) {
        for change in &unit.endpoints {
            let [start, end] = &mut self.endpoints[change.index as usize];
            if change.before.start_ms != change.after.start_ms {
                start.push(unit.index);
            }
            if change.before.end_ms != change.after.end_ms {
                end.push(unit.index);
            }
        }
    }
    pub fn read_by(
        &self,
        current: usize,
        next: Option<usize>,
        following: Option<usize>,
    ) -> Vec<u32> {
        let mut ids = self.reading.clone();
        ids.extend(&self.frontier);
        for index in Some(current).into_iter().chain(next) {
            for writers in &self.endpoints[index] {
                ids.extend(writers);
            }
        }
        if let Some(index) = following {
            ids.extend(&self.endpoints[index][0]);
        }
        ids.sort_unstable();
        ids.dedup();
        ids
    }
    pub fn advance(&mut self, index: usize, reaches_frontier: bool, inherits_reading: bool) {
        let [start, end] = &self.endpoints[index];
        if reaches_frontier {
            self.frontier.clone_from(end);
        }
        if !inherits_reading {
            self.reading.clear();
        }
        self.reading.extend(start);
        self.reading.extend(end);
        self.reading.sort_unstable();
        self.reading.dedup();
    }
}

pub(super) fn link_alternatives(units: &mut [Unit]) {
    if units.iter().all(|unit| unit.conflicts_with.is_empty()) {
        return;
    }
    let positions: HashMap<_, _> = units
        .iter()
        .enumerate()
        .map(|(position, unit)| (unit.index, position))
        .collect();
    let mut reverse = vec![Vec::new(); units.len()];
    for unit in units.iter_mut() {
        unit.conflicts_with.retain(|id| positions.contains_key(id));
        for id in &unit.conflicts_with {
            reverse[positions[id]].push(unit.index);
        }
    }
    for (unit, other) in units.iter_mut().zip(reverse) {
        unit.conflicts_with.extend(other);
        unit.conflicts_with.sort_unstable();
        unit.conflicts_with.dedup();
    }
}

/// A partial cannot replace a member of its own prerequisite closure.
/// Report it as manual instead of showing a checkbox that cannot stay ticked.
pub(super) fn withhold_incompatible_partials(
    units: &mut Vec<Unit>,
    withheld: &mut HashMap<usize, u32>,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<()> {
    if !units
        .iter()
        .any(|unit| unit.partial && !unit.conflicts_with.is_empty() && !unit.depends_on.is_empty())
    {
        return Ok(());
    }
    let lookup: HashMap<_, _> = units.iter().map(|unit| (unit.index, unit)).collect();
    let mut removed = HashSet::new();
    for partial in units
        .iter()
        .filter(|u| u.partial && !u.conflicts_with.is_empty())
    {
        let mut ancestors = partial.depends_on.clone();
        let mut seen = HashSet::new();
        while let Some(id) = ancestors.pop() {
            checkpoint()?;
            if partial.conflicts_with.binary_search(&id).is_ok() {
                removed.insert(partial.index);
                withheld.insert(partial.index as usize, id);
                break;
            }
            if seen.insert(id) {
                ancestors.extend_from_slice(&lookup[&id].depends_on);
            }
        }
    }
    units.retain_mut(|unit| {
        unit.conflicts_with.retain(|id| !removed.contains(id));
        !removed.contains(&unit.index)
    });
    Ok(())
}

pub struct CheckboxState {
    pub selected: Vec<u32>,
    /// Which active partial caused a row to turn off, including descendants.
    pub disabled_by: HashMap<u32, u32>,
}

fn untick(
    id: u32,
    partial: Option<u32>,
    children: &HashMap<u32, Vec<u32>>,
    selected: &mut HashSet<u32>,
    disabled_by: &mut HashMap<u32, u32>,
) {
    let mut stack = vec![id];
    while let Some(current) = stack.pop() {
        if selected.remove(&current) {
            if let Some(partial) = partial {
                disabled_by.insert(current, partial);
            }
            if let Some(dependents) = children.get(&current) {
                stack.extend_from_slice(dependents);
            }
        }
    }
}

/// Remove every member of an unsafe group and follow its dependency edges.
pub(super) fn withhold(
    units: &mut Vec<Unit>,
    groups: &[usize],
    unsafe_cues: &HashSet<usize>,
    withheld: &mut HashMap<usize, u32>,
) {
    let bad_groups: HashSet<_> = unsafe_cues.iter().map(|&index| groups[index]).collect();
    let mut children = HashMap::<u32, Vec<u32>>::new();
    let mut stack = Vec::new();
    for unit in units.iter() {
        for &parent in &unit.depends_on {
            children.entry(parent).or_default().push(unit.index);
        }
        if unit
            .endpoints
            .iter()
            .any(|change| bad_groups.contains(&groups[change.index as usize]))
        {
            stack.push((unit.index, unit.index));
        }
    }
    let mut removed = HashMap::new();
    while let Some((id, root)) = stack.pop() {
        if let std::collections::hash_map::Entry::Vacant(entry) = removed.entry(id) {
            entry.insert(root);
            if let Some(ids) = children.get(&id) {
                stack.extend(ids.iter().map(|&child| (child, root)));
            }
        }
    }
    units.retain(|unit| {
        let Some(&root) = removed.get(&unit.index) else {
            return true;
        };
        withheld.insert(unit.index as usize, root);
        for change in &unit.endpoints {
            withheld.insert(change.index as usize, root);
        }
        false
    });
}

impl Proposal {
    pub fn review_selection(
        &self,
        toggled: &[u32],
        omit_trades: bool,
        checkpoint: impl FnMut() -> Result<()>,
    ) -> Result<(CheckboxState, timing_selection::Plan)> {
        let state = self.checkbox_state(toggled, omit_trades)?;
        let plan = self.select(&state.selected, checkpoint)?;
        Ok((state, plan))
    }

    /// Replay user actions in order. Repeating a checkbox reverses its current state.
    pub fn selected_units(&self, toggled: &[u32], omit_trades: bool) -> Result<Vec<u32>> {
        Ok(self.checkbox_state(toggled, omit_trades)?.selected)
    }
    /// Actions to append to the current history. A cascade can remove later
    /// trade-offs, so only toggle a row that is still selected.
    pub fn trade_toggles(&self, current: &HashSet<u32>) -> Vec<u32> {
        let mut selected = current.clone();
        let mut actions = Vec::new();
        let mut children = HashMap::<u32, Vec<u32>>::new();
        for unit in &self.units {
            for &parent in &unit.depends_on {
                children.entry(parent).or_default().push(unit.index);
            }
        }
        let mut disabled_by = HashMap::new();
        for unit in &self.units {
            if unit.trade_off && selected.contains(&unit.index) {
                actions.push(unit.index);
                untick(unit.index, None, &children, &mut selected, &mut disabled_by);
            }
        }
        actions
    }

    pub fn checkbox_state(&self, toggled: &[u32], omit_trades: bool) -> Result<CheckboxState> {
        let mut selected = HashSet::new();
        let mut disabled_by = HashMap::new();
        let mut children = HashMap::<u32, Vec<u32>>::new();
        let lookup: HashMap<_, _> = self.units.iter().map(|unit| (unit.index, unit)).collect();
        for unit in &self.units {
            for &id in &unit.depends_on {
                children.entry(id).or_default().push(unit.index);
            }
            if !unit.partial {
                selected.insert(unit.index);
            }
        }
        if omit_trades {
            for unit in self.units.iter().filter(|unit| unit.trade_off) {
                untick(unit.index, None, &children, &mut selected, &mut disabled_by);
            }
        }
        for &id in toggled {
            let requested = lookup
                .get(&id)
                .ok_or_else(|| Error::Invalid("Unknown timing unit".into()))?;
            if selected.contains(&id) {
                untick(id, None, &children, &mut selected, &mut disabled_by);
                continue;
            }
            let mut stack = vec![id];
            while let Some(index) = stack.pop() {
                let unit = lookup[&index];
                if selected.insert(index) {
                    disabled_by.remove(&index);
                    for &conflict in &unit.conflicts_with {
                        untick(
                            conflict,
                            requested.partial.then_some(id),
                            &children,
                            &mut selected,
                            &mut disabled_by,
                        );
                    }
                    stack.extend_from_slice(&unit.depends_on);
                }
            }
        }
        disabled_by.retain(|_, partial| selected.contains(partial));
        Ok(CheckboxState {
            selected: self
                .units
                .iter()
                .map(|unit| unit.index)
                .filter(|id| selected.contains(id))
                .collect(),
            disabled_by,
        })
    }
}

/// Reverting one endpoint can reactivate an original overlap even when the
/// planned cues are separated. A reader and its repair then support each
/// other. Link local writers in a chain so storage stays linear in group size.
pub(super) fn couple_readers(
    document: &Document,
    points: &[Endpoints],
    order: &[usize],
    units: &mut [Unit],
) {
    let mut group = vec![0; points.len()];
    let mut sizes = Vec::new();
    let mut frontier = 0;
    for &index in order {
        let original = &document.cues()[index];
        let start = original.start().get().min(points[index].start_ms);
        if sizes.is_empty() || start >= frontier {
            sizes.push(0);
        }
        let id = sizes.len() - 1;
        sizes[id] += 1;
        group[index] = id;
        frontier = frontier.max(original.end().get()).max(points[index].end_ms);
    }
    let mut previous = vec![None; sizes.len()];
    let mut edges = Vec::new();
    for (position, unit) in units.iter().enumerate().filter(|(_, unit)| !unit.partial) {
        for change in &unit.endpoints {
            let id = group[change.index as usize];
            if sizes[id] > 1 {
                if let Some(prior) = previous[id]
                    && prior != position
                {
                    edges.push((prior, position));
                }
                previous[id] = Some(position);
            }
        }
    }
    for (earlier, later) in edges {
        let before = units[earlier].index;
        let after = units[later].index;
        units[earlier].depends_on.push(after);
        units[later].depends_on.push(before);
    }
    for unit in units {
        unit.depends_on.sort_unstable();
        unit.depends_on.dedup();
    }
}
