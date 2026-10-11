//! One owned resync analysis, with no pending queue. Analysis and input rechecks
//! run off the UI thread. Final acceptance locks media selection, this owner,
//! then documents, so replacement/cancellation cannot cross the commit boundary.
use crate::{
    document::{Capture, Documents, Snapshot},
    media::Media,
};
use std::{
    path::PathBuf,
    sync::{Arc, Condvar, Mutex},
    thread::JoinHandle,
    time::Duration,
};
use tauri::Manager;
use vsf_application::resync::{self as application, Analysis, Budget, Cancellation, Stage};

mod view;
pub use view::View;
#[cfg(test)]
pub(crate) use view::ipc_sample;
type Result<T> = std::result::Result<T, String>;
const TIMEOUT: Duration = Duration::from_secs(1800);

#[derive(Default)]
pub struct Resync {
    owner: Mutex<Owner>,
    idle: Condvar,
}
#[derive(Default)]
struct Owner {
    next_id: u64,
    closing: bool,
    accepting: bool,
    job: Option<Job>,
}
struct Job {
    id: u64,
    session: u64,
    capture: Capture,
    cancel: Cancellation,
    stage: Arc<Mutex<Stage>>,
    worker: Option<JoinHandle<Result<Analysis>>>,
    result: Option<Arc<Analysis>>,
    error: Option<String>,
    accepted: bool,
}
impl Job {
    fn refresh(&mut self) {
        if let Some(worker) = self.worker.take_if(|worker| worker.is_finished()) {
            let result = worker
                .join()
                .unwrap_or_else(|_| Err("Resync worker stopped unexpectedly".into()));
            if !self.cancel.is_cancelled() {
                match result {
                    Ok(result) => self.result = Some(Arc::new(result)),
                    Err(error) => self.error = Some(error),
                }
            }
        }
    }
    fn cancel(&mut self) {
        self.cancel.cancel();
        self.result = None;
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

impl Resync {
    fn start(
        &self,
        app: &tauri::AppHandle,
        source: PathBuf,
        session: u64,
        generation: u64,
        revision: u64,
    ) -> Result<u64> {
        let media = app.state::<Media>();
        let status = media.request(crate::media::Action::Status, session)?;
        if !status.loaded || status.session != session {
            return Err("Wait for the target video to load before analysis".into());
        }
        media.with_current(session, |target| {
            let capture = app
                .state::<Documents>()
                .capture_resync(generation, revision)?;
            let request = application::Request {
                source_video: source,
                target_video: target.into(),
                subtitles: application::Subtitles::Snapshot(capture.snapshot.clone()),
            };
            let scenes = app.state::<crate::scenes::Scenes>().inner().clone();
            self.launch(session, capture, move |token, progress| {
                // Drain cancelled scene work before starting any resync child.
                // Waveform keeps its one supervised child; native playback
                // remains independent. Resync itself decodes sequentially.
                let budget = Budget::new(&token, TIMEOUT);
                scenes.pause_for_resync()?;
                let remaining = budget.remaining().map_err(|error| error.to_string())?;
                application::analyze(
                    &media_analysis::Tools::discover(),
                    request,
                    &token,
                    remaining,
                    |next| {
                        if let Ok(mut current) = progress.lock() {
                            *current = next;
                        }
                    },
                )
                .map_err(|error| error.to_string())
            })
        })
    }

    // This owner admits one worker with no queue. Keeping admission separate
    // from Tauri composition also lets lifecycle tests control worker completion.
    fn launch(
        &self,
        session: u64,
        capture: Capture,
        run: impl FnOnce(Cancellation, Arc<Mutex<Stage>>) -> Result<Analysis> + Send + 'static,
    ) -> Result<u64> {
        let mut owner = self.owner.lock().map_err(|_| "Resync state unavailable")?;
        if let Some(job) = &mut owner.job {
            job.refresh();
        }
        if owner.closing
            || owner.accepting
            || owner.job.as_ref().is_some_and(|job| job.worker.is_some())
        {
            return Err("Wait for the current resync operation to finish cancelling".into());
        }
        let id = owner
            .next_id
            .checked_add(1)
            .filter(|id| *id <= crate::ipc_number::MAX_SAFE_INTEGER)
            .ok_or("Resync identities exhausted")?;
        let cancel = Cancellation::default();
        let stage = Arc::new(Mutex::new(Stage::ReadInputs));
        let token = cancel.clone();
        let progress = stage.clone();
        let worker = std::thread::Builder::new()
            .name("subtitle-resync".into())
            .spawn(move || run(token, progress))
            .map_err(|error| format!("Cannot start resync worker: {error}"))?;
        owner.next_id = id;
        owner.job = Some(Job {
            id,
            session,
            capture,
            cancel,
            stage,
            worker: Some(worker),
            result: None,
            error: None,
            accepted: false,
        });
        Ok(id)
    }

    // Scene admission holds the owner only while scheduling. A resync start
    // cannot race a scene scan past this check; no job/worker lock is reversed.
    pub(crate) fn admit_scene<T>(&self, start: impl FnOnce() -> Result<T>) -> Result<T> {
        let mut owner = self.owner.lock().map_err(|_| "Resync state unavailable")?;
        if let Some(job) = &mut owner.job {
            job.refresh();
        }
        if owner.closing
            || owner.accepting
            || owner.job.as_ref().is_some_and(|job| job.worker.is_some())
        {
            return Err("Scene scanning is paused during resynchronization".into());
        }
        start()
    }

    pub(crate) fn cancel_current(&self) {
        if let Ok(mut owner) = self.owner.lock()
            && let Some(job) = &mut owner.job
        {
            job.cancel();
        }
    }
    fn cancel(&self, id: u64) -> Result<()> {
        let mut owner = self.owner.lock().map_err(|_| "Resync state unavailable")?;
        let job = owner
            .job
            .as_mut()
            .filter(|job| job.id == id)
            .ok_or("Resync job was replaced")?;
        job.cancel();
        Ok(())
    }

    fn accept(&self, id: u64, media: &Media, documents: &Documents) -> Result<Snapshot> {
        let (analysis, capture, cancel, session) = {
            let mut owner = self.owner.lock().map_err(|_| "Resync state unavailable")?;
            if owner.closing || owner.accepting {
                return Err("Resync acceptance is unavailable".into());
            }
            let job = owner
                .job
                .as_mut()
                .filter(|job| job.id == id)
                .ok_or("Resync job was replaced")?;
            job.refresh();
            let analysis = job
                .result
                .clone()
                .filter(|result| result.proposal().is_complete())
                .ok_or("Only a complete proposal can be accepted")?;
            let state = (
                analysis,
                job.capture.clone(),
                job.cancel.clone(),
                job.session,
            );
            owner.accepting = true;
            state
        };
        // Cancellation remains available while slow media identities are read.
        // Shutdown waits for this guard as well as the analysis thread.
        let _accepting = Accepting(self);
        analysis
            .verify_inputs(&Budget::new(&cancel, TIMEOUT))
            .map_err(|error| error.to_string())?;
        media.with_current(session, |_| {
            let mut owner = self.owner.lock().map_err(|_| "Resync state unavailable")?;
            if owner.closing {
                return Err("Application is shutting down".into());
            }
            let job = owner
                .job
                .as_mut()
                .filter(|job| job.id == id && !job.cancel.is_cancelled())
                .ok_or("Resync proposal was cancelled or replaced")?;
            let snapshot = documents.accept_resync(&capture, analysis.proposal())?;
            job.result = None;
            job.accepted = true;
            Ok(snapshot)
        })
    }

    pub(crate) fn shutdown(&self) {
        let worker = {
            let Ok(mut owner) = self.owner.lock() else {
                return;
            };
            owner.closing = true;
            if let Some(job) = &mut owner.job {
                job.cancel();
            }
            while owner.accepting {
                let Ok(next) = self.idle.wait(owner) else {
                    return;
                };
                owner = next;
            }
            owner.job.as_mut().and_then(|job| job.worker.take())
        };
        if let Some(worker) = worker {
            let _ = worker.join();
        }
    }
}
struct Accepting<'a>(&'a Resync);
impl Drop for Accepting<'_> {
    fn drop(&mut self) {
        if let Ok(mut owner) = self.0.owner.lock() {
            owner.accepting = false;
        }
        self.0.idle.notify_all();
    }
}

