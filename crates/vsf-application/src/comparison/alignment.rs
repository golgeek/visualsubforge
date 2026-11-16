use super::*;
use std::{collections::HashMap, ops::Range};
type Group = (Range<usize>, Range<usize>);
pub(super) fn align(
    a: &[CueSnapshot],
    b: &[CueSnapshot],
    options: &Options,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<Vec<Group>> {
    let mut by_text: HashMap<String, Vec<usize>> = HashMap::new();
    for (j, c) in b.iter().enumerate() {
        checkpoint()?;
        by_text.entry(comparable(c, options)).or_default().push(j);
    }
    let mut groups = Vec::new();
    let (mut ai, mut bi) = (0, 0);
    for (i, c) in a.iter().enumerate() {
        checkpoint()?;
        let Some(candidates) = by_text.get(&comparable(c, options)) else {
            continue;
        };
        let start = candidates.partition_point(|&j| {
            j < bi || b[j].start_ms.saturating_add(options.tolerance_ms as u64) < c.start_ms
        });
        let anchor = candidates[start..]
            .iter()
            .copied()
            .take_while(|&j| {
                b[j].start_ms <= c.start_ms.saturating_add(options.tolerance_ms as u64)
            })
            .find(|&j| b[j].end_ms.abs_diff(c.end_ms) <= options.tolerance_ms as u64);
        if let Some(j) = anchor {
            anchored_gap(a, b, options, ai..i, bi..j, &mut groups, checkpoint)?;
            groups.push((i..i + 1, j..j + 1));
            ai = i + 1;
            bi = j + 1;
        }
    }
    anchored_gap(
        a,
        b,
        options,
        ai..a.len(),
        bi..b.len(),
        &mut groups,
        checkpoint,
    )?;
    Ok(groups)
}
fn anchored_gap(
    a: &[CueSnapshot],
    b: &[CueSnapshot],
    options: &Options,
    ar: Range<usize>,
    br: Range<usize>,
    out: &mut Vec<Group>,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<()> {
    if ar.is_empty() || br.is_empty() {
        return gap(a, b, ar, br, [0; 2], out, checkpoint);
    }
    let mut keys = |side: &[CueSnapshot]| -> Result<Vec<_>> {
        side.iter()
            .map(|c| {
                checkpoint()?;
                Ok((comparable(c, options), c.end_ms - c.start_ms))
            })
            .collect()
    };
    let (ak, bk) = (keys(&a[ar.clone()])?, keys(&b[br.clone()])?);
    let window = |expected: usize, bounds: Range<usize>| {
        bounds.start.max(expected.saturating_sub(OFFSET_RADIUS))
            ..bounds.end.min(expected + OFFSET_RADIUS + 1)
    };
    let (mut ai, mut bi) = (ar.start, br.start);
    let tolerance = u64::from(options.tolerance_ms);
    // The outer pass brackets this region using unshifted time.
    let mut confirmed = 0_i64;
    let mut tentative: Option<(usize, usize, i64)> = None;
    for i in ar.clone() {
        checkpoint()?;
        let expected = bi + (i - ai);
        let key = &ak[i - ar.start];
        let mut candidates = window(expected, bi..br.end).filter(|&j| &bk[j - br.start] == key);
        let Some(j) = candidates.next() else { continue };
        // Only a locally unambiguous match in both directions can establish a
        // new offset. Preserve tentative evidence across cuts and repeated lines.
        if candidates.next().is_some()
            || window(ai + (j - bi), ai..ar.end).any(|x| x != i && &ak[x - ar.start] == key)
        {
            continue;
        }
        let delta = b[j].start_ms as i64 - a[i].start_ms as i64;
        if confirmed.abs_diff(delta) > tolerance {
            let Some((x, y, d)) =
                tentative.filter(|&(_, y, d)| y < j && d.abs_diff(delta) <= tolerance)
            else {
                tentative = Some((i, j, delta));
                continue;
            };
            // The transition may contain edits from either section.
            gap(a, b, ai..x, bi..y, [confirmed, d], out, checkpoint)?;
            out.push((x..x + 1, y..y + 1));
            (ai, bi) = (x + 1, y + 1);
            confirmed = d;
        }
        gap(a, b, ai..i, bi..j, [confirmed; 2], out, checkpoint)?;
        out.push((i..i + 1, j..j + 1));
        (ai, bi) = (i + 1, j + 1);
        tentative = None;
    }
    // An exact anchor just beyond this gap is evidence for zero as well.
    let boundary = if ar.end < a.len() { 0 } else { confirmed };
    gap(
        a,
        b,
        ai..ar.end,
        bi..br.end,
        [confirmed, boundary],
        out,
        checkpoint,
    )
}
fn gap(
    a: &[CueSnapshot],
    b: &[CueSnapshot],
    ar: Range<usize>,
    br: Range<usize>,
    offsets: [i64; 2],
    out: &mut Vec<Group>,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<()> {
    let (n, m) = (ar.len(), br.len());
    if n == 0 {
        for j in br {
            out.push((ar.start..ar.start, j..j + 1));
        }
        return Ok(());
    }
    if m == 0 {
        for i in ar {
            out.push((i..i + 1, br.start..br.start));
        }
        return Ok(());
    }
    // A sparse dynamic program maximizes savings against deleting/adding every
    // cue. Fenwick prefix maxima find the best predecessor ending before j;
    // delayed publication at i+x prevents original-side group overlap.
    struct Node {
        group: Group,
        score: f64,
        variation_ms: u64,
        previous: Option<usize>,
    }
    impl Node {
        fn preferred_to(&self, other: &Self) -> bool {
            self.score > other.score
                || (self.score == other.score && self.variation_ms < other.variation_ms)
        }
    }
    fn best(tree: &[Option<usize>], nodes: &[Node], mut index: usize) -> Option<usize> {
        let mut result = None;
        while index > 0 {
            if let Some(candidate) = tree[index]
                && result.is_none_or(|old: usize| nodes[candidate].preferred_to(&nodes[old]))
            {
                result = Some(candidate);
            }
            index &= index - 1;
        }
        result
    }
    let mut tree = vec![None; m + 2];
    let mut due = vec![Vec::new(); n + 1];
    let mut nodes: Vec<Node> = Vec::new();
    let mut prefix_end = Vec::with_capacity(m);
    let mut end = 0;
    for cue in &b[br.clone()] {
        end = end.max(cue.end_ms);
        prefix_end.push(end);
    }
    let mut candidates = 0usize;
    for i in 0..=n {
        checkpoint()?;
        for node in &due[i] {
            let node: usize = *node;
            let mut index = nodes[node].group.1.end - br.start + 1;
            while index < tree.len() {
                if tree[index].is_none_or(|old: usize| nodes[node].preferred_to(&nodes[old])) {
                    tree[index] = Some(node);
                }
                index += index.isolate_lowest_one();
            }
        }
        if i == n {
            break;
        }
        let first = &a[ar.start + i];
        let end = a[ar.start + i..(ar.start + i + 3).min(ar.end)]
            .iter()
            .map(|c| c.end_ms)
            .max()
            .unwrap_or(first.end_ms);
        let revised = &b[br.clone()];
        // Split the fixed candidate budget across the two time windows at a
        // transition. Visit overlapping candidates once, in revised order.
        let two_windows = offsets[0] != offsets[1];
        let limit = if two_windows {
            DP_START_LIMIT / 2
        } else {
            DP_START_LIMIT
        };
        let window = |offset: i64| {
            let start = first.start_ms.saturating_add_signed(offset);
            let end = end.saturating_add_signed(offset);
            let low = prefix_end
                .partition_point(|e| e.saturating_add(CANDIDATE_WINDOW_MS) < start)
                .saturating_sub(2);
            let high =
                revised.partition_point(|c| c.start_ms <= end.saturating_add(CANDIDATE_WINDOW_MS));
            let center = revised.partition_point(|c| c.start_ms < start);
            let low = low.max(
                center
                    .saturating_sub(limit / 2)
                    .min(high.saturating_sub(limit)),
            );
            low..high.min(low + limit)
        };
        let left = window(offsets[0].min(offsets[1]));
        let right = if two_windows {
            window(offsets[0].max(offsets[1]))
        } else {
            0..0
        };
        let right = right.start.max(left.end)..right.end;
        for j in left.chain(right) {
            let previous = best(&tree, &nodes, j + 1);
            for (x, y) in [(1, 1), (1, 2), (1, 3), (2, 1), (3, 1)] {
                if i + x > n || j + y > m {
                    continue;
                }
                candidates += 1;
                if candidates.is_multiple_of(64) {
                    checkpoint()?;
                }
                let aa = &a[ar.start + i..ar.start + i + x];
                let bb = &b[br.start + j..br.start + j + y];
                if !offsets.iter().any(|&offset| close(aa, bb, offset)) {
                    continue;
                }
                let at: Vec<_> = aa.iter().map(TimedText::from).collect();
                let bt: Vec<_> = bb.iter().map(TimedText::from).collect();
                let (score, similarity) = scores(&at, &bt, &offsets);
                if score < MIN_PAIR_SCORE {
                    continue;
                }
                if x + y > 2
                    && (similarity < GROUP_TEXT_THRESHOLD
                        || !if x == 1 {
                            explains_parts(at[0], &bt)
                        } else {
                            explains_parts(bt[0], &at)
                        })
                {
                    continue;
                }
                let saving = GAP_COST * 2.0 - (1.0 - score + GROUP_PENALTY * (x + y - 2) as f64);
                if saving <= 0.0 {
                    continue;
                }
                let score = saving + previous.map_or(0.0, |p| nodes[p].score);
                // Equally good paths through repeated text can differ only in
                // their timing continuity (e.g. one duplicate inserted halfway).
                let delta = bb[0].start_ms as i64 - aa[0].start_ms as i64;
                let variation_ms = previous.map_or(0, |p| {
                    let pred = &nodes[p];
                    let prior_delta = b[pred.group.1.start].start_ms as i64
                        - a[pred.group.0.start].start_ms as i64;
                    pred.variation_ms
                        .saturating_add(delta.abs_diff(prior_delta))
                });
                due[i + x].push(nodes.len());
                nodes.push(Node {
                    group: (
                        ar.start + i..ar.start + i + x,
                        br.start + j..br.start + j + y,
                    ),
                    score,
                    variation_ms,
                    previous,
                });
            }
        }
    }
    let mut selected = Vec::new();
    let mut next = best(&tree, &nodes, m + 1);
    while let Some(index) = next {
        selected.push(nodes[index].group.clone());
        next = nodes[index].previous;
    }
    selected.reverse();
    let (mut ai, mut bi) = (ar.start, br.start);
    for (aa, bb) in selected {
        unmatched(a, b, ai..aa.start, bi..bb.start, out);
        ai = aa.end;
        bi = bb.end;
        out.push((aa, bb));
    }
    unmatched(a, b, ai..ar.end, bi..br.end, out);
    Ok(())
}
fn unmatched(
    a: &[CueSnapshot],
    b: &[CueSnapshot],
    ar: Range<usize>,
    br: Range<usize>,
    out: &mut Vec<Group>,
) {
    let (mut i, mut j) = (ar.start, br.start);
    while i < ar.end || j < br.end {
        if i < ar.end && (j == br.end || a[i].start_ms <= b[j].start_ms) {
            out.push((i..i + 1, j..j));
            i += 1;
        } else {
            out.push((i..i, j..j + 1));
            j += 1;
        }
    }
}
