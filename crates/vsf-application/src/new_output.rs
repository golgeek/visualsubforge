//! Paths for explicit new-file publication. Existing entries, including dangling
//! symlinks, are refused; atomic no-clobber publication still arbitrates races.
use std::{
    fmt, fs,
    path::{Path, PathBuf},
};
use unicode_normalization::UnicodeNormalization;

#[derive(Debug)]
pub enum Error {
    InvalidInput(String),
    Io(String),
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(message) | Self::Io(message) => f.write_str(message),
        }
    }
}
impl std::error::Error for Error {}

fn location(path: &Path) -> Result<PathBuf, Error> {
    let absolute = std::path::absolute(path).map_err(|e| Error::Io(e.to_string()))?;
    let name = absolute
        .file_name()
        .ok_or_else(|| Error::InvalidInput("Output needs a file name".into()))?;
    let parent = absolute
        .parent()
        .ok_or_else(|| Error::InvalidInput("Output needs a parent directory".into()))?
        .canonicalize()
        .map_err(|e| Error::Io(format!("Cannot resolve output directory: {e}")))?;
    if !parent.is_dir() {
        return Err(Error::InvalidInput(
            "Output parent is not a directory".into(),
        ));
    }
    Ok(parent.join(name))
}
pub fn resolve(path: &Path) -> Result<PathBuf, Error> {
    let path = location(path)?;
    require_absent(&path)?;
    Ok(path)
}
pub fn require_absent(path: &Path) -> Result<(), Error> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(Error::Io(format!(
            "Cannot inspect output {}: {error}",
            path.display()
        ))),
        Ok(_) => Err(Error::InvalidInput(format!(
            "Output already exists: {}",
            path.display()
        ))),
    }
}
/// Protect provenance even when the original file was deleted after opening.
/// Existing file/symlink/hardlink aliases are already rejected by resolve.
pub fn protect_source(output: &Path, source: &Path) -> Result<(), Error> {
    let absolute = std::path::absolute(source).map_err(|e| Error::Io(e.to_string()))?;
    // Be conservative even on case-sensitive filesystems: a deleted provenance
    // name must not become writable merely through case/Unicode normalization.
    let same = |path: &Path| {
        output == path
            || output
                .to_string_lossy()
                .nfd()
                .flat_map(char::to_lowercase)
                .eq(path.to_string_lossy().nfd().flat_map(char::to_lowercase))
    };
    if same(&absolute) || location(source).is_ok_and(|resolved| same(&resolved)) {
        return Err(Error::InvalidInput(
            "Choose a different file to preserve the original subtitles".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deleted_source_provenance_survives_path_aliases() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("Original-é.ass");
        fs::write(&source, "original").unwrap();
        assert!(resolve(&source).is_err());
        fs::remove_file(&source).unwrap();
        let output = resolve(&root.path().join("original-e\u{301}.ASS")).unwrap();
        assert!(protect_source(&output, &source).is_err());
        assert!(
            protect_source(
                &resolve(&root.path().join("converted.ass")).unwrap(),
                &source
            )
            .is_ok()
        );
        #[cfg(unix)]
        {
            let alias = root.path().join("alias");
            std::os::unix::fs::symlink(root.path(), &alias).unwrap();
            assert!(
                protect_source(&resolve(&alias.join("Original-é.ass")).unwrap(), &source).is_err()
            );
            let dangling = root.path().join("dangling.ass");
            std::os::unix::fs::symlink(&source, &dangling).unwrap();
            assert!(resolve(&dangling).is_err());
        }
    }
}
