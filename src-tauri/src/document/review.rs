//! Independent review slot. Workers retain immutable captures, never Store.
use super::translation::context::ReferenceFile;
use super::*;
use crate::ipc_number::SafeU64;
use std::{
    sync::atomic::{AtomicBool, Ordering},
    thread::JoinHandle,
};
use vsf_application::comparison as engine;

#[derive(Clone, Default, Debug)]
pub(crate) struct Context {
    pub(crate) reference: translation::Context,
    pub(crate) options: engine::Options,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Stored {
    reference: Option<translation::Stored>,
    options: engine::Options,
}
impl Context {
    pub(crate) fn stored(&self) -> Result<Option<Stored>> {
        if self.reference.is_empty() {
            return Ok(None);
        }
        Ok(Some(Stored {
            reference: self.reference.stored()?,
            options: self.options.clone(),
        }))
    }
}
impl Stored {
    pub(crate) fn restore(&self) -> Result<Context> {
        Ok(Context {
            reference: self
                .reference
                .as_ref()
                .map(|r| r.restore())
                .transpose()?
                .unwrap_or_default(),
            options: self.options.clone(),
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewKey {
    generation: SafeU64,
    revision: SafeU64,
    epoch: SafeU64,
}
#[derive(Clone, Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ReviewSummary {
    pub epoch: SafeU64,
    pub dirty: bool,
    pub path: Option<String>,
    pub options: engine::Options,
}
#[derive(Deserialize, specta::Type)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ReviewAction {
    Status,
    Inspect {
        original: String,
        revised: String,
    },
    Attach {
        key: ReviewKey,
        path: String,
    },
    Start {
        key: ReviewKey,
        path: String,
        discard: bool,
    },
    Detach {
        key: ReviewKey,
    },
    Options {
        key: ReviewKey,
        options: engine::Options,
    },
    Query {
        key: ReviewKey,
        offset: u32,
        limit: u32,
        filter: Option<engine::Kind>,
    },
}
#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ReviewReply {
    key: ReviewKey,
    context: Option<ReviewSummary>,
    document: Option<Snapshot>,
    working: bool,
    statistics: Option<engine::Statistics>,
    rows: Vec<engine::Change>,
    total: u32,
    error: Option<String>,
    newer_original: Option<bool>,
}
struct Job {
    key: ReviewKey,
    cancel: Arc<AtomicBool>,
    worker: Option<JoinHandle<Result<engine::Comparison>>>,
    result: Option<engine::Comparison>,
    error: Option<String>,
}
impl Job {
    fn refresh(&mut self) {
        if let Some(worker) = self.worker.take_if(|w| w.is_finished()) {
            match worker
                .join()
                .unwrap_or_else(|_| Err("Comparison worker stopped".into()))
            {
                Ok(result) if !self.cancel.load(Ordering::Acquire) => self.result = Some(result),
                Err(error) if !self.cancel.load(Ordering::Acquire) => self.error = Some(error),
                _ => {}
            }
        }
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
#[derive(Default)]
pub(super) struct State {
    pub(super) epoch: u64,
    job: Option<Job>,
    closed: bool,
}
impl Store {
    fn review_key(&self) -> Result<ReviewKey> {
        Ok(ReviewKey {
            generation: self.generation.try_into()?,
            revision: self
                .loaded
                .as_ref()
                .map_or(0, |l| l.editor.revision())
                .try_into()?,
            epoch: self.review.epoch.try_into()?,
        })
    }
    fn check_review_key(&self, key: ReviewKey) -> Result<()> {
        if self.review.closed || key != self.review_key()? {
            Err("Review changed; refresh before continuing".into())
        } else {
            Ok(())
        }
    }
    pub(super) fn review_summary(&self) -> Option<ReviewSummary> {
        let context = &self.loaded.as_ref()?.review;
        if context.reference.is_empty() && self.review.epoch == 0 {
            return None;
        }
        Some(ReviewSummary {
            epoch: self.review.epoch.try_into().ok()?,
            dirty: context.reference.dirty,
            path: context
                .reference
                .reference
                .as_ref()
                .map(|r| r.origin.path.to_string_lossy().into_owned()),
            options: context.options.clone(),
        })
    }
    fn next_review_epoch(&mut self) -> Result<()> {
        self.review.epoch = self
            .review
            .epoch
            .checked_add(1)
            .filter(|v| *v <= crate::ipc_number::MAX_SAFE_INTEGER)
            .ok_or("Review epoch exhausted")?;
        if let Some(job) = &mut self.review.job {
            job.cancel.store(true, Ordering::Release);
            job.result = None;
        }
        Ok(())
    }
    fn review_reply(
        &mut self,
        document: Option<Snapshot>,
        offset: u32,
        limit: u32,
        filter: Option<engine::Kind>,
    ) -> Result<ReviewReply> {
        let key = self.review_key()?;
        let context = self.review_summary();
        let mut reply = ReviewReply {
            key,
            context,
            document,
            working: false,
            statistics: None,
            rows: Vec::new(),
            total: 0,
            error: None,
            newer_original: None,
        };
        if let Some(job) = &mut self.review.job {
            if job.key != key {
                job.cancel.store(true, Ordering::Release);
                job.result = None;
            }
            job.refresh();
            reply.working = job.worker.is_some();
            if job.key == key {
                reply.error = job.error.clone();
                if let Some(result) = &job.result {
                    reply.statistics = Some(result.statistics.clone());
                    let matches = |c: &&engine::Change| {
                        c.kind != engine::Kind::Unchanged && filter.is_none_or(|f| c.kind == f)
                    };
                    reply.total = result.changes.iter().filter(matches).count() as u32;
                    reply.rows = result
                        .changes
                        .iter()
                        .filter(matches)
                        .skip(offset as usize)
                        .take(limit as usize)
                        .cloned()
                        .collect();
                    for cue in reply
                        .rows
                        .iter()
                        .flat_map(|c| c.original.iter().chain(&c.revised))
                    {
                        SafeU64::try_from(cue.cue_id)?;
                        SafeU64::try_from(cue.start_ms)?;
                        SafeU64::try_from(cue.end_ms)?;
                    }
                }
            }
        }
        Ok(reply)
    }
}
impl Documents {
    pub(crate) fn review_action(&self, action: ReviewAction) -> Result<ReviewReply> {
        if let ReviewAction::Inspect { original, revised } = &action {
            let original = ReferenceFile::load(Path::new(original), || Ok(()))?;
            let revised = ReferenceFile::load(Path::new(revised), || Ok(()))?;
            let newer = std::fs::metadata(&original.origin.path)
                .and_then(|m| m.modified())
                .ok()
                .zip(
                    std::fs::metadata(&revised.origin.path)
                        .and_then(|m| m.modified())
                        .ok(),
                )
                .is_some_and(|(a, b)| a > b);
            let mut store = self.0.lock().map_err(|_| "Document unavailable")?;
            let mut reply = store.review_reply(None, 0, 100, None)?;
            reply.newer_original = Some(newer);
            return Ok(reply);
        }
        if let ReviewAction::Start { key, path, discard } = action {
            let (checkpoint, source, saved, protected) = {
                let store = self.0.lock().map_err(|_| "Document unavailable")?;
                store.check_review_key(key)?;
                let loaded = store.loaded.as_ref().ok_or("Open subtitles first")?;
                if loaded.subtitle_dirty() && !discard {
                    return Err("Save or discard subtitle changes before starting a review".into());
                }
                let source = loaded
                    .path
                    .clone()
                    .ok_or("Save the subtitles before starting a review")?;
                let protected: Vec<_> = loaded
                    .translation
                    .protected_paths()
                    .chain(loaded.review.reference.protected_paths())
                    .chain(loaded.extracted_from.iter().map(|p| p.media_path.as_path()))
                    .map(Path::to_owned)
                    .collect();
                (
                    store.checkpoint_key(),
                    source,
                    loaded.saved_bytes.clone(),
                    protected,
                )
            };
            let original = ReferenceFile::load(&source, || Ok(()))?;
            if original.reference.original_bytes() != saved {
                return Err("Original changed on disk; reopen it first".into());
            }
            let output = vsf_application::new_output::resolve(Path::new(&path))
                .map_err(|e| e.to_string())?;
            for p in protected {
                vsf_application::new_output::protect_source(&output, &p)
                    .map_err(|e| e.to_string())?;
            }
            original.origin.protect(&output)?;
            let prepared = vsf_application::atomic_file::prepare(&output, &saved)?;
            let parsed = original.reference.copy_target();
            let mut store = self.0.lock().map_err(|_| "Document unavailable")?;
            store.check_review_key(key)?;
            if store.checkpoint_key() != checkpoint || store.loaded_mut()?.saved_bytes != saved {
                return Err("Document changed while preparing reviewed copy".into());
            }
            prepared.commit_new()?;
            let loaded = store.loaded_mut()?;
            // Preserve extraction provenance so Save As continues protecting the video.
            loaded.editor = Editor::with_properties(parsed.document, parsed.source);
            loaded.recovered = false;
            loaded.resynced = false;
            // Copy preserves any already-attached base and translation slot.
            if loaded.review.reference.reference.is_none() {
                loaded.review.options = engine::Options::for_format(original.reference.format());
                loaded.review.reference.reference = Some(original.clone());
            }
            loaded.review.reference.copied_from = Some(original.origin);
            loaded.review.reference.dirty = true;
            loaded.path = Some(output);
            store.generation += 1;
            store.next_review_epoch()?;
            let document = Some(store.snapshot());
            return store.review_reply(document, 0, 100, None);
        }
        // File preparation runs outside Store. Adoption rechecks all identities.
        if let ReviewAction::Attach { key, path } = action {
            {
                self.0
                    .lock()
                    .map_err(|_| "Document unavailable")?
                    .check_review_key(key)?;
            }
            let original = ReferenceFile::load(Path::new(&path), || Ok(()))?;
            let mut store = self.0.lock().map_err(|_| "Document unavailable")?;
            store.check_review_key(key)?;
            let loaded = store.loaded_mut()?;
            if loaded.path.as_ref() == Some(&original.origin.path) {
                return Err("Choose a different original file".into());
            }
            original.verify(|| Ok(()))?;
            let options = engine::Options::for_format(original.reference.format());
            loaded.review = Context {
                reference: translation::Context {
                    reference: Some(original),
                    copied_from: loaded.review.reference.copied_from.clone(),
                    dirty: true,
                },
                options,
            };
            store.next_review_epoch()?;
            let document = Some(store.snapshot());
            return store.review_reply(document, 0, 100, None);
        }
        let mut store = self.0.lock().map_err(|_| "Document unavailable")?;
        let mut document = None;
        let (mut offset, mut limit, mut filter) = (0, 100, None);
        match action {
            ReviewAction::Attach { .. } | ReviewAction::Inspect { .. } => unreachable!(),
            ReviewAction::Status => {}
            ReviewAction::Start { .. } => unreachable!(),
            ReviewAction::Detach { key } => {
                store.check_review_key(key)?;
                let c = &mut store.loaded_mut()?.review;
                c.reference.reference = None;
                c.reference.dirty = true;
                store.next_review_epoch()?;
                document = Some(store.snapshot());
            }
            ReviewAction::Options { key, options } => {
                store.check_review_key(key)?;
                let c = &mut store.loaded_mut()?.review;
                c.options = options;
                c.reference.dirty = true;
                store.next_review_epoch()?;
                document = Some(store.snapshot());
            }
            ReviewAction::Query {
                key,
                offset: o,
                limit: l,
                filter: f,
            } => {
                store.check_review_key(key)?;
                if !(1..=200).contains(&l) {
                    return Err("Comparison pages require 1–200 rows".into());
                }
                (offset, limit, filter) = (o, l, f);
                if let Some(job) = &mut store.review.job {
                    job.refresh();
                    if job.key != key {
                        job.cancel.store(true, Ordering::Release);
                    }
                }
                if store
                    .review
                    .job
                    .as_ref()
                    .is_none_or(|j| j.key != key && j.worker.is_none())
                {
                    let loaded = store.loaded.as_ref().ok_or("Open subtitles first")?;
                    let original = loaded
                        .review
                        .reference
                        .reference
                        .as_ref()
                        .ok_or("Attach an original first")?
                        .reference
                        .clone();
                    let cues = loaded.editor.document().cues().to_vec();
                    let format = loaded.source().format();
                    let options = loaded.review.options.clone();
                    let cancel = Arc::new(AtomicBool::new(false));
                    let token = cancel.clone();
                    let worker = std::thread::Builder::new()
                        .name("subtitle-comparison".into())
                        .spawn(move || {
                            let deadline =
                                std::time::Instant::now() + std::time::Duration::from_secs(10);
                            engine::compare(
                                original.cues(),
                                original.format(),
                                &cues,
                                format,
                                options,
                                || {
                                    if token.load(Ordering::Acquire) {
                                        Err("Comparison cancelled".into())
                                    } else if std::time::Instant::now() > deadline {
                                        Err("Comparison timed out".into())
                                    } else {
                                        Ok(())
                                    }
                                },
                            )
                        })
                        .map_err(|e| e.to_string())?;
                    store.review.job = Some(Job {
                        key,
                        cancel,
                        worker: Some(worker),
                        result: None,
                        error: None,
                    });
                }
            }
        }
        store.review_reply(document, offset, limit, filter)
    }
    pub(crate) fn shutdown_review(&self) -> Result<()> {
        let job = {
            let mut store = self.0.lock().map_err(|_| "Document unavailable")?;
            store.review.closed = true;
            store.review.job.take()
        };
        drop(job);
        Ok(())
    }
}
#[tauri::command]
#[specta::specta]
pub async fn review_action(
    documents: tauri::State<'_, Documents>,
    action: ReviewAction,
) -> Result<ReviewReply> {
    let documents = documents.inner().clone();
    tauri::async_runtime::spawn_blocking(move || documents.review_action(action))
        .await
        .map_err(|e| e.to_string())?
}

#[derive(Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewProjectContext {
    reference: translation::ProjectContext,
    options: engine::Options,
}
impl Context {
    pub(crate) fn project_description(&self, base: &Path) -> Result<Option<ReviewProjectContext>> {
        Ok(self
            .reference
            .project_description(base)?
            .map(|reference| ReviewProjectContext {
                reference,
                options: self.options.clone(),
            }))
    }
}
impl ReviewProjectContext {
    pub(crate) fn validate(&self) -> Result<()> {
        self.reference.validate()
    }
    pub(crate) fn protect(&self, base: &Path, path: &Path) -> Result<()> {
        self.reference.protect(base, path)
    }
    pub(crate) fn load(
        &self,
        base: &Path,
        overrides: &translation::ReferenceOverride,
    ) -> std::result::Result<Context, translation::ReferenceIssue> {
        Ok(Context {
            reference: self.reference.load(base, overrides)?,
            options: self.options.clone(),
        })
    }
}

#[derive(Clone, Default, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectOverride {
    pub path: Option<String>,
    #[serde(default)]
    pub end: bool,
}

#[cfg(test)]
mod tests;
