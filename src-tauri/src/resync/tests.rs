use super::*;
use crate::document::Action;
use std::{fs, path::Path, sync::mpsc, time::Instant};

const WAIT: Duration = Duration::from_secs(5);
fn documents(path: &Path) -> Documents {
    fs::write(path, "1\n00:00:03,000 --> 00:00:04,000\nOriginal\n").unwrap();
    let docs = Documents::default();
    docs.apply(
        Action::Open {
            path: path.to_string_lossy().into_owned(),
            discard: false,
        },
        0,
        0,
    )
    .unwrap();
    docs
}
fn finish(owner: &Resync) {
    let deadline = Instant::now() + WAIT;
    loop {
        let mut state = owner.owner.lock().unwrap();
        let job = state.job.as_mut().unwrap();
        job.refresh();
        if job.worker.is_none() {
            return;
        }
        drop(state);
        assert!(Instant::now() < deadline, "Worker did not finish");
        std::thread::yield_now();
    }
}

#[test]
fn cancellation_keeps_admission_closed_until_cleanup_then_allows_retry() {
    let dir = tempfile::tempdir().unwrap();
    let docs = documents(&dir.path().join("source.srt"));
    let capture = docs.capture_resync(1, 0).unwrap();
    let owner = Resync::default();
    let (started, started_rx) = mpsc::channel();
    let (release, release_rx) = mpsc::channel();
    let id = owner
        .launch(1, capture.clone(), move |token, _| {
            started.send(token).unwrap();
            release_rx.recv_timeout(WAIT).unwrap();
            Err("Late failure after cancellation".into())
        })
        .unwrap();
    let token = started_rx.recv_timeout(WAIT).unwrap();
    assert!(
        owner
            .launch(1, capture.clone(), |_, _| panic!("No queued job"))
            .is_err()
    );
    assert!(owner.admit_scene(|| Ok(())).is_err());
    assert!(owner.cancel(id + 1).is_err());
    assert!(!token.is_cancelled());
    owner.cancel(id).unwrap();
    assert!(token.is_cancelled());
    assert!(
        owner
            .launch(1, capture.clone(), |_, _| panic!("Cleanup still running"))
            .is_err()
    );
    release.send(()).unwrap();
    finish(&owner);
    assert!(
        owner
            .owner
            .lock()
            .unwrap()
            .job
            .as_ref()
            .unwrap()
            .error
            .is_none()
    );
    assert!(owner.admit_scene(|| Ok(())).is_ok());
    let next = owner
        .launch(1, capture, |_, _| Err("New job failed".into()))
        .unwrap();
    assert!(next > id && owner.cancel(id).is_err());
    finish(&owner);
    assert_eq!(
        owner
            .owner
            .lock()
            .unwrap()
            .job
            .as_ref()
            .unwrap()
            .error
            .as_deref(),
        Some("New job failed")
    );
    owner.shutdown();
}

#[test]
fn worker_panic_is_reported_and_shutdown_waits_for_acceptance_and_worker_cleanup() {
    let dir = tempfile::tempdir().unwrap();
    let docs = documents(&dir.path().join("source.srt"));
    let capture = docs.capture_resync(1, 0).unwrap();
    let owner = Arc::new(Resync::default());
    owner
        .launch(1, capture.clone(), |_, _| panic!("Controlled failure"))
        .unwrap();
    finish(&owner);
    assert!(
        owner
            .owner
            .lock()
            .unwrap()
            .job
            .as_ref()
            .unwrap()
            .error
            .as_ref()
            .unwrap()
            .contains("unexpectedly")
    );
    let (started, started_rx) = mpsc::channel();
    let (release, release_rx) = mpsc::channel();
    owner
        .launch(1, capture.clone(), move |token, _| {
            started.send(token).unwrap();
            release_rx.recv_timeout(WAIT).unwrap();
            Err("Cancelled".into())
        })
        .unwrap();
    let token = started_rx.recv_timeout(WAIT).unwrap();
    // Hold the same guard used while acceptance verifies input identities.
    owner.owner.lock().unwrap().accepting = true;
    let accepting = Accepting(&owner);
    let shutdown_owner = owner.clone();
    let (done, done_rx) = mpsc::channel();
    let shutdown = std::thread::spawn(move || {
        shutdown_owner.shutdown();
        done.send(()).unwrap();
    });
    let deadline = Instant::now() + WAIT;
    while !owner.owner.lock().unwrap().closing {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert!(token.is_cancelled());
    assert!(matches!(done_rx.try_recv(), Err(mpsc::TryRecvError::Empty)));
    release.send(()).unwrap();
    assert!(matches!(done_rx.try_recv(), Err(mpsc::TryRecvError::Empty)));
    drop(accepting);
    done_rx.recv_timeout(WAIT).unwrap();
    shutdown.join().unwrap();
    assert!(
        owner
            .owner
            .lock()
            .unwrap()
            .job
            .as_ref()
            .unwrap()
            .worker
            .is_none()
    );
    assert!(
        owner
            .launch(1, capture, |_, _| panic!("Closed admission"))
            .is_err()
    );
    assert!(owner.admit_scene(|| Ok(())).is_err());
}

#[test]
fn polling_cancels_stale_documents_and_media_without_publishing_late_results() {
    for replace_media in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let docs = documents(&dir.path().join("source.srt"));
        let media = Media::test_selection(&dir.path().join("target.mkv"));
        let owner = Resync::default();
        let (started, started_rx) = mpsc::channel();
        let (release, release_rx) = mpsc::channel();
        let id = owner
            .launch(
                if replace_media { 2 } else { 1 },
                docs.capture_resync(1, 0).unwrap(),
                move |token, _| {
                    started.send(token).unwrap();
                    release_rx.recv_timeout(WAIT).unwrap();
                    Err("Late failure".into())
                },
            )
            .unwrap();
        let token = started_rx.recv_timeout(WAIT).unwrap();
        if !replace_media {
            docs.apply(
                Action::Edit {
                    cue_id: 1,
                    start_ms: 3000,
                    end_ms: 4000,
                    text: "Edited".into(),
                },
                1,
                0,
            )
            .unwrap();
        }
        let view = serde_json::to_value(owner.view(id, 0, 0, &media, &docs).unwrap()).unwrap();
        assert_eq!(view["phase"], "cancelling");
        assert!(token.is_cancelled());
        release.send(()).unwrap();
        finish(&owner);
        let view = serde_json::to_value(owner.view(id, 0, 0, &media, &docs).unwrap()).unwrap();
        assert_eq!(view["phase"], "failed");
        assert!(view["error"].as_str().unwrap().contains("changed"));
        assert!(owner.accept(id, &media, &docs).is_err());
        owner.shutdown();
    }
}

