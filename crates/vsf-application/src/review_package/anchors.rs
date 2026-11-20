use super::{Anchor, SnapshotCue, Thread};
use crate::subtitle_markup::Projection;
use crate::{
    Result,
    comparison::{self, Comparison, Kind, TimedText},
};
use std::collections::BTreeMap;
use subtitle_formats::Format;

fn reading(side: &[SnapshotCue], format: Format) -> Vec<String> {
    side.iter()
        .map(|c| Projection::for_format(&c.text, format).visible)
        .collect()
}
fn timed<'a>(side: &[SnapshotCue], text: &'a [String]) -> Vec<TimedText<'a>> {
    side.iter()
        .zip(text)
        .map(|(c, t)| TimedText {
            start_ms: c.start_ms,
            end_ms: c.end_ms,
            text: t,
        })
        .collect()
}
/// Indices refer only to this comparison. Stored anchors never move.
pub fn reanchor(
    threads: &[Thread],
    comparison: &Comparison,
    original: Format,
    revised: Format,
    mut checkpoint: impl FnMut() -> Result<()>,
) -> Result<Vec<Option<usize>>> {
    let mut keys = BTreeMap::new();
    let mut timeline = Vec::new();
    for (index, c) in comparison.changes.iter().enumerate() {
        checkpoint()?;
        keys.entry(&c.key).or_insert(index);
        for cue in &c.revised {
            timeline.push((cue.start_ms, index))
        }
        if let Some(cue) = c.original.first() {
            timeline.push((cue.start_ms, index))
        }
    }
    timeline.sort_unstable();
    threads
        .iter()
        .map(|thread| {
            checkpoint()?;
            if let Anchor::Change { change_key, .. } = &thread.anchor
                && let Some(&index) = keys.get(change_key)
            {
                return Ok(Some(index));
            }
            let (a, b) = thread.anchor.sides();
            let (at, bt) = (reading(a, original), reading(b, revised));
            let (a, b) = (timed(a, &at), timed(b, &bt));
            let start = b
                .first()
                .or(a.first())
                .ok_or("Empty review anchor")?
                .start_ms;
            let window = comparison::CANDIDATE_WINDOW_MS;
            let first = timeline.partition_point(|&(time, _)| time < start.saturating_sub(window));
            let mut best = None;
            for &(time, index) in timeline[first..]
                .iter()
                .take_while(|(time, _)| *time <= start.saturating_add(window))
            {
                checkpoint()?;
                let c = &comparison.changes[index];
                let score = match &thread.anchor {
                    Anchor::Cue { .. } => c
                        .revised
                        .iter()
                        .map(|q| comparison::pair_score(&b, &[q.into()]))
                        .fold(0.0, f64::max),
                    Anchor::Change { .. }
                        if c.kind != Kind::Unchanged
                            && a.is_empty() == c.original.is_empty()
                            && b.is_empty() == c.revised.is_empty()
                            && [(&a, &c.original), (&b, &c.revised)].into_iter().any(
                                |(old, new)| {
                                    old.iter().any(|before| {
                                        new.iter().any(|after| {
                                            before.start_ms < after.end_ms
                                                && after.start_ms < before.end_ms
                                        })
                                    })
                                },
                            ) =>
                    {
                        let mut sum = 0.0;
                        for (old, new) in [(&a, &c.original), (&b, &c.revised)] {
                            if !old.is_empty() {
                                sum += comparison::pair_score(
                                    old,
                                    &new.iter().map(Into::into).collect::<Vec<_>>(),
                                )
                            }
                        }
                        sum / f64::from(u8::from(!a.is_empty()) + u8::from(!b.is_empty()))
                    }
                    _ => continue,
                };
                let candidate = (
                    score,
                    std::cmp::Reverse(time.abs_diff(start)),
                    std::cmp::Reverse(index),
                );
                if score >= comparison::MIN_PAIR_SCORE && best.is_none_or(|v| candidate > v) {
                    best = Some(candidate)
                }
            }
            Ok(best.map(|(_, _, i)| i.0))
        })
        .collect()
}
