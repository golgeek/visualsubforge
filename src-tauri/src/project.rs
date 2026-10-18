//! Versioned project metadata; subtitle bytes remain in separately saved files.
//! All filesystem work is dispatched off the UI thread and saves replace atomically.
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};
use tauri::Manager;
pub(crate) mod import;
mod store;
pub use store::{Attachment, Opened, Projects};

type Result<T> = std::result::Result<T, String>;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MediaIdentity {
    #[serde(with = "crate::ipc_number")]
    #[specta(type = crate::ipc_number::SafeU64)]
    size: u64,
    modified: String,
    sample_hash: String,
}
pub fn identify(path: &Path) -> Result<MediaIdentity> {
    let mut file = fs::File::open(path).map_err(|e| format!("Cannot read media: {e}"))?;
    let before = file.metadata().map_err(|e| e.to_string())?;
    if !before.is_file() {
        return Err("Media must be a local file".into());
    }
    let modified = before.modified().map_err(|e| e.to_string())?;
    let mut hash = 0xcbf29ce484222325u64;
    let size = before.len();
    crate::ipc_number::SafeU64::try_from(size)?;
    for offset in [0, size / 2, size.saturating_sub(65_536)] {
        file.seek(SeekFrom::Start(offset))
            .map_err(|e| e.to_string())?;
        let mut bytes = Vec::new();
        (&mut file)
            .take(65_536)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        for byte in bytes {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
        }
    }
    let after = file.metadata().map_err(|e| e.to_string())?;
    if after.len() != size || after.modified().map_err(|e| e.to_string())? != modified {
        return Err("Media changed while its identity was read".into());
    }
    Ok(MediaIdentity {
        size,
        modified: format!(
            "{}.{:09}",
            modified
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_secs(),
            modified
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .subsec_nanos()
        ),
        sample_hash: format!("{hash:016x}"),
    })
}
#[derive(Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Options {
    #[serde(with = "crate::ipc_number::finite")]
    #[specta(type = crate::ipc_number::finite::FiniteF64)]
    pub waveform_start: f64,
    #[serde(with = "crate::ipc_number::finite")]
    #[specta(type = crate::ipc_number::finite::FiniteF64)]
    pub waveform_span: f64,
    pub follow_playback: bool,
    pub follow_subtitle: bool,
    pub show_scenes: bool,
    #[serde(default)]
    pub timing: crate::timing_preset::Settings,
    #[serde(default)]
    pub quality: Option<crate::quality_profile::Profile>,
}
#[derive(Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Project {
    version: u32,
    pub media_path: String,
    pub subtitle_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle_format: Option<String>,
    pub media_identity: MediaIdentity,
    #[serde(with = "crate::ipc_number::finite")]
    #[specta(type = crate::ipc_number::finite::FiniteF64)]
    pub position_seconds: f64,
    pub options: Options,
    pub scenes: Option<crate::scenes::Saved>,
}
impl Project {
    fn validate(&self) -> Result<()> {
        self.options.timing.validate()?;
        if let Some(profile) = &self.options.quality {
            crate::quality_profile::Policy::new(profile.clone(), self.options.timing.clone())?;
        }
        if self.version != 1 {
            return Err("Unsupported VSF project version".into());
        }
        if let Some(format) = &self.subtitle_format {
            let format = subtitle_formats::Format::from_extension(format)
                .ok_or("Unknown project subtitle format")?;
            let path = self
                .subtitle_path
                .as_ref()
                .ok_or("Project subtitle format requires a path")?;
            if Path::new(path)
                .extension()
                .and_then(|ext| ext.to_str())
                .and_then(subtitle_formats::Format::from_extension)
                .is_some_and(|extension| extension != format)
            {
                return Err("Project subtitle extension and format disagree".into());
            }
        }
        if ![
            self.position_seconds,
            self.options.waveform_start,
            self.options.waveform_span,
        ]
        .iter()
        .all(|n| n.is_finite())
            || self.position_seconds < 0.
            || self.options.waveform_start < 0.
            || self.options.waveform_span <= 0.
        {
            return Err("Invalid project playback or waveform position".into());
        }
        if let Some(scenes) = &self.scenes {
            scenes.validate()?;
            if scenes.identity != self.media_identity {
                return Err("Scene results belong to different media".into());
            }
        }
        Ok(())
    }
}
fn absolute(base: &Path, path: &str) -> PathBuf {
    let path = Path::new(path);
    if path.is_absolute() {
        path.into()
    } else {
        base.join(path)
    }
}
fn portable(base: &Path, path: &Path) -> String {
    path.strip_prefix(base)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}
