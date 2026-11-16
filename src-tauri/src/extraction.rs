//! One bounded analysis worker; immutable results await explicit guarded adoption.
use crate::{
    analysis_worker::Worker,
    document::{Documents, Snapshot, extraction::ExtractionCapture},
};
use media_analysis::{
    Cancellation, Tools,
    subtitle_tracks::{self, Listing},
};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use vsf_application::subtitle_extraction::{self, Prepared, Provenance};
type Result<T> = std::result::Result<T, String>;
#[derive(Deserialize, specta::Type)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ExtractionAction {
    List {
        path: String,
    },
    Start {
        path: String,
        index: u32,
        #[serde(with = "crate::ipc_number")]
        #[specta(type = crate::ipc_number::SafeU64)]
        generation: u64,
        #[serde(with = "crate::ipc_number")]
        #[specta(type = crate::ipc_number::SafeU64)]
        revision: u64,
        discard: bool,
    },
    View {
        id: u32,
    },
    Cancel {
        id: u32,
    },
    Accept {
        id: u32,
    },
}
#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionReply {
    id: u32,
    status: String,
    listing: Option<Listing>,
    error: Option<String>,
    snapshot: Option<Snapshot>,
    provenance: Option<Provenance>,
    cue_count: u32,
    font_count: u32,
    repaired_cues: Vec<u32>,
}
struct Job {
    id: u32,
    cancel: Cancellation,
    capture: Option<ExtractionCapture>,
    listing: Option<Listing>,
    result: Option<Prepared>,
    status: String,
    error: Option<String>,
}
impl Job {
    fn reply(&self) -> ExtractionReply {
        ExtractionReply {
            id: self.id,
            status: self.status.clone(),
            listing: self.listing.clone(),
            error: self.error.clone(),
            snapshot: None,
            provenance: self.result.as_ref().map(|p| p.provenance.clone()),
            cue_count: self
                .result
                .as_ref()
                .map_or(0, |p| p.parsed.document.cues().len() as u32),
            font_count: self.result.as_ref().map_or(0, |p| p.font_count),
            repaired_cues: self
                .result
                .as_ref()
                .map_or_else(Vec::new, |p| p.repaired_cues.clone()),
        }
    }
}
#[derive(Clone)]
pub struct Extraction {
    worker: Arc<Mutex<Worker>>,
    state: Arc<Mutex<Option<Job>>>,
}
impl Default for Extraction {
    fn default() -> Self {
        Self {
            worker: Arc::new(Mutex::new(Worker::new("subtitle-extraction"))),
            state: Arc::default(),
        }
    }
}
impl Extraction {
    pub fn shutdown(&self) {
        if let Ok(mut worker) = self.worker.lock() {
            worker.shutdown();
        }
    }
    fn apply(&self, documents: &Documents, action: ExtractionAction) -> Result<ExtractionReply> {
        match action {
            ExtractionAction::List { path } => self.start(documents, path, None),
            ExtractionAction::Start {
                path,
                index,
                generation,
                revision,
                discard,
            } => {
                let capture = documents.capture_extraction(generation, revision, discard)?;
                self.start(documents, path, Some((index, capture)))
            }
            ExtractionAction::View { id }
            | ExtractionAction::Cancel { id }
            | ExtractionAction::Accept { id } => {
                let mut state = self
                    .state
                    .lock()
                    .map_err(|_| "Extraction state unavailable")?;
                let job = state
                    .as_mut()
                    .filter(|j| j.id == id)
                    .ok_or("Extraction job expired")?;
                if matches!(action, ExtractionAction::Cancel { .. }) {
                    job.cancel.cancel();
                    job.result = None;
                    job.status = "cancelled".into();
                }
                let mut reply = job.reply();
                if matches!(action, ExtractionAction::Accept { .. }) {
                    if job.cancel.is_cancelled() {
                        return Err("Extraction cancelled".into());
                    }
                    let prepared = job.result.take().ok_or("Extraction is not ready")?;
                    reply.snapshot = Some(documents.accept_extraction(
                        job.capture.as_ref().ok_or("Missing extraction context")?,
                        prepared,
                    )?);
                    job.status = "accepted".into();
                    reply.status = job.status.clone();
                }
                Ok(reply)
            }
        }
    }
    fn start(
        &self,
        _documents: &Documents,
        path: String,
        selection: Option<(u32, ExtractionCapture)>,
    ) -> Result<ExtractionReply> {
        let mut worker = self
            .worker
            .lock()
            .map_err(|_| "Extraction worker unavailable")?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| "Extraction state unavailable")?;
        let id = match state.as_ref() {
            Some(job) => job
                .id
                .checked_add(1)
                .ok_or("Extraction job IDs exhausted; restart the app")?,
            None => 1,
        };
        if let Some(job) = state.as_mut() {
            job.cancel.cancel();
        }
        let cancel = Cancellation::default();
        let index = selection.as_ref().map(|(index, _)| *index);
        let job = Job {
            id,
            cancel: cancel.clone(),
            capture: selection.map(|(_, capture)| capture),
            listing: None,
            result: None,
            status: "running".into(),
            error: None,
        };
        let reply = job.reply();
        let shared = self.state.clone();
        worker.replace(cancel.clone(), move || {
            let result = (|| -> Result<(Option<Listing>, Option<Prepared>)> {
                let path = PathBuf::from(path)
                    .canonicalize()
                    .map_err(|e| e.to_string())?;
                let tools = Tools::discover().map_err(|e| e.to_string())?;
                if let Some(index) = index {
                    Ok((
                        None,
                        Some(subtitle_extraction::extract(
                            &tools,
                            &path,
                            index,
                            &cancel,
                            Duration::from_secs(1800),
                        )?),
                    ))
                } else {
                    Ok((
                        Some(
                            subtitle_tracks::list(&tools, &path, &cancel, Duration::from_secs(30))
                                .map_err(|e| e.to_string())?,
                        ),
                        None,
                    ))
                }
            })();
            if let Ok(mut state) = shared.lock()
                && let Some(job) = state
                    .as_mut()
                    .filter(|job| job.id == id && !cancel.is_cancelled())
            {
                match result {
                    Ok((listing, result)) => {
                        job.listing = listing;
                        job.result = result;
                        job.status = "ready".into();
                    }
                    Err(error) => {
                        job.error = Some(error);
                        job.status = "failed".into();
                    }
                }
            }
        })?;
        *state = Some(job);
        Ok(reply)
    }
}
#[tauri::command]
#[specta::specta]
pub async fn extraction_action(
    extraction: tauri::State<'_, Extraction>,
    documents: tauri::State<'_, Documents>,
    action: ExtractionAction,
) -> Result<ExtractionReply> {
    // All service entry points execute off the UI thread. Worker waits and joins
    // never occur while holding the document lock.
    let extraction = extraction.inner().clone();
    let documents = documents.inner().clone();
    tauri::async_runtime::spawn_blocking(move || extraction.apply(&documents, action))
        .await
        .map_err(|e| e.to_string())?
}
