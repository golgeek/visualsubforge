//! Accepted document state and file operations. The async handler delegates all
//! work to blocking workers; the mutex serializes operations, never the UI thread.
use serde::{Deserialize, Serialize};
#[cfg(test)]
use std::fs;
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use subtitle_core::{CueId, Edit, Editor, Milliseconds};
use subtitle_formats::srt;

mod autosave;
mod checks;
mod gaps;
#[cfg(test)]
pub(crate) use checks::ipc_sample as quality_ipc_sample;
#[cfg(test)]
pub(crate) use gaps::ipc_sample as gap_ipc_sample;
mod quality;
mod resync;
pub(crate) use resync::Capture;
#[cfg(test)]
pub(crate) mod ipc_tests;
#[cfg(test)]
mod limits_tests;
#[cfg(test)]
mod save_tests;
use autosave::RecoveryControl;

type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Default)]
pub struct Documents(Arc<Mutex<Store>>, Arc<RecoveryControl>);
#[derive(Default)]
struct Store {
    generation: u64,
    loaded: Option<Loaded>,
    recovery_root: Option<PathBuf>,
    recovery_error: Option<String>,
    quality: quality::State,
    checks: checks::Checks,
    gaps: gaps::State,
}
struct Loaded {
    editor: Editor,
    source: srt::Source,
    path: Option<PathBuf>,
    // Used to reject overwriting a file changed outside this application.
    saved_bytes: Vec<u8>,
    recovered: bool,
    // The current path is source provenance until a different file is saved.
    // Independent of editor history: Undo must not remove this protection.
    resynced: bool,
}
#[derive(Deserialize, specta::Type)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Action {
    Status,
    Recover {
        id: String,
        discard: bool,
    },
    Open {
        path: String,
        discard: bool,
    },
    Close {
        discard: bool,
    },
    Edit {
        #[serde(with = "crate::ipc_number")]
        #[specta(type = crate::ipc_number::SafeU64)]
        cue_id: u64,
        #[serde(with = "crate::ipc_number")]
        #[specta(type = crate::ipc_number::SafeU64)]
        start_ms: u64,
        #[serde(with = "crate::ipc_number")]
        #[specta(type = crate::ipc_number::SafeU64)]
        end_ms: u64,
        text: String,
    },
    Create {
        #[serde(default, with = "crate::ipc_number::optional")]
        #[specta(type = Option<crate::ipc_number::SafeU64>)]
        after_id: Option<u64>,
        #[serde(with = "crate::ipc_number")]
        #[specta(type = crate::ipc_number::SafeU64)]
        start_ms: u64,
        #[serde(with = "crate::ipc_number")]
        #[specta(type = crate::ipc_number::SafeU64)]
        end_ms: u64,
        text: String,
    },
    Delete {
        #[serde(with = "crate::ipc_number")]
        #[specta(type = crate::ipc_number::SafeU64)]
        cue_id: u64,
    },
    Split {
        #[serde(with = "crate::ipc_number")]
        #[specta(type = crate::ipc_number::SafeU64)]
        cue_id: u64,
        #[serde(with = "crate::ipc_number")]
        #[specta(type = crate::ipc_number::SafeU64)]
        at_ms: u64,
    },
    Merge {
        #[serde(with = "crate::ipc_number")]
        #[specta(type = crate::ipc_number::SafeU64)]
        first_id: u64,
        #[serde(with = "crate::ipc_number")]
        #[specta(type = crate::ipc_number::SafeU64)]
        second_id: u64,
    },
    Undo,
    Redo,
    Save {
        path: Option<String>,
    },
}
#[derive(Serialize, Debug, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    #[serde(with = "crate::ipc_number")]
    #[specta(type = crate::ipc_number::SafeU64)]
    generation: u64,
    #[serde(with = "crate::ipc_number")]
    #[specta(type = crate::ipc_number::SafeU64)]
    revision: u64,
    loaded: bool,
    path: Option<String>,
    dirty: bool,
    requires_save_as: bool,
    can_undo: bool,
    can_redo: bool,
    cues: Vec<CueView>,
}
#[derive(Serialize, Debug, specta::Type)]
#[serde(rename_all = "camelCase")]
struct CueView {
    #[serde(with = "crate::ipc_number")]
    #[specta(type = crate::ipc_number::SafeU64)]
    id: u64,
    #[serde(with = "crate::ipc_number")]
    #[specta(type = crate::ipc_number::SafeU64)]
    start_ms: u64,
    #[serde(with = "crate::ipc_number")]
    #[specta(type = crate::ipc_number::SafeU64)]
    end_ms: u64,
    text: String,
}

impl Loaded {
    fn apply_edit(&mut self, revision: u64, edit: Edit) -> Result<()> {
        self.editor
            .apply_edit(revision, edit, |document| {
                crate::srt_limits::validate_document(document, &self.source)
            })
            .map(|_| ())
    }
}

