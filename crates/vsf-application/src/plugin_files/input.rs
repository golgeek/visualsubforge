//! Bounded regular-file reads; identities describe the very bytes being parsed.
use super::{Budget, Error, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
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
pub(super) struct CapturedFile {
    pub identity: FileIdentity,
    pub bytes: Vec<u8>,
}
pub(super) fn digest(bytes: &[u8]) -> String {
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
    pub(super) fn verify(&self, budget: &Budget<'_>) -> Result<()> {
        let now = CapturedFile::read(&self.requested_path, self.bytes, budget)
            .map_err(|e| changed(e, &self.requested_path))?;
        if now.identity != *self {
            return Err(Error::Changed(self.requested_path.display().to_string()));
        }
        Ok(())
    }
}
pub(super) fn changed(error: Error, path: &Path) -> Error {
    match error {
        Error::Cancelled | Error::Timeout => error,
        _ => Error::Changed(format!(
            "{} (changed or no longer readable)",
            path.display()
        )),
    }
}
