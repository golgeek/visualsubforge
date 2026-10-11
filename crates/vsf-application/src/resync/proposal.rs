//! Pure proposal construction: no files, processes, desktop state or IPC.
use super::{Budget, Error, Result};
use crate::srt_limits;
use media_analysis::alignment::{Alignment, Segment};
use serde::Serialize;
use sha2::{Digest, Sha256};
use subtitle_core::{CueTiming, Document, Edit, Editor, Milliseconds};
use subtitle_formats::srt;

/// An immutable accepted document copy, including format metadata. GUI callers
/// capture generation/revision separately; this never reads frontend drafts.
#[derive(Clone)]
pub struct SubtitleSnapshot {
    document: Document,
    source: srt::Source,
    sha256: String,
}
impl SubtitleSnapshot {
    pub fn new(document: Document, source: srt::Source) -> Result<Self> {
        srt_limits::validate_document(&document, &source).map_err(Error::InvalidInput)?;
        if document.cues().is_empty() {
            return Err(Error::InvalidInput(
                "Resynchronization needs at least one cue".into(),
            ));
        }
        let normalized = srt::serialize(&document, &source).map_err(Error::InvalidInput)?;
        let sha256 = format!("{:x}", Sha256::digest(normalized.as_bytes()));
        Ok(Self {
            document,
            source,
            sha256,
        })
    }
    pub fn document(&self) -> &Document {
        &self.document
    }
    /// Digest of the normalized SRT snapshot, not necessarily the file bytes.
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UnresolvedReason {
    ReorderedContent,
    UnsupportedCoverage,
    CrossesDiscontinuity,
    InvalidMappedRange,
}

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CueOutcome {
    Mapped {
        start_ms: u64,
        end_ms: u64,
        section_index: usize,
    },
    Unresolved {
        reason: UnresolvedReason,
    },
}

#[derive(Debug, Serialize)]
pub struct CueProposal {
    /// String on the report wire: Rust cue identifiers need not fit JS numbers.
    pub cue_id: String,
    pub source_start_ms: u64,
    pub source_end_ms: u64,
    pub outcome: CueOutcome,
}

/// Owned and immutable after construction. A projection/report cannot be used
/// to author new accepted timings; callers retain this object for acceptance.
pub struct Proposal {
    snapshot: SubtitleSnapshot,
    alignment: Alignment,
    cues: Vec<CueProposal>,
    timings: Option<Vec<CueTiming>>,
    srt: Option<String>,
    srt_sha256: Option<String>,
}
impl Proposal {
    pub fn build(
        snapshot: SubtitleSnapshot,
        alignment: Alignment,
        budget: &Budget<'_>,
    ) -> Result<Self> {
        budget.check()?;
        validate_map(&alignment)?;
        let mut cues = Vec::with_capacity(snapshot.document.cues().len());
        let mut timings = Vec::with_capacity(snapshot.document.cues().len());
        for cue in snapshot.document.cues() {
            budget.check()?;
            let outcome = map_cue(cue.start(), cue.end(), &alignment);
            if let CueOutcome::Mapped {
                start_ms, end_ms, ..
            } = outcome
            {
                timings.push(CueTiming {
                    id: cue.id(),
                    start: Milliseconds::new(start_ms).map_err(Error::InvalidInput)?,
                    end: Milliseconds::new(end_ms).map_err(Error::InvalidInput)?,
                });
            }
            cues.push(CueProposal {
                cue_id: cue.id().0.to_string(),
                source_start_ms: cue.start().get(),
                source_end_ms: cue.end().get(),
                outcome,
            });
        }
        let complete = timings.len() == cues.len();
        let srt = if complete {
            // Exercise the same atomic domain acceptance used later by desktop.
            let mut editor = Editor::new(snapshot.document.clone());
            editor
                .apply_edit(
                    0,
                    Edit::Retime {
                        timings: timings.clone(),
                    },
                    |document| srt_limits::validate_document(document, &snapshot.source),
                )
                .map_err(Error::InvalidInput)?;
            Some(srt::serialize(editor.document(), &snapshot.source).map_err(Error::InvalidInput)?)
        } else {
            None
        };
        let srt_sha256 = srt
            .as_ref()
            .map(|s| format!("{:x}", Sha256::digest(s.as_bytes())));
        budget.check()?;
        Ok(Self {
            snapshot,
            alignment,
            cues,
            timings: complete.then_some(timings),
            srt,
            srt_sha256,
        })
    }
    pub fn is_complete(&self) -> bool {
        self.timings.is_some()
    }
    pub fn cues(&self) -> &[CueProposal] {
        &self.cues
    }
    pub fn alignment(&self) -> &Alignment {
        &self.alignment
    }
    pub fn snapshot(&self) -> &SubtitleSnapshot {
        &self.snapshot
    }
    pub fn srt(&self) -> Option<&str> {
        self.srt.as_deref()
    }
    pub fn srt_sha256(&self) -> Option<&str> {
        self.srt_sha256.as_deref()
    }

