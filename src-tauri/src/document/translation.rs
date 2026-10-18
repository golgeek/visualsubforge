//! One reference worker, no queue. Workers own immutable captures and never lock
//! Store; only guarded adoption changes the loaded target's translation context.
use super::*;
use crate::ipc_number::{SafeI64, SafeU64};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    thread::JoinHandle,
    time::{Duration, Instant},
};
use vsf_application::translation as engine;

mod context;
use context::ReferenceFile;
pub(crate) use context::{Context, Stored};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Key {
    generation: SafeU64,
    revision: SafeU64,
    epoch: SafeU64,
}
#[derive(Clone, Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceInfo {
    path: String,
    format: String,
    sha256: String,
    cues: u32,
    offset_ms: SafeI64,
}
#[derive(Clone, Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    epoch: SafeU64,
    dirty: bool,
    reference: Option<ReferenceInfo>,
}
#[derive(Deserialize, specta::Type)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Action {
    Status,
    Load {
        key: Key,
        path: String,
        copy: bool,
    },
    Query {
        key: Key,
        target_cue_id: Option<SafeU64>,
        offset: u32,
        limit: u32,
        selected: Option<u32>,
    },
    View {
        id: SafeU64,
    },
    Cancel {
        id: SafeU64,
    },
    Accept {
        id: SafeU64,
        discard: bool,
    },
    SetOffset {
        key: Key,
        offset_ms: SafeI64,
    },
    Detach {
        key: Key,
    },
}
#[derive(Clone, Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    index: u32,
    start_ms: SafeI64,
    end_ms: SafeI64,
    overlap_ms: Option<SafeU64>,
    raw: String,
    reading: Option<String>,
    warning: Option<String>,
    timed: bool,
    layout: bool,
    truncated: bool,
}
#[derive(Clone, Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    total: u32,
    offset: u32,
    rows: Vec<Row>,
    selected: Option<Row>,
}
#[derive(Clone, Copy, Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
enum Phase {
    Working,
    Ready,
    Accepting,
    Accepted,
    Cancelling,
    Cancelled,
    Stale,
    Failed,
    Closed,
}
#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct JobView {
    id: SafeU64,
    key: Key,
    copy: bool,
    phase: Phase,
    reference: Option<ReferenceInfo>,
    page: Option<Page>,
    error: Option<String>,
}
#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Reply {
    key: Key,
    context: Option<Summary>,
    job: Option<JobView>,
    document: Option<Snapshot>,
}

