#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod analysis_tools;
mod analysis_worker;
mod atomic_file;
#[cfg(all(feature = "media-prototype", target_os = "macos"))]
mod audio_smoke;
mod desktop;
mod document;
#[cfg(all(feature = "media-prototype", target_os = "macos"))]
mod extended_playback;
mod file_lease;
mod ipc;
mod ipc_number;
#[cfg(target_os = "linux")]
mod linux_dialog;
#[cfg(all(feature = "media-prototype", target_os = "linux"))]
mod linux_locale;
mod media;
#[cfg(all(feature = "media-prototype", target_os = "macos"))]
mod media_memory;
#[cfg(all(
    feature = "media-prototype",
    any(target_os = "macos", target_os = "windows", target_os = "linux")
))]
mod media_smoke;
#[cfg(all(
    feature = "media-prototype",
    any(target_os = "macos", target_os = "linux")
))]
mod media_soak;
#[cfg(any(
    test,
    all(
        feature = "media-prototype",
        any(target_os = "macos", target_os = "windows", target_os = "linux")
    )
))]
mod media_transport;
#[cfg(target_os = "macos")]
mod native_quit;
#[cfg(all(
    feature = "media-prototype",
    any(target_os = "macos", target_os = "windows", target_os = "linux")
))]
mod native_surface;
#[cfg(all(feature = "media-prototype", target_os = "macos"))]
mod playback_bench;
mod preferences;
mod project;
mod recovery_store;
mod save_lease;
mod scenes;
mod srt_limits;
#[cfg(any(
    test,
    all(
        feature = "media-prototype",
        any(target_os = "macos", target_os = "windows", target_os = "linux")
    )
))]
mod subtitle_preview;
mod timing_preset;
mod waveform;
mod workflow_trace;
use std::sync::atomic::Ordering;
use tauri::Manager;
use tauri_specta::Event;

