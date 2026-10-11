//! Subtitle state and reversible edits. No filesystem, UI or media dependencies.
mod document;
mod editor;
pub use document::{Cue, CueId, Document, Milliseconds};

pub type Result<T> = std::result::Result<T, String>;

pub use editor::Editor;
