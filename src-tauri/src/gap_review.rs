//! Thin desktop composition for immutable shared gap proposals. All capture,
//! projection and acceptance work is off the UI thread.
use crate::{
    document::{Documents, Snapshot},
    ipc_number::SafeU64,
    quality_check::{Key, Metric},
};
use serde::Serialize;
use tauri::Manager;
use vsf_application::quality::{
    Profile,
    gap::{SceneCoverage, Skip},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Phase {
    Preparing,
    Reviewing,
    Cancelling,
    Cancelled,
    Stale,
    Failed,
    Accepted,
    Closed,
}
#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Row {
    pub cue_id: String,
    pub cue_index: u32,
    pub next_cue_index: u32,
    pub new_end_ms: Option<SafeU64>,
    pub skipped: Option<Skip>,
    pub explanation: Option<String>,
    pub before: Metric,
    pub after: Metric,
    pub next_before: Metric,
    pub next_after: Metric,
}
#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GapView {
    pub id: SafeU64,
    pub key: Key,
    pub session: SafeU64,
    pub phase: Phase,
    pub profile: Profile,
    pub profile_hash: String,
    pub error: Option<String>,
    pub coverage: Option<SceneCoverage>,
    pub total: u32,
    pub changed: u32,
    pub skipped: u32,
    pub before_findings: u32,
    pub after_findings: u32,
    pub offset: u32,
    pub rows: Vec<Row>,
}
#[tauri::command]
#[specta::specta]
pub(crate) async fn gap_start(
    app: tauri::AppHandle,
    session: SafeU64,
    generation: SafeU64,
    revision: SafeU64,
) -> Result<SafeU64, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<crate::media::Media>()
            .with_context(session.get(), |path| {
                let scenes = app
                    .state::<crate::scenes::Scenes>()
                    .capture_quality(session.get(), path.is_some())?;
                Ok(app
                    .state::<Documents>()
                    .gap_start(generation.get(), revision.get(), scenes)?
                    .try_into()?)
            })
    })
    .await
    .map_err(|e| e.to_string())?
}
fn with_current<T>(
    app: &tauri::AppHandle,
    id: u64,
    operation: impl FnOnce(&Documents) -> Result<T, String>,
) -> Result<T, String> {
    let documents = app.state::<Documents>();
    let key = documents.gap_context(id)?;
    let result = app
        .state::<crate::media::Media>()
        .with_context(key.session, |path| {
            if path.is_some() != key.attached {
                return Err("Media changed after gap preview".into());
            }
            app.state::<crate::scenes::Scenes>()
                .with_quality_current(key, || Ok(operation(&documents)))
        });
    match result {
        Ok(result) => result,
        Err(error) => {
            let _ = documents.gap_cancel(id);
            Err(error)
        }
    }
}
#[tauri::command]
#[specta::specta]
pub(crate) async fn gap_view(
    app: tauri::AppHandle,
    id: SafeU64,
    offset: u32,
    limit: u32,
) -> Result<GapView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        with_current(&app, id.get(), |documents| {
            documents.gap_view(id.get(), offset, limit)
        })
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
#[specta::specta]
pub(crate) async fn gap_accept(
    app: tauri::AppHandle,
    id: SafeU64,
    acknowledge_no_media: bool,
) -> Result<Snapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        with_current(&app, id.get(), |documents| {
            documents.gap_accept(id.get(), acknowledge_no_media)
        })
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
#[specta::specta]
pub(crate) async fn gap_cancel(app: tauri::AppHandle, id: SafeU64) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || app.state::<Documents>().gap_cancel(id.get()))
        .await
        .map_err(|e| e.to_string())?
}