fn read_bytes(path: &Path) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|e| format!("Cannot open project: {e}"))?
        .take(8 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err("Project exceeds 8 MiB limit".into());
    }
    Ok(bytes)
}
fn resolve(path: &Path, bytes: &[u8]) -> Result<Project> {
    let mut project: Project =
        serde_json::from_slice(bytes).map_err(|e| format!("Invalid project: {e}"))?;
    project.validate()?;
    if project.options.quality.is_none() {
        project.options.quality = Some(
            crate::quality_profile::Policy::legacy(project.options.timing.clone(), "project")?
                .profile,
        );
    }
    let base = path.parent().ok_or("Project has no parent directory")?;
    let media = absolute(base, &project.media_path)
        .canonicalize()
        .map_err(|e| format!("Project media is unavailable: {e}"))?;
    if identify(&media)? != project.media_identity {
        return Err(
            "Project media has changed; open it directly and rescan before saving a new project"
                .into(),
        );
    }
    project.media_path = media.to_string_lossy().into_owned();
    if let Some(subtitle) = &project.subtitle_path {
        let subtitle = absolute(base, subtitle)
            .canonicalize()
            .map_err(|e| format!("Project subtitles are unavailable: {e}"))?;
        // Opening the project must not touch the active workspace until references validate.
        let bytes = crate::document::read_file(&subtitle)?;
        let parsed = vsf_application::subtitle_file::parse_document(
            &bytes,
            project
                .subtitle_format
                .as_deref()
                .or_else(|| subtitle.extension().and_then(|s| s.to_str())),
        )
        .map_err(|e| format!("Project subtitles: {e}"))?;
        project.subtitle_format = Some(parsed.source.format().extension().into());
        project.subtitle_path = Some(subtitle.to_string_lossy().into_owned());
    }
    Ok(project)
}
#[cfg(test)]
fn read(path: &Path) -> Result<Project> {
    resolve(path, &read_bytes(path)?)
}
#[tauri::command]
#[specta::specta]
pub async fn project_open(app: tauri::AppHandle, path: String) -> Result<Opened> {
    tauri::async_runtime::spawn_blocking(move || app.state::<Projects>().open(Path::new(&path)))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
#[specta::specta]
pub async fn project_attach(
    app: tauri::AppHandle,
    token: crate::ipc_number::SafeU64,
    session: crate::ipc_number::SafeU64,
    generation: crate::ipc_number::SafeU64,
    revision: crate::ipc_number::SafeU64,
) -> Result<Attachment> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<crate::media::Media>()
            .with_current(session.get(), |path| {
                let path = path.canonicalize().map_err(|e| e.to_string())?;
                app.state::<crate::document::Documents>()
                    .attach_quality_project(
                        &app.state::<Projects>(),
                        token.get(),
                        generation.get(),
                        revision.get(),
                        &path,
                    )
            })
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
#[specta::specta]
pub async fn project_detach(app: tauri::AppHandle) -> Result<()> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<crate::document::Documents>()
            .detach_quality_project(&app.state::<Projects>())
    })
    .await
    .map_err(|e| e.to_string())?
}
#[derive(Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SaveRequest {
    path: String,
    #[serde(default, with = "crate::ipc_number::optional")]
    #[specta(type = Option<crate::ipc_number::SafeU64>)]
    token: Option<u64>,
    media_path: String,
    subtitle_path: Option<String>,
    #[serde(with = "crate::ipc_number")]
    #[specta(type = crate::ipc_number::SafeU64)]
    session: u64,
    #[serde(with = "crate::ipc_number::finite")]
    #[specta(type = crate::ipc_number::finite::FiniteF64)]
    position_seconds: f64,
    options: Options,
}
#[tauri::command]
#[specta::specta]
pub async fn project_save(app: tauri::AppHandle, request: SaveRequest) -> Result<Attachment> {
    tauri::async_runtime::spawn_blocking(move || save(&app, request))
        .await
        .map_err(|e| e.to_string())?
}
fn save(app: &tauri::AppHandle, request: SaveRequest) -> Result<Attachment> {
    let media = Path::new(&request.media_path)
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let path = Path::new(&request.path);
    if path.extension().is_none_or(|s| s != "vsfproject") {
        return Err("Project filename must end in .vsfproject".into());
    }
    let destination = destination(path)?;
    let base = destination.parent().ok_or("Project path has no parent")?;
    let subtitle = request
        .subtitle_path
        .as_deref()
        .map(Path::new)
        .map(Path::canonicalize)
        .transpose()
        .map_err(|e| e.to_string())?;
    let identity = identify(&media)?;
    let scenes = app
        .state::<crate::scenes::Scenes>()
        .saved(request.session, &identity)?;
    let mut project = Project {
        version: 1,
        media_path: portable(base, &media),
        subtitle_path: subtitle.as_ref().map(|p| portable(base, p)),
        subtitle_format: None,
        media_identity: identity,
        position_seconds: request.position_seconds,
        options: request.options,
        scenes,
    };

    // Keep media selection -> document -> project locked through publication,
    // matching attach. A new media selection cannot reset the policy between
    // capturing its snapshot and writing this project's references.
    app.state::<crate::media::Media>()
        .with_current(request.session, |selected| {
            if selected.canonicalize().map_err(|e| e.to_string())? != media {
                return Err("Project references differ from the selected media".into());
            }
            app.state::<crate::document::Documents>()
                .with_project_subtitle(subtitle.as_deref(), |context, format| {
                    // Only Rust's accepted effective rules may be persisted.
                    // Ignore optional frontend profile and stale timing fields.
                    project.options.quality = Some(context.profile.clone());
                    project.options.timing = context.timing.clone();
                    project.subtitle_format = format.map(|format| format.extension().into());
                    app.state::<Projects>()
                        .save(&destination, request.token, &project)
                })
        })
}
fn destination(path: &Path) -> Result<PathBuf> {
    let base = path.parent().ok_or("Project path has no parent")?;
    if path.exists() {
        path.canonicalize().map_err(|e| e.to_string())
    } else {
        Ok(base
            .canonicalize()
            .map_err(|e| e.to_string())?
            .join(path.file_name().ok_or("Choose a project filename")?))
    }
}
fn serialize(path: &Path, project: &Project) -> Result<Vec<u8>> {
    project.validate()?;
    let base = path.parent().ok_or("Project path has no parent")?;
    for reference in std::iter::once(&project.media_path).chain(project.subtitle_path.iter()) {
        let reference = absolute(base, reference)
            .canonicalize()
            .map_err(|e| e.to_string())?;
        if reference == path {
            return Err("Project destination must differ from media and subtitle files".into());
        }
    }
    let bytes = serde_json::to_vec_pretty(project).map_err(|e| e.to_string())?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err("Project exceeds 8 MiB limit".into());
    }
    Ok(bytes)
}
#[cfg(test)]
fn write(path: &Path, project: &Project) -> Result<()> {
    let path = destination(path)?;
    crate::atomic_file::write(&path, &serialize(&path, project)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    pub(super) fn fixture(root: &Path) -> Project {
        let media = root.join("clip.mkv");
        fs::write(&media, b"synthetic media identity").unwrap();
        Project {
            version: 1,
            media_path: "clip.mkv".into(),
            subtitle_path: None,
            subtitle_format: None,
            media_identity: identify(&media).unwrap(),
            position_seconds: 12.5,
            options: Options {
                waveform_start: 10.,
                waveform_span: 20.,
                follow_playback: false,
                follow_subtitle: true,
                show_scenes: false,
                timing: crate::timing_preset::Settings::default(),
                quality: None,
            },
            scenes: None,
        }
    }
    #[test]
    fn project_retains_ass_ssa_webvtt_identity_and_rejects_conflicts() {
        for (extension, source) in [
            (
                "ass",
                include_str!("../../tests/fixtures/subtitles/styles.ass"),
            ),
            (
                "ssa",
                include_str!("../../tests/fixtures/subtitles/styles.ssa"),
            ),
            (
                "vtt",
                include_str!("../../tests/fixtures/subtitles/styles.vtt"),
            ),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let mut project = fixture(dir.path());
            let filename = format!("captions.{extension}");
            fs::write(dir.path().join(&filename), source).unwrap();
            project.subtitle_path = Some(filename);
            let path = dir.path().join("styles.vsfproject");
            // Old projects omit identity and still resolve by the actual file.
            write(&path, &project).unwrap();
            let mut loaded = read(&path).unwrap();
            assert_eq!(loaded.subtitle_format.as_deref(), Some(extension));
            write(&path, &loaded).unwrap();
            let before = fs::read(&path).unwrap();
            assert_eq!(
                read(&path).unwrap().subtitle_format.as_deref(),
                Some(extension)
            );
            loaded.subtitle_format = Some("srt".into());
            assert!(write(&path, &loaded).is_err());
            assert_eq!(fs::read(&path).unwrap(), before);
            // A file replaced by a different format must not silently reinterpret it.
            fs::write(
                dir.path().join(format!("captions.{extension}")),
                "1\n00:00:01,000 --> 00:00:02,000\nSRT\n",
            )
            .unwrap();
            assert!(read(&path).is_err());
        }
    }
    #[test]
    fn project_roundtrip_resolves_relative_paths_and_preserves_options() {
        let dir = tempfile::tempdir().unwrap();
        let mut project = fixture(dir.path());
        fs::write(
            dir.path().join("captions.srt"),
            "1\n00:00:01,000 --> 00:00:02,000\nHello\n",
        )
        .unwrap();
        project.subtitle_path = Some("captions.srt".into());
        project.scenes = Some(serde_json::from_value(serde_json::json!({"version":1,"mode":"visual","cuts":[1.0,2.0],"identity":project.media_identity})).unwrap());
        let path = dir.path().join("test.vsfproject");
        write(&path, &project).unwrap();
        let loaded = read(&path).unwrap();
        assert!(Path::new(&loaded.media_path).is_absolute());
        assert!(Path::new(loaded.subtitle_path.as_ref().unwrap()).is_absolute());
        assert_eq!(loaded.position_seconds, 12.5);
        assert!(!loaded.options.follow_playback && !loaded.options.show_scenes);
        assert!(loaded.scenes.is_some());
        assert_eq!(
            serde_json::to_value(loaded.scenes).unwrap(),
            serde_json::to_value(project.scenes).unwrap()
        );
    }
    #[test]
    fn changed_media_missing_subtitle_unknown_version_and_bad_times_fail() {
        let dir = tempfile::tempdir().unwrap();
        let mut project = fixture(dir.path());
        let path = dir.path().join("test.vsfproject");
        let write = |p: &Project| fs::write(&path, serde_json::to_vec(p).unwrap()).unwrap();
        project.version = 2;
        write(&project);
        assert!(read(&path).err().unwrap().contains("version"));
        project.version = 1;
        project.subtitle_path = Some("missing.srt".into());
        write(&project);
        assert!(read(&path).err().unwrap().contains("subtitles"));
        project.subtitle_path = None;
        project.position_seconds = -1.;
        write(&project);
        assert!(read(&path).is_err());
        project.position_seconds = 0.;
        write(&project);
        fs::write(dir.path().join("clip.mkv"), b"different media content!").unwrap();
        assert!(read(&path).err().unwrap().contains("media has changed"));
    }
    #[test]
    fn rejected_save_preserves_project_and_never_overwrites_inputs() {
        let dir = tempfile::tempdir().unwrap();
        let mut project = fixture(dir.path());
        let path = dir.path().join("test.vsfproject");
        write(&path, &project).unwrap();
        let previous = fs::read(&path).unwrap();
        project.position_seconds = -1.;
        assert!(write(&path, &project).is_err());
        assert_eq!(fs::read(&path).unwrap(), previous);
        project.position_seconds = 1.;
        let media = dir.path().join("clip.mkv");
        let original = fs::read(&media).unwrap();
        assert!(write(&media, &project).is_err());
        assert_eq!(fs::read(media).unwrap(), original);
    }
    #[test]
    fn old_projects_default_to_optional_magnets_and_settings_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let mut project = fixture(dir.path());
        let mut json = serde_json::to_value(&project).unwrap();
        json["options"].as_object_mut().unwrap().remove("timing");
        let old: Project = serde_json::from_value(json).unwrap();
        assert_eq!(
            old.options.timing,
            crate::timing_preset::Settings::default()
        );
        project.options.timing.enabled = true;
        project.options.timing.cue_gap_ms = 200;
        let path = dir.path().join("settings.vsfproject");
        write(&path, &project).unwrap();
        assert_eq!(read(&path).unwrap().options.timing, project.options.timing);
        project.options.timing.snap_distance_px = 0;
        assert!(write(&path, &project).is_err());
        assert_eq!(read(&path).unwrap().options.timing.snap_distance_px, 8);
    }
    #[test]
    fn malformed_cut_lists_cannot_be_restored() {
        let dir = tempfile::tempdir().unwrap();
        let project = fixture(dir.path());
        for cuts in [vec![2., 1.], vec![-1.], vec![1., 1.]] {
            let saved: crate::scenes::Saved = serde_json::from_value(serde_json::json!({"version":1,"mode":"keyframes","cuts":cuts,"identity":project.media_identity})).unwrap();
            assert!(saved.validate().is_err());
        }
    }
}

