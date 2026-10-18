//! One review slot owns preparation and subsequent new-file publication. IO runs
//! off the document lock; the no-clobber commit and final identity check serialize
//! with document operations. Publication never adopts a document or marks saved.
use super::*;
use crate::conversion_review::{ConversionPhase, ConversionReview};
use vsf_application::{conversion as engine, new_output};

#[derive(Clone)]
pub(super) struct PreparedConversion {
    proposal: Arc<engine::Proposal>,
    source_path: Option<PathBuf>,
}
impl Documents {
    pub(crate) fn conversion_start(
        &self,
        generation: u64,
        revision: u64,
        options: engine::ConversionOptions,
    ) -> Result<u64> {
        let mut store = self.0.lock().map_err(|_| "Document state unavailable")?;
        let key = store.quality_key()?;
        if !key.loaded || key.generation.get() != generation || key.revision.get() != revision {
            return Err("Subtitles changed before conversion review; try again".into());
        }
        store.texts.ensure_idle()?;
        let loaded = store.loaded.as_ref().ok_or("Open subtitles first")?;
        if loaded.source().format() == options.target.format() {
            return Err("Choose a different format; use Save As for a same-format copy".into());
        }
        let document = loaded.editor.document().clone();
        let source = loaded.source().clone();
        let source_path = loaded.path.clone();
        let profile = store.quality.context().profile;
        store
            .texts
            .launch(key, Request::Conversion(options), profile, move |cancel| {
                let start = Instant::now();
                let proposal =
                    engine::propose(&document, &source, options, || checkpoint(&cancel, start))?;
                Ok(Prepared::Conversion(PreparedConversion {
                    proposal: Arc::new(proposal),
                    source_path,
                }))
            })
    }
    pub(crate) fn conversion_view(&self, id: u64) -> Result<ConversionReview> {
        let mut store = self.0.lock().map_err(|_| "Document state unavailable")?;
        let key = store.quality_key()?;
        store.texts.invalidate(key);
        let source_path = store.loaded.as_ref().and_then(|d| d.path.clone());
        let closed = store.texts.closed;
        let job = store
            .texts
            .job
            .as_mut()
            .filter(|j| j.id == id)
            .ok_or("Conversion review was replaced")?;
        let Request::Conversion(options) = job.operation else {
            return Err("This proposal belongs to another document review".into());
        };
        job.refresh();
        if !job.accepted
            && matches!(&job.proposal, Some(Prepared::Conversion(p)) if p.source_path != source_path)
        {
            job.stale = true;
            job.cancel();
        }
        Ok(ConversionReview {
            id: id.try_into()?,
            key: job.key,
            options,
            phase: match phase(job, closed) {
                Phase::Accepted => ConversionPhase::Published,
                Phase::Closed => ConversionPhase::Closed,
                Phase::Stale => ConversionPhase::Stale,
                Phase::Cancelled => ConversionPhase::Cancelled,
                Phase::Cancelling => ConversionPhase::Cancelling,
                Phase::Failed => ConversionPhase::Failed,
                _ if job.publishing => ConversionPhase::Publishing,
                Phase::Reviewing => ConversionPhase::Reviewing,
                _ => ConversionPhase::Preparing,
            },
            report: match &job.proposal {
                Some(Prepared::Conversion(p)) => Some(p.proposal.report().clone()),
                _ => None,
            },
            output_path: job.published_path.clone(),
            error: job.error.clone(),
        })
    }
    pub(crate) fn conversion_cancel(&self, id: u64) -> Result<()> {
        let mut store = self.0.lock().map_err(|_| "Document state unavailable")?;
        if let Some(job) =
            store.texts.job.as_mut().filter(|j| {
                j.id == id && matches!(j.operation, Request::Conversion(_)) && !j.accepted
            })
        {
            job.cancel();
        }
        Ok(())
    }
    pub(crate) fn conversion_publish(&self, id: u64, path: &str) -> Result<()> {
        self.publish_conversion(id, path, || Ok(()))
    }
    // Test hooks model a delayed filesystem boundary without changing guards or
    // adding a production IPC escape hatch. The same final check always runs.
    fn publish_conversion(
        &self,
        id: u64,
        path: &str,
        prepared_hook: impl FnOnce() -> Result<()> + Send + 'static,
    ) -> Result<()> {
        if path.is_empty() || path.len() > 32_768 {
            return Err("Choose an output filename with a supported path length".into());
        }
        let mut store = self.0.lock().map_err(|_| "Document state unavailable")?;
        let key = store.quality_key()?;
        store.texts.invalidate(key);
        let source_path = store.loaded.as_ref().and_then(|d| d.path.clone());
        if store.texts.closed {
            return Err("Document review is shutting down".into());
        }
        let job = store
            .texts
            .job
            .as_mut()
            .filter(|j| j.id == id && matches!(j.operation, Request::Conversion(_)))
            .ok_or("Conversion review was replaced")?;
        job.refresh();
        if job.key != key || job.stale || job.cancel.load(Ordering::Relaxed) || job.accepted {
            return Err("Conversion is no longer current; create a new review".into());
        }
        if job.worker.is_some() {
            return Err("Wait for the current conversion operation to finish".into());
        }
        let Some(Prepared::Conversion(prepared)) = &job.proposal else {
            return Err("Wait for a complete conversion review".into());
        };
        if prepared.source_path != source_path {
            return Err("The subtitle path changed; review conversion again".into());
        }
        if prepared.proposal.output().is_none() {
            return Err("Resolve the blocking conversion findings before export".into());
        }
        let prepared = prepared.clone();
        // Store owns this worker, so a strong capture would form a lifetime cycle.
        let documents = Arc::downgrade(&self.0);
        let cancel = job.cancel.clone();
        let path = PathBuf::from(path);
        let worker = std::thread::Builder::new()
            .name("document-export".into())
            .spawn(move || {
                let start = Instant::now();
                checkpoint(&cancel, start)?;
                let path = new_output::resolve(&path).map_err(|e| e.to_string())?;
                if path
                    .extension()
                    .and_then(|e| e.to_str())
                    .and_then(subtitle_formats::Format::from_extension)
                    != Some(prepared.proposal.report().options.target.format())
                {
                    return Err(format!(
                        "Converted output must use .{}",
                        prepared
                            .proposal
                            .report()
                            .options
                            .target
                            .format()
                            .extension()
                    ));
                }
                if let Some(source) = &prepared.source_path {
                    new_output::protect_source(&path, source).map_err(|e| e.to_string())?;
                }
                checkpoint(&cancel, start)?;
                let _lease = vsf_application::save_lease::acquire(&path)?;
                new_output::require_absent(&path).map_err(|e| e.to_string())?;
                let pending = vsf_application::atomic_file::prepare_with_checkpoint(
                    &path,
                    prepared
                        .proposal
                        .output()
                        .ok_or("Conversion has no publishable output")?
                        .as_bytes(),
                    || checkpoint(&cancel, start),
                )?;
                prepared_hook()?;
                checkpoint(&cancel, start)?;
                // Recheck namespace/provenance after temporary IO. The final
                // no-clobber commit remains authoritative if a writer races it.
                if new_output::resolve(&path).map_err(|e| e.to_string())? != path {
                    return Err("Output directory changed during export; choose it again".into());
                }
                if let Some(source) = &prepared.source_path {
                    new_output::protect_source(&path, source).map_err(|e| e.to_string())?;
                }
                // The worker can only commit the still-current review. Shutdown and
                // document changes acquire this same lock; joins happen outside it.
                let documents = documents
                    .upgrade()
                    .ok_or("Document owner closed during export")?;
                let mut store = documents.lock().map_err(|_| "Document state unavailable")?;
                let current = store.quality_key()?;
                store.texts.invalidate(current);
                if store.texts.closed
                    || current != key
                    || store.loaded.as_ref().and_then(|d| d.path.as_ref())
                        != prepared.source_path.as_ref()
                {
                    return Err(
                        "Subtitles or their path changed during export; review conversion again"
                            .into(),
                    );
                }
                let job = store
                    .texts
                    .job
                    .as_mut()
                    .filter(|j| j.id == id && matches!(j.operation, Request::Conversion(_)))
                    .ok_or("Conversion review was replaced")?;
                if job.accepted || job.stale {
                    return Err("Conversion is no longer current".into());
                }
                checkpoint(&cancel, start)?;
                // Once commit succeeds, late cancellation cannot undo publication.
                pending.commit_new()?;
                job.accepted = true;
                job.published_path = Some(path.to_string_lossy().into_owned());
                drop(store);
                Ok(Prepared::Conversion(prepared))
            })
            .map_err(|e| format!("Cannot start conversion export: {e}"))?;
        job.error = None;
        job.publishing = true;
        job.worker = Some(worker);
        Ok(())
    }
}
fn checkpoint(cancel: &AtomicBool, start: Instant) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        return Err("Conversion cancelled".into());
    }
    if start.elapsed() > Duration::from_secs(10) {
        return Err("Conversion exceeded its 10 second deadline".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests;
#[cfg(test)]
pub(crate) use tests::ipc_samples;