impl Store {
    fn snapshot(&self) -> Snapshot {
        let loaded = self.loaded.as_ref();
        Snapshot {
            generation: self.generation,
            revision: loaded.map_or(0, |d| d.editor.revision()),
            loaded: loaded.is_some(),
            path: loaded
                .and_then(|d| d.path.as_ref())
                .map(|p| p.to_string_lossy().into_owned()),
            dirty: loaded.is_some_and(|d| d.editor.dirty() || d.recovered || d.resynced),
            requires_save_as: loaded.is_some_and(|d| d.recovered || d.resynced || d.path.is_none()),
            can_undo: loaded.is_some_and(|d| d.editor.can_undo()),
            can_redo: loaded.is_some_and(|d| d.editor.can_redo()),
            cues: loaded.map_or_else(Vec::new, |d| {
                d.editor
                    .document()
                    .cues()
                    .iter()
                    .map(|c| CueView {
                        id: c.id().0,
                        start_ms: c.start().get(),
                        end_ms: c.end().get(),
                        text: c.text().into(),
                    })
                    .collect()
            }),
        }
    }
    fn apply(&mut self, action: Action, generation: u64, revision: u64) -> Result<Snapshot> {
        if matches!(action, Action::Status) {
            return Ok(self.snapshot());
        }
        if generation != self.generation
            || revision != self.loaded.as_ref().map_or(0, |d| d.editor.revision())
        {
            return Err("Subtitle document changed; refresh before retrying this operation".into());
        }
        match action {
            Action::Status => unreachable!(),
            Action::Open { path, discard } => {
                self.check_discard(discard)?;
                let path = Path::new(&path)
                    .canonicalize()
                    .map_err(|e| format!("Cannot open subtitles: {e}"))?;
                let bytes = read_file(&path)?;
                let parsed = vsf_application::subtitle_file::parse(&bytes)?;
                // Commit only after every read and parse operation succeeds.
                self.loaded = Some(Loaded {
                    editor: Editor::new(parsed.document),
                    source: parsed.source,
                    path: Some(path),
                    saved_bytes: bytes,
                    recovered: false,
                    resynced: false,
                });
                self.generation += 1;
            }
            Action::Recover { id, discard } => {
                self.check_discard(discard)?;
                let root = self
                    .recovery_root
                    .as_ref()
                    .ok_or("Recovery storage is unavailable")?;
                let record = crate::recovery_store::recover(root, &id)?;
                let parsed = srt::parse(&record.accepted_srt)?;
                self.loaded = Some(Loaded {
                    editor: Editor::new(parsed.document),
                    source: parsed.source,
                    path: record.source_path.map(|path| {
                        save_path(Path::new(&path)).unwrap_or_else(|_| PathBuf::from(path))
                    }),
                    saved_bytes: record.saved_srt.into_bytes(),
                    recovered: true,
                    resynced: record.resynced,
                });
                self.generation += 1;
            }
            Action::Close { discard } => {
                self.check_discard(discard)?;
                self.loaded = None;
                self.generation += 1;
            }
            Action::Edit {
                cue_id,
                start_ms,
                end_ms,
                text,
            } => {
                srt::validate_text(&text)?;
                self.loaded_mut()?.apply_edit(
                    revision,
                    Edit::Text {
                        id: CueId(cue_id),
                        start: Milliseconds::new(start_ms)?,
                        end: Milliseconds::new(end_ms)?,
                        text,
                    },
                )?;
            }
            Action::Undo => {
                self.loaded_mut()?.editor.undo(revision)?;
            }
            Action::Create {
                after_id,
                start_ms,
                end_ms,
                text,
            } => {
                srt::validate_text(&text)?;
                // Build the first cue off to the side: invalid input must not
                // publish an empty document or consume a generation.
                let mut untitled = if self.loaded.is_none() {
                    let parsed = srt::parse("")?;
                    Some(Loaded {
                        editor: Editor::new(parsed.document),
                        source: parsed.source,
                        path: None,
                        saved_bytes: Vec::new(),
                        recovered: false,
                        resynced: false,
                    })
                } else {
                    None
                };
                let loaded = match untitled.as_mut() {
                    Some(loaded) => loaded,
                    None => self.loaded_mut()?,
                };
                loaded.apply_edit(
                    revision,
                    Edit::Create {
                        after: after_id.map(CueId),
                        start: Milliseconds::new(start_ms)?,
                        end: Milliseconds::new(end_ms)?,
                        text,
                    },
                )?;
                if let Some(loaded) = untitled {
                    self.loaded = Some(loaded);
                    self.generation += 1;
                }
            }
            Action::Delete { cue_id } => {
                self.loaded_mut()?
                    .apply_edit(revision, Edit::Delete { id: CueId(cue_id) })?;
            }
            Action::Split { cue_id, at_ms } => {
                self.loaded_mut()?.apply_edit(
                    revision,
                    Edit::Split {
                        id: CueId(cue_id),
                        at: Milliseconds::new(at_ms)?,
                    },
                )?;
            }
            Action::Merge {
                first_id,
                second_id,
            } => {
                self.loaded_mut()?.apply_edit(
                    revision,
                    Edit::Merge {
                        first: CueId(first_id),
                        second: CueId(second_id),
                    },
                )?;
            }
            Action::Redo => {
                self.loaded_mut()?.editor.redo(revision)?;
            }
            Action::Save { path } => {
                let loaded = self.loaded_mut()?;
                if loaded.resynced && path.is_none() {
                    return Err(
                        "Resynchronized subtitles require Save As to a different file".into(),
                    );
                }
                if loaded.recovered && path.is_none() {
                    return Err(
                        "Recovered subtitles require Save As to choose a destination".into(),
                    );
                }
                let target = match path {
                    Some(path) => save_path(Path::new(&path))?,
                    None => loaded
                        .path
                        .clone()
                        .ok_or("Choose a filename with Save As for untitled subtitles")?,
                };
                if loaded.resynced && loaded.path.as_ref() == Some(&target) {
                    return Err("Choose a different file to preserve the original subtitles".into());
                }
                let bytes = srt::serialize(loaded.editor.document(), &loaded.source)?.into_bytes();
                crate::srt_limits::validate_size(bytes.len())?;
                // Serialize before taking the lease, but compare disk bytes only
                // while holding it. Retain it through saved-state publication.
                let _lease = crate::save_lease::acquire(&target)?;
                if loaded.path.as_ref() == Some(&target)
                    && read_file(&target)? != loaded.saved_bytes
                {
                    return Err(
                        "Subtitle file changed on disk. Use Save As to preserve both versions"
                            .into(),
                    );
                }
                crate::atomic_file::write(&target, &bytes)?;
                // File replacement succeeded; only now update path and saved state.
                loaded.editor.mark_saved(revision)?;
                loaded.path = Some(target);
                loaded.saved_bytes = bytes;
                loaded.recovered = false;
                loaded.resynced = false;
            }
        }
        Ok(self.snapshot())
    }
    fn loaded_mut(&mut self) -> Result<&mut Loaded> {
        self.loaded
            .as_mut()
            .ok_or_else(|| "Open subtitles first".into())
    }
    fn check_discard(&self, discard: bool) -> Result<()> {
        if !discard
            && self
                .loaded
                .as_ref()
                .is_some_and(|d| d.editor.dirty() || d.recovered || d.resynced)
        {
            return Err("Save or explicitly discard subtitle changes first".into());
        }
        Ok(())
    }
}
pub(crate) use vsf_application::subtitle_file::read as read_file;
fn save_path(path: &Path) -> Result<PathBuf> {
    let name = path.file_name().ok_or("Choose a subtitle filename")?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let target = parent
        .canonicalize()
        .map_err(|e| format!("Cannot access save directory: {e}"))?
        .join(name);
    // Resolve an existing symlink, preserving its meaning when saving through it.
    if target.exists() {
        target.canonicalize().map_err(|e| e.to_string())
    } else {
        Ok(target)
    }
}

