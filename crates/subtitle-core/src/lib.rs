//! Subtitle state and reversible edits. No filesystem, UI or media dependencies.
mod document;
mod editor;
pub use document::{Cue, CueId, CueMetadata, Document, Milliseconds};

pub type Result<T> = std::result::Result<T, String>;

pub use editor::{CueText, CueTiming, Edit, EditOutcome, Editor};
