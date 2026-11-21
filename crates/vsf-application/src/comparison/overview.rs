use super::{Change, Kind};
use serde::Serialize;

/// Counts revised cues (each split member separately), or original cues for removals.
/// Stack order: additions, removals, groups, text, style, timing.
#[derive(Default, Clone, Debug, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Bucket {
    pub total: u32,
    pub kinds: [u32; 6],
    pub first: Option<u32>,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Overview {
    #[cfg_attr(feature = "specta", specta(type = f64))]
    pub width_ms: u64,
    pub buckets: Vec<Bucket>,
}
pub fn kind_index(c: &Change) -> Option<usize> {
    Some(match c.kind {
        Kind::Added => 0,
        Kind::Removed => 1,
        Kind::Split | Kind::Merged => 2,
        Kind::Text | Kind::TextAndTiming => 3,
        Kind::Style => 4,
        Kind::Timing if c.style_changed => 4,
        Kind::Timing => 5,
        Kind::Unchanged => return None,
    })
}
fn bucket_width(duration: u64) -> u64 {
    [10, 15, 30, 60, 90, 120, 180, 300, 600, 900, 1800]
        .into_iter()
        .map(|s| s * 1000)
        .find(|w| duration.div_ceil(*w) <= 100)
        .unwrap_or_else(|| duration.div_ceil(100))
}
pub fn overview(changes: &[Change]) -> Overview {
    fn side(c: &Change) -> &[super::CueSnapshot] {
        if c.revised.is_empty() {
            &c.original
        } else {
            &c.revised
        }
    }
    let duration = changes
        .iter()
        .flat_map(side)
        .map(|q| q.end_ms)
        .max()
        .unwrap_or(0);
    let width_ms = bucket_width(duration);
    let mut buckets = vec![Bucket::default(); duration.div_ceil(width_ms).max(1) as usize];
    for (index, change) in changes.iter().enumerate() {
        for cue in side(change) {
            let bucket = &mut buckets[(cue.start_ms / width_ms) as usize];
            bucket.total += 1;
            if let Some(kind) = kind_index(change) {
                bucket.kinds[kind] += 1;
                bucket.first.get_or_insert(index as u32);
            }
        }
    }
    Overview { width_ms, buckets }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn adaptive_widths_bound_short_episode_feature_and_long_files() {
        for (seconds, width) in [(180, 10), (3500, 60), (7200, 90), (300_000, 3000)] {
            assert_eq!(bucket_width(seconds * 1000), width * 1000);
        }
    }
}
