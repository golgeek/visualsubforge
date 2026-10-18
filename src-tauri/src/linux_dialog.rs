//! Parented portal dialogs with cancellation distinct from backend failure.
use std::path::Path;

use ashpd::{
    WindowIdentifier,
    desktop::{
        ResponseError,
        file_chooser::{FileFilter, SelectedFiles},
    },
};
use futures_util::{
    StreamExt,
    future::{Either, select},
    pin_mut,
};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use tauri::Manager;

use crate::desktop::FileChoice;

pub async fn choose_file(
    app: tauri::AppHandle,
    kind: FileChoice,
    current: Option<String>,
    subtitle_extension: &'static str,
) -> Result<Option<String>, String> {
    let window = app
        .get_webview_window("main")
        .ok_or("Main window is missing")?;
    let parent = window.clone();
    let (sender, mut receiver) = tauri::async_runtime::channel(1);
    app.run_on_main_thread(move || {
        // Borrow native handles only on GTK's main context. The async operation
        // keeps its window alive through export. ashpd performs Wayland
        // roundtrips on its own worker. Portal IO runs on Tauri's async runtime.
        // No blocking wait, file IO, decoded pixels or document state enter here.
        gtk::glib::MainContext::default().spawn_local(async move {
            let result = parent_identifier(&parent).await;
            let _ = sender.send(result).await;
        });
    })
    .map_err(|e| format!("Could not schedule the file picker: {e}"))?;
    let identifier = receiver
        .recv()
        .await
        .ok_or("File picker stopped before replying")??;
    let result = choose_with_parent(identifier, kind, current.as_deref(), subtitle_extension).await;
    // Keep the parent wrapper alive for the entire modal request. The existing
    // frontend busy/exit gate prevents normal window closure during a picker.
    drop(window);
    result
}

async fn parent_identifier(window: &tauri::WebviewWindow) -> Result<WindowIdentifier, String> {
    let handle = window
        .window_handle()
        .map_err(|e| format!("File picker parent is unavailable: {e}"))?;
    let display = window
        .display_handle()
        .map_err(|e| format!("File picker display is unavailable: {e}"))?;
    WindowIdentifier::from_raw_handle(&handle.as_raw(), Some(&display.as_raw()))
        .await
        .ok_or_else(|| "Could not identify the main window for the file picker".into())
}

async fn choose_with_parent(
    identifier: WindowIdentifier,
    kind: FileChoice,
    current: Option<&str>,
    subtitle_extension: &str,
) -> Result<Option<String>, String> {
    // Subscribe before sending: ashpd waits for Response even if the service
    // disappears after replying with a request handle. Watch only this request's
    // lifetime, without a worker task or a deadline on user interaction. Dropping
    // the losing future releases its subscription and exported parent identifier.
    let connection = zbus::Connection::session()
        .await
        .map_err(ashpd::Error::from)
        .map_err(portal_failure)?;
    let bus = zbus::fdo::DBusProxy::new(&connection)
        .await
        .map_err(ashpd::Error::from)
        .map_err(portal_failure)?;
    let mut changes = bus
        .receive_name_owner_changed_with_args(&[(0, "org.freedesktop.portal.Desktop")])
        .await
        .map_err(ashpd::Error::from)
        .map_err(portal_failure)?;
    let lost_service = async {
        while let Some(change) = changes.next().await {
            let args = change
                .args()
                .map_err(ashpd::Error::from)
                .map_err(portal_failure)?;
            // Initial activation is expected. Loss or replacement of an existing
            // owner invalidates an outstanding request, even if a new one starts.
            if args.old_owner().as_ref().is_some() {
                return Err("The desktop file picker disconnected. Reopen the picker after the session's FileChooser backend restarts.".into());
            }
        }
        Err("The desktop file picker connection closed. Check the session bus and FileChooser backend, then reopen the picker.".into())
    };
    let request = request_file(identifier, kind, current, subtitle_extension);
    pin_mut!(request, lost_service);
    match select(request, lost_service).await {
        Either::Left((result, _)) | Either::Right((result, _)) => result,
    }
}

