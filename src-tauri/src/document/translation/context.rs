//! Desktop paths/provenance and recovery representation. The reference engine
//! remains filesystem-free; restoration deliberately never rereads source files.
use super::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Origin {
    pub(crate) path: PathBuf,
    selected: PathBuf,
}
impl Origin {
    fn validate(&self) -> Result<()> {
        for path in [&self.path, &self.selected] {
            if !path.is_absolute() || path.as_os_str().len() > 32_768 {
                return Err("Reference paths must be absolute and at most 32 KiB".into());
            }
        }
        Ok(())
    }
    pub(super) fn protect(&self, output: &Path) -> Result<()> {
        for source in [&self.path, &self.selected] {
            vsf_application::new_output::protect_source(output, source)
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ReferenceFile {
    pub(crate) origin: Origin,
    pub(crate) reference: engine::Reference,
    pub(crate) offset: engine::Offset,
}
impl ReferenceFile {
    pub(super) fn load(path: &Path, mut checkpoint: impl FnMut() -> Result<()>) -> Result<Self> {
        if path.as_os_str().is_empty() || path.as_os_str().len() > 32_768 {
            return Err("Choose a reference filename with a supported path length".into());
        }
        checkpoint()?;
        let selected = std::path::absolute(path).map_err(|e| e.to_string())?;
        let path = selected
            .canonicalize()
            .map_err(|e| format!("Cannot open reference: {e}"))?;
        let origin = Origin { path, selected };
        origin.validate()?;
        let bytes = read_file(&origin.path)?;
        let reference = engine::Reference::parse(
            bytes,
            origin.path.extension().and_then(|s| s.to_str()),
            &mut checkpoint,
        )?;
        Ok(Self {
            origin,
            reference,
            offset: engine::Offset::default(),
        })
    }
    pub(super) fn verify(&self, mut checkpoint: impl FnMut() -> Result<()>) -> Result<()> {
        checkpoint()?;
        if self
            .origin
            .selected
            .canonicalize()
            .map_err(|e| format!("Reference is unavailable: {e}"))?
            != self.origin.path
            || read_file(&self.origin.path)? != self.reference.original_bytes()
        {
            return Err("Reference changed while loading; select it again".into());
        }
        checkpoint()
    }
    pub(super) fn metadata(&self) -> Result<ReferenceInfo> {
        Ok(ReferenceInfo {
            path: self.origin.path.to_string_lossy().into_owned(),
            format: self.reference.format().extension().into(),
            sha256: self.reference.fingerprint().into(),
            cues: self
                .reference
                .cues()
                .len()
                .try_into()
                .map_err(|_| "Too many reference cues")?,
            offset_ms: self.offset.get().try_into()?,
        })
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Context {
    pub(crate) reference: Option<ReferenceFile>,
    // Sticky for this target's lifetime, including after detaching a reference.
    pub(crate) copied_from: Option<Origin>,
    pub(crate) dirty: bool,
}
impl Context {
    pub(crate) fn is_empty(&self) -> bool {
        self.reference.is_none() && self.copied_from.is_none() && !self.dirty
    }
    pub(crate) fn protect(&self, output: &Path) -> Result<()> {
        if let Some(reference) = &self.reference {
            reference.origin.protect(output)?;
        }
        if let Some(origin) = &self.copied_from {
            origin.protect(output)?;
        }
        Ok(())
    }
    pub(crate) fn stored(&self) -> Result<Option<Stored>> {
        if self.is_empty() {
            return Ok(None);
        }
        Ok(Some(Stored {
            copied_from: self.copied_from.clone(),
            dirty: self.dirty,
            reference: self
                .reference
                .as_ref()
                .map(|file| -> Result<StoredReference> {
                    Ok(StoredReference {
                        origin: file.origin.clone(),
                        format: file.reference.format().extension().into(),
                        sha256: file.reference.fingerprint().into(),
                        text: String::from_utf8(file.reference.original_bytes().to_vec())
                            .map_err(|e| e.to_string())?,
                        offset_ms: file.offset.get(),
                    })
                })
                .transpose()?,
        }))
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Stored {
    copied_from: Option<Origin>,
    dirty: bool,
    reference: Option<StoredReference>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredReference {
    origin: Origin,
    format: String,
    sha256: String,
    text: String,
    offset_ms: i64,
}
impl Stored {
    pub(crate) fn restore(&self) -> Result<Context> {
        if let Some(origin) = &self.copied_from {
            origin.validate()?;
        }
        let reference = self
            .reference
            .as_ref()
            .map(|stored| -> Result<_> {
                stored.origin.validate()?;
                let format = subtitle_formats::Format::from_extension(&stored.format)
                    .ok_or("Unknown recovery reference format")?;
                let reference = engine::Reference::parse(
                    stored.text.as_bytes().to_vec(),
                    Some(&stored.format),
                    || Ok(()),
                )?;
                if reference.format() != format || reference.fingerprint() != stored.sha256 {
                    return Err(
                        "Recovery reference format or content identity does not match".into(),
                    );
                }
                Ok(ReferenceFile {
                    origin: stored.origin.clone(),
                    reference,
                    offset: engine::Offset::new(stored.offset_ms)?,
                })
            })
            .transpose()?;
        Ok(Context {
            reference,
            copied_from: self.copied_from.clone(),
            dirty: self.dirty,
        })
    }
}
