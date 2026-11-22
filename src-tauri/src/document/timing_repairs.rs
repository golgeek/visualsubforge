//! One explicit proposal worker and no pending queue. Immutable capture and
//! publication share the document lock; evaluation and shutdown join do not.
use super::*;
use crate::batch_timing::{BatchRow, BatchView};
use crate::{
    quality_check::Key,
    scenes::{SceneCapture, SceneKey},
    timing_review::{FixView, Input, Phase, Purpose, Section},
};
use std::collections::HashSet;
use std::{
    sync::atomic::{AtomicBool, Ordering},
    thread::JoinHandle,
    time::{Duration, Instant},
};
use vsf_application::batch_timing as batch;
use vsf_application::quality::{self as engine, Profile, timing_fix, timing_selection};

#[derive(Clone)]
enum Operation {
    Fix(Input),
    Batch(batch::Request),
    CueFixes(CueId),
}
impl Operation {
    fn purpose(&self) -> Purpose {
        match self {
            Self::Fix(_) => Purpose::Fix,
            Self::Batch(_) => Purpose::Batch,
            Self::CueFixes(_) => Purpose::CueFixes,
        }
    }
}
enum Proposal {
    Fix(
        Box<timing_fix::review::Review>,
        engine::scene::SceneCoverage,
    ),
    Batch(batch::Proposal),
    CueFixes(engine::cue_fixes::Proposal),
}
impl Proposal {
    fn edit(&self, document: &subtitle_core::Document) -> engine::Result<Edit> {
        match self {
            Self::Fix(review, _) => review.plan.edit(document),
            Self::Batch(p) => p.edit(document),
            Self::CueFixes(p) => p.edit(document),
        }
    }
}

