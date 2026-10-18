//! Font inspection and proposals share the text/style review worker. Captured
//! bytes become immutable source properties; acceptance never rereads the file.
use super::*;
use crate::font_review::{FontOperation, FontReview};
use std::{fs, io::Read};
use vsf_application::authoring::{attachments as fonts, fonts::MAX_FONT_BYTES};

pub(super) enum PreparedFont {
    Inventory(fonts::AttachmentInventory),
    Edit(Box<fonts::Proposal>),
}
impl Documents {
    pub(crate) fn font_start(
        &self,
        generation: u64,
        revision: u64,
        operation: FontOperation,
    ) -> Result<u64> {
        if let FontOperation::Add { path } | FontOperation::Replace { path, .. } = &operation
            && (path.is_empty() || path.len() > 32_768)
        {
            return Err("Choose a font file with a supported path length".into());
        }
        let mut store = self.0.lock().map_err(|_| "Document state unavailable")?;
        let key = store.quality_key()?;
        if !key.loaded || key.generation.get() != generation || key.revision.get() != revision {
            return Err("Subtitles changed before font review; try again".into());
        }
        store.texts.ensure_idle()?;
        let loaded = store.loaded.as_ref().ok_or("Open subtitles first")?;
        if loaded.source().ass().is_none() {
            return Err("Font attachments require ASS/SSA subtitles".into());
        }
        // Inspection needs no cue clone. Proposal captures stay inside the same
        // admission boundary, so a rejected concurrent start allocates none.
        let document = (!matches!(operation, FontOperation::Inspect))
            .then(|| loaded.editor.document().clone());
        let source = loaded.source().clone();
        let profile = store.quality.context().profile;
        let request = operation.clone();
        store
            .texts
            .launch(key, Request::Font(operation), profile, move |cancel| {
                let start = Instant::now();
                let mut checkpoint = || {
                    if cancel.load(Ordering::Relaxed) {
                        return Err("Font review cancelled".into());
                    }
                    if start.elapsed() > Duration::from_secs(10) {
                        return Err("Font review exceeded its 10 second deadline".into());
                    }
                    Ok(())
                };
                if matches!(request, FontOperation::Inspect) {
                    return fonts::inspect(&source, checkpoint)
                        .map(PreparedFont::Inventory)
                        .map(Prepared::Font);
                }
                let data;
                let edit = match &request {
                    FontOperation::Add { path } | FontOperation::Replace { path, .. } => {
                        data = read_font(Path::new(path), &mut checkpoint)?;
                        fonts::FontEdit::Put {
                            replace: if let FontOperation::Replace { id, .. } = request {
                                Some(id)
                            } else {
                                None
                            },
                            name: &data.0,
                            bytes: &data.1,
                        }
                    }
                    FontOperation::Remove { id } => fonts::FontEdit::Remove { id: *id },
                    FontOperation::Inspect => {
                        return Err("Font inspection was already handled".into());
                    }
                };
                fonts::propose(
                    document
                        .as_ref()
                        .ok_or("Font proposal capture is unavailable")?,
                    &source,
                    edit,
                    checkpoint,
                )
                .map(Box::new)
                .map(PreparedFont::Edit)
                .map(Prepared::Font)
            })
    }
    pub(crate) fn font_view(&self, id: u64) -> Result<FontReview> {
        let mut store = self.0.lock().map_err(|_| "Document state unavailable")?;
        let key = store.quality_key()?;
        store.texts.invalidate(key);
        let closed = store.texts.closed;
        let job = store
            .texts
            .job
            .as_mut()
            .filter(|j| j.id == id)
            .ok_or("Font review was replaced")?;
        let Request::Font(operation) = &job.operation else {
            return Err("This proposal belongs to another document review".into());
        };
        let operation = operation.clone();
        job.refresh();
        let (inventory, proposal) = match &job.proposal {
            Some(Prepared::Font(PreparedFont::Inventory(list))) => (Some(list.clone()), None),
            Some(Prepared::Font(PreparedFont::Edit(proposal))) => (None, Some(proposal)),
            _ => (None, None),
        };
        Ok(FontReview {
            id: id.try_into()?,
            key: job.key,
            phase: phase(job, closed),
            operation,
            inventory,
            summary: proposal.map(|p| p.summary.clone()),
            changed: proposal.is_some_and(|p| p.changed),
            error: job.error.clone(),
        })
    }
    pub(crate) fn font_cancel(&self, id: u64) -> Result<()> {
        let mut store = self.0.lock().map_err(|_| "Document state unavailable")?;
        if let Some(job) = store
            .texts
            .job
            .as_mut()
            .filter(|j| j.id == id && matches!(j.operation, Request::Font(_)))
        {
            job.cancel();
        }
        Ok(())
    }
    pub(crate) fn font_accept(&self, id: u64) -> Result<Snapshot> {
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
            .filter(|j| j.id == id && matches!(j.operation, Request::Font(_)))
            .ok_or("Font proposal was replaced")?;
        job.refresh();
        if job.key != key || job.stale || job.cancel.load(Ordering::Relaxed) || job.accepted {
            return Err("Font proposal is no longer current; create a new preview".into());
        }
        if !matches!(&job.proposal, Some(Prepared::Font(PreparedFont::Edit(_)))) {
            return Err(
                "Wait for a completed font change preview; inspection alone cannot be accepted"
                    .into(),
            );
        }
        let Some(Prepared::Font(PreparedFont::Edit(proposal))) = job.proposal.take() else {
            unreachable!("proposal kind checked above")
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

fn read_font(
    path: &Path,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<(String, Vec<u8>)> {
    checkpoint()?;
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("Font filename must be UTF-8")?;
    if name.len() > 128
        || !path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("ttf") || e.eq_ignore_ascii_case("otf"))
    {
        return Err("Choose a .ttf or .otf filename of at most 128 UTF-8 bytes".into());
    }
    let metadata = fs::metadata(path).map_err(|e| format!("Cannot inspect font file: {e}"))?;
    if !metadata.is_file() || metadata.len() > MAX_FONT_BYTES as u64 {
        return Err("Choose a regular font file no larger than 4 MiB".into());
    }
    let mut file = fs::File::open(path).map_err(|e| format!("Cannot open font file: {e}"))?;
    let metadata = file
        .metadata()
        .map_err(|e| format!("Cannot inspect opened font file: {e}"))?;
    if !metadata.is_file() || metadata.len() > MAX_FONT_BYTES as u64 {
        return Err("Opened font must be a regular file no larger than 4 MiB".into());
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    let mut chunk = [0; 64 * 1024];
    loop {
        checkpoint()?;
        let count = file
            .read(&mut chunk)
            .map_err(|e| format!("Cannot read font file: {e}"))?;
        if count == 0 {
            break;
        }
        if bytes.len() + count > MAX_FONT_BYTES {
            return Err("Font file grew beyond the 4 MiB limit".into());
        }
        bytes.extend_from_slice(&chunk[..count]);
    }
    checkpoint()?;
    Ok((name.into(), bytes))
}

#[cfg(test)]
mod tests;
#[cfg(test)]
pub(crate) use tests::ipc_samples;
