//! Deterministic, bounded subtitle comparison. Immutable captures only; no IO.
mod alignment;
mod overview;
pub use overview::{Overview, kind_index, overview};
mod report;
pub(crate) use report::{change_ids, timecode};
mod words;
use crate::{Result, quality::metrics, subtitle_markup::Projection};
pub use report::{ExtraSections, ReportOptions, escape_html, html};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use subtitle_core::Cue;
use subtitle_formats::Format;
pub use words::{TokenChange, TokenKind, word_diff};

// Time is a useful prior; text distinguishes simultaneous dialogue and splits.
pub const TIME_WEIGHT: f64 = 0.4;
// Reading text distinguishes simultaneous dialogue.
pub const TEXT_WEIGHT: f64 = 0.6;
// Limit the timeline distance for unanchored alignment.
pub const CANDIDATE_WINDOW_MS: u64 = 10_000;
// Discard weak pairings before allocating a DP node.
pub const MIN_PAIR_SCORE: f64 = 0.25;
// Require the joined parts to reproduce most of the single text.
pub const GROUP_TEXT_THRESHOLD: f64 = 0.75;
// Require the union of the parts to cover half the single interval.
pub const GROUP_SPAN_THRESHOLD: f64 = 0.5;
// Cost of leaving a cue unmatched.
pub const GAP_COST: f64 = 0.65;
// Each additional part must improve the match enough to justify grouping.
pub const GROUP_PENALTY: f64 = 0.1;
// Each part must contribute mostly words from the single cue.
const PART_CONTAINMENT: f64 = 0.75;
// Search nearby positions for displaced anchors after insertions or deletions.
const OFFSET_RADIUS: usize = 8;
// Limit candidate starts per original cue in ambiguous dense regions.
const DP_START_LIMIT: usize = 16;
// Two successive retimes remain individual rows; three can describe a scene.
pub const MIN_SHIFT_BLOCK: usize = 3;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct Options {
    pub tolerance_ms: u32,
    pub ignore_tags: bool,
    pub ignore_line_breaks: bool,
    pub ignore_whitespace: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            tolerance_ms: 1,
            ignore_tags: false,
            ignore_line_breaks: false,
            ignore_whitespace: true,
        }
    }
}
impl Options {
    pub fn for_format(format: Format) -> Self {
        Self {
            tolerance_ms: if matches!(format, Format::Ass | Format::Ssa) {
                10
            } else {
                1
            },
            ..Self::default()
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Unchanged,
    Text,
    Timing,
    TextAndTiming,
    Added,
    Removed,
    Split,
    Merged,
    Style,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct CueSnapshot {
    pub source_index: u32,
    #[cfg_attr(feature = "specta", specta(type = u32))]
    pub cue_id: u64,
    #[cfg_attr(feature = "specta", specta(type = u32))]
    pub start_ms: u64,
    #[cfg_attr(feature = "specta", specta(type = u32))]
    pub end_ms: u64,
    pub text: String,
    pub reading: String,
    pub style: Option<String>,
    pub actor: Option<String>,
    pub characters: u32,
    pub cps: metrics::Rate,
    pub reading_speed: metrics::Rate,
}
impl CueSnapshot {
    pub fn duration_rating(&self) -> Option<metrics::Rating> {
        self.reading_speed.value()?;
        metrics::rates(self.characters, self.end_ms - self.start_ms, false).2
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Change {
    pub key: String,
    pub kind: Kind,
    pub original: Vec<CueSnapshot>,
    pub revised: Vec<CueSnapshot>,
    pub words: Vec<TokenChange>,
    pub text_changed: bool,
    pub timing_changed: bool,
    pub style_changed: bool,
}
impl Change {
    pub fn band_change(&self) -> Option<String> {
        fn labels(cues: &[CueSnapshot]) -> String {
            cues.iter()
                .map(|cue| {
                    cue.duration_rating()
                        .map_or("Not rated", metrics::Rating::label)
                })
                .collect::<Vec<_>>()
                .join(" / ")
        }
        if self.original.is_empty() || self.revised.is_empty() {
            return None;
        }
        let before = labels(&self.original);
        let after = labels(&self.revised);
        (before != after).then(|| format!("{before} → {after}"))
    }
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Statistics {
    pub original_cues: u32,
    pub changes: u32,
    pub added_cues: u32,
    pub text_cues: u32,
    pub timing_cues: u32,
    pub style_cues: u32,
    pub changed_cues: u32,
    pub text_percent: f64,
    pub timing_percent: f64,
    pub total_percent: f64,
}
/// Half-open range in Comparison.changes, including unchanged rows in the indexing.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ShiftBlock {
    pub first: u32,
    pub end: u32,
    #[cfg_attr(feature = "specta", specta(type = i32))]
    pub delta_ms: i64,
    #[cfg_attr(feature = "specta", specta(type = u32))]
    pub start_ms: u64,
    #[cfg_attr(feature = "specta", specta(type = u32))]
    pub end_ms: u64,
    #[cfg_attr(feature = "specta", specta(type = u32))]
    pub revised_start_ms: u64,
    #[cfg_attr(feature = "specta", specta(type = u32))]
    pub revised_end_ms: u64,
}
fn shift_blocks(changes: &[Change], tolerance: u32) -> Vec<ShiftBlock> {
    let mut blocks = Vec::new();
    let mut first = 0;
    while first < changes.len() {
        let mut end = first;
        let mut block = ShiftBlock {
            first: first as u32,
            end: 0,
            delta_ms: 0,
            revised_start_ms: u64::MAX,
            revised_end_ms: 0,
            start_ms: u64::MAX,
            end_ms: 0,
        };
        while let Some(change) = changes.get(end).filter(|c| c.kind == Kind::Timing) {
            let (a, b) = (&change.original[0], &change.revised[0]);
            let delta = b.start_ms as i64 - a.start_ms as i64;
            if first == end {
                block.delta_ms = delta;
            }
            if delta.abs_diff(block.delta_ms) > u64::from(tolerance)
                || (b.end_ms as i64 - a.end_ms as i64).abs_diff(block.delta_ms)
                    > u64::from(tolerance)
            {
                break;
            }
            block.revised_start_ms = block.revised_start_ms.min(b.start_ms);
            block.revised_end_ms = block.revised_end_ms.max(b.end_ms);
            block.start_ms = block.start_ms.min(a.start_ms.min(b.start_ms));
            block.end_ms = block.end_ms.max(a.end_ms.max(b.end_ms));
            end += 1;
        }
        if end - first >= MIN_SHIFT_BLOCK {
            block.end = end as u32;
            blocks.push(block);
        }
        first = end.max(first + 1);
    }
    blocks
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Comparison {
    pub schema_version: u32,
    pub options: Options,
    pub statistics: Statistics,
    /// Includes unchanged groups so reports can include them without recomputing.
    pub changes: Vec<Change>,
    pub blocks: Vec<ShiftBlock>,
}
fn normalized(text: &str, options: &Options) -> String {
    let text = if options.ignore_line_breaks {
        text.replace('\n', " ")
    } else {
        text.to_owned()
    };
    if options.ignore_whitespace {
        text.split('\n')
            .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        text
    }
}
fn comparable(cue: &CueSnapshot, options: &Options) -> String {
    normalized(
        if options.ignore_tags {
            &cue.reading
        } else {
            &cue.text
        },
        options,
    )
}
fn snapshots(
    cues: &[Cue],
    format: Format,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<Vec<CueSnapshot>> {
    let mut result = Vec::with_capacity(cues.len());
    for (index, cue) in cues.iter().enumerate() {
        checkpoint()?;
        let reading = Projection::for_format(cue.text(), format).visible;
        let metrics = metrics::text_for_format(cue.text(), format, || {
            checkpoint().map_err(crate::quality::Error::Invalid)
        })
        .map_err(|e| e.to_string())?;
        let (cps, reading_speed, _) = metrics::rates(
            metrics.count,
            cue.end().get() - cue.start().get(),
            metrics.empty,
        );
        result.push(CueSnapshot {
            source_index: index as u32,
            cue_id: cue.id().0,
            start_ms: cue.start().get(),
            end_ms: cue.end().get(),
            text: cue.text().into(),
            reading,
            style: cue
                .metadata()
                .and_then(|m| m.get("Style"))
                .map(str::to_owned),
            actor: cue
                .metadata()
                .and_then(|m| m.get("Name"))
                .map(str::to_owned),
            characters: metrics.count,
            cps,
            reading_speed,
        });
    }
    result.sort_by_key(|c| (c.start_ms, c.end_ms, c.source_index));
    Ok(result)
}
fn span(group: &[CueSnapshot]) -> Option<(u64, u64)> {
    Some((
        group.iter().map(|c| c.start_ms).min()?,
        group.iter().map(|c| c.end_ms).max()?,
    ))
}
/// Text is the visible reading projection for the source subtitle format.
#[derive(Clone, Copy)]
pub struct TimedText<'a> {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: &'a str,
}
impl<'a> From<&'a CueSnapshot> for TimedText<'a> {
    fn from(c: &'a CueSnapshot) -> Self {
        Self {
            start_ms: c.start_ms,
            end_ms: c.end_ms,
            text: &c.reading,
        }
    }
}
fn token_counts<'a>(side: &[TimedText<'a>]) -> std::collections::BTreeMap<&'a str, usize> {
    let mut counts = std::collections::BTreeMap::new();
    for word in side.iter().flat_map(|c| c.text.split_whitespace()) {
        *counts.entry(word).or_default() += 1;
    }
    counts
}
fn common_words(
    a: &std::collections::BTreeMap<&str, usize>,
    b: &std::collections::BTreeMap<&str, usize>,
) -> usize {
    a.iter()
        .map(|(word, n)| (*n).min(*b.get(word).unwrap_or(&0)))
        .sum()
}
fn scores(a: &[TimedText], b: &[TimedText], offsets: &[i64]) -> (f64, f64) {
    fn interval(s: &[TimedText]) -> Option<(u64, u64)> {
        Some((
            s.iter().map(|c| c.start_ms).min()?,
            s.iter().map(|c| c.end_ms).max()?,
        ))
    }
    let overlap = match (interval(a), interval(b)) {
        (Some((s, e)), Some((t, f))) => {
            offsets
                .iter()
                .map(|&d| {
                    // Signed coordinates preserve duration even when a hypothesized
                    // offset places a candidate before the start of the file.
                    let (s, e) = (i128::from(s) + i128::from(d), i128::from(e) + i128::from(d));
                    let (t, f) = (i128::from(t), i128::from(f));
                    (e.min(f) - s.max(t)).max(0) as f64 / (e.max(f) - s.min(t)).max(1) as f64
                })
                .fold(0.0, f64::max)
        }
        _ => 0.0,
    };
    let (ac, bc) = (token_counts(a), token_counts(b));
    let total = ac.values().sum::<usize>() + bc.values().sum::<usize>();
    let similarity = if total == 0 {
        1.0
    } else {
        2.0 * common_words(&ac, &bc) as f64 / total as f64
    };
    (TIME_WEIGHT * overlap + TEXT_WEIGHT * similarity, similarity)
}
pub fn pair_score(a: &[TimedText], b: &[TimedText]) -> f64 {
    scores(a, b, &[0]).0
}
fn explains_parts(single: TimedText, parts: &[TimedText]) -> bool {
    let mut available = token_counts(&[single]);
    let mut covered = 0;
    let mut cursor = single.start_ms;
    for part in parts {
        let words = token_counts(&[*part]);
        let count = words.values().sum::<usize>();
        if count == 0 || (common_words(&words, &available) as f64) < PART_CONTAINMENT * count as f64
        {
            return false;
        }
        for (word, n) in words {
            if let Some(left) = available.get_mut(word) {
                *left = left.saturating_sub(n);
            }
        }
        let end = single.end_ms.min(part.end_ms);
        covered += end.saturating_sub(cursor.max(part.start_ms));
        cursor = cursor.max(end);
    }
    covered as f64 >= GROUP_SPAN_THRESHOLD * (single.end_ms - single.start_ms) as f64
}
fn close(a: &[CueSnapshot], b: &[CueSnapshot], offset_ms: i64) -> bool {
    match (span(a), span(b)) {
        (Some((s, e)), Some((t, f))) => {
            let offset = i128::from(offset_ms);
            let window = i128::from(CANDIDATE_WINDOW_MS);
            i128::from(s) + offset <= i128::from(f) + window
                && i128::from(t) <= i128::from(e) + offset + window
        }
        _ => false,
    }
}
fn key(kind: Kind, a: &[CueSnapshot], b: &[CueSnapshot]) -> String {
    // Length-framed JSON tuples exclude unstable cue IDs and file positions.
    fn side(g: &[CueSnapshot]) -> Vec<(u64, u64, &str)> {
        g.iter()
            .map(|c| (c.start_ms, c.end_ms, c.text.as_str()))
            .collect()
    }
    content_key(kind, &side(a), &side(b))
}
pub(crate) fn content_key(
    kind: Kind,
    original: &[(u64, u64, &str)],
    revised: &[(u64, u64, &str)],
) -> String {
    let bytes = serde_json::to_vec(&(kind, original, revised))
        .expect("scalar comparison key serialization");
    format!("{:x}", Sha256::digest(bytes))
}
pub fn compare(
    original: &[Cue],
    original_format: Format,
    revised: &[Cue],
    revised_format: Format,
    options: Options,
    mut checkpoint: impl FnMut() -> Result<()>,
) -> Result<Comparison> {
    let a = snapshots(original, original_format, &mut checkpoint)?;
    let b = snapshots(revised, revised_format, &mut checkpoint)?;
    let groups = alignment::align(&a, &b, &options, &mut checkpoint)?;
    let mut statistics = Statistics {
        original_cues: a.len() as u32,
        ..Statistics::default()
    };
    let mut changes = Vec::with_capacity(groups.len());
    for (ar, br) in groups {
        checkpoint()?;
        let (a, b) = (&a[ar], &b[br]);
        let text_changed = a.len() != b.len()
            || a.iter()
                .zip(b)
                .any(|(a, b)| comparable(a, &options) != comparable(b, &options));
        let timing_changed = a.len() != b.len()
            || a.iter().zip(b).any(|(a, b)| {
                a.start_ms.abs_diff(b.start_ms) > options.tolerance_ms as u64
                    || a.end_ms.abs_diff(b.end_ms) > options.tolerance_ms as u64
            });
        let style_changed = a
            .iter()
            .zip(b)
            .any(|(a, b)| a.style != b.style || a.actor != b.actor);
        let kind = match (a.len(), b.len()) {
            (0, _) => Kind::Added,
            (_, 0) => Kind::Removed,
            (1, 2..) => Kind::Split,
            (2.., 1) => Kind::Merged,
            _ if text_changed && timing_changed => Kind::TextAndTiming,
            _ if text_changed => Kind::Text,
            _ if timing_changed => Kind::Timing,
            _ if style_changed => Kind::Style,
            _ => Kind::Unchanged,
        };
        if kind != Kind::Unchanged {
            statistics.changes += 1;
            statistics.changed_cues += a.len() as u32;
            if text_changed {
                statistics.text_cues += a.len() as u32;
            }
            if timing_changed {
                statistics.timing_cues += a.len() as u32;
            }
            if style_changed {
                statistics.style_cues += a.len() as u32;
            }
            if kind == Kind::Added {
                statistics.added_cues += b.len() as u32;
            }
        }
        let join = |g: &[CueSnapshot]| {
            g.iter()
                .map(|c| c.text.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        };
        changes.push(Change {
            key: key(kind, a, b),
            kind,
            original: a.to_vec(),
            revised: b.to_vec(),
            words: word_diff(&join(a), &join(b)),
            text_changed,
            timing_changed,
            style_changed,
        });
    }
    if statistics.original_cues > 0 {
        let denominator = f64::from(statistics.original_cues);
        statistics.text_percent = 100.0 * f64::from(statistics.text_cues) / denominator;
        statistics.timing_percent = 100.0 * f64::from(statistics.timing_cues) / denominator;
        statistics.total_percent = 100.0 * f64::from(statistics.changed_cues) / denominator;
    }
    Ok(Comparison {
        schema_version: 2,
        blocks: shift_blocks(&changes, options.tolerance_ms),
        options,
        statistics,
        changes,
    })
}

#[cfg(test)]
mod tests;
