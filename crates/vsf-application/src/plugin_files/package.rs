//! Explicit local packages; never search outside the selected manifest directory.
use super::{
    Budget, Error, Result,
    input::{CapturedFile, FileIdentity, changed},
};
use crate::plugins::{MAX_FILES, MAX_PACKAGE_BYTES, Manifest, Package};
use std::{collections::BTreeMap, fs, path::Path};

const MAX_MANIFEST_BYTES: usize = 512 * 1024;
const MAX_ENTRIES: usize = 1024;
const MAX_DEPTH: usize = 16;

pub struct LoadedPackage {
    pub(super) package: Package,
    pub(super) fingerprint: String,
    pub(super) files: Vec<FileIdentity>,
}
impl LoadedPackage {
    pub fn read(manifest: &Path, budget: &Budget<'_>) -> Result<Self> {
        budget.check()?;
        let kind = fs::symlink_metadata(manifest)
            .map_err(|e| Error::Io(format!("Cannot inspect plugin manifest: {e}")))?;
        if !kind.is_file() || kind.is_symlink() {
            return Err(Error::Invalid(
                "Choose a regular plugin manifest; symbolic links are unsupported".into(),
            ));
        }
        let captured = CapturedFile::read(manifest, MAX_MANIFEST_BYTES, budget)?;
        let manifest: Manifest = serde_json::from_slice(&captured.bytes)
            .map_err(|e| Error::Invalid(format!("Invalid plugin manifest: {e}")))?;
        let root = captured
            .identity
            .canonical_path
            .parent()
            .ok_or_else(|| Error::Invalid("Plugin manifest needs a parent directory".into()))?
            .to_owned();
        let mut files = vec![captured.identity];
        let mut scripts = BTreeMap::new();
        let mut pending = vec![(root.clone(), 0)];
        let mut entries = 0;
        let mut bytes = 0;
        while let Some((directory, depth)) = pending.pop() {
            budget.check()?;
            if depth > MAX_DEPTH {
                return Err(Error::Invalid(
                    "Plugin package directory nesting exceeds 16".into(),
                ));
            }
            for entry in fs::read_dir(&directory)
                .map_err(|e| Error::Io(format!("Cannot list plugin package: {e}")))?
            {
                budget.check()?;
                entries += 1;
                if entries > MAX_ENTRIES {
                    return Err(Error::Invalid(
                        "Plugin package has more than 1024 directory entries".into(),
                    ));
                }
                let entry = entry.map_err(|e| Error::Io(e.to_string()))?;
                let path = entry.path();
                let kind = fs::symlink_metadata(&path).map_err(|e| Error::Io(e.to_string()))?;
                if kind.is_symlink() || (!kind.is_dir() && !kind.is_file()) {
                    return Err(Error::Invalid(
                        "Plugin packages cannot contain symbolic links or special files".into(),
                    ));
                }
                let canonical = path.canonicalize().map_err(|e| Error::Io(e.to_string()))?;
                if !canonical.starts_with(&root) {
                    return Err(Error::Invalid(
                        "Plugin package path escaped its root".into(),
                    ));
                }
                if kind.is_dir() {
                    pending.push((path, depth + 1));
                    continue;
                }
                if path.extension().is_none_or(|e| e != "js") {
                    continue;
                }
                if scripts.len() >= MAX_FILES {
                    return Err(Error::Invalid(
                        "Plugin package has more than 64 JavaScript files".into(),
                    ));
                }
                let relative = path
                    .strip_prefix(&root)
                    .map_err(|e| Error::Invalid(e.to_string()))?;
                let name = relative
                    .components()
                    .map(|part| {
                        part.as_os_str()
                            .to_str()
                            .ok_or_else(|| Error::Invalid("Plugin filenames must be UTF-8".into()))
                    })
                    .collect::<Result<Vec<_>>>()?
                    .join("/");
                let file = CapturedFile::read(&path, MAX_PACKAGE_BYTES - bytes, budget)?;
                if !file.identity.canonical_path.starts_with(&root) {
                    return Err(Error::Changed(path.display().to_string()));
                }
                bytes += file.bytes.len();
                let text = String::from_utf8(file.bytes)
                    .map_err(|_| Error::Invalid(format!("Plugin script {name} must be UTF-8")))?;
                scripts.insert(name, text);
                files.push(file.identity);
            }
        }
        files[1..].sort_by(|a, b| a.requested_path.cmp(&b.requested_path));
        let package = Package {
            manifest,
            files: scripts,
        };
        let fingerprint = budget.pure(package.fingerprint())?;
        Ok(Self {
            package,
            fingerprint,
            files,
        })
    }
    pub fn package(&self) -> &Package {
        &self.package
    }
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }
    pub fn files(&self) -> &[FileIdentity] {
        &self.files
    }
    pub fn manifest_path(&self) -> &Path {
        &self.files[0].requested_path
    }
    pub fn verify(&self, budget: &Budget<'_>) -> Result<()> {
        let now = Self::read(self.manifest_path(), budget)
            .map_err(|e| changed(e, self.manifest_path()))?;
        if now.files != self.files || now.fingerprint != self.fingerprint {
            return Err(Error::Changed(self.manifest_path().display().to_string()));
        }
        Ok(())
    }
    pub(super) fn protect(&self, output: &Path) -> Result<()> {
        for file in &self.files {
            super::protect(output, file)?;
        }
        Ok(())
    }
}
