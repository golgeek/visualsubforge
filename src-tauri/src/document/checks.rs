//! The document mutex protects admission, capture and publication. The one CPU
//! worker owns an immutable domain clone, never the document owner or its lock.
//! There is one latest draft and no queued document snapshots. After completion
//! fires, refresh joins the returning thread under the lock. Shutdown releases the
//! document lock before joining a worker that might still be computing.
mod index;
use super::*;
use crate::quality_check::{Draft, DraftChange, Key, Metric, Phase, Request, View};
use index::ReportIndex;
use std::{
    collections::HashMap,
    sync::atomic::{AtomicBool, Ordering},
    thread::JoinHandle,
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;
use vsf_application::language::LanguageCache;
use vsf_application::quality::{self as engine, Report};

#[derive(Default)]
pub(super) struct Checks {
    key: Option<Key>,
    scenes: Option<crate::scenes::SceneKey>,
    id: u64,
    id_key: Option<Key>,
    id_scenes: Option<crate::scenes::SceneKey>,
    worker: Option<Worker>,
    language_cache: Option<LanguageCache>,
    report: Option<AcceptedReport>,
    completed_at_ms: Option<crate::ipc_number::SafeU64>,
    report_id: Option<crate::ipc_number::SafeU64>,
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
    handle: Option<JoinHandle<(Result<Output>, LanguageCache)>>,
    finished: CancellationToken,
}
struct AcceptedReport {
    data: Report,
    index: ReportIndex,
}
enum Output {
    Accepted(Box<AcceptedReport>),
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
            if self
                .key
                .is_some_and(|prior| prior.generation != key.generation || prior.epoch != key.epoch)
            {
                self.language_cache = None;
            }
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
        let Some(mut worker) = self.worker.take_if(|w| w.finished.is_cancelled()) else {
            return;
        };
        let cancelled = worker.cancel.load(Ordering::Relaxed);
        let result = worker
            .handle
            .take()
            .expect("finished handle exists")
            .join()
            .map_err(|_| "Quality worker stopped unexpectedly".to_string())
            .and_then(|(result, cache)| {
                if worker.draft.is_none()
                    && (cancelled || result.is_ok())
                    && self.key.is_some_and(|key| {
                        key.generation == worker.key.generation && key.epoch == worker.key.epoch
                    })
                {
                    self.language_cache = Some(cache);
                }
                result
            });
        if cancelled || Some(worker.key) != self.key {
            return;
        }
        match result {
            Ok(Output::Accepted(report)) => {
                self.completed_at_ms = crate::recent::now().ok();
                self.report = Some(*report);
                self.report_id = self.id.try_into().ok();
            }
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
        capture: super::quality::Capture,
        draft: Option<Draft>,
        scenes: engine::scene::Scenes,
    ) -> Result<()> {
        self.launch_with(key, draft.clone(), move |cancel, cache| {
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
            let context = capture.evaluation();
            let checkpoint = |_, _| {
                if cancel.load(Ordering::Relaxed) {
                    return Err(engine::Error::Cancelled);
                }
                if began.elapsed() > Duration::from_secs(10) {
                    return Err(engine::Error::Limit(
                        "Quality check exceeded its 10 second deadline".into(),
                    ));
                }
                Ok(())
            };
            match draft {
                Some(draft) => {
                    draft_metric(&document, context, &scenes, draft.cue_id.get(), checkpoint)
                        .map(Output::Draft)
                }
                None => {
                    let report =
                        engine::evaluate_cached(&document, context, &scenes, cache, checkpoint)
                            .map_err(|e| e.to_string())?;
                    let index = ReportIndex::build(&report, || {
                        if cancel.load(Ordering::Relaxed) {
                            return Err("Quality check cancelled".into());
                        }
                        if began.elapsed() > Duration::from_secs(10) {
                            return Err("Quality check exceeded its 10 second deadline".into());
                        }
                        Ok(())
                    })?;
                    Ok(Output::Accepted(Box::new(AcceptedReport {
                        data: report,
                        index,
                    })))
                }
            }
        })
    }
    fn launch_with(
        &mut self,
        key: Key,
        draft: Option<Draft>,
        run: impl FnOnce(Arc<AtomicBool>, &mut LanguageCache) -> Result<Output> + Send + 'static,
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
        let finished = CancellationToken::new();
        let completion = finished.clone().drop_guard();
        let mut cache = if draft.is_none() {
            self.language_cache.take().unwrap_or_default()
        } else {
            LanguageCache::default()
        };
        let handle = std::thread::Builder::new()
            .name("subtitle-quality".into())
            .spawn(move || {
                let _completion = completion;
                (run(token, &mut cache), cache)
            })
            .map_err(|e| format!("Cannot start quality worker: {e}"))?;
        self.worker = Some(Worker {
            key,
            draft,
            cancel,
            handle: Some(handle),
            finished,
        });
        self.id = id;
        self.id_key = Some(key);
        self.id_scenes = self.scenes;
        Ok(())
    }
}
fn draft_metric(
    document: &subtitle_core::Document,
    context: engine::Evaluation<'_>,
    scenes: &engine::scene::Scenes,
    id: u64,
    checkpoint: impl FnMut(usize, usize) -> engine::Result<()>,
) -> Result<Metric> {
    let id = id.to_string();
    let mut profile = context.profile.clone();
    profile.language_tools = Default::default();
    let context = engine::Evaluation {
        profile: &profile,
        ..context
    };
    engine::evaluate_with_scenes(document, context, scenes, checkpoint)
        .map_err(|e| e.to_string())?
        .metrics
        .iter()
        .find(|m| m.cue_id == id)
        .map(Metric::from)
        .ok_or_else(|| "Draft cue no longer exists".into())
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
        if let Ok(key) = self.translation_key() {
            self.translation.invalidate(key);
        }
        if let Ok(key) = self.quality_key() {
            self.checks.synchronize(key);
            self.statistics.invalidate(key);
            self.timing_repairs.invalidate(key);
            self.texts.invalidate(key);
            self.plugins.invalidate(key);
        } else {
            self.checks.cancel_worker();
            self.checks.report = None;
        }
    }
}
impl Documents {
    pub(crate) fn quality_completion(&self, id: u64) -> Result<Option<CancellationToken>> {
        let store = self.0.lock().map_err(|_| "Document state unavailable")?;
        Ok(store
            .checks
            .worker
            .as_ref()
            .filter(|_| store.checks.id == id)
            .map(|w| w.finished.clone()))
    }

