//! Rust owns the current save baseline and at most one staged open.
//! The frontend carries opaque tokens, adopting a staged open only after restore.
//! All calls run on blocking workers; the mutex serializes this process's writes.
use super::*;
use std::sync::Mutex;

#[cfg(test)]
mod tests;

#[derive(Default)]
pub struct Projects(Mutex<Store>);

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
}
#[derive(Serialize)]
pub struct Attachment {
    pub path: String,
    pub token: u64,
}
#[derive(Serialize)]
pub struct Opened {
    #[serde(flatten)]
    project: Project,
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
            .filter(|token| *token <= 9_007_199_254_740_991)
            .ok_or("Project save identities exhausted")?;
        Ok(self.next_token)
    }
}
impl Projects {
    pub fn open(&self, path: &Path) -> Result<Opened> {
        let mut store = self.0.lock().map_err(|_| "Project state unavailable")?;
        let path = path
            .canonicalize()
            .map_err(|e| format!("Cannot open project: {e}"))?;
        let bytes = read_bytes(&path)?;
        let project = resolve(&path, &bytes)?;
        let token = store.token()?;
        store.pending = Some(Baseline { token, path, bytes });
        Ok(Opened { project, token })
    }
    pub fn attach(&self, token: u64) -> Result<Attachment> {
        let mut store = self.0.lock().map_err(|_| "Project state unavailable")?;
        if store
            .pending
            .as_ref()
            .is_none_or(|pending| pending.token != token)
        {
            return Err("Project open was replaced; open the project again".into());
        }
        let pending = store.pending.take().ok_or("Project open unavailable")?;
        let attachment = pending.attachment();
        store.active = Some(pending);
        Ok(attachment)
    }
    pub fn detach(&self) -> Result<()> {
        let mut store = self.0.lock().map_err(|_| "Project state unavailable")?;
        store.active = None;
        store.pending = None;
        Ok(())
    }
    pub fn save(&self, path: &Path, token: Option<u64>, project: &Project) -> Result<Attachment> {
        let mut store = self.0.lock().map_err(|_| "Project state unavailable")?;
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
        let token = store.token()?;
        crate::atomic_file::write(&path, &bytes)?;
        let saved = Baseline { token, path, bytes };
        let attachment = saved.attachment();
        store.active = Some(saved);
        // A staged read must not replace the just-saved attachment later.
        store.pending = None;
        Ok(attachment)
    }
}
