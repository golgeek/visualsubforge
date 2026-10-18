//! Immutable reference subtitles and time relationships for manual translation.
//! No paths, filesystem, UI, native media or accepted target/history owner here.
use crate::subtitle_markup::{MAX_CUE_BYTES, Projection};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use subtitle_core::{Cue, Milliseconds, Result};
use subtitle_formats::{Format, Parsed};

pub const MAX_PAGE_ROWS: usize = 200;

/// A reference-only mapping: target time = original reference time + offset.
/// It never changes cue endpoints or the meaning of embedded timed markup.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Offset(i64);
impl Offset {
    pub fn new(milliseconds: i64) -> Result<Self> {
        if milliseconds.unsigned_abs() > Milliseconds::MAX {
            return Err("Reference offset exceeds the supported subtitle time range".into());
        }
        Ok(Self(milliseconds))
    }
    pub fn get(self) -> i64 {
        self.0
    }
    pub fn map(self, time: Milliseconds) -> i64 {
        // Both terms are bounded by Milliseconds::MAX, well inside signed i64.
        // Negative mapped times are intentional; clamping would invent overlaps.
        time.get() as i64 + self.0
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PageRequest {
    pub offset: usize,
    pub limit: usize,
}
impl PageRequest {
    fn validate(self) -> Result<()> {
        if !(1..=MAX_PAGE_ROWS).contains(&self.limit) {
            return Err(format!("Reference pages require 1–{MAX_PAGE_ROWS} rows"));
        }
        Ok(())
    }
}

/// Indices are reference-file positions, never target cue identities.
#[derive(Debug)]
pub struct Row<'a> {
    pub source_index: usize,
    pub cue: &'a Cue,
    pub mapped_start_ms: i64,
    pub mapped_end_ms: i64,
    pub overlap_ms: Option<u64>,
}
#[derive(Debug)]
pub struct Page<'a> {
    pub total: usize,
    pub rows: Vec<Row<'a>>,
}
pub struct Reading {
    pub text: String,
    pub warning: Option<&'static str>,
    pub timed: bool,
    pub layout: bool,
}

