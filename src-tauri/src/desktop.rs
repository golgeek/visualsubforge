//! Native menus/dialogs route to the same frontend operation dispatcher as buttons.
use serde::Deserialize;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{
    Emitter, Manager,
    menu::{CheckMenuItemBuilder, MenuBuilder, MenuItemBuilder, SubmenuBuilder},
};
#[cfg(not(target_os = "linux"))]
use tauri_plugin_dialog::DialogExt;

#[derive(Default)]
pub struct ExitGate(pub AtomicBool);

pub fn install(app: &tauri::App) -> tauri::Result<()> {
    let item = |id: &str, title: &str, shortcut: &str| {
        MenuItemBuilder::with_id(id, title)
            .accelerator(shortcut)
            .build(app)
    };
    #[cfg(target_os = "macos")]
    let application = SubmenuBuilder::new(app, "VisualSubForge")
        .about(None)
        .separator()
        .item(&item("settings", "Settings…", "CmdOrCtrl+,")?)
        .separator()
        .hide()
        .hide_others()
        .show_all()
        .separator()
        .quit()
        .build()?;
    let file = SubmenuBuilder::new(app, "File")
        .text("openProject", "Open Project…")
        .text("saveProject", "Save Project")
        .text("saveProjectAs", "Save Project As…")
        .separator()
        .item(&item("openMedia", "Open Media…", "CmdOrCtrl+O")?)
        .item(&item(
            "openSubtitles",
            "Open Subtitles…",
            "CmdOrCtrl+Shift+O",
        )?)
        .separator()
        .item(&item("save", "Save Subtitles", "CmdOrCtrl+S")?)
        .item(&item("saveAs", "Save Subtitles As…", "CmdOrCtrl+Shift+S")?)
        .separator()
        .text("closeMedia", "Close Media")
        .text("closeSubtitles", "Close Subtitles")
        .build()?;
    let edit = SubmenuBuilder::new(app, "Edit")
        .item(&item("undo", "Undo", "CmdOrCtrl+Z")?)
        .item(&item("redo", "Redo", "CmdOrCtrl+Shift+Z")?)
        .item(&item("apply", "Apply Cue Edit", "CmdOrCtrl+Return")?)
        .separator()
        .item(&item("createCue", "Create Cue", "CmdOrCtrl+Shift+N")?)
        .item(&item("deleteCue", "Delete Cue", "CmdOrCtrl+Shift+D")?)
        .item(&item("splitCue", "Split at Playhead", "CmdOrCtrl+Shift+K")?)
        .item(&item(
            "mergePrevious",
            "Merge with Previous",
            "CmdOrCtrl+Shift+H",
        )?)
        .item(&item("mergeNext", "Merge with Next", "CmdOrCtrl+Shift+J")?)
        .separator()
        .cut()
        .copy()
        .paste()
        .select_all();
    #[cfg(not(target_os = "macos"))]
    let edit = edit
        .separator()
        .item(&item("settings", "Settings…", "CmdOrCtrl+,")?);
    let edit = edit.build()?;
    let view = SubmenuBuilder::new(app, "View")
        .item(
            &CheckMenuItemBuilder::with_id("showScenes", "Show Scene Changes")
                .checked(true)
                .build(app)?,
        )
        .separator()
        .text("resetLayout", "Reset Pane Sizes")
        .item(
            &CheckMenuItemBuilder::with_id("diagnostics", "Playback Diagnostics")
                .checked(false)
                .build(app)?,
        )
        .build()?;
    let playback = SubmenuBuilder::new(app, "Playback")
        .item(&item("play", "Play / Pause", "CmdOrCtrl+Alt+Return")?)
        .text("previousFrame", "Previous Frame")
        .text("nextFrame", "Next Frame")
        .separator()
        .text("playRange", "Loop Selected Range")
        .text("clearLoop", "Clear Loop")
        .build()?;
    let tools = SubmenuBuilder::new(app, "Tools")
        .text("scanScenes", "Scan Scene Changes")
        .text("cancelSceneScan", "Cancel Scene Scan")
        .separator()
        .text("previousScene", "Previous Scene Change")
        .text("nextScene", "Next Scene Change")
        .separator()
        .text("timingSettings", "Timing and Scene Settings…")
        .build()?;
    let menu = MenuBuilder::new(app);
    #[cfg(target_os = "macos")]
    let menu = menu.item(&application);
    let menu = menu
        .items(&[&file, &edit, &view, &playback, &tools])
        .build()?;
    app.set_menu(menu)?;
    app.on_menu_event(|app, event| {
        let _ = app.emit("workspace-action", event.id().as_ref());
    });
    Ok(())
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MenuState {
    diagnostics: bool,
    available: bool,
    media: bool,
    document: bool,
    undo: bool,
    redo: bool,
    range: bool,
    busy: bool,
    create_cue: bool,
    delete_cue: bool,
    split_cue: bool,
    merge_previous: bool,
    merge_next: bool,
    show_scenes: bool,
    scenes_running: bool,
    scenes_pending: bool,
    previous_scene: bool,
    next_scene: bool,
}
#[tauri::command]
pub fn update_menu(app: tauri::AppHandle, state: MenuState) -> Result<(), String> {
    if let Some(menu) = app.menu() {
        for (id, enabled) in [
            ("settings", true),
            ("timingSettings", true),
            (
                "scanScenes",
                state.media && !state.scenes_running && !state.scenes_pending,
            ),
            (
                "cancelSceneScan",
                state.media && state.scenes_running && !state.scenes_pending,
            ),
            ("previousScene", state.media && state.previous_scene),
            ("nextScene", state.media && state.next_scene),
            ("openProject", state.available),
            ("saveProject", state.media),
            ("saveProjectAs", state.media),
            ("openMedia", state.available),
            ("openSubtitles", true),
            ("save", state.document),
            ("apply", state.document),
            ("createCue", state.create_cue),
            ("deleteCue", state.delete_cue),
            ("splitCue", state.split_cue),
            ("mergePrevious", state.merge_previous),
            ("mergeNext", state.merge_next),
            ("saveAs", state.document),
            ("closeSubtitles", state.document),
            ("closeMedia", state.media),
            ("undo", state.undo),
            ("redo", state.redo),
            ("play", state.media),
            ("previousFrame", state.media),
            ("nextFrame", state.media),
            ("playRange", state.media && state.range),
            ("clearLoop", state.media),
        ] {
            if let Some(item) = menu
                .items()
                .map_err(|e| e.to_string())?
                .into_iter()
                .find_map(|v| v.as_submenu().and_then(|submenu| submenu.get(id)))
                .and_then(|v| v.as_menuitem().cloned())
            {
                item.set_enabled(enabled && !state.busy)
                    .map_err(|e| e.to_string())?;
            }
        }
        for (id, checked) in [
            ("showScenes", state.show_scenes),
            ("diagnostics", state.diagnostics),
        ] {
            if let Some(item) = menu
                .items()
                .map_err(|e| e.to_string())?
                .into_iter()
                .find_map(|v| v.as_submenu().and_then(|submenu| submenu.get(id)))
                .and_then(|v| v.as_check_menuitem().cloned())
            {
                item.set_checked(checked).map_err(|e| e.to_string())?;
                item.set_enabled(!state.busy).map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(())
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FileChoice {
    Media,
    Subtitles,
    SaveSubtitles,
    Project,
    SaveProject,
    TimingPreset,
    SaveTimingPreset,
}
#[tauri::command]
pub async fn choose_file(
    app: tauri::AppHandle,
    kind: FileChoice,
    current: Option<String>,
) -> Result<Option<String>, String> {
    #[cfg(target_os = "linux")]
    return crate::linux_dialog::choose_file(app, kind, current).await;

    #[cfg(not(target_os = "linux"))]
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
        }
        let picked = match kind {
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
            FileChoice::SaveProject => dialog
                .set_title("Save VSF Project As")
                .add_filter("VSF project", &["vsfproject"])
                .blocking_save_file(),
            FileChoice::Media => dialog.set_title("Open Media").blocking_pick_file(),
            FileChoice::Subtitles => dialog
                .set_title("Open Subtitles")
                .add_filter("SubRip subtitles", &["srt"])
                .blocking_pick_file(),
            FileChoice::SaveSubtitles => dialog
                .set_title("Save Subtitles As")
                .add_filter("SubRip subtitles", &["srt"])
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
pub async fn finish_exit(
    app: tauri::AppHandle,
    generation: u64,
    revision: u64,
    discard: bool,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<crate::document::Documents>()
            .approve_exit(generation, revision, discard)?;
        app.state::<crate::waveform::Waveforms>().shutdown();
        app.state::<crate::scenes::Scenes>().shutdown();
        app.state::<ExitGate>().0.store(true, Ordering::Release);
        app.exit(0);
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}
