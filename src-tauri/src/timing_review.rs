//! Thin desktop composition for immutable shared timing proposals. All capture,
//! projection and acceptance work is off the UI thread.
use crate::{
    document::{Documents, Snapshot, TimingStartError},
    ipc_number::SafeU64,
};
use serde::{Deserialize, Serialize};
use tauri::Manager;
use vsf_application::quality::{
    scene::SceneCoverage,
    timing_fix::{
        self, Families, Options,
        review::{Counts, ManualRow, Row},
    },
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Purpose {
    Fix,
    Batch,
    CueFixes,
}
#[derive(Clone, Copy, Debug, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Section {
    Changes,
    Manual,
}
#[derive(Clone, Default, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Input {
    pub families: Families,
    pub options: Options,
    pub scope: Option<Vec<SafeU64>>,
    #[serde(default)]
    pub toggled: Vec<u32>,
    #[serde(default)]
    pub omit_trades: bool,
}
#[cfg(test)]
impl Input {
    pub(crate) fn preset(gaps: bool, speed: bool, shots: bool) -> Self {
        Self {
            families: Families { gaps, speed, shots },
            ..Self::default()
        }
    }
}
impl Input {
    pub fn domain(&self) -> Result<timing_fix::Request, String> {
        if self.scope.as_ref().is_some_and(|s| s.len() > 100000) || self.toggled.len() > 100000 {
            return Err("Timing review exceeds 100,000 cues".into());
        }
        self.options.validate().map_err(|e| e.to_string())?;
        Ok(timing_fix::Request {
            families: self.families,
            options: self.options,
            scope: self.scope.as_ref().map(|ids| {
                ids.iter()
                    .map(|id| subtitle_core::CueId(id.get()))
                    .collect()
            }),
        })
    }
}
#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FixView {
    pub key: crate::quality_check::Key,
    pub profile: vsf_application::quality::Profile,
    pub profile_hash: String,
    pub id: SafeU64,
    pub phase: Phase,
    pub error: Option<String>,
    pub selection_error: Option<String>,
    pub selection_conflicts: Vec<u32>,
    pub coverage: Option<SceneCoverage>,
    pub before: Counts,
    pub after: Counts,
    pub total: u32,
    pub changed: u32,
    pub manual_count: u32,
    pub trades: u32,
    pub withheld_all: bool,
    pub without_trades_toggles: Vec<u32>,
    pub offset: u32,
    pub rows: Vec<Row>,
    pub manual: Vec<ManualRow>,
}
#[tauri::command]
#[specta::specta]
pub(crate) async fn timing_fix_start(
    app: tauri::AppHandle,
    session: SafeU64,
    generation: SafeU64,
    revision: SafeU64,
    input: Input,
) -> Result<SafeU64, TimingStartError> {
    tauri::async_runtime::spawn_blocking(move || {
        input.domain()?;
        start_in_context(&app, session, |documents, scenes| {
            documents.timing_fix_start(generation.get(), revision.get(), scenes, input)
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

// The media owner guards its selection while Rust captures scenes and admits
// the document job. Keep its context failures distinct from typed admission.
pub(crate) fn start_in_context(
    app: &tauri::AppHandle,
    session: SafeU64,
    start: impl FnOnce(&Documents, crate::scenes::SceneCapture) -> Result<u64, TimingStartError>,
) -> Result<SafeU64, TimingStartError> {
    let id = app
        .state::<crate::media::Media>()
        .with_context(session.get(), |path| {
            let scenes = app
                .state::<crate::scenes::Scenes>()
                .capture_quality(session.get(), path.is_some())?;
            Ok(start(&app.state::<Documents>(), scenes))
        })??;
    Ok(id.try_into()?)
}
pub(crate) fn with_current<T>(
    app: &tauri::AppHandle,
    id: u64,
    purpose: Purpose,
    operation: impl FnOnce(&Documents) -> Result<T, String>,
) -> Result<T, String> {
    let documents = app.state::<Documents>();
    let key = documents.timing_repair_context(id, purpose)?;
    let result = app
        .state::<crate::media::Media>()
        .with_context(key.session, |path| {
            if path.is_some() != key.attached {
                return Err("Media changed after timing preview".into());
            }
            app.state::<crate::scenes::Scenes>()
                .with_quality_current(key, || Ok(operation(&documents)))
        });
    match result {
        Ok(result) => result,
        Err(error) => {
            let _ = documents.timing_repair_cancel(id, purpose);
            Err(error)
        }
    }
}
#[tauri::command]
#[specta::specta]
pub(crate) async fn timing_fix_view(
    app: tauri::AppHandle,
    id: SafeU64,
    offset: u32,
    limit: u32,
    section: Section,
    preview_ids: Option<Vec<SafeU64>>,
) -> Result<FixView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let preview_ids = crate::review_selection::preview_ids(preview_ids)?;
        with_current(&app, id.get(), Purpose::Fix, |documents| {
            documents.timing_fix_view_for_cues(
                id.get(),
                offset,
                limit,
                section,
                preview_ids.as_ref(),
            )
        })
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
#[specta::specta]
pub(crate) async fn timing_fix_accept(
    app: tauri::AppHandle,
    id: SafeU64,
) -> Result<Snapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        with_current(&app, id.get(), Purpose::Fix, |documents| {
            documents.timing_repair_accept(id.get(), Purpose::Fix)
        })
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
#[specta::specta]
pub(crate) async fn timing_fix_cancel(app: tauri::AppHandle, id: SafeU64) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Documents>()
            .timing_repair_cancel(id.get(), Purpose::Fix)
    })
    .await
    .map_err(|e| e.to_string())?
}