impl Documents {
    #[cfg(any(
        test,
        all(
            feature = "media-prototype",
            any(target_os = "macos", target_os = "windows", target_os = "linux")
        )
    ))]
    pub(crate) fn preview_since(
        &self,
        previous: Option<crate::subtitle_preview::Revision>,
    ) -> Result<Option<crate::subtitle_preview::Snapshot>> {
        use crate::subtitle_preview::{Revision, Snapshot};
        // Save/open can hold the document lock for IO. Never stall transport for it.
        let store = match self.0.try_lock() {
            Ok(store) => store,
            Err(std::sync::TryLockError::WouldBlock) => return Ok(None),
            Err(_) => return Err("Document worker state is unavailable".into()),
        };
        let revision = Revision(
            store.generation,
            store.loaded.as_ref().map_or(0, |d| d.editor.revision()),
        );
        if previous == Some(revision) {
            return Ok(None);
        }
        let content = match &store.loaded {
            Some(loaded) => crate::subtitle_preview::serialize(loaded.editor.document()),
            _ => Ok(None),
        };
        Ok(Some(Snapshot { revision, content }))
    }
    pub fn approve_exit(&self, generation: u64, revision: u64, discard: bool) -> Result<()> {
        self.apply(Action::Close { discard }, generation, revision)?;
        self.1
            .stop
            .store(true, std::sync::atomic::Ordering::Release);
        if let Some(handle) = self
            .1
            .thread
            .lock()
            .map_err(|_| "Recovery worker unavailable")?
            .take()
        {
            handle.thread().unpark();
            handle
                .join()
                .map_err(|_| "Recovery worker shutdown failed")?;
        }
        Ok(())
    }

    pub(crate) fn apply(&self, action: Action, generation: u64, revision: u64) -> Result<Snapshot> {
        let mut store = self
            .0
            .lock()
            .map_err(|_| "Document worker state is unavailable")?;
        let result = store.apply(action, generation, revision);
        store.invalidate_checks();
        result
    }
}
// Save As and no-op resync acceptance can change persistence state without an
// editor revision. Recovery publication and proposal acceptance recheck all fields.
#[derive(Clone, PartialEq, Eq)]
struct CheckpointKey {
    generation: u64,
    revision: u64,
    dirty: bool,
    path: Option<PathBuf>,
    resynced: bool,
}
impl Store {
    fn publish_checkpoint(
        &self,
        key: &CheckpointKey,
        prepared: Option<crate::atomic_file::Prepared>,
        session: &crate::recovery_store::Session,
    ) -> Result<bool> {
        if self.checkpoint_key() != *key {
            return Ok(false);
        }
        match prepared {
            Some(prepared) => prepared.commit()?,
            None => session.clear()?,
        }
        Ok(true)
    }
    fn checkpoint_key(&self) -> CheckpointKey {
        CheckpointKey {
            generation: self.generation,
            revision: self.loaded.as_ref().map_or(0, |d| d.editor.revision()),
            dirty: self
                .loaded
                .as_ref()
                .is_some_and(|d| d.editor.dirty() || d.recovered || d.resynced),
            path: self.loaded.as_ref().and_then(|d| d.path.clone()),
            resynced: self.loaded.as_ref().is_some_and(|d| d.resynced),
        }
    }
}
#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryStatus {
    entries: Vec<crate::recovery_store::Entry>,
    error: Option<String>,
}
#[tauri::command]
#[specta::specta]
pub async fn recovery_status(documents: tauri::State<'_, Documents>) -> Result<RecoveryStatus> {
    let documents = documents.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (root, error) = {
            let store = documents
                .0
                .lock()
                .map_err(|_| "Document worker unavailable")?;
            (store.recovery_root.clone(), store.recovery_error.clone())
        };
        let entries = match root {
            Some(root) => crate::recovery_store::list(&root)?,
            None => Vec::new(),
        };
        Ok(RecoveryStatus { entries, error })
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
#[specta::specta]
pub async fn recovery_discard(documents: tauri::State<'_, Documents>, id: String) -> Result<()> {
    let documents = documents.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let root = documents
            .0
            .lock()
            .map_err(|_| "Document worker unavailable")?
            .recovery_root
            .clone()
            .ok_or("Recovery storage is unavailable")?;
        crate::recovery_store::discard(&root, &id)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
#[specta::specta]
pub async fn document_action(
    documents: tauri::State<'_, Documents>,
    action: Action,
    generation: crate::ipc_number::SafeU64,
    revision: crate::ipc_number::SafeU64,
) -> Result<Snapshot> {
    let documents = documents.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        documents.apply(action, generation.get(), revision.get())
    })
    .await
    .map_err(|e| format!("Document worker failed: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    const ORIGINAL: &str = "\u{feff}007\r\n00:00:01,000 --> 00:00:02,000\r\n <i>Héllo</i> \r\n\r\n";
    fn open(store: &mut Store, path: &Path) {
        store
            .apply(
                Action::Open {
                    path: path.to_string_lossy().into_owned(),
                    discard: false,
                },
                0,
                0,
            )
            .unwrap();
    }
    fn edit(store: &mut Store) {
        store
            .apply(
                Action::Edit {
                    cue_id: 1,
                    start_ms: 1100,
                    end_ms: 2200,
                    text: "Changed\n世界".into(),
                },
                1,
                0,
            )
            .unwrap();
    }
    fn first_cue(store: &mut Store) -> Snapshot {
        store
            .apply(
                Action::Create {
                    after_id: None,
                    start_ms: 1250,
                    end_ms: 3250,
                    text: "New 世界".into(),
                },
                0,
                0,
            )
            .unwrap()
    }

    #[test]
    fn untitled_create_undo_first_save_and_external_change_protection() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::default();
        let created = first_cue(&mut store);
        assert!(created.loaded && created.dirty && created.requires_save_as && created.can_undo);
        assert_eq!((created.generation, created.revision), (1, 1));
        assert!(created.path.is_none());
        assert_eq!(created.cues[0].start_ms, 1250);
        assert!(store.apply(Action::Close { discard: false }, 1, 1).is_err());
        // A cancelled picker sends no save command. A missing destination is
        // independently rejected by Rust without changing the accepted state.
        assert!(store.apply(Action::Save { path: None }, 1, 1).is_err());
        let missing = directory.path().join("missing/output.srt");
        assert!(
            store
                .apply(
                    Action::Save {
                        path: Some(missing.to_string_lossy().into_owned())
                    },
                    1,
                    1
                )
                .is_err()
        );
        assert!(store.snapshot().dirty && store.snapshot().path.is_none());
        let undone = store.apply(Action::Undo, 1, 1).unwrap();
        assert!(undone.loaded && undone.cues.is_empty() && !undone.dirty);
        assert!(undone.requires_save_as && undone.path.is_none());
        let redone = store.apply(Action::Redo, 1, 2).unwrap();
        assert_eq!(redone.cues[0].text, "New 世界");
        let path = directory.path().join("new.srt");
        let saved = store
            .apply(
                Action::Save {
                    path: Some(path.to_string_lossy().into_owned()),
                },
                1,
                3,
            )
            .unwrap();
        assert!(!saved.dirty && !saved.requires_save_as);
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "1\n00:00:01,250 --> 00:00:03,250\nNew 世界\n\n"
        );
        store.apply(Action::Undo, 1, 3).unwrap();
        assert!(store.snapshot().dirty);
        store.apply(Action::Redo, 1, 4).unwrap();
        assert!(!store.snapshot().dirty);
        fs::write(&path, "external change").unwrap();
        assert!(store.apply(Action::Save { path: None }, 1, 5).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "external change");
    }

    #[test]
    fn failed_first_cue_is_atomic_and_old_requests_cannot_replace_it() {
        let mut store = Store::default();
        for (after_id, start_ms, end_ms) in [(None, 2000, 1000), (Some(99), 0, 2000)] {
            assert!(
                store
                    .apply(
                        Action::Create {
                            after_id,
                            start_ms,
                            end_ms,
                            text: String::new()
                        },
                        0,
                        0
                    )
                    .is_err()
            );
            assert!(!store.snapshot().loaded);
            assert_eq!(store.snapshot().generation, 0);
        }
        first_cue(&mut store);
        assert!(
            store
                .apply(
                    Action::Create {
                        after_id: None,
                        start_ms: 0,
                        end_ms: 2000,
                        text: String::new()
                    },
                    0,
                    0
                )
                .is_err()
        );
        assert_eq!(store.snapshot().cues.len(), 1);
    }

    #[test]
    fn untitled_recovery_keeps_no_path_until_a_successful_first_save() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("recovery");
        let session = crate::recovery_store::Session::create(&root).unwrap();
        let mut store = Store::default();
        first_cue(&mut store);
        let loaded = store.loaded.as_ref().unwrap();
        let record = crate::recovery_store::Record::new(
            None,
            String::new(),
            srt::serialize(loaded.editor.document(), &loaded.source).unwrap(),
            1,
            1,
        )
        .unwrap();
        session.prepare(&record).unwrap().commit().unwrap();
        drop(session);
        let entries = crate::recovery_store::list(&root).unwrap();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].source_path.is_none() && entries[0].error.is_none());
        let mut restored = Store {
            recovery_root: Some(root.clone()),
            ..Store::default()
        };
        let snap = restored
            .apply(
                Action::Recover {
                    id: entries[0].id.clone(),
                    discard: false,
                },
                0,
                0,
            )
            .unwrap();
        assert!(snap.loaded && snap.dirty && snap.requires_save_as && snap.path.is_none());
        assert_eq!(snap.cues[0].text, "New 世界");
        assert!(restored.apply(Action::Save { path: None }, 1, 0).is_err());
        let path = directory.path().join("recovered.srt");
        let saved = restored
            .apply(
                Action::Save {
                    path: Some(path.to_string_lossy().into_owned()),
                },
                1,
                0,
            )
            .unwrap();
        assert!(!saved.dirty && !saved.requires_save_as);
        assert!(fs::read_to_string(path).unwrap().contains("New 世界"));
        assert_eq!(crate::recovery_store::list(&root).unwrap().len(), 1);
    }

    #[test]
    fn stale_prepared_recovery_cannot_replace_newer_edit_or_clean_save() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("original.srt");
        fs::write(&path, ORIGINAL).unwrap();
        let session = crate::recovery_store::Session::create(dir.path()).unwrap();
        let mut store = Store::default();
        open(&mut store, &path);
        edit(&mut store);
        let key = store.checkpoint_key();
        let record = crate::recovery_store::Record::new(
            path.to_string_lossy().into_owned(),
            ORIGINAL.into(),
            ORIGINAL.replace("Héllo", "snapshot"),
            key.generation,
            key.revision,
        )
        .unwrap();
        let pending = session.prepare(&record).unwrap();
        store
            .apply(
                Action::Edit {
                    cue_id: 1,
                    start_ms: 1200,
                    end_ms: 2200,
                    text: "Newer".into(),
                },
                1,
                1,
            )
            .unwrap();
        assert!(
            !store
                .publish_checkpoint(&key, Some(pending), &session)
                .unwrap()
        );
        let newer = store.checkpoint_key();
        assert!(
            store
                .publish_checkpoint(&newer, Some(session.prepare(&record).unwrap()), &session)
                .unwrap()
        );
        let pending = session.prepare(&record).unwrap();
        store.apply(Action::Save { path: None }, 1, 2).unwrap();
        assert!(
            !store
                .publish_checkpoint(&newer, Some(pending), &session)
                .unwrap()
        );
        assert!(
            store
                .publish_checkpoint(&store.checkpoint_key(), None, &session)
                .unwrap()
        );
        drop(session);
        assert!(crate::recovery_store::list(dir.path()).unwrap().is_empty());
        assert!(fs::read_to_string(path).unwrap().contains("Newer"));
    }
    #[test]
    fn delayed_recovery_write_and_clear_cannot_cross_document_replacement() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("original.srt");
        fs::write(&path, ORIGINAL).unwrap();
        let recovery_root = dir.path().join("recovery");
        let session = crate::recovery_store::Session::create(&recovery_root).unwrap();
        let mut store = Store::default();
        open(&mut store, &path);
        edit(&mut store);

        // Capture actual accepted bytes, as the background worker does, before
        // close/reopen reuses the same cue IDs and editor revision numbers.
        let capture = |store: &Store| {
            let loaded = store.loaded.as_ref().unwrap();
            crate::recovery_store::Record::new(
                loaded
                    .path
                    .as_ref()
                    .map(|path| path.to_string_lossy().into_owned()),
                String::from_utf8(loaded.saved_bytes.clone()).unwrap(),
                srt::serialize(loaded.editor.document(), &loaded.source).unwrap(),
                store.generation,
                loaded.editor.revision(),
            )
            .unwrap()
        };
        let old_key = store.checkpoint_key();
        let old_record = capture(&store);
        let delayed_write = session.prepare(&old_record).unwrap();
        store.apply(Action::Close { discard: true }, 1, 1).unwrap();
        let closed_key = store.checkpoint_key();
        store
            .apply(
                Action::Open {
                    path: path.to_string_lossy().into_owned(),
                    discard: false,
                },
                2,
                0,
            )
            .unwrap();
        store
            .apply(
                Action::Edit {
                    cue_id: 1,
                    start_ms: 1300,
                    end_ms: 2400,
                    text: "Replacement accepted 世界".into(),
                },
                3,
                0,
            )
            .unwrap();
        let latest = capture(&store);
        assert_eq!(old_record.revision, latest.revision);
        assert_eq!(old_record.source_path, latest.source_path);
        assert!(
            store
                .publish_checkpoint(
                    &store.checkpoint_key(),
                    Some(session.prepare(&latest).unwrap()),
                    &session,
                )
                .unwrap()
        );
        assert!(
            !store
                .publish_checkpoint(&old_key, Some(delayed_write), &session)
                .unwrap()
        );
        assert!(
            !store
                .publish_checkpoint(&closed_key, None, &session)
                .unwrap()
        );
        drop(session);

        let entries = crate::recovery_store::list(&recovery_root).unwrap();
        assert_eq!(entries.len(), 1);
        let restored = crate::recovery_store::recover(&recovery_root, &entries[0].id).unwrap();
        assert_eq!(restored.accepted_srt, latest.accepted_srt);
        assert_eq!(restored.saved_srt.as_bytes(), ORIGINAL.as_bytes());
        assert_eq!(restored.generation, 3);
        assert_eq!(fs::read(&path).unwrap(), ORIGINAL.as_bytes());
    }
    #[test]
    fn recovered_copy_is_dirty_requires_save_as_and_protects_newer_source_and_edits() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("original.srt");
        fs::write(&path, ORIGINAL).unwrap();
        let root = dir.path().join("recovery");
        let session = crate::recovery_store::Session::create(&root).unwrap();
        let accepted = ORIGINAL.replace("Héllo", "Recovered 世界");
        session
            .write(Some(
                &crate::recovery_store::Record::new(
                    path.to_string_lossy().into_owned(),
                    ORIGINAL.into(),
                    accepted.clone(),
                    1,
                    7,
                )
                .unwrap(),
            ))
            .unwrap();
        drop(session);
        let id = crate::recovery_store::list(&root).unwrap()[0].id.clone();
        let mut store = Store {
            recovery_root: Some(root.clone()),
            ..Store::default()
        };
        open(&mut store, &path);
        edit(&mut store);
        assert!(
            store
                .apply(
                    Action::Recover {
                        id: id.clone(),
                        discard: false
                    },
                    1,
                    1
                )
                .is_err()
        );
        assert!(
            store
                .apply(
                    Action::Recover {
                        id: id.clone(),
                        discard: true
                    },
                    1,
                    0
                )
                .is_err()
        );
        assert_eq!(store.snapshot().cues[0].text, "Changed\n世界");
        fs::write(&path, "external newer bytes").unwrap();
        let snap = store
            .apply(Action::Recover { id, discard: true }, 1, 1)
            .unwrap();
        assert_eq!(snap.generation, 2);
        assert!(snap.dirty && snap.requires_save_as);
        assert!(!snap.can_undo);
        assert!(store.apply(Action::Close { discard: false }, 2, 0).is_err());
        assert!(store.apply(Action::Save { path: None }, 2, 0).is_err());
        assert!(
            store
                .apply(
                    Action::Save {
                        path: Some(path.to_string_lossy().into_owned())
                    },
                    2,
                    0
                )
                .is_err()
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), "external newer bytes");
        let copy = dir.path().join("copy.srt");
        let snap = store
            .apply(
                Action::Save {
                    path: Some(copy.to_string_lossy().into_owned()),
                },
                2,
                0,
            )
            .unwrap();
        assert!(!snap.dirty && !snap.requires_save_as);
        assert_eq!(fs::read_to_string(copy).unwrap(), accepted);
        assert_eq!(crate::recovery_store::list(&root).unwrap().len(), 1); // durable original is retained
    }
    #[test]
    fn empty_recovery_survives_missing_original_and_can_be_saved_elsewhere() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing-directory/source.srt");
        let root = dir.path().join("recovery");
        let session = crate::recovery_store::Session::create(&root).unwrap();
        session
            .write(Some(
                &crate::recovery_store::Record::new(
                    missing.to_string_lossy().into_owned(),
                    ORIGINAL.into(),
                    String::new(),
                    1,
                    9,
                )
                .unwrap(),
            ))
            .unwrap();
        drop(session);
        let id = crate::recovery_store::list(&root).unwrap()[0].id.clone();
        let mut store = Store {
            recovery_root: Some(root),
            ..Store::default()
        };
        let snap = store
            .apply(Action::Recover { id, discard: false }, 0, 0)
            .unwrap();
        assert!(snap.loaded && snap.dirty && snap.requires_save_as);
        assert!(snap.cues.is_empty());
        let copy = dir.path().join("empty.srt");
        store
            .apply(
                Action::Save {
                    path: Some(copy.to_string_lossy().into_owned()),
                },
                1,
                0,
            )
            .unwrap();
        assert_eq!(fs::read(copy).unwrap(), b"");
        assert!(!missing.exists());
    }
    #[test]
    fn recovery_retries_initial_storage_failure_with_latest_accepted_state() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("original.srt");
        fs::write(&path, ORIGINAL).unwrap();
        let root = dir.path().join("recovery");
        // A file obstructs directory creation on every platform, without relying
        // on user privileges or platform-specific permission behavior.
        fs::write(&root, b"obstruction").unwrap();
        let documents = Documents::default();
        {
            let mut store = documents.0.lock().unwrap();
            open(&mut store, &path);
            edit(&mut store);
        }
        documents.start_recovery(root.clone()).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while documents.0.lock().unwrap().recovery_error.is_none() {
            assert!(
                std::time::Instant::now() < deadline,
                "missing recovery error"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let latest = "Newest accepted 日本語";
        documents
            .apply(
                Action::Edit {
                    cue_id: 1,
                    start_ms: 1100,
                    end_ms: 2100,
                    text: latest.into(),
                },
                1,
                1,
            )
            .unwrap();
        assert_eq!(fs::read(&root).unwrap(), b"obstruction");
        fs::remove_file(&root).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let snapshot = loop {
            let snapshot = fs::read_dir(&root).ok().and_then(|entries| {
                entries
                    .flatten()
                    .find_map(|entry| fs::read(entry.path().join("snapshot.json")).ok())
            });
            if let Some(snapshot) = snapshot {
                break snapshot;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "recovery did not retry"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        let record: crate::recovery_store::Record = serde_json::from_slice(&snapshot).unwrap();
        assert_eq!((record.generation, record.revision), (1, 2));
        let store = documents.0.lock().unwrap();
        let loaded = store.loaded.as_ref().unwrap();
        assert_eq!(
            record.accepted_srt,
            srt::serialize(loaded.editor.document(), &loaded.source).unwrap()
        );
        assert!(record.accepted_srt.contains(latest));
        assert!(store.recovery_error.is_none());
        drop(store);
        assert_eq!(record.saved_srt, ORIGINAL);
        assert_eq!(fs::read_to_string(&path).unwrap(), ORIGINAL);
        assert!(crate::recovery_store::list(&root).unwrap().is_empty());
        documents.approve_exit(1, 2, true).unwrap();
        assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
    }
    #[test]
    fn shutdown_joins_recovery_while_initial_storage_is_unavailable() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("recovery");
        fs::write(&root, b"obstruction").unwrap();
        let documents = Documents::default();
        documents.start_recovery(root.clone()).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while documents.0.lock().unwrap().recovery_error.is_none() {
            assert!(
                std::time::Instant::now() < deadline,
                "missing recovery error"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(
            !documents
                .1
                .thread
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .is_finished(),
            "initial storage failure must not stop the recovery worker"
        );
        documents.approve_exit(0, 0, false).unwrap();
        assert!(documents.1.thread.lock().unwrap().is_none());
        assert_eq!(fs::read(&root).unwrap(), b"obstruction");
    }
    #[test]
    fn background_checkpoint_and_normal_exit_join_without_leaving_recovery() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("original.srt");
        fs::write(&path, ORIGINAL).unwrap();
        let root = dir.path().join("recovery");
        let documents = Documents::default();
        documents.start_recovery(root.clone()).unwrap();
        {
            let mut store = documents.0.lock().unwrap();
            open(&mut store, &path);
            edit(&mut store);
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let present = fs::read_dir(&root).ok().is_some_and(|entries| {
                entries
                    .flatten()
                    .any(|e| e.path().join("snapshot.json").is_file())
            });
            if present {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "background snapshot did not arrive"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(fs::read_to_string(&path).unwrap(), ORIGINAL);
        assert!(crate::recovery_store::list(&root).unwrap().is_empty()); // live owner
        documents.approve_exit(1, 1, true).unwrap();
        assert!(crate::recovery_store::list(&root).unwrap().is_empty()); // joined and cleared
        assert_eq!(fs::read_to_string(path).unwrap(), ORIGINAL);
    }
    #[test]
    fn structural_revisions_refresh_preview_and_last_delete_clears_it() {
        use crate::subtitle_preview::Revision;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("preview.srt");
        fs::write(&path, ORIGINAL).unwrap();
        let documents = Documents::default();
        documents
            .apply(
                Action::Open {
                    path: path.to_string_lossy().into_owned(),
                    discard: false,
                },
                0,
                0,
            )
            .unwrap();
        documents
            .apply(
                Action::Split {
                    cue_id: 1,
                    at_ms: 1500,
                },
                1,
                0,
            )
            .unwrap();
        let split = documents
            .preview_since(Some(Revision(1, 0)))
            .unwrap()
            .unwrap();
        assert_eq!(split.revision, Revision(1, 1));
        let text = split.content.unwrap().unwrap().url;
        assert!(text.contains("00:00:01.000 --> 00:00:01.500"));
        assert!(text.contains("00:00:01.500 --> 00:00:02.000"));
        assert_eq!(text.matches("Héllo").count(), 2);
        documents
            .apply(
                Action::Merge {
                    first_id: 1,
                    second_id: 2,
                },
                1,
                1,
            )
            .unwrap();
        let merged = documents
            .preview_since(Some(Revision(1, 1)))
            .unwrap()
            .unwrap();
        assert_eq!(merged.revision, Revision(1, 2));
        assert_eq!(
            merged
                .content
                .unwrap()
                .unwrap()
                .url
                .matches(" --> ")
                .count(),
            1
        );
        documents.apply(Action::Delete { cue_id: 1 }, 1, 2).unwrap();
        assert!(
            documents
                .preview_since(Some(Revision(1, 2)))
                .unwrap()
                .unwrap()
                .content
                .unwrap()
                .is_none()
        );
        documents.apply(Action::Undo, 1, 3).unwrap();
        let restored = documents
            .preview_since(Some(Revision(1, 3)))
            .unwrap()
            .unwrap();
        assert_eq!(restored.revision, Revision(1, 4));
        assert!(restored.content.unwrap().unwrap().url.contains("Héllo"));
    }
    #[test]
    fn structural_operations_save_reopen_and_reject_old_documents_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("operations.srt");
        fs::write(&path, ORIGINAL).unwrap();
        let mut store = Store::default();
        open(&mut store, &path);
        store
            .apply(
                Action::Split {
                    cue_id: 1,
                    at_ms: 1500,
                },
                1,
                0,
            )
            .unwrap();
        let right = store.snapshot().cues[1].id;
        store
            .apply(
                Action::Create {
                    after_id: Some(right),
                    start_ms: 3000,
                    end_ms: 4000,
                    text: "世界".into(),
                },
                1,
                1,
            )
            .unwrap();
        let new = store.snapshot().cues[2].id;
        assert!(store.apply(Action::Delete { cue_id: new }, 0, 2).is_err());
        assert!(store.apply(Action::Delete { cue_id: new }, 1, 1).is_err());
        assert!(
            store
                .apply(
                    Action::Create {
                        after_id: None,
                        start_ms: 0,
                        end_ms: 1000,
                        text: "bad\n\ntext".into()
                    },
                    1,
                    2
                )
                .is_err()
        );
        assert_eq!(store.snapshot().revision, 2);
        assert_eq!(store.snapshot().cues.len(), 3);
        store
            .apply(
                Action::Merge {
                    first_id: right,
                    second_id: new,
                },
                1,
                2,
            )
            .unwrap();
        assert_eq!(store.snapshot().cues[1].text, " <i>Héllo</i> \n世界");
        store.apply(Action::Delete { cue_id: 1 }, 1, 3).unwrap();
        assert!(
            store
                .apply(
                    Action::Save {
                        path: Some(dir.path().to_string_lossy().into_owned())
                    },
                    1,
                    4
                )
                .is_err()
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), ORIGINAL);
        assert!(store.snapshot().dirty);
        assert_eq!(store.snapshot().revision, 4);
        store.apply(Action::Save { path: None }, 1, 4).unwrap();
        let saved = fs::read_to_string(&path).unwrap();
        assert!(saved.starts_with("\u{feff}1\r\n00:00:01,500 --> 00:00:04,000"));
        for revision in 4..8 {
            store.apply(Action::Undo, 1, revision).unwrap();
        }
        assert_eq!(
            srt::serialize(
                store.loaded.as_ref().unwrap().editor.document(),
                &store.loaded.as_ref().unwrap().source
            )
            .unwrap(),
            ORIGINAL
        );
        for revision in 8..12 {
            store.apply(Action::Redo, 1, revision).unwrap();
        }
        assert!(!store.snapshot().dirty);
        store
            .apply(
                Action::Open {
                    path: path.to_string_lossy().into_owned(),
                    discard: false,
                },
                1,
                12,
            )
            .unwrap();
        assert_eq!(store.snapshot().generation, 2);
        assert_eq!(store.snapshot().cues[0].text, " <i>Héllo</i> \n世界");
        assert!(
            store
                .apply(
                    Action::Split {
                        cue_id: 1,
                        at_ms: 2000
                    },
                    1,
                    0
                )
                .is_err()
        );
        assert!(!store.snapshot().dirty);
    }
    #[test]
    fn preview_uses_accepted_revision_and_never_waits_for_file_work() {
        use crate::subtitle_preview::Revision;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.srt");
        fs::write(&path, ORIGINAL).unwrap();
        let documents = Documents::default();
        {
            let mut store = documents.0.lock().unwrap();
            open(&mut store, &path);
            assert!(documents.preview_since(None).unwrap().is_none());
        }
        let first = documents.preview_since(None).unwrap().unwrap();
        assert_eq!(first.revision, Revision(1, 0));
        assert_eq!(
            first.content.unwrap().unwrap().url,
            "memory://WEBVTT\n\n00:00:01.000 --> 00:00:02.000\n <i>Héllo</i> \n\n"
        );
        {
            let mut store = documents.0.lock().unwrap();
            edit(&mut store);
        }
        let edited = documents
            .preview_since(Some(Revision(1, 0)))
            .unwrap()
            .unwrap();
        assert!(
            edited
                .content
                .unwrap()
                .unwrap()
                .url
                .contains("Changed\n世界")
        );
        documents.apply(Action::Undo, 1, 1).unwrap();
        let undo = documents
            .preview_since(Some(Revision(1, 1)))
            .unwrap()
            .unwrap();
        assert_eq!(undo.revision, Revision(1, 2));
        assert_eq!(
            undo.content.unwrap().unwrap().url,
            "memory://WEBVTT\n\n00:00:01.000 --> 00:00:02.000\n <i>Héllo</i> \n\n"
        );
        assert!(
            documents
                .preview_since(Some(Revision(1, 2)))
                .unwrap()
                .is_none()
        );
        assert!(documents.apply(Action::Redo, 1, 0).is_err());
        assert!(
            documents
                .preview_since(Some(Revision(1, 2)))
                .unwrap()
                .is_none()
        );
        documents
            .apply(Action::Close { discard: true }, 1, 2)
            .unwrap();
        let closed = documents
            .preview_since(Some(Revision(1, 2)))
            .unwrap()
            .unwrap();
        assert_eq!(closed.revision, Revision(2, 0));
        assert!(closed.content.unwrap().is_none());
    }

    #[test]
    fn ass_preview_edit_undo_redo_and_save_preserve_original_markup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tagged.srt");
        let original = "\u{feff}7\r\n00:00:01,001 --> 00:00:02,999\r\n{\\an8\\c&H0000FF&}<i>Original</i>\\N日本語\r\n\r\n";
        fs::write(&path, original).unwrap();
        let documents = Documents::default();
        documents
            .apply(
                Action::Open {
                    path: path.to_string_lossy().into_owned(),
                    discard: false,
                },
                0,
                0,
            )
            .unwrap();
        let before = documents
            .preview_since(None)
            .unwrap()
            .unwrap()
            .content
            .unwrap();
        assert!(before.as_ref().unwrap().url.starts_with("hex://"));
        let edited = r"{\a6\t(0,800,\fs40)}Edited <b>bold</b>";
        documents
            .apply(
                Action::Edit {
                    cue_id: 1,
                    start_ms: 1001,
                    end_ms: 2999,
                    text: edited.into(),
                },
                1,
                0,
            )
            .unwrap();
        let after = documents
            .preview_since(None)
            .unwrap()
            .unwrap()
            .content
            .unwrap();
        assert_ne!(before, after);
        documents.apply(Action::Undo, 1, 1).unwrap();
        assert_eq!(
            documents
                .preview_since(None)
                .unwrap()
                .unwrap()
                .content
                .unwrap(),
            before
        );
        documents.apply(Action::Save { path: None }, 1, 2).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
        documents.apply(Action::Redo, 1, 2).unwrap();
        assert_eq!(
            documents
                .preview_since(None)
                .unwrap()
                .unwrap()
                .content
                .unwrap(),
            after
        );
        documents.apply(Action::Save { path: None }, 1, 3).unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            original.replace(r"{\an8\c&H0000FF&}<i>Original</i>\N日本語", edited)
        );
    }

    #[test]
    fn edit_save_reopen_and_undo_saved_state() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.srt");
        fs::write(&path, ORIGINAL).unwrap();
        let mut store = Store::default();
        open(&mut store, &path);
        edit(&mut store);
        assert!(store.apply(Action::Close { discard: false }, 1, 1).is_err());
        store.apply(Action::Save { path: None }, 1, 1).unwrap();
        assert!(!store.snapshot().dirty);
        let saved = fs::read_to_string(&path).unwrap();
        assert!(saved.starts_with("\u{feff}007\r\n"));
        assert!(saved.contains("Changed\r\n世界"));
        store.apply(Action::Undo, 1, 1).unwrap();
        assert!(store.snapshot().dirty);
        store.apply(Action::Redo, 1, 2).unwrap();
        assert!(!store.snapshot().dirty);
        store
            .apply(
                Action::Open {
                    path: path.to_string_lossy().into_owned(),
                    discard: false,
                },
                1,
                3,
            )
            .unwrap();
        let snap = store.snapshot();
        assert_eq!(snap.generation, 2);
        assert_eq!(snap.cues[0].text, "Changed\n世界");
        assert_eq!(snap.cues[0].start_ms, 1100);
        assert_eq!(snap.cues[0].end_ms, 2200);
        assert!(store.apply(Action::Undo, 1, 0).is_err());
    }
    #[test]
    fn failed_open_and_save_preserve_document_and_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.srt");
        fs::write(&path, ORIGINAL).unwrap();
        let mut store = Store::default();
        open(&mut store, &path);
        edit(&mut store);
        let invalid = dir.path().join("bad.srt");
        fs::write(&invalid, "not subtitles").unwrap();
        assert!(
            store
                .apply(
                    Action::Open {
                        path: invalid.to_string_lossy().into_owned(),
                        discard: true
                    },
                    1,
                    1
                )
                .is_err()
        );
        assert!(
            store
                .apply(
                    Action::Save {
                        path: Some(dir.path().to_string_lossy().into_owned())
                    },
                    1,
                    1
                )
                .is_err()
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), ORIGINAL);
        let snap = store.snapshot();
        assert!(snap.dirty);
        assert!(snap.can_undo);
        assert_eq!(snap.revision, 1);
        assert_eq!(snap.cues[0].text, "Changed\n世界");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2); // failed temp write is cleaned up
        fs::write(&path, "external change").unwrap();
        assert!(store.apply(Action::Save { path: None }, 1, 1).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "external change");
        let copy = dir.path().join("copy.srt");
        store
            .apply(
                Action::Save {
                    path: Some(copy.to_string_lossy().into_owned()),
                },
                1,
                1,
            )
            .unwrap();
        assert!(!store.snapshot().dirty);
    }
}
