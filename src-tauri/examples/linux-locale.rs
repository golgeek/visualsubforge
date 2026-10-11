//! Separate-process startup regression; run with an actual GTK display.
#[cfg(target_os = "linux")]
#[path = "../src/linux_locale.rs"]
mod linux_locale;

#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let expected_text_locale = std::env::args()
        .nth(1)
        .ok_or("expected LC_CTYPE argument")?;
    let environment: Vec<_> = ["LANG", "LC_ALL", "LC_NUMERIC", "LC_CTYPE"]
        .into_iter()
        .map(|key| (key, std::env::var_os(key)))
        .collect();
    // SAFETY: this standalone process has not initialized GTK or started workers.
    #[allow(unsafe_code)]
    unsafe {
        linux_locale::initialize()?;
    }
    gtk::init()?;
    #[allow(unsafe_code)]
    fn category(category: libc::c_int) -> String {
        // SAFETY: null queries rather than mutates; startup policy is complete
        // and this probe never changes locale again. Copy libc's borrowed value.
        unsafe {
            let value = libc::setlocale(category, std::ptr::null());
            assert!(!value.is_null());
            std::ffi::CStr::from_ptr(value)
                .to_string_lossy()
                .into_owned()
        }
    }
    assert_eq!(category(libc::LC_NUMERIC), "C", "GTK reset numeric locale");
    assert_eq!(
        category(libc::LC_CTYPE),
        expected_text_locale,
        "text locale changed"
    );
    let worker_numeric = std::thread::spawn(|| category(libc::LC_NUMERIC))
        .join()
        .expect("worker panicked");
    assert_eq!(worker_numeric, "C");
    for (key, before) in environment {
        assert_eq!(std::env::var_os(key), before, "environment changed: {key}");
    }
    println!(
        "PASS: GTK and worker numeric locale C; text locale {expected_text_locale}; environment unchanged"
    );
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("Linux-only locale regression: no validation on this platform");
}
