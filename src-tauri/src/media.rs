//! Session controller for the opt-in experiment. Native ownership stays on one worker.
use serde::{Deserialize, Serialize};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc,
};

#[cfg_attr(
    not(all(
        feature = "media-prototype",
        any(target_os = "macos", target_os = "windows", target_os = "linux")
    )),
    allow(dead_code)
)]
#[derive(Debug, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Action {
    Occlude {
        occluded: bool,
    },
    Layout {
        #[specta(type = crate::ipc_number::finite::FiniteF64)]
        x: f64,
        #[specta(type = crate::ipc_number::finite::FiniteF64)]
        y: f64,
        #[specta(type = crate::ipc_number::finite::FiniteF64)]
        width: f64,
        #[specta(type = crate::ipc_number::finite::FiniteF64)]
        height: f64,
        visible: bool,
    },
    Open {
        path: String,
        hardware: bool,
    },
    Close,
    Status,
    Pause {
        paused: bool,
    },
    Seek {
        #[specta(type = crate::ipc_number::finite::FiniteF64)]
        seconds: f64,
    },
    Step {
        backwards: bool,
    },
    Track {
        id: u32,
    },
    Loop {
        #[specta(type = crate::ipc_number::finite::FiniteF64)]
        start: f64,
        #[specta(type = crate::ipc_number::finite::FiniteF64)]
        end: f64,
    },
    ClearLoop,
    Decode {
        hardware: bool,
    },
}
#[derive(Clone, Default, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    #[serde(with = "crate::ipc_number")]
    #[specta(type = crate::ipc_number::SafeU64)]
    pub session: u64,
    pub available: bool,
    pub loaded: bool,
    pub paused: bool,
    pub seeking: bool,
    pub queued_audio_clock: bool,
    pub position_seconds: Option<f64>,
    pub duration_seconds: Option<f64>,
    pub audio_track: Option<f64>,
    pub audio_tracks: Vec<AudioTrack>,
    pub av_sync_seconds: Option<f64>,
    #[serde(with = "crate::ipc_number")]
    #[specta(type = crate::ipc_number::SafeU64)]
    pub rendered_frames: u64,
    pub swap_counter_available: bool,
    pub video_output_configured: bool,
    pub surface_above_webview: bool,
    pub hardware_decoder: Option<String>,
    pub decoder_drops: Option<f64>,
    pub output_drops: Option<f64>,
    pub error: Option<String>,
    pub subtitle_preview_error: Option<String>,
    #[serde(default)]
    pub subtitle_font_diagnostics: Vec<String>,
    #[serde(default, with = "crate::ipc_number::optional")]
    #[specta(type = Option<crate::ipc_number::SafeU64>)]
    pub preview_document_generation: Option<u64>,
    #[serde(default, with = "crate::ipc_number::optional")]
    #[specta(type = Option<crate::ipc_number::SafeU64>)]
    pub preview_document_revision: Option<u64>,
}
#[derive(Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AudioTrack {
    id: u32,
    title: String,
    audio_ordinal: u32,
}
#[cfg(all(
    feature = "media-prototype",
    any(target_os = "macos", target_os = "windows", target_os = "linux")
))]
fn audio_tracks(player: &mut media_player::Player) -> Vec<AudioTrack> {
    let count = player.number(c"track-list/count").unwrap_or(0.0).min(256.0) as usize;
    let mut tracks = Vec::new();
    for index in 0..count {
        let property = |key: &str| std::ffi::CString::new(format!("track-list/{index}/{key}")).ok();
        let text = |player: &mut media_player::Player, key: &str| {
            property(key).and_then(|name| player.text(&name))
        };
        let number = |player: &mut media_player::Player, key: &str| {
            property(key).and_then(|name| player.number(&name))
        };
        if text(player, "type").as_deref() != Some("audio") {
            continue;
        }
        if let (Some(id), Some(stream)) = (number(player, "id"), number(player, "ff-index")) {
            let label = text(player, "title")
                .or_else(|| text(player, "lang"))
                .unwrap_or_else(|| format!("Audio {}", id as u32));
            tracks.push((stream as u32, id as u32, label));
        }
    }
    tracks.sort_by_key(|t| t.0);
    tracks
        .into_iter()
        .enumerate()
        .map(|(ordinal, (_, id, title))| AudioTrack {
            id,
            title,
            audio_ordinal: ordinal as u32,
        })
        .collect()
}
fn validate(action: &Action) -> Result<(), String> {
    match action {
        Action::Seek { seconds } if !seconds.is_finite() || *seconds < 0.0 => {
            Err("Seek time must be finite and nonnegative".into())
        }
        Action::Loop { start, end }
            if !start.is_finite() || !end.is_finite() || *start < 0.0 || end <= start =>
        {
            Err("Loop requires 0 ≤ start < end".into())
        }
        Action::Track { id: 0 } => Err("Audio track IDs start at 1".into()),
        _ => Ok(()),
    }
}
#[cfg_attr(
    not(all(
        feature = "media-prototype",
        any(target_os = "macos", target_os = "windows", target_os = "linux")
    )),
    allow(dead_code)
)]
struct Request {
    action: Action,
    session: u64,
    reply: mpsc::Sender<Result<Status, String>>,
}
pub struct Media {
    sender: Option<mpsc::SyncSender<Request>>,
    latest: Arc<AtomicU64>,
    // Serializes file-session replacement with resync capture/acceptance.
    // Never held by the native worker or while waiting for a native reply.
    selection: std::sync::Mutex<Option<std::path::PathBuf>>,
    stopping: Arc<AtomicBool>,
    shutdown_complete: AtomicBool,
    worker: std::sync::Mutex<Option<std::thread::JoinHandle<()>>>,
}
impl Default for Media {
    fn default() -> Self {
        Self {
            sender: None,
            latest: Arc::new(AtomicU64::new(0)),
            selection: Default::default(),
            stopping: Arc::new(AtomicBool::new(false)),
            shutdown_complete: AtomicBool::new(false),
            worker: Default::default(),
        }
    }
}
impl Media {
    // A selected-file identity for document/resync unit tests. No native player,
    // command transport, decoder or playback validation is implied.
    #[cfg(test)]
    pub(crate) fn test_selection(path: &std::path::Path) -> Self {
        Self {
            latest: Arc::new(AtomicU64::new(1)),
            selection: std::sync::Mutex::new(Some(path.into())),
            ..Default::default()
        }
    }
    pub fn current_session(&self) -> u64 {
        self.latest.load(Ordering::Relaxed)
    }
    pub fn available(&self) -> bool {
        self.sender.is_some()
    }
    pub fn request(&self, action: Action, session: u64) -> Result<Status, String> {
        self.request_with_replacement(action, session, || Ok(()))
    }
    fn request_with_replacement(
        &self,
        action: Action,
        session: u64,
        replace: impl FnOnce() -> Result<(), String>,
    ) -> Result<Status, String> {
        validate(&action)?;
        let Some(sender) = &self.sender else {
            return if matches!(action, Action::Status) {
                Ok(Status::default())
            } else {
                Err(
                    "Native playback requires a supported build with --features media-prototype"
                        .into(),
                )
            };
        };
        if self.stopping.load(Ordering::Relaxed) {
            return Err("Player is shutting down".into());
        }
        let session = if matches!(action, Action::Open { .. } | Action::Close) {
            let mut selection = self
                .selection
                .lock()
                .map_err(|_| "Media selection unavailable")?;
            let next = self
                .latest
                .load(Ordering::Relaxed)
                .checked_add(1)
                .filter(|id| *id <= crate::ipc_number::MAX_SAFE_INTEGER)
                .ok_or("Media session identities exhausted")?;
            // Media selection -> document is the same order used by project
            // attachment and proposal acceptance. Reset before publishing the
            // new session, so a late Open reply cannot erase an attached policy.
            replace()?;
            *selection = match &action {
                Action::Open { path, .. } => Some(path.into()),
                _ => None,
            };
            self.latest.store(next, Ordering::Relaxed);
            next
        } else {
            session
        };
        let (reply, response) = mpsc::channel();
        sender
            .send(Request {
                action,
                session,
                reply,
            })
            .map_err(|_| "Player command queue is closed")?;
        response
            .recv()
            .map_err(|_| String::from("Player worker stopped"))?
    }
    pub(crate) fn with_current<T>(
        &self,
        session: u64,
        operation: impl FnOnce(&std::path::Path) -> Result<T, String>,
    ) -> Result<T, String> {
        self.with_context(session, |path| {
            operation(path.ok_or("Open the target video first")?)
        })
    }
    /// Serialize operations that also support an intentionally media-free workspace.
    pub(crate) fn with_context<T>(
        &self,
        session: u64,
        operation: impl FnOnce(Option<&std::path::Path>) -> Result<T, String>,
    ) -> Result<T, String> {
        let selection = self
            .selection
            .lock()
            .map_err(|_| "Media selection unavailable")?;
        if self.stopping.load(Ordering::Relaxed) || self.current_session() != session {
            return Err("Media changed or is shutting down".into());
        }
        operation(selection.as_deref())
    }

