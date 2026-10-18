//! Rust owns the current save baseline and at most one staged open.
//! The frontend carries opaque tokens, adopting a staged open only after restore.
//! All calls run on blocking workers; the mutex serializes this process's writes.
use super::*;
use std::sync::Mutex;

#[cfg(test)]
mod tests;

#[derive(Default)]
pub struct Projects {
    state: Mutex<Store>,
    pub(crate) imports: super::import::Importer,
}

#[derive(Default)]
struct Store {
    next_token: u64,
    active: Option<Baseline>,
    pending: Option<Baseline>,
}
struct Baseline {
    token: u64,
    path: PathBuf,
    bytes: Vec<u8>,
    policy: crate::quality_profile::Policy,
    media_path: PathBuf,
}
#[derive(Serialize, specta::Type)]
pub struct Attachment {
    pub path: String,
    #[serde(with = "crate::ipc_number")]
    #[specta(type = crate::ipc_number::SafeU64)]
    pub token: u64,
}
#[derive(Serialize, specta::Type)]
pub struct Opened {
    #[serde(flatten)]
    project: Project,
    #[serde(with = "crate::ipc_number")]
    #[specta(type = crate::ipc_number::SafeU64)]
    pub token: u64,
}
impl Baseline {
    fn attachment(&self) -> Attachment {
        Attachment {
            path: self.path.to_string_lossy().into_owned(),
            token: self.token,
        }
    }
}
impl Store {
    fn token(&mut self) -> Result<u64> {
        self.next_token = self
            .next_token
            .checked_add(1)
            .filter(|token| *token <= crate::ipc_number::MAX_SAFE_INTEGER)
            .ok_or("Project save identities exhausted")?;
        Ok(self.next_token)
    }
}
impl Projects {
    pub fn open(&self, path: &Path) -> Result<Opened> {
        let mut store = self.state.lock().map_err(|_| "Project state unavailable")?;
        let path = path
            .canonicalize()
            .map_err(|e| format!("Cannot open project: {e}"))?;
        let bytes = read_bytes(&path)?;
        let project = resolve(&path, &bytes)?;
        let token = store.token()?;
        let policy = match &project.options.quality {
            Some(profile) => crate::quality_profile::Policy::new(
                profile.clone(),
                project.options.timing.clone(),
            )?,
            None => {
                crate::quality_profile::Policy::legacy(project.options.timing.clone(), "project")?
            }
        };
        store.pending = Some(Baseline {
            token,
            path,
            bytes,
            policy,
            media_path: project.media_path.clone().into(),
        });
        Ok(Opened { project, token })
    }
    #[cfg(test)]
    pub fn attach(&self, token: u64) -> Result<Attachment> {
        self.attach_with(token, None, |_| {})
    }
    pub(crate) fn attach_with(
        &self,
        token: u64,
        media_path: Option<&Path>,
        accept: impl FnOnce(&crate::quality_profile::Policy),
    ) -> Result<Attachment> {
        let mut store = self.state.lock().map_err(|_| "Project state unavailable")?;
        if store
            .pending
            .as_ref()
            .is_none_or(|pending| pending.token != token)
        {
            return Err("Project open was replaced; open the project again".into());
        }
        if media_path.is_some_and(|path| {
            store
                .pending
                .as_ref()
                .is_none_or(|pending| pending.media_path != path)
        }) {
            return Err("Project media changed during restoration".into());
        }
        let pending = store.pending.take().ok_or("Project open unavailable")?;
        // Infallible assignment callback, with document -> project lock order.
        accept(&pending.policy);
        let attachment = pending.attachment();
        store.active = Some(pending);
        Ok(attachment)
    }
    pub fn detach(&self) -> Result<()> {
        let mut store = self.state.lock().map_err(|_| "Project state unavailable")?;
        store.active = None;
        store.pending = None;
        Ok(())
    }
    pub fn save(&self, path: &Path, token: Option<u64>, project: &Project) -> Result<Attachment> {
        let mut store = self.state.lock().map_err(|_| "Project state unavailable")?;
        let active = match token {
            Some(token) => Some(
                store
                    .active
                    .as_ref()
                    .filter(|saved| saved.token == token)
                    .ok_or(
                        "Project changed; reopen it or use Save As from the current workspace",
                    )?,
            ),
            None => None,
        };
        let path = destination(path)?;
        let bytes = serialize(&path, project)?;
        let _lease = crate::save_lease::acquire(&path)?;
        if let Some(saved) = active.filter(|saved| saved.path == path)
            && read_bytes(&path).ok().as_ref() != Some(&saved.bytes)
        {
            return Err("Project changed on disk. Reopen it or use Save Project As to preserve both versions".into());
        }
        // Selecting a different destination through Save As explicitly permits
        // replacing that file. The attached path always retains conflict checking.
        let policy = match &project.options.quality {
            Some(profile) => crate::quality_profile::Policy::new(
                profile.clone(),
                project.options.timing.clone(),
            )?,
            None => {
                crate::quality_profile::Policy::legacy(project.options.timing.clone(), "project")?
            }
        };
        let token = store.token()?;
        crate::atomic_file::write(&path, &bytes)?;
        let saved = Baseline {
            token,
            path,
            bytes,
            policy,
            media_path: project.media_path.clone().into(),
        };
        let attachment = saved.attachment();
        store.active = Some(saved);
        // A staged read must not replace the just-saved attachment later.
        store.pending = None;
        Ok(attachment)
    }
}

#[cfg(test)]
pub(super) fn ipc_sample(project: Project) -> serde_json::Value {
    serde_json::to_value(Opened {
        project,
        token: crate::ipc_number::MAX_SAFE_INTEGER,
    })
    .unwrap()
}