#[cfg(test)]
pub(crate) fn ipc_sample() -> serde_json::Value {
    // This older on-disk form omits timing settings; the opened IPC output must
    // include their defaults, with project fields flattened beside the token.
    let project: Project = serde_json::from_value(serde_json::json!({
        "version": 1,
        "mediaPath": "target.mkv",
        "subtitlePath": null,
        "mediaIdentity": {"size": 123, "modified": "1.0", "sampleHash": "sample"},
        "positionSeconds": 12.5,
        "options": {"waveformStart": 0., "waveformSpan": 20.,
            "followPlayback": true, "followSubtitle": false, "showScenes": true},
        "scenes": null,
    }))
    .unwrap();
    project.validate().unwrap();
    store::ipc_sample(project)
}

#[test]
fn ipc_project_defaults_flattening_and_numeric_bounds_match_json() {
    let opened = ipc_sample();
    assert!(opened.get("project").is_none());
    assert_eq!(opened["token"], crate::ipc_number::MAX_SAFE_INTEGER);
    assert_eq!(
        opened["options"]["timing"],
        serde_json::to_value(crate::timing_preset::Settings::default()).unwrap()
    );
    let mut project = opened.clone();
    project.as_object_mut().unwrap().remove("token");
    project["mediaIdentity"]["size"] = (crate::ipc_number::MAX_SAFE_INTEGER + 1).into();
    assert!(serde_json::from_value::<Project>(project).is_err());
    let mut identity: MediaIdentity =
        serde_json::from_value(opened["mediaIdentity"].clone()).unwrap();
    identity.size = crate::ipc_number::MAX_SAFE_INTEGER + 1;
    assert!(serde_json::to_value(identity).is_err());
}