async fn request_file(
    identifier: WindowIdentifier,
    kind: FileChoice,
    current: Option<&str>,
    subtitle_extension: &str,
) -> Result<Option<String>, String> {
    let path = current.map(Path::new);
    let folder = path.and_then(Path::parent);
    let project = matches!(kind, FileChoice::Project | FileChoice::SaveProject);
    let legacy_project = matches!(kind, FileChoice::LegacyProject);
    let font = matches!(kind, FileChoice::Font);
    let plugin = matches!(kind, FileChoice::Plugin);
    let conversion = matches!(
        kind,
        FileChoice::ExportSrt | FileChoice::ExportAss | FileChoice::ExportSsa
    );
    let preset = matches!(
        kind,
        FileChoice::TimingPreset | FileChoice::SaveTimingPreset
    );
    let saving_preset = matches!(kind, FileChoice::SaveTimingPreset);
    let quality = matches!(
        kind,
        FileChoice::QualityPreset | FileChoice::SaveQualityPreset
    );
    let filter = || {
        if plugin {
            FileFilter::new("Plugin manifest").glob("*.json")
        } else if font {
            FileFilter::new("Single fonts").glob("*.ttf").glob("*.otf")
        } else if quality {
            FileFilter::new("VSF quality profile")
                .glob("*.vsfpreset")
                .glob("*.json")
        } else if preset {
            if saving_preset {
                FileFilter::new("VSF timing preset").glob("*.vsfpreset")
            } else {
                FileFilter::new("Timing presets")
                    .glob("*.vsfpreset")
                    .glob("*.json")
                    .glob("*.ini")
            }
        } else if project {
            FileFilter::new("VSF project").glob("*.vsfproject")
        } else if legacy_project {
            FileFilter::new("VisualSubSync project").glob("*.vssprj")
        } else if conversion {
            FileFilter::new("Converted subtitles").glob(format!("*.{subtitle_extension}"))
        } else if matches!(kind, FileChoice::SaveSubtitles) {
            FileFilter::new("Subtitles (same format)").glob(format!("*.{subtitle_extension}"))
        } else {
            FileFilter::new("Subtitles")
                .glob("*.srt")
                .glob("*.ass")
                .glob("*.ssa")
                .glob("*.vtt")
        }
    };
    let default_subtitle_name = format!(
        "{}.{subtitle_extension}",
        if conversion { "Converted" } else { "Untitled" }
    );
    let response = match kind {
        FileChoice::SaveSubtitles
        | FileChoice::ExportSrt
        | FileChoice::ExportAss
        | FileChoice::ExportSsa
        | FileChoice::SaveProject
        | FileChoice::SaveTimingPreset
        | FileChoice::SaveQualityPreset => {
            SelectedFiles::save_file()
                .identifier(identifier)
                .title(if conversion {
                    "Export Converted Subtitles to a New File"
                } else if quality {
                    "Export Quality Profile"
                } else if preset {
                    "Export Timing Preset"
                } else if project {
                    "Save VSF Project As"
                } else {
                    "Save Subtitles As"
                })
                .modal(true)
                .filter(filter())
                .current_name(
                    path.and_then(Path::file_name)
                        .and_then(|name| name.to_str())
                        .or_else(|| {
                            (conversion || matches!(kind, FileChoice::SaveSubtitles))
                                .then_some(default_subtitle_name.as_str())
                        }),
                )
                .current_folder::<&Path>(folder)
                .map_err(portal_failure)?
                .send()
                .await
        }
        FileChoice::Media
        | FileChoice::Font
        | FileChoice::Plugin
        | FileChoice::Subtitles
        | FileChoice::Project
        | FileChoice::LegacyProject
        | FileChoice::QualityPreset
        | FileChoice::TimingPreset => {
            let subtitles = matches!(kind, FileChoice::Subtitles);
            let mut request = SelectedFiles::open_file()
                .identifier(identifier)
                .title(if plugin {
                    "Choose Subtitle Plugin Manifest"
                } else if font {
                    "Choose Font Attachment"
                } else if quality {
                    "Import Quality Profile"
                } else if preset {
                    "Import Timing Preset"
                } else if project {
                    "Open VSF Project"
                } else if legacy_project {
                    "Import VisualSubSync Project"
                } else if subtitles {
                    "Open Subtitles"
                } else {
                    "Open Media"
                })
                .modal(true)
                .multiple(false)
                .current_folder::<&Path>(folder)
                .map_err(portal_failure)?;
            if subtitles || project || legacy_project || preset || quality || font || plugin {
                request = request.filter(filter());
            }
            request.send().await
        }
    }
    .and_then(|request| request.response());
    selected_path(response)
}

