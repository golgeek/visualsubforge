//! Language preferences adapter. Dictionary work uses the existing text worker.
use crate::{document::Documents, ipc_number::SafeU64};
use serde::{Deserialize, Serialize};
use tauri::Manager;
use vsf_application::language::{Language, WordLists};

#[derive(Serialize, specta::Type)]
pub(crate) struct Words {
    pub epoch: SafeU64,
    pub personal: WordLists,
    pub ignored: WordLists,
}
#[derive(Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum Change {
    Ignore { language: Language, word: String },
    Remember { language: Language, word: String },
    Forget { language: Language, word: String },
    ClearIgnored,
}
#[tauri::command]
#[specta::specta]
pub(crate) async fn language_words(app: tauri::AppHandle) -> Result<Words, String> {
    tauri::async_runtime::spawn_blocking(move || app.state::<Documents>().language_words())
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
#[specta::specta]
pub(crate) async fn language_words_change(
    app: tauri::AppHandle,
    epoch: SafeU64,
    change: Change,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = crate::preferences::settings_path(&app)?;
        app.state::<Documents>()
            .language_words_change(epoch.get(), change, |personal| {
                let mut preferences = crate::preferences::read(&path)?;
                preferences.personal_words = personal.clone();
                crate::preferences::write(&path, &preferences)
            })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Serialize, specta::Type)]
pub(crate) struct Notices {
    pub english: String,
    pub french: String,
    pub mpl: String,
    pub sources: String,
}
#[tauri::command]
#[specta::specta]
pub(crate) fn language_notices() -> Notices {
    Notices {
        english: include_str!("../../crates/vsf-application/assets/dictionaries/README-en-US.txt")
            .into(),
        french: include_str!("../../crates/vsf-application/assets/dictionaries/README-fr.txt")
            .into(),
        mpl: include_str!("../../crates/vsf-application/assets/dictionaries/MPL-2.0.txt").into(),
        sources: include_str!("../../crates/vsf-application/assets/dictionaries/sources.json")
            .into(),
    }
}
