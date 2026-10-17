//! Generated transport only; the document owner handles snapshot and job lifetime.
use crate::{document::Documents, ipc_number::SafeU64, quality_check::Key};
use serde::Serialize;
use tauri::Manager;
use vsf_application::quality::{
    Finding,
    statistics::{Selection, Summary},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Phase {
    Preparing,
    Ready,
    Cancelling,
    Cancelled,
    Stale,
    Failed,
    Closed,
}
#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StatisticsView {
    pub id: SafeU64,
    pub key: Key,
    pub session: SafeU64,
    pub phase: Phase,
    pub error: Option<String>,
    pub summary: Option<Summary>,
    pub offset: u32,
    pub findings: Vec<Finding>,
}
#[tauri::command]
#[specta::specta]
pub(crate) async fn statistics_start(
    app: tauri::AppHandle,
    session: SafeU64,
    generation: SafeU64,
    revision: SafeU64,
    selection: Selection,
) -> Result<SafeU64, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<crate::media::Media>()
            .with_context(session.get(), |path| {
                let scenes = app
                    .state::<crate::scenes::Scenes>()
                    .capture_quality(session.get(), path.is_some())?;
                Ok(app
                    .state::<Documents>()
                    .statistics_start(generation.get(), revision.get(), scenes, selection)?
                    .try_into()?)
            })
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
#[specta::specta]
pub(crate) async fn statistics_view(
    app: tauri::AppHandle,
    id: SafeU64,
    offset: u32,
    limit: u32,
) -> Result<StatisticsView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let docs = app.state::<Documents>();
        let key = docs.statistics_context(id.get())?;
        let result = app
            .state::<crate::media::Media>()
            .with_context(key.session, |path| {
                if path.is_some() != key.attached {
                    return Err("Media changed after statistics; calculate again".into());
                }
                app.state::<crate::scenes::Scenes>()
                    .with_quality_current(key, || Ok(docs.statistics_view(id.get(), offset, limit)))
            });
        match result {
            Ok(view) => view,
            Err(error) => {
                let _ = docs.statistics_cancel(id.get());
                Err(error)
            }
        }
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
#[specta::specta]
pub(crate) async fn statistics_cancel(app: tauri::AppHandle, id: SafeU64) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Documents>().statistics_cancel(id.get())
    })
    .await
    .map_err(|e| e.to_string())?
}
