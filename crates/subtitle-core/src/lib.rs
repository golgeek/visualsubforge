//! Subtitle state and reversible edits. No filesystem, UI or media dependencies.
mod document;
mod editor;
mod error;
pub use document::{Cue, CueId, CueMetadata, Document, Milliseconds};
pub use error::Error;

pub type Result<T> = std::result::Result<T, Error>;

pub use editor::{CueText, CueTiming, CueUpdate, Edit, EditOutcome, Editor};
