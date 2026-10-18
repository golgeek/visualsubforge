//! Thin blocking-task adapters for Rust-owned style review.
use crate::{
    document::{Documents, Snapshot},
    ipc_number::SafeU64,
    quality_check::Key,
    text_review::Phase,
};
use serde::Serialize;
use tauri::Manager;
use vsf_application::authoring::styles::{StyleOperation, StyleSummary};

#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StyleFormField {
    pub name: String,
    pub value: String,
    pub editable: bool,
}
#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StyleForm {
    pub key: Key,
    pub fields: Vec<StyleFormField>,
}
#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StyleReview {
    pub id: SafeU64,
    pub key: Key,
    pub phase: Phase,
    pub operation: StyleOperation,
    pub summary: Option<StyleSummary>,
    pub changed: bool,
    pub error: Option<String>,
}
#[tauri::command]
#[specta::specta]
pub(crate) async fn style_form(
    app: tauri::AppHandle,
    generation: SafeU64,
    revision: SafeU64,
    name: Option<String>,
) -> Result<StyleForm, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Documents>()
            .style_form(generation.get(), revision.get(), name.as_deref())
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
#[specta::specta]
pub(crate) async fn style_start(
    app: tauri::AppHandle,
    generation: SafeU64,
    revision: SafeU64,
    operation: StyleOperation,
) -> Result<SafeU64, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Documents>()
            .style_start(generation.get(), revision.get(), operation)?
            .try_into()
            .map_err(str::to_owned)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
#[specta::specta]
pub(crate) async fn style_view(app: tauri::AppHandle, id: SafeU64) -> Result<StyleReview, String> {
    tauri::async_runtime::spawn_blocking(move || app.state::<Documents>().style_view(id.get()))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
#[specta::specta]
pub(crate) async fn style_cancel(app: tauri::AppHandle, id: SafeU64) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || app.state::<Documents>().style_cancel(id.get()))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
#[specta::specta]
pub(crate) async fn style_accept(app: tauri::AppHandle, id: SafeU64) -> Result<Snapshot, String> {
    tauri::async_runtime::spawn_blocking(move || app.state::<Documents>().style_accept(id.get()))
        .await
        .map_err(|e| e.to_string())?
}
