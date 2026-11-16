//! Bounded historical metadata, never an accepted document or another checker.
//! Capture releases media/scene/document/project locks before hashing and disk IO.
use crate::ipc_number::SafeU64;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::Manager;
type Result<T> = std::result::Result<T, String>;
const MAX_ENTRIES: usize = 12;
const MAX_BYTES: u64 = 128 * 1024;
static WRITER: Mutex<()> = Mutex::new(());

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    Subtitles,
    Media,
    Project,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Summary {
    pub cues: u32,
    pub issues: u32,
    pub complete: bool,
    pub profile_name: String,
    pub profile_hash: String,
    pub source_sha256: String,
    pub checked_at_ms: SafeU64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Entry {
    pub kind: Kind,
    pub path: String,
    pub media_path: Option<String>,
    pub subtitle_path: Option<String>,
    pub used_at_ms: SafeU64,
    pub summary: Option<Summary>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    version: u32,
    entries: Vec<Entry>,
}
pub(crate) struct ProjectCapture {
    pub path: PathBuf,
    pub media_path: Option<PathBuf>,
    pub subtitle_path: Option<PathBuf>,
}
pub(crate) struct SubtitleCapture {
    pub path: PathBuf,
    pub bytes: Option<Vec<u8>>,
    pub summary: Option<Summary>,
}
#[derive(Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub session: SafeU64,
    pub generation: SafeU64,
    pub revision: SafeU64,
    pub project_token: Option<SafeU64>,
}
fn path(app: &tauri::AppHandle) -> Result<PathBuf> {
    Ok(crate::preferences::settings_path(app)?.with_file_name("recent-work-v1.json"))
}
pub(crate) fn now() -> Result<SafeU64> {
    let value = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis();
    SafeU64::try_from(u64::try_from(value).map_err(|e| e.to_string())?).map_err(str::to_owned)
}
fn validate(entries: &[Entry]) -> Result<()> {
    if entries.len() > MAX_ENTRIES {
        return Err("Recent-work list exceeds its limit".into());
    }
    let mut seen = std::collections::HashSet::new();
    for entry in entries {
        for path in std::iter::once(&entry.path)
            .chain(entry.media_path.iter())
            .chain(entry.subtitle_path.iter())
        {
            if path.is_empty()
                || path.len() > 16_384
                || path.contains('\0')
                || !Path::new(path).is_absolute()
            {
                return Err("Invalid recent-work path".into());
            }
        }
        if !seen.insert((&entry.kind, &entry.path)) {
            return Err("Duplicate recent-work entry".into());
        }
        if let Some(summary) = &entry.summary
            && (summary.profile_name.len() > 4096
                || [&summary.profile_hash, &summary.source_sha256]
                    .iter()
                    .any(|hash| hash.len() != 64 || !hash.bytes().all(|c| c.is_ascii_hexdigit())))
        {
            return Err("Invalid recent-work check metadata".into());
        }
    }
    Ok(())
}
fn read(path: &Path) -> Result<Vec<Entry>> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(error) => return Err(format!("Cannot read recent work: {error}")),
    };
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("Recent-work file exceeds its limit".into());
    }
    let file: File =
        serde_json::from_slice(&bytes).map_err(|e| format!("Invalid recent work: {e}"))?;
    if file.version != 1 {
        return Err("Unsupported recent-work version".into());
    }
    validate(&file.entries)?;
    Ok(file.entries)
}
fn update(path: &Path, change: impl FnOnce(&mut Vec<Entry>)) -> Result<Vec<Entry>> {
    let _writer = WRITER
        .lock()
        .map_err(|_| "Recent-work writer unavailable")?;
    let mut entries = read(path)?;
    change(&mut entries);
    validate(&entries)?;
    let bytes = serde_json::to_vec_pretty(&File {
        version: 1,
        entries: entries.clone(),
    })
    .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("Recent-work file exceeds its limit".into());
    }
    fs::create_dir_all(path.parent().ok_or("Recent-work path has no parent")?)
        .map_err(|e| e.to_string())?;
    crate::atomic_file::write(path, &bytes)?;
    Ok(entries)
}
fn remember(path: &Path, mut entry: Entry) -> Result<Vec<Entry>> {
    update(path, |entries| {
        // Reopening a video alone revisits its most recent subtitle pairing.
        // Preserve that historical pair rather than inventing another session.
        if entry.kind == Kind::Media
            && let Some(pair) = entries.iter().find(|old| {
                old.kind == Kind::Subtitles && old.media_path.as_ref() == Some(&entry.path)
            })
        {
            let used_at_ms = entry.used_at_ms;
            entry = pair.clone();
            entry.used_at_ms = used_at_ms;
        }
        if entry.summary.is_none()
            && let Some(old) = entries.iter().find(|old| {
                old.kind == entry.kind
                    && old.path == entry.path
                    && old.subtitle_path == entry.subtitle_path
            })
            && let (Some(summary), Some(source)) = (&old.summary, &entry.subtitle_path)
        {
            // Historical counts remain valid only for exactly these saved bytes.
            // Bound the read just like accepted subtitle input.
            if let Ok(file) = fs::File::open(source) {
                let mut bytes = Vec::new();
                if file
                    .take(64 * 1024 * 1024 + 1)
                    .read_to_end(&mut bytes)
                    .is_ok()
                    && bytes.len() <= 64 * 1024 * 1024
                    && format!("{:x}", Sha256::digest(&bytes)) == summary.source_sha256
                {
                    entry.summary = Some(summary.clone());
                }
            }
        }
        entries.retain(|old| {
            let same_entry = old.kind == entry.kind && old.path == entry.path;
            // Completing a media/subtitle pair upgrades its media-only entry.
            // Other subtitles attached to the same video remain independent work.
            let completed_pair = entry.kind == Kind::Subtitles
                && old.kind == Kind::Media
                && entry.media_path.as_ref() == Some(&old.path);
            !same_entry && !completed_pair
        });
        entries.insert(0, entry);
        entries.truncate(MAX_ENTRIES);
    })
}
#[tauri::command]
#[specta::specta]
pub async fn recent_list(app: tauri::AppHandle) -> Result<Vec<Entry>> {
    tauri::async_runtime::spawn_blocking(move || read(&path(&app)?))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
#[specta::specta]
pub async fn recent_forget(
    app: tauri::AppHandle,
    kind: Kind,
    entry_path: String,
) -> Result<Vec<Entry>> {
    tauri::async_runtime::spawn_blocking(move || {
        update(&path(&app)?, |entries| {
            entries.retain(|entry| entry.kind != kind || entry.path != entry_path)
        })
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
#[specta::specta]
pub async fn recent_capture(app: tauri::AppHandle, request: Request) -> Result<Vec<Entry>> {
    tauri::async_runtime::spawn_blocking(move || {
        let (media, mut subtitle, project) =
            app.state::<crate::media::Media>()
                .with_context(request.session.get(), |media| {
                    let scenes = app.state::<crate::scenes::Scenes>();
                    let capture = scenes.capture_quality(request.session.get(), media.is_some())?;
                    scenes.with_quality_current(capture.key, || {
                        let subtitle = app.state::<crate::document::Documents>().recent_subtitles(
                            request.generation.get(),
                            request.revision.get(),
                            capture.key,
                        )?;
                        let project = app
                            .state::<crate::project::Projects>()
                            .recent_paths(request.project_token.map(SafeU64::get))?;
                        Ok((media.map(Path::to_path_buf), subtitle, project))
                    })
                })?;
        let timestamp = now()?;
        if let Some(subtitle) = &mut subtitle
            && let (Some(summary), Some(bytes)) = (&mut subtitle.summary, &subtitle.bytes)
        {
            summary.source_sha256 = format!("{:x}", Sha256::digest(bytes));
        }
        let text = |path: PathBuf| path.to_string_lossy().into_owned();
        let entry = if let Some(project) = project {
            let summary = subtitle
                .as_ref()
                .filter(|value| Some(&value.path) == project.subtitle_path.as_ref())
                .and_then(|value| value.summary.clone());
            Some(Entry {
                kind: Kind::Project,
                path: text(project.path),
                media_path: project.media_path.map(text),
                subtitle_path: project.subtitle_path.map(text),
                used_at_ms: timestamp,
                summary,
            })
        } else if let Some(subtitle) = subtitle {
            Some(Entry {
                kind: Kind::Subtitles,
                path: text(subtitle.path.clone()),
                media_path: media.map(text),
                subtitle_path: Some(text(subtitle.path)),
                used_at_ms: timestamp,
                summary: subtitle.summary,
            })
        } else {
            media.map(|media| Entry {
                kind: Kind::Media,
                path: text(media),
                media_path: None,
                subtitle_path: None,
                used_at_ms: timestamp,
                summary: None,
            })
        };
        match entry {
            Some(entry) => remember(&path(&app)?, entry),
            None => read(&path(&app)?),
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    fn entry(dir: &Path, index: u32) -> Entry {
        Entry {
            kind: Kind::Subtitles,
            path: dir
                .join(format!("{index}.srt"))
                .to_string_lossy()
                .into_owned(),
            media_path: None,
            subtitle_path: None,
            used_at_ms: u64::from(index).try_into().unwrap(),
            summary: None,
        }
    }
    #[test]
    fn bounded_recency_roundtrip_and_forgetting_never_touch_source_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("recent.json");
        for index in 0..15 {
            remember(&path, entry(dir.path(), index)).unwrap();
        }
        let list = read(&path).unwrap();
        assert_eq!(list.len(), 12);
        assert!(list[0].path.ends_with("14.srt"));
        let mut checked = entry(dir.path(), 8);
        fs::write(&checked.path, b"original subtitle bytes").unwrap();
        checked.summary = Some(Summary {
            cues: 3,
            issues: 2,
            complete: false,
            profile_name: "Authoring".into(),
            profile_hash: "a".repeat(64),
            source_sha256: format!("{:x}", Sha256::digest(b"original subtitle bytes")),
            checked_at_ms: 10.try_into().unwrap(),
        });
        remember(&path, checked.clone()).unwrap();
        let list = read(&path).unwrap();
        assert_eq!(list.len(), 12);
        assert_eq!(list[0], checked);
        update(&path, |entries| {
            entries.retain(|entry| entry.path != checked.path)
        })
        .unwrap();
        assert_eq!(read(&path).unwrap().len(), 11);
        assert_eq!(fs::read(&checked.path).unwrap(), b"original subtitle bytes");
    }
    #[test]
    fn missing_summary_preserves_only_identical_saved_source() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("recent.json");
        let mut checked = entry(dir.path(), 1);
        checked.subtitle_path = Some(checked.path.clone());
        fs::write(&checked.path, b"saved").unwrap();
        checked.summary = Some(Summary {
            cues: 2,
            issues: 1,
            complete: true,
            profile_name: "Test".into(),
            profile_hash: "a".repeat(64),
            source_sha256: format!("{:x}", Sha256::digest(b"saved")),
            checked_at_ms: 1.try_into().unwrap(),
        });
        remember(&path, checked.clone()).unwrap();
        let mut dirty = checked.clone();
        dirty.summary = None;
        assert_eq!(
            remember(&path, dirty.clone()).unwrap()[0].summary,
            checked.summary
        );
        fs::write(&checked.path, b"different").unwrap();
        assert!(remember(&path, dirty.clone()).unwrap()[0].summary.is_none());
        fs::remove_file(&checked.path).unwrap();
        assert!(remember(&path, dirty).unwrap()[0].summary.is_none());
    }
    #[test]
    fn opening_media_and_subtitles_coalesces_in_either_order() {
        for media_first in [true, false] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("recent.json");
            let video = dir.path().join("demo.mp4").to_string_lossy().into_owned();
            let mut subtitles = entry(dir.path(), 1);
            subtitles.subtitle_path = Some(subtitles.path.clone());
            let media = Entry {
                kind: Kind::Media,
                path: video.clone(),
                media_path: None,
                subtitle_path: None,
                used_at_ms: 1.try_into().unwrap(),
                summary: None,
            };
            remember(
                &path,
                if media_first {
                    media.clone()
                } else {
                    subtitles.clone()
                },
            )
            .unwrap();
            subtitles.media_path = Some(video);
            let list = remember(&path, subtitles.clone()).unwrap();
            assert_eq!(list, vec![subtitles.clone()]);
            let reopened = Entry {
                used_at_ms: 9.try_into().unwrap(),
                ..media
            };
            subtitles.used_at_ms = reopened.used_at_ms;
            assert_eq!(remember(&path, reopened).unwrap(), vec![subtitles]);
        }
    }
    #[test]
    fn rejected_metadata_and_unknown_versions_preserve_stored_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("recent.json");
        remember(&path, entry(dir.path(), 1)).unwrap();
        let before = fs::read(&path).unwrap();
        let mut invalid = entry(dir.path(), 2);
        invalid.path = "relative.srt".into();
        assert!(remember(&path, invalid).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
        let bytes = br#"{"version":2,"entries":[]}"#;
        fs::write(&path, bytes).unwrap();
        assert!(remember(&path, entry(dir.path(), 3)).is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
        fs::write(&path, vec![b' '; MAX_BYTES as usize + 1]).unwrap();
        assert!(read(&path).is_err());
    }
    #[test]
    fn concurrent_updates_do_not_lose_entries() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("recent.json");
        std::thread::scope(|scope| {
            for index in 0..12 {
                let path = &path;
                let dir = dir.path();
                scope.spawn(move || {
                    remember(path, entry(dir, index)).unwrap();
                });
            }
        });
        assert_eq!(read(&path).unwrap().len(), 12);
    }
}