    pub fn shutdown_complete(&self) -> bool {
        self.shutdown_complete.load(Ordering::Acquire)
    }
    pub fn begin_shutdown(&self) -> bool {
        !self.stopping.swap(true, Ordering::AcqRel)
    }
    pub fn shutdown(&self) {
        self.stopping.store(true, Ordering::Relaxed);
        if let Ok(mut worker) = self.worker.lock()
            && let Some(worker) = worker.take()
        {
            let _ = worker.join();
        }
        self.shutdown_complete.store(true, Ordering::Release);
    }
}
#[cfg(all(
    feature = "media-prototype",
    any(target_os = "macos", target_os = "windows", target_os = "linux")
))]
impl Media {
    // Diagnostic coordination only: reserve-before-send is the cancellation boundary.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    pub(super) fn latest_generation(&self) -> u64 {
        self.latest.load(Ordering::Relaxed)
    }
    pub fn start(
        surface: media_player::Surface,
        documents: crate::document::Documents,
    ) -> Result<Self, String> {
        let (sender, receiver) = mpsc::sync_channel::<Request>(16);
        let latest = Arc::new(AtomicU64::new(0));
        let stopping = Arc::new(AtomicBool::new(false));
        let newest = latest.clone();
        let stop = stopping.clone();
        let worker = std::thread::Builder::new()
            .name("media-control".into())
            .spawn(move || {
                let mut player: Option<media_player::Player> = None;
                // Engine lifetime and file-session lifetime are independent.
                let mut active_session = false;
                let mut preview = crate::subtitle_preview::Renderer::default();
                let mut frame_origin = 0;
                let mut status = Status {
                    available: true,
                    swap_counter_available: cfg!(target_os = "macos"),
                    surface_above_webview: cfg!(any(
                        target_os = "windows",
                        target_os = "linux",
                        all(target_os = "macos", feature = "macos-angle")
                    )),
                    paused: true,
                    ..Default::default()
                };
                while !stop.load(Ordering::Relaxed) {
                    preview.reset(status.session);
                    if active_session
                        && status.loaded
                        && status.session == newest.load(Ordering::Relaxed)
                        && let Some(player) = &mut player
                    {
                        preview.reconcile(player, &documents);
                        status.subtitle_preview_error = preview.error.clone();
                        status.subtitle_font_diagnostics = player.font_diagnostics();
                        let revision = preview.revision().filter(|_| preview.error.is_none());
                        status.preview_document_generation = revision.map(|r| r.0);
                        status.preview_document_revision = revision.map(|r| r.1);
                    }
                    let request = match receiver.recv_timeout(std::time::Duration::from_millis(20))
                    {
                        Ok(request) => request,
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            if active_session
                                && let Some(player) = &mut player
                                && let Some(error) = player.event_error()
                            {
                                status.error = Some(error);
                                status.loaded = false;
                            }
                            continue;
                        }
                        Err(_) => break,
                    };
                    if std::env::var_os("VSF_MEDIA_SMOKE").is_some()
                        && !matches!(request.action, Action::Status)
                    {
                        eprintln!(
                            "VSF action: {:?}, session={}",
                            request.action, request.session
                        );
                    }
                    let result = (|| {
                        if !matches!(
                            request.action,
                            Action::Status | Action::Layout { .. } | Action::Occlude { .. }
                        ) && request.session != newest.load(Ordering::Relaxed)
                        {
                            return Err("Superseded media session".into());
                        }
                        match &request.action {
                            Action::Occlude { occluded } => surface.occlude(*occluded)?,
                            Action::Open { path, hardware } => {
                                active_session = false;
                                status = Status {
                                    session: request.session,
                                    available: true,
                                    swap_counter_available: cfg!(target_os = "macos"),
                                    surface_above_webview: cfg!(any(
                                        target_os = "windows",
                                        target_os = "linux",
                                        all(target_os = "macos", feature = "macos-angle")
                                    )),
                                    paused: true,
                                    ..Default::default()
                                };
                                if let Some(engine) = &mut player
                                    && let Err(error) = engine.unload()
                                {
                                    player = None; // failed engines are never reused
                                    return Err(error);
                                }
                                let path = std::path::Path::new(path)
                                    .canonicalize()
                                    .map_err(|e| format!("Cannot open media: {e}"))?;
                                if !path.is_file() {
                                    return Err("Media must be a local file".into());
                                }
                                let path = path.to_str().ok_or("Media path is not valid UTF-8")?;
                                if player.is_none() {
                                    player = Some(media_player::Player::new(&surface)?);
                                }
                                status.queued_audio_clock = player.as_mut().is_some_and(|engine| {
                                    engine.text(c"vsf-audio-clock-version").as_deref() == Some("1")
                                });
                                let next = player.as_mut().ok_or("Player initialization failed")?;
                                // Provisional Mac preview font: the default family on this
                                // development build has no working Japanese fallback.
                                #[cfg(target_os = "macos")]
                                next.command(&["set", "sub-font", "Hiragino Sans"])?;
                                frame_origin = next.frames();
                                // Choose before load: switching after the first frame retains
                                // software decoder allocations even in a hardware session.
                                next.command(&[
                                    "set",
                                    "hwdec",
                                    if *hardware { "auto-copy" } else { "no" },
                                ])?;
                                next.command(&["loadfile", path, "replace"])?;
                                if request.session != newest.load(Ordering::Relaxed) {
                                    return Err("Superseded media session".into());
                                }
                                next.presenting(true);
                                active_session = true;
                            }
                            Action::Close => {
                                active_session = false;
                                status = Status {
                                    session: request.session,
                                    available: true,
                                    swap_counter_available: cfg!(target_os = "macos"),
                                    surface_above_webview: cfg!(any(
                                        target_os = "windows",
                                        target_os = "linux",
                                        all(target_os = "macos", feature = "macos-angle")
                                    )),
                                    paused: true,
                                    ..Default::default()
                                };
                                if let Some(engine) = &mut player
                                    && let Err(error) = engine.unload()
                                {
                                    player = None;
                                    return Err(error);
                                }
                            }
                            Action::Layout {
                                x,
                                y,
                                width,
                                height,
                                visible,
                            } => {
                                surface.layout(*x, *y, *width, *height, *visible)?;
                            }
                            Action::Status => {}
                            action => {
                                if request.session != status.session {
                                    return Err("Stale media session".into());
                                }
                                if !active_session {
                                    return Err("No media is open".into());
                                }
                                let active = player.as_mut().ok_or("No media is open")?;
                                match action {
                                    Action::Pause { paused } => active.command(&[
                                        "set",
                                        "pause",
                                        if *paused { "yes" } else { "no" },
                                    ])?,
                                    Action::Seek { seconds } => active.command(&[
                                        "seek",
                                        &seconds.to_string(),
                                        "absolute+exact",
                                    ])?,
                                    Action::Step { backwards } => {
                                        active.command(&[if *backwards {
                                            "frame-back-step"
                                        } else {
                                            "frame-step"
                                        }])?
                                    }
                                    Action::Track { id } => {
                                        active.command(&["set", "aid", &id.to_string()])?
                                    }
                                    Action::Loop { start, end } => {
                                        active.command(&[
                                            "set",
                                            "ab-loop-a",
                                            &start.to_string(),
                                        ])?;
                                        if let Err(error) =
                                            active.command(&["set", "ab-loop-b", &end.to_string()])
                                        {
                                            let _ = active.command(&["set", "ab-loop-a", "no"]);
                                            return Err(error);
                                        }
                                    }
                                    Action::Decode { hardware } => active.command(&[
                                        "set",
                                        "hwdec",
                                        if *hardware { "auto-copy" } else { "no" },
                                    ])?,
                                    Action::ClearLoop => {
                                        crate::media_transport::clear_loop(
                                            active,
                                            status.queued_audio_clock,
                                        )?;
                                    }
                                    _ => unreachable!(),
                                }
                            }
                        }
                        if active_session && let Some(active) = &mut player {
                            if let Some(error) = active.event_error() {
                                status.error = Some(error);
                            }
                            status.position_seconds =
                                crate::media_transport::position(active, status.queued_audio_clock);
                            status.duration_seconds = active.number(c"duration");
                            status.audio_track = active.number(c"aid");
                            status.av_sync_seconds = active.signed_number(c"avsync");
                            // Headers/duration and renderer updates can precede
                            // FILE_LOADED and track selection on replacement.
                            status.loaded = active.file_loaded()
                                && status.duration_seconds.is_some()
                                && status.error.is_none();
                            if status.loaded && status.audio_tracks.is_empty() {
                                status.audio_tracks = audio_tracks(active);
                            }
                            status.paused = active.paused();
                            status.seeking = active.seeking();
                            status.rendered_frames = active.frames().saturating_sub(frame_origin);
                            status.video_output_configured = active.flag(c"vo-configured");
                            status.hardware_decoder = active.text(c"hwdec-current");
                            status.decoder_drops = active.number(c"decoder-frame-drop-count");
                            status.output_drops = active.number(c"frame-drop-count");
                        }
                        Ok(status.clone())
                    })();
                    if matches!(request.action, Action::Open { .. })
                        && request.session == status.session
                        && let Err(error) = &result
                    {
                        active_session = false;
                        // A superseded or partially failed load must not keep playing.
                        if let Some(engine) = &mut player
                            && engine.unload().is_err()
                        {
                            player = None;
                        }
                        status.error = Some(error.clone());
                    }
                    let _ = request.reply.send(result);
                }
                drop(player); // renderer joins before main-thread drawable cleanup
                drop(surface);
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            sender: Some(sender),
            latest,
            selection: Default::default(),
            stopping,
            shutdown_complete: AtomicBool::new(false),
            worker: std::sync::Mutex::new(Some(worker)),
        })
    }
}

