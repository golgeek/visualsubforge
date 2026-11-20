//! Read the user's regional date preference independently of the UI language.
use std::sync::OnceLock;

#[tauri::command]
#[specta::specta]
pub(crate) async fn system_region() -> Option<String> {
    static REGION: OnceLock<Option<String>> = OnceLock::new();
    REGION.get_or_init(read_region).clone()
}

#[cfg(target_os = "macos")]
fn read_region() -> Option<String> {
    let locale = objc2_foundation::NSLocale::currentLocale();
    let region = if objc2::available!(macos = 14.0) {
        locale.regionCode()
    } else {
        // The desktop also runs on macOS 11–13.
        #[allow(deprecated)]
        locale.countryCode()
    };
    region.map(|code| code.to_string())
}

#[cfg(target_os = "windows")]
fn read_region() -> Option<String> {
    use windows_sys::Win32::Globalization::{GetLocaleInfoEx, LOCALE_SISO3166CTRYNAME};
    let mut code = [0u16; 4];
    // SAFETY: null selects LOCALE_NAME_USER_DEFAULT and the writable length
    // is the buffer's UTF-16 capacity. Windows includes the terminating NUL.
    #[allow(unsafe_code)]
    let len = unsafe {
        GetLocaleInfoEx(
            std::ptr::null(),
            LOCALE_SISO3166CTRYNAME,
            code.as_mut_ptr(),
            code.len() as i32,
        )
    };
    (len > 1).then(|| String::from_utf16_lossy(&code[..len as usize - 1]))
}

#[cfg(target_os = "linux")]
fn read_region() -> Option<String> {
    let locale = ["LC_ALL", "LC_TIME", "LANG"]
        .into_iter()
        .filter_map(|name| std::env::var(name).ok())
        .find(|value| !value.is_empty())?;
    posix_region(&locale)
}

#[cfg(any(target_os = "linux", test))]
fn posix_region(locale: &str) -> Option<String> {
    let base = locale.split(['.', '@']).next()?;
    let (_, region) = base.split_once('_')?;
    (region.len() == 2 && region.bytes().all(|c| c.is_ascii_alphabetic()))
        .then(|| region.to_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn posix_time_locale_needs_an_explicit_territory() {
        assert_eq!(posix_region("fr_CH.UTF-8@euro").as_deref(), Some("CH"));
        for neutral in ["en", "C.UTF-8", "POSIX"] {
            assert_eq!(posix_region(neutral), None);
        }
    }
}
