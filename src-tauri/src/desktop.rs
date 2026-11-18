//! Native menus/dialogs route to the same frontend operation dispatcher as buttons.
mod shortcuts;
use serde::{Deserialize, Serialize};
pub(crate) use shortcuts::with_shortcuts;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{
    Manager,
    menu::{CheckMenuItemBuilder, MenuBuilder, MenuItemBuilder, SubmenuBuilder},
};
#[cfg(all(not(target_os = "linux"), not(feature = "desktop-e2e")))]
use tauri_plugin_dialog::DialogExt;
use tauri_specta::Event;

/// Shared native-menu/quit payload; the wire event name stays workspace-action.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub enum WorkspaceAction {
    CommandPalette,
    ShortcutSheet,
    ThemeSystem,
    ThemeDark,
    ThemeLight,
    DensityCompact,
    DensityComfortable,
    LayoutOptions,
    LayoutTiming,
    LayoutEditorAbove,
    LayoutReview,
    LayoutTranslation,
    LayoutBigPicture,
    Problems,
    Statistics,
    FixGaps,
    FixReadingSpeed,
    SnapShots,
    ShiftCues,
    PreviousProblem,
    NextProblem,
    Find,
    Replace,
    ZoomIn,
    ZoomOut,
    FitWaveform,
    SetStart,
    SetEnd,
    Replay,
    SeekBack,
    SeekForward,
    TextTools,
    Styles,
    Fonts,
    FollowPlayhead,
    Settings,
    TimingSettings,
    Resync,
    Plugins,
    TranslationReference,
    ShowScenes,
    ScanScenes,
    CancelSceneScan,
    PreviousScene,
    NextScene,
    OpenProject,
    ImportLegacyProject,
    SaveProject,
    SaveProjectAs,
    CreateCue,
    DeleteCue,
    SplitCue,
    MergePrevious,
    MergeNext,
    Apply,
    OpenMedia,
    OpenSubtitles,
    ExtractSubtitles,
    Save,
    SaveAs,
    ConvertSubtitles,
    CloseMedia,
    CloseSubtitles,
    Undo,
    Redo,
    Play,
    PreviousFrame,
    NextFrame,
    PlayRange,
    ClearLoop,
    ResetLayout,
    ArrangePanels,
    Diagnostics,
    Quit,
}
impl WorkspaceAction {
    fn from_menu_id(id: &str) -> Option<Self> {
        // Standard OS menu items are not application actions.
        Self::deserialize(serde::de::value::StrDeserializer::<serde::de::value::Error>::new(id))
            .ok()
    }
}

#[derive(Default)]
pub struct ExitGate(pub AtomicBool);

#[derive(Deserialize)]
struct ActionDescription {
    native: bool,
    group: String,
    order: u32,
    label: String,
    shortcut: String,
}
fn action_descriptions() -> tauri::Result<std::collections::BTreeMap<String, ActionDescription>> {
    serde_json::from_str(include_str!(
        "../../src/lib/workspace/action-descriptions.json"
    ))
    .map_err(|error| tauri::Error::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, error)))
}

