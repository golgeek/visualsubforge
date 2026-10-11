//! Shared application operations for desktop and headless callers.
//! Filesystem operations block; desktop adapters must dispatch them off the UI.
//! No Tauri, webview, native playback or frontend DTO dependency belongs here.
pub mod atomic_file;
pub mod file_lease;
pub mod save_lease;
pub mod srt_limits;
pub mod subtitle_file;
