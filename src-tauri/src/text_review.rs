//! Thin IPC for shared text transformations and document-owned proposals.
use crate::{
    document::{Documents, Snapshot},
    ipc_number::SafeU64,
    quality_check::Key,
};
use serde::{Deserialize, Serialize};
use tauri::Manager;
pub(crate) use vsf_application::text::{Format, Formatted, Operation};

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
pub(crate) struct Match {
    pub start: u32,
    pub end: u32,
    pub skipped: Option<String>,
}
#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Row {
    pub cue_id: SafeU64,
    pub cue_index: u32,
    pub before: String,
    pub after: Option<String>,
    pub skipped: Option<String>,
    pub matches: Vec<Match>,
    pub match_count: u32,
}
#[derive(Clone, Copy, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Target {
    pub cue_index: u32,
    pub match_index: u32,
}
#[derive(Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct FormattingContext {
    pub generation: SafeU64,
    pub revision: SafeU64,
    pub cue_id: SafeU64,
    pub style_name: String,
}
#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TextView {
    pub id: SafeU64,
    pub key: Key,
    pub phase: Phase,
    pub profile_name: String,
    pub profile_hash: String,
    pub operation: Operation,
    pub error: Option<String>,
    pub total: u32,
    pub changed: u32,
    pub matches: u32,
    pub before_findings: u32,
    pub after_findings: u32,
    pub offset: u32,
    pub rows: Vec<Row>,
}
#[tauri::command]
#[specta::specta]
pub(crate) async fn text_start(
    app: tauri::AppHandle,
    generation: SafeU64,
    revision: SafeU64,
    operation: Operation,
) -> Result<SafeU64, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Documents>()
            .text_start(generation.get(), revision.get(), operation)?
            .try_into()
            .map_err(str::to_owned)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
#[specta::specta]
pub(crate) async fn text_view(
    app: tauri::AppHandle,
    id: SafeU64,
    offset: u32,
    limit: u32,
) -> Result<TextView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Documents>().text_view(id.get(), offset, limit)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
#[specta::specta]
pub(crate) async fn text_matches(
    app: tauri::AppHandle,
    id: SafeU64,
    cue_index: u32,
    offset: u32,
    limit: u32,
) -> Result<Vec<Match>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Documents>()
            .text_matches(id.get(), cue_index, offset, limit)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
#[specta::specta]
pub(crate) async fn text_cancel(app: tauri::AppHandle, id: SafeU64) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || app.state::<Documents>().text_cancel(id.get()))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
#[specta::specta]
pub(crate) async fn text_accept(
    app: tauri::AppHandle,
    id: SafeU64,
    target: Option<Target>,
) -> Result<Snapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Documents>().text_accept(id.get(), target)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
#[specta::specta]
pub(crate) async fn text_format(
    app: tauri::AppHandle,
    text: String,
    start: u32,
    end: u32,
    style: Format,
    context: Option<FormattingContext>,
) -> Result<Formatted, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Documents>()
            .format_text(&text, start, end, style, context.as_ref())
    })
    .await
    .map_err(|e| e.to_string())?
}
