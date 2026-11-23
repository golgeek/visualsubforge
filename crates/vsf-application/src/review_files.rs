//! Bounded regular-file reads; identities describe the very bytes being parsed.
use crate::{atomic_file, new_output, save_lease};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fmt,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileIdentity {
    pub requested_path: PathBuf,
    pub canonical_path: PathBuf,
    pub bytes: usize,
    pub sha256: String,
}
pub(crate) struct CapturedFile {
    pub identity: FileIdentity,
    pub bytes: Vec<u8>,
}
pub(crate) fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

impl CapturedFile {
    pub fn read(path: &Path, limit: usize, budget: &Budget<'_>) -> Result<Self> {
        budget.check()?;
        let io = |e| Error::Io(format!("Cannot read {}: {e}", path.display()));
        let requested_path = std::path::absolute(path).map_err(io)?;
        let canonical_path = requested_path.canonicalize().map_err(io)?;
        let metadata = fs::metadata(&canonical_path).map_err(io)?;
        // Avoid opening ordinary FIFOs/devices. Concurrent hostile type changes
        // are outside this application's filesystem snapshot guarantees.
        if !metadata.is_file() || metadata.len() > limit as u64 {
            return Err(Error::Invalid(format!(
                "{} must be a regular file of at most {limit} bytes",
                path.display()
            )));
        }
        let mut file = fs::File::open(&canonical_path).map_err(io)?;
        let opened = file.metadata().map_err(io)?;
        if !opened.is_file()
            || opened.len() != metadata.len()
            || opened.modified().ok() != metadata.modified().ok()
        {
            return Err(Error::Changed(path.display().to_string()));
        }
        let mut bytes = Vec::new();
        let mut chunk = [0u8; 65536];
        loop {
            budget.check()?;
            let count = file.read(&mut chunk).map_err(io)?;
            if count == 0 {
                break;
            }
            if bytes.len().saturating_add(count) > limit
                || bytes.len().saturating_add(count) > metadata.len() as usize
            {
                return Err(Error::Changed(path.display().to_string()));
            }
            bytes.extend_from_slice(&chunk[..count]);
        }
        budget.check()?;
        let after = file.metadata().map_err(io)?;
        if after.len() != metadata.len()
            || bytes.len() as u64 != metadata.len()
            || after.modified().ok() != metadata.modified().ok()
            || requested_path.canonicalize().map_err(io)? != canonical_path
        {
            return Err(Error::Changed(path.display().to_string()));
        }
        let identity = FileIdentity {
            requested_path,
            canonical_path,
            bytes: bytes.len(),
            sha256: digest(&bytes),
        };
        Ok(Self { identity, bytes })
    }
}
impl FileIdentity {
    pub(crate) fn verify(&self, budget: &Budget<'_>) -> Result<()> {
        let now = CapturedFile::read(&self.requested_path, self.bytes, budget)
            .map_err(|e| changed(e, &self.requested_path))?;
        if now.identity != *self {
            return Err(Error::Changed(self.requested_path.display().to_string()));
        }
        Ok(())
    }
}
pub(crate) fn changed(error: Error, path: &Path) -> Error {
    match error {
        Error::Cancelled | Error::Timeout => error,
        _ => Error::Changed(format!(
            "{} (changed or no longer readable)",
            path.display()
        )),
    }
}

pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug)]
pub enum Error {
    Invalid(String),
    Io(String),
    Changed(String),
    Execution(String),
    Cancelled,
    Timeout,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(m) | Self::Io(m) | Self::Execution(m) => f.write_str(m),
            Self::Changed(m) => write!(f, "Review input or review changed; preview again: {m}"),
            Self::Cancelled => f.write_str("Review operation cancelled"),
            Self::Timeout => f.write_str("Review operation exceeded its time limit"),
        }
    }
}
impl std::error::Error for Error {}
pub struct Budget<'a> {
    cancel: &'a AtomicBool,
    deadline: Option<Instant>,
}
impl<'a> Budget<'a> {
    pub fn new(cancel: &'a AtomicBool, duration: Duration) -> Self {
        Self {
            cancel,
            deadline: Some(Instant::now() + duration),
        }
    }
    pub(crate) fn cancellable(cancel: &'a AtomicBool) -> Self {
        Self {
            cancel,
            deadline: None,
        }
    }
    pub fn remaining(&self) -> Result<Duration> {
        if self.cancel.load(Ordering::Acquire) {
            return Err(Error::Cancelled);
        }
        let Some(deadline) = self.deadline else {
            return Ok(Duration::MAX);
        };
        deadline
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
            .ok_or(Error::Timeout)
    }
    pub fn check(&self) -> Result<()> {
        self.remaining().map(|_| ())
    }
    pub(crate) fn checkpoint(&self) -> std::result::Result<(), String> {
        self.check().map_err(|e| e.to_string())
    }
    pub(crate) fn pure<T>(&self, value: std::result::Result<T, impl Into<String>>) -> Result<T> {
        self.check()?;
        value.map_err(|error| Error::Invalid(error.into()))
    }
}
pub(crate) fn publish(
    path: &Path,
    format: &str,
    bytes: &[u8],
    budget: &Budget<'_>,
    protect: impl Fn(&Path) -> Result<()>,
    verify: impl FnOnce() -> Result<()>,
) -> Result<PathBuf> {
    if path
        .extension()
        .and_then(|e| e.to_str())
        .is_none_or(|e| !e.eq_ignore_ascii_case(format))
    {
        return Err(Error::Invalid(format!(
            "Output must keep the .{} source format",
            format
        )));
    }
    let output = new_output::resolve(path).map_err(output_error)?;
    protect(&output)?;
    let _lease = save_lease::acquire(&output).map_err(Error::Io)?;
    new_output::require_absent(&output).map_err(output_error)?;
    let pending = atomic_file::prepare_with_checkpoint(&output, bytes, || budget.checkpoint())
        .map_err(|e| budget.check().err().unwrap_or(Error::Io(e)))?;
    verify()?;
    if new_output::resolve(path).map_err(output_error)? != output {
        return Err(Error::Changed("output directory changed".into()));
    }
    protect(&output)?;
    budget.check()?;
    pending.commit_new().map_err(Error::Io)?;
    // A cancellation arriving after commit cannot retract successful output.
    Ok(output)
}

pub(crate) fn protect(output: &Path, input: &FileIdentity) -> Result<()> {
    for path in [&input.requested_path, &input.canonical_path] {
        new_output::protect_source(output, path).map_err(output_error)?;
    }
    Ok(())
}
fn output_error(error: new_output::Error) -> Error {
    match error {
        new_output::Error::InvalidInput(e) => Error::Invalid(e),
        new_output::Error::Io(e) => Error::Io(e),
    }
}
