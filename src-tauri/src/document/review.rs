//! Independent review slot. Workers retain immutable captures, never Store.
use super::translation::context::ReferenceFile;
use super::*;
mod discussion;
mod exchange;
use exchange::{ImportChoice, Inspection, OutputKind, PackageLink};
mod operations;
use crate::ipc_number::SafeU64;
use discussion::{CommentEdit, Discussion, DiscussionPage, ThreadIndex};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    thread::JoinHandle,
};
use vsf_application::comparison as engine;
use vsf_application::review_package;

#[derive(Clone, Default, Debug)]
pub(crate) struct Context {
    pub(crate) reference: translation::Context,
    pub(crate) options: engine::Options,
    discussion: Discussion,
    package: Option<PackageLink>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Stored {
    reference: Option<translation::Stored>,
    options: engine::Options,
    #[serde(default, skip_serializing_if = "Discussion::empty")]
    discussion: Discussion,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    package: Option<PackageLink>,
}
impl Context {
    pub(crate) fn stored(&self) -> Result<Option<Stored>> {
        if self.reference.is_empty() {
            return Ok(None);
        }
        Ok(Some(Stored {
            reference: self.reference.stored()?,
            options: self.options.clone(),
            discussion: self.discussion.clone(),
            package: self.package.clone(),
        }))
    }
}
impl Stored {
    pub(crate) fn has_discussion(&self) -> bool {
        !self.discussion.empty() || self.package.is_some()
    }
    pub(crate) fn restore(&self) -> Result<Context> {
        self.discussion.validate()?;
        if let Some(link) = &self.package {
            link.validate()?;
            if !Path::new(&link.path).is_absolute() {
                return Err("Expected absolute review package path in recovery".into());
            }
        }
        let context = Context {
            reference: self
                .reference
                .as_ref()
                .map(|r| r.restore())
                .transpose()?
                .unwrap_or_default(),
            options: self.options.clone(),
            discussion: self.discussion.clone(),
            package: self.package.clone(),
        };
        if let Some(link) = &context.package
            && context.reference.reference.as_ref().is_none_or(|r| {
                r.reference.fingerprint() != link.sha256 || r.origin.path != Path::new(&link.path)
            })
        {
            return Err("Recovery package identity mismatch".into());
        }
        Ok(context)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewKey {
    generation: SafeU64,
    revision: SafeU64,
    epoch: SafeU64,
}
#[derive(Clone, Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ReviewSummary {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub round: Option<u32>,
    pub epoch: SafeU64,
    pub dirty: bool,
    pub path: Option<String>,
    pub options: engine::Options,
}
#[derive(Deserialize, specta::Type)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ReviewAction {
    Status,
    Comment {
        key: ReviewKey,
        author: String,
        edit: CommentEdit,
    },
    Threads {
        key: ReviewKey,
        open: bool,
        offset: u32,
        limit: u32,
    },
    NavigateThread {
        key: ReviewKey,
        from: Option<String>,
        forward: bool,
    },
    RevertBlock {
        key: ReviewKey,
        first: u32,
    },
    Revert {
        key: ReviewKey,
        change_key: Option<String>,
        source_index: Option<u32>,
        filter: Option<engine::Kind>,
    },
    InspectPackage {
        key: ReviewKey,
        path: String,
    },
    ImportPackage {
        key: ReviewKey,
        path: String,
        digest: String,
        choice: ImportChoice,
        discard: bool,
    },
    Export {
        #[serde(default)]
        output: Option<OutputKind>,
        #[serde(default)]
        author: String,
        key: ReviewKey,
        path: String,
        include_unchanged: bool,
        #[serde(default)]
        utc_offset_minutes: i16,
    },
    Projection {
        key: ReviewKey,
        ids: Vec<SafeU64>,
        start_ms: SafeU64,
        end_ms: SafeU64,
    },
    Navigate {
        #[serde(default)]
        expanded: Vec<u32>,
        key: ReviewKey,
        from: Option<String>,
        cue_id: Option<SafeU64>,
        forward: bool,
        filter: Option<engine::Kind>,
    },
    Inspect {
        original: String,
        revised: String,
    },
    Attach {
        key: ReviewKey,
        path: String,
    },
    Start {
        key: ReviewKey,
        path: String,
        discard: bool,
    },
    Detach {
        key: ReviewKey,
    },
    Options {
        key: ReviewKey,
        options: engine::Options,
    },
    Query {
        #[serde(default)]
        expanded: Vec<u32>,
        key: ReviewKey,
        offset: u32,
        limit: u32,
        filter: Option<engine::Kind>,
    },
}
#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ReviewReply {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    blocks: Vec<BlockRow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    discussion: Option<DiscussionPage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thread_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    inspection: Option<Inspection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    notice: Option<String>,
    matched: u32,
    key: ReviewKey,
    context: Option<ReviewSummary>,
    document: Option<Snapshot>,
    working: bool,
    statistics: Option<engine::Statistics>,
    rows: Vec<engine::Change>,
    total: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    position: Option<u32>,
    error: Option<String>,
    newer_original: Option<bool>,
}
#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
struct BlockRow {
    row: u32,
    block: engine::ShiftBlock,
    first_cue: SafeU64,
    last_cue: SafeU64,
}
enum ComparisonInput {
    Retained(engine::Comparison),
    Cues(Vec<subtitle_core::Cue>),
}
struct Job {
    key: ReviewKey,
    cancel: Arc<AtomicBool>,
    worker: Option<JoinHandle<Result<(engine::Comparison, ThreadIndex)>>>,
    threads: Option<ThreadIndex>,
    result: Option<engine::Comparison>,
    needs_reanchor: bool,
    error: Option<String>,
}
impl Job {
    fn refresh(&mut self) {
        if let Some(worker) = self.worker.take_if(|w| w.is_finished()) {
            match worker
                .join()
                .unwrap_or_else(|_| Err("Comparison worker stopped".into()))
            {
                Ok((result, threads)) if !self.cancel.load(Ordering::Acquire) => {
                    self.result = Some(result);
                    self.threads = (!self.needs_reanchor).then_some(threads)
                }
                Err(error) if !self.cancel.load(Ordering::Acquire) => self.error = Some(error),
                _ => {}
            }
        }
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
#[derive(Default)]
pub(super) struct State {
    pub(super) epoch: u64,
    job: Option<Job>,
    closed: bool,
}
impl Store {
    fn review_key(&self) -> Result<ReviewKey> {
        Ok(ReviewKey {
            generation: self.generation.try_into()?,
            revision: self
                .loaded
                .as_ref()
                .map_or(0, |l| l.editor.revision())
                .try_into()?,
            epoch: self
                .loaded
                .as_ref()
                .map_or(0, |_| self.review.epoch)
                .try_into()?,
        })
    }
    fn check_review_key(&self, key: ReviewKey) -> Result<()> {
        if self.review.closed || key != self.review_key()? {
            Err("Review changed; refresh before continuing".into())
        } else {
            Ok(())
        }
    }
    pub(super) fn review_summary(&self) -> Option<ReviewSummary> {
        let context = &self.loaded.as_ref()?.review;
        if context.reference.is_empty() && self.review.epoch == 0 {
            return None;
        }
        Some(ReviewSummary {
            round: (context.discussion.round > 0).then_some(context.discussion.round),
            epoch: self.review.epoch.try_into().ok()?,
            dirty: context.reference.dirty,
            path: context
                .reference
                .reference
                .as_ref()
                .map(|r| r.origin.path.to_string_lossy().into_owned()),
            options: context.options.clone(),
        })
    }
    fn prospective_review_epoch(&self) -> Result<u64> {
        self.review
            .epoch
            .checked_add(1)
            .filter(|v| *v <= crate::ipc_number::MAX_SAFE_INTEGER)
            .ok_or_else(|| "Review epoch exhausted".into())
    }
    // A discussion update cannot invalidate cue matching. An index addresses threads
    // by their canonical positions, so only a change of thread IDs invalidates it.
    fn advance_discussion(&mut self, key: ReviewKey, anchors_changed: bool) -> Result<()> {
        self.review.epoch = self.prospective_review_epoch()?;
        if let Some(job) = self.review.job.as_mut().filter(|j| j.key == key) {
            job.key.epoch = self.review.epoch.try_into()?;
            if anchors_changed {
                job.needs_reanchor = true;
                job.threads = None;
            }
        }
        Ok(())
    }
    fn next_review_epoch(&mut self) -> Result<()> {
        self.review.epoch = self.prospective_review_epoch()?;
        if let Some(job) = &mut self.review.job {
            job.cancel.store(true, Ordering::Release);
            job.result = None;
        }
        Ok(())
    }
    fn review_reply(
        &mut self,
        document: Option<Snapshot>,
        offset: u32,
        limit: u32,
        filter: Option<engine::Kind>,
    ) -> Result<ReviewReply> {
        self.review_page(document, offset, limit, filter, &[])
    }
    fn review_page(
        &mut self,
        document: Option<Snapshot>,
        offset: u32,
        limit: u32,
        filter: Option<engine::Kind>,
        expanded: &[u32],
    ) -> Result<ReviewReply> {
        let key = self.review_key()?;
        let context = self.review_summary();
        let mut reply = ReviewReply {
            blocks: Vec::new(),
            discussion: None,
            thread_id: None,
            inspection: None,
            notice: None,
            matched: 0,
            key,
            context,
            document,
            working: false,
            statistics: None,
            rows: Vec::new(),
            total: 0,
            position: None,
            error: None,
            newer_original: None,
        };
        if let Some(job) = &mut self.review.job {
            if job.key != key {
                job.cancel.store(true, Ordering::Release);
                job.result = None;
            }
            job.refresh();
            reply.working = job.worker.is_some() || job.needs_reanchor && job.error.is_none();
            if job.key == key {
                reply.error = job.error.clone();
                if let (Some(result), Some(index)) = (&job.result, &job.threads) {
                    reply.statistics = Some(result.statistics.clone());
                    let discussion = &self
                        .loaded
                        .as_ref()
                        .ok_or("Review has ended")?
                        .review
                        .discussion;
                    let mut page = DiscussionPage {
                        open: discussion.threads.iter().filter(|t| !t.resolved()).count() as u32,
                        ..Default::default()
                    };
                    let stops = operations::stops(result, filter, expanded);
                    reply.matched = result
                        .changes
                        .iter()
                        .filter(|c| operations::matches(c, filter))
                        .count() as u32;
                    reply.total = stops.len() as u32;
                    for stop in stops.iter().skip(offset as usize).take(limit as usize) {
                        index.project(
                            stop.first,
                            &result.changes[stop.first],
                            discussion,
                            &mut page,
                        );
                        if let Some(block) = stop.block {
                            page.block_counts
                                .insert(block.first, index.block_count(block));
                            reply.blocks.push(BlockRow {
                                row: reply.rows.len() as u32,
                                block: block.clone(),
                                first_cue: result.changes[block.first as usize].revised[0]
                                    .cue_id
                                    .try_into()?,
                                last_cue: result.changes[block.end as usize - 1].revised[0]
                                    .cue_id
                                    .try_into()?,
                            });
                        }
                        reply.rows.push(result.changes[stop.first].clone());
                    }
                    if filter.is_none() {
                        reply.total += index.detached.len() as u32;
                        for &t in index
                            .detached
                            .iter()
                            .skip((offset as usize).saturating_sub(stops.len()))
                            .take((limit as usize).saturating_sub(reply.rows.len()))
                        {
                            let thread = &discussion.threads[t];
                            page.include_exported(thread, discussion);
                            page.detached.push(thread.clone());
                        }
                    }
                    reply.discussion = Some(page);
                    for cue in reply
                        .rows
                        .iter()
                        .flat_map(|c| c.original.iter().chain(&c.revised))
                    {
                        SafeU64::try_from(cue.cue_id)?;
                        SafeU64::try_from(cue.start_ms)?;
                        SafeU64::try_from(cue.end_ms)?;
                    }
                }
            }
        }
        Ok(reply)
    }
}
fn now_ms() -> Result<u64> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis()
        .try_into()
        .map_err(|_| "Review timestamp out of range".into())
}
impl Documents {
    pub(crate) fn review_action(&self, action: ReviewAction) -> Result<ReviewReply> {
        if let ReviewAction::InspectPackage { key, path } = &action {
            return self.import_review(*key, Path::new(path), None);
        }
        if let ReviewAction::ImportPackage {
            key,
            path,
            digest,
            choice,
            discard,
        } = &action
        {
            return self.import_review(*key, Path::new(path), Some((digest, *choice, *discard)));
        }
        if let ReviewAction::Export {
            output,
            author,
            key,
            path,
            include_unchanged,
            utc_offset_minutes,
        } = &action
        {
            return self.publish_review(
                *key,
                Path::new(path),
                output.unwrap_or(OutputKind::Readable),
                author,
                *utc_offset_minutes,
                *include_unchanged,
            );
        }
        if let ReviewAction::Inspect { original, revised } = &action {
            let original = ReferenceFile::load(Path::new(original), || Ok(()))?;
            let revised = ReferenceFile::load(Path::new(revised), || Ok(()))?;
            let newer = std::fs::metadata(&original.origin.path)
                .and_then(|m| m.modified())
                .ok()
                .zip(
                    std::fs::metadata(&revised.origin.path)
                        .and_then(|m| m.modified())
                        .ok(),
                )
                .is_some_and(|(a, b)| a > b);
            let mut store = self.0.lock().map_err(|_| "Document unavailable")?;
            let mut reply = store.review_reply(None, 0, 100, None)?;
            reply.newer_original = Some(newer);
            return Ok(reply);
        }
        if let ReviewAction::Start { key, path, discard } = action {
            let (checkpoint, source, saved, protected) = {
                let store = self.0.lock().map_err(|_| "Document unavailable")?;
                store.check_review_key(key)?;
                let loaded = store.loaded.as_ref().ok_or("Open subtitles first")?;
                if loaded.subtitle_dirty() && !discard {
                    return Err("Save or discard subtitle changes before starting a review".into());
                }
                let source = loaded
                    .path
                    .clone()
                    .ok_or("Save the subtitles before starting a review")?;
                let protected: Vec<_> = loaded
                    .translation
                    .protected_paths()
                    .chain(loaded.review.reference.protected_paths())
                    .chain(loaded.extracted_from.iter().map(|p| p.media_path.as_path()))
                    .map(Path::to_owned)
                    .collect();
                (
                    store.checkpoint_key(),
                    source,
                    loaded.saved_bytes.clone(),
                    protected,
                )
            };
            let original = ReferenceFile::load(&source, || Ok(()))?;
            if original.reference.original_bytes() != saved {
                return Err("Original changed on disk; reopen it first".into());
            }
            let output = vsf_application::new_output::resolve(Path::new(&path))
                .map_err(|e| e.to_string())?;
            for p in protected {
                vsf_application::new_output::protect_source(&output, &p)
                    .map_err(|e| e.to_string())?;
            }
            original.origin.protect(&output)?;
            let prepared = vsf_application::atomic_file::prepare(&output, &saved)?;
            let parsed = original.reference.copy_target();
            let mut store = self.0.lock().map_err(|_| "Document unavailable")?;
            store.check_review_key(key)?;
            if store.checkpoint_key() != checkpoint || store.loaded_mut()?.saved_bytes != saved {
                return Err("Document changed while preparing reviewed copy".into());
            }
            prepared.commit_new()?;
            let loaded = store.loaded_mut()?;
            // Preserve extraction provenance so Save As continues protecting the video.
            loaded.editor = Editor::with_properties(parsed.document, parsed.source);
            loaded.recovered = false;
            loaded.resynced = false;
            // Copy preserves any already-attached base and translation slot.
            if loaded.review.reference.reference.is_none() {
                loaded.review.options = engine::Options::for_format(original.reference.format());
                loaded.review.reference.reference = Some(original.clone());
            }
            loaded.review.reference.copied_from = Some(original.origin);
            loaded.review.reference.dirty = true;
            loaded.path = Some(output);
            store.generation += 1;
            store.next_review_epoch()?;
            let document = Some(store.snapshot());
            return store.review_reply(document, 0, 100, None);
        }
        // File preparation runs outside Store. Adoption rechecks all identities.
        if let ReviewAction::Attach { key, path } = action {
            {
                self.0
                    .lock()
                    .map_err(|_| "Document unavailable")?
                    .check_review_key(key)?;
            }
            let original = ReferenceFile::load(Path::new(&path), || Ok(()))?;
            let mut store = self.0.lock().map_err(|_| "Document unavailable")?;
            store.check_review_key(key)?;
            let loaded = store.loaded_mut()?;
            if loaded.path.as_ref() == Some(&original.origin.path) {
                return Err("Choose a different original file".into());
            }
            original.verify(|| Ok(()))?;
            let options = engine::Options::for_format(original.reference.format());
            loaded.review = Context {
                reference: translation::Context {
                    reference: Some(original),
                    copied_from: loaded.review.reference.copied_from.clone(),
                    dirty: true,
                },
                options,
                discussion: Default::default(),
                package: None,
            };
            store.next_review_epoch()?;
            let document = Some(store.snapshot());
            return store.review_reply(document, 0, 100, None);
        }
        let mut store = self.0.lock().map_err(|_| "Document unavailable")?;
        let mut document = None;
        let (mut offset, mut limit, mut filter) = (0, 100, None);
        let mut expanded = Vec::new();
        match action {
            ReviewAction::Attach { .. } | ReviewAction::Inspect { .. } => unreachable!(),
            ReviewAction::Status => {}
            ReviewAction::Comment { key, author, edit } => {
                store.comment(key, author, now_ms()?, edit)?;
            }
            ReviewAction::Threads {
                key,
                open,
                offset,
                limit,
            } => return store.thread_page(key, open, offset, limit, None),
            ReviewAction::NavigateThread { key, from, forward } => {
                return store.thread_page(key, true, 0, 1, Some((from, forward)));
            }
            ReviewAction::Start { .. } => unreachable!(),
            ReviewAction::Export { .. }
            | ReviewAction::InspectPackage { .. }
            | ReviewAction::ImportPackage { .. } => unreachable!(),
            ReviewAction::RevertBlock { key, first } => {
                store.revert_review(key, None, None, None, Some(first))?;
                document = Some(store.snapshot());
                store.invalidate_checks();
            }
            ReviewAction::Revert {
                key,
                change_key,
                source_index,
                filter,
            } => {
                store.revert_review(key, change_key.as_deref(), source_index, filter, None)?;
                document = Some(store.snapshot());
                store.invalidate_checks();
            }
            ReviewAction::Projection {
                key,
                ids,
                start_ms,
                end_ms,
            } => return store.review_projection(key, &ids, start_ms.get(), end_ms.get()),
            ReviewAction::Navigate {
                expanded,
                key,
                from,
                cue_id,
                forward,
                filter,
            } => {
                return store.navigate_review(
                    key,
                    from.as_deref(),
                    cue_id.map(|id| id.get()),
                    forward,
                    filter,
                    &expanded,
                );
            }
            ReviewAction::Detach { key } => {
                store.check_review_key(key)?;
                let c = &mut store.loaded_mut()?.review;
                c.reference.reference = None;
                c.discussion = Discussion::default();
                c.package = None;
                c.reference.dirty = true;
                store.next_review_epoch()?;
                document = Some(store.snapshot());
            }
            ReviewAction::Options { key, options } => {
                store.check_review_key(key)?;
                let c = &mut store.loaded_mut()?.review;
                c.options = options;
                c.reference.dirty = true;
                store.next_review_epoch()?;
                document = Some(store.snapshot());
            }
            ReviewAction::Query {
                expanded: requested,
                key,
                offset: o,
                limit: l,
                filter: f,
            } => {
                store.check_review_key(key)?;
                expanded = requested;
                if !(1..=200).contains(&l) {
                    return Err("Comparison pages require 1–200 rows".into());
                }
                (offset, limit, filter) = (o, l, f);
                if let Some(job) = &mut store.review.job {
                    job.refresh();
                    if job.key != key {
                        job.cancel.store(true, Ordering::Release);
                    }
                }
                if store.review.job.as_ref().is_none_or(|j| {
                    (j.key != key || (j.needs_reanchor && j.result.is_some())) && j.worker.is_none()
                }) {
                    let retained = store
                        .review
                        .job
                        .as_mut()
                        .filter(|j| j.key == key)
                        .and_then(|j| j.result.take());
                    let loaded = store.loaded.as_ref().ok_or("Open subtitles first")?;
                    let original = loaded
                        .review
                        .reference
                        .reference
                        .as_ref()
                        .ok_or("Attach an original first")?
                        .reference
                        .clone();
                    let input = match retained {
                        Some(c) => ComparisonInput::Retained(c),
                        None => ComparisonInput::Cues(loaded.editor.document().cues().to_vec()),
                    };
                    let format = loaded.source().format();
                    let options = loaded.review.options.clone();
                    let threads = loaded.review.discussion.threads.clone();
                    let cancel = Arc::new(AtomicBool::new(false));
                    let token = cancel.clone();
                    let worker = std::thread::Builder::new()
                        .name("subtitle-comparison".into())
                        .spawn(move || {
                            let deadline =
                                std::time::Instant::now() + std::time::Duration::from_secs(10);
                            let checkpoint = || {
                                if token.load(Ordering::Acquire) {
                                    Err("Comparison cancelled".into())
                                } else if std::time::Instant::now() > deadline {
                                    Err("Comparison timed out".into())
                                } else {
                                    Ok(())
                                }
                            };
                            let c = match input {
                                ComparisonInput::Retained(c) => c,
                                ComparisonInput::Cues(cues) => engine::compare(
                                    original.cues(),
                                    original.format(),
                                    &cues,
                                    format,
                                    options,
                                    checkpoint,
                                )?,
                            };
                            let positions = review_package::reanchor(
                                &threads,
                                &c,
                                original.format(),
                                format,
                                checkpoint,
                            )?;
                            Ok((c, ThreadIndex::new(positions)))
                        })
                        .map_err(|e| e.to_string())?;
                    store.review.job = Some(Job {
                        key,
                        cancel,
                        worker: Some(worker),
                        result: None,
                        needs_reanchor: false,
                        threads: None,
                        error: None,
                    });
                }
            }
        }
        store.review_page(document, offset, limit, filter, &expanded)
    }
    pub(crate) fn shutdown_review(&self) -> Result<()> {
        let job = {
            let mut store = self.0.lock().map_err(|_| "Document unavailable")?;
            store.review.closed = true;
            store.review.job.take()
        };
        drop(job);
        Ok(())
    }
}
#[tauri::command]
#[specta::specta]
pub async fn review_action(
    documents: tauri::State<'_, Documents>,
    action: ReviewAction,
) -> Result<ReviewReply> {
    let documents = documents.inner().clone();
    tauri::async_runtime::spawn_blocking(move || documents.review_action(action))
        .await
        .map_err(|e| e.to_string())?
}

#[derive(Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewProjectContext {
    reference: translation::ProjectContext,
    options: engine::Options,
    #[serde(default, skip_serializing_if = "Discussion::empty")]
    discussion: Discussion,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    package: Option<PackageLink>,
}
impl Context {
    pub(crate) fn project_description(&self, base: &Path) -> Result<Option<ReviewProjectContext>> {
        Ok(self
            .reference
            .project_description(base)?
            .map(|reference| ReviewProjectContext {
                reference,
                options: self.options.clone(),
                discussion: self.discussion.clone(),
                package: self.package.clone().map(|mut l| {
                    l.path = crate::project::portable(base, Path::new(&l.path));
                    l
                }),
            }))
    }
}
impl ReviewProjectContext {
    pub(crate) fn has_discussion(&self) -> bool {
        !self.discussion.empty() || self.package.is_some()
    }
    pub(crate) fn validate(&self) -> Result<()> {
        self.discussion.validate()?;
        if let Some(l) = &self.package {
            l.validate()?;
            if !self.reference.identifies(&l.path, &l.sha256) {
                return Err("Package link and review original disagree".into());
            }
        }
        self.reference.validate()
    }
    pub(crate) fn protect(&self, base: &Path, path: &Path) -> Result<()> {
        self.reference.protect(base, path)
    }
    pub(crate) fn load(
        &self,
        base: &Path,
        overrides: &translation::ReferenceOverride,
    ) -> std::result::Result<Context, translation::ReferenceIssue> {
        let reference = self.reference.load_with(base, overrides, |path| {
            if let Some(link) = &self.package {
                let p = vsf_application::review_file::read(path)?;
                if p.original.sha256 != link.sha256 {
                    return Err(
                        "Choose the review package with the expected original fingerprint".into(),
                    );
                }
                exchange::reference(path, &p.original)
            } else {
                ReferenceFile::load(path, || Ok(()))
            }
        })?;
        let package = self.package.clone().map(|mut l| {
            if let Some(r) = &reference.reference {
                l.path = r.origin.path.to_string_lossy().into()
            }
            l
        });
        Ok(Context {
            reference,
            options: self.options.clone(),
            discussion: self.discussion.clone(),
            package,
        })
    }
}

#[derive(Clone, Default, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectOverride {
    pub path: Option<String>,
    #[serde(default)]
    pub end: bool,
}

#[cfg(test)]
mod tests;
