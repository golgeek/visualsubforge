use super::*;
use crate::{quality_profile::Policy, scenes::Scenes};
use std::sync::mpsc;
pub(super) fn fixture() -> (tempfile::TempDir, Documents, Scenes) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("input.srt");
    fs::write(&path,"\u{feff}007\r\n00:00:00,000 --> 00:00:02,500\r\n{\\i1}A😀\r\n\r\n8\r\n00:00:02,000 --> 00:00:02,400\r\n0\r\n\r\n9\r\n00:00:04,000 --> 00:00:06,000\r\n<i>Other text</i>\r\n\r\n").unwrap();
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
    let scenes = Scenes::default();
    scenes.test_completed(7, vec![1., 4.], false);
    (dir, docs, scenes)
}
pub(super) fn start(docs: &Documents, scenes: &Scenes, selection: Selection) -> u64 {
    let snapshot = docs.apply(Action::Status, 0, 0).unwrap();
    docs.statistics_start(
        snapshot.generation,
        snapshot.revision,
        scenes.capture_quality(7, true).unwrap(),
        selection,
    )
    .unwrap()
}
pub(super) fn ready(docs: &Documents, id: u64) -> StatisticsView {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let v = docs.statistics_view(id, 0, 100).unwrap();
        if v.phase == Phase::Ready {
            return v;
        }
        assert_eq!(v.phase, Phase::Preparing, "{:?}", v.error);
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn value(docs: &Documents) -> serde_json::Value {
    serde_json::to_value(docs.apply(Action::Status, 0, 0).unwrap()).unwrap()
}
#[test]
fn read_only_report_matches_shared_scope_and_preserves_document_history_source_and_protections() {
    let (dir, docs, scenes) = fixture();
    let source = fs::read(dir.path().join("input.srt")).unwrap();
    docs.apply(
        Action::Edit {
            cue_id: 2,
            start_ms: 2000,
            end_ms: 2400,
            text: "Draft accepted previously".into(),
        },
        1,
        0,
    )
    .unwrap();
    docs.apply(Action::Undo, 1, 1).unwrap();
    {
        let mut store = docs.0.lock().unwrap();
        let loaded = store.loaded.as_mut().unwrap();
        loaded.recovered = true;
        loaded.resynced = true;
    }
    let before = value(&docs);
    let selection = Selection {
        range: Some(engine::Range { first: 1, last: 3 }),
        excluded: vec![3],
    };
    let id = start(&docs, &scenes, selection.clone());
    let view = ready(&docs, id);
    let expected = {
        let store = docs.0.lock().unwrap();
        engine::evaluate(
            store.loaded.as_ref().unwrap().editor.document(),
            &store.quality.context().profile,
            &scenes.capture_quality(7, true).unwrap().data,
            selection,
            || Ok(()),
        )
        .unwrap()
    };
    assert_eq!(view.summary.as_ref(), Some(&expected.summary));
    assert_eq!(view.findings, expected.findings);
    assert_eq!(value(&docs), before);
    assert_eq!(fs::read(dir.path().join("input.srt")).unwrap(), source);
    assert!(docs.0.lock().unwrap().loaded.as_ref().unwrap().resynced);
    let page = docs.statistics_view(id, 1, 1).unwrap();
    assert_eq!(page.findings.len(), 1);
    assert_eq!(page.findings[0], expected.findings[1]);
    assert!(docs.statistics_view(id, 0, 201).is_err());
    docs.statistics_cancel(id).unwrap();
    assert_eq!(
        docs.statistics_view(id, 0, 100).unwrap().phase,
        Phase::Cancelled
    );
    assert_eq!(value(&docs), before);
    let redo = docs.apply(Action::Redo, 1, 2).unwrap();
    assert_eq!(redo.cues[1].text, "Draft accepted previously");
}
#[test]
fn invalid_scope_empty_document_and_document_profile_scene_changes_are_guarded() {
    let (_dir, docs, scenes) = fixture();
    let before = value(&docs);
    assert!(
        docs.statistics_start(
            99,
            0,
            scenes.capture_quality(7, true).unwrap(),
            Selection::default()
        )
        .is_err()
    );
    assert!(
        docs.statistics_start(
            1,
            0,
            scenes.capture_quality(7, true).unwrap(),
            Selection {
                range: None,
                excluded: vec![4]
            }
        )
        .is_err()
    );
    assert_eq!(value(&docs), before);
    let id = start(&docs, &scenes, Selection::default());
    ready(&docs, id);
    docs.apply(
        Action::Edit {
            cue_id: 1,
            start_ms: 0,
            end_ms: 2500,
            text: "changed".into(),
        },
        1,
        0,
    )
    .unwrap();
    let stale = docs.statistics_view(id, 0, 100).unwrap();
    assert_eq!(stale.phase, Phase::Stale);
    assert!(stale.summary.is_none() && stale.findings.is_empty());
    docs.apply(Action::Undo, 1, 1).unwrap();
    assert_eq!(
        docs.statistics_view(id, 0, 100).unwrap().phase,
        Phase::Stale
    );
    let id = start(&docs, &scenes, Selection::default());
    ready(&docs, id);
    let mut profile = vsf_application::quality::Profile::default()
        .custom("stats-owner".into(), "Changed profile".into());
    profile.rules.duration.minimum_ms = 900;
    docs.initialize_quality(Policy::new(profile, Default::default()).unwrap())
        .unwrap();
    assert_eq!(
        docs.statistics_view(id, 0, 100).unwrap().phase,
        Phase::Stale
    );
    let id = start(&docs, &scenes, Selection::default());
    ready(&docs, id);
    let key = docs.statistics_context(id).unwrap();
    scenes.test_completed(7, vec![2.], false);
    assert!(
        scenes
            .with_quality_current(key, || docs.statistics_view(id, 0, 100))
            .is_err()
    );
    let empty = Documents::default();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("empty.srt");
    fs::write(&path, "").unwrap();
    empty
        .apply(
            Action::Open {
                path: path.to_string_lossy().into_owned(),
                discard: false,
            },
            0,
            0,
        )
        .unwrap();
    let id = start(&empty, &scenes, Selection::default());
    assert_eq!(ready(&empty, id).summary.unwrap().total_cues, 0);
}
#[test]
fn cancellation_rejects_queued_snapshots_and_late_ids_do_not_cancel_retry() {
    let (_dir, docs, scenes) = fixture();
    let (entered, rx) = mpsc::channel();
    let (release, wait) = mpsc::channel();
    let id = {
        let mut store = docs.0.lock().unwrap();
        let key = store.quality_key().unwrap();
        store
            .statistics
            .launch(
                key,
                scenes.capture_quality(7, true).unwrap().key,
                move |cancel| {
                    entered.send(()).unwrap();
                    wait.recv().unwrap();
                    assert!(cancel.load(Ordering::Relaxed));
                    Err("cancelled".into())
                },
            )
            .unwrap()
    };
    rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let before = value(&docs);
    assert!(
        docs.statistics_start(
            1,
            0,
            scenes.capture_quality(7, true).unwrap(),
            Selection::default()
        )
        .is_err()
    );
    docs.statistics_cancel(id).unwrap();
    assert_eq!(
        docs.statistics_view(id, 0, 100).unwrap().phase,
        Phase::Cancelling
    );
    assert_eq!(value(&docs), before);
    release.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while docs.statistics_view(id, 0, 100).unwrap().phase != Phase::Cancelled {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    let next = start(&docs, &scenes, Selection::default());
    docs.statistics_cancel(id).unwrap();
    assert!(next > id);
    ready(&docs, next);
    assert!(docs.statistics_view(id, 0, 100).is_err());
    assert_eq!(value(&docs), before);
}
#[test]
fn worker_failure_and_panic_allow_retry_shutdown_joins_outside_document_mutex() {
    let (_dir, docs, scenes) = fixture();
    for panic in [false, true] {
        let id = {
            let mut store = docs.0.lock().unwrap();
            let key = store.quality_key().unwrap();
            store
                .statistics
                .launch(
                    key,
                    scenes.capture_quality(7, true).unwrap().key,
                    move |_| {
                        assert!(!panic, "injected statistics panic");
                        Err("injected calculation failure".into())
                    },
                )
                .unwrap()
        };
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let v = docs.statistics_view(id, 0, 100).unwrap();
            if v.phase == Phase::Failed {
                assert!(v.summary.is_none() && v.error.is_some());
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        let next = start(&docs, &scenes, Selection::default());
        ready(&docs, next);
    }
    let owned = docs.clone();
    let (entered, rx) = mpsc::channel();
    {
        let mut store = docs.0.lock().unwrap();
        let key = store.quality_key().unwrap();
        store
            .statistics
            .launch(
                key,
                scenes.capture_quality(7, true).unwrap().key,
                move |cancel| {
                    entered.send(()).unwrap();
                    while !cancel.load(Ordering::Relaxed) {
                        std::thread::yield_now();
                    }
                    let _guard = owned.0.lock().unwrap();
                    Err("cancelled".into())
                },
            )
            .unwrap();
    }
    rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let closed = docs.clone();
    let (done, wait) = mpsc::channel();
    let thread = std::thread::spawn(move || {
        closed.shutdown_statistics().unwrap();
        done.send(()).unwrap();
    });
    wait.recv_timeout(Duration::from_secs(3)).unwrap();
    thread.join().unwrap();
    assert!(
        docs.statistics_start(
            1,
            0,
            scenes.capture_quality(7, true).unwrap(),
            Selection::default()
        )
        .is_err()
    );
}