#[tauri::command]
#[specta::specta]
pub async fn media_action(
    app: tauri::AppHandle,
    action: Action,
    session: crate::ipc_number::SafeU64,
) -> Result<Status, String> {
    use tauri::Manager;
    let session = session.get();
    tauri::async_runtime::spawn_blocking(move || {
        if matches!(action, Action::Open { .. } | Action::Close) {
            app.state::<crate::resync::Resync>().cancel_current();
        }
        if matches!(
            action,
            Action::Open { .. } | Action::Close | Action::Track { .. }
        ) {
            app.state::<crate::waveform::Waveforms>().cancel();
        }
        if matches!(action, Action::Open { .. } | Action::Close) {
            app.state::<crate::scenes::Scenes>().cancel();
        }
        app.state::<Media>()
            .request_with_replacement(action, session, || {
                app.state::<crate::document::Documents>().reset_quality()
            })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shutdown_is_started_once_and_final_exit_is_permitted() {
        let media = Media::default();
        assert!(!media.shutdown_complete());
        assert!(media.begin_shutdown());
        assert!(!media.begin_shutdown());
        assert!(!media.shutdown_complete());
        media.shutdown();
        assert!(media.shutdown_complete());
        assert!(!media.begin_shutdown());
    }
    #[test]
    fn selection_guard_serializes_replacement_and_rejects_old_or_closed_sessions() {
        use std::time::Duration;
        let wait = Duration::from_secs(5);
        let (sender, receiver) = mpsc::sync_channel::<Request>(2);
        let media = Media {
            sender: Some(sender),
            ..Media::test_selection(std::path::Path::new("first.mkv"))
        };
        // Controlled command acknowledgments; this tests selection ownership,
        // not native loading, rendering or audio.
        let replies = std::thread::spawn(move || {
            for _ in 0..2 {
                let request = receiver.recv_timeout(wait).unwrap();
                request
                    .reply
                    .send(Ok(Status {
                        session: request.session,
                        ..Default::default()
                    }))
                    .unwrap();
            }
        });
        std::thread::scope(|scope| {
            let (locked, locked_rx) = mpsc::channel();
            let (release, release_rx) = mpsc::channel();
            let selected = &media;
            let guard = scope.spawn(move || {
                selected.with_current(1, |path| {
                    assert_eq!(path, std::path::Path::new("first.mkv"));
                    locked.send(()).unwrap();
                    release_rx.recv_timeout(wait).unwrap();
                    Ok(())
                })
            });
            locked_rx.recv_timeout(wait).unwrap();
            let replacement = scope.spawn(|| {
                media.request(
                    Action::Open {
                        path: "second.mkv".into(),
                        hardware: false,
                    },
                    1,
                )
            });
            assert_eq!(media.current_session(), 1);
            assert!(!replacement.is_finished());
            release.send(()).unwrap();
            guard.join().unwrap().unwrap();
            assert_eq!(replacement.join().unwrap().unwrap().session, 2);
        });
        assert!(media.with_current(1, |_| Ok(())).is_err());
        media
            .with_current(2, |path| {
                assert_eq!(path, std::path::Path::new("second.mkv"));
                Ok(())
            })
            .unwrap();
        assert_eq!(media.request(Action::Close, 2).unwrap().session, 3);
        assert!(media.with_current(3, |_| Ok(())).is_err());
        replies.join().unwrap();
    }

    #[test]
    fn reject_invalid_times_and_tracks() {
        assert!(validate(&Action::Seek { seconds: f64::NAN }).is_err());
        assert!(validate(&Action::Seek { seconds: -1.0 }).is_err());
        assert!(
            validate(&Action::Loop {
                start: 2.0,
                end: 1.0
            })
            .is_err()
        );
        assert!(
            validate(&Action::Loop {
                start: 0.0,
                end: f64::INFINITY
            })
            .is_err()
        );
        assert!(validate(&Action::Track { id: 0 }).is_err());
        assert!(
            validate(&Action::Loop {
                start: 0.0,
                end: 1.0
            })
            .is_ok()
        );
    }
}
