//! Format adapters: no filesystem, Tauri or native media dependencies.
pub mod ass;
mod source;
pub mod srt;
pub use source::{Format, Parsed, Source, parse};
