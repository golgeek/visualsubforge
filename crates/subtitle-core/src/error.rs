use crate::{CueId, Milliseconds};
use std::fmt;

/// Domain failures. Display preserves the desktop/CLI wording; callers should
/// match variants instead of inspecting that wording.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    TimeOutOfRange {
        value: u64,
    },
    InvalidTimeRange {
        start: Milliseconds,
        end: Milliseconds,
    },
    InvalidText,
    InvalidMetadata {
        field: String,
    },
    InvalidStyleName,
    MissingStyle,
    DuplicateCue {
        id: CueId,
    },
    StaleRevision {
        expected: u64,
        actual: u64,
    },
    MissingCue {
        id: CueId,
    },
    InvalidCueBatch,
    InvalidTextBatch,
    InvalidRetiming,
    InvalidSplit {
        id: CueId,
        at: Milliseconds,
        start: Milliseconds,
        end: Milliseconds,
    },
    NonAdjacentMerge {
        first: CueId,
        second: CueId,
    },
    MergeMetadataMismatch,
    ChangedCueIdentities,
    CueIdentitiesExhausted,
    CreateWithoutIdentity,
    SplitWithoutIdentity,
    /// The synchronous application validator rejected the proposal. The domain
    /// deliberately does not interpret its format, size or other policy message.
    ValidationRejected {
        message: String,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TimeOutOfRange { .. } => f.write_str("Time exceeds 9999:59:59,999"),
            Self::InvalidTimeRange { .. } => f.write_str("Cue end must be later than its start"),
            Self::InvalidText => {
                f.write_str("Cue text cannot contain carriage returns or NUL characters")
            }
            Self::InvalidMetadata { .. } => f.write_str("Invalid or duplicate cue metadata field"),
            Self::InvalidStyleName => {
                f.write_str("Style name must be nonempty and contain no commas or line breaks")
            }
            Self::MissingStyle => f.write_str("This cue has no assignable style"),
            Self::DuplicateCue { .. } => f.write_str("Cue identities must be unique"),
            Self::StaleRevision { .. } => {
                f.write_str("Subtitle document changed; refresh before committing this edit")
            }
            Self::MissingCue { .. } => f.write_str("Cue no longer exists"),
            Self::InvalidCueBatch => {
                f.write_str("Cue batch must include every cue once in document order")
            }
            Self::InvalidTextBatch => {
                f.write_str("Text batch must include every cue once in document order")
            }
            Self::InvalidRetiming => {
                f.write_str("Retiming must include every cue once in document order")
            }
            Self::InvalidSplit { .. } => {
                f.write_str("Split time must be strictly inside the selected cue")
            }
            Self::NonAdjacentMerge { .. } => {
                f.write_str("Merge requires two adjacent cues in document order")
            }
            Self::MergeMetadataMismatch => {
                f.write_str("Merge requires matching cue metadata and style")
            }
            Self::ChangedCueIdentities => {
                f.write_str("Document-level edits must preserve cue identities and order")
            }
            Self::CueIdentitiesExhausted => f.write_str("Cue identities exhausted"),
            Self::CreateWithoutIdentity => f.write_str("Create did not allocate a cue"),
            Self::SplitWithoutIdentity => f.write_str("Split did not allocate a cue"),
            Self::ValidationRejected { message } => f.write_str(message),
        }
    }
}
impl std::error::Error for Error {}

/// Compatibility at application/desktop/CLI boundaries that still report text.
impl From<Error> for String {
    fn from(error: Error) -> Self {
        error.to_string()
    }
}