#[derive(Clone, Debug)]
pub struct Reference(Arc<Data>);
#[derive(Debug)]
struct Data {
    bytes: Vec<u8>,
    fingerprint: String,
    parsed: Parsed,
    // Chronological order with original order breaking ties. Prefix maxima allow
    // skipping early cues without dropping a long cue spanning later short ones.
    timeline: Vec<IndexedCue>,
}
#[derive(Debug)]
struct IndexedCue {
    source_index: usize,
    prefix_end_ms: u64,
}
impl Reference {
    pub fn parse(
        bytes: Vec<u8>,
        extension: Option<&str>,
        mut checkpoint: impl FnMut() -> Result<()>,
    ) -> Result<Self> {
        checkpoint()?;
        let parsed = crate::subtitle_file::parse_document(&bytes, extension)?;
        checkpoint()?;
        let cues = parsed.document.cues();
        let mut timeline = Vec::with_capacity(cues.len());
        for source_index in 0..cues.len() {
            if source_index % 128 == 0 {
                checkpoint()?;
            }
            timeline.push(IndexedCue {
                source_index,
                prefix_end_ms: 0,
            });
        }
        timeline
            .sort_unstable_by_key(|entry| (cues[entry.source_index].start(), entry.source_index));
        checkpoint()?;
        let mut end = 0;
        for (index, entry) in timeline.iter_mut().enumerate() {
            if index % 128 == 0 {
                checkpoint()?;
            }
            end = end.max(cues[entry.source_index].end().get());
            entry.prefix_end_ms = end;
        }
        let fingerprint = format!("{:x}", Sha256::digest(&bytes));
        checkpoint()?;
        Ok(Self(Arc::new(Data {
            bytes,
            fingerprint,
            parsed,
            timeline,
        })))
    }
    pub fn format(&self) -> Format {
        self.0.parsed.source.format()
    }
    pub fn original_bytes(&self) -> &[u8] {
        &self.0.bytes
    }
    pub fn fingerprint(&self) -> &str {
        &self.0.fingerprint
    }
    pub fn cues(&self) -> &[Cue] {
        self.0.parsed.document.cues()
    }
    /// Independent mutable target data; source records remain safely shared and
    /// immutable. The desktop must protect the original when publishing/adopting.
    pub fn copy_target(&self) -> Parsed {
        Parsed {
            document: self.0.parsed.document.clone(),
            source: self.0.parsed.source.clone(),
        }
    }
    fn row(&self, source_index: usize, offset: Offset, overlap_ms: Option<u64>) -> Row<'_> {
        let cue = &self.cues()[source_index];
        Row {
            source_index,
            cue,
            mapped_start_ms: offset.map(cue.start()),
            mapped_end_ms: offset.map(cue.end()),
            overlap_ms,
        }
    }
    /// Browse original file order, including references with unsorted timing.
    pub fn browse(
        &self,
        offset: Offset,
        page: PageRequest,
        mut checkpoint: impl FnMut() -> Result<()>,
    ) -> Result<Page<'_>> {
        page.validate()?;
        checkpoint()?;
        let start = page.offset.min(self.cues().len());
        let stop = start + page.limit.min(self.cues().len() - start);
        let mut rows = Vec::with_capacity(stop - start);
        for index in start..stop {
            checkpoint()?;
            rows.push(self.row(index, offset, None));
        }
        checkpoint()?;
        Ok(Page {
            total: self.cues().len(),
            rows,
        })
    }
    /// Positive half-open overlap only. Results follow start time then source
    /// order; no heuristic nearest-row fallback changes the relationship.
    pub fn matches(
        &self,
        start: Milliseconds,
        end: Milliseconds,
        offset: Offset,
        page: PageRequest,
        mut checkpoint: impl FnMut() -> Result<()>,
    ) -> Result<Page<'_>> {
        page.validate()?;
        if end <= start {
            return Err("Target interval must have a positive duration".into());
        }
        checkpoint()?;
        let lower = start.get() as i64 - offset.get();
        let upper = end.get() as i64 - offset.get();
        let timeline = &self.0.timeline;
        let from = timeline.partition_point(|entry| entry.prefix_end_ms as i64 <= lower);
        let to = timeline.partition_point(|entry| {
            (self.cues()[entry.source_index].start().get() as i64) < upper
        });
        let mut total = 0;
        let mut rows = Vec::with_capacity(page.limit.min(to.saturating_sub(from)));
        for (index, entry) in timeline[from.min(to)..to].iter().enumerate() {
            if index % 128 == 0 {
                checkpoint()?;
            }
            let cue = &self.cues()[entry.source_index];
            let overlap =
                (cue.end().get() as i64).min(upper) - (cue.start().get() as i64).max(lower);
            if overlap > 0 {
                if total >= page.offset && rows.len() < page.limit {
                    rows.push(self.row(entry.source_index, offset, Some(overlap as u64)));
                }
                total += 1;
            }
        }
        checkpoint()?;
        Ok(Page { total, rows })
    }
    /// A reading aid, not subtitle rendering or an HTML fragment. The caller must
    /// display it as text and retain access to the original cue and diagnostics.
    pub fn reading(&self, source_index: usize) -> Result<Reading> {
        let cue = self
            .cues()
            .get(source_index)
            .ok_or("Reference cue no longer exists")?;
        if cue.text().len() > MAX_CUE_BYTES {
            return Err(
                "Reference cue exceeds the 64 KiB reading-preview limit; original text is retained"
                    .into(),
            );
        }
        let projection = Projection::for_format(cue.text(), self.format());
        Ok(Reading {
            text: projection.visible,
            warning: projection.ambiguous,
            timed: projection.timed
                || cue
                    .metadata()
                    .and_then(|m| m.get("Effect"))
                    .is_some_and(|v| !v.trim().is_empty()),
            layout: projection.layout,
        })
    }
}

#[cfg(test)]
mod tests;
