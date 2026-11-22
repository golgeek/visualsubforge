//! Atomic sibling replacement shared by saved documents and recovery records.
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
type Result<T> = std::result::Result<T, String>;
pub struct Prepared {
    temp: tempfile::NamedTempFile,
    path: PathBuf,
}
impl Prepared {
    /// Publish complete bytes only if the destination is still absent. The
    /// no-clobber operation, not a prior existence check, arbitrates writers.
    /// Some filesystems can leave the temporary hard link after a crash; neither
    /// this nor replacement promises directory durability across power loss.
    pub fn commit_new(self) -> Result<()> {
        self.temp.persist_noclobber(&self.path).map_err(|e| {
            format!(
                "Cannot publish new file {}: {}",
                self.path.display(),
                e.error
            )
        })?;
        Ok(())
    }
    pub fn commit(self) -> Result<()> {
        self.temp
            .persist(&self.path)
            .map_err(|e| format!("Cannot replace document file: {}", e.error))?;
        Ok(())
    }
}
pub fn write(path: &Path, bytes: &[u8]) -> Result<()> {
    prepare(path, bytes)?.commit()
}
pub fn prepare(path: &Path, bytes: &[u8]) -> Result<Prepared> {
    prepare_bytes(path, bytes, NewFile::Output)
}

/// Application bookkeeping can contain document text and local paths.
pub fn prepare_private(path: &Path, bytes: &[u8]) -> Result<Prepared> {
    prepare_bytes(path, bytes, NewFile::Private)
}

pub fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    prepare_private(path, bytes)?.commit()
}

#[derive(Clone, Copy)]
enum NewFile {
    Output,
    Private,
}

fn prepare_bytes(path: &Path, bytes: &[u8], creation: NewFile) -> Result<Prepared> {
    prepare_with_io(
        path,
        bytes,
        creation,
        |file, bytes| file.write_all(bytes),
        |file| file.sync_all(),
    )
}

/// Bounded writes check cancellation before/after IO and between 64 KiB chunks.
/// A blocking filesystem call cannot itself be interrupted safely. Dropping the
/// prepared value removes its temporary without publishing a destination.
pub fn prepare_with_checkpoint(
    path: &Path,
    bytes: &[u8],
    mut checkpoint: impl FnMut() -> Result<()>,
) -> Result<Prepared> {
    checkpoint()?;
    let prepared = prepare_with_io(
        path,
        bytes,
        NewFile::Output,
        |file, bytes| {
            for chunk in bytes.chunks(64 * 1024) {
                checkpoint().map_err(std::io::Error::other)?;
                file.write_all(chunk)?;
            }
            checkpoint().map_err(std::io::Error::other)
        },
        |file| file.sync_all(),
    )?;
    checkpoint()?;
    Ok(prepared)
}

// Keep failure injection private to this boundary. Tests use real temporary
// files, replacing only the write/sync operations that can fail before publish.
fn prepare_with_io(
    path: &Path,
    bytes: &[u8],
    creation: NewFile,
    write: impl FnOnce(&mut fs::File, &[u8]) -> std::io::Result<()>,
    sync: impl FnOnce(&fs::File) -> std::io::Result<()>,
) -> Result<Prepared> {
    let parent = path.parent().ok_or("Save path has no parent directory")?;
    let permissions = match fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => Some(metadata.permissions()),
        Ok(_) => return Err("Save destination must be a regular file".into()),
        #[cfg(unix)]
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            return Err(format!("Cannot inspect save destination: {error}"));
        }
        Err(_) => None,
    };
    let new_output = permissions.is_none() && matches!(creation, NewFile::Output);
    let mut temp = sibling_tempfile(parent, new_output)
        .map_err(|e| format!("Cannot create temporary document file: {e}"))?;
    if let Some(permissions) = permissions {
        temp.as_file()
            .set_permissions(permissions)
            .map_err(|e| e.to_string())?;
    }
    write(temp.as_file_mut(), bytes)
        .and_then(|_| sync(temp.as_file()))
        .map_err(|e| format!("Cannot write document: {e}"))?;
    Ok(Prepared {
        temp,
        path: path.to_owned(),
    })
}

