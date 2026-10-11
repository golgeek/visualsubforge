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
    let parent = path.parent().ok_or("Save path has no parent directory")?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)
        .map_err(|e| format!("Cannot create temporary document file: {e}"))?;
    if let Ok(metadata) = fs::metadata(path) {
        if !metadata.is_file() {
            return Err("Save destination must be a regular file".into());
        }
        temp.as_file()
            .set_permissions(metadata.permissions())
            .map_err(|e| e.to_string())?;
    }
    temp.write_all(bytes)
        .and_then(|_| temp.as_file().sync_all())
        .map_err(|e| format!("Cannot write document: {e}"))?;
    Ok(Prepared {
        temp,
        path: path.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
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