enum Prepared {
    Reference(ReferenceFile),
    Page(Page),
}
struct Job {
    id: u64,
    key: Key,
    copy: bool,
    cancel: Arc<AtomicBool>,
    worker: Option<JoinHandle<Result<Prepared>>>,
    result: Option<Prepared>,
    error: Option<String>,
    stale: bool,
    accepting: bool,
    // A one-shot lifetime channel lets shutdown await off-lock acceptance IO as
    // well as the owned worker. Dropping its sender signals every return path.
    acceptance_done: Option<std::sync::mpsc::Receiver<()>>,
    accepted: bool,
}
impl Job {
    fn cancel(&mut self) {
        if !self.accepted {
            self.cancel.store(true, Ordering::Release);
            self.result = None;
        }
    }
    fn refresh(&mut self) {
        if let Some(worker) = self.worker.take_if(|w| w.is_finished()) {
            let result = worker
                .join()
                .unwrap_or_else(|_| Err("Reference worker stopped unexpectedly".into()));
            if !self.cancel.load(Ordering::Acquire) {
                match result {
                    Ok(value) => self.result = Some(value),
                    Err(error) => self.error = Some(error),
                }
            }
        }
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
        // Fallback only; normal shutdown takes and joins the handle off-lock.
        // This worker never owns/upgrades Store and therefore cannot join itself.
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
#[derive(Default)]
pub(super) struct State {
    pub(super) epoch: u64,
    next_id: u64,
    closed: bool,
    job: Option<Job>,
}
impl State {
    fn next_epoch(&self) -> Result<u64> {
        next(self.epoch, "Reference context")
    }
    pub(super) fn invalidate(&mut self, key: Key) {
        if let Some(job) = &mut self.job
            && job.key != key
            && !job.accepted
        {
            job.stale = true;
            job.cancel();
        }
    }
    fn launch(
        &mut self,
        key: Key,
        copy: bool,
        run: impl FnOnce(Arc<AtomicBool>) -> Result<Prepared> + Send + 'static,
    ) -> Result<()> {
        if let Some(job) = &mut self.job {
            job.refresh();
        }
        if self.closed {
            return Err("Reference worker is shutting down".into());
        }
        if self
            .job
            .as_ref()
            .is_some_and(|j| j.worker.is_some() || j.accepting)
        {
            return Err("Wait for the current reference operation to finish or cancel".into());
        }
        let id = next(self.next_id, "Reference operation")?;
        let cancel = Arc::new(AtomicBool::new(false));
        let token = cancel.clone();
        let worker = std::thread::Builder::new()
            .name("translation-reference".into())
            .spawn(move || run(token))
            .map_err(|e| format!("Cannot start reference worker: {e}"))?;
        self.job = Some(Job {
            id,
            key,
            copy,
            cancel,
            worker: Some(worker),
            result: None,
            error: None,
            stale: false,
            accepting: false,
            acceptance_done: None,
            accepted: false,
        });
        self.next_id = id;
        Ok(())
    }
}
fn next(value: u64, name: &str) -> Result<u64> {
    value
        .checked_add(1)
        .filter(|&n| n <= crate::ipc_number::MAX_SAFE_INTEGER)
        .ok_or_else(|| format!("{name} identities exhausted"))
}
fn checkpoint(cancel: &AtomicBool, start: Instant) -> Result<()> {
    if cancel.load(Ordering::Acquire) {
        return Err("Reference operation cancelled".into());
    }
    if start.elapsed() > Duration::from_secs(10) {
        return Err("Reference operation exceeded ten seconds".into());
    }
    Ok(())
}
impl Store {
    pub(super) fn translation_key(&self) -> Result<Key> {
        Ok(Key {
            generation: self.generation.try_into()?,
            revision: self
                .loaded
                .as_ref()
                .map_or(0, |d| d.editor.revision())
                .try_into()?,
            epoch: self.translation.epoch.try_into()?,
        })
    }
    fn check_translation_key(&self, key: Key) -> Result<()> {
        if self.translation.closed {
            return Err("Reference worker is shutting down".into());
        }
        if self.translation_key()? != key {
            return Err("Target or reference changed; refresh and try again".into());
        }
        Ok(())
    }
    pub(super) fn translation_summary(&self) -> Option<Summary> {
        let context = &self.loaded.as_ref()?.translation;
        if context.is_empty() {
            return None;
        }
        Some(Summary {
            epoch: self.translation.epoch.try_into().ok()?,
            dirty: context.dirty,
            reference: context.reference.as_ref().and_then(|r| r.metadata().ok()),
        })
    }
    fn translation_reply(&mut self, document: Option<Snapshot>) -> Result<Reply> {
        let key = self.translation_key()?;
        self.translation.invalidate(key);
        let closed = self.translation.closed;
        let job = self
            .translation
            .job
            .as_mut()
            .map(|job| -> Result<JobView> {
                job.refresh();
                let phase = if job.accepted {
                    Phase::Accepted
                } else if closed {
                    Phase::Closed
                } else if job.stale {
                    Phase::Stale
                } else if job.cancel.load(Ordering::Acquire) {
                    if job.worker.is_some() || job.accepting {
                        Phase::Cancelling
                    } else {
                        Phase::Cancelled
                    }
                } else if job.accepting {
                    Phase::Accepting
                } else if job.worker.is_some() {
                    Phase::Working
                } else if job.error.is_some() {
                    Phase::Failed
                } else {
                    Phase::Ready
                };
                Ok(JobView {
                    id: job.id.try_into()?,
                    key: job.key,
                    copy: job.copy,
                    phase,
                    reference: match &job.result {
                        Some(Prepared::Reference(r)) => Some(r.metadata()?),
                        _ => None,
                    },
                    page: match &job.result {
                        Some(Prepared::Page(p)) => Some(p.clone()),
                        _ => None,
                    },
                    error: job.error.clone(),
                })
            })
            .transpose()?;
        Ok(Reply {
            key,
            context: self.translation_summary(),
            job,
            document,
        })
    }
}
impl Documents {
    pub(crate) fn translation_action(&self, action: Action) -> Result<Reply> {
        if let Action::Accept { id, discard } = action {
            return self.accept_reference(id.get(), discard);
        }
        let mut store = self.0.lock().map_err(|_| "Document state unavailable")?;
        match action {
            Action::Status => {}
            Action::View { id } => {
                if store
                    .translation
                    .job
                    .as_ref()
                    .is_none_or(|j| j.id != id.get())
                {
                    return Err("Reference operation was replaced".into());
                }
            }
            Action::Cancel { id } => {
                if let Some(job) = store.translation.job.as_mut().filter(|j| j.id == id.get()) {
                    job.cancel();
                }
            }
            Action::Load { key, path, copy } => {
                store.check_translation_key(key)?;
                if !copy && store.loaded.is_none() {
                    return Err("Open target subtitles first, or start a translation copy".into());
                }
                store.translation.launch(key, copy, move |cancel| {
                    let start = Instant::now();
                    ReferenceFile::load(Path::new(&path), || checkpoint(&cancel, start))
                        .map(Prepared::Reference)
                })?;
            }
            Action::Query {
                key,
                target_cue_id,
                offset,
                limit,
                selected,
            } => {
                store.check_translation_key(key)?;
                if !(1..=engine::MAX_PAGE_ROWS as u32).contains(&limit) {
                    return Err("Reference pages require 1–200 rows".into());
                }
                let loaded = store.loaded.as_ref().ok_or("Open target subtitles first")?;
                let reference = loaded
                    .translation
                    .reference
                    .clone()
                    .ok_or("Attach a reference first")?;
                let interval = target_cue_id
                    .map(|id| {
                        loaded
                            .editor
                            .document()
                            .cues()
                            .iter()
                            .find(|c| c.id().0 == id.get())
                            .map(|c| (c.start(), c.end()))
                            .ok_or_else(|| "Target cue no longer exists".to_owned())
                    })
                    .transpose()?;
                store.translation.launch(key, false, move |cancel| {
                    let start = Instant::now();
                    query(&reference, interval, offset, limit, selected, || {
                        checkpoint(&cancel, start)
                    })
                    .map(Prepared::Page)
                })?;
            }
            Action::SetOffset { key, offset_ms } => {
                store.check_translation_key(key)?;
                let offset = engine::Offset::new(offset_ms.get())?;
                let epoch = store.translation.next_epoch()?;
                let context = &mut store.loaded_mut()?.translation;
                let reference = context
                    .reference
                    .as_mut()
                    .ok_or("Attach a reference first")?;
                if reference.offset != offset {
                    reference.offset = offset;
                    context.dirty = true;
                    store.translation.epoch = epoch;
                }
            }
            Action::Detach { key } => {
                store.check_translation_key(key)?;
                let epoch = store.translation.next_epoch()?;
                let context = &mut store.loaded_mut()?.translation;
                if context.reference.take().is_some() {
                    context.dirty = true;
                    store.translation.epoch = epoch;
                }
            }
            Action::Accept { .. } => unreachable!(),
        }
        store.translation_reply(None)
    }
    fn accept_reference(&self, id: u64, discard: bool) -> Result<Reply> {
        self.accept_reference_with(id, discard, || Ok(()))
    }
    // A test boundary models changes during off-lock IO; final production guards
    // are identical and there is no IPC hook or alternate acceptance path.
    fn accept_reference_with(
        &self,
        id: u64,
        discard: bool,
        prepared_hook: impl FnOnce() -> Result<()>,
    ) -> Result<Reply> {
        let (key, reference, copy, cancel, _completion) = {
            let mut store = self.0.lock().map_err(|_| "Document state unavailable")?;
            let key = store.translation_key()?;
            store.translation.invalidate(key);
            if store.translation.closed {
                return Err("Reference worker is shutting down".into());
            }
            let job = store
                .translation
                .job
                .as_mut()
                .filter(|j| j.id == id)
                .ok_or("Reference operation was replaced")?;
            job.refresh();
            if job.worker.is_some()
                || job.accepting
                || job.accepted
                || job.stale
                || job.cancel.load(Ordering::Acquire)
            {
                return Err("Wait for a current complete reference load".into());
            }
            let Some(Prepared::Reference(reference)) = &job.result else {
                return Err("This operation has no reference to attach".into());
            };
            let (completion, done) = std::sync::mpsc::channel::<()>();
            let capture = (
                key,
                reference.clone(),
                job.copy,
                job.cancel.clone(),
                completion,
            );
            job.accepting = true;
            job.acceptance_done = Some(done);
            capture
        };
        // No document lock for IO, parsing or copying the independently editable target.
        let start = Instant::now();
        let prepared = (|| {
            reference.verify(|| checkpoint(&cancel, start))?;
            let target = copy.then(|| reference.reference.copy_target());
            prepared_hook()?;
            checkpoint(&cancel, start)?;
            Ok::<_, String>(target)
        })();
        let mut store = self.0.lock().map_err(|_| "Document state unavailable")?;
        if let Some(job) = store.translation.job.as_mut().filter(|j| j.id == id) {
            job.accepting = false;
        }
        let target = prepared?;
        store.check_translation_key(key)?;
        checkpoint(&cancel, start)?;
        let job = store
            .translation
            .job
            .as_ref()
            .filter(|j| j.id == id && !j.accepted && !j.stale)
            .ok_or("Reference operation was replaced")?;
        if job.cancel.load(Ordering::Acquire) {
            return Err("Reference operation cancelled".into());
        }
        let epoch = store.translation.next_epoch()?;
        if let Some(target) = target {
            store.check_discard(discard)?;
            let generation = next(store.generation, "Subtitle document")?;
            let origin = reference.origin.clone();
            let saved_bytes = reference.reference.original_bytes().to_vec();
            store.loaded = Some(Loaded {
                editor: Editor::with_properties(target.document, target.source),
                path: None,
                saved_bytes,
                recovered: false,
                resynced: false,
                translation: Context {
                    reference: Some(reference),
                    copied_from: Some(origin),
                    dirty: true,
                },
            });
            store.generation = generation;
        } else {
            let loaded = store.loaded_mut()?;
            if let Some(path) = &loaded.path {
                reference.origin.protect(path)?;
            }
            loaded.translation.reference = Some(reference);
            loaded.translation.dirty = true;
        }
        store.translation.epoch = epoch;
        if let Some(job) = store.translation.job.as_mut() {
            job.accepted = true;
            job.result = None;
        }
        store.invalidate_checks();
        let snapshot = copy.then(|| store.snapshot());
        store.translation_reply(snapshot)
    }
    pub(crate) fn shutdown_translation(&self) -> Result<()> {
        let (worker, acceptance_done) = {
            let mut store = self.0.lock().map_err(|_| "Document state unavailable")?;
            store.translation.closed = true;
            store.translation.job.as_mut().map_or((None, None), |job| {
                job.cancel();
                (job.worker.take(), job.acceptance_done.take())
            })
        };
        if let Some(worker) = worker {
            let _ = worker
                .join()
                .map_err(|_| "Reference worker shutdown failed")?;
        }
        if let Some(done) = acceptance_done {
            // The channel intentionally has no message. Sender destruction means
            // acceptance has returned, including after cancellation or failure.
            let _ = done.recv();
        }
        Ok(())
    }
}
fn truncated(text: &str, limit: usize) -> (String, bool) {
    let mut end = text.len().min(limit);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (text[..end].into(), end != text.len())
}
fn row(file: &ReferenceFile, index: usize, overlap: Option<u64>, limit: usize) -> Result<Row> {
    let cue = file
        .reference
        .cues()
        .get(index)
        .ok_or("Reference cue no longer exists")?;
    let (raw, raw_cut) = truncated(cue.text(), limit);
    let (reading, warning, timed, layout, reading_cut) = match file.reference.reading(index) {
        Ok(reading) => {
            let (text, cut) = truncated(&reading.text, limit);
            (
                Some(text),
                reading.warning.map(str::to_owned),
                reading.timed,
                reading.layout,
                cut,
            )
        }
        Err(error) => (None, Some(error), false, false, false),
    };
    Ok(Row {
        index: index
            .try_into()
            .map_err(|_| "Reference index exceeds range")?,
        start_ms: file.offset.map(cue.start()).try_into()?,
        end_ms: file.offset.map(cue.end()).try_into()?,
        overlap_ms: overlap.map(SafeU64::try_from).transpose()?,
        raw,
        reading,
        warning,
        timed,
        layout,
        truncated: raw_cut || reading_cut,
    })
}
fn query(
    file: &ReferenceFile,
    interval: Option<(Milliseconds, Milliseconds)>,
    offset: u32,
    limit: u32,
    selected: Option<u32>,
    mut checkpoint: impl FnMut() -> Result<()>,
) -> Result<Page> {
    let request = engine::PageRequest {
        offset: offset as usize,
        limit: limit as usize,
    };
    let page = match interval {
        Some((start, end)) => {
            file.reference
                .matches(start, end, file.offset, request, &mut checkpoint)?
        }
        None => file
            .reference
            .browse(file.offset, request, &mut checkpoint)?,
    };
    let mut rows = Vec::with_capacity(page.rows.len());
    for item in page.rows {
        checkpoint()?;
        rows.push(row(file, item.source_index, item.overlap_ms, 1024)?);
    }
    let selected = selected
        .map(|index| row(file, index as usize, None, 65_536))
        .transpose()?;
    checkpoint()?;
    Ok(Page {
        total: page
            .total
            .try_into()
            .map_err(|_| "Reference count exceeds range")?,
        offset,
        rows,
        selected,
    })
}
#[tauri::command]
#[specta::specta]
pub async fn translation_action(
    documents: tauri::State<'_, Documents>,
    action: Action,
) -> Result<Reply> {
    let documents = documents.inner().clone();
    tauri::async_runtime::spawn_blocking(move || documents.translation_action(action))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) fn ipc_samples() -> Vec<(&'static str, &'static str, serde_json::Value)> {
    let key = Key {
        generation: 1.try_into().unwrap(),
        revision: 2.try_into().unwrap(),
        epoch: 3.try_into().unwrap(),
    };
    let reference = ReferenceInfo {
        path: "/example/reference.srt".into(),
        format: "srt".into(),
        sha256: "a".repeat(64),
        cues: 1,
        offset_ms: (-1500).try_into().unwrap(),
    };
    let row = Row {
        index: 0,
        start_ms: (-500).try_into().unwrap(),
        end_ms: 500.try_into().unwrap(),
        overlap_ms: Some(500.try_into().unwrap()),
        raw: "{\\an8}<i>Été 世界</i>".into(),
        reading: Some("Été 世界".into()),
        warning: None,
        timed: false,
        layout: true,
        truncated: false,
    };
    let reply = Reply {
        key,
        context: Some(Summary {
            epoch: key.epoch,
            dirty: true,
            reference: Some(reference),
        }),
        job: Some(JobView {
            id: 7.try_into().unwrap(),
            key,
            copy: false,
            phase: Phase::Ready,
            reference: None,
            page: Some(Page {
                total: 1,
                offset: 0,
                rows: vec![row.clone()],
                selected: Some(row),
            }),
            error: None,
        }),
        document: None,
    };
    let actions = serde_json::json!([
        {"kind":"status"}, {"kind":"load","key":key,"path":"/example/reference.srt","copy":true},
        {"kind":"query","key":key,"targetCueId":1,"offset":0,"limit":200,"selected":0},
        {"kind":"view","id":7}, {"kind":"cancel","id":7}, {"kind":"accept","id":7,"discard":false},
        {"kind":"setOffset","key":key,"offsetMs":-1500}, {"kind":"detach","key":key}
    ]);
    for action in actions.as_array().unwrap() {
        serde_json::from_value::<Action>(action.clone()).unwrap();
    }
    vec![
        (
            "translationReply",
            "TranslationReply",
            serde_json::to_value(reply).unwrap(),
        ),
        ("translationActions", "TranslationAction[]", actions),
    ]
}