pub fn install(app: &tauri::App) -> tauri::Result<()> {
    app.manage(shortcuts::Runtime::default());
    let descriptions = action_descriptions()?;
    let description = |id: &str| {
        descriptions.get(id).ok_or_else(|| {
            tauri::Error::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Missing action description: {id}"),
            ))
        })
    };
    let item = |id: &str| {
        let action = description(id)?;
        let builder = MenuItemBuilder::with_id(id, &action.label);
        let builder = if action.shortcut.is_empty() {
            builder
        } else {
            builder.accelerator(&action.shortcut)
        };
        builder.build(app)
    };
    #[cfg(target_os = "macos")]
    let application = SubmenuBuilder::new(app, "VisualSubForge")
        .about(None)
        .separator()
        .item(&item("settings")?)
        .separator()
        .hide()
        .hide_others()
        .show_all()
        .separator()
        .quit()
        .build()?;
    let checked_ids = [
        "translationReference",
        "showScenes",
        "diagnostics",
        "problems",
        "statistics",
        "followPlayhead",
        "layoutTiming",
        "layoutEditorAbove",
        "layoutReview",
        "layoutTranslation",
        "layoutBigPicture",
    ];
    let mut ordered: Vec<_> = descriptions.iter().collect();
    ordered.sort_by_key(|(_, action)| action.order);
    let mut groups = Vec::new();
    for name in ["File", "Edit", "Cue", "Playback", "Review", "View", "Help"] {
        let mut group = SubmenuBuilder::new(app, name);
        if name == "Edit" {
            group = group
                .item(&item("undo")?)
                .item(&item("redo")?)
                .separator()
                .cut()
                .copy()
                .paste()
                .select_all()
                .separator();
        }
        for &(id, action) in &ordered {
            if (name == "Edit" && matches!(id.as_str(), "undo" | "redo"))
                || !action.native
                || action.group != name
                || id.starts_with("theme")
                || id.starts_with("density")
                || id.starts_with("layout")
                || id == "quit"
                || id == "arrangePanels"
                || id == "resetLayout"
            {
                continue;
            }
            #[cfg(target_os = "macos")]
            if id == "settings" {
                continue;
            }
            if checked_ids.contains(&id.as_str()) {
                let check = CheckMenuItemBuilder::with_id(id, &action.label);
                let check = if action.shortcut.is_empty() {
                    check
                } else {
                    check.accelerator(&action.shortcut)
                };
                group = group.item(&check.build(app)?);
            } else {
                group = group.item(&item(id)?);
            }
        }
        if name == "View" {
            let mut layout = SubmenuBuilder::new(app, "Layout");
            for id in [
                "layoutTiming",
                "layoutEditorAbove",
                "layoutReview",
                "layoutTranslation",
                "layoutBigPicture",
            ] {
                layout = layout
                    .item(&CheckMenuItemBuilder::with_id(id, &description(id)?.label).build(app)?);
            }
            layout = layout
                .separator()
                .item(&item("layoutOptions")?)
                .item(&item("arrangePanels")?)
                .item(&item("resetLayout")?);
            group = group.item(&layout.build()?);
            for (name, ids) in [
                ("Theme", vec!["themeSystem", "themeDark", "themeLight"]),
                ("Density", vec!["densityCompact", "densityComfortable"]),
            ] {
                let mut submenu = SubmenuBuilder::new(app, name);
                for id in ids {
                    submenu = submenu.item(
                        &CheckMenuItemBuilder::with_id(id, &description(id)?.label).build(app)?,
                    );
                }
                group = group.item(&submenu.build()?);
            }
        }
        let group = group.build()?;
        groups.push(group);
    }
    let mut menu = MenuBuilder::new(app);
    #[cfg(target_os = "macos")]
    {
        menu = menu.item(&application);
    }
    for group in &groups {
        menu = menu.item(group);
    }
    app.set_menu(menu.build()?)?;
    #[cfg(target_os = "macos")]
    if let Some(help) = groups.last() {
        help.set_as_help_menu_for_nsapp()?;
    }
    app.on_menu_event(|app, event| {
        if let Some(action) = WorkspaceAction::from_menu_id(event.id().as_ref()) {
            let _ = action.emit(app);
        }
    });
    Ok(())
}
#[derive(Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MenuState {
    enabled: Vec<String>,
    checked: Vec<String>,
}
fn sync_menu(
    items: Vec<tauri::menu::MenuItemKind<tauri::Wry>>,
    state: &MenuState,
) -> Result<(), String> {
    for item in items {
        let enabled = state.enabled.iter().any(|id| id == item.id().as_ref());
        match item {
            tauri::menu::MenuItemKind::Submenu(menu) => {
                sync_menu(menu.items().map_err(|e| e.to_string())?, state)?
            }
            tauri::menu::MenuItemKind::MenuItem(item) => {
                item.set_enabled(enabled).map_err(|e| e.to_string())?
            }
            tauri::menu::MenuItemKind::Check(item) => {
                item.set_enabled(enabled).map_err(|e| e.to_string())?;
                item.set_checked(state.checked.iter().any(|id| id == item.id().as_ref()))
                    .map_err(|e| e.to_string())?;
            }
            _ => {}
        }
    }
    Ok(())
}
#[tauri::command]
#[specta::specta]
pub fn update_menu(app: tauri::AppHandle, state: MenuState) -> Result<(), String> {
    if let Some(menu) = app.menu() {
        sync_menu(menu.items().map_err(|e| e.to_string())?, &state)?;
    }
    Ok(())
}
#[derive(Debug, PartialEq, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum FileChoice {
    Workspace,
    ExtractSubtitles,
    VideoFonts,
    Media,
    Subtitles,
    Font,
    Plugin,
    SaveSubtitles,
    ExportSrt,
    ExportAss,
    ExportSsa,
    Project,
    LegacyProject,
    SaveProject,
    TimingPreset,
    SaveTimingPreset,
    QualityPreset,
    SaveQualityPreset,
}
#[tauri::command]
#[specta::specta]
pub async fn choose_file(
    app: tauri::AppHandle,
    kind: FileChoice,
    current: Option<String>,
) -> Result<Option<String>, String> {
    let subtitle_extension = if matches!(kind, FileChoice::SaveSubtitles) {
        let documents = app.state::<crate::document::Documents>().inner().clone();
        tauri::async_runtime::spawn_blocking(move || documents.subtitle_extension())
            .await
            .map_err(|e| e.to_string())??
    } else {
        match kind {
            FileChoice::ExportAss => "ass",
            FileChoice::ExportSsa => "ssa",
            _ => "srt",
        }
    };
    #[cfg(feature = "desktop-e2e")]
    {
        let _ = (current, subtitle_extension);
        app.state::<crate::desktop_test::Driver>().choose(kind)
    }

    #[cfg(all(target_os = "linux", not(feature = "desktop-e2e")))]
    return crate::linux_dialog::choose_file(app, kind, current, subtitle_extension).await;

    #[cfg(all(not(target_os = "linux"), not(feature = "desktop-e2e")))]
    tauri::async_runtime::spawn_blocking(move || {
        let mut dialog = app.dialog().file();
        if let Some(window) = app.get_webview_window("main") {
            dialog = dialog.set_parent(&window);
        }
        if let Some(current) = current {
            let path = std::path::Path::new(&current);
            if let Some(parent) = path.parent() {
                dialog = dialog.set_directory(parent);
            }
            if let Some(name) = path.file_name() {
                dialog = dialog.set_file_name(name.to_string_lossy());
            }
        } else if matches!(kind, FileChoice::SaveSubtitles) {
            dialog = dialog.set_file_name(format!("Untitled.{subtitle_extension}"));
        } else if matches!(
            kind,
            FileChoice::ExportSrt | FileChoice::ExportAss | FileChoice::ExportSsa
        ) {
            dialog = dialog.set_file_name(format!("Converted.{subtitle_extension}"));
        }
        let picked = match kind {
            FileChoice::QualityPreset => dialog
                .set_title("Import Quality Profile")
                .add_filter("VSF quality profile", &["vsfpreset", "json"])
                .blocking_pick_file(),
            FileChoice::SaveQualityPreset => dialog
                .set_title("Export Quality Profile")
                .add_filter("VSF quality profile", &["vsfpreset"])
                .blocking_save_file(),
            FileChoice::TimingPreset => dialog
                .set_title("Import Timing Preset")
                .add_filter("Timing presets", &["vsfpreset", "json", "ini"])
                .blocking_pick_file(),
            FileChoice::SaveTimingPreset => dialog
                .set_title("Export Timing Preset")
                .add_filter("VSF timing preset", &["vsfpreset"])
                .blocking_save_file(),
            FileChoice::Project => dialog
                .set_title("Open VSF Project")
                .add_filter("VSF project", &["vsfproject"])
                .blocking_pick_file(),
            FileChoice::LegacyProject => dialog
                .set_title("Import VisualSubSync Project")
                .add_filter("VisualSubSync project", &["vssprj"])
                .blocking_pick_file(),
            FileChoice::SaveProject => dialog
                .set_title("Save VSF Project As")
                .add_filter("VSF project", &["vsfproject"])
                .blocking_save_file(),
            FileChoice::Workspace => dialog
                .set_title("Open video, subtitles or project")
                .blocking_pick_file(),
            FileChoice::ExtractSubtitles => dialog
                .set_title("Open Subtitles from Video")
                .blocking_pick_file(),
            FileChoice::VideoFonts => dialog
                .set_title("Embed Fonts from Video")
                .blocking_pick_file(),
            FileChoice::Media => dialog.set_title("Open Media").blocking_pick_file(),
            FileChoice::Font => dialog
                .set_title("Choose Font Attachment")
                .add_filter("Single fonts", &["ttf", "otf"])
                .blocking_pick_file(),
            FileChoice::Plugin => dialog
                .set_title("Choose Subtitle Plugin Manifest")
                .add_filter("Plugin manifest", &["json"])
                .blocking_pick_file(),
            FileChoice::Subtitles => dialog
                .set_title("Open Subtitles")
                .add_filter("Subtitles", &["srt", "ass", "ssa", "vtt"])
                .blocking_pick_file(),
            FileChoice::SaveSubtitles => dialog
                .set_title("Save Subtitles As")
                .add_filter("Subtitles (same format)", &[subtitle_extension])
                .blocking_save_file(),
            FileChoice::ExportSrt | FileChoice::ExportAss | FileChoice::ExportSsa => dialog
                .set_title("Export Converted Subtitles to a New File")
                .add_filter("Converted subtitles", &[subtitle_extension])
                .blocking_save_file(),
        };
        picked
            .map(|p| {
                p.into_path()
                    .map(|p| p.to_string_lossy().into_owned())
                    .map_err(|e| e.to_string())
            })
            .transpose()
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
#[specta::specta]
pub async fn finish_exit(
    app: tauri::AppHandle,
    generation: crate::ipc_number::SafeU64,
    revision: crate::ipc_number::SafeU64,
    discard: bool,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<crate::document::Documents>().approve_exit(
            generation.get(),
            revision.get(),
            discard,
        )?;
        app.state::<crate::resync::Resync>().shutdown();
        app.state::<crate::extraction::Extraction>().shutdown();
        app.state::<crate::project::Projects>().imports.shutdown()?;
        app.state::<crate::document::Documents>()
            .shutdown_quality()?;
        app.state::<crate::waveform::Waveforms>().shutdown();
        app.state::<crate::scenes::Scenes>().shutdown();
        app.state::<ExitGate>().0.store(true, Ordering::Release);
        app.exit(0);
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
pub(crate) fn ipc_sample() -> serde_json::Value {
    let values = serde_json::to_value(
        action_descriptions()
            .unwrap()
            .into_iter()
            .filter(|(_, description)| description.native)
            .map(|(id, _)| id)
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let actions: Vec<WorkspaceAction> = serde_json::from_value(values.clone()).unwrap();
    assert_eq!(serde_json::to_value(actions).unwrap(), values);
    values
}

#[test]
fn workspace_event_preserves_name_and_native_menu_payloads() {
    assert_eq!(WorkspaceAction::NAME, "workspace-action");
    for value in ipc_sample().as_array().unwrap() {
        assert!(WorkspaceAction::from_menu_id(value.as_str().unwrap()).is_some());
    }
    assert!(WorkspaceAction::from_menu_id("os-unknown").is_none());
}

#[test]
fn shared_action_descriptions_preserve_native_labels_and_accelerators() {
    let descriptions = action_descriptions().expect("valid bundled action descriptions");
    for value in ipc_sample().as_array().unwrap() {
        let id = value.as_str().unwrap();
        assert!(!descriptions[id].label.is_empty(), "{id}");
    }
    assert_eq!(descriptions["save"].shortcut, "CmdOrCtrl+S");
    assert_eq!(descriptions["splitCue"].shortcut, "CmdOrCtrl+Shift+K");
    assert_eq!(descriptions["play"].shortcut, "CmdOrCtrl+Alt+Return");
    assert_eq!(descriptions["convertSubtitles"].label, "Export As…");
}
