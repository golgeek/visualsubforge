//! Data-only timing policies. No plugin code is loaded or evaluated.
use serde::{Deserialize, Serialize};
use std::{fs, io::Read, path::Path};
type Result<T> = std::result::Result<T, String>;
const MAX_BYTES: u64 = 256 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Settings {
    pub enabled: bool,
    pub snap_cues: bool,
    pub snap_scenes: bool,
    pub cue_gap_ms: u32,
    pub scene_before_ms: u32,
    pub scene_after_ms: u32,
    pub snap_distance_px: u32,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: false,
            snap_cues: true,
            snap_scenes: true,
            cue_gap_ms: 80,
            scene_before_ms: 130,
            scene_after_ms: 130,
            snap_distance_px: 8,
        }
    }
}
impl Settings {
    pub fn validate(&self) -> Result<()> {
        if [self.cue_gap_ms, self.scene_before_ms, self.scene_after_ms]
            .iter()
            .any(|n| *n > 10_000)
            || !(1..=32).contains(&self.snap_distance_px)
        {
            return Err("Timing buffers must be 0–10000 ms and snap distance 1–32 pixels".into());
        }
        Ok(())
    }
}
#[derive(Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Preset {
    version: u32,
    name: String,
    timing: Settings,
}
impl Preset {
    fn validate(&self) -> Result<()> {
        if self.version != 1 {
            return Err("Unsupported timing preset version".into());
        }
        if self.name.trim().is_empty()
            || self.name.chars().count() > 120
            || self.name.chars().any(char::is_control)
        {
            return Err("Preset name must contain 1–120 printable characters".into());
        }
        self.timing.validate()
    }
}
#[derive(Serialize, specta::Type)]
pub struct Imported {
    preset: Preset,
    warnings: Vec<String>,
}
fn legacy(text: &str, name: &str) -> Result<Imported> {
    let mut settings = Settings::default();
    let mut section = String::new();
    let mut mapped = 0;
    let mut ignored = 0;
    let mut seen = std::collections::HashSet::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with([';', '#']) {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].to_ascii_lowercase();
            continue;
        }
        let (key, value) = line.split_once('=').ok_or("Malformed VSS INI entry")?;
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim();
        let pair = (section.as_str(), key.as_str());
        let target = match pair {
            ("misc", "spacekeyblankbetweensubtitles") => Some(&mut settings.cue_gap_ms),
            ("wavdisplay", "scenechangestartoffset") => Some(&mut settings.scene_before_ms),
            ("wavdisplay", "scenechangestopoffset") => Some(&mut settings.scene_after_ms),
            _ => None,
        };
        let supported = target.is_some() || pair == ("misc", "enablemousesnapping");
        if supported && !seen.insert((section.clone(), key.clone())) {
            return Err(format!("Duplicate timing setting: {key}"));
        }
        if let Some(target) = target {
            *target = value
                .parse()
                .map_err(|_| format!("Invalid nonnegative millisecond value for {key}"))?;
            mapped += 1;
        } else if pair == ("misc", "enablemousesnapping") {
            settings.enabled = match value.to_ascii_lowercase().as_str() {
                "1" | "true" => true,
                "0" | "false" => false,
                _ => return Err("Invalid EnableMouseSnapping boolean".into()),
            };
            mapped += 1;
        } else {
            ignored += 1;
        }
    }
    if mapped == 0 {
        return Err("No supported VSS timing settings found".into());
    }
    let preset = Preset {
        version: 1,
        name: name.into(),
        timing: settings,
    };
    preset.validate()?;
    Ok(Imported {
        preset,
        warnings: vec![format!(
            "VSS timing subset: {mapped} fields imported, {ignored} unsupported entries ignored; omitted fields use VSF defaults. Plugin rules, frame conversion and scene filtering are not imported"
        )],
    })
}
fn read(path: &Path) -> Result<Imported> {
    let file = fs::File::open(path).map_err(|e| format!("Cannot open timing preset: {e}"))?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("Preset must be a regular file".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("Timing preset exceeds 256 KiB".into());
    }
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| "Timing preset must be UTF-8")?
        .trim_start_matches('\u{feff}');
    if path
        .extension()
        .is_some_and(|s| s.eq_ignore_ascii_case("ini"))
    {
        return legacy(
            text,
            path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Imported VSS timing"),
        );
    }
    let preset: Preset =
        serde_json::from_str(text).map_err(|e| format!("Invalid timing preset: {e}"))?;
    preset.validate()?;
    Ok(Imported {
        preset,
        warnings: Vec::new(),
    })
}
fn write(path: &Path, preset: &Preset) -> Result<()> {
    preset.validate()?;
    if path.extension().is_none_or(|s| s != "vsfpreset") {
        return Err("Preset filename must end in .vsfpreset".into());
    }
    let bytes = serde_json::to_vec_pretty(preset).map_err(|e| e.to_string())?;
    crate::atomic_file::write(path, &bytes)
}
#[tauri::command]
#[specta::specta]
pub async fn timing_preset_open(path: String) -> Result<Imported> {
    tauri::async_runtime::spawn_blocking(move || read(Path::new(&path)))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
#[specta::specta]
pub async fn timing_preset_save(path: String, preset: Preset) -> Result<()> {
    tauri::async_runtime::spawn_blocking(move || write(Path::new(&path), &preset))
        .await
        .map_err(|e| e.to_string())?
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn portable_roundtrip_and_failed_save_preserve_previous_preset() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("norm.vsfpreset");
        let mut preset = Preset {
            version: 1,
            name: "Community timing".into(),
            timing: Settings {
                enabled: true,
                cue_gap_ms: 100,
                ..Settings::default()
            },
        };
        write(&path, &preset).unwrap();
        assert_eq!(read(&path).unwrap().preset.timing, preset.timing);
        let original = fs::read(&path).unwrap();
        preset.timing.cue_gap_ms = 10001;
        assert!(write(&path, &preset).is_err());
        assert_eq!(fs::read(&path).unwrap(), original);
    }
    #[test]
    fn vss_subset_maps_core_fields_and_reports_unsupported_plugin_entries() {
        let imported = legacy("[Misc]\nEnableMouseSnapping=1\nSpaceKeyBlankBetweenSubtitles=80\n[WAVDisplay]\nSceneChangeStartOffset=120\nSceneChangeStopOffset=160\nSceneChangeFilterOffset=250\n[Plugin]\nEnabled=1", "Example").unwrap();
        assert_eq!(
            imported.preset.timing,
            Settings {
                enabled: true,
                scene_before_ms: 120,
                scene_after_ms: 160,
                ..Settings::default()
            }
        );
        assert!(imported.warnings[0].contains("2 unsupported"));
        assert!(legacy("[Misc]\nSpaceKeyBlankBetweenSubtitles=-1", "Bad").is_err());
        assert!(
            legacy(
                "[Misc]\nEnableMouseSnapping=1\nEnableMouseSnapping=0",
                "Bad"
            )
            .is_err()
        );
        assert!(legacy("[Plugin]\nEnabled=1", "No timing").is_err());
    }
    #[test]
    fn unknown_version_fields_and_oversized_files_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.vsfpreset");
        let preset = Preset {
            version: 2,
            name: "Bad".into(),
            timing: Settings::default(),
        };
        fs::write(&path, serde_json::to_vec(&preset).unwrap()).unwrap();
        assert!(read(&path).is_err());
        fs::write(&path, vec![b' '; MAX_BYTES as usize + 1]).unwrap();
        assert!(read(&path).is_err());
        let mut json = serde_json::to_value(&preset).unwrap();
        json["version"] = 1.into();
        json["execute"] = "script.js".into();
        fs::write(&path, serde_json::to_vec(&json).unwrap()).unwrap();
        assert!(read(&path).is_err());
    }
}
