use super::*;
use std::collections::HashMap;

/// Lightweight identities for filtering the virtual list. Diff tokens, cue text
/// and discussion bodies stay in the separately bounded page and projection.
#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(super) struct Presentation {
    rows: Vec<ReviewIndexEntry>,
    overview: engine::Overview,
}
#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
struct ReviewIndexEntry {
    key: String,
    occurrence: u32,
    index: u32,
    kind: engine::Kind,
    source_index: u32,
    revised: Vec<SafeU64>,
    next_cue: Option<SafeU64>,
    has_threads: bool,
    open: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    band_change: Option<String>,
    block: Option<u32>,
    #[specta(type=Option<f64>)]
    shift_ms: Option<i64>,
}
pub(super) fn build(
    c: &engine::Comparison,
    index: &ThreadIndex,
    discussion: &Discussion,
) -> Result<Presentation> {
    let mut rows = Vec::new();
    let mut occurrences = HashMap::new();
    let mut blocks = c.blocks.iter().peekable();
    let mut removals = Vec::<usize>::new();
    for (i, change) in c.changes.iter().enumerate() {
        if let Some(q) = change.revised.first() {
            for pending in removals.drain(..) {
                let row: &mut ReviewIndexEntry = &mut rows[pending];
                row.next_cue = Some(q.cue_id.try_into()?);
            }
        }
        let threads = index.row(i);
        if change.kind == engine::Kind::Unchanged && threads.is_empty() {
            continue;
        }
        while blocks.peek().is_some_and(|b| b.end as usize <= i) {
            blocks.next();
        }
        let block = blocks.peek().filter(|b| b.first as usize <= i);
        let occurrence = occurrences.entry(&change.key).or_insert(0);
        if change.kind == engine::Kind::Removed {
            removals.push(rows.len());
        }
        rows.push(ReviewIndexEntry {
            key: change.key.clone(),
            occurrence: *occurrence,
            index: i as u32,
            kind: change.kind,
            source_index: change
                .original
                .first()
                .or(change.revised.first())
                .ok_or("Empty comparison group")?
                .source_index,
            revised: change
                .revised
                .iter()
                .map(|q| SafeU64::try_from(q.cue_id).map_err(str::to_string))
                .collect::<Result<_>>()?,
            next_cue: None,
            has_threads: !threads.is_empty(),
            open: threads.iter().any(|t| !discussion.threads[*t].resolved()),
            band_change: band_change(change),
            block: block.map(|b| b.first),
            shift_ms: block.map(|b| b.delta_ms),
        });
        *occurrence += 1;
    }
    Ok(Presentation {
        rows,
        overview: engine::overview(&c.changes),
    })
}

fn band_change(c: &engine::Change) -> Option<String> {
    fn labels(side: &[engine::CueSnapshot]) -> Option<String> {
        if side.is_empty() {
            return None;
        }
        side.iter()
            .map(|q| {
                q.reading_speed.value()?;
                vsf_application::quality::metrics::rates(q.characters, q.end_ms - q.start_ms, false)
                    .2
                    .map(|r| r.label())
            })
            .collect::<Option<Vec<_>>>()
            .map(|labels| labels.join(" / "))
    }
    let a = labels(&c.original)?;
    let b = labels(&c.revised)?;
    (a != b).then(|| format!("{a} → {b}"))
}