fn sibling_tempfile(parent: &Path, new_output: bool) -> std::io::Result<tempfile::NamedTempFile> {
    #[cfg(unix)]
    if new_output {
        use std::os::unix::fs::PermissionsExt;
        // Pass the mode to open(2). The kernel applies the umask atomically.
        return tempfile::Builder::new()
            .permissions(fs::Permissions::from_mode(0o666))
            .tempfile_in(parent);
    }
    #[cfg(not(unix))]
    let _ = new_output;
    tempfile::NamedTempFile::new_in(parent)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn new_outputs_follow_normal_creation_and_overwrites_preserve_the_existing_mode() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let control = dir.path().join("ordinary");
        fs::File::create_new(&control).unwrap();
        let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o7777;
        let path = dir.path().join("published");
        prepare(&path, b"first").unwrap().commit_new().unwrap();
        assert_eq!(mode(&path), mode(&control));
        let save_as = dir.path().join("save-as");
        write(&save_as, b"new").unwrap();
        assert_eq!(mode(&save_as), mode(&control));
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        write(&path, b"replacement").unwrap();
        assert_eq!(mode(&path), 0o640);
        let private = dir.path().join("snapshot.json");
        write_private(&private, b"draft").unwrap();
        assert_eq!(mode(&private) & 0o077, 0);
    }

    #[test]
    fn cancellation_at_each_write_checkpoint_removes_only_owned_temporary() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("output.ass");
        let bytes = vec![42; 200_000];
        let mut calls = 0;
        drop(
            prepare_with_checkpoint(&path, &bytes, || {
                calls += 1;
                Ok(())
            })
            .unwrap(),
        );
        for existing in [false, true] {
            if existing {
                fs::write(&path, b"original").unwrap();
            }
            for stop in 1..=calls {
                let mut at = 0;
                assert!(
                    prepare_with_checkpoint(&path, &bytes, || {
                        at += 1;
                        if at == stop {
                            Err("cancel-once".into())
                        } else {
                            Ok(())
                        }
                    })
                    .err()
                    .unwrap()
                    .contains("cancel-once")
                );
                assert_eq!(
                    fs::read_dir(directory.path()).unwrap().count(),
                    usize::from(existing)
                );
                if existing {
                    assert_eq!(fs::read(&path).unwrap(), b"original");
                }
            }
        }
    }

    #[test]
    fn partial_write_and_sync_failures_preserve_destination_and_remove_temporary() {
        for existing in [false, true] {
            for fail_write in [false, true] {
                let directory = tempfile::tempdir().unwrap();
                let target = directory.path().join("subtitles.srt");
                if existing {
                    fs::write(&target, b"original").unwrap();
                }
                let sync_called = std::cell::Cell::new(false);
                let result = prepare_with_io(
                    &target,
                    b"replacement",
                    NewFile::Output,
                    |file, bytes| {
                        if fail_write {
                            file.write_all(&bytes[..3])?;
                            Err(std::io::Error::other("injected write failure"))
                        } else {
                            file.write_all(bytes)
                        }
                    },
                    |file| {
                        sync_called.set(true);
                        assert_eq!(file.metadata()?.len(), 11);
                        Err(std::io::Error::other("injected sync failure"))
                    },
                );
                assert_eq!(sync_called.get(), !fail_write);
                let error = result
                    .err()
                    .expect("failed IO must not prepare a publishable file");
                assert!(error.contains(if fail_write {
                    "injected write failure"
                } else {
                    "injected sync failure"
                }));
                if existing {
                    assert_eq!(fs::read(&target).unwrap(), b"original");
                } else {
                    assert!(!target.exists());
                }
                assert_eq!(
                    fs::read_dir(directory.path()).unwrap().count(),
                    usize::from(existing)
                );
            }
        }
    }

    #[test]
    fn abandoning_prepared_write_preserves_existing_bytes_and_cleans_temporary() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("snapshot.json");
        fs::write(&target, b"previous durable bytes").unwrap();
        let pending = prepare(&target, b"replacement").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"previous durable bytes");
        drop(pending);
        assert_eq!(fs::read(&target).unwrap(), b"previous durable bytes");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
        assert!(prepare(directory.path(), b"invalid destination").is_err());
        assert_eq!(fs::read(&target).unwrap(), b"previous durable bytes");
        write(&target, b"replacement").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"replacement");
    }
}
