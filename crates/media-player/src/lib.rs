//! Native surface experiment. This module is the only Rust FFI boundary.

#[cfg(all(
    feature = "native-mpv",
    any(target_os = "macos", target_os = "windows", target_os = "linux")
))]
#[allow(unsafe_code)]
mod native;
#[cfg(all(
    feature = "native-mpv",
    any(target_os = "macos", target_os = "windows", target_os = "linux")
))]
pub use native::{Player, Surface};
