//! The sole unsafe call site in the Tauri adapter.
#![allow(unsafe_code)]
pub fn attach(window: &tauri::WebviewWindow) -> Result<media_player::Surface, String> {
    #[cfg(target_os = "macos")]
    let parent = window.ns_view().map_err(|e| e.to_string())?;
    #[cfg(target_os = "windows")]
    let parent = window.hwnd().map_err(|e| e.to_string())?.0;
    #[cfg(target_os = "linux")]
    let parent = {
        use gtk::glib::object::ObjectType;
        window
            .default_vbox()
            .map_err(|e| e.to_string())?
            .as_ptr()
            .cast()
    };
    // SAFETY: setup invokes this on Tauri's main thread with its live platform
    // parent (NSView, HWND, or GTK box). The GTK box is retained by the window.
    // The exit gate keeps the parent alive until native child removal completes.
    unsafe { media_player::Surface::attach(parent) }
}
