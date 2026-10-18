//! Typed projections and off-UI adapters for the project import owner.
use super::*;
use crate::{document::Documents, ipc_number::SafeU64, project::Projects};
use serde::{Deserialize, Serialize};
use tauri::Manager;
use vsf_application::legacy_project as engine;

#[derive(Clone, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LegacyImportRequest {
    pub path: String,
    pub media_path: Option<String>,
    pub subtitle_path: Option<String>,
}
impl LegacyImportRequest {
    pub(super) fn validate(&self) -> Result<()> {
        for path in std::iter::once(&self.path)
            .chain(self.media_path.iter())
            .chain(self.subtitle_path.iter())
        {
            if path.is_empty() || path.len() > 32_768 || path.contains('\0') {
                return Err("Choose a nonempty local path with a supported length".into());
            }
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum LegacyImportPhase {
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
#[derive(Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LegacyImportReference {
    pub field: String,
    pub original: Option<String>,
    pub candidate: Option<String>,
    pub resolved: Option<String>,
    pub overridden: bool,
    pub issue: Option<String>,
}
impl From<&engine::ReferenceStatus> for LegacyImportReference {
    fn from(value: &engine::ReferenceStatus) -> Self {
        Self {
            field: value.field.into(),
            original: value.original.clone(),
            candidate: value
                .candidate
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned()),
            resolved: value
                .resolved
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned()),
            overridden: value.overridden,
            issue: value.issue.clone(),
        }
    }
}
#[derive(Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LegacyImportWarning {
    pub field: String,
    pub line: Option<u32>,
    pub value: Option<String>,
    pub message: String,
}
#[derive(Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LegacyImportReport {
    pub source_path: String,
    pub encoding: String,
    pub media: LegacyImportReference,
    pub subtitles: LegacyImportReference,
    pub subtitle_format: Option<String>,
    pub cue_count: Option<SafeU64>,
    pub position_ms: SafeU64,
    pub waveform_start_ms: SafeU64,
    pub waveform_span_ms: SafeU64,
    pub quality_profile_name: String,
    pub warnings: Vec<LegacyImportWarning>,
    pub can_publish: bool,
}
impl LegacyImportReport {
    pub(super) fn from_inspection(
        input: &engine::Inspection,
        quality_profile_name: String,
        can_publish: bool,
    ) -> Result<Self> {
        Ok(Self {
            source_path: input.source_path.to_string_lossy().into_owned(),
            encoding: match input.project.encoding {
                engine::Encoding::Utf8 => "UTF-8",
                engine::Encoding::Utf16Le => "UTF-16LE",
                engine::Encoding::Utf16Be => "UTF-16BE",
            }
            .into(),
            media: (&input.media).into(),
            subtitles: (&input.subtitles).into(),
            subtitle_format: input.subtitle_format.map(|f| f.extension().into()),
            cue_count: input
                .subtitle_cues
                .map(|n| {
                    u64::try_from(n)
                        .map_err(|e| e.to_string())
                        .and_then(|n| n.try_into().map_err(str::to_owned))
                })
                .transpose()?,
            position_ms: u64::from(input.project.view.focused_ms.unwrap_or(0)).try_into()?,
            waveform_start_ms: u64::from(input.project.view.start_ms).try_into()?,
            waveform_span_ms: u64::from(input.project.view.span_ms).try_into()?,
            quality_profile_name,
            warnings: input
                .project
                .warnings
                .iter()
                .map(|w| {
                    Ok(LegacyImportWarning {
                        field: w.field.clone(),
                        line: w
                            .line
                            .map(u32::try_from)
                            .transpose()
                            .map_err(|e| e.to_string())?,
                        value: w.value.clone(),
                        message: w.message.clone(),
                    })
                })
                .collect::<Result<_>>()?,
            can_publish,
        })
    }
}
#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LegacyImportReview {
    pub id: SafeU64,
    pub phase: LegacyImportPhase,
    pub running: bool,
    pub report: Option<LegacyImportReport>,
    pub output_path: Option<String>,
    pub error: Option<String>,
}

