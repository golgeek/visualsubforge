//! Consent lives in Preferences. Only this service writes attempt and dismissal history.
mod network;
mod releases;

use crate::preferences;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Read,
    path::Path,
    sync::Mutex,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::Manager;
use tokio_util::sync::CancellationToken;

const REPOSITORY: &str = "golgeek/visualsubforge";
const API: &str = "https://api.github.com";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum Failure {
    Connection,
    Timeout,
    RateLimit,
    Unavailable,
    TooLarge,
    Invalid,
    Storage,
}

#[derive(Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Outcome {
    Quiet,
    Current,
    Available { version: String },
    Failed { reason: Failure },
}

#[derive(Default, Deserialize, Serialize)]
struct History {
    last_attempt: Option<u64>,
    dismissed: Option<String>,
}

fn history(
    path: &Path,
    change: impl FnOnce(&mut History) -> bool,
) -> Result<Option<History>, Failure> {
    fs::create_dir_all(path.parent().ok_or(Failure::Storage)?).map_err(|_| Failure::Storage)?;
    // Lock a stable sibling: an atomic replacement changes the history inode.
    // Concurrent app processes must reserve the same 24-hour interval.
    let writer = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path.with_extension("lock"))
        .map_err(|_| Failure::Storage)?;
    writer.lock().map_err(|_| Failure::Storage)?;
    let mut state = match fs::File::open(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => History::default(),
        Ok(file) => {
            let mut bytes = Vec::new();
            file.take(4097)
                .read_to_end(&mut bytes)
                .map_err(|_| Failure::Storage)?;
            if bytes.len() > 4096 {
                return Err(Failure::Storage);
            }
            serde_json::from_slice(&bytes).map_err(|_| Failure::Storage)?
        }
        Err(_) => return Err(Failure::Storage),
    };
    if !change(&mut state) {
        return Ok(None);
    }
    let bytes = serde_json::to_vec(&state).map_err(|_| Failure::Storage)?;
    crate::atomic_file::write(path, &bytes).map_err(|_| Failure::Storage)?;
    Ok(Some(state))
}

#[derive(Default)]
struct Session {
    active: Option<CancellationToken>,
    closed: bool,
}

#[derive(Default)]
pub struct Updates(Mutex<Session>);

impl Updates {
    pub fn shutdown(&self) {
        if let Ok(mut session) = self.0.lock() {
            session.closed = true;
            if let Some(active) = session.active.take() {
                active.cancel();
            }
        }
    }

    fn begin(&self, manual: bool) -> Option<CancellationToken> {
        let mut session = self.0.lock().ok()?;
        if session.closed || (!manual && session.active.is_some()) {
            return None;
        }
        let token = CancellationToken::new();
        if let Some(old) = session.active.replace(token.clone()) {
            old.cancel();
        }
        Some(token)
    }

    fn finish(&self, token: &CancellationToken, outcome: Outcome) -> Outcome {
        let Ok(mut session) = self.0.lock() else {
            return Outcome::Quiet;
        };
        if token.is_cancelled() {
            return Outcome::Quiet;
        }
        session.active.take();
        outcome
    }
}

fn read_fixture(path: &Path) -> Result<Vec<u8>, Failure> {
    // Reject UNC shares and device namespaces before the first filesystem call.
    #[cfg(windows)]
    if let Some(std::path::Component::Prefix(prefix)) = path.components().next()
        && !matches!(
            prefix.kind(),
            std::path::Prefix::Disk(_) | std::path::Prefix::VerbatimDisk(_)
        )
    {
        return Err(Failure::Storage);
    }
    let metadata = fs::metadata(path).map_err(|_| Failure::Storage)?;
    if !metadata.is_file() {
        return Err(Failure::Storage);
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|file| {
            file.take(releases::MAX_BYTES as u64 + 1)
                .read_to_end(&mut bytes)
        })
        .map_err(|_| Failure::Storage)?;
    if bytes.len() > releases::MAX_BYTES {
        return Err(Failure::TooLarge);
    }
    Ok(bytes)
}

async fn check(
    app: tauri::AppHandle,
    manual: bool,
    token: CancellationToken,
) -> Result<Outcome, Failure> {
    let admitted = tauri::async_runtime::spawn_blocking(move || {
        let path = preferences::settings_path(&app).map_err(|_| Failure::Storage)?;
        let enabled = preferences::read(&path)
            .map_err(|_| Failure::Storage)?
            .automatic_updates;
        if !manual && enabled != Some(true) {
            return Ok(None);
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| Failure::Storage)?
            .as_secs();
        history(&path.with_file_name("update-check-v1.json"), |state| {
            if token.is_cancelled() || (!manual && !releases::due(enabled, state.last_attempt, now))
            {
                return false;
            }
            state.last_attempt = Some(now);
            if manual {
                state.dismissed = None;
            }
            true
        })
    })
    .await
    .map_err(|_| Failure::Storage)??;
    let Some(state) = admitted else {
        return Ok(Outcome::Quiet);
    };
    let version = std::env::var("VSF_UPDATE_TEST_VERSION")
        .unwrap_or_else(|_| env!("CARGO_PKG_VERSION").into());
    let current = Version::parse(&version).map_err(|_| Failure::Invalid)?;
    let bytes = if let Some(path) = std::env::var_os("VSF_UPDATE_TEST_FIXTURE") {
        tauri::async_runtime::spawn_blocking(move || read_fixture(Path::new(&path)))
            .await
            .map_err(|_| Failure::Storage)??
    } else {
        let client = network::client(Duration::from_secs(10), true)?;
        let url = format!("{API}/repos/{REPOSITORY}/releases?per_page=20");
        log::debug!("Checking the public VisualSubForge release list");
        if std::env::var_os("VSF_UPDATE_DIAGNOSTICS").is_some() {
            eprintln!("VSF update check: requesting public release metadata");
        }
        network::read(client, &url).await?
    };
    match releases::newest(&bytes, &current)? {
        Some(version) if !manual && Some(version.to_string()) == state.dismissed => {
            Ok(Outcome::Quiet)
        }
        Some(version) => Ok(Outcome::Available {
            version: version.to_string(),
        }),
        None if manual => Ok(Outcome::Current),
        None => Ok(Outcome::Quiet),
    }
}

