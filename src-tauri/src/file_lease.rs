//! Scoped ownership of an OS advisory lock; no UI or native-media affinity.
use std::fs::{File, TryLockError};

pub(crate) struct FileLease(File);

impl FileLease {
    pub fn lock(file: File) -> std::io::Result<Self> {
        file.lock()?;
        Ok(Self(file))
    }

    pub fn try_lock(file: File) -> Result<Self, TryLockError> {
        file.try_lock()?;
        Ok(Self(file))
    }
}

impl Drop for FileLease {
    fn drop(&mut self) {
        // Closing alone can retain a lock until a concurrently spawned child
        // closes its inherited duplicate during exec. The scoped owner releases
        // explicitly, then File's drop closes our descriptor even if unlock fails.
        let _ = self.0.unlock();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owner_drop_releases_the_lock_even_while_a_duplicate_handle_exists() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("lease");
        let file = File::options()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        let owner = FileLease::lock(file).unwrap();
        // Models the inherited descriptor between process creation and exec.
        let duplicate = owner.0.try_clone().unwrap();
        let open = || File::options().read(true).write(true).open(&path).unwrap();
        assert!(matches!(
            FileLease::try_lock(open()),
            Err(TryLockError::WouldBlock)
        ));
        drop(owner);
        let next = FileLease::try_lock(open()).unwrap();
        drop(duplicate);
        assert!(matches!(
            FileLease::try_lock(open()),
            Err(TryLockError::WouldBlock)
        ));
        drop(next);
        assert!(FileLease::try_lock(open()).is_ok());
    }
}
