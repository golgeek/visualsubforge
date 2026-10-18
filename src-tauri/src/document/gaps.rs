//! One explicit proposal worker and no pending queue. Immutable capture and
//! publication share the document lock; evaluation and shutdown join do not.
use super::*;
use crate::{
    gap_review::{GapView, Phase, Row},
    quality_check::Key,
    scenes::{SceneCapture, SceneKey},
};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    thread::JoinHandle,
    time::{Duration, Instant},
};
use vsf_application::quality::{self as engine, Profile, gap::Proposal};

#[derive(Default)]
pub(super) struct State {
    next_id: u64,
    closed: bool,
    job: Option<Job>,
}
struct Job {
    id: u64,
    key: Key,
    scenes: SceneKey,
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
                .unwrap_or_else(|_| Err("Gap proposal worker stopped unexpectedly".into()));
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
            return Err("Gap preview is shutting down".into());
        }
        if self.job.as_ref().is_some_and(|j| j.worker.is_some()) {
            return Err("Wait for the current gap proposal to finish cancelling".into());
        }
        Ok(())
    }
    fn launch(
        &mut self,
        key: Key,
        scenes: SceneKey,
        profile: Profile,
        run: impl FnOnce(Arc<AtomicBool>) -> Result<Proposal> + Send + 'static,
    ) -> Result<u64> {
        self.ensure_idle()?;
        let id = self
            .next_id
            .checked_add(1)
            .filter(|i| *i <= crate::ipc_number::MAX_SAFE_INTEGER)
            .ok_or("Gap identities exhausted")?;
        let hash = profile.fingerprint().map_err(|e| e.to_string())?;
        let cancel = Arc::new(AtomicBool::new(false));
        let token = cancel.clone();
        let worker = std::thread::Builder::new()
            .name("gap-proposal".into())
            .spawn(move || run(token))
            .map_err(|e| format!("Cannot start gap proposal: {e}"))?;
        self.job = Some(Job {
            id,
            key,
            scenes,
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
    pub(crate) fn gap_start(
        &self,
        generation: u64,
        revision: u64,
        scenes: SceneCapture,
    ) -> Result<u64> {
        let mut store = self.0.lock().map_err(|_| "Document state unavailable")?;
        let key = store.quality_key()?;
        if !key.loaded || key.generation.get() != generation || key.revision.get() != revision {
            return Err("Subtitles changed before gap preview; try again".into());
        }
        // Reject busy admission before cloning another document snapshot.
        store.gaps.ensure_idle()?;
        let loaded = store.loaded.as_ref().ok_or("Open subtitles first")?;
        let document = loaded.editor.document().clone();
        let format = loaded.source.format();
        let profile = store.quality.context().profile;
        let captured = profile.clone();
        let lexicon = store.quality.lexicon();
        store.gaps.launch(key, scenes.key, profile, move |cancel| {
            let start = Instant::now();
            engine::gap::propose_for_format(
                document,
                format,
                engine::Evaluation {
                    profile: &captured,
                    format,
                    lexicon: Some(&lexicon),
                },
                &scenes.data,
                || {
                    if cancel.load(Ordering::Relaxed) {
                        return Err(engine::Error::Cancelled);
                    }
                    if start.elapsed() > Duration::from_secs(10) {
                        return Err(engine::Error::Limit(
                            "Gap preview exceeded its 10 second deadline".into(),
                        ));
                    }
                    Ok(())
                },
            )
            .map_err(|e| e.to_string())
        })
    }
    pub(crate) fn gap_context(&self, id: u64) -> Result<SceneKey> {
        let store = self.0.lock().map_err(|_| "Document state unavailable")?;
        Ok(store
            .gaps
            .job
            .as_ref()
            .filter(|j| j.id == id)
            .ok_or("Gap proposal was replaced")?
            .scenes)
    }
    pub(crate) fn gap_view(&self, id: u64, offset: u32, limit: u32) -> Result<GapView> {
        if limit > 200 {
            return Err("Gap pages are limited to 200 rows".into());
        }
        let mut store = self.0.lock().map_err(|_| "Document state unavailable")?;
        let key = store.quality_key()?;
        store.gaps.invalidate(key);
        let closed = store.gaps.closed;
        let job = store
            .gaps
            .job
            .as_mut()
            .filter(|j| j.id == id)
            .ok_or("Gap proposal was replaced")?;
        job.refresh();
        project(job, closed, offset, limit)
    }
    pub(crate) fn gap_cancel(&self, id: u64) -> Result<()> {
        let mut store = self.0.lock().map_err(|_| "Document state unavailable")?;
        if let Some(job) = store.gaps.job.as_mut().filter(|j| j.id == id) {
            job.cancel();
        }
        Ok(())
    }
    /// Called while media selection and scene state remain locked and current.
    pub(crate) fn gap_accept(&self, id: u64, acknowledge_no_media: bool) -> Result<Snapshot> {
        let mut store = self.0.lock().map_err(|_| "Document state unavailable")?;
        let key = store.quality_key()?;
        store.gaps.invalidate(key);
        if store.gaps.closed {
            return Err("Gap preview is shutting down".into());
        }
        let job = store
            .gaps
            .job
            .as_mut()
            .filter(|j| j.id == id)
            .ok_or("Gap proposal was replaced")?;
        job.refresh();
        if job.key != key || job.stale || job.cancel.load(Ordering::Relaxed) || job.accepted {
            return Err("Gap proposal is no longer current; create a new preview".into());
        }
        let proposal = store
            .gaps
            .job
            .as_ref()
            .and_then(|j| j.proposal.as_ref())
            .ok_or("Wait for a completed gap preview")?;
        let edit = proposal
            .edit(
                store
                    .loaded
                    .as_ref()
                    .ok_or("Open subtitles first")?
                    .editor
                    .document(),
                acknowledge_no_media,
            )
            .map_err(|e| e.to_string())?;
        store.loaded_mut()?.apply_edit(key.revision.get(), edit)?;
        if let Some(job) = &mut store.gaps.job {
            job.accepted = true;
            job.proposal = None;
        }
        store.invalidate_checks();
        Ok(store.snapshot())
    }
    pub(super) fn shutdown_gaps(&self) -> Result<()> {
        let job = {
            let mut store = self.0.lock().map_err(|_| "Document state unavailable")?;
            store.gaps.closed = true;
            if let Some(job) = &mut store.gaps.job {
                job.cancel();
            }
            store.gaps.job.take()
        };
        drop(job); // cancellation and join outside the document mutex
        Ok(())
    }
}
fn project(job: &Job, closed: bool, offset: u32, limit: u32) -> Result<GapView> {
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
    let mut view = GapView {
        id: job.id.try_into()?,
        key: job.key,
        session: job.scenes.session.try_into()?,
        phase,
        profile: job.profile.clone(),
        profile_hash: job.hash.clone(),
        error: job.error.clone(),
        coverage: None,
        total: 0,
        changed: 0,
        skipped: 0,
        before_findings: 0,
        after_findings: 0,
        offset,
        rows: vec![],
    };
    if let Some(p) = &job.proposal {
        view.coverage = Some(p.coverage());
        view.total = p.rows().len() as u32;
        view.changed = p.changed() as u32;
        view.skipped = view.total - view.changed;
        view.before_findings = p.before().findings.len() as u32;
        view.after_findings = p.after().findings.len() as u32;
        view.offset = offset.min(view.total.saturating_sub(1) / limit.max(1) * limit.max(1));
        for row in p
            .rows()
            .iter()
            .skip(view.offset as usize)
            .take(limit as usize)
        {
            view.rows.push(Row {
                cue_id: p.before().metrics[row.cue_index].cue_id.clone(),
                cue_index: row.cue_index as u32,
                next_cue_index: row.next_cue_index as u32,
                new_end_ms: row.new_end.map(|e| e.get().try_into()).transpose()?,
                skipped: row.skipped,
                explanation: row.skipped.map(|s| s.explanation().into()),
                before: (&p.before().metrics[row.cue_index]).into(),
                after: (&p.after().metrics[row.cue_index]).into(),
                next_before: (&p.before().metrics[row.next_cue_index]).into(),
                next_after: (&p.after().metrics[row.next_cue_index]).into(),
            });
        }
    }
    Ok(view)
}

#[cfg(test)]
mod tests;
#[cfg(test)]
pub(crate) fn ipc_sample() -> serde_json::Value {
    let (_directory, docs) = tests::fixture();
    let id = tests::start(&docs);
    serde_json::to_value(tests::ready(&docs, id)).unwrap()
}