fn portal_failure(error: ashpd::Error) -> String {
    format!(
        "The desktop file picker failed. Check that the session's xdg-desktop-portal and FileChooser backend are running. Details: {error}"
    )
}

fn selected_path(response: Result<SelectedFiles, ashpd::Error>) -> Result<Option<String>, String> {
    let files = match response {
        Err(ashpd::Error::Response(ResponseError::Cancelled)) => return Ok(None),
        Err(error) => return Err(portal_failure(error)),
        Ok(files) => files,
    };
    selected_uri(files.uris()).map(Some)
}

fn selected_uri(uris: &[url::Url]) -> Result<String, String> {
    let [uri] = uris else {
        return Err("The file picker did not return exactly one file".into());
    };
    let path = uri
        .to_file_path()
        .map_err(|()| "The file picker returned a non-local file")?;
    let path = path
        .into_os_string()
        .into_string()
        .map_err(|_| "The selected path is not valid UTF-8")?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    // A fresh process is required for each private bus because ashpd caches its
    // session connection. The harness owns the bus/service and bounds the test;
    // no display, native window, user files or real desktop service is involved.
    #[test]
    #[ignore = "run with tests/native/linux-dialogs/check.py on Linux"]
    fn private_portal_lifecycle() {
        let case = std::env::var("VSF_PORTAL_TEST_CASE").expect("private portal harness required");
        let result = tauri::async_runtime::block_on(choose_with_parent(
            WindowIdentifier::from_xid(0),
            FileChoice::Subtitles,
            None,
            "srt",
        ));
        match case.as_str() {
            "success" => assert_eq!(result, Ok(Some("/tmp/café subtitle.srt".into()))),
            "cancel" => assert_eq!(result, Ok(None)),
            "disconnect" | "replace" => {
                assert!(result.unwrap_err().contains("file picker disconnected"));
            }
            "restart" => {
                assert!(result.unwrap_err().contains("file picker disconnected"));
                // The replacement service starts independently. Retry in this
                // same process to exercise ashpd's cached session connection.
                for _ in 0..20 {
                    let retry = tauri::async_runtime::block_on(choose_with_parent(
                        WindowIdentifier::from_xid(0),
                        FileChoice::Subtitles,
                        None,
                        "srt",
                    ));
                    if let Ok(path) = retry {
                        assert_eq!(path, Some("/tmp/café subtitle.srt".into()));
                        return;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                panic!("file picker did not recover after service restart");
            }
            "bus-loss" => assert!(result.is_err(), "bus loss must not look like cancellation"),
            _ => panic!("unknown private portal case: {case}"),
        }
    }

    #[test]
    fn cancellation_is_distinct_from_failure() {
        assert_eq!(
            selected_path(Err(ashpd::Error::Response(ResponseError::Cancelled))),
            Ok(None)
        );
        for error in [
            ashpd::Error::Response(ResponseError::Other),
            ashpd::Error::NoResponse,
            ashpd::Error::IO(std::io::Error::from(std::io::ErrorKind::NotFound)),
        ] {
            assert!(
                selected_path(Err(error))
                    .unwrap_err()
                    .contains("FileChooser backend")
            );
        }
    }

    #[test]
    fn preserves_local_unicode_path_and_rejects_invalid_selection() {
        assert_eq!(
            selected_uri(&["file:///tmp/caf%C3%A9%20subtitle.srt".parse().unwrap()]),
            Ok("/tmp/café subtitle.srt".into())
        );
        for uris in [
            vec![],
            vec!["file:///tmp/a", "file:///tmp/b"],
            vec!["https://example.com/a"],
            vec!["file:///tmp/%FF"],
        ] {
            let uris: Vec<url::Url> = uris.iter().map(|uri| uri.parse().unwrap()).collect();
            assert!(selected_uri(&uris).is_err());
        }
    }
}
