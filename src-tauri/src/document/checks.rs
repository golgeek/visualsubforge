//! The document mutex protects admission, capture and publication. The one CPU
//! worker owns an immutable domain clone, never the document owner or its lock.
//! There is one latest draft and no queued document snapshots. Joining running
//! work happens only during explicit shutdown, after releasing the document lock.
use super::*;
use crate::quality_check::{Draft, DraftChange, Key, Metric, Phase, Request, View};
use std::{
    collections::HashSet,
    sync::atomic::{AtomicBool, Ordering},
    thread::JoinHandle,
    time::{Duration, Instant},
};
use vsf_application::quality::{self as engine, Report};

#[derive(Default)]
pub(super) struct Checks {
    key: Option<Key>,
    id: u64,
    id_key: Option<Key>,
    worker: Option<Worker>,
    report: Option<Report>,
    error: Option<String>,
    draft: Option<Draft>,
    draft_metric: Option<Metric>,
    draft_error: Option<String>,
    cancelled: bool,
    closed: bool,
}
struct Worker {
    key: Key,
    draft: Option<Draft>,
    cancel: Arc<AtomicBool>,
    handle: Option<JoinHandle<Result<Output>>>,
}
enum Output {
    Accepted(Report),
    Draft(Metric),
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        // Normally refresh/shutdown already reaped it. Final owner destruction
        // must also reap a worker (tests and abnormal application teardown).
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}
impl Checks {
    fn cancel_worker(&self) {
        if let Some(worker) = &self.worker {
            worker.cancel.store(true, Ordering::Relaxed);
        }
    }
    fn synchronize(&mut self, key: Key) {
        if self.key != Some(key) {
            self.cancel_worker();
            self.key = Some(key);
            self.report = None;
            self.error = None;
            self.draft = None;
            self.draft_metric = None;
            self.draft_error = None;
            self.cancelled = false;
        }
    }
    fn refresh(&mut self) {
        let Some(mut worker) = self
            .worker
            .take_if(|w| w.handle.as_ref().is_some_and(|h| h.is_finished()))
        else {
            return;
        };
        let result = worker
            .handle
            .take()
            .expect("finished handle exists")
            .join()
            .unwrap_or_else(|_| Err("Quality worker stopped unexpectedly".into()));
        if worker.cancel.load(Ordering::Relaxed) || Some(worker.key) != self.key {
            return;
        }
        match result {
            Ok(Output::Accepted(report)) => self.report = Some(report),
            Ok(Output::Draft(metric)) if worker.draft == self.draft => {
                self.draft_metric = Some(metric)
            }
            Err(error) if worker.draft.is_none() => self.error = Some(error),
            Err(error) if worker.draft == self.draft => self.draft_error = Some(error),
            _ => {}
        }
    }
    fn launch(
        &mut self,
        key: Key,
        document: subtitle_core::Document,
        profile: engine::Profile,
        draft: Option<Draft>,
    ) -> Result<()> {
        self.launch_with(key, draft.clone(), move |cancel| {
            let began = Instant::now();
            let mut document = document;
            if let Some(draft) = &draft {
                let mut editor = Editor::new(document);
                editor.edit(
                    0,
                    CueId(draft.cue_id.get()),
                    Milliseconds::new(draft.start_ms.get())?,
                    Milliseconds::new(draft.end_ms.get())?,
                    draft.text.clone(),
                )?;
                document = editor.document().clone();
            }
            let report = engine::evaluate(&document, &profile, |_, _| {
                if cancel.load(Ordering::Relaxed) {
                    return Err(engine::Error::Cancelled);
                }
                if began.elapsed() > Duration::from_secs(10) {
                    return Err(engine::Error::Limit(
                        "Quality check exceeded its 10 second deadline".into(),
                    ));
                }
                Ok(())
            })
            .map_err(|e| e.to_string())?;
            match draft {
                Some(draft) => report
                    .metrics
                    .iter()
                    .find(|m| m.cue_id == draft.cue_id.get().to_string())
                    .map(|m| Output::Draft(m.into()))
                    .ok_or_else(|| "Draft cue no longer exists".into()),
                None => Ok(Output::Accepted(report)),
            }
        })
    }
    fn launch_with(
        &mut self,
        key: Key,
        draft: Option<Draft>,
        run: impl FnOnce(Arc<AtomicBool>) -> Result<Output> + Send + 'static,
    ) -> Result<()> {
        if self.worker.is_some() || self.closed {
            return Err("Quality worker is not idle".into());
        }
        let id = self
            .id
            .checked_add(1)
            .filter(|id| *id <= crate::ipc_number::MAX_SAFE_INTEGER)
            .ok_or("Quality identities exhausted")?;
        let cancel = Arc::new(AtomicBool::new(false));
        let token = cancel.clone();
        let handle = std::thread::Builder::new()
            .name("subtitle-quality".into())
            .spawn(move || run(token))
            .map_err(|e| format!("Cannot start quality worker: {e}"))?;
        self.worker = Some(Worker {
            key,
            draft,
            cancel,
            handle: Some(handle),
        });
        self.id = id;
        self.id_key = Some(key);
        Ok(())
    }
}
impl Store {
    pub(super) fn quality_key(&self) -> Result<Key> {
        Ok(Key {
            generation: self.generation.try_into()?,
            revision: self
                .loaded
                .as_ref()
                .map_or(0, |d| d.editor.revision())
                .try_into()?,
            epoch: self.quality.epoch().try_into()?,
            loaded: self.loaded.is_some(),
        })
    }
    pub(super) fn invalidate_checks(&mut self) {
        if let Ok(key) = self.quality_key() {
            self.checks.synchronize(key);
            self.gaps.invalidate(key);
        } else {
            self.checks.cancel_worker();
            self.checks.report = None;
        }
    }
}
impl Documents {
    pub(crate) fn quality_read(&self, mut request: Request) -> Result<View> {
        if request.limit > 200 || request.metrics_limit > 200 {
            return Err("Quality pages are limited to 200 rows".into());
        }
        let mut store = self.0.lock().map_err(|_| "Document state unavailable")?;
        let key = store.quality_key()?;
        if request.generation != key.generation || request.revision != key.revision {
            return Err("Subtitles changed before quality read".into());
        }
        store.checks.synchronize(key);
        store.checks.refresh();
        let profile = store.quality.context().profile;
        if request.restart {
            store.checks.cancel_worker();
            store.checks.report = None;
            store.checks.error = None;
            store.checks.draft_metric = None;
            store.checks.draft_error = None;
            store.checks.cancelled = false;
        }
        if let Some(change) = request.draft.take() {
            let draft = match change {
                DraftChange::Clear => None,
                DraftChange::Update { value } => Some(value),
            };
            if draft
                .as_ref()
                .is_some_and(|d| d.text.len() > engine::MAX_TEXT_BYTES)
            {
                return Err("Quality draft exceeds 16 MiB".into());
            }
            if draft != store.checks.draft {
                if store
                    .checks
                    .worker
                    .as_ref()
                    .is_some_and(|w| w.draft.is_some())
                {
                    store.checks.cancel_worker();
                }
                store.checks.draft = draft;
                store.checks.draft_metric = None;
                store.checks.draft_error = None;
            }
        }
        let checks = &store.checks;
        if request.run
            && key.loaded
            && !checks.closed
            && !checks.cancelled
            && checks.worker.is_none()
        {
            let accepted = checks.report.is_none() && checks.error.is_none();
            let draft = if checks.report.is_some()
                && checks.draft_metric.is_none()
                && checks.draft_error.is_none()
            {
                checks.draft.clone()
            } else {
                None
            };
            if accepted || draft.is_some() {
                let document = store
                    .loaded
                    .as_ref()
                    .ok_or("Open subtitles first")?
                    .editor
                    .document()
                    .clone();
                store.checks.launch(key, document, profile.clone(), draft)?;
            }
        }
        project_view(&store.checks, key, profile, &request)
    }
    pub(crate) fn quality_cancel(&self, id: u64) -> Result<()> {
        let mut store = self.0.lock().map_err(|_| "Document state unavailable")?;
        if store.checks.id == id && store.checks.id_key == store.checks.key {
            store.checks.cancel_worker();
            store.checks.cancelled = true;
            store.checks.draft_metric = None;
        }
        Ok(())
    }
    pub(crate) fn shutdown_quality(&self) -> Result<()> {
        let worker = {
            let mut store = self.0.lock().map_err(|_| "Document state unavailable")?;
            store.checks.closed = true;
            store.checks.cancel_worker();
            store.checks.worker.take()
        };
        drop(worker); // joins outside the document lock
        self.shutdown_gaps()
    }
}
fn project_view(
    checks: &Checks,
    key: Key,
    profile: engine::Profile,
    request: &Request,
) -> Result<View> {
    let phase = if checks.closed {
        Phase::Closed
    } else if checks
        .worker
        .as_ref()
        .is_some_and(|w| w.cancel.load(Ordering::Relaxed))
    {
        Phase::Cancelling
    } else if checks.cancelled {
        Phase::Cancelled
    } else if checks.report.is_some() {
        Phase::Ready
    } else if checks.error.is_some() {
        Phase::Failed
    } else if checks.worker.is_some() {
        Phase::Checking
    } else {
        Phase::Idle
    };
    let mut view = View {
        key,
        id: checks.id.try_into()?,
        phase,
        profile,
        profile_hash: None,
        error: checks.error.clone(),
        findings_count: 0,
        affected_cues: 0,
        filtered_count: 0,
        filtered_cues: 0,
        offset: request.offset,
        findings: vec![],
        metrics: vec![],
        selected: None,
        previous_cue: None,
        next_cue: None,
        draft: checks.draft_metric.clone(),
        draft_checking: checks.draft.is_some()
            && checks.draft_metric.is_none()
            && checks.draft_error.is_none()
            && !checks.cancelled,
        draft_error: checks.draft_error.clone(),
    };
    let Some(report) = &checks.report else {
        return Ok(view);
    };
    view.profile_hash = Some(report.profile_hash.clone());
    view.findings_count = report.findings.len() as u32;
    view.affected_cues = report.affected_cues;
    let filtered: Vec<_> = report
        .findings
        .iter()
        .filter(|f| {
            request.rule.is_none_or(|r| f.rule == r)
                && request.severity.is_none_or(|s| f.severity == s)
        })
        .collect();
    let affected: HashSet<&str> = filtered
        .iter()
        .flat_map(|f| std::iter::once(f.cue_id.as_str()).chain(f.related_cue_id.as_deref()))
        .collect();
    let cues: Vec<_> = report
        .metrics
        .iter()
        .filter(|m| affected.contains(m.cue_id.as_str()))
        .collect();
    view.filtered_count = filtered.len() as u32;
    view.filtered_cues = cues.len() as u32;
    view.offset = request
        .offset
        .min(view.filtered_count.saturating_sub(1) / request.limit.max(1) * request.limit.max(1));
    view.findings = filtered
        .into_iter()
        .skip(view.offset as usize)
        .take(request.limit as usize)
        .cloned()
        .collect();
    view.metrics = report
        .metrics
        .iter()
        .skip(request.metrics_offset as usize)
        .take(request.metrics_limit as usize)
        .map(Metric::from)
        .collect();
    let selected = request.selected.map(|id| id.get().to_string());
    let index = selected
        .as_ref()
        .and_then(|id| report.metrics.iter().position(|m| &m.cue_id == id));
    view.selected = index.map(|i| (&report.metrics[i]).into());
    view.next_cue = cues
        .iter()
        .find(|m| index.is_none_or(|i| m.cue_index as usize > i))
        .or(cues.first())
        .map(|m| m.cue_id.clone());
    view.previous_cue = cues
        .iter()
        .rev()
        .find(|m| index.is_none_or(|i| (m.cue_index as usize) < i))
        .or(cues.last())
        .map(|m| m.cue_id.clone());
    Ok(view)
}

#[cfg(test)]
mod tests;
#[cfg(test)]
pub(crate) fn ipc_sample() -> serde_json::Value {
    serde_json::to_value(tests::ready(&tests::fixture())).unwrap()
}
