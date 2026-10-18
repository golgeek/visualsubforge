//! Style review shares admission, cancellation, identities and shutdown with text
//! review. Only one of these explicit document proposals can run at a time.
use super::*;
use crate::style_review::{StyleForm, StyleFormField, StyleReview};
use style_engine::StyleOperation;

impl Documents {
    pub(crate) fn style_form(
        &self,
        generation: u64,
        revision: u64,
        name: Option<&str>,
    ) -> Result<StyleForm> {
        let store = self.0.lock().map_err(|_| "Document state unavailable")?;
        let key = store.quality_key()?;
        current(key, generation, revision)?;
        let script = store
            .loaded
            .as_ref()
            .and_then(|l| l.source().ass())
            .ok_or("Style authoring requires ASS/SSA subtitles")?;
        let default;
        let style = if let Some(name) = name {
            script
                .styles()
                .iter()
                .find(|s| s.name == name)
                .ok_or("Style no longer exists")?
        } else {
            default = style_engine::default_style(script.variant());
            &default
        };
        if style.fields.len() > 64
            || style
                .fields
                .iter()
                .any(|(name, value)| name.len() > 128 || value.len() > 1024)
        {
            return Err("This imported style exceeds the editable field limits; its original definition remains preserved".into());
        }
        Ok(StyleForm {
            key,
            fields: style
                .fields
                .iter()
                .map(|(name, value)| StyleFormField {
                    name: name.clone(),
                    value: value.trim().into(),
                    editable: style_engine::supported_field(script.variant(), name),
                })
                .collect(),
        })
    }
    pub(crate) fn style_start(
        &self,
        generation: u64,
        revision: u64,
        operation: StyleOperation,
    ) -> Result<u64> {
        let mut store = self.0.lock().map_err(|_| "Document state unavailable")?;
        let key = store.quality_key()?;
        current(key, generation, revision)?;
        store.texts.ensure_idle()?;
        let loaded = store.loaded.as_ref().ok_or("Open subtitles first")?;
        if loaded.source().ass().is_none() {
            return Err("Style authoring requires ASS/SSA subtitles".into());
        }
        let document = loaded.editor.document().clone();
        let source = loaded.source().clone();
        let profile = store.quality.context().profile;
        let request = operation.clone();
        store
            .texts
            .launch(key, Request::Style(operation), profile, move |cancel| {
                let start = Instant::now();
                style_engine::propose(&document, &source, &request, || {
                    if cancel.load(Ordering::Relaxed) {
                        return Err("Style preview cancelled".into());
                    }
                    if start.elapsed() > Duration::from_secs(10) {
                        return Err("Style preview exceeded its 10 second deadline".into());
                    }
                    Ok(())
                })
                .map(Prepared::Style)
            })
    }
    pub(crate) fn style_view(&self, id: u64) -> Result<StyleReview> {
        let mut store = self.0.lock().map_err(|_| "Document state unavailable")?;
        let key = store.quality_key()?;
        store.texts.invalidate(key);
        let closed = store.texts.closed;
        let job = store
            .texts
            .job
            .as_mut()
            .filter(|j| j.id == id)
            .ok_or("Style proposal was replaced")?;
        let Request::Style(operation) = &job.operation else {
            return Err("This proposal belongs to text review".into());
        };
        let operation = operation.clone();
        job.refresh();
        let proposal = match &job.proposal {
            Some(Prepared::Style(p)) => Some(p),
            _ => None,
        };
        Ok(StyleReview {
            id: id.try_into()?,
            key: job.key,
            phase: phase(job, closed),
            operation,
            summary: proposal.map(|p| p.summary.clone()),
            changed: proposal.is_some_and(|p| p.changed),
            error: job.error.clone(),
        })
    }
    pub(crate) fn style_cancel(&self, id: u64) -> Result<()> {
        let mut store = self.0.lock().map_err(|_| "Document state unavailable")?;
        if let Some(job) = store
            .texts
            .job
            .as_mut()
            .filter(|j| j.id == id && matches!(j.operation, Request::Style(_)))
        {
            job.cancel();
        }
        Ok(())
    }
    pub(crate) fn style_accept(&self, id: u64) -> Result<Snapshot> {
        let mut store = self.0.lock().map_err(|_| "Document state unavailable")?;
        let key = store.quality_key()?;
        store.texts.invalidate(key);
        if store.texts.closed {
            return Err("Document review is shutting down".into());
        }
        let job = store
            .texts
            .job
            .as_mut()
            .filter(|j| j.id == id && matches!(j.operation, Request::Style(_)))
            .ok_or("Style proposal was replaced")?;
        job.refresh();
        if job.key != key || job.stale || job.cancel.load(Ordering::Relaxed) || job.accepted {
            return Err("Style proposal is no longer current; create a new preview".into());
        }
        let Some(Prepared::Style(proposal)) = job.proposal.take() else {
            return Err("Wait for a completed style preview".into());
        };
        let (document, source) = proposal.into_parts();
        if let Err(error) = store.loaded_mut()?.editor.apply_document(
            key.revision.get(),
            document,
            source,
            vsf_application::subtitle_file::validate_document,
        ) {
            if let Some(job) = &mut store.texts.job {
                job.error = Some(error.clone());
            }
            return Err(error);
        }
        if let Some(job) = &mut store.texts.job {
            job.accepted = true;
        }
        store.invalidate_checks();
        Ok(store.snapshot())
    }
}
fn current(key: Key, generation: u64, revision: u64) -> Result<()> {
    if !key.loaded || key.generation.get() != generation || key.revision.get() != revision {
        return Err("Subtitles changed before style review; try again".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) use tests::ipc_samples;