    /// Small read-only projection; no worker admission, reaping or file reads.
    pub(crate) fn recent_subtitles(
        &self,
        generation: u64,
        revision: u64,
        scenes: crate::scenes::SceneKey,
    ) -> Result<Option<crate::recent::SubtitleCapture>> {
        let store = self.0.lock().map_err(|_| "Document state unavailable")?;
        let key = store.quality_key()?;
        if key.generation.get() != generation || key.revision.get() != revision {
            return Err("Subtitles changed before recent-work capture".into());
        }
        let Some(loaded) = &store.loaded else {
            return Ok(None);
        };
        let Some(path) = &loaded.path else {
            return Ok(None);
        };
        let checks = &store.checks;
        let report = checks.report.as_ref().filter(|_| {
            !loaded.subtitle_dirty()
                && !loaded.recovered
                && !loaded.resynced
                && checks.key == Some(key)
                && checks.scenes == Some(scenes)
                && !checks.cancelled
                && !checks.closed
                && checks.error.is_none()
        });
        let summary =
            report
                .map(|r| &r.data)
                .zip(checks.completed_at_ms)
                .map(|(report, checked_at_ms)| crate::recent::Summary {
                    cues: loaded.editor.document().cues().len() as u32,
                    issues: report.findings.len() as u32,
                    complete: report.complete,
                    profile_name: report.profile.name.clone(),
                    profile_hash: report.profile_hash.clone(),
                    source_sha256: String::new(),
                    checked_at_ms,
                });
        Ok(Some(crate::recent::SubtitleCapture {
            path: path.clone(),
            bytes: summary.as_ref().map(|_| loaded.saved_bytes.clone()),
            summary,
        }))
    }
    #[cfg(test)]
    pub(crate) fn quality_read(&self, request: Request) -> Result<View> {
        let capture =
            crate::scenes::Scenes::default().capture_quality(request.session.get(), false)?;
        self.quality_read_scenes(request, capture)
    }
    pub(crate) fn quality_read_scenes(
        &self,
        mut request: Request,
        scenes: crate::scenes::SceneCapture,
    ) -> Result<View> {
        if request.limit > 200 || request.metrics_limit > 200 {
            return Err("Quality pages are limited to 200 rows".into());
        }
        let mut store = self.0.lock().map_err(|_| "Document state unavailable")?;
        let key = store.quality_key()?;
        if request.generation != key.generation || request.revision != key.revision {
            return Err("Subtitles changed before quality read".into());
        }
        store.checks.synchronize(key);
        if store.checks.scenes != Some(scenes.key) {
            store.checks.cancel_worker();
            store.checks.scenes = Some(scenes.key);
            store.checks.report = None;
            store.checks.error = None;
            store.checks.draft_metric = None;
            store.checks.draft_error = None;
            store.checks.cancelled = false;
            // Retain the latest draft; the frontend may not resend identical text.
        }
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
                let format = store
                    .loaded
                    .as_ref()
                    .ok_or("Open subtitles first")?
                    .source()
                    .format();
                let capture = store.quality.capture(format);
                store
                    .checks
                    .launch(key, document, capture, draft, scenes.data)?;
            }
        }
        project_view(&store.checks, key, profile, &request)
    }
    pub(crate) fn quality_cancel(&self, id: u64) -> Result<()> {
        let mut store = self.0.lock().map_err(|_| "Document state unavailable")?;
        if store.checks.id == id
            && store.checks.id_key == store.checks.key
            && store.checks.id_scenes == store.checks.scenes
        {
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
        self.shutdown_statistics()?;
        self.shutdown_timing_repairs()?;
        self.shutdown_texts()?;
        self.shutdown_translation()?;
        self.shutdown_review()?;
        self.shutdown_plugins()
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
        report_id: None,
        issue_index: None,
        profile,
        profile_hash: None,
        scene_coverage: None,
        language_coverage: None,
        error: checks.error.clone(),
        findings_count: 0,
        timing_findings_count: 0,
        rule_cues: vec![],
        affected_cues: 0,
        filtered_count: 0,
        filtered_cues: 0,
        offset: request.offset,
        findings: vec![],
        metrics: vec![],
        selected: None,
        selected_issues: None,
        previous_cue: None,
        next_cue: None,
        draft: checks.draft_metric.clone(),
        draft_checking: checks.draft.is_some()
            && checks.draft_metric.is_none()
            && checks.draft_error.is_none()
            && !checks.cancelled,
        draft_error: checks.draft_error.clone(),
    };
    let Some(AcceptedReport {
        data: report,
        index,
    }) = &checks.report
    else {
        return Ok(view);
    };
    view.report_id = checks.report_id;
    index.project(report, request, &mut view);
    view.profile_hash = Some(report.profile_hash.clone());
    view.scene_coverage = Some(report.scene_coverage);
    view.language_coverage = Some(report.language_coverage.clone());
    view.findings_count = report.findings.len() as u32;
    view.affected_cues = report.affected_cues;
    view.metrics = report
        .metrics
        .iter()
        .skip(request.metrics_offset as usize)
        .take(request.metrics_limit as usize)
        .map(Metric::from)
        .collect();
    Ok(view)
}

