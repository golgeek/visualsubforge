//! Typed batch inputs; domain planning runs on the existing timing worker.
use crate::{
    document::{Documents, Snapshot},
    ipc_number::SafeU64,
    quality_check::Key,
    timing_review::{Phase, Purpose, with_current},
};
use serde::{Deserialize, Serialize};
use subtitle_core::{CueId, Milliseconds};
use tauri::Manager;
use vsf_application::batch_timing as batch;

#[derive(Deserialize, specta::Type)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub(crate) enum Scope {
    Selected { cue_ids: Vec<SafeU64> },
    All,
    Following { cue_id: SafeU64 },
    AtOrAfter { position_ms: SafeU64 },
}
#[derive(Deserialize, specta::Type)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub(crate) enum Change {
    Shift { earlier: bool, amount_ms: SafeU64 },
    Align { playhead_ms: SafeU64 },
}
#[derive(Deserialize, specta::Type)]
#[serde(deny_unknown_fields)]
pub(crate) struct BatchRequest {
    pub scope: Scope,
    pub change: Change,
}
impl BatchRequest {
    fn domain(self) -> Result<batch::Request, String> {
        let scope = match self.scope {
            Scope::Selected { cue_ids } => {
                batch::Scope::Selected(cue_ids.into_iter().map(|i| CueId(i.get())).collect())
            }
            Scope::All => batch::Scope::All,
            Scope::Following { cue_id } => batch::Scope::Following(CueId(cue_id.get())),
            Scope::AtOrAfter { position_ms } => {
                batch::Scope::AtOrAfter(Milliseconds::new(position_ms.get())?)
            }
        };
        let change = match self.change {
            Change::Shift { earlier, amount_ms } => {
                let amount = Milliseconds::new(amount_ms.get())?.get() as i64;
                batch::Change::Shift(if earlier { -amount } else { amount })
            }
            Change::Align { playhead_ms } => {
                batch::Change::Align(Milliseconds::new(playhead_ms.get())?)
            }
        };
        Ok(batch::Request { scope, change })
    }
}
#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BatchRow {
    pub cue_id: String,
    pub cue_index: u32,
    pub old_start_ms: SafeU64,
    pub old_end_ms: SafeU64,
    pub new_start_ms: SafeU64,
    pub new_end_ms: SafeU64,
}
#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BatchView {
    pub id: SafeU64,
    pub key: Key,
    pub session: SafeU64,
    pub phase: Phase,
    pub error: Option<String>,
    pub total: u32,
    pub changed: u32,
    pub offset: u32,
    pub offset_ms: Option<String>,
    pub aligned_ms: Option<SafeU64>,
    pub rows: Vec<BatchRow>,
}
#[tauri::command]
#[specta::specta]
pub(crate) async fn batch_timing_start(
    app: tauri::AppHandle,
    session: SafeU64,
    generation: SafeU64,
    revision: SafeU64,
    request: BatchRequest,
) -> Result<SafeU64, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let request = request.domain()?;
        app.state::<crate::media::Media>()
            .with_context(session.get(), |path| {
                let scenes = app
                    .state::<crate::scenes::Scenes>()
                    .capture_quality(session.get(), path.is_some())?;
                Ok(app
                    .state::<Documents>()
                    .batch_timing_start(generation.get(), revision.get(), scenes, request)?
                    .try_into()?)
            })
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
#[specta::specta]
pub(crate) async fn batch_timing_view(
    app: tauri::AppHandle,
    id: SafeU64,
    offset: u32,
    limit: u32,
) -> Result<BatchView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        with_current(&app, id.get(), Purpose::Batch, |d| {
            d.batch_timing_view(id.get(), offset, limit)
        })
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
#[specta::specta]
pub(crate) async fn batch_timing_accept(
    app: tauri::AppHandle,
    id: SafeU64,
) -> Result<Snapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        with_current(&app, id.get(), Purpose::Batch, |d| {
            d.batch_timing_accept(id.get())
        })
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
#[specta::specta]
pub(crate) async fn batch_timing_cancel(app: tauri::AppHandle, id: SafeU64) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Documents>()
            .timing_repair_cancel(id.get(), Purpose::Batch)
    })
    .await
    .map_err(|e| e.to_string())?
}
