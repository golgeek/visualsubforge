//! Explicit read-only reports. The worker owns a snapshot; accepted state is never
//! handed to it. Admission/publication are locked; evaluation/joining are not.
use super::*;
use crate::{
    quality_check::Key,
    scenes::{SceneCapture, SceneKey},
    statistics::{Phase, StatisticsView},
};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    thread::JoinHandle,
    time::{Duration, Instant},
};
use vsf_application::quality::statistics::{self as engine, Report, Selection};
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
    cancel: Arc<AtomicBool>,
    worker: Option<JoinHandle<Result<Report>>>,
    report: Option<Report>,
    error: Option<String>,
    stale: bool,
}
impl Job {
    fn cancel(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        self.report = None;
    }
    fn refresh(&mut self) {
        if let Some(worker) = self.worker.take_if(|w| w.is_finished()) {
            let result = worker
                .join()
                .unwrap_or_else(|_| Err("Statistics worker stopped unexpectedly".into()));
            if !self.cancel.load(Ordering::Relaxed) {
                match result {
                    Ok(report) => self.report = Some(report),
                    Err(error) => self.error = Some(error),
                }
            }
        }
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
impl State {
    pub(super) fn invalidate(&mut self, key: Key) {
        if let Some(job) = &mut self.job
            && job.key != key
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
            return Err("Statistics is shutting down".into());
        }
        if self.job.as_ref().is_some_and(|j| j.worker.is_some()) {
            return Err("Wait for the current statistics calculation to finish cancelling".into());
        }
        Ok(())
    }
    fn launch(
        &mut self,
        key: Key,
        scenes: SceneKey,
        run: impl FnOnce(Arc<AtomicBool>) -> Result<Report> + Send + 'static,
    ) -> Result<u64> {
        self.ensure_idle()?;
        let id = self
            .next_id
            .checked_add(1)
            .filter(|i| *i <= crate::ipc_number::MAX_SAFE_INTEGER)
            .ok_or("Statistics identities exhausted")?;
        let cancel = Arc::new(AtomicBool::new(false));
        let token = cancel.clone();
        let worker = std::thread::Builder::new()
            .name("subtitle-statistics".into())
            .spawn(move || run(token))
            .map_err(|e| format!("Cannot start statistics: {e}"))?;
        self.job = Some(Job {
            id,
            key,
            scenes,
            cancel,
            worker: Some(worker),
            report: None,
            error: None,
            stale: false,
        });
        self.next_id = id;
        Ok(id)
    }
}
impl Documents {
    pub(crate) fn statistics_start(
        &self,
        generation: u64,
        revision: u64,
        scenes: SceneCapture,
        mut selection: Selection,
    ) -> Result<u64> {
        let mut store = self.0.lock().map_err(|_| "Document state unavailable")?;
        let key = store.quality_key()?;
        if !key.loaded || key.generation.get() != generation || key.revision.get() != revision {
            return Err("Subtitles changed before statistics; calculate again".into());
        }
        store.statistics.ensure_idle()?;
        let loaded = store.loaded.as_ref().ok_or("Open subtitles first")?;
        selection
            .validate(loaded.editor.document().cues().len())
            .map_err(|e| e.to_string())?;
        let document = loaded.editor.document().clone();
        let profile = store.quality.context().profile;
        store.statistics.launch(key, scenes.key, move |cancel| {
            let began = Instant::now();
            engine::evaluate(&document, &profile, &scenes.data, selection, || {
                if cancel.load(Ordering::Relaxed) {
                    return Err(vsf_application::quality::Error::Cancelled);
                }
                if began.elapsed() > Duration::from_secs(10) {
                    return Err(vsf_application::quality::Error::Limit(
                        "Statistics exceeded its 10 second deadline".into(),
                    ));
                }
                Ok(())
            })
            .map_err(|e| e.to_string())
        })
    }
    pub(crate) fn statistics_context(&self, id: u64) -> Result<SceneKey> {
        let store = self.0.lock().map_err(|_| "Document state unavailable")?;
        Ok(store
            .statistics
            .job
            .as_ref()
            .filter(|j| j.id == id)
            .ok_or("Statistics report was replaced")?
            .scenes)
    }
    pub(crate) fn statistics_view(
        &self,
        id: u64,
        offset: u32,
        limit: u32,
    ) -> Result<StatisticsView> {
        if limit > 200 {
            return Err("Statistics pages are limited to 200 findings".into());
        }
        let mut store = self.0.lock().map_err(|_| "Document state unavailable")?;
        let key = store.quality_key()?;
        store.statistics.invalidate(key);
        let closed = store.statistics.closed;
        let job = store
            .statistics
            .job
            .as_mut()
            .filter(|j| j.id == id)
            .ok_or("Statistics report was replaced")?;
        job.refresh();
        let phase = if closed {
            Phase::Closed
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
        } else if job.report.is_some() {
            Phase::Ready
        } else {
            Phase::Preparing
        };
        let page = job
            .report
            .as_ref()
            .map(|r| {
                offset
                    .min((r.findings.len() as u32).saturating_sub(1) / limit.max(1) * limit.max(1))
            })
            .unwrap_or(0);
        Ok(StatisticsView {
            id: id.try_into()?,
            key: job.key,
            session: job.scenes.session.try_into()?,
            phase,
            error: job.error.clone(),
            summary: job.report.as_ref().map(|r| r.summary.clone()),
            offset: page,
            findings: job
                .report
                .as_ref()
                .map(|r| {
                    r.findings
                        .iter()
                        .skip(page as usize)
                        .take(limit as usize)
                        .cloned()
                        .collect()
                })
                .unwrap_or_default(),
        })
    }
    pub(crate) fn statistics_cancel(&self, id: u64) -> Result<()> {
        let mut store = self.0.lock().map_err(|_| "Document state unavailable")?;
        if let Some(job) = store.statistics.job.as_mut().filter(|j| j.id == id) {
            job.cancel();
        }
        Ok(())
    }
    pub(super) fn shutdown_statistics(&self) -> Result<()> {
        let job = {
            let mut store = self.0.lock().map_err(|_| "Document state unavailable")?;
            store.statistics.closed = true;
            if let Some(job) = &mut store.statistics.job {
                job.cancel();
            }
            store.statistics.job.take()
        };
        drop(job);
        Ok(()) // join outside the document lock
    }
}
#[cfg(test)]
mod tests;
#[cfg(test)]
pub(crate) fn ipc_sample() -> serde_json::Value {
    let (_dir, docs, scenes) = tests::fixture();
    let id = tests::start(&docs, &scenes, Selection::default());
    serde_json::to_value(tests::ready(&docs, id)).unwrap()
}