#[test]
#[ignore = "Requires python3 scripts/resync-fixtures.py and FFmpeg/ffprobe; no native playback"]
fn actual_analysis_reviews_bounded_pages_accepts_once_and_rechecks_all_identities() {
    let fixtures =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/generated/resync");
    let dir = tempfile::tempdir().unwrap();
    let subtitle_path = dir.path().join("source.srt");
    let target = dir.path().join("target.mkv");
    fs::copy(fixtures.join("offset.mkv"), &target).unwrap();
    let text: String = (1..=205)
        .map(|id| format!("{id}\n00:00:03,000 --> 00:00:04,000\nCue {id}\n\n"))
        .collect();
    fs::write(&subtitle_path, &text).unwrap();
    let open = || {
        let docs = Documents::default();
        docs.apply(
            Action::Open {
                path: subtitle_path.to_string_lossy().into_owned(),
                discard: false,
            },
            0,
            0,
        )
        .unwrap();
        docs
    };
    let docs = open();
    let media = Media::test_selection(&target);
    let owner = Resync::default();
    let capture = docs.capture_resync(1, 0).unwrap();
    let request = application::Request {
        source_video: fixtures.join("source.mkv"),
        target_video: target.clone(),
        subtitles: application::Subtitles::Snapshot(capture.snapshot.clone()),
    };
    let id = owner
        .launch(1, capture, move |cancel, stage| {
            application::analyze(
                &media_analysis::Tools::discover().unwrap(),
                request,
                &cancel,
                Duration::from_secs(90),
                |next| *stage.lock().unwrap() = next,
            )
            .map_err(|error| error.to_string())
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(95);
    loop {
        let view = serde_json::to_value(owner.view(id, 0, 0, &media, &docs).unwrap()).unwrap();
        if view["phase"] != "analyzing" {
            assert_eq!(view["phase"], "reviewing", "{view}");
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(20));
    }
    for (offset, count) in [(0, 100), (100, 100), (200, 5), (u32::MAX, 0)] {
        let view = serde_json::to_value(owner.view(id, offset, 0, &media, &docs).unwrap()).unwrap();
        assert_eq!(view["totalCues"], 205);
        assert_eq!(view["complete"], true);
        assert_eq!(view["cues"].as_array().unwrap().len(), count);
        if count > 0 {
            assert_eq!(view["cues"][0]["sourceStartMs"], 3000);
            assert_eq!(view["cues"][0]["startMs"], 6000);
        }
    }
    let analysis = owner
        .owner
        .lock()
        .unwrap()
        .job
        .as_ref()
        .unwrap()
        .result
        .clone()
        .unwrap();
    let accepted = owner.accept(id, &media, &docs).unwrap();
    let accepted = serde_json::to_value(accepted).unwrap();
    assert_eq!(accepted["revision"], 1);
    assert_eq!(accepted["requiresSaveAs"], true);
    assert!(
        accepted["cues"]
            .as_array()
            .unwrap()
            .iter()
            .all(|cue| cue["startMs"] == 6000)
    );
    assert!(owner.accept(id, &media, &docs).is_err());
    assert_eq!(fs::read_to_string(&subtitle_path).unwrap(), text);

    // Reuse the measured proposal with fresh captures. Each guard must reject
    // without relying on frontend polling to invalidate the job first.
    for failure in ["document", "media", "cancel", "job", "input"] {
        let docs = open();
        let owner = Resync::default();
        let capture = docs.capture_resync(1, 0).unwrap();
        owner.owner.lock().unwrap().job = Some(Job {
            id: 1,
            session: if failure == "media" { 2 } else { 1 },
            capture,
            cancel: Cancellation::default(),
            stage: Arc::new(Mutex::new(Stage::Propose)),
            worker: None,
            result: Some(analysis.clone()),
            error: None,
            accepted: false,
        });
        match failure {
            "document" => {
                docs.apply(
                    Action::Edit {
                        cue_id: 1,
                        start_ms: 3000,
                        end_ms: 4000,
                        text: "Edited".into(),
                    },
                    1,
                    0,
                )
                .unwrap();
            }
            "cancel" => owner.cancel(1).unwrap(),
            "input" => fs::write(&target, "Replaced video").unwrap(),
            _ => {}
        }
        let before = serde_json::to_value(docs.apply(Action::Status, 0, 0).unwrap()).unwrap();
        assert!(
            owner
                .accept(if failure == "job" { 2 } else { 1 }, &media, &docs)
                .is_err(),
            "{failure}"
        );
        assert_eq!(
            serde_json::to_value(docs.apply(Action::Status, 0, 0).unwrap()).unwrap(),
            before,
            "{failure}"
        );
        assert!(!owner.owner.lock().unwrap().accepting);
        owner.shutdown();
    }
}
