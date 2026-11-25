use super::*;
use crate::ipc_number::SafeU64;
use vsf_application::create_project::{Outcome, Targets};

#[derive(Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CreateRequest {
    folder: PathBuf,
    project_name: String,
    companion_name: Option<String>,
    generation: SafeU64,
    revision: SafeU64,
    workspace: WorkspaceInput,
}

fn create(app: &tauri::AppHandle, request: CreateRequest) -> Result<Outcome<Attachment>> {
    let targets = Targets::new(
        &request.folder,
        &request.project_name,
        request.companion_name.as_deref(),
    )?;
    let original = request
        .workspace
        .subtitle_path
        .as_deref()
        .map(PathBuf::from);
    let subtitle = targets.companion.as_deref().or(original.as_deref());
    with_workspace(
        app,
        request.workspace,
        &targets.project,
        subtitle,
        |project| {
            let projects = app.state::<Projects>();
            projects.require_unattached()?;
            app.state::<crate::document::Documents>().create_project(
                (request.generation.get(), request.revision.get()),
                original.as_deref(),
                &targets,
                |context, format, translation, review| {
                    let project = project.with_document(
                        &targets.project,
                        context,
                        format,
                        translation,
                        review,
                    )?;
                    projects.publish(&targets.project, None, &project, true)
                },
            )
        },
    )
}

#[tauri::command]
#[specta::specta]
pub async fn create_project(
    app: tauri::AppHandle,
    request: CreateRequest,
) -> Result<Outcome<Attachment>> {
    tauri::async_runtime::spawn_blocking(move || {
        create(&app, request).unwrap_or_else(|error| Outcome::NothingWritten { error })
    })
    .await
    .map_err(|e| format!("Project worker failed: {e}"))
}
