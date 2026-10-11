//! Process startup policy for GTK and libmpv. No worker changes global locale.
#![allow(unsafe_code)]

/// Establish the user's locale except for libmpv's required C numeric category.
///
/// # Safety
/// Call once at the beginning of main, before GTK initialization or any threads
/// that can read or mutate C locale state. Never call from an app/media worker.
pub unsafe fn initialize() -> Result<(), &'static str> {
    // SAFETY: the caller guarantees single-threaded startup. All C strings are
    // static and terminated; returned libc-owned pointers are only null-checked.
    // Like GTK, retain the initial C locale if the environment is unsupported.
    if unsafe { libc::setlocale(libc::LC_ALL, c"".as_ptr()) }.is_null() {
        eprintln!("Unsupported system locale; VisualSubForge is using the C locale");
        if unsafe { libc::setlocale(libc::LC_ALL, c"C".as_ptr()) }.is_null() {
            return Err("the C locale is unavailable");
        }
    }
    if unsafe { libc::setlocale(libc::LC_NUMERIC, c"C".as_ptr()) }.is_null() {
        return Err("cannot establish the C numeric locale required by media playback");
    }
    // Prevent gtk_init() inside Tauri from restoring LC_NUMERIC from LC_ALL or
    // LANG. Other categories already reflect the user's environment, unchanged.
    gtk::disable_setlocale();
    Ok(())
}