#[tauri::command]
#[specta::specta]
pub async fn resync_start(
    app: tauri::AppHandle,
    source: String,
    session: crate::ipc_number::SafeU64,
    generation: crate::ipc_number::SafeU64,
    revision: crate::ipc_number::SafeU64,
) -> Result<crate::ipc_number::SafeU64> {
    tauri::async_runtime::spawn_blocking(move || {
        let id = app.state::<Resync>().start(
            &app,
            source.into(),
            session.get(),
            generation.get(),
            revision.get(),
        )?;
        crate::ipc_number::SafeU64::try_from(id).map_err(str::to_owned)
    })
    .await
    .map_err(|error| error.to_string())?
}
#[tauri::command]
#[specta::specta]
pub async fn resync_cancel(app: tauri::AppHandle, id: crate::ipc_number::SafeU64) -> Result<()> {
    tauri::async_runtime::spawn_blocking(move || app.state::<Resync>().cancel(id.get()))
        .await
        .map_err(|error| error.to_string())?
}
#[tauri::command]
#[specta::specta]
pub async fn resync_accept(
    app: tauri::AppHandle,
    id: crate::ipc_number::SafeU64,
) -> Result<Snapshot> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Resync>()
            .accept(id.get(), &app.state::<Media>(), &app.state::<Documents>())
    })
    .await
    .map_err(|error| error.to_string())?
}
#[tauri::command]
#[specta::specta]
pub async fn resync_view(
    app: tauri::AppHandle,
    id: crate::ipc_number::SafeU64,
    cue_offset: u32,
    section_offset: u32,
) -> Result<View> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Resync>().view(
            id.get(),
            cue_offset,
            section_offset,
            &app.state::<Media>(),
            &app.state::<Documents>(),
        )
    })
    .await
    .map_err(|error| error.to_string())?
}

#[cfg(test)]
mod tests;
