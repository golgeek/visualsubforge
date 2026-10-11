//! Process-lifetime macOS termination hook, independent of the media adapter.
//! Installation is main-thread-only during setup. The Objective-C delegate
//! method retains only a function pointer; the AppHandle is kept in a OnceLock
//! for the process lifetime, so callbacks cannot outlive their Rust context.
#![allow(unsafe_code)]
use std::sync::OnceLock;
static APP: OnceLock<tauri::AppHandle> = OnceLock::new();
unsafe extern "C" {
    fn vsf_install_quit_guard(callback: extern "C" fn()) -> bool;
}
extern "C" fn request_exit() {
    if let Some(app) = APP.get() {
        app.exit(0);
    }
}
pub fn install(app: &tauri::AppHandle) -> Result<(), String> {
    APP.set(app.clone())
        .map_err(|_| "Quit guard already installed")?;
    // SAFETY: setup runs on AppKit's main thread. The callback and static
    // AppHandle remain valid until process exit; no native pointer crosses IPC.
    if unsafe { vsf_install_quit_guard(request_exit) } {
        Ok(())
    } else {
        Err("Cannot install macOS quit protection: inspect the application delegate before continuing".into())
    }
}
