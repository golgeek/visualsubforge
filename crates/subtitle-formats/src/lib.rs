//! Format adapters: no filesystem, Tauri or native media dependencies.
pub mod ass;
mod source;
pub mod srt;
pub mod webvtt;
pub use source::{Format, Parsed, Source, parse};

mod error;
pub use error::Error;
pub type Result<T> = std::result::Result<T, Error>;