#[tauri::command]
#[specta::specta]
pub async fn project_import_start(
    app: tauri::AppHandle,
    request: LegacyImportRequest,
) -> Result<SafeU64> {
    tauri::async_runtime::spawn_blocking(move || {
        let policy = app.state::<Documents>().default_quality_policy()?;
        app.state::<Projects>()
            .imports
            .start(request, policy)?
            .try_into()
            .map_err(str::to_owned)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
#[specta::specta]
pub async fn project_import_view(app: tauri::AppHandle, id: SafeU64) -> Result<LegacyImportReview> {
    tauri::async_runtime::spawn_blocking(move || app.state::<Projects>().imports.view(id.get()))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
#[specta::specta]
pub async fn project_import_cancel(app: tauri::AppHandle, id: SafeU64) -> Result<()> {
    tauri::async_runtime::spawn_blocking(move || app.state::<Projects>().imports.cancel(id.get()))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
#[specta::specta]
pub async fn project_import_publish(
    app: tauri::AppHandle,
    id: SafeU64,
    path: String,
) -> Result<()> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Projects>().imports.publish(id.get(), path)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
pub(crate) fn ipc_samples() -> Vec<(&'static str, &'static str, serde_json::Value)> {
    let reference = LegacyImportReference {
        field: "VideoSource".into(),
        original: Some("movie.mkv".into()),
        candidate: Some("/fixture/movie.mkv".into()),
        resolved: Some("/fixture/movie.mkv".into()),
        overridden: false,
        issue: None,
    };
    let mut view = LegacyImportReview {
        id: 1.try_into().unwrap(),
        phase: LegacyImportPhase::Reviewing,
        running: false,
        output_path: None,
        error: None,
        report: Some(LegacyImportReport {
            source_path: "/fixture/legacy.vssprj".into(),
            encoding: "UTF-16LE".into(),
            media: reference.clone(),
            subtitles: LegacyImportReference {
                field: "SubtitlesFile".into(),
                original: Some("subtitles.ass".into()),
                candidate: Some("/fixture/subtitles.ass".into()),
                resolved: Some("/fixture/subtitles.ass".into()),
                ..reference
            },
            subtitle_format: Some("ass".into()),
            cue_count: Some(42.try_into().unwrap()),
            position_ms: 1501.try_into().unwrap(),
            waveform_start_ms: 1000.try_into().unwrap(),
            waveform_span_ms: 15000.try_into().unwrap(),
            quality_profile_name: "Balanced".into(),
            warnings: vec![LegacyImportWarning {
                field: "Dictionary".into(),
                line: Some(8),
                value: Some("French".into()),
                message:
                    "Legacy dictionary selection is not imported; choose language settings in VSF."
                        .into(),
            }],
            can_publish: true,
        }),
    };
    let mut samples = vec![(
        "legacyImportReview",
        "LegacyImportReview",
        serde_json::to_value(&view).unwrap(),
    )];
    view.report = None;
    for (name, phase) in [
        ("legacyImportPreparing", LegacyImportPhase::Preparing),
        ("legacyImportPublishing", LegacyImportPhase::Publishing),
        ("legacyImportPublished", LegacyImportPhase::Published),
        ("legacyImportCancelling", LegacyImportPhase::Cancelling),
        ("legacyImportCancelled", LegacyImportPhase::Cancelled),
        ("legacyImportStale", LegacyImportPhase::Stale),
        ("legacyImportFailed", LegacyImportPhase::Failed),
        ("legacyImportClosed", LegacyImportPhase::Closed),
    ] {
        view.phase = phase;
        view.running = matches!(
            phase,
            LegacyImportPhase::Preparing
                | LegacyImportPhase::Publishing
                | LegacyImportPhase::Cancelling
        );
        view.output_path =
            (phase == LegacyImportPhase::Published).then(|| "/fixture/new.vsfproject".into());
        view.error = matches!(phase, LegacyImportPhase::Stale | LegacyImportPhase::Failed)
            .then(|| "Inspect the project again".into());
        samples.push((
            name,
            "LegacyImportReview",
            serde_json::to_value(&view).unwrap(),
        ));
    }
    samples
}