#[tauri::command]
#[specta::specta]
pub async fn update_check(app: tauri::AppHandle, manual: bool) -> Outcome {
    let service = app.state::<Updates>();
    let Some(token) = service.begin(manual) else {
        return Outcome::Quiet;
    };
    let result = tokio::select! {
        biased;
        () = token.cancelled() => return Outcome::Quiet,
        result = check(app.clone(), manual, token.clone()) => result,
    };
    let outcome = match result {
        Ok(outcome) => outcome,
        Err(reason) if manual => Outcome::Failed { reason },
        Err(reason) => {
            log::debug!("Automatic update check failed: {reason:?}");
            Outcome::Quiet
        }
    };
    service.finish(&token, outcome)
}

#[tauri::command]
#[specta::specta]
pub async fn update_consent(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        preferences::update(&preferences::settings_path(&app)?, |p| {
            p.automatic_updates = Some(enabled)
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

fn checked_version(value: &str) -> Result<Version, String> {
    if value.len() > 128 {
        return Err("Invalid release version".into());
    }
    Version::parse(value).map_err(|_| "Invalid release version".into())
}

#[tauri::command]
#[specta::specta]
pub async fn update_dismiss(app: tauri::AppHandle, version: String) -> Result<(), String> {
    let version = checked_version(&version)?.to_string();
    tauri::async_runtime::spawn_blocking(move || {
        let path = preferences::settings_path(&app)?.with_file_name("update-check-v1.json");
        history(&path, |h| {
            h.dismissed = Some(version);
            true
        })
        .map(|_| ())
        .map_err(|_| "Cannot save update preference".to_owned())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
#[specta::specta]
pub async fn update_open(version: String) -> Result<(), String> {
    let version = checked_version(&version)?;
    tauri::async_runtime::spawn_blocking(move || {
        open::that(format!(
            "https://github.com/{REPOSITORY}/releases/tag/v{version}"
        ))
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|_| "Could not open the release page".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixture_input_is_a_bounded_regular_file() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read_fixture(dir.path()), Err(Failure::Storage));
        let file = dir.path().join("releases.json");
        fs::write(&file, "[]").unwrap();
        assert_eq!(read_fixture(&file).unwrap(), b"[]");
        fs::write(&file, vec![0; releases::MAX_BYTES + 1]).unwrap();
        assert_eq!(read_fixture(&file), Err(Failure::TooLarge));
    }

    #[cfg(windows)]
    #[test]
    fn fixture_rejects_network_and_device_paths_before_opening() {
        for path in [r"\\server\share\releases.json", r"\\.\pipe\test"] {
            assert_eq!(read_fixture(Path::new(path)), Err(Failure::Storage));
        }
    }

    #[test]
    fn manual_check_supersedes_automatic_and_shutdown_cancels() {
        let service = Updates::default();
        let auto = service.begin(false).unwrap();
        let manual = service.begin(true).unwrap();
        assert!(auto.is_cancelled());
        assert!(service.begin(false).is_none());
        assert!(matches!(
            service.finish(&auto, Outcome::Current),
            Outcome::Quiet
        ));
        service.shutdown();
        assert!(manual.is_cancelled());
        assert!(service.begin(true).is_none());
    }

    #[test]
    fn concurrent_writers_admit_one_automatic_attempt() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("update-check-v1.json");
        let admitted = std::thread::scope(|scope| {
            let calls: Vec<_> = (0..8)
                .map(|_| {
                    scope.spawn(|| {
                        history(&path, |state| {
                            if !releases::due(Some(true), state.last_attempt, 123) {
                                return false;
                            }
                            state.last_attempt = Some(123);
                            true
                        })
                        .unwrap()
                        .is_some()
                    })
                })
                .collect();
            calls
                .into_iter()
                .map(|call| usize::from(call.join().unwrap()))
                .sum::<usize>()
        });
        assert_eq!(admitted, 1);
    }

    #[test]
    fn skips_preserve_history_and_failed_writes_surface() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("update-check-v1.json");
        assert!(history(&path, |_| false).unwrap().is_none());
        assert!(!path.exists());
        history(&path, |h| {
            h.last_attempt = Some(100);
            h.dismissed = Some("0.2.0".into());
            true
        })
        .unwrap();
        let saved = fs::read(&path).unwrap();
        assert!(
            history(&path, |h| releases::due(Some(true), h.last_attempt, 101))
                .unwrap()
                .is_none()
        );
        assert_eq!(fs::read(&path).unwrap(), saved);
        let invalid = dir.path().join("invalid-history.json");
        fs::create_dir(&invalid).unwrap();
        assert!(history(&invalid, |_| true).is_err());
        assert!(checked_version("https://example.org/").is_err());
    }
}
