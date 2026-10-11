//! Small versioned user preferences; blocking IO stays off the UI thread.
//! Projects retain their own timing policy and never write these defaults.
use crate::timing_preset::Settings as Timing;
use media_analysis::scenes::Detail;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};
use tauri::Manager;
type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Preferences {
    version: u32,
    auto_scan_scenes: bool,
    scene_detail: Detail,
    timing: Timing,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            version: 1,
            auto_scan_scenes: true,
            scene_detail: Detail::Standard,
            timing: Timing::default(),
        }
    }
}
impl Preferences {
    fn validate(&self) -> Result<()> {
        if self.version != 1 {
            return Err("Unsupported settings version".into());
        }
        self.timing.validate()
    }
}
fn read(path: &Path) -> Result<Preferences> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Preferences::default()),
        Err(e) => return Err(format!("Cannot read settings: {e}")),
    };
    let mut bytes = Vec::new();
    file.take(16_385)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 16_384 {
        return Err("Settings file exceeds limit".into());
    }
    let value: Preferences =
        serde_json::from_slice(&bytes).map_err(|e| format!("Invalid settings: {e}"))?;
    value.validate()?;
    Ok(value)
}
fn write(path: &Path, value: &Preferences) -> Result<()> {
    value.validate()?;
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    fs::create_dir_all(path.parent().ok_or("Settings path has no parent")?)
        .map_err(|e| e.to_string())?;
    crate::atomic_file::write(path, &bytes)
}
#[tauri::command]
#[specta::specta]
pub async fn preferences_get(app: tauri::AppHandle) -> Result<Preferences> {
    tauri::async_runtime::spawn_blocking(move || read(&settings_path(&app)?))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
#[specta::specta]
pub async fn preferences_save(app: tauri::AppHandle, preferences: Preferences) -> Result<()> {
    tauri::async_runtime::spawn_blocking(move || write(&settings_path(&app)?, &preferences))
        .await
        .map_err(|e| e.to_string())?
}

fn settings_path(app: &tauri::AppHandle) -> Result<PathBuf> {
    #[cfg(feature = "desktop-e2e")]
    return Ok(app
        .state::<crate::desktop_test::Driver>()
        .root()
        .join("settings-v1.json"));
    #[cfg(not(feature = "desktop-e2e"))]
    app.path()
        .app_config_dir()
        .map(|path| path.join("settings-v1.json"))
        .map_err(|e| e.to_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_roundtrip_and_failed_write_preserves_preferences() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        assert!(read(&path).unwrap().auto_scan_scenes);
        let value = Preferences {
            auto_scan_scenes: false,
            scene_detail: Detail::Reduced,
            ..Preferences::default()
        };
        write(&path, &value).unwrap();
        let restored = read(&path).unwrap();
        assert!(!restored.auto_scan_scenes);
        assert!(matches!(restored.scene_detail, Detail::Reduced));
        let original = fs::read(&path).unwrap();
        let mut invalid = value;
        invalid.timing.cue_gap_ms = 10_001;
        assert!(write(&path, &invalid).is_err());
        assert_eq!(fs::read(&path).unwrap(), original);
        assert!(write(dir.path(), &Preferences::default()).is_err());
        fs::write(&path, b"invalid").unwrap();
        assert!(read(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"invalid");
    }
}
