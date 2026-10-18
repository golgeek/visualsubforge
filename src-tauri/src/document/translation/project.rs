//! Portable project references. Full reference contents belong only to accepted
//! context and recovery; a project records the explicit expected byte identity.
use super::*;
use crate::project::{absolute, portable};

#[derive(Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectContext {
    reference: Option<ProjectReference>,
    copied_from: Option<ProjectOrigin>,
}
#[derive(Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectReference {
    path: String,
    format: String,
    sha256: String,
    offset_ms: SafeI64,
}
#[derive(Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectOrigin {
    path: String,
    selected: String,
}

#[derive(Clone, Default, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReferenceOverride {
    path: Option<String>,
    #[serde(default)]
    reload: bool,
}
#[derive(Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceIssue {
    pub path: String,
    pub message: String,
    pub can_reload: bool,
}

fn validate_path(path: &str) -> Result<()> {
    if path.is_empty()
        || path.len() > 32_768
        || path.contains('\0')
        || Path::new(path).file_name().is_none()
    {
        return Err("Project reference requires a filename of at most 32 KiB".into());
    }
    Ok(())
}
impl ProjectOrigin {
    fn capture(origin: &context::Origin, base: &Path) -> Self {
        Self {
            path: portable(base, &origin.path),
            selected: portable(base, &origin.selected),
        }
    }
    fn restore(&self, base: &Path) -> Result<context::Origin> {
        validate_path(&self.path)?;
        validate_path(&self.selected)?;
        let origin = context::Origin {
            path: absolute(base, &self.path),
            selected: absolute(base, &self.selected),
        };
        origin.validate()?;
        Ok(origin)
    }
}
impl Context {
    pub(crate) fn project_description(&self, base: &Path) -> Result<Option<ProjectContext>> {
        if self.reference.is_none() && self.copied_from.is_none() {
            return Ok(None);
        }
        Ok(Some(ProjectContext {
            reference: self
                .reference
                .as_ref()
                .map(|file| -> Result<_> {
                    Ok(ProjectReference {
                        path: portable(base, &file.origin.path),
                        format: file.reference.format().extension().into(),
                        sha256: file.reference.fingerprint().into(),
                        offset_ms: file.offset.get().try_into()?,
                    })
                })
                .transpose()?,
            copied_from: self
                .copied_from
                .as_ref()
                .map(|origin| ProjectOrigin::capture(origin, base)),
        }))
    }
    pub(crate) fn verify_project_reference(&self) -> Result<()> {
        if let Some(reference) = &self.reference {
            reference.verify(|| Ok(()))?;
        }
        Ok(())
    }
}
impl ProjectContext {
    pub(crate) fn validate(&self) -> Result<()> {
        if let Some(reference) = &self.reference {
            validate_path(&reference.path)?;
            subtitle_formats::Format::from_extension(&reference.format)
                .ok_or("Unknown project reference format")?;
            if reference.sha256.len() != 64
                || !reference
                    .sha256
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            {
                return Err(
                    "Project reference SHA-256 must contain 64 lowercase hexadecimal digits".into(),
                );
            }
            engine::Offset::new(reference.offset_ms.get())?;
        }
        if let Some(origin) = &self.copied_from {
            validate_path(&origin.path)?;
            validate_path(&origin.selected)?;
        }
        Ok(())
    }
    pub(crate) fn protect(&self, base: &Path, output: &Path) -> Result<()> {
        if let Some(reference) = &self.reference {
            let source = absolute(base, &reference.path);
            vsf_application::new_output::protect_source(output, &source)
                .map_err(|e| e.to_string())?;
            if let Ok(source) = source.canonicalize() {
                vsf_application::new_output::protect_source(output, &source)
                    .map_err(|e| e.to_string())?;
            }
        }
        if let Some(origin) = &self.copied_from {
            origin.restore(base)?.protect(output)?;
        }
        Ok(())
    }
    pub(crate) fn load(
        &self,
        base: &Path,
        overrides: &ReferenceOverride,
    ) -> std::result::Result<Context, ReferenceIssue> {
        let path = self
            .reference
            .as_ref()
            .map(|r| absolute(base, overrides.path.as_deref().unwrap_or(&r.path)));
        let issue = |message: String, can_reload| ReferenceIssue {
            path: path
                .as_ref()
                .map_or_else(String::new, |path| path.to_string_lossy().into_owned()),
            message,
            can_reload,
        };
        self.validate().map_err(|e| issue(e, false))?;
        let copied_from = self
            .copied_from
            .as_ref()
            .map(|o| o.restore(base))
            .transpose()
            .map_err(|e| issue(e, false))?;
        let mut dirty = false;
        let reference = match (&self.reference, path.as_ref()) {
            (Some(expected), Some(path)) => {
                let mut file = ReferenceFile::load(path, || Ok(())).map_err(|e| issue(e, false))?;
                let changed = file.reference.fingerprint() != expected.sha256
                    || file.reference.format().extension() != expected.format;
                if changed && !overrides.reload {
                    return Err(issue("The reference content changed. Explicitly reload it or choose the expected reference file.".into(), true));
                }
                file.offset =
                    engine::Offset::new(expected.offset_ms.get()).map_err(|e| issue(e, false))?;
                dirty = changed || overrides.path.is_some();
                Some(file)
            }
            _ => None,
        };
        Ok(Context {
            reference,
            copied_from,
            dirty,
        })
    }
}
