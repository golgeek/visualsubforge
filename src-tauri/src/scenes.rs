//! One active scene job and one coalesced pending request. No frame pixels cross
//! IPC. The worker owns supervised children and joins off the UI thread.
use crate::analysis_worker::Worker;
use media_analysis::{
    Cancellation, Tools,
    scenes::{Detail, Phase, Update},
};
use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::Manager;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    // Retained only to read projects written by the former keyframe experiment.
    Keyframes,
    Visual,
}

type Result<T> = std::result::Result<T, String>;
#[derive(Clone)]
pub struct Scenes {
    job: Arc<Mutex<Job>>,
    worker: Arc<Mutex<Worker>>,
}
impl Default for Scenes {
    fn default() -> Self {
        Self {
            job: Arc::default(),
            worker: Arc::new(Mutex::new(Worker::new("scene-analysis"))),
        }
    }
}
#[derive(Default)]
struct Job {
    generation: u64,
    session: u64,
    cancellation: Cancellation,
    running: bool,
    error: Option<String>,
    cuts: Vec<f64>,
    mode: Option<Mode>,
    analysis_version: u32,
    scanning_detail: Detail,
    scanning: Option<Mode>,
    identity: Option<crate::project::MediaIdentity>,
    // Last completed cache stays separate from live display results.
    completed: Option<Vec<f64>>,
    partial: bool,
    phase: Option<Phase>,
    progress: f64,
    note: Option<String>,
}
impl Job {
    fn update(&mut self, generation: u64, session: u64, current: u64, update: Update) {
        if self.generation != generation
            || self.session != session
            || current != session
            || !self.running
            || self.cancellation.is_cancelled()
        {
            return;
        }
        self.phase = Some(update.phase);
        self.progress = update.progress;
        self.note = update.note;
        self.partial = true;
        self.mode = Some(Mode::Visual);
        self.cuts = update.cuts;
        if let Some(completed) = &self.completed {
            self.cuts.extend(completed);
            self.cuts.sort_by(f64::total_cmp);
            self.cuts.dedup_by(|a, b| (*a - *b).abs() <= 0.000001);
        }
    }
    fn finish(
        &mut self,
        generation: u64,
        session: u64,
        current: u64,
        result: Result<Vec<f64>>,
    ) -> bool {
        if self.generation != generation
            || self.session != session
            || current != session
            || self.cancellation.is_cancelled()
        {
            return false;
        }
        self.running = false;
        match result {
            Ok(cuts) => {
                self.cuts = cuts;
                self.mode = self.scanning;
                self.analysis_version = self.scanning_detail.analysis_version() + 2;
                self.partial = false;
                self.completed = None;
                self.progress = 1.;
            }
            Err(error) => self.error = Some(error),
        }
        self.scanning = None;
        true
    }
}
#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SceneView {
    mode: Option<Mode>,
    scanning: Option<Mode>,
    #[serde(with = "crate::ipc_number")]
    #[specta(type = crate::ipc_number::SafeU64)]
    generation: u64,
    running: bool,
    error: Option<String>,
    #[serde(serialize_with = "crate::ipc_number::serialize_usize")]
    #[specta(type = crate::ipc_number::SafeU64)]
    total: usize,
    truncated: bool,
    #[serde(serialize_with = "crate::ipc_number::finite::serialize_cuts")]
    #[specta(type = Vec<crate::ipc_number::finite::FiniteF64>)]
    cuts: Vec<f64>,
    previous: Option<f64>,
    next: Option<f64>,
    phase: Option<Phase>,
    #[serde(with = "crate::ipc_number::finite")]
    #[specta(type = crate::ipc_number::finite::FiniteF64)]
    progress: f64,
    partial: bool,
    note: Option<String>,
}
#[derive(Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Saved {
    version: u32,
    mode: Mode,
    #[serde(serialize_with = "crate::ipc_number::finite::serialize_cuts")]
    #[specta(type = Vec<crate::ipc_number::finite::FiniteF64>)]
    cuts: Vec<f64>,
    pub identity: crate::project::MediaIdentity,
}
impl Saved {
    pub fn validate(&self) -> Result<()> {
        if !matches!(self.version, 1..=4) {
            return Err("Unsupported scene analysis version; rescan media".into());
        }
        if self.cuts.len() > 100_000
            || self
                .cuts
                .iter()
                .any(|t| !t.is_finite() || *t < 0. || *t > 14_400.)
            || self.cuts.windows(2).any(|p| p[0] >= p[1])
        {
            return Err("Invalid project scene timestamps".into());
        }
        Ok(())
    }
}
impl Scenes {
    pub fn saved(
        &self,
        session: u64,
        identity: &crate::project::MediaIdentity,
    ) -> Result<Option<Saved>> {
        let job = self.job.lock().map_err(|_| "Scene state unavailable")?;
        let Some(mode @ Mode::Visual) = job.mode else {
            return Ok(None);
        };
        let cuts = if job.partial {
            let Some(cuts) = &job.completed else {
                return Ok(None);
            };
            cuts
        } else {
            &job.cuts
        };
        if session != job.session || job.identity.as_ref() != Some(identity) {
            return Err("Media changed since scene analysis; rescan before saving project".into());
        }
        Ok(Some(Saved {
            version: job.analysis_version,
            mode,
            cuts: cuts.clone(),
            identity: identity.clone(),
        }))
    }
    fn restore(&self, session: u64, saved: Saved) -> Result<u64> {
        saved.validate()?;
        if saved.mode != Mode::Visual {
            return Err("Keyframes are not scene changes; run a visual scan".into());
        }
        let mut worker = self.worker.lock().map_err(|_| "Scene worker unavailable")?;
        if worker.is_closing() {
            return Err("Application is shutting down".into());
        }
        worker.cancel();
        let mut job = self.job.lock().map_err(|_| "Scene state unavailable")?;
        job.cancellation.cancel();
        *job = Job {
            generation: job.generation + 1,
            session,
            mode: Some(saved.mode),
            analysis_version: saved.version,
            cuts: saved.cuts,
            identity: Some(saved.identity),
            ..Job::default()
        };
        Ok(job.generation)
    }
    pub fn cancel(&self) {
        if let Ok(mut worker) = self.worker.lock() {
            worker.cancel();
            self.invalidate();
        }
    }
    fn invalidate(&self) {
        if let Ok(mut job) = self.job.lock() {
            job.cancellation.cancel();
            job.generation += 1;
            job.running = false;
            job.error = None;
            job.cuts.clear();
            job.mode = None;
            job.scanning = None;
            job.identity = None;
            job.completed = None;
            job.partial = false;
            job.phase = None;
            job.progress = 0.;
            job.note = None;
        }
    }
    pub(crate) fn pause_for_resync(&self) -> Result<()> {
        let mut worker = self.worker.lock().map_err(|_| "Scene worker unavailable")?;
        worker.cancel();
        {
            let mut job = self.job.lock().map_err(|_| "Scene state unavailable")?;
            job.cancellation.cancel();
            if job.running {
                job.running = false;
                job.scanning = None;
                job.error = None;
                job.note = Some("Scene scanning paused for resynchronization".into());
            }
            // Keep the generation: existing frontend polls must observe this
            // stop without a separate scene-command acknowledgment. Cancelled
            // token/running checks reject callbacks from the drained worker.
        }
        worker.cancel_and_wait()
    }
    fn stop(&self) -> Result<u64> {
        let mut worker = self.worker.lock().map_err(|_| "Scene worker unavailable")?;
        worker.cancel();
        let mut job = self.job.lock().map_err(|_| "Scene state unavailable")?;
        job.cancellation.cancel();
        job.generation += 1;
        job.running = false;
        job.scanning = None;
        job.error = None;
        Ok(job.generation)
    }
    pub fn shutdown(&self) {
        if let Ok(mut worker) = self.worker.lock() {
            self.invalidate();
            worker.shutdown();
        }
    }
    fn start(
        &self,
        app: tauri::AppHandle,
        path: String,
        session: u64,
        mode: Mode,
        detail: Detail,
    ) -> Result<u64> {
        if mode != Mode::Visual {
            return Err(
                "Keyframe scene detection is no longer supported; run a visual scan".into(),
            );
        }
        let mut worker = self.worker.lock().map_err(|_| "Scene worker unavailable")?;
        if worker.is_closing() {
            return Err("Application is shutting down".into());
        }
        if app.state::<crate::media::Media>().current_session() != session {
            return Err("Media changed before scene detection started".into());
        }
        let (generation, cancellation) = {
            let mut job = self.job.lock().map_err(|_| "Scene state unavailable")?;
            job.cancellation.cancel();
            let generation = job.generation + 1;
            let identity = if job.session == session {
                job.identity.clone()
            } else {
                None
            };
            let completed = if job.session == session {
                if job.partial {
                    job.completed.clone()
                } else if job.mode == Some(Mode::Visual) {
                    Some(job.cuts.clone())
                } else {
                    None
                }
            } else {
                None
            };
            let (cuts, previous_mode, analysis_version) = if job.session == session {
                (
                    std::mem::take(&mut job.cuts),
                    job.mode,
                    job.analysis_version,
                )
            } else {
                (Vec::new(), None, 0)
            };
            *job = Job {
                cuts,
                identity,
                mode: previous_mode,
                analysis_version,
                completed,
                partial: true,
                phase: Some(Phase::Candidates),
                scanning: Some(mode),
                scanning_detail: detail,
                generation,
                session,
                running: true,
                ..Job::default()
            };
            (generation, job.cancellation.clone())
        };
        let state = self.clone();
        worker
            .replace(cancellation.clone(), move || {
                let source_identity = crate::project::identify(Path::new(&path));
                let result = source_identity
                    .as_ref()
                    .map_err(Clone::clone)
                    .and_then(|identity| {
                        media_analysis::scenes::scan(
                            &Tools::discover().map_err(|error| error.to_string())?,
                            Path::new(&path),
                            &cancellation,
                            Duration::from_secs(1800),
                            detail,
                            |update| {
                                if let Ok(mut job) = state.job.lock() {
                                    job.update(
                                        generation,
                                        session,
                                        app.state::<crate::media::Media>().current_session(),
                                        update,
                                    );
                                }
                            },
                        )
                        .map(|data| data.seconds)
                        .map_err(|e| e.to_string())
                        .and_then(|cuts| {
                            if crate::project::identify(Path::new(&path))? != *identity {
                                return Err("Media changed during scene analysis".into());
                            }
                            Ok(cuts)
                        })
                    });
                if let Ok(mut job) = state.job.lock() {
                    let succeeded = result.is_ok();
                    if job.finish(
                        generation,
                        session,
                        app.state::<crate::media::Media>().current_session(),
                        result,
                    ) && succeeded
                    {
                        job.identity = source_identity.ok();
                    }
                }
            })
            .inspect_err(|e| {
                if let Ok(mut job) = self.job.lock() {
                    job.running = false;
                    job.error = Some(e.clone());
                }
            })?;
        Ok(generation)
    }
    fn view(&self, generation: u64, start: f64, end: f64, position: f64) -> Result<SceneView> {
        if ![start, end, position].iter().all(|n| n.is_finite()) || start < 0.0 || end <= start {
            return Err("Invalid scene viewport".into());
        }
        let job = self.job.lock().map_err(|_| "Scene state unavailable")?;
        if generation != job.generation {
            return Err("Scene analysis was replaced".into());
        }
        let cuts = if job.mode == Some(Mode::Visual) {
            job.cuts.as_slice()
        } else {
            &[]
        };
        let first = cuts.partition_point(|t| *t < start);
        let last = cuts.partition_point(|t| *t <= end);
        let before = cuts.partition_point(|t| *t < position - 0.001);
        let after = cuts.partition_point(|t| *t <= position + 0.001);
        Ok(SceneView {
            mode: job.mode.filter(|mode| *mode == Mode::Visual),
            scanning: job.scanning,
            generation,
            running: job.running,
            error: job.error.clone(),
            total: cuts.len(),
            truncated: last - first > 4096,
            cuts: cuts[first..last.min(first + 4096)].to_vec(),
            previous: before.checked_sub(1).map(|i| cuts[i]),
            next: cuts.get(after).copied(),
            phase: job.phase,
            progress: job.progress,
            partial: job.partial,
            note: job.note.clone(),
        })
    }
}
#[tauri::command]
#[specta::specta]
pub async fn scenes_start(
    app: tauri::AppHandle,
    path: String,
    session: crate::ipc_number::SafeU64,
    mode: Mode,
    detail: Option<Detail>,
) -> Result<crate::ipc_number::SafeU64> {
    tauri::async_runtime::spawn_blocking(move || {
        let generation = app.state::<crate::resync::Resync>().admit_scene(|| {
            app.state::<Scenes>().inner().clone().start(
                app.clone(),
                path,
                session.get(),
                mode,
                detail.unwrap_or_default(),
            )
        })?;
        Ok(crate::ipc_number::SafeU64::try_from(generation)?)
    })
    .await
    .map_err(|error| error.to_string())?
}
#[tauri::command]
#[specta::specta]
pub async fn scenes_cancel(
    app: tauri::AppHandle,
    session: crate::ipc_number::SafeU64,
) -> Result<crate::ipc_number::SafeU64> {
    if app.state::<crate::media::Media>().current_session() != session.get() {
        return Err("Media changed".into());
    }
    Ok(crate::ipc_number::SafeU64::try_from(
        app.state::<Scenes>().stop()?,
    )?)
}
#[tauri::command]
#[specta::specta]
pub async fn scenes_view(
    state: tauri::State<'_, Scenes>,
    generation: crate::ipc_number::SafeU64,
    start: crate::ipc_number::finite::FiniteF64,
    end: crate::ipc_number::finite::FiniteF64,
    position: crate::ipc_number::finite::FiniteF64,
) -> Result<SceneView> {
    let start = start.get();
    let end = end.get();
    let position = position.get();
    state.view(generation.get(), start, end, position)
}
#[tauri::command]
#[specta::specta]
pub async fn scenes_restore(
    app: tauri::AppHandle,
    path: String,
    session: crate::ipc_number::SafeU64,
    saved: Saved,
) -> Result<crate::ipc_number::SafeU64> {
    let session = session.get();
    tauri::async_runtime::spawn_blocking(move || {
        saved.validate()?;
        if crate::project::identify(Path::new(&path))? != saved.identity {
            return Err("Media changed before restoring scene results".into());
        }
        if app.state::<crate::media::Media>().current_session() != session {
            return Err("Media changed".into());
        }
        Ok(crate::ipc_number::SafeU64::try_from(
            app.state::<Scenes>().restore(session, saved)?,
        )?)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stale_or_cancelled_results_are_never_published() {
        let mut job = Job {
            generation: 2,
            session: 3,
            running: true,
            ..Job::default()
        };
        assert!(!job.finish(1, 3, 3, Ok(vec![1.])));
        assert!(!job.finish(2, 3, 4, Ok(vec![1.])));
        assert!(job.finish(2, 3, 3, Ok(vec![2.])));
        job.cancellation.cancel();
        assert!(!job.finish(2, 3, 3, Ok(vec![3.])));
        assert_eq!(job.cuts, vec![2.]);
    }
    #[test]
    fn view_bounds_and_navigation_are_independent_of_visible_window() {
        let scenes = Scenes::default();
        scenes.job.lock().unwrap().cuts = vec![1., 2., 3., 9.];
        scenes.job.lock().unwrap().mode = Some(Mode::Visual);
        let view = scenes.view(0, 1.5, 3., 3.).unwrap();
        assert_eq!(view.cuts, vec![2., 3.]);
        assert_eq!(view.previous, Some(2.));
        assert_eq!(view.next, Some(9.));
        scenes.cancel();
        assert!(scenes.view(0, 0., 10., 0.).is_err());
        assert_eq!(scenes.view(1, 0., 10., 0.).unwrap().total, 0);
        assert!(scenes.view(1, 0., f64::NAN, 0.).is_err());
        scenes.job.lock().unwrap().mode = Some(Mode::Visual);
        scenes.job.lock().unwrap().cuts = (0..5000).map(|n| n as f64).collect();
        let dense = scenes.view(1, 0., 5000., 4998.).unwrap();
        assert_eq!(dense.cuts.len(), 4096);
        assert!(dense.truncated);
        assert_eq!(dense.total, 5000);
        assert_eq!(dense.next, Some(4999.));
    }
    #[test]
    fn resync_pause_keeps_poll_identity_and_saved_cache_but_rejects_late_updates() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("clip");
        std::fs::write(&path, b"fixture").unwrap();
        let identity = crate::project::identify(&path).unwrap();
        let scenes = Scenes::default();
        let generation = scenes
            .restore(
                7,
                Saved {
                    version: 3,
                    mode: Mode::Visual,
                    cuts: vec![1., 2.],
                    identity: identity.clone(),
                },
            )
            .unwrap();
        {
            let mut job = scenes.job.lock().unwrap();
            job.running = true;
            job.partial = true;
            job.scanning = Some(Mode::Visual);
            job.completed = Some(job.cuts.clone());
            job.cuts.push(3.);
        }
        scenes.pause_for_resync().unwrap();
        let view = scenes.view(generation, 0., 4., 0.).unwrap();
        assert!(!view.running && view.partial);
        assert_eq!(view.cuts, vec![1., 2., 3.]);
        assert_eq!(
            scenes.saved(7, &identity).unwrap().unwrap().cuts,
            vec![1., 2.]
        );
        assert!(
            !scenes
                .job
                .lock()
                .unwrap()
                .finish(generation, 7, 7, Ok(vec![9.]))
        );
        assert_eq!(
            scenes.view(generation, 0., 4., 0.).unwrap().cuts,
            vec![1., 2., 3.]
        );
        scenes.shutdown();
    }

    #[test]
    fn cancellation_and_failure_keep_last_completed_markers() {
        let scenes = Scenes::default();
        {
            let mut job = scenes.job.lock().unwrap();
            job.cuts = vec![1., 2.];
            job.mode = Some(Mode::Visual);
            job.scanning = Some(Mode::Visual);
            job.running = true;
        }
        let generation = scenes.stop().unwrap();
        let view = scenes.view(generation, 0., 5., 0.).unwrap();
        assert_eq!(view.cuts, vec![1., 2.]);
        assert_eq!(view.mode, Some(Mode::Visual));
        assert!(!view.running);
        let mut job = Job {
            mode: Some(Mode::Visual),
            cuts: vec![1.],
            scanning: Some(Mode::Visual),
            ..Job::default()
        };
        assert!(job.finish(0, 0, 0, Err("failed".into())));
        assert_eq!(job.cuts, vec![1.]);
        assert_eq!(job.mode, Some(Mode::Visual));
    }
    #[test]
    fn restored_generation_rejects_previous_worker_and_wrong_save_session() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("clip");
        std::fs::write(&path, b"fixture").unwrap();
        let identity = crate::project::identify(&path).unwrap();
        let scenes = Scenes::default();
        let token = scenes.job.lock().unwrap().cancellation.clone();
        let (started, started_rx) = std::sync::mpsc::channel();
        let (release, release_rx) = std::sync::mpsc::channel();
        let (finished, finished_rx) = std::sync::mpsc::channel();
        let result_state = scenes.job.clone();
        scenes
            .worker
            .lock()
            .unwrap()
            .replace(token.clone(), move || {
                started.send(()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                let accepted = result_state.lock().unwrap().finish(0, 0, 0, Ok(vec![3.]));
                finished.send(accepted).unwrap();
            })
            .unwrap();
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let generation = scenes
            .restore(
                7,
                Saved {
                    version: 1,
                    mode: Mode::Visual,
                    cuts: vec![1., 2.],
                    identity: identity.clone(),
                },
            )
            .unwrap();
        assert!(token.is_cancelled());
        release.send(()).unwrap();
        assert!(!finished_rx.recv_timeout(Duration::from_secs(5)).unwrap());
        assert!(!scenes.job.lock().unwrap().finish(0, 7, 7, Ok(vec![3.])));
        assert_eq!(
            scenes.view(generation, 0., 4., 0.).unwrap().cuts,
            vec![1., 2.]
        );
        assert!(scenes.saved(6, &identity).is_err());
        assert_eq!(
            scenes.saved(7, &identity).unwrap().unwrap().mode,
            Mode::Visual
        );
        scenes.shutdown();
    }
    #[test]
    fn visual_cache_version_tracks_completed_analysis_not_rescan_attempts() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("clip");
        std::fs::write(&path, b"fixture").unwrap();
        let identity = crate::project::identify(&path).unwrap();
        let scenes = Scenes::default();
        let generation = scenes
            .restore(
                7,
                Saved {
                    version: 1,
                    mode: Mode::Visual,
                    cuts: vec![1.],
                    identity: identity.clone(),
                },
            )
            .unwrap();
        assert_eq!(scenes.saved(7, &identity).unwrap().unwrap().version, 1);
        {
            let mut job = scenes.job.lock().unwrap();
            job.scanning = Some(Mode::Visual);
            assert!(job.finish(generation, 7, 7, Err("failed".into())));
        }
        assert_eq!(scenes.saved(7, &identity).unwrap().unwrap().version, 1);
        {
            let mut job = scenes.job.lock().unwrap();
            job.scanning = Some(Mode::Visual);
            job.scanning_detail = Detail::Reduced;
            assert!(job.finish(generation, 7, 7, Ok(vec![2.])));
        }
        let saved = scenes.saved(7, &identity).unwrap().unwrap();
        assert_eq!(saved.version, Detail::Reduced.analysis_version() + 2);
        let mut future = saved.clone();
        future.version = 999;
        assert!(future.validate().is_err());
        let restored = Scenes::default();
        restored.restore(8, saved).unwrap();
        assert_eq!(restored.saved(8, &identity).unwrap().unwrap().version, 4);
    }
    #[test]
    fn legacy_keyframes_cannot_be_restored_displayed_or_saved_as_cuts() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("clip");
        std::fs::write(&path, b"fixture").unwrap();
        let identity = crate::project::identify(&path).unwrap();
        let saved: Saved = serde_json::from_value(serde_json::json!({
            "version": 1, "mode": "keyframes", "cuts": [1., 2.], "identity": identity
        }))
        .unwrap();
        saved.validate().unwrap(); // Old projects remain readable.
        let scenes = Scenes::default();
        assert!(scenes.restore(0, saved).is_err());
        {
            let mut job = scenes.job.lock().unwrap();
            job.mode = Some(Mode::Keyframes);
            job.cuts = vec![1., 2.];
            job.identity = Some(identity.clone());
        }
        let view = scenes.view(0, 0., 4., 1.).unwrap();
        assert!(view.cuts.is_empty());
        assert_eq!(view.total, 0);
        assert_eq!(view.mode, None);
        assert_eq!(view.previous, None);
        assert_eq!(view.next, None);
        assert!(scenes.saved(0, &identity).unwrap().is_none());
    }
    #[test]
    fn live_updates_are_guarded_and_partial_cuts_never_replace_saved_cache() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("clip");
        std::fs::write(&path, b"fixture").unwrap();
        let identity = crate::project::identify(&path).unwrap();
        let scenes = Scenes::default();
        let generation = scenes
            .restore(
                7,
                Saved {
                    version: 1,
                    mode: Mode::Visual,
                    cuts: vec![1.],
                    identity: identity.clone(),
                },
            )
            .unwrap();
        let update = || Update {
            phase: Phase::Full,
            progress: 0.4,
            cuts: vec![2.],
            note: None,
        };
        {
            let mut job = scenes.job.lock().unwrap();
            job.completed = Some(job.cuts.clone());
            job.partial = true;
            job.running = true;
            job.scanning = Some(Mode::Visual);
            job.update(generation - 1, 7, 7, update());
            job.update(generation, 7, 8, update());
            assert_eq!(job.cuts, vec![1.]);
            job.update(generation, 7, 7, update());
            assert_eq!(job.cuts, vec![1., 2.]);
        }
        let saved = scenes.saved(7, &identity).unwrap().unwrap();
        assert_eq!(saved.cuts, vec![1.]);
        assert_eq!(saved.version, 1);
        let stopped = scenes.stop().unwrap();
        let view = scenes.view(stopped, 0., 4., 0.).unwrap();
        assert!(view.partial && !view.running);
        assert_eq!(view.progress, 0.4);
        scenes
            .job
            .lock()
            .unwrap()
            .update(generation, 7, 7, update());
        assert_eq!(scenes.saved(7, &identity).unwrap().unwrap().cuts, vec![1.]);
        {
            let mut job = scenes.job.lock().unwrap();
            job.cancellation = Cancellation::default();
            job.scanning = Some(Mode::Visual);
            job.finish(stopped, 7, 7, Ok(vec![2., 3.]));
        }
        let saved = scenes.saved(7, &identity).unwrap().unwrap();
        assert_eq!(saved.version, 3);
        assert_eq!(saved.cuts, vec![2., 3.]);
        scenes.cancel();
        {
            let mut job = scenes.job.lock().unwrap();
            job.mode = Some(Mode::Visual);
            job.partial = true;
            job.cuts = vec![9.];
        }
        assert!(scenes.saved(7, &identity).unwrap().is_none());
    }
    #[test]
    fn shutdown_cancels_and_joins_workers() {
        let scenes = Scenes::default();
        let token = scenes.job.lock().unwrap().cancellation.clone();
        let (started, started_rx) = std::sync::mpsc::channel();
        scenes
            .worker
            .lock()
            .unwrap()
            .replace(token.clone(), move || {
                started.send(()).unwrap();
                while !token.is_cancelled() {
                    std::thread::yield_now();
                }
            })
            .unwrap();
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        scenes.shutdown();
        assert!(scenes.worker.lock().unwrap().is_closing());
    }
}

#[cfg(test)]
pub(crate) fn ipc_sample() -> serde_json::Value {
    let scenes = Scenes::default();
    {
        let mut job = scenes.job.lock().unwrap();
        job.mode = Some(Mode::Visual);
        job.cuts = vec![1.25, 3.5];
        job.phase = Some(Phase::Full);
        job.progress = 0.5;
    }
    serde_json::to_value(scenes.view(0, 0., 10., 2.).unwrap()).unwrap()
}

#[test]
fn ipc_scenes_reject_nonfinite_cuts_and_progress() {
    let scenes = Scenes::default();
    let mut view = scenes.view(0, 0., 10., 2.).unwrap();
    view.cuts = vec![f64::NAN];
    assert!(serde_json::to_value(&view).is_err());
    view.cuts.clear();
    view.progress = f64::INFINITY;
    assert!(serde_json::to_value(&view).is_err());
}