fn main() {
    #[cfg(all(feature = "media-prototype", target_os = "linux"))]
    {
        // SAFETY: first startup operation, before Tauri, GTK or our workers.
        #[allow(unsafe_code)]
        if let Err(error) = unsafe { linux_locale::initialize() } {
            eprintln!("Unable to initialize VisualSubForge locale: {error}");
            std::process::exit(1);
        }
    }
    let ipc = ipc::bindings();
    let builder = tauri::Builder::default();
    #[cfg(not(target_os = "linux"))]
    let builder = builder.plugin(tauri_plugin_dialog::init());
    let app = builder
        .invoke_handler(ipc.invoke_handler())
        .setup(move |app| {
            ipc.mount_events(app);
            let documents = document::Documents::default();
            documents.start_recovery(app.path().app_data_dir()?.join("recovery-v1"))?;
            app.manage(documents);
            app.manage(project::Projects::default());
            app.manage(desktop::ExitGate::default());
            app.manage(waveform::Waveforms::default());
            app.manage(scenes::Scenes::default());
            desktop::install(app)?;
            #[cfg(target_os = "windows")]
            {
                // Native resizing can expose the window/WebView background before
                // the next web content frame. Match the workspace's dark base.
                app.get_webview_window("main")
                    .ok_or("Main window is missing")?
                    .set_background_color(Some(tauri::window::Color(23, 35, 45, 255)))?;
            }
            #[cfg(target_os = "macos")]
            native_quit::install(app.handle())?;
            #[cfg(all(feature = "media-prototype", target_os = "macos"))]
            {
                let window = app
                    .get_webview_window("main")
                    .ok_or("Main window is missing")?;
                // Legacy experiment uses a transparent webview; ANGLE keeps it opaque.
                // Wry currently implements macOS webview transparency with private
                // WebKit drawsBackground KVC; see decision 0003 before shipping.
                window.set_background_color(Some(tauri::window::Color(23, 35, 45, 255)))?;
                #[cfg(not(feature = "macos-angle"))]
                window
                    .as_ref()
                    .set_background_color(Some(tauri::window::Color(0, 0, 0, 0)))?;
                let surface = native_surface::attach(&window)?;
                app.manage(media::Media::start(
                    surface,
                    app.state::<document::Documents>().inner().clone(),
                )?);
                if let Ok(path) = std::env::var("VSF_AUDIO_SMOKE") {
                    audio_smoke::start(app.handle().clone(), path);
                } else if let Ok(directory) = std::env::var("VSF_EXTENDED_PLAYBACK") {
                    extended_playback::start(app.handle().clone(), directory);
                } else if let Ok(directory) = std::env::var("VSF_MEDIA_MEMORY") {
                    media_memory::start(app.handle().clone(), directory);
                } else if let Ok(directory) = std::env::var("VSF_MEDIA_SOAK") {
                    media_soak::start(app.handle().clone(), directory);
                } else if let Ok(directory) = std::env::var("VSF_PLAYBACK_BENCH") {
                    playback_bench::start(app.handle().clone(), directory);
                } else if let Ok(path) = std::env::var("VSF_MEDIA_SMOKE") {
                    media_smoke::start(app.handle().clone(), path);
                }
            }
            #[cfg(all(
                feature = "media-prototype",
                any(target_os = "windows", target_os = "linux")
            ))]
            {
                let window = app
                    .get_webview_window("main")
                    .ok_or("Main window is missing")?;
                let surface = native_surface::attach(&window)?;
                app.manage(media::Media::start(
                    surface,
                    app.state::<document::Documents>().inner().clone(),
                )?);
                #[cfg(target_os = "linux")]
                if let Ok(directory) = std::env::var("VSF_MEDIA_SOAK") {
                    media_soak::start(app.handle().clone(), directory);
                    return Ok(());
                }
                if let Ok(path) = std::env::var("VSF_MEDIA_SMOKE") {
                    media_smoke::start(app.handle().clone(), path);
                }
            }
            #[cfg(not(all(
                feature = "media-prototype",
                any(target_os = "macos", target_os = "windows", target_os = "linux")
            )))]
            app.manage(media::Media::default());
            Ok(())
        })
        .build(tauri::generate_context!());
    match app {
        Ok(app) => app.run(|app, event| {
            // Linux can deliver runtime events before setup installs media.
            // No native surface exists to protect at that point; querying
            // required state would panic before the window can finish setup.
            let Some(media) = app.try_state::<media::Media>() else {
                return;
            };
            match event {
                tauri::RunEvent::WindowEvent {
                    event: tauri::WindowEvent::CloseRequested { api, .. },
                    ..
                } => {
                    // Retain the live drawable until renderer and AppKit cleanup finish.
                    api.prevent_close();
                    app.exit(0);
                }
                tauri::RunEvent::ExitRequested { api, .. } => {
                    if !app.state::<desktop::ExitGate>().0.load(Ordering::Acquire) {
                        api.prevent_exit();
                        let _ = desktop::WorkspaceAction::Quit.emit(app);
                        return;
                    }
                    if !media.available() || media.shutdown_complete() {
                        return;
                    }
                    api.prevent_exit();
                    if media.begin_shutdown() {
                        app.state::<waveform::Waveforms>().cancel();
                        app.state::<scenes::Scenes>().cancel();
                        let app = app.clone();
                        tauri::async_runtime::spawn_blocking(move || {
                            app.state::<media::Media>().shutdown();
                            app.exit(0); // completion flag permits this final exit
                        });
                    }
                }
                _ => {}
            }
        }),
        Err(error) => {
            eprintln!("Unable to run VisualSubForge: {error}");
            std::process::exit(1);
        }
    }
}

// Opt-in diagnostic automation uses normal asynchronous native cleanup.
#[cfg(all(
    feature = "media-prototype",
    any(target_os = "macos", target_os = "windows", target_os = "linux")
))]
fn diagnostic_exit(app: &tauri::AppHandle, success: bool) {
    if std::env::var_os("VSF_DIAGNOSTIC_EXIT").is_some() {
        app.state::<desktop::ExitGate>()
            .0
            .store(true, Ordering::Release);
        app.state::<waveform::Waveforms>().shutdown();
        app.state::<scenes::Scenes>().shutdown();
        // Complete cleanup before asking to exit so failure codes are preserved.
        let media = app.state::<media::Media>();
        if media.begin_shutdown() {
            media.shutdown();
            app.exit(if success { 0 } else { 1 });
        }
    }
}