    /// The desktop must additionally check its captured generation/revision and
    /// media/job identities under its document owner before calling apply_edit.
    pub fn edit(&self, current: &Document) -> Result<Edit> {
        if current != &self.snapshot.document {
            return Err(Error::InputChanged("subtitle document".into()));
        }
        let timings = self
            .timings
            .as_ref()
            .ok_or_else(|| Error::InvalidInput("Proposal contains unresolved cues".into()))?;
        Ok(Edit::Retime {
            timings: timings.clone(),
        })
    }
}

fn validate_map(alignment: &Alignment) -> Result<()> {
    // This also guarantees monotonic timing across supported sections. Existing
    // overlaps/order remain legal; a map cannot introduce inversions or overlaps
    // between previously separated cues. Rounding is monotonic too. Collapsed
    // individual ranges are rejected separately, never lengthened or clamped.
    if alignment.segments.len() > media_analysis::scenes::visual::MAX_SAMPLES
        || (alignment.reordered && !alignment.segments.is_empty())
        || alignment.segments.iter().any(|s| {
            s.source_end_us <= s.source_start_us
                || s.target_end_us <= s.target_start_us
                || s.source_end_us > 14_400_000_000
                || s.target_end_us > 14_400_000_000
                || s.anchors < 3
                || !s.max_residual_us.is_finite()
                || s.max_residual_us < 0.
        })
        || alignment.segments.windows(2).any(|p| {
            p[0].source_end_us > p[1].source_start_us || p[0].target_end_us > p[1].target_start_us
        })
    {
        return Err(Error::InvalidInput(
            "Invalid or non-monotonic alignment map".into(),
        ));
    }
    Ok(())
}

fn map_cue(start: Milliseconds, end: Milliseconds, alignment: &Alignment) -> CueOutcome {
    let unresolved = |reason| CueOutcome::Unresolved { reason };
    if alignment.reordered {
        return unresolved(UnresolvedReason::ReorderedContent);
    }
    // The domain's four-digit-hour ceiling makes multiplication safe.
    let (start_us, end_us) = (start.get() * 1_000, end.get() * 1_000);
    let sections = &alignment.segments;
    let index = sections.partition_point(|s| s.source_end_us <= start_us);
    let Some(section) = sections.get(index) else {
        return unresolved(UnresolvedReason::UnsupportedCoverage);
    };
    if section.source_start_us > start_us || section.source_end_us < end_us {
        let touches_supported =
            section.source_start_us < end_us && section.source_end_us > start_us;
        return unresolved(if touches_supported {
            UnresolvedReason::CrossesDiscontinuity
        } else {
            UnresolvedReason::UnsupportedCoverage
        });
    }
    match (map_ms(section, start_us), map_ms(section, end_us)) {
        (Some(start_ms), Some(end_ms)) if end_ms > start_ms => CueOutcome::Mapped {
            start_ms,
            end_ms,
            section_index: index,
        },
        _ => unresolved(UnresolvedReason::InvalidMappedRange),
    }
}

/// Exact rational mapping, rounded once to milliseconds (positive ties up).
/// No float-to-microsecond intermediate or nominal frame-rate conversion.
fn map_ms(section: &Segment, source_us: u64) -> Option<u64> {
    let source_span = u128::from(section.source_end_us.checked_sub(section.source_start_us)?);
    let target_span = u128::from(section.target_end_us.checked_sub(section.target_start_us)?);
    let offset = u128::from(source_us.checked_sub(section.source_start_us)?);
    let numerator = u128::from(section.target_start_us)
        .checked_mul(source_span)?
        .checked_add(offset.checked_mul(target_span)?)?;
    let denominator = source_span.checked_mul(1_000)?;
    let rounded = numerator
        .checked_add(denominator / 2)?
        .checked_div(denominator)?;
    u64::try_from(rounded)
        .ok()
        .filter(|value| *value <= Milliseconds::MAX)
}
