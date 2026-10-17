//! Thin desktop composition for immutable shared scene proposals. All capture,
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
    scene::{SceneCoverage, Skip},
};

pub(crate) use crate::gap_review::Phase;
#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Row {
    pub cue_id: String,
    pub cue_index: u32,
    pub new_start_ms: Option<SafeU64>,
    pub new_end_ms: Option<SafeU64>,
    pub skipped: Option<Skip>,
    pub explanation: Option<String>,
    pub before: Metric,
    pub after: Metric,
}
#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SceneRepairView {
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
pub(crate) async fn scene_repair_start(
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
                    .scene_repair_start(generation.get(), revision.get(), scenes)?
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
    let key = documents.scene_repair_context(id)?;
    let result = app
        .state::<crate::media::Media>()
        .with_context(key.session, |path| {
            if path.is_some() != key.attached {
                return Err("Media changed after scene preview".into());
            }
            app.state::<crate::scenes::Scenes>()
                .with_quality_current(key, || Ok(operation(&documents)))
        });
    match result {
        Ok(result) => result,
        Err(error) => {
            let _ = documents.scene_repair_cancel(id);
            Err(error)
        }
    }
}
#[tauri::command]
#[specta::specta]
pub(crate) async fn scene_repair_view(
    app: tauri::AppHandle,
    id: SafeU64,
    offset: u32,
    limit: u32,
) -> Result<SceneRepairView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        with_current(&app, id.get(), |documents| {
            documents.scene_repair_view(id.get(), offset, limit)
        })
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
#[specta::specta]
pub(crate) async fn scene_repair_accept(
    app: tauri::AppHandle,
    id: SafeU64,
) -> Result<Snapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        with_current(&app, id.get(), |documents| {
            documents.scene_repair_accept(id.get())
        })
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
#[specta::specta]
pub(crate) async fn scene_repair_cancel(app: tauri::AppHandle, id: SafeU64) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Documents>().scene_repair_cancel(id.get())
    })
    .await
    .map_err(|e| e.to_string())?
}
