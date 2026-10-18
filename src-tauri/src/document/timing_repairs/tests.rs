use super::*;
use crate::{quality_profile::Policy, scenes::Scenes};
use std::sync::mpsc;

pub(super) fn fixture() -> (tempfile::TempDir, Documents, Scenes) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("source.srt");
    fs::write(&path, "\u{feff}1\r\n00:00:01,000 --> 00:00:03,000\r\n{\\i1}First é 😀\r\n\r\n2\r\n00:00:04,050 --> 00:00:06,050\r\nSecond\r\n\r\n3\r\n00:00:08,000 --> 00:00:10,000\r\n{\\k20}Timed\r\n\r\n").unwrap();
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
    let mut profile =
        Profile::default().custom("scene-owner-test".into(), "Scene owner test".into());
    profile.rules.speed.enabled = false;
    docs.initialize_quality(Policy::new(profile, Default::default()).unwrap())
        .unwrap();
    let scenes = Scenes::default();
    scenes.test_completed(7, vec![1., 4., 6., 8.], false);
    (directory, docs, scenes)
}
pub(super) fn start(docs: &Documents, scenes: &Scenes) -> u64 {
    let doc = docs.apply(Action::Status, 0, 0).unwrap();
    docs.scene_repair_start(
        doc.generation,
        doc.revision,
        scenes.capture_quality(7, true).unwrap(),
    )
    .unwrap()
}
pub(super) fn ready(docs: &Documents, id: u64) -> SceneRepairView {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let view = docs.scene_repair_view(id, 0, 100).unwrap();
        if view.phase == Phase::Reviewing {
            return view;
        }
        assert_eq!(view.phase, Phase::Preparing, "{:?}", view.error);
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn value(docs: &Documents) -> serde_json::Value {
    serde_json::to_value(docs.apply(Action::Status, 0, 0).unwrap()).unwrap()
}
#[test]
fn bounded_preview_acceptance_history_and_exact_save_preserve_all_source_data() {
    let (dir, docs, scenes) = fixture();
    let source = fs::read(dir.path().join("source.srt")).unwrap();
    let before = value(&docs);
    let id = start(&docs, &scenes);
    let view = ready(&docs, id);
    assert_eq!((view.total, view.changed, view.skipped), (3, 2, 1));
    assert_eq!(view.coverage, Some(engine::scene::SceneCoverage::Checked));
    assert_eq!((view.before_findings, view.after_findings), (4, 1));
    assert_eq!(value(&docs), before);
    let page = docs.scene_repair_view(id, 1, 1).unwrap();
    assert_eq!(page.rows.len(), 1);
    assert_eq!(page.rows[0].cue_index, 1);
    assert!(docs.scene_repair_view(id, 0, 201).is_err());
    let accepted = docs.scene_repair_accept(id).unwrap();
    assert_eq!(accepted.revision, 1);
    assert_eq!(
        accepted
            .cues
            .iter()
            .map(|c| (c.start_ms, c.end_ms))
            .collect::<Vec<_>>(),
        [(1130, 3000), (4130, 5870), (8000, 10000)]
    );
    for (i, cue) in accepted.cues.iter().enumerate() {
        assert_eq!(cue.text, before["cues"][i]["text"]);
        assert_eq!(cue.id, before["cues"][i]["id"]);
    }
    assert!(docs.scene_repair_accept(id).is_err());
    assert_eq!(
        docs.scene_repair_view(id, 0, 100).unwrap().phase,
        Phase::Accepted
    );
    let undone = docs.apply(Action::Undo, 1, 1).unwrap();
    assert_eq!(serde_json::to_value(&undone.cues).unwrap(), before["cues"]);
    assert!(!undone.can_undo && !undone.dirty);
    let redone = docs.apply(Action::Redo, 1, 2).unwrap();
    assert_eq!(
        serde_json::to_value(&redone.cues).unwrap(),
        serde_json::to_value(&accepted.cues).unwrap()
    );
    let output = dir.path().join("copy.srt");
    docs.apply(
        Action::Save {
            path: Some(output.to_string_lossy().into_owned()),
        },
        1,
        3,
    )
    .unwrap();
    let expected = String::from_utf8(source.clone())
        .unwrap()
        .replace("00:00:01,000", "00:00:01,130")
        .replace("00:00:04,050", "00:00:04,130")
        .replace("00:00:06,050", "00:00:05,870");
    assert_eq!(fs::read(&output).unwrap(), expected.as_bytes());
    assert_eq!(fs::read(dir.path().join("source.srt")).unwrap(), source);
    let reopened = Documents::default()
        .apply(
            Action::Open {
                path: output.to_string_lossy().into_owned(),
                discard: false,
            },
            0,
            0,
        )
        .unwrap();
    assert_eq!(
        serde_json::to_value(reopened.cues).unwrap(),
        serde_json::to_value(accepted.cues).unwrap()
    );
}
#[test]
fn profile_history_document_and_scene_replacement_prevent_stale_acceptance() {
    let (_dir, docs, scenes) = fixture();
    let id = start(&docs, &scenes);
    ready(&docs, id);
    let key = docs.scene_repair_context(id).unwrap();
    scenes.test_completed(7, vec![2.], true);
    assert!(
        scenes
            .with_quality_current(key, || docs.scene_repair_accept(id))
            .is_err()
    );
    assert_eq!(value(&docs)["revision"], 0);
    docs.scene_repair_cancel(id).unwrap();
    let id = start(&docs, &scenes);
    ready(&docs, id);
    docs.reset_quality().unwrap();
    assert!(docs.scene_repair_accept(id).is_err());
    assert_eq!(
        docs.scene_repair_view(id, 0, 100).unwrap().phase,
        Phase::Stale
    );
    let id = start(&docs, &scenes);
    ready(&docs, id);
    docs.apply(
        Action::Edit {
            cue_id: 1,
            start_ms: 1000,
            end_ms: 3000,
            text: "Changed".into(),
        },
        1,
        0,
    )
    .unwrap();
    docs.apply(Action::Undo, 1, 1).unwrap();
    assert!(docs.scene_repair_accept(id).is_err());
    let id = start(&docs, &scenes);
    ready(&docs, id);
    docs.apply(Action::Close { discard: true }, 1, 2).unwrap();
    assert!(docs.scene_repair_accept(id).is_err());
}
#[test]
fn failed_saves_and_recovery_resync_protection_survive_acceptance_and_undo() {
    let (dir, docs, scenes) = fixture();
    {
        let mut store = docs.0.lock().unwrap();
        let loaded = store.loaded_mut().unwrap();
        loaded.resynced = true;
        loaded.recovered = true;
    }
    let id = start(&docs, &scenes);
    ready(&docs, id);
    let accepted = docs.scene_repair_accept(id).unwrap();
    assert!(accepted.requires_save_as);
    let before = value(&docs);
    assert!(
        docs.apply(
            Action::Save {
                path: Some(dir.path().to_string_lossy().into_owned())
            },
            1,
            1
        )
        .is_err()
    );
    assert_eq!(value(&docs), before);
    assert!(docs.apply(Action::Undo, 1, 1).unwrap().requires_save_as);
}
#[test]
fn cancelling_failure_retry_and_shutdown_keep_one_worker_and_join_outside_lock() {
    let (_dir, docs, scenes) = fixture();
    let (entered, started) = mpsc::channel();
    let (release, wait) = mpsc::channel();
    let id = {
        let mut store = docs.0.lock().unwrap();
        let key = store.quality_key().unwrap();
        let profile = store.quality.context().profile;
        store
            .timing_repairs
            .launch(
                key,
                scenes.capture_quality(7, true).unwrap().key,
                Operation::Scene,
                profile,
                move |cancel| {
                    entered.send(cancel).unwrap();
                    wait.recv_timeout(Duration::from_secs(5)).unwrap();
                    Err("Injected failure".into())
                },
            )
            .unwrap()
    };
    let cancel = started.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(
        docs.scene_repair_start(1, 0, scenes.capture_quality(7, true).unwrap())
            .is_err()
    );
    docs.scene_repair_cancel(id).unwrap();
    assert!(cancel.load(Ordering::Relaxed));
    release.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while docs.scene_repair_view(id, 0, 100).unwrap().phase == Phase::Cancelling {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert!(docs.scene_repair_accept(id).is_err());
    let next = start(&docs, &scenes);
    ready(&docs, next);
    docs.scene_repair_cancel(id).unwrap();
    assert_eq!(
        docs.scene_repair_view(next, 0, 100).unwrap().phase,
        Phase::Reviewing
    );
    let clone = docs.clone();
    let (entered, started) = mpsc::channel();
    {
        let mut store = docs.0.lock().unwrap();
        let key = store.quality_key().unwrap();
        let profile = store.quality.context().profile;
        store
            .timing_repairs
            .launch(
                key,
                scenes.capture_quality(7, true).unwrap().key,
                Operation::Scene,
                profile,
                move |cancel| {
                    entered.send(()).unwrap();
                    let end = Instant::now() + Duration::from_secs(5);
                    while !cancel.load(Ordering::Relaxed) && Instant::now() < end {
                        std::thread::yield_now();
                    }
                    assert!(cancel.load(Ordering::Relaxed));
                    clone.0.lock().unwrap().snapshot(); // shutdown cannot hold the document mutex while joining
                    Err("Shutdown".into())
                },
            )
            .unwrap();
    }
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    docs.shutdown_quality().unwrap();
    assert!(docs.0.lock().unwrap().timing_repairs.job.is_none());
    assert!(
        docs.scene_repair_start(1, 0, scenes.capture_quality(7, true).unwrap())
            .is_err()
    );
}

#[test]
fn worker_failure_is_visible_and_a_new_preview_can_recover() {
    let (_dir, docs, scenes) = fixture();
    let id = {
        let mut store = docs.0.lock().unwrap();
        let key = store.quality_key().unwrap();
        let profile = store.quality.context().profile;
        store
            .timing_repairs
            .launch(
                key,
                scenes.capture_quality(7, true).unwrap().key,
                Operation::Scene,
                profile,
                |_| Err("Injected failure".into()),
            )
            .unwrap()
    };
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let view = docs.scene_repair_view(id, 0, 100).unwrap();
        if view.phase == Phase::Failed {
            assert_eq!(view.error.as_deref(), Some("Injected failure"));
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert!(docs.scene_repair_accept(id).is_err());
    let next = start(&docs, &scenes);
    assert_eq!(ready(&docs, next).changed, 2);
}
