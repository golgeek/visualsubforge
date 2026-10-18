use super::*;
use crate::project::{self, Options, Project};
use std::path::Path;
use vsf_application::{legacy_project as engine, new_output};

pub(super) struct Prepared {
    pub report: ImportReport,
    inspection: engine::Inspection,
    project: Option<Project>,
}
pub(super) struct ExportError {
    pub stale: bool,
    pub message: String,
}
impl From<String> for ExportError {
    fn from(message: String) -> Self {
        Self {
            stale: false,
            message,
        }
    }
}
impl ExportError {
    fn stale(message: impl Into<String>) -> Self {
        Self {
            stale: true,
            message: message.into(),
        }
    }
}

pub(super) fn inspect(
    request: ImportRequest,
    policy: crate::quality_profile::Policy,
    mut checkpoint: impl FnMut() -> Result<()>,
) -> Result<Prepared> {
    let mut inspection = engine::inspect(
        Path::new(&request.path),
        &engine::Overrides {
            media: request.media_path.map(PathBuf::from),
            subtitles: request.subtitle_path.map(PathBuf::from),
        },
        &mut checkpoint,
    )?;
    let identity = if let Some(path) = &inspection.media.resolved {
        match project::identify(path) {
            Ok(identity) => Some(identity),
            Err(error) => {
                inspection.media.issue = Some(error);
                None
            }
        }
    } else {
        None
    };
    checkpoint()?;
    let project = match (
        &inspection.media.resolved,
        &inspection.subtitles.resolved,
        identity,
    ) {
        (Some(media), Some(subtitles), Some(identity)) if inspection.ready() => Some(Project {
            version: 1,
            media_path: Some(media.to_string_lossy().into_owned()),
            subtitle_path: Some(subtitles.to_string_lossy().into_owned()),
            subtitle_format: inspection.subtitle_format.map(|f| f.extension().into()),
            media_identity: Some(identity),
            translation: None,
            position_seconds: f64::from(inspection.project.view.focused_ms.unwrap_or(0)) / 1000.,
            options: Options {
                waveform_start: f64::from(inspection.project.view.start_ms) / 1000.,
                waveform_span: f64::from(inspection.project.view.span_ms) / 1000.,
                follow_playback: true,
                follow_subtitle: true,
                show_scenes: true,
                timing: policy.timing,
                quality: Some(policy.profile.clone()),
            },
            scenes: None,
        }),
        _ => None,
    };
    if let Some(project) = &project {
        project.validate()?;
    }
    let report =
        ImportReport::from_inspection(&inspection, policy.profile.name, project.is_some())?;
    Ok(Prepared {
        report,
        inspection,
        project,
    })
}

pub(super) fn publish(
    prepared: &Prepared,
    selected: &Path,
    state: &Mutex<JobState>,
    mut checkpoint: impl FnMut() -> Result<()>,
    hook: impl FnOnce() -> Result<()>,
) -> std::result::Result<(), ExportError> {
    checkpoint()?;
    if selected.extension().and_then(|s| s.to_str()) != Some("vsfproject") {
        return Err("Imported project filename must end in .vsfproject"
            .to_owned()
            .into());
    }
    let path = new_output::resolve(selected).map_err(|e| e.to_string())?;
    let input = &prepared.inspection;
    let base = input
        .source_path
        .parent()
        .ok_or_else(|| "Legacy project has no directory".to_owned())?;
    let protected = std::iter::once(input.source_selection.clone())
        .chain(std::iter::once(input.source_path.clone()))
        .chain(input.media.candidate.clone())
        .chain(input.media.resolved.clone())
        .chain(input.subtitles.candidate.clone())
        .chain(input.subtitles.resolved.clone())
        .chain(
            input
                .project
                .references
                .iter()
                .filter_map(|r| engine::local_path(base, &r.value).ok()),
        );
    for original in protected {
        new_output::protect_source(&path, &original).map_err(|_| {
            "Choose a different project destination to preserve all original inputs".to_owned()
        })?;
    }
    let mut project = prepared
        .project
        .clone()
        .ok_or_else(|| "Import references are incomplete".to_owned())?;
    let parent = path
        .parent()
        .ok_or_else(|| "Output has no parent directory".to_owned())?;
    project.media_path = project
        .media_path
        .map(|p| project::portable(parent, Path::new(&p)));
    project.subtitle_path = project
        .subtitle_path
        .map(|s| project::portable(parent, Path::new(&s)));
    let bytes = project::serialize(&path, &project)?;
    checkpoint()?;
    let _lease = vsf_application::save_lease::acquire(&path)?;
    new_output::require_absent(&path).map_err(|e| e.to_string())?;
    let pending =
        vsf_application::atomic_file::prepare_with_checkpoint(&path, &bytes, &mut checkpoint)?;
    hook()?;
    engine::verify_text_inputs(input, &mut checkpoint).map_err(ExportError::stale)?;
    let media = input
        .media
        .resolved
        .as_ref()
        .ok_or_else(|| ExportError::stale("Media reference is unavailable"))?;
    if input
        .media
        .candidate
        .as_ref()
        .ok_or_else(|| ExportError::stale("Media selection is unavailable"))?
        .canonicalize()
        .map_err(|e| ExportError::stale(format!("Media is unavailable: {e}")))?
        != *media
        || Some(project::identify(media).map_err(ExportError::stale)?) != project.media_identity
    {
        return Err(ExportError::stale(
            "Media changed after review; inspect the project again",
        ));
    }
    // This single-file publication is serialized with cancellation. No document,
    // native-media or project-attachment mutex is held during IO or this commit.
    let mut state = state
        .lock()
        .map_err(|_| "Project import result unavailable".to_owned())?;
    checkpoint()?;
    if state.phase != ImportPhase::Publishing {
        return Err("Project import is no longer current".to_owned().into());
    }
    pending.commit_new()?;
    state.phase = ImportPhase::Published;
    state.output = Some(path.to_string_lossy().into_owned());
    state.error = None;
    Ok(())
}
