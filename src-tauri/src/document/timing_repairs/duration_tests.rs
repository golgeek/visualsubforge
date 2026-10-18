use super::*;
use crate::{quality_profile::Policy, scenes::Scenes};
use std::sync::mpsc;

pub(super) fn fixture() -> (tempfile::TempDir, Documents, Scenes) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("source.srt");
    fs::write(&path,"\u{feff}1\r\n00:00:01,000 --> 00:00:01,400\r\n{\\i1}0123456789é😀abcdefg\r\n\r\n2\r\n00:00:05,000 --> 00:00:13,000\r\n0123456789\r\n\r\n3\r\n00:00:14,000 --> 00:00:14,400\r\n{\\k20}Timed\r\n\r\n").unwrap();
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
    let mut p =
        Profile::default().custom("duration-owner-test".into(), "Duration owner test".into());
    p.rules.duration.maximum_ms = 3000;
    p.rules.speed.minimum_cps = Some(5);
    p.rules.speed.maximum_cps = Some(10);
    docs.initialize_quality(Policy::new(p, Default::default()).unwrap())
        .unwrap();
    let scenes = Scenes::default();
    scenes.test_completed(7, vec![3., 7.], false);
    (directory, docs, scenes)
}
pub(super) fn start(docs: &Documents, scenes: &Scenes, scope: Scope) -> u64 {
    let d = docs.apply(Action::Status, 0, 0).unwrap();
    docs.duration_repair_start(
        d.generation,
        d.revision,
        scenes.capture_quality(7, true).unwrap(),
        scope,
    )
    .unwrap()
}
pub(super) fn ready(docs: &Documents, id: u64) -> DurationRepairView {
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        let v = docs.duration_repair_view(id, 0, 100).unwrap();
        if v.phase == Phase::Reviewing {
            return v;
        }
        assert_eq!(v.phase, Phase::Preparing, "{:?}", v.error);
        assert!(Instant::now() < end);
        std::thread::yield_now();
    }
}
fn status(docs: &Documents) -> Snapshot {
    docs.apply(Action::Status, 0, 0).unwrap()
}
fn value(docs: &Documents) -> serde_json::Value {
    serde_json::to_value(status(docs)).unwrap()
}
#[test]
fn duration_preview_accepts_one_atomic_end_only_batch_and_preserves_exact_srt() {
    let (dir, docs, scenes) = fixture();
    let source = fs::read(dir.path().join("source.srt")).unwrap();
    let before = value(&docs);
    let id = start(&docs, &scenes, Scope::Document);
    let v = ready(&docs, id);
    assert_eq!((v.total, v.changed, v.skipped), (3, 2, 1));
    assert_eq!((v.before_findings, v.after_findings), (9, 3));
    assert_eq!(v.selected_cue_id, None);
    assert_eq!(value(&docs), before);
    assert_eq!(
        docs.duration_repair_view(id, 1, 1).unwrap().rows[0].cue_index,
        1
    );
    assert!(docs.duration_repair_view(id, 0, 201).is_err());
    let accepted = docs.duration_repair_accept(id).unwrap();
    assert_eq!(accepted.revision, 1);
    assert_eq!(
        accepted.cues.iter().map(|c| c.end_ms).collect::<Vec<_>>(),
        [3130, 6870, 14400]
    );
    for (i, c) in accepted.cues.iter().enumerate() {
        assert_eq!(c.id, before["cues"][i]["id"]);
        assert_eq!(c.text, before["cues"][i]["text"]);
        assert_eq!(c.start_ms, before["cues"][i]["startMs"]);
    }
    assert!(docs.duration_repair_accept(id).is_err());
    let undo = docs.apply(Action::Undo, 1, 1).unwrap();
    assert_eq!(serde_json::to_value(undo.cues).unwrap(), before["cues"]);
    assert!(!undo.can_undo && !undo.dirty);
    let redo = docs.apply(Action::Redo, 1, 2).unwrap();
    assert_eq!(
        serde_json::to_value(redo.cues).unwrap(),
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
        .replace("00:00:01,400", "00:00:03,130")
        .replace("00:00:13,000", "00:00:06,870");
    assert_eq!(fs::read(&output).unwrap(), expected.as_bytes());
    assert_eq!(fs::read(dir.path().join("source.srt")).unwrap(), source);
    let opened = Documents::default()
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
        serde_json::to_value(opened.cues).unwrap(),
        serde_json::to_value(accepted.cues).unwrap()
    );
}
#[test]
fn selected_scope_and_wrong_kind_cannot_apply_or_cancel_another_tool() {
    let (_dir, docs, scenes) = fixture();
    let id = start(&docs, &scenes, Scope::Cue(CueId(2)));
    let v = ready(&docs, id);
    assert_eq!(v.selected_cue_id.as_deref(), Some("2"));
    assert_eq!((v.total, v.changed), (1, 1));
    assert!(docs.scene_repair_view(id, 0, 100).is_err());
    assert!(docs.scene_repair_accept(id).is_err());
    assert!(docs.scene_repair_cancel(id).is_err());
    assert!(docs.timing_repair_context(id, Purpose::Scene).is_err());
    assert_eq!(
        docs.duration_repair_view(id, 0, 100).unwrap().phase,
        Phase::Reviewing
    );
    let accepted = docs.duration_repair_accept(id).unwrap();
    assert_eq!(
        accepted.cues.iter().map(|c| c.end_ms).collect::<Vec<_>>(),
        [1400, 6870, 14400]
    );
    let old = start(&docs, &scenes, Scope::Document);
    ready(&docs, old);
    let scene = docs
        .scene_repair_start(1, 1, scenes.capture_quality(7, true).unwrap())
        .unwrap();
    super::tests::ready(&docs, scene);
    assert!(docs.duration_repair_view(old, 0, 100).is_err());
    assert!(docs.duration_repair_accept(scene).is_err());
    assert!(docs.timing_repair_cancel(scene, Purpose::Duration).is_err());
}
#[test]
fn duration_failure_save_and_protected_recovery_resync_state_remain_unchanged() {
    let (dir, docs, scenes) = fixture();
    {
        let mut s = docs.0.lock().unwrap();
        let d = s.loaded_mut().unwrap();
        d.recovered = true;
        d.resynced = true;
    }
    let id = start(&docs, &scenes, Scope::Document);
    ready(&docs, id);
    assert!(docs.duration_repair_accept(id).unwrap().requires_save_as);
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
    assert!(docs.apply(Action::Redo, 1, 2).unwrap().requires_save_as);
}
#[test]
fn document_profile_scene_and_history_guards_reject_duration_previews() {
    let (_dir, docs, scenes) = fixture();
    let id = start(&docs, &scenes, Scope::Document);
    ready(&docs, id);
    let key = docs.timing_repair_context(id, Purpose::Duration).unwrap();
    scenes.test_completed(7, vec![4.], true);
    assert!(
        scenes
            .with_quality_current(key, || docs.duration_repair_accept(id))
            .is_err()
    );
    assert_eq!(value(&docs)["revision"], 0);
    docs.timing_repair_cancel(id, Purpose::Duration).unwrap();
    let id = start(&docs, &scenes, Scope::Document);
    ready(&docs, id);
    docs.reset_quality().unwrap();
    assert!(docs.duration_repair_accept(id).is_err());
    assert_eq!(
        docs.duration_repair_view(id, 0, 100).unwrap().phase,
        Phase::Stale
    );
    let id = start(&docs, &scenes, Scope::Document);
    ready(&docs, id);
    docs.apply(
        Action::Edit {
            cue_id: 1,
            start_ms: 1000,
            end_ms: 1400,
            text: "new text".into(),
        },
        1,
        0,
    )
    .unwrap();
    docs.apply(Action::Undo, 1, 1).unwrap();
    assert!(docs.duration_repair_accept(id).is_err());
    let id = start(&docs, &scenes, Scope::Document);
    ready(&docs, id);
    docs.apply(Action::Close { discard: true }, 1, 2).unwrap();
    assert!(docs.duration_repair_accept(id).is_err());
}
#[test]
fn shared_worker_has_no_cross_tool_queue_and_late_cancel_cannot_affect_retry() {
    let (_dir, docs, scenes) = fixture();
    let (send, recv) = mpsc::channel();
    let (release, wait) = mpsc::channel();
    let id = {
        let mut s = docs.0.lock().unwrap();
        let key = s.quality_key().unwrap();
        let p = s.quality.context().profile;
        s.timing_repairs
            .launch(
                key,
                scenes.capture_quality(7, true).unwrap().key,
                Operation::Duration(Scope::Document),
                p,
                move |cancel| {
                    send.send(cancel).unwrap();
                    wait.recv_timeout(Duration::from_secs(5)).unwrap();
                    Err("injected".into())
                },
            )
            .unwrap()
    };
    let token = recv.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(
        docs.scene_repair_start(1, 0, scenes.capture_quality(7, true).unwrap())
            .is_err()
    );
    assert!(
        docs.duration_repair_start(
            1,
            0,
            scenes.capture_quality(7, true).unwrap(),
            Scope::Document
        )
        .is_err()
    );
    docs.timing_repair_cancel(id, Purpose::Duration).unwrap();
    assert!(token.load(Ordering::Relaxed));
    release.send(()).unwrap();
    let end = Instant::now() + Duration::from_secs(5);
    while docs.duration_repair_view(id, 0, 100).unwrap().phase == Phase::Cancelling {
        assert!(Instant::now() < end);
        std::thread::yield_now();
    }
    assert!(docs.duration_repair_accept(id).is_err());
    let next = start(&docs, &scenes, Scope::Document);
    ready(&docs, next);
    docs.timing_repair_cancel(id, Purpose::Duration).unwrap();
    assert_eq!(
        docs.duration_repair_view(next, 0, 100).unwrap().phase,
        Phase::Reviewing
    );
    docs.shutdown_quality().unwrap();
    assert!(docs.0.lock().unwrap().timing_repairs.job.is_none());
}
#[test]
fn missing_scene_data_and_missing_cue_never_publish_an_acceptable_plan() {
    let (_dir, docs, scenes) = fixture();
    let id = docs
        .duration_repair_start(
            1,
            0,
            scenes.capture_quality(0, false).unwrap(),
            Scope::Document,
        )
        .unwrap();
    let v = ready(&docs, id);
    assert_eq!(v.changed, 0);
    assert_eq!(v.coverage, Some(engine::scene::SceneCoverage::NoMedia));
    assert!(docs.duration_repair_accept(id).is_err());
    let id = start(&docs, &scenes, Scope::Cue(CueId(99)));
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        let v = docs.duration_repair_view(id, 0, 100).unwrap();
        if v.phase == Phase::Failed {
            assert!(v.error.unwrap().contains("Selected cue"));
            break;
        }
        assert!(Instant::now() < end);
        std::thread::yield_now();
    }
    assert!(docs.duration_repair_accept(id).is_err());
    let id = start(&docs, &scenes, Scope::Document);
    assert_eq!(ready(&docs, id).changed, 2);
}
