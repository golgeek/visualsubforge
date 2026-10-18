//! Typed conversion review/export adapters. Preparation and publication reuse the
//! document review owner; handlers never perform filesystem work on the UI thread.
use crate::{document::Documents, ipc_number::SafeU64, quality_check::Key};
use serde::Serialize;
use tauri::Manager;
use vsf_application::conversion::{ConversionOptions, ConversionReport};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ConversionPhase {
    Preparing,
    Reviewing,
    Publishing,
    Published,
    Cancelling,
    Cancelled,
    Stale,
    Failed,
    Closed,
}
#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConversionReview {
    pub id: SafeU64,
    pub key: Key,
    pub phase: ConversionPhase,
    pub options: ConversionOptions,
    pub report: Option<ConversionReport>,
    pub output_path: Option<String>,
    pub error: Option<String>,
}
#[tauri::command]
#[specta::specta]
pub(crate) async fn conversion_start(
    app: tauri::AppHandle,
    generation: SafeU64,
    revision: SafeU64,
    options: ConversionOptions,
) -> Result<SafeU64, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Documents>()
            .conversion_start(generation.get(), revision.get(), options)?
            .try_into()
            .map_err(str::to_owned)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
#[specta::specta]
pub(crate) async fn conversion_view(
    app: tauri::AppHandle,
    id: SafeU64,
) -> Result<ConversionReview, String> {
    tauri::async_runtime::spawn_blocking(move || app.state::<Documents>().conversion_view(id.get()))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
#[specta::specta]
pub(crate) async fn conversion_cancel(app: tauri::AppHandle, id: SafeU64) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Documents>().conversion_cancel(id.get())
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
#[specta::specta]
pub(crate) async fn conversion_publish(
    app: tauri::AppHandle,
    id: SafeU64,
    path: String,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Documents>().conversion_publish(id.get(), &path)
    })
    .await
    .map_err(|e| e.to_string())?
}
