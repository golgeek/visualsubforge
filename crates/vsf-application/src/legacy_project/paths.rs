use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathKind {
    Relative,
    PosixAbsolute,
    WindowsAbsolute,
    Unc,
    DriveRelative,
    WindowsRootRelative,
    Device,
    Url,
}

pub fn classify_path(value: &str) -> PathKind {
    let path = value.replace('\\', "/");
    if path.starts_with("//?/") || path.starts_with("//./") {
        return PathKind::Device;
    }
    if path.starts_with("//") {
        return PathKind::Unc;
    }
    if path.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
        && path.as_bytes().get(1) == Some(&b':')
    {
        return if path.as_bytes().get(2) == Some(&b'/') {
            PathKind::WindowsAbsolute
        } else {
            PathKind::DriveRelative
        };
    }
    // Schemes have no meaning as local legacy paths, even file:// URLs.
    if path.split_once(':').is_some_and(|(scheme, _)| {
        !scheme.is_empty()
            && scheme
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'+' | b'-' | b'.'))
    }) {
        return PathKind::Url;
    }
    if value.starts_with('\\') {
        PathKind::WindowsRootRelative
    } else if value.starts_with('/') {
        PathKind::PosixAbsolute
    } else {
        PathKind::Relative
    }
}

/// Produce a candidate only; no basename search, environment expansion, drive
/// guessing or IO. Canonicalization happens during reference inspection.
pub fn local_path(base: &Path, value: &str) -> Result<PathBuf> {
    if value.is_empty() || value.chars().any(char::is_control) {
        return Err("Choose a nonempty local file path without control characters".into());
    }
    if !base.is_absolute() {
        return Err("Legacy project directory must be absolute".into());
    }
    let path = value.replace('\\', "/");
    match classify_path(value) {
        PathKind::Relative => Ok(base.join(path)),
        PathKind::PosixAbsolute if !cfg!(windows) => Ok(path.into()),
        PathKind::WindowsAbsolute | PathKind::Unc if cfg!(windows) => Ok(path.into()),
        PathKind::DriveRelative | PathKind::WindowsRootRelative => Err(
            "This Windows path depends on a drive/current directory; choose the file explicitly"
                .into(),
        ),
        PathKind::Device | PathKind::Url => {
            Err("Device paths and URLs are not imported; choose a local file".into())
        }
        _ => Err(
            "This absolute path belongs to another platform; choose the file's current location"
                .into(),
        ),
    }
}
