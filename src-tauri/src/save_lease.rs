//! Coordinates project and subtitle writers across atomic file replacement.
//! Callers resolve the destination's canonical parent/existing symlink first and
//! hold the returned lease through comparison, replacement and state publication.
//! Advisory locking excludes cooperating VSF instances, not external editors.
use crate::file_lease::FileLease;
use std::{fs, path::Path};

pub(crate) fn acquire(path: &Path) -> Result<FileLease, String> {
    match fs::metadata(path) {
        Ok(metadata) if !metadata.is_file() => {
            return Err("Save destination must be a regular file".into());
        }
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            return Err(format!("Cannot inspect save destination: {error}"));
        }
        _ => {}
    }
    let mut name = std::ffi::OsString::from(".");
    name.push(path.file_name().ok_or("Save filename is missing")?);
    name.push(".vsf-lock");
    // Never unlink this sibling: replacing the lock inode permits two owners.
    // The lease releases its OS lock; process termination closes the descriptor.
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path.with_file_name(name))
        .map_err(|e| format!("Cannot lock file for saving: {e}"))?;
    match FileLease::try_lock(file) {
        Ok(lease) => Ok(lease),
        Err(std::fs::TryLockError::WouldBlock) => {
            Err("File is being saved by another instance; retry when it finishes".into())
        }
        Err(error) => Err(format!("Cannot lock file for saving: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_survives_replacement_excludes_another_process_and_releases_on_drop() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("shared.srt");
        let lease = acquire(&path).unwrap();
        let child = |expected| {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "save_lease::tests::save_lock_child", "--ignored"])
                .env("VSF_SAVE_LOCK_TEST_PATH", &path)
                .env("VSF_SAVE_LOCK_TEST_EXPECT", expected)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stdout)
            );
        };
        child("locked");
        crate::atomic_file::write(&path, b"first version").unwrap();
        child("locked");
        crate::atomic_file::write(&path, b"replacement").unwrap();
        child("locked");
        drop(lease);
        child("available");
        assert_eq!(fs::read(path).unwrap(), b"replacement");
    }

    #[test]
    #[ignore = "Subprocess entry point for save lock ownership regression"]
    fn save_lock_child() {
        let path = std::path::PathBuf::from(
            std::env::var_os("VSF_SAVE_LOCK_TEST_PATH").expect("parent supplies path"),
        );
        let expected = std::env::var("VSF_SAVE_LOCK_TEST_EXPECT").unwrap();
        assert_eq!(acquire(&path).is_ok(), expected == "available");
    }
}