struct Prepared {
    proposal: Proposal,
    selection: Option<timing_selection::Plan>,
}
impl Proposal {
    fn select(
        &self,
        document: &subtitle_core::Document,
        selection: timing_selection::Selection,

        checkpoint: impl FnMut() -> engine::Result<()>,
    ) -> engine::Result<timing_selection::Plan> {
        match self {
            Self::Batch(p) => {
                timing_selection::prepare_shift(document, p.edit(document)?, selection, checkpoint)
            }
            Self::Fix(..) => Err(engine::Error::Invalid(
                "Fix units have already been selected".into(),
            )),
            Self::CueFixes(_) => Err(engine::Error::Invalid(
                "Cue fixes are not a timing selection".into(),
            )),
        }
    }
}

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
    scenes: SceneKey,
    profile: Profile,
    hash: String,
    cancel: Arc<AtomicBool>,
    worker: Option<JoinHandle<Result<Prepared>>>,
    proposal: Option<Proposal>,
    selection: Option<timing_selection::Plan>,
    error: Option<String>,
    stale: bool,
    accepted: bool,
}
impl Job {
    fn cancel(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        self.proposal = None;
        self.selection = None;
    }
    fn refresh(&mut self) {
        if let Some(worker) = self.worker.take_if(|w| w.is_finished()) {
            let result = worker
                .join()
                .unwrap_or_else(|_| Err("Timing proposal worker stopped unexpectedly".into()));
            if !self.cancel.load(Ordering::Relaxed) {
                match result {
                    Ok(prepared) => {
                        self.proposal = Some(prepared.proposal);
                        self.selection = prepared.selection;
                    }
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
            return Err("Timing preview is shutting down".into());
        }
        if self.job.as_ref().is_some_and(|j| j.worker.is_some()) {
            return Err("Wait for the current timing proposal to finish cancelling".into());
        }
        Ok(())
    }
    fn launch(
        &mut self,
        key: Key,
        scenes: SceneKey,
        operation: Operation,
        profile: Profile,
        run: impl FnOnce(Arc<AtomicBool>) -> Result<Prepared> + Send + 'static,
    ) -> Result<u64> {
        self.ensure_idle()?;
        let id = self
            .next_id
            .checked_add(1)
            .filter(|i| *i <= crate::ipc_number::MAX_SAFE_INTEGER)
            .ok_or("Timing identities exhausted")?;
        let hash = profile.fingerprint().map_err(|e| e.to_string())?;
        let cancel = Arc::new(AtomicBool::new(false));
        let token = cancel.clone();
        let worker = std::thread::Builder::new()
            .name("timing-proposal".into())
            .spawn(move || run(token))
            .map_err(|e| format!("Cannot start timing proposal: {e}"))?;
        self.job = Some(Job {
            id,
            key,
            operation,
            scenes,
            profile,
            hash,
            cancel,
            worker: Some(worker),
            proposal: None,
            selection: None,
            error: None,
            stale: false,
            accepted: false,
        });
        self.next_id = id;
        Ok(id)
    }
}
impl Documents {
    pub(crate) fn initialize_timing_options(
        &self,
        stored: timing_fix::Options,
    ) -> Result<timing_fix::Options> {
        Ok(*self
            .0
            .lock()
            .map_err(|_| "Document state unavailable")?
            .timing_options
            .get_or_insert(stored))
    }
    pub(crate) fn set_timing_options(&self, value: timing_fix::Options) -> Result<()> {
        let mut store = self.0.lock().map_err(|_| "Document state unavailable")?;
        store.timing_options = Some(value);
        if let Some(job) = &mut store.timing_repairs.job
            && matches!(&job.operation,Operation::Fix(input) if input.options!=value)
        {
            job.stale = true;
            job.cancel();
        }
        Ok(())
    }

    pub(crate) fn timing_fix_start(
        &self,
        generation: u64,
        revision: u64,
        scenes: SceneCapture,
        input: Input,
    ) -> Result<u64> {
        self.timing_repair_start(generation, revision, scenes, Operation::Fix(input), None)
    }
    #[cfg(test)]
    pub(crate) fn batch_timing_start(
        &self,
        generation: u64,
        revision: u64,
        scenes: SceneCapture,
        request: batch::Request,
    ) -> Result<u64> {
        self.batch_timing_start_selected(generation, revision, scenes, request, None)
    }
    pub(crate) fn batch_timing_start_selected(
        &self,
        generation: u64,
        revision: u64,
        scenes: SceneCapture,
        request: batch::Request,
        selection: Option<timing_selection::Selection>,
    ) -> Result<u64> {
        self.timing_repair_start(
            generation,
            revision,
            scenes,
            Operation::Batch(request),
            selection,
        )
    }
    pub(crate) fn cue_fixes_start(
        &self,
        generation: u64,
        revision: u64,
        scenes: SceneCapture,
        cue: CueId,
    ) -> Result<u64> {
        self.timing_repair_start(generation, revision, scenes, Operation::CueFixes(cue), None)
    }
    pub(crate) fn cue_fixes_view(&self, id: u64) -> Result<crate::cue_fixes::View> {
        let mut store = self.0.lock().map_err(|_| "Document state unavailable")?;
        let key = store.quality_key()?;
        store.timing_repairs.invalidate(key);
        let closed = store.timing_repairs.closed;
        let job = store
            .timing_repairs
            .job
            .as_mut()
            .filter(|job| job.id == id && job.operation.purpose() == Purpose::CueFixes)
            .ok_or("Cue fixes were replaced")?;
        job.refresh();
        let mut view = crate::cue_fixes::View {
            id: id.try_into()?,
            key: job.key,
            phase: phase(job, closed),
            error: job.error.clone(),
            cue_id: None,
            fixed: 0,
            remaining: 0,
        };
        if let Some(Proposal::CueFixes(plan)) = &job.proposal {
            view.cue_id = Some(plan.cue_id.0.to_string());
            view.fixed = plan.fixed;
            view.remaining = plan.remaining;
        }
        Ok(view)
    }
    pub(crate) fn cue_fixes_accept(&self, id: u64) -> Result<Snapshot> {
        self.timing_repair_accept(id, Purpose::CueFixes)
    }
    fn timing_repair_start(
        &self,
        generation: u64,
        revision: u64,
        scenes: SceneCapture,
        operation: Operation,
        selection: Option<timing_selection::Selection>,
    ) -> Result<u64> {
        let mut store = self.0.lock().map_err(|_| "Document state unavailable")?;
        let key = store.quality_key()?;
        if !key.loaded || key.generation.get() != generation || key.revision.get() != revision {
            return Err("Subtitles changed before timing preview; try again".into());
        }
        if let Operation::Fix(input) = &operation
            && store
                .timing_options
                .is_some_and(|saved| input.options != saved)
        {
            return Err("Timing policy changed before preview; try again".into());
        }
        // Reject busy admission before cloning another document snapshot.
        store.timing_repairs.ensure_idle()?;
        let loaded = store.loaded.as_ref().ok_or("Open subtitles first")?;
        let document = loaded.editor.document().clone();
        let format = loaded.source().format();
        let profile = store.quality.context().profile;
        let captured = profile.clone();
        let lexicon = store.quality.lexicon();
        store
            .timing_repairs
            .launch(key, scenes.key, operation.clone(), profile, move |cancel| {
                let start = Instant::now();
                let mut checkpoint = || {
                    if cancel.load(Ordering::Relaxed) {
                        return Err(engine::Error::Cancelled);
                    }
                    if start.elapsed() > Duration::from_secs(10) {
                        return Err(engine::Error::Limit(
                            "Timing preview exceeded its 10 second deadline".into(),
                        ));
                    }
                    Ok(())
                };
                let context = engine::Evaluation {
                    profile: &captured,
                    format,
                    lexicon: Some(&lexicon),
                };
                let original = selection.as_ref().map(|_| document.clone());
                let proposal = match operation {
                    Operation::CueFixes(cue) => engine::cue_fixes::propose(
                        document,
                        context,
                        &scenes.data,
                        cue,
                        &mut checkpoint,
                    )
                    .map(Proposal::CueFixes),
                    Operation::Batch(request) => {
                        batch::propose(document, format, request, &mut checkpoint)
                            .map(Proposal::Batch)
                    }
                    Operation::Fix(input) => {
                        let request = input.domain()?;
                        timing_fix::propose(
                            document,
                            context,
                            &scenes.data,
                            request,
                            &mut checkpoint,
                        )
                        .and_then(|p| {
                            p.review(&input.toggled, input.omit_trades, &mut checkpoint)
                                .map(|r| Proposal::Fix(Box::new(r), p.before().scene_coverage))
                        })
                    }
                }
                .map_err(|e| e.to_string())?;
                let selection = if let (Some(selection), Some(original)) = (selection, original) {
                    Some(
                        proposal
                            .select(&original, selection, checkpoint)
                            .map_err(|e| e.to_string())?,
                    )
                } else {
                    None
                };
                Ok(Prepared {
                    proposal,
                    selection,
                })
            })
    }
    pub(crate) fn timing_repair_context(&self, id: u64, purpose: Purpose) -> Result<SceneKey> {
        let store = self.0.lock().map_err(|_| "Document state unavailable")?;
        Ok(store
            .timing_repairs
            .job
            .as_ref()
            .filter(|j| j.id == id && j.operation.purpose() == purpose)
            .ok_or("Timing proposal was replaced or has a different kind")?
            .scenes)
    }
    pub(crate) fn timing_fix_view_for_cues(
        &self,
        id: u64,
        offset: u32,
        limit: u32,
        section: Section,
        preview: Option<&HashSet<CueId>>,
    ) -> Result<FixView> {
        if limit > 200 {
            return Err("Timing review pages are limited to 200 rows".into());
        }
        let mut store = self.0.lock().map_err(|_| "Document state unavailable")?;
        let key = store.quality_key()?;
        store.timing_repairs.invalidate(key);
        let closed = store.timing_repairs.closed;
        let job = store
            .timing_repairs
            .job
            .as_mut()
            .filter(|j| j.id == id && j.operation.purpose() == Purpose::Fix)
            .ok_or("Timing proposal was replaced or has a different kind")?;
        job.refresh();
        let mut view = FixView {
            key: job.key,
            id: job.id.try_into()?,
            profile: job.profile.clone(),
            profile_hash: job.hash.clone(),
            phase: phase(job, closed),
            error: job.error.clone(),
            selection_error: None,
            selection_conflicts: Vec::new(),
            coverage: None,
            before: Default::default(),
            after: Default::default(),
            total: 0,
            changed: 0,
            manual_count: 0,
            trades: 0,
            withheld_all: false,
            without_trades_toggles: Vec::new(),
            offset,
            rows: vec![],
            manual: vec![],
        };
        if let Some(Proposal::Fix(review, coverage)) = &job.proposal {
            view.withheld_all = review.withheld_all;
            view.coverage = Some(*coverage);
            view.before = review.before;
            view.after = review.after;
            view.selection_error = review.plan.error.clone();
            view.selection_conflicts = review.plan.selection_conflicts.clone();
            view.total = review.changes.len() as u32;
            view.changed = review.selected;
            view.trades = review.trades;
            view.without_trades_toggles = review.without_trades_toggles.clone();
            view.manual_count = review.manual.len() as u32;
            let visible = |r: &&vsf_application::quality::timing_fix::review::Row| {
                preview.is_none_or(|ids| {
                    r.unit.endpoints.iter().any(|e| {
                        review
                            .plan
                            .cue_id_at(e.index as usize)
                            .is_some_and(|id| ids.contains(&id))
                    })
                })
            };
            let count = match section {
                Section::Changes if preview.is_some() => {
                    review.changes.iter().filter(visible).count() as u32
                }
                Section::Changes => view.total,
                Section::Manual => view.manual_count,
            };
            view.offset = offset.min(count.saturating_sub(1) / limit.max(1) * limit.max(1));
            match section {
                Section::Changes => {
                    view.rows = review
                        .changes
                        .iter()
                        .filter(visible)
                        .skip(view.offset as usize)
                        .take(limit as usize)
                        .cloned()
                        .collect();
                    if let Some(ids) = preview {
                        for row in &mut view.rows {
                            row.ghosts.retain(|g| {
                                g.cue_id.parse().is_ok_and(|id| ids.contains(&CueId(id)))
                            });
                        }
                    }
                }
                Section::Manual => {
                    view.manual = review
                        .manual
                        .iter()
                        .skip(view.offset as usize)
                        .take(limit as usize)
                        .cloned()
                        .collect()
                }
            }
        }
        Ok(view)
    }
    #[cfg(test)]
    pub(crate) fn timing_fix_view(&self, id: u64, offset: u32, limit: u32) -> Result<FixView> {
        self.timing_fix_view_for_cues(id, offset, limit, Section::Changes, None)
    }
    #[cfg(test)]
    pub(crate) fn batch_timing_view(&self, id: u64, offset: u32, limit: u32) -> Result<BatchView> {
        self.batch_timing_view_for_cues(id, offset, limit, None)
    }
    pub(crate) fn batch_timing_view_for_cues(
        &self,
        id: u64,
        offset: u32,
        limit: u32,
        preview: Option<&HashSet<CueId>>,
    ) -> Result<BatchView> {
        if limit > 200 {
            return Err("Batch pages are limited to 200 rows".into());
        }
        let mut store = self.0.lock().map_err(|_| "Document state unavailable")?;
        let key = store.quality_key()?;
        store.timing_repairs.invalidate(key);
        let closed = store.timing_repairs.closed;
        let job = store
            .timing_repairs
            .job
            .as_mut()
            .filter(|j| j.id == id && j.operation.purpose() == Purpose::Batch)
            .ok_or("Timing proposal was replaced or has a different kind")?;
        job.refresh();
        let mut view = BatchView {
            id: job.id.try_into()?,
            key: job.key,
            session: job.scenes.session.try_into()?,
            phase: phase(job, closed),
            error: job.error.clone(),
            selection_error: job.selection.as_ref().and_then(|s| s.error.clone()),
            total: 0,
            changed: 0,
            offset,
            offset_ms: None,
            aligned_ms: None,
            rows: vec![],
        };
        if let Some(Proposal::Batch(p)) = &job.proposal {
            let rows: Vec<_> = p
                .rows
                .iter()
                .filter(|row| {
                    job.selection
                        .as_ref()
                        .is_none_or(|s| s.in_scope(row.before.id))
                        && preview.is_none_or(|ids| ids.contains(&row.before.id))
                })
                .collect();
            view.total = rows.len() as u32;
            view.changed = job.selection.as_ref().map_or(p.rows.len(), |s| s.changed) as u32;
            view.offset_ms = Some(p.offset_ms.to_string());
            view.aligned_ms = p.aligned_ms.map(|t| t.get().try_into()).transpose()?;
            view.offset = offset.min(view.total.saturating_sub(1) / limit.max(1) * limit.max(1));
            for row in rows
                .into_iter()
                .skip(view.offset as usize)
                .take(limit as usize)
            {
                view.rows.push(BatchRow {
                    cue_id: row.before.id.0.to_string(),
                    cue_index: row.cue_index as u32,
                    selected: job
                        .selection
                        .as_ref()
                        .is_none_or(|s| s.selected(row.before.id)),
                    old_start_ms: row.before.start.get().try_into()?,
                    old_end_ms: row.before.end.get().try_into()?,
                    new_start_ms: row.after.start.get().try_into()?,
                    new_end_ms: row.after.end.get().try_into()?,
                });
            }
        }
        Ok(view)
    }
    pub(crate) fn batch_timing_accept(&self, id: u64) -> Result<Snapshot> {
        self.timing_repair_accept(id, Purpose::Batch)
    }
    pub(crate) fn timing_repair_cancel(&self, id: u64, purpose: Purpose) -> Result<()> {
        let mut store = self.0.lock().map_err(|_| "Document state unavailable")?;
        if let Some(job) = store.timing_repairs.job.as_mut().filter(|j| j.id == id) {
            if job.operation.purpose() != purpose {
                return Err("Timing proposal has a different kind".into());
            }
            job.cancel();
        }
        Ok(())
    }
    /// Called while media selection and scene state remain locked and current.
    pub(crate) fn timing_repair_accept(&self, id: u64, purpose: Purpose) -> Result<Snapshot> {
        let mut store = self.0.lock().map_err(|_| "Document state unavailable")?;
        let key = store.quality_key()?;
        store.timing_repairs.invalidate(key);
        if store.timing_repairs.closed {
            return Err("Timing preview is shutting down".into());
        }
        let job = store
            .timing_repairs
            .job
            .as_mut()
            .filter(|j| j.id == id && j.operation.purpose() == purpose)
            .ok_or("Timing proposal was replaced or has a different kind")?;
        job.refresh();
        if job.key != key || job.stale || job.cancel.load(Ordering::Relaxed) || job.accepted {
            return Err("Timing proposal is no longer current; create a new preview".into());
        }
        let proposal = store
            .timing_repairs
            .job
            .as_ref()
            .and_then(|j| j.proposal.as_ref())
            .ok_or("Wait for a completed timing preview")?;
        let edit = proposal
            .edit(
                store
                    .loaded
                    .as_ref()
                    .ok_or("Open subtitles first")?
                    .editor
                    .document(),
            )
            .map_err(|e| e.to_string())?;
        let edit = if let Some(selection) = store
            .timing_repairs
            .job
            .as_ref()
            .and_then(|job| job.selection.as_ref())
        {
            selection
                .edit(
                    store
                        .loaded
                        .as_ref()
                        .ok_or("Open subtitles first")?
                        .editor
                        .document(),
                )
                .map_err(|e| e.to_string())?
        } else {
            edit
        };
        store.loaded_mut()?.apply_edit(key.revision.get(), edit)?;
        if let Some(job) = &mut store.timing_repairs.job {
            job.accepted = true;
            job.proposal = None;
            job.selection = None;
        }
        store.invalidate_checks();
        Ok(store.snapshot())
    }
    pub(super) fn shutdown_timing_repairs(&self) -> Result<()> {
        let job = {
            let mut store = self.0.lock().map_err(|_| "Document state unavailable")?;
            store.timing_repairs.closed = true;
            if let Some(job) = &mut store.timing_repairs.job {
                job.cancel();
            }
            store.timing_repairs.job.take()
        };
        drop(job); // cancellation and join outside the document mutex
        Ok(())
    }
}
fn phase(job: &Job, closed: bool) -> Phase {
    if closed {
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
    }
}
#[cfg(test)]
mod tests;
#[cfg(test)]
pub(crate) fn ipc_sample() -> serde_json::Value {
    let (_directory, docs, scenes) = tests::fixture();
    let id = tests::start(&docs, &scenes);
    let mut view = tests::ready(&docs, id);
    view.manual = docs
        .timing_fix_view_for_cues(id, 0, 100, Section::Manual, None)
        .unwrap()
        .manual;
    serde_json::to_value(view).unwrap()
}

#[cfg(test)]
mod batch_tests;
#[cfg(test)]
mod duration_tests;
#[cfg(test)]
mod gap_tests;

#[cfg(test)]
pub(crate) fn batch_ipc_sample() -> serde_json::Value {
    let (_dir, docs, scenes) = tests::fixture();
    let id = batch_tests::start(&docs, &scenes, 123);
    serde_json::to_value(batch_tests::ready(&docs, id)).unwrap()
}

#[cfg(test)]
pub(crate) fn preset_samples() -> Vec<(&'static str, &'static str, serde_json::Value)> {
    let (_tmp, docs) = gap_tests::fixture();
    let id = gap_tests::start(&docs);
    let mut gaps = gap_tests::ready(&docs, id);
    gaps.manual = docs
        .timing_fix_view_for_cues(id, 0, 100, Section::Manual, None)
        .unwrap()
        .manual;
    let (_tmp, docs, scenes) = duration_tests::fixture();
    let id = duration_tests::start(&docs, &scenes, None);
    let mut speed = duration_tests::ready(&docs, id);
    speed.manual = docs
        .timing_fix_view_for_cues(id, 0, 100, Section::Manual, None)
        .unwrap()
        .manual;
    vec![
        (
            "fixGapsSample",
            "TimingFixView",
            serde_json::to_value(&gaps).unwrap(),
        ),
        (
            "fixSpeedSample",
            "TimingFixView",
            serde_json::to_value(speed).unwrap(),
        ),
    ]
}
