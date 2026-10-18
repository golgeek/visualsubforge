//! Desktop policy projection and bounded profile-file commands. Rules remain in
//! the shared application crate; Documents owns the accepted workspace snapshot.
use crate::timing_preset::Settings as Timing;
use serde::Serialize;
use tauri::Manager;
pub(crate) use vsf_application::quality::Profile;

type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Policy {
    pub profile: Profile,
    pub timing: Timing,
}
impl Policy {
    pub fn new(profile: Profile, timing: Timing) -> Result<Self> {
        profile.validate().map_err(|e| e.to_string())?;
        timing.validate()?;
        let policy = Self { profile, timing };
        if policy.profile.timing != policy.thresholds() {
            return Err("Quality and timing thresholds differ; apply one effective profile".into());
        }
        Ok(policy)
    }
    fn thresholds(&self) -> vsf_application::quality::profile::Timing {
        vsf_application::quality::profile::Timing {
            cue_gap_ms: self.timing.cue_gap_ms,
            scene_before_ms: self.timing.scene_before_ms,
            scene_after_ms: self.timing.scene_after_ms,
        }
    }
    /// Old settings/projects have timing only. Preserve those values and use
    /// explicitly named default quality rules, never a fabricated historical norm.
    pub fn legacy(timing: Timing, origin: &str) -> Result<Self> {
        timing.validate()?;
        let mut policy = Self {
            timing,
            ..Self::default()
        };
        if policy.profile.timing != policy.thresholds() {
            policy.profile = policy.profile.custom(
                format!("legacy-{origin}-timing"),
                format!("Existing {origin} timing · default quality"),
            );
            policy.profile.timing = policy.thresholds();
        }
        Self::new(policy.profile, policy.timing)
    }
}

#[derive(Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Context {
    #[serde(with = "crate::ipc_number")]
    #[specta(type = crate::ipc_number::SafeU64)]
    pub epoch: u64,
    pub profile: Profile,
    pub timing: Timing,
}
#[derive(Serialize, specta::Type)]
pub(crate) struct SettingsData {
    pub current: Context,
    pub builtins: Vec<Profile>,
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn quality_settings(app: tauri::AppHandle) -> Result<SettingsData> {
    tauri::async_runtime::spawn_blocking(move || {
        Ok(SettingsData {
            current: app
                .state::<crate::document::Documents>()
                .quality_context()?,
            builtins: Profile::builtins(),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
#[specta::specta]
pub(crate) async fn quality_profile_open(path: String) -> Result<Profile> {
    tauri::async_runtime::spawn_blocking(move || {
        vsf_application::quality::read_profile(std::path::Path::new(&path))
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
#[specta::specta]
pub(crate) async fn quality_profile_save(path: String, profile: Profile) -> Result<()> {
    tauri::async_runtime::spawn_blocking(move || {
        vsf_application::quality::write_profile(std::path::Path::new(&path), &profile)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
