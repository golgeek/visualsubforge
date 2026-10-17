//! One explicit proposal worker and no pending queue. Immutable capture and
//! publication share the document lock; evaluation and shutdown join do not.
use super::*;
use crate::{
    quality_check::Key,
    text_review::{Match, Operation, Phase, Row, Target, TextView},
};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    thread::JoinHandle,
    time::{Duration, Instant},
};
use vsf_application::{
    quality::{self as engine, Profile},
    text::{self, Proposal},
};

#[derive(Default)]
pub(super) struct State {
    next_id: u64,
    closed: bool,
    job: Option<Job>,
}
struct Job {
    id: u64,
    key: Key,
    operation: Operation,
    profile: Profile,
    hash: String,
    cancel: Arc<AtomicBool>,
    worker: Option<JoinHandle<Result<Proposal>>>,
    proposal: Option<Proposal>,
    error: Option<String>,
    stale: bool,
    accepted: bool,
}
impl Job {
    fn cancel(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        self.proposal = None;
    }
    fn refresh(&mut self) {
        if let Some(worker) = self.worker.take_if(|w| w.is_finished()) {
            let result = worker
                .join()
                .unwrap_or_else(|_| Err("Text proposal worker stopped unexpectedly".into()));
            if !self.cancel.load(Ordering::Relaxed) {
                match result {
                    Ok(proposal) => self.proposal = Some(proposal),
                    Err(e) => self.error = Some(e),
                }
            }
        }
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        // Normally finished work was reaped or shutdown took it outside the lock.
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
impl State {
    pub(super) fn invalidate(&mut self, key: Key) {
        if let Some(job) = &mut self.job
            && job.key != key
            && !job.accepted
        {
            job.stale = true;
            job.cancel();
        }
    }
    fn ensure_idle(&mut self) -> Result<()> {
        if let Some(job) = &mut self.job {
            job.refresh();
        }
        if self.closed {
            return Err("Text preview is shutting down".into());
        }
        if self.job.as_ref().is_some_and(|j| j.worker.is_some()) {
            return Err("Wait for the current text proposal to finish cancelling".into());
        }
        Ok(())
    }
    fn launch(
        &mut self,
        key: Key,
        operation: Operation,
        profile: Profile,
        run: impl FnOnce(Arc<AtomicBool>) -> Result<Proposal> + Send + 'static,
    ) -> Result<u64> {
        self.ensure_idle()?;
        let id = self
            .next_id
            .checked_add(1)
            .filter(|i| *i <= crate::ipc_number::MAX_SAFE_INTEGER)
            .ok_or("Text identities exhausted")?;
        let hash = profile.fingerprint().map_err(|e| e.to_string())?;
        let cancel = Arc::new(AtomicBool::new(false));
        let token = cancel.clone();
        let worker = std::thread::Builder::new()
            .name("text-proposal".into())
            .spawn(move || run(token))
            .map_err(|e| format!("Cannot start text proposal: {e}"))?;
        self.job = Some(Job {
            id,
            key,
            operation,
            profile,
            hash,
            cancel,
            worker: Some(worker),
            proposal: None,
            error: None,
            stale: false,
            accepted: false,
        });
        self.next_id = id;
        Ok(id)
    }
}
impl Documents {
    pub(crate) fn text_matches(
        &self,
        id: u64,
        cue_index: u32,
        offset: u32,
        limit: u32,
    ) -> Result<Vec<Match>> {
        if limit > 200 {
            return Err("Match pages are limited to 200 entries".into());
        }
        let mut store = self.0.lock().map_err(|_| "Document state unavailable")?;
        let key = store.quality_key()?;
        store.texts.invalidate(key);
        let job = store
            .texts
            .job
            .as_ref()
            .filter(|j| j.id == id && j.key == key && !j.cancel.load(Ordering::Relaxed))
            .ok_or("Search changed; search again")?;
        let row = job
            .proposal
            .as_ref()
            .and_then(|p| p.rows().iter().find(|r| r.cue_index == cue_index as usize))
            .ok_or("Search result unavailable")?;
        Ok(row
            .matches
            .iter()
            .skip(offset as usize)
            .take(limit as usize)
            .map(match_view)
            .collect())
    }
    pub(crate) fn text_start(
        &self,
        generation: u64,
        revision: u64,
        operation: Operation,
    ) -> Result<u64> {
        let mut store = self.0.lock().map_err(|_| "Document state unavailable")?;
        let key = store.quality_key()?;
        if !key.loaded || key.generation.get() != generation || key.revision.get() != revision {
            return Err("Subtitles changed before text preview; try again".into());
        }
        // Reject busy admission before cloning another document snapshot.
        store.texts.ensure_idle()?;
        let document = store
            .loaded
            .as_ref()
            .ok_or("Open subtitles first")?
            .editor
            .document()
            .clone();
        let profile = store.quality.context().profile;
        let captured = profile.clone();
        let request = operation.clone();
        store.texts.launch(key, operation, profile, move |cancel| {
            let start = Instant::now();
            text::propose(document, &captured, &request, || {
                if cancel.load(Ordering::Relaxed) {
                    return Err(engine::Error::Cancelled);
                }
                if start.elapsed() > Duration::from_secs(10) {
                    return Err(engine::Error::Limit(
                        "Text preview exceeded its 10 second deadline".into(),
                    ));
                }
                Ok(())
            })
            .map_err(|e| e.to_string())
        })
    }
    pub(crate) fn text_view(&self, id: u64, offset: u32, limit: u32) -> Result<TextView> {
        if limit > 200 {
            return Err("Text pages are limited to 200 rows".into());
        }
        let mut store = self.0.lock().map_err(|_| "Document state unavailable")?;
        let key = store.quality_key()?;
        store.texts.invalidate(key);
        let closed = store.texts.closed;
        let job = store
            .texts
            .job
            .as_mut()
            .filter(|j| j.id == id)
            .ok_or("Text proposal was replaced")?;
        job.refresh();
        project(job, closed, offset, limit)
    }
    pub(crate) fn text_cancel(&self, id: u64) -> Result<()> {
        let mut store = self.0.lock().map_err(|_| "Document state unavailable")?;
        if let Some(job) = store.texts.job.as_mut().filter(|j| j.id == id) {
            job.cancel();
        }
        Ok(())
    }
    pub(crate) fn text_accept(&self, id: u64, target: Option<Target>) -> Result<Snapshot> {
        let mut store = self.0.lock().map_err(|_| "Document state unavailable")?;
        let key = store.quality_key()?;
        store.texts.invalidate(key);
        if store.texts.closed {
            return Err("Text preview is shutting down".into());
        }
        let job = store
            .texts
            .job
            .as_mut()
            .filter(|j| j.id == id)
            .ok_or("Text proposal was replaced")?;
        job.refresh();
        if job.key != key || job.stale || job.cancel.load(Ordering::Relaxed) || job.accepted {
            return Err("Text proposal is no longer current; create a new preview".into());
        }
        let proposal = store
            .texts
            .job
            .as_ref()
            .and_then(|j| j.proposal.as_ref())
            .ok_or("Wait for a completed text preview")?;
        let edit = proposal
            .edit(
                store
                    .loaded
                    .as_ref()
                    .ok_or("Open subtitles first")?
                    .editor
                    .document(),
                target.map(|t| (t.cue_index as usize, t.match_index as usize)),
            )
            .map_err(|e| e.to_string())?;
        store.loaded_mut()?.apply_edit(key.revision.get(), edit)?;
        if let Some(job) = &mut store.texts.job {
            job.accepted = true;
            job.proposal = None;
        }
        store.invalidate_checks();
        Ok(store.snapshot())
    }
    pub(super) fn shutdown_texts(&self) -> Result<()> {
        let job = {
            let mut store = self.0.lock().map_err(|_| "Document state unavailable")?;
            store.texts.closed = true;
            if let Some(job) = &mut store.texts.job {
                job.cancel();
            }
            store.texts.job.take()
        };
        drop(job); // cancellation and join outside the document mutex
        Ok(())
    }
}

fn match_view(found: &text::Match) -> Match {
    Match {
        start: found.start,
        end: found.end,
        skipped: found.skipped.map(str::to_owned),
    }
}
fn project(job: &Job, closed: bool, offset: u32, limit: u32) -> Result<TextView> {
    let phase = if closed {
        Phase::Closed
    } else if job.accepted {
        Phase::Accepted
    } else if job.stale {
        Phase::Stale
    } else if job.cancel.load(Ordering::Relaxed) {
        if job.worker.is_some() {
            Phase::Cancelling
        } else {
            Phase::Cancelled
        }
    } else if job.error.is_some() {
        Phase::Failed
    } else if job.proposal.is_some() {
        Phase::Reviewing
    } else {
        Phase::Preparing
    };
    let mut view = TextView {
        id: job.id.try_into()?,
        key: job.key,
        phase,
        profile_name: job.profile.name.clone(),
        profile_hash: job.hash.clone(),
        operation: job.operation.clone(),
        error: job.error.clone(),
        total: 0,
        changed: 0,
        matches: 0,
        before_findings: 0,
        after_findings: 0,
        offset,
        rows: vec![],
    };
    if let Some(p) = &job.proposal {
        view.total = p.rows().len() as u32;
        view.changed = p.changed() as u32;
        view.matches = p.matches() as u32;
        view.before_findings = p.before_findings() as u32;
        view.after_findings = p.after_findings() as u32;
        view.offset = offset.min(view.total.saturating_sub(1) / limit.max(1) * limit.max(1));
        for row in p
            .rows()
            .iter()
            .skip(view.offset as usize)
            .take(limit as usize)
        {
            let cue = &p.document().cues()[row.cue_index];
            view.rows.push(Row {
                cue_id: cue.id().0.try_into()?,
                cue_index: row.cue_index as u32,
                before: if cue.text().len() > text::MAX_CUE_BYTES {
                    format!(
                        "{}\n[… oversized cue; open it in the editor to see all text …]",
                        cue.text().chars().take(512).collect::<String>()
                    )
                } else {
                    cue.text().into()
                },
                after: row.after.clone(),
                skipped: row.skipped.map(str::to_owned),
                match_count: row.matches.len() as u32,
                matches: row.matches.iter().take(100).map(match_view).collect(),
            });
        }
    }
    Ok(view)
}

#[cfg(test)]
mod tests;
#[cfg(test)]
pub(crate) fn ipc_sample() -> serde_json::Value {
    let (_dir, docs) = tests::fixture();
    let id = tests::start(&docs);
    serde_json::to_value(tests::ready(&docs, id)).unwrap()
}
