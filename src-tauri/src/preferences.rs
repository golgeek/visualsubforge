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
    quality: crate::quality_profile::Profile,
    show_quality_metrics: bool,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            version: 1,
            auto_scan_scenes: true,
            scene_detail: Detail::Standard,
            timing: Timing::default(),
            quality: Default::default(),
            show_quality_metrics: false,
        }
    }
}
impl Preferences {
    fn validate(&self) -> Result<()> {
        if self.version != 1 {
            return Err("Unsupported settings version".into());
        }
        self.policy().map(|_| ())
    }
    fn policy(&self) -> Result<crate::quality_profile::Policy> {
        crate::quality_profile::Policy::new(self.quality.clone(), self.timing.clone())
    }
}
fn read(path: &Path) -> Result<Preferences> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Preferences::default()),
        Err(e) => return Err(format!("Cannot read settings: {e}")),
    };
    let mut bytes = Vec::new();
    file.take(256 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 256 * 1024 {
        return Err("Settings file exceeds limit".into());
    }
    let mut json: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| format!("Invalid settings: {e}"))?;
    let object = json.as_object_mut().ok_or("Settings must be an object")?;
    if !object.contains_key("quality") {
        let timing: Timing = serde_json::from_value(
            object
                .get("timing")
                .cloned()
                .ok_or("Settings lack timing values")?,
        )
        .map_err(|e| format!("Invalid timing settings: {e}"))?;
        let policy = crate::quality_profile::Policy::legacy(timing, "workspace")?;
        object.insert(
            "quality".into(),
            serde_json::to_value(policy.profile).map_err(|e| e.to_string())?,
        );
    }
    object
        .entry("showQualityMetrics")
        .or_insert(serde_json::Value::Bool(false));
    let value: Preferences =
        serde_json::from_value(json).map_err(|e| format!("Invalid settings: {e}"))?;
    value.validate()?;
    Ok(value)
}
fn write(path: &Path, value: &Preferences) -> Result<()> {
    value.validate()?;
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    if bytes.len() > 256 * 1024 {
        return Err("Settings file exceeds limit".into());
    }
    fs::create_dir_all(path.parent().ok_or("Settings path has no parent")?)
        .map_err(|e| e.to_string())?;
    crate::atomic_file::write(path, &bytes)
}
#[tauri::command]
#[specta::specta]
pub async fn preferences_get(app: tauri::AppHandle) -> Result<Preferences> {
    tauri::async_runtime::spawn_blocking(move || {
        let value = read(&settings_path(&app)?)?;
        app.state::<crate::document::Documents>()
            .initialize_quality(value.policy()?)?;
        Ok(value)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
#[specta::specta]
pub async fn preferences_save(
    app: tauri::AppHandle,
    preferences: Preferences,
    epoch: crate::ipc_number::SafeU64,
) -> Result<()> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = settings_path(&app)?;
        let policy = preferences.policy()?;
        app.state::<crate::document::Documents>()
            .apply_quality_defaults(epoch.get(), policy, || write(&path, &preferences))
    })
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
    fn old_timing_preferences_migrate_without_writes_and_invalid_profiles_preserve_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let mut old = serde_json::to_value(Preferences::default()).unwrap();
        old.as_object_mut().unwrap().remove("quality");
        old.as_object_mut().unwrap().remove("showQualityMetrics");
        old["timing"]["cueGapMs"] = 167.into();
        let bytes = serde_json::to_vec(&old).unwrap();
        fs::write(&path, &bytes).unwrap();
        let migrated = read(&path).unwrap();
        assert_eq!(migrated.quality.id, "legacy-workspace-timing");
        assert_eq!(
            migrated.quality.rules,
            crate::quality_profile::Profile::default().rules
        );
        assert_eq!(migrated.quality.timing.cue_gap_ms, 167);
        assert!(!migrated.show_quality_metrics);
        assert_eq!(fs::read(&path).unwrap(), bytes);
        write(&path, &migrated).unwrap();
        let saved = fs::read(&path).unwrap();
        assert_eq!(read(&path).unwrap().quality, migrated.quality);
        let mut mismatch = migrated.clone();
        mismatch.timing.cue_gap_ms = 80;
        assert!(write(&path, &mismatch).is_err());
        assert_eq!(fs::read(&path).unwrap(), saved);
        let mut invalid = serde_json::to_value(&migrated).unwrap();
        invalid["quality"]["textAlgorithm"] = "unknown".into();
        let bytes = serde_json::to_vec(&invalid).unwrap();
        fs::write(&path, &bytes).unwrap();
        assert!(read(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
        fs::write(&path, vec![b' '; 256 * 1024 + 1]).unwrap();
        assert!(read(&path).err().unwrap().contains("limit"));
    }
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
