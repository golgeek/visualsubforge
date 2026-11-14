//! Selected-cue fixes reuse the bounded timing-proposal worker and acceptance
//! lock order. No second accepted editor or additional background worker.
use crate::{
    document::{Documents, Snapshot},
    ipc_number::SafeU64,
    quality_check::Key,
    timing_review::{self, Phase, Purpose},
};
use serde::Serialize;
use tauri::Manager;
#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct View {
    pub id: SafeU64,
    pub key: Key,
    pub phase: Phase,
    pub error: Option<String>,
    pub cue_id: Option<String>,
    pub fixed: u32,
    pub remaining: u32,
}
#[tauri::command]
#[specta::specta]
pub(crate) async fn cue_fixes_start(
    app: tauri::AppHandle,
    session: SafeU64,
    generation: SafeU64,
    revision: SafeU64,
    cue_id: SafeU64,
) -> Result<SafeU64, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<crate::media::Media>()
            .with_context(session.get(), |path| {
                let scenes = app
                    .state::<crate::scenes::Scenes>()
                    .capture_quality(session.get(), path.is_some())?;
                Ok(app
                    .state::<Documents>()
                    .cue_fixes_start(
                        generation.get(),
                        revision.get(),
                        scenes,
                        subtitle_core::CueId(cue_id.get()),
                    )?
                    .try_into()?)
            })
    })
    .await
    .map_err(|error| error.to_string())?
}
#[tauri::command]
#[specta::specta]
pub(crate) async fn cue_fixes_view(app: tauri::AppHandle, id: SafeU64) -> Result<View, String> {
    tauri::async_runtime::spawn_blocking(move || {
        timing_review::with_current(&app, id.get(), Purpose::CueFixes, |documents| {
            documents.cue_fixes_view(id.get())
        })
    })
    .await
    .map_err(|error| error.to_string())?
}
#[tauri::command]
#[specta::specta]
pub(crate) async fn cue_fixes_accept(
    app: tauri::AppHandle,
    id: SafeU64,
) -> Result<Snapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        timing_review::with_current(&app, id.get(), Purpose::CueFixes, |documents| {
            documents.cue_fixes_accept(id.get())
        })
    })
    .await
    .map_err(|error| error.to_string())?
}
#[tauri::command]
#[specta::specta]
pub(crate) async fn cue_fixes_cancel(app: tauri::AppHandle, id: SafeU64) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Documents>()
            .timing_repair_cancel(id.get(), Purpose::CueFixes)
    })
    .await
    .map_err(|error| error.to_string())?
}
