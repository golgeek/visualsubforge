//! Shared quality operations. `metrics` and `check` are deterministic and do no IO.
//! Only this outer adapter reads/writes files; desktop callers use workers.
mod check;
pub mod duration;
pub mod gap;
pub mod metrics;
pub mod profile;
pub mod scene;
pub mod statistics;
mod timing_validation;
pub use check::{CueMetrics, Finding, Report, Rule, Severity, evaluate, evaluate_with_scenes};
pub use profile::Profile;
use std::{fmt, fs, io::Read, path::Path};

pub const MAX_PROFILE_BYTES: usize = 256 * 1024;
pub const MAX_CUES: usize = 100_000;
pub const MAX_TEXT_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_FINDINGS: usize = 100_000;
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    Invalid(String),
    Io(String),
    Limit(String),
    Cancelled,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(s) | Self::Io(s) | Self::Limit(s) => f.write_str(s),
            Self::Cancelled => f.write_str("Quality check cancelled"),
        }
    }
}
impl std::error::Error for Error {}

pub fn parse_profile(bytes: &[u8]) -> Result<Profile> {
    if bytes.len() > MAX_PROFILE_BYTES {
        return Err(Error::Limit("Quality preset exceeds 256 KiB".into()));
    }
    let text = std::str::from_utf8(bytes)
        .map_err(|_| Error::Invalid("Quality preset must be UTF-8".into()))?;
    let profile: Profile = serde_json::from_str(text.trim_start_matches('\u{feff}'))
        .map_err(|e| Error::Invalid(format!("Invalid quality preset: {e}")))?;
    profile.validate()?;
    Ok(profile)
}

pub fn read_profile(path: &Path) -> Result<Profile> {
    parse_profile(&read_bounded(path, MAX_PROFILE_BYTES, "Quality preset")?)
}

fn read_bounded(path: &Path, maximum: usize, label: &str) -> Result<Vec<u8>> {
    let file = fs::File::open(path).map_err(|e| Error::Io(format!("Cannot open {label}: {e}")))?;
    if !file
        .metadata()
        .map_err(|e| Error::Io(e.to_string()))?
        .is_file()
    {
        return Err(Error::Invalid(format!("{label} must be a regular file")));
    }
    let mut bytes = Vec::new();
    file.take(maximum as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| Error::Io(e.to_string()))?;
    if bytes.len() > maximum {
        return Err(Error::Limit(format!("{label} exceeds {maximum} bytes")));
    }
    Ok(bytes)
}

/// Checks a bounded file snapshot without loading desktop settings or tools.
/// Blocking filesystem calls are not interruptible; check cancellation on either
/// side of IO and throughout the deterministic evaluation.
pub fn check_file(
    path: &Path,
    profile: &Profile,
    mut checkpoint: impl FnMut(usize, usize) -> Result<()>,
) -> Result<Report> {
    checkpoint(0, 0)?;
    let bytes = read_bounded(path, crate::srt_limits::MAX_BYTES, "Subtitles")?;
    checkpoint(0, 0)?;
    let parsed = crate::subtitle_file::parse(&bytes).map_err(Error::Invalid)?;
    evaluate(&parsed.document, profile, checkpoint)
}

pub fn write_profile(path: &Path, profile: &Profile) -> Result<()> {
    profile.validate()?;
    if path.extension().is_none_or(|s| s != "vsfpreset") {
        return Err(Error::Invalid(
            "Quality preset filename must end in .vsfpreset".into(),
        ));
    }
    let bytes = serde_json::to_vec_pretty(profile).map_err(|e| Error::Invalid(e.to_string()))?;
    if bytes.len() > MAX_PROFILE_BYTES {
        return Err(Error::Limit("Quality preset exceeds 256 KiB".into()));
    }
    crate::atomic_file::write(path, &bytes).map_err(Error::Io)
}
