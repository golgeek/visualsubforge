//! Typed desktop transport. Documents owns admission, proposals and acceptance.
use crate::{
    document::{Documents, Snapshot},
    ipc_number::{SafeU64, finite::FiniteF64},
    quality_check::Key,
    scenes::SceneKey,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tauri::Manager;
use vsf_application::quality::scene::SceneCoverage;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(untagged)]
pub(crate) enum Scalar {
    Boolean(bool),
    Number(FiniteF64),
    Text(String),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Mode {
    Check,
    Fix,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Input {
    pub mode: Mode,
    /// Empty scope means the whole accepted document. These are stable cue IDs.
    pub scope: Vec<String>,
    pub selection: Vec<String>,
    pub cursor_ms: Option<SafeU64>,
    pub settings: BTreeMap<String, Scalar>,
    pub cps_target: Option<FiniteF64>,
    pub rs_target_index: Option<u32>,
    /// Explicit plugin context; VSF has no corresponding global filter setting.
    pub scene_filter_offset_ms: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Phase {
    Preparing,
    Reviewing,
    Accepting,
    Cancelling,
    Cancelled,
    Stale,
    Failed,
    Accepted,
    Closed,
}
#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TextPreview {
    pub text: String,
    pub total_bytes: u32,
    pub truncated: bool,
}
#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Content {
    pub start_ms: SafeU64,
    pub end_ms: SafeU64,
    pub text: TextPreview,
}
#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Row {
    pub cue_id: String,
    pub cue_index: u32,
    pub before: Content,
    pub after: Content,
    pub outside_scope: bool,
    pub warning: Option<String>,
}
#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Finding {
    pub cue_id: String,
    pub message: String,
}
#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Preferences {
    pub minimum_blank_ms: SafeU64,
    pub minimum_duration_ms: SafeU64,
    pub maximum_duration_ms: SafeU64,
    pub cps_target: FiniteF64,
    pub rs_target_index: Option<u32>,
}
#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct View {
    pub id: SafeU64,
    pub key: Key,
    pub session: SafeU64,
    pub phase: Phase,
    pub mode: Mode,
    pub path: String,
    pub package_id: Option<String>,
    pub fingerprint: Option<String>,
    pub name: Option<String>,
    pub can_fix: bool,
    pub error: Option<String>,
    pub scope_count: u32,
    pub selection_count: u32,
    pub cursor_ms: Option<SafeU64>,
    pub preferences: Preferences,
    pub scene_coverage: SceneCoverage,
    pub scene_filter_offset_ms: u32,
    pub settings: BTreeMap<String, Scalar>,
    pub logs: Vec<String>,
    pub findings_total: u32,
    pub changes_total: u32,
    pub findings_offset: u32,
    pub changes_offset: u32,
    pub findings: Vec<Finding>,
    pub rows: Vec<Row>,
}
#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TextChunk {
    pub offset: u32,
    pub next_offset: u32,
    pub total_bytes: u32,
    pub text: String,
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn plugin_start(
    app: tauri::AppHandle,
    session: SafeU64,
    generation: SafeU64,
    revision: SafeU64,
    path: String,
    input: Input,
) -> Result<SafeU64, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<crate::media::Media>()
            .with_context(session.get(), |path_media| {
                let scenes = app
                    .state::<crate::scenes::Scenes>()
                    .capture_quality(session.get(), path_media.is_some())?;
                app.state::<Documents>()
                    .plugin_start(generation.get(), revision.get(), scenes, path, input)?
                    .try_into()
                    .map_err(str::to_owned)
            })
    })
    .await
    .map_err(|e| e.to_string())?
}
fn with_current<T>(
    app: &tauri::AppHandle,
    id: u64,
    key: SceneKey,
    operation: impl FnOnce(&Documents) -> Result<T, String>,
) -> Result<T, String> {
    let docs = app.state::<Documents>();
    with_current_owners(
        &app.state::<crate::media::Media>(),
        &app.state::<crate::scenes::Scenes>(),
        &docs,
        id,
        key,
        operation,
    )
}
pub(crate) fn with_current_owners<T>(
    media: &crate::media::Media,
    scenes: &crate::scenes::Scenes,
    docs: &Documents,
    id: u64,
    key: SceneKey,
    operation: impl FnOnce(&Documents) -> Result<T, String>,
) -> Result<T, String> {
    let result = media.with_context(key.session, |path| {
        if path.is_some() != key.attached {
            return Err("Media changed after plugin capture; run again".into());
        }
        scenes.with_quality_current(key, || Ok(operation(docs)))
    });
    match result {
        Ok(value) => value,
        Err(error) => {
            let _ = docs.plugin_invalidate(id);
            Err(error)
        }
    }
}
#[tauri::command]
#[specta::specta]
pub(crate) async fn plugin_view(
    app: tauri::AppHandle,
    id: SafeU64,
    findings_offset: u32,
    changes_offset: u32,
    limit: u32,
) -> Result<View, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let key = app.state::<Documents>().plugin_context(id.get())?;
        with_current(&app, id.get(), key, |docs| {
            docs.plugin_view(id.get(), findings_offset, changes_offset, limit)
        })
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
#[specta::specta]
pub(crate) async fn plugin_text(
    app: tauri::AppHandle,
    id: SafeU64,
    row: u32,
    after: bool,
    offset: u32,
    limit: u32,
) -> Result<TextChunk, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let key = app.state::<Documents>().plugin_context(id.get())?;
        with_current(&app, id.get(), key, |docs| {
            docs.plugin_text(id.get(), row, after, offset, limit)
        })
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
#[specta::specta]
pub(crate) async fn plugin_cancel(app: tauri::AppHandle, id: SafeU64) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || app.state::<Documents>().plugin_cancel(id.get()))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
#[specta::specta]
pub(crate) async fn plugin_accept(
    app: tauri::AppHandle,
    id: SafeU64,
    input: Input,
) -> Result<Snapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let docs = app.state::<Documents>();
        let pending = docs.plugin_begin_accept(id.get(), &input)?;
        // Package IO takes no media, scene or document mutex. The reservation
        // prevents replacement and makes shutdown wait for this handler's end.
        let pending = pending.verify()?;
        with_current(&app, id.get(), pending.scene_key(), |docs| {
            docs.plugin_commit(&pending)
        })
    })
    .await
    .map_err(|e| e.to_string())?
}
