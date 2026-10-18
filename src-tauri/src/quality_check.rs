//! Thin generated IPC for the document-owned, bounded quality worker.
use serde::{Deserialize, Serialize};
use tauri::Manager;
use vsf_application::quality::{CueMetrics, Finding, Profile, Rule, Severity};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Key {
    pub generation: crate::ipc_number::SafeU64,
    pub revision: crate::ipc_number::SafeU64,
    pub epoch: crate::ipc_number::SafeU64,
    pub loaded: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Draft {
    pub cue_id: crate::ipc_number::SafeU64,
    pub start_ms: crate::ipc_number::SafeU64,
    pub end_ms: crate::ipc_number::SafeU64,
    pub text: String,
}
#[derive(Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum DraftChange {
    Clear,
    Update { value: Draft },
}
#[derive(Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Request {
    pub session: crate::ipc_number::SafeU64,
    pub generation: crate::ipc_number::SafeU64,
    pub revision: crate::ipc_number::SafeU64,
    pub run: bool,
    pub restart: bool,
    pub offset: u32,
    pub limit: u32,
    pub metrics_offset: u32,
    pub metrics_limit: u32,
    pub selected: Option<crate::ipc_number::SafeU64>,
    pub rule: Option<Rule>,
    pub severity: Option<Severity>,
    pub draft: Option<DraftChange>,
}
#[derive(Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Metric {
    pub data: CueMetrics,
    pub line_count: u32,
    pub longest_line: u32,
    pub lines_truncated: bool,
}
impl From<&CueMetrics> for Metric {
    fn from(row: &CueMetrics) -> Self {
        // Do not clone an arbitrarily long line vector just to truncate it.
        let data = CueMetrics {
            cue_id: row.cue_id.clone(),
            cue_index: row.cue_index,
            start_ms: row.start_ms,
            end_ms: row.end_ms,
            duration_ms: row.duration_ms,
            reading_start_ms: row.reading_start_ms,
            effective_duration_ms: row.effective_duration_ms,
            characters: row.characters,
            line_characters: row.line_characters.iter().take(20).copied().collect(),
            empty: row.empty,
            cps: row.cps.clone(),
            rs: row.rs.clone(),
            rating: row.rating,
            next_cue_id: row.next_cue_id.clone(),
            gap_ms: row.gap_ms,
        };
        Self {
            data,
            line_count: row.line_characters.len() as u32,
            longest_line: row.line_characters.iter().copied().max().unwrap_or(0),
            lines_truncated: row.line_characters.len() > 20,
        }
    }
}
#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct View {
    pub key: Key,
    pub id: crate::ipc_number::SafeU64,
    pub phase: Phase,
    pub profile: Profile,
    pub profile_hash: Option<String>,
    pub scene_coverage: Option<vsf_application::quality::scene::SceneCoverage>,
    pub language_coverage: Option<vsf_application::language::Coverage>,
    pub error: Option<String>,
    pub findings_count: u32,
    pub affected_cues: u32,
    pub filtered_count: u32,
    pub filtered_cues: u32,
    pub offset: u32,
    pub findings: Vec<Finding>,
    pub metrics: Vec<Metric>,
    pub selected: Option<Metric>,
    pub previous_cue: Option<String>,
    pub next_cue: Option<String>,
    pub draft: Option<Metric>,
    pub draft_checking: bool,
    pub draft_error: Option<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Phase {
    Idle,
    Checking,
    Ready,
    Cancelling,
    Cancelled,
    Failed,
    Closed,
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn quality_read(app: tauri::AppHandle, request: Request) -> Result<View, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<crate::media::Media>()
            .with_context(request.session.get(), |path| {
                let scenes = app.state::<crate::scenes::Scenes>();
                let capture = scenes.capture_quality(request.session.get(), path.is_some())?;
                scenes.with_quality_current(capture.key, || {
                    app.state::<crate::document::Documents>()
                        .quality_read_scenes(request, capture)
                })
            })
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
#[specta::specta]
pub(crate) async fn quality_cancel(
    app: tauri::AppHandle,
    id: crate::ipc_number::SafeU64,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<crate::document::Documents>()
            .quality_cancel(id.get())
    })
    .await
    .map_err(|e| e.to_string())?
}