fn cue_issues<'a>(
    findings: impl Iterator<Item = &'a vsf_application::quality::Finding>,
    id: &str,
    offset: u32,
) -> crate::quality_check::CueIssues {
    use crate::quality_check::{CueIssue, CueIssues};
    use vsf_application::quality::{Finding, Rule, Severity};
    let mut issues: Vec<(Rule, Vec<&Finding>)> = Vec::new();
    for finding in findings {
        let rule = match finding.rule {
            Rule::CpsHigh | Rule::RsHigh => Rule::RsHigh,
            Rule::CpsLow | Rule::RsLow => Rule::RsLow,
            rule => rule,
        };
        if matches!(rule, Rule::RsHigh | Rule::RsLow)
            && let Some(issue) = issues.iter_mut().find(|issue| issue.0 == rule)
        {
            issue.1.push(finding);
        } else {
            issues.push((rule, vec![finding]));
        }
    }
    // Stable within one immutable report; errors precede advisories. Clone only
    // the requested page, including both source findings for a speed issue.
    issues.sort_by_key(|issue| {
        !issue
            .1
            .iter()
            .any(|finding| finding.severity == Severity::Violation)
    });
    let count = issues.len() as u32;
    let offset = page_offset(offset, count, 200);
    CueIssues {
        cue_id: id.into(),
        count,
        offset,
        issues: issues
            .into_iter()
            .skip(offset as usize)
            .take(200)
            .map(|(rule, findings)| CueIssue {
                rule,
                findings: findings.into_iter().cloned().collect(),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
pub(crate) fn ipc_sample() -> serde_json::Value {
    serde_json::to_value(tests::ready(&tests::fixture())).unwrap()
}
