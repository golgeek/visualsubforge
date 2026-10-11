//! Content identities detect changed inputs; they do not provide filesystem
//! snapshot isolation against an adversarial writer or changes restored mid-read.
use super::{Budget, Error, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

pub const MAX_MEDIA_BYTES: u64 = 256 * 1024 * 1024 * 1024;

#[derive(Debug, Serialize)]
pub struct FileIdentity {
    /// Original absolute path is retained to detect a changed symlink target.
    pub requested_path: PathBuf,
    pub canonical_path: PathBuf,
    pub bytes: u64,
    pub sha256: String,
}
impl FileIdentity {
    pub fn capture(path: &Path, limit: u64, budget: &Budget<'_>) -> Result<Self> {
        read_hashed(path, limit, budget, |_| {})
    }

    pub fn verify(&self, budget: &Budget<'_>) -> Result<()> {
        let current = Self::capture(&self.requested_path, self.bytes, budget).map_err(|error| {
            match error {
                // A previously valid regular file that grew or changed type
                // is a changed input, not an invalid original request.
                Error::InvalidInput(_) => {
                    Error::InputChanged(self.requested_path.display().to_string())
                }
                other => other,
            }
        })?;
        if current.canonical_path != self.canonical_path
            || current.bytes != self.bytes
            || current.sha256 != self.sha256
        {
            return Err(Error::InputChanged(
                self.requested_path.display().to_string(),
            ));
        }
        Ok(())
    }
}

/// Reads/hash the same bytes. Do not parse one read and hash a separate read;
/// that could attach a newer identity to an older subtitle snapshot.
pub(super) fn read_subtitles(path: &Path, budget: &Budget<'_>) -> Result<(FileIdentity, Vec<u8>)> {
    let mut bytes = Vec::new();
    let identity = read_hashed(path, crate::srt_limits::MAX_BYTES as u64, budget, |chunk| {
        bytes.extend_from_slice(chunk)
    })?;
    Ok((identity, bytes))
}

fn read_hashed(
    path: &Path,
    limit: u64,
    budget: &Budget<'_>,
    mut consume: impl FnMut(&[u8]),
) -> Result<FileIdentity> {
    budget.check()?;
    let io_error = |e| Error::Io(format!("Cannot read {}: {e}", path.display()));
    let requested_path = std::path::absolute(path).map_err(io_error)?;
    let canonical_path = requested_path.canonicalize().map_err(io_error)?;
    // Check type before opening, so an ordinary named pipe is never opened for
    // a blocking read. A hostile concurrent file-type replacement is outside
    // this desktop application's snapshot guarantees.
    let metadata = fs::metadata(&canonical_path).map_err(io_error)?;
    if !metadata.is_file() || metadata.len() > limit {
        return Err(Error::InvalidInput(format!(
            "{} must be a regular file of at most {limit} bytes",
            path.display()
        )));
    }
    let mut file = fs::File::open(&canonical_path).map_err(io_error)?;
    let opened = file.metadata().map_err(io_error)?;
    if !opened.is_file()
        || opened.len() != metadata.len()
        || opened.modified().ok() != metadata.modified().ok()
    {
        return Err(Error::InputChanged(path.display().to_string()));
    }
    let mut digest = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 65_536];
    loop {
        budget.check()?;
        let count = file.read(&mut buffer).map_err(io_error)?;
        if count == 0 {
            break;
        }
        total = total
            .checked_add(count as u64)
            .ok_or_else(|| Error::InvalidInput("Input size overflow".into()))?;
        if total > metadata.len() || total > limit {
            return Err(Error::InputChanged(path.display().to_string()));
        }
        digest.update(&buffer[..count]);
        consume(&buffer[..count]);
    }
    budget.check()?;
    let after = file.metadata().map_err(io_error)?;
    if total != metadata.len()
        || after.len() != total
        || opened.modified().ok() != after.modified().ok()
        || requested_path.canonicalize().map_err(io_error)? != canonical_path
    {
        return Err(Error::InputChanged(path.display().to_string()));
    }
    Ok(FileIdentity {
        requested_path,
        canonical_path,
        bytes: total,
        sha256: format!("{:x}", digest.finalize()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resync::Cancellation;
    use std::time::Duration;

    #[test]
    fn same_length_and_modified_time_do_not_hide_changed_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("source");
        fs::write(&path, b"original").unwrap();
        let cancel = Cancellation::default();
        let budget = Budget::new(&cancel, Duration::from_secs(5));
        let identity = FileIdentity::capture(&path, 1024, &budget).unwrap();
        let modified = fs::metadata(&path).unwrap().modified().unwrap();
        fs::write(&path, b"modified").unwrap();
        fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(modified))
            .unwrap();
        assert!(matches!(
            identity.verify(&budget),
            Err(Error::InputChanged(_))
        ));
        assert!(FileIdentity::capture(&path, 7, &budget).is_err());
        assert!(FileIdentity::capture(dir.path(), 1024, &budget).is_err());
        fs::write(&path, b"longer modified content").unwrap();
        assert!(matches!(
            identity.verify(&budget),
            Err(Error::InputChanged(_))
        ));
    }

    #[test]
    fn mid_hash_cancellation_and_growth_never_return_an_identity() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("source");
        fs::write(&path, vec![0; 131_072]).unwrap();
        let cancel = Cancellation::default();
        let budget = Budget::new(&cancel, Duration::from_secs(5));
        assert!(matches!(
            read_hashed(&path, 200_000, &budget, |_| cancel.cancel()),
            Err(Error::Cancelled)
        ));
        let cancel = Cancellation::default();
        let budget = Budget::new(&cancel, Duration::from_secs(5));
        let mut grew = false;
        let result = read_hashed(&path, 200_000, &budget, |_| {
            if !grew {
                use std::io::Write;
                fs::OpenOptions::new()
                    .append(true)
                    .open(&path)
                    .unwrap()
                    .write_all(b"growth")
                    .unwrap();
                grew = true;
            }
        });
        assert!(matches!(result, Err(Error::InputChanged(_))));
    }
}
