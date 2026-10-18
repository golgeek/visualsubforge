//! Thin adapters; font IO and interpretation run on the document review worker.
use crate::{
    document::{Documents, Snapshot},
    ipc_number::SafeU64,
    quality_check::Key,
    text_review::Phase,
};
use serde::{Deserialize, Serialize};
use tauri::Manager;
use vsf_application::authoring::attachments::{AttachmentInventory, AttachmentSummary};

#[derive(Clone, Debug, Deserialize, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum FontOperation {
    Inspect,
    Add { path: String },
    Replace { id: u32, path: String },
    Remove { id: u32 },
}
#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FontReview {
    pub id: SafeU64,
    pub key: Key,
    pub phase: Phase,
    pub operation: FontOperation,
    pub inventory: Option<AttachmentInventory>,
    pub summary: Option<AttachmentSummary>,
    pub changed: bool,
    pub error: Option<String>,
}
#[tauri::command]
#[specta::specta]
pub(crate) async fn font_start(
    app: tauri::AppHandle,
    generation: SafeU64,
    revision: SafeU64,
    operation: FontOperation,
) -> Result<SafeU64, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Documents>()
            .font_start(generation.get(), revision.get(), operation)?
            .try_into()
            .map_err(str::to_owned)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
#[specta::specta]
pub(crate) async fn font_view(app: tauri::AppHandle, id: SafeU64) -> Result<FontReview, String> {
    tauri::async_runtime::spawn_blocking(move || app.state::<Documents>().font_view(id.get()))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
#[specta::specta]
pub(crate) async fn font_cancel(app: tauri::AppHandle, id: SafeU64) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || app.state::<Documents>().font_cancel(id.get()))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
#[specta::specta]
pub(crate) async fn font_accept(app: tauri::AppHandle, id: SafeU64) -> Result<Snapshot, String> {
    tauri::async_runtime::spawn_blocking(move || app.state::<Documents>().font_accept(id.get()))
        .await
        .map_err(|e| e.to_string())?
}
