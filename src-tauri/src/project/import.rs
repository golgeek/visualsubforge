//! One bounded project-import IO worker; no queue or accepted workspace state.
//! Workers own only a job's immutable inputs/state, never the owning Importer.
//! Cancellation and final no-clobber publication serialize on the job mutex.
mod api;
mod prepare;
#[cfg(test)]
mod tests;
use super::Result;
pub use api::*;
use api::{
    LegacyImportPhase as ImportPhase, LegacyImportReport as ImportReport,
    LegacyImportRequest as ImportRequest, LegacyImportReview as ImportReview,
};
use prepare::Prepared;
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

const DEADLINE: Duration = Duration::from_secs(10);

#[derive(Default)]
pub struct Importer {
    state: Mutex<State>,
}
#[derive(Default)]
struct State {
    next_id: u64,
    closed: bool,
    job: Option<Job>,
}
struct Job {
    id: u64,
    cancel: Arc<AtomicBool>,
    state: Arc<Mutex<JobState>>,
    worker: Option<JoinHandle<()>>,
}
struct JobState {
    phase: ImportPhase,
    prepared: Option<Arc<Prepared>>,
    error: Option<String>,
    output: Option<String>,
}
impl JobState {
    fn cancel(&mut self, cancel: &AtomicBool) {
        // A completed publication remains observable, even after late Cancel.
        if self.phase != ImportPhase::Published {
            cancel.store(true, Ordering::Release);
            self.phase = ImportPhase::Cancelled;
            self.prepared = None;
        }
    }
}
fn checkpoint(cancel: &AtomicBool, started: Instant) -> Result<()> {
    if cancel.load(Ordering::Acquire) {
        return Err("Legacy project import cancelled".into());
    }
    if started.elapsed() >= DEADLINE {
        return Err("Legacy project import exceeded its ten-second deadline".into());
    }
    Ok(())
}
impl Importer {
    /// Join only finished workers here, always outside the owner/job locks.
    fn reap(&self) -> Result<()> {
        let finished = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| "Project import state unavailable")?;
            state.job.as_mut().and_then(|job| {
                if job.worker.as_ref().is_some_and(JoinHandle::is_finished) {
                    job.worker.take().map(|worker| (worker, job.state.clone()))
                } else {
                    None
                }
            })
        };
        if let Some((worker, state)) = finished
            && worker.join().is_err()
        {
            let mut state = state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if !matches!(state.phase, ImportPhase::Published | ImportPhase::Cancelled) {
                state.phase = ImportPhase::Failed;
                state.prepared = None;
                state.error =
                    Some("Legacy project import worker failed; inspect the project again".into());
            }
        }
        Ok(())
    }
    pub(crate) fn start(
        &self,
        request: ImportRequest,
        policy: crate::quality_profile::Policy,
    ) -> Result<u64> {
        self.start_with(request, policy, || Ok(()))
    }
    fn start_with(
        &self,
        request: ImportRequest,
        policy: crate::quality_profile::Policy,
        hook: impl FnOnce() -> Result<()> + Send + 'static,
    ) -> Result<u64> {
        request.validate()?;
        let policy = crate::quality_profile::Policy::new(policy.profile, policy.timing)?;
        self.reap()?;
        let mut owner = self
            .state
            .lock()
            .map_err(|_| "Project import state unavailable")?;
        if owner.closed {
            return Err("Project import is shutting down".into());
        }
        if owner.job.as_ref().is_some_and(|job| job.worker.is_some()) {
            return Err(
                "Wait for the current import operation to finish or finish cancelling".into(),
            );
        }
        let id = owner
            .next_id
            .checked_add(1)
            .filter(|id| *id <= crate::ipc_number::MAX_SAFE_INTEGER)
            .ok_or("Project import identities exhausted")?;
        let cancel = Arc::new(AtomicBool::new(false));
        let state = Arc::new(Mutex::new(JobState {
            phase: ImportPhase::Preparing,
            prepared: None,
            error: None,
            output: None,
        }));
        let worker_state = state.clone();
        let worker_cancel = cancel.clone();
        let worker = std::thread::Builder::new()
            .name("project-import".into())
            .spawn(move || {
                let started = Instant::now();
                let result = (|| {
                    checkpoint(&worker_cancel, started)?;
                    hook()?;
                    prepare::inspect(request, policy, || checkpoint(&worker_cancel, started))
                })();
                let mut state = worker_state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if state.phase != ImportPhase::Preparing || worker_cancel.load(Ordering::Acquire) {
                    return;
                }
                match result {
                    Ok(prepared) => {
                        state.prepared = Some(Arc::new(prepared));
                        state.phase = ImportPhase::Reviewing;
                    }
                    Err(error) => {
                        state.phase = ImportPhase::Failed;
                        state.error = Some(error);
                    }
                }
            })
            .map_err(|e| format!("Cannot start project import: {e}"))?;
        // No previous worker remains; replacing a review invalidates its ID.
        owner.next_id = id;
        owner.job = Some(Job {
            id,
            cancel,
            state,
            worker: Some(worker),
        });
        Ok(id)
    }
    pub(crate) fn view(&self, id: u64) -> Result<ImportReview> {
        self.reap()?;
        let owner = self
            .state
            .lock()
            .map_err(|_| "Project import state unavailable")?;
        let job = owner
            .job
            .as_ref()
            .filter(|j| j.id == id)
            .ok_or("Project import review was replaced")?;
        let state = job
            .state
            .lock()
            .map_err(|_| "Project import result unavailable")?;
        let running = job.worker.is_some();
        let phase = match state.phase {
            ImportPhase::Published => ImportPhase::Published,
            _ if owner.closed => ImportPhase::Closed,
            ImportPhase::Cancelled if running => ImportPhase::Cancelling,
            phase => phase,
        };
        Ok(ImportReview {
            id: id.try_into()?,
            phase,
            running,
            report: state.prepared.as_ref().map(|p| {
                let mut report = p.report.clone();
                report.can_publish &= phase == ImportPhase::Reviewing && !running;
                report
            }),
            output_path: state.output.clone(),
            error: state.error.clone(),
        })
    }
    pub(crate) fn cancel(&self, id: u64) -> Result<()> {
        let owner = self
            .state
            .lock()
            .map_err(|_| "Project import state unavailable")?;
        if let Some(job) = owner.job.as_ref().filter(|j| j.id == id) {
            job.state
                .lock()
                .map_err(|_| "Project import result unavailable")?
                .cancel(&job.cancel);
        }
        Ok(())
    }
    pub(crate) fn publish(&self, id: u64, path: String) -> Result<()> {
        self.publish_with(id, path, || Ok(()))
    }
    fn publish_with(
        &self,
        id: u64,
        path: String,
        hook: impl FnOnce() -> Result<()> + Send + 'static,
    ) -> Result<()> {
        if path.is_empty() || path.len() > 32_768 || path.contains('\0') {
            return Err("Choose an output filename with a supported path length".into());
        }
        self.reap()?;
        let mut owner = self
            .state
            .lock()
            .map_err(|_| "Project import state unavailable")?;
        if owner.closed {
            return Err("Project import is shutting down".into());
        }
        let job = owner
            .job
            .as_mut()
            .filter(|j| j.id == id)
            .ok_or("Project import review was replaced")?;
        if job.worker.is_some() {
            return Err("Wait for the current import operation to finish".into());
        }
        let mut state = job
            .state
            .lock()
            .map_err(|_| "Project import result unavailable")?;
        if state.phase != ImportPhase::Reviewing || job.cancel.load(Ordering::Acquire) {
            return Err("Inspect the project again before exporting".into());
        }
        let prepared = state
            .prepared
            .as_ref()
            .filter(|p| p.report.can_publish)
            .ok_or("Resolve the missing or invalid references before exporting")?
            .clone();
        let worker_state = job.state.clone();
        let cancel = job.cancel.clone();
        state.phase = ImportPhase::Publishing;
        state.error = None;
        let worker = std::thread::Builder::new()
            .name("project-import-export".into())
            .spawn(move || {
                let started = Instant::now();
                let result = prepare::publish(
                    &prepared,
                    &PathBuf::from(path),
                    &worker_state,
                    || checkpoint(&cancel, started),
                    hook,
                );
                if let Err(error) = result {
                    let mut state = worker_state
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    if state.phase == ImportPhase::Publishing {
                        state.phase = if error.stale {
                            ImportPhase::Stale
                        } else {
                            ImportPhase::Reviewing
                        };
                        state.error = Some(error.message);
                    }
                }
            });
        match worker {
            Ok(worker) => job.worker = Some(worker),
            Err(error) => {
                state.phase = ImportPhase::Reviewing;
                return Err(format!("Cannot start project export: {error}"));
            }
        }
        Ok(())
    }
    pub(crate) fn shutdown(&self) -> Result<()> {
        let worker = {
            let mut owner = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            owner.closed = true;
            owner.job.as_mut().and_then(|job| {
                job.state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .cancel(&job.cancel);
                job.worker.take()
            })
        };
        if let Some(worker) = worker {
            worker
                .join()
                .map_err(|_| "Project import worker failed during shutdown")?;
        }
        Ok(())
    }
}
impl Drop for Importer {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}
