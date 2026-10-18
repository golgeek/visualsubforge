//! One plugin job/no queue. Worker and acceptance IO never retain or lock Store.
use super::*;
use crate::{
    plugin_review as view,
    quality_check::Key,
    scenes::{SceneCapture, SceneKey},
};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::JoinHandle,
    time::Duration,
};
use vsf_application::{
    plugin_files::{Budget, LoadedPackage},
    plugins as engine,
    quality::scene::{SceneCoverage, Scenes},
};

const OPERATION_TIMEOUT: Duration = Duration::from_secs(10);
const PREVIEW_BYTES: usize = 2048;
#[derive(Default)]
pub(super) struct State {
    next_id: u64,
    closed: bool,
    job: Option<Job>,
}
struct Prepared {
    package: LoadedPackage,
    proposal: engine::Proposal,
    request: engine::Request,
}
struct Job {
    id: u64,
    key: Key,
    scenes: SceneKey,
    path: String,
    input: view::Input,
    request: engine::Request,
    coverage: SceneCoverage,
    cancel: Arc<AtomicBool>,
    worker: Option<JoinHandle<Result<Prepared>>>,
    prepared: Option<Arc<Prepared>>,
    error: Option<String>,
    stale: bool,
    accepted: bool,
    accepting: Option<mpsc::Receiver<()>>,
}
impl Job {
    fn cancel(&mut self) {
        self.cancel.store(true, Ordering::Release);
        self.prepared = None;
    }
    fn refresh(&mut self) {
        if self
            .accepting
            .as_ref()
            .is_some_and(|r| matches!(r.try_recv(), Err(mpsc::TryRecvError::Disconnected)))
        {
            self.accepting = None;
        }
        if let Some(worker) = self.worker.take_if(|w| w.is_finished()) {
            let result = worker
                .join()
                .unwrap_or_else(|_| Err("Plugin worker stopped unexpectedly".into()));
            if !self.cancel.load(Ordering::Acquire) {
                match result {
                    Ok(prepared) => self.prepared = Some(Arc::new(prepared)),
                    Err(e) => self.error = Some(e),
                }
            }
        }
    }
    fn phase(&self, closed: bool) -> view::Phase {
        use view::Phase;
        if self.accepted {
            Phase::Accepted
        } else if closed {
            Phase::Closed
        } else if self.stale {
            Phase::Stale
        } else if self.cancel.load(Ordering::Acquire) {
            if self.worker.is_some() || self.accepting.is_some() {
                Phase::Cancelling
            } else {
                Phase::Cancelled
            }
        } else if self.accepting.is_some() {
            Phase::Accepting
        } else if self.error.is_some() {
            Phase::Failed
        } else if self.prepared.is_some() {
            Phase::Reviewing
        } else {
            Phase::Preparing
        }
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
        // Fallback on final owner destruction. Workers cannot own Store or join
        // themselves; normal shutdown takes this handle and joins off-lock.
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
    fn idle(&mut self) -> Result<()> {
        if let Some(job) = &mut self.job {
            job.refresh();
        }
        if self.closed {
            return Err("Plugin host is shutting down".into());
        }
        if self
            .job
            .as_ref()
            .is_some_and(|j| j.worker.is_some() || j.accepting.is_some())
        {
            return Err("Wait for the current plugin operation to finish or cancel".into());
        }
        Ok(())
    }
}

/// Its sender is a lifetime signal, not a message: shutdown waits for Drop after
/// verification/commit returns. No document owner is needed in this reservation.
pub(crate) struct Acceptance {
    id: u64,
    key: Key,
    scenes: SceneKey,
    prepared: Arc<Prepared>,
    cancel: Arc<AtomicBool>,
    _completion: mpsc::Sender<()>,
}
pub(crate) struct VerifiedAcceptance {
    pending: Acceptance,
    deadline: std::time::Instant,
}
impl Acceptance {
    pub(crate) fn verify(self) -> Result<VerifiedAcceptance> {
        let deadline = std::time::Instant::now() + OPERATION_TIMEOUT;
        self.prepared
            .package
            .verify(&Budget::new(&self.cancel, OPERATION_TIMEOUT))
            .map_err(|e| e.to_string())?;
        Ok(VerifiedAcceptance {
            pending: self,
            deadline,
        })
    }
}
impl VerifiedAcceptance {
    pub(crate) fn scene_key(&self) -> SceneKey {
        self.pending.scenes
    }
}
fn ids(values: &[String]) -> Result<Vec<CueId>> {
    if values.len() > engine::MAX_CUES {
        return Err("Plugin scope/selection exceeds 100000 cues".into());
    }
    values
        .iter()
        .map(|s| {
            if s.len() > 20 || s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
                return Err("Plugin cue IDs must be decimal integers".into());
            }
            s.parse::<u64>()
                .map(CueId)
                .map_err(|_| "Invalid plugin cue identity".into())
        })
        .collect()
}
fn request(
    store: &Store,
    key: Key,
    input: &view::Input,
    scenes: &Scenes,
) -> Result<engine::Request> {
    if input.settings.len() > 64
        || input
            .settings
            .iter()
            .any(|(k, v)| k.len() > 128 || matches!(v,view::Scalar::Text(t) if t.len()>4096))
    {
        return Err("Plugin settings exceed their supported bounds".into());
    }
    let profile = store.quality.context().profile;
    let cps = input
        .cps_target
        .map(|v| v.get())
        .or(profile.rules.speed.maximum_cps.map(f64::from))
        .ok_or("The current profile has no CPS target; supply an explicit plugin CPS target")?;
    let preferences = engine::Preferences {
        minimum_blank_ms: u64::from(profile.timing.cue_gap_ms),
        minimum_duration_ms: u64::from(profile.rules.duration.minimum_ms),
        maximum_duration_ms: u64::from(profile.rules.duration.maximum_ms),
        cps_target: cps,
        rs_target_index: input.rs_target_index,
    };
    let cuts = match scenes {
        Scenes::Complete(c) | Scenes::Reduced(c) => Some(c),
        _ => None,
    };
    let scenes = cuts.map(|c| engine::Scenes {
        cuts_ms: c.iter().map(|t| t.get()).collect(),
        start_offset_ms: u64::from(profile.timing.scene_before_ms),
        stop_offset_ms: u64::from(profile.timing.scene_after_ms),
        filter_offset_ms: u64::from(input.scene_filter_offset_ms),
    });
    let document = store
        .loaded
        .as_ref()
        .ok_or("Open subtitles first")?
        .editor
        .document();
    Ok(engine::Request {
        key: engine::Key {
            generation: key.generation.get(),
            revision: key.revision.get(),
            context: key.epoch.get(),
        },
        mode: match input.mode {
            view::Mode::Check => engine::Mode::Check,
            view::Mode::Fix => engine::Mode::Fix,
        },
        scope: if input.scope.is_empty() {
            document.cues().iter().map(|c| c.id()).collect()
        } else {
            ids(&input.scope)?
        },
        context: engine::Context {
            selection: ids(&input.selection)?,
            cursor: input
                .cursor_ms
                .map(|v| Milliseconds::new(v.get()))
                .transpose()?,
            preferences,
            scenes,
        },
        settings: input
            .settings
            .iter()
            .map(|(k, v)| {
                Ok((
                    k.clone(),
                    serde_json::to_value(v).map_err(|e| e.to_string())?,
                ))
            })
            .collect::<Result<_>>()?,
    })
}
fn execute(
    execution: engine::Execution,
    cancel: &AtomicBool,
    remaining: Duration,
) -> Result<serde_json::Value> {
    let desktop = std::env::current_exe().map_err(|e| e.to_string())?;
    execute_from(&desktop, execution, cancel, remaining)
}
fn runner_path(desktop: &Path) -> Result<PathBuf> {
    // Every maintained package already installs vsf beside the desktop binary.
    // Never search PATH or silently run an unrelated installation's executable.
    let runner = desktop
        .parent()
        .ok_or("Cannot locate the application directory")?
        .join(format!("vsf{}", std::env::consts::EXE_SUFFIX));
    let metadata=std::fs::symlink_metadata(&runner).map_err(|_|format!("Plugin runner is missing beside VisualSubForge ({}). Reinstall the complete application; development builds must also build vsf-cli for the same target and profile.",runner.display()))?;
    if !metadata.is_file() || metadata.is_symlink() {
        return Err("The installed plugin runner must be a regular executable file".into());
    }
    Ok(runner)
}
fn execute_from(
    desktop: &Path,
    execution: engine::Execution,
    cancel: &AtomicBool,
    remaining: Duration,
) -> Result<serde_json::Value> {
    let worker = runner_path(desktop)?;
    let mut request = vsf_plugin_host::Request::new(execution.script, execution.input);
    request.limits.timeout_ms =
        1000.min(remaining.as_millis().clamp(1, u128::from(u32::MAX)) as u32);
    vsf_plugin_host::run(
        &worker,
        &[OsString::from(vsf_plugin_host::WORKER_ARGUMENT)],
        &request,
        cancel,
    )
    .map_err(|e| e.to_string())
}
impl Documents {
    pub(crate) fn plugin_start(
        &self,
        generation: u64,
        revision: u64,
        scenes: SceneCapture,
        path: String,
        input: view::Input,
    ) -> Result<u64> {
        self.plugin_start_with(generation, revision, scenes, path, input, execute)
    }
    // Internal executor seam keeps owner race tests deterministic; production
    // always uses the bounded helper above and exposes no executor through IPC.
    fn plugin_start_with(
        &self,
        generation: u64,
        revision: u64,
        scenes: SceneCapture,
        path: String,
        input: view::Input,
        execute: impl FnOnce(engine::Execution, &AtomicBool, Duration) -> Result<serde_json::Value>
        + Send
        + 'static,
    ) -> Result<u64> {
        if path.is_empty() || path.len() > 32768 {
            return Err("Choose a plugin manifest with a supported path length".into());
        }
        let mut store = self.0.lock().map_err(|_| "Document state unavailable")?;
        let key = store.quality_key()?;
        if !key.loaded || key.generation.get() != generation || key.revision.get() != revision {
            return Err("Subtitles changed before plugin capture; run again".into());
        }
        store.plugins.idle()?;
        let request = request(&store, key, &input, &scenes.data)?;
        let coverage = match scenes.data {
            Scenes::NoMedia => SceneCoverage::NoMedia,
            Scenes::Unavailable => SceneCoverage::Unavailable,
            Scenes::Partial => SceneCoverage::Partial,
            Scenes::Reduced(_) => SceneCoverage::Reduced,
            Scenes::Complete(_) => SceneCoverage::Checked,
        };
        let loaded = store.loaded.as_ref().ok_or("Open subtitles first")?;
        let document = loaded.editor.document().clone();
        let source = loaded.source().clone();
        let id = store
            .plugins
            .next_id
            .checked_add(1)
            .filter(|i| *i <= crate::ipc_number::MAX_SAFE_INTEGER)
            .ok_or("Plugin job identities exhausted")?;
        let cancel = Arc::new(AtomicBool::new(false));
        let token = cancel.clone();
        let captured_request = request.clone();
        let selected = path.clone();
        let worker = std::thread::Builder::new()
            .name("subtitle-plugin".into())
            .spawn(move || {
                let budget = Budget::new(&token, OPERATION_TIMEOUT);
                let check = || budget.check().map_err(|e| e.to_string());
                let package = LoadedPackage::read(Path::new(&selected), &budget)
                    .map_err(|e| e.to_string())?;
                let capture = engine::Capture::new(
                    document,
                    source,
                    package.package().clone(),
                    captured_request.clone(),
                    check,
                )?;
                let execution = capture.execution(check)?;
                let value = execute(
                    execution,
                    &token,
                    budget.remaining().map_err(|e| e.to_string())?,
                )?;
                check()?;
                let proposal = capture.validate(value, check)?;
                package.verify(&budget).map_err(|e| e.to_string())?;
                Ok(Prepared {
                    package,
                    proposal,
                    request: captured_request,
                })
            })
            .map_err(|e| format!("Cannot start plugin worker: {e}"))?;
        store.plugins.job = Some(Job {
            id,
            key,
            scenes: scenes.key,
            path,
            input,
            request,
            coverage,
            cancel,
            worker: Some(worker),
            prepared: None,
            error: None,
            stale: false,
            accepted: false,
            accepting: None,
        });
        store.plugins.next_id = id;
        Ok(id)
    }
    pub(crate) fn plugin_context(&self, id: u64) -> Result<SceneKey> {
        let store = self.0.lock().map_err(|_| "Document state unavailable")?;
        Ok(store
            .plugins
            .job
            .as_ref()
            .filter(|j| j.id == id)
            .ok_or("Plugin job was replaced")?
            .scenes)
    }
    pub(crate) fn plugin_cancel(&self, id: u64) -> Result<()> {
        let mut store = self.0.lock().map_err(|_| "Document state unavailable")?;
        if let Some(job) = store.plugins.job.as_mut().filter(|j| j.id == id) {
            job.cancel();
        }
        Ok(())
    }
    pub(crate) fn plugin_invalidate(&self, id: u64) -> Result<()> {
        let mut store = self.0.lock().map_err(|_| "Document state unavailable")?;
        if let Some(job) = store
            .plugins
            .job
            .as_mut()
            .filter(|j| j.id == id && !j.accepted)
        {
            job.stale = true;
            job.cancel();
        }
        Ok(())
    }
    pub(crate) fn plugin_view(
        &self,
        id: u64,
        findings_offset: u32,
        changes_offset: u32,
        limit: u32,
    ) -> Result<view::View> {
        if !(1..=200).contains(&limit) {
            return Err("Plugin pages require 1–200 rows".into());
        }
        let mut store = self.0.lock().map_err(|_| "Document state unavailable")?;
        let key = store.quality_key()?;
        store.plugins.invalidate(key);
        let closed = store.plugins.closed;
        let job = store
            .plugins
            .job
            .as_mut()
            .filter(|j| j.id == id)
            .ok_or("Plugin job was replaced")?;
        job.refresh();
        project(job, closed, findings_offset, changes_offset, limit)
    }
    pub(crate) fn plugin_text(
        &self,
        id: u64,
        row: u32,
        after: bool,
        offset: u32,
        limit: u32,
    ) -> Result<view::TextChunk> {
        if !(4..=65536).contains(&limit) {
            return Err("Plugin text chunks require 4–65536 bytes".into());
        }
        let mut store = self.0.lock().map_err(|_| "Document state unavailable")?;
        let key = store.quality_key()?;
        store.plugins.invalidate(key);
        let job = store
            .plugins
            .job
            .as_mut()
            .filter(|j| j.id == id)
            .ok_or("Plugin job was replaced")?;
        job.refresh();
        let row = job
            .prepared
            .as_ref()
            .and_then(|p| p.proposal.rows().get(row as usize))
            .ok_or("Plugin change is no longer available")?;
        let text = if after {
            &row.after.text
        } else {
            &row.before.text
        };
        let start = offset as usize;
        if start > text.len() || !text.is_char_boundary(start) {
            return Err("Plugin text offset must be a UTF-8 boundary".into());
        }
        let mut end = (start + limit as usize).min(text.len());
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        Ok(view::TextChunk {
            offset,
            next_offset: end as u32,
            total_bytes: text.len() as u32,
            text: text[start..end].into(),
        })
    }
    pub(crate) fn plugin_begin_accept(&self, id: u64, input: &view::Input) -> Result<Acceptance> {
        let mut store = self.0.lock().map_err(|_| "Document state unavailable")?;
        let key = store.quality_key()?;
        store.plugins.invalidate(key);
        if store.plugins.closed {
            return Err("Plugin host is shutting down".into());
        }
        let job = store
            .plugins
            .job
            .as_mut()
            .filter(|j| j.id == id)
            .ok_or("Plugin job was replaced")?;
        job.refresh();
        if job.phase(false) != view::Phase::Reviewing
            || job.input != *input
            || input.mode != view::Mode::Fix
        {
            return Err("Plugin context or review changed; create a current fix preview".into());
        }
        let prepared = job
            .prepared
            .clone()
            .ok_or("Wait for a completed plugin preview")?;
        if prepared.proposal.rows().is_empty() {
            return Err("Plugin proposed no changes".into());
        }
        let (completion, done) = mpsc::channel();
        job.accepting = Some(done);
        Ok(Acceptance {
            id,
            key,
            scenes: job.scenes,
            prepared,
            cancel: job.cancel.clone(),
            _completion: completion,
        })
    }
    pub(crate) fn plugin_commit(&self, verified: &VerifiedAcceptance) -> Result<Snapshot> {
        if std::time::Instant::now() >= verified.deadline {
            return Err("Plugin acceptance exceeded ten seconds; retry the review".into());
        }
        let pending = &verified.pending;
        let mut store = self.0.lock().map_err(|_| "Document state unavailable")?;
        if std::time::Instant::now() >= verified.deadline {
            return Err("Plugin acceptance exceeded ten seconds; retry the review".into());
        }
        let key = store.quality_key()?;
        store.plugins.invalidate(key);
        let job = store
            .plugins
            .job
            .as_ref()
            .filter(|j| j.id == pending.id)
            .ok_or("Plugin job was replaced")?;
        if store.plugins.closed
            || key != pending.key
            || job.stale
            || job.accepted
            || job.accepting.is_none()
            || pending.cancel.load(Ordering::Acquire)
        {
            return Err("Plugin proposal is cancelled or stale; run again".into());
        }
        let loaded = store.loaded.as_ref().ok_or("Open subtitles first")?;
        let edit = pending.prepared.proposal.edit(
            loaded.editor.document(),
            loaded.source(),
            &pending.prepared.request,
        )?;
        store.loaded_mut()?.apply_edit(key.revision.get(), edit)?;
        if let Some(job) = &mut store.plugins.job {
            job.accepted = true;
            job.prepared = None;
        }
        store.invalidate_checks();
        Ok(store.snapshot())
    }
    pub(super) fn shutdown_plugins(&self) -> Result<()> {
        let (worker, accepting) = {
            let mut store = self.0.lock().map_err(|_| "Document state unavailable")?;
            store.plugins.closed = true;
            store.plugins.job.as_mut().map_or((None, None), |j| {
                j.cancel();
                (j.worker.take(), j.accepting.take())
            })
        };
        if let Some(worker) = worker {
            let _ = worker.join().map_err(|_| "Plugin shutdown failed")?;
        }
        if let Some(done) = accepting {
            let _ = done.recv();
        }
        Ok(())
    }
}
fn content(c: &subtitle_core::CueUpdate) -> Result<view::Content> {
    let mut end = c.text.len().min(PREVIEW_BYTES);
    while !c.text.is_char_boundary(end) {
        end -= 1;
    }
    Ok(view::Content {
        start_ms: c.start.get().try_into()?,
        end_ms: c.end.get().try_into()?,
        text: view::TextPreview {
            text: c.text[..end].into(),
            total_bytes: c.text.len() as u32,
            truncated: end < c.text.len(),
        },
    })
}
fn project(
    job: &Job,
    closed: bool,
    findings_offset: u32,
    changes_offset: u32,
    limit: u32,
) -> Result<view::View> {
    let prepared = job.prepared.as_ref();
    let proposal = prepared.map(|p| &p.proposal);
    let preferences = &job.request.context.preferences;
    let settings = proposal
        .map(|p| {
            p.settings()
                .iter()
                .map(|(k, v)| {
                    Ok((
                        k.clone(),
                        serde_json::from_value(v.clone()).map_err(|e| e.to_string())?,
                    ))
                })
                .collect::<Result<BTreeMap<_, _>>>()
        })
        .transpose()?
        .unwrap_or_default();
    Ok(view::View {
        id: job.id.try_into()?,
        key: job.key,
        session: job.scenes.session.try_into()?,
        phase: job.phase(closed),
        mode: job.input.mode,
        path: job.path.clone(),
        package_id: prepared.map(|p| p.package.package().manifest.id.clone()),
        fingerprint: prepared.map(|p| p.package.fingerprint().into()),
        name: proposal.map(|p| p.name().into()),
        can_fix: proposal.is_some_and(|p| p.can_fix()),
        error: job.error.clone(),
        scope_count: job.request.scope.len() as u32,
        selection_count: job.request.context.selection.len() as u32,
        cursor_ms: job.input.cursor_ms,
        preferences: view::Preferences {
            minimum_blank_ms: preferences.minimum_blank_ms.try_into()?,
            minimum_duration_ms: preferences.minimum_duration_ms.try_into()?,
            maximum_duration_ms: preferences.maximum_duration_ms.try_into()?,
            cps_target: preferences.cps_target.try_into()?,
            rs_target_index: preferences.rs_target_index,
        },
        scene_coverage: job.coverage,
        scene_filter_offset_ms: job.input.scene_filter_offset_ms,
        settings,
        logs: proposal.map(|p| p.logs().to_vec()).unwrap_or_default(),
        findings_total: proposal.map_or(0, |p| p.findings().len() as u32),
        changes_total: proposal.map_or(0, |p| p.rows().len() as u32),
        findings_offset,
        changes_offset,
        findings: proposal
            .map(|p| {
                p.findings()
                    .iter()
                    .skip(findings_offset as usize)
                    .take(limit as usize)
                    .map(|f| view::Finding {
                        cue_id: f.cue_id.clone(),
                        message: f.message.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        rows: proposal
            .map(|p| {
                p.rows()
                    .iter()
                    .skip(changes_offset as usize)
                    .take(limit as usize)
                    .map(|r| {
                        Ok(view::Row {
                            cue_id: r.after.id.0.to_string(),
                            cue_index: r.cue_index as u32,
                            before: content(&r.before)?,
                            after: content(&r.after)?,
                            outside_scope: r.outside_scope,
                            warning: r.warning.clone(),
                        })
                    })
                    .collect::<Result<Vec<_>>>()
            })
            .transpose()?
            .unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests;
#[cfg(test)]
pub(crate) fn ipc_samples() -> Vec<(&'static str, &'static str, serde_json::Value)> {
    tests::samples()
}
