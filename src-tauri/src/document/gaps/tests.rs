use super::*;
use crate::{quality_profile::Policy, scenes::Scenes};
use std::sync::mpsc;

pub(super) fn fixture() -> (tempfile::TempDir, Documents) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("subtitles.srt");
    fs::write(&path,"1\n00:00:01,000 --> 00:00:02,000\n{\\i1}First\n\n2\n00:00:02,050 --> 00:00:03,050\nSecond\n\n3\n00:00:03,100 --> 00:00:04,100\nThird\n\n").unwrap();
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
    (directory, docs)
}
pub(super) fn start(docs: &Documents) -> u64 {
    let before = docs.apply(Action::Status, 0, 0).unwrap();
    docs.gap_start(
        before.generation,
        before.revision,
        Scenes::default().capture_quality(0, false).unwrap(),
    )
    .unwrap()
}
pub(super) fn ready(docs: &Documents, id: u64) -> GapView {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let view = docs.gap_view(id, 0, 100).unwrap();
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
fn checked(docs: &Documents) -> crate::quality_check::View {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let current = docs.apply(Action::Status, 0, 0).unwrap();
        let report = docs
            .quality_read(crate::quality_check::Request {
                session: 0.try_into().unwrap(),
                generation: current.generation.try_into().unwrap(),
                revision: current.revision.try_into().unwrap(),
                run: true,
                restart: false,
                offset: 0,
                limit: 100,
                metrics_offset: 0,
                metrics_limit: 100,
                selected: None,
                rule: None,
                severity: None,
                draft: None,
            })
            .unwrap();
        if report.profile_hash.is_some() {
            return report;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
}
#[test]
fn bounded_preview_is_read_only_and_acceptance_is_one_undo_redo_with_recheck() {
    let (_dir, docs) = fixture();
    let before = value(&docs);
    let original = docs.apply(Action::Status, 0, 0).unwrap();
    assert_eq!(
        checked(&docs)
            .findings
            .iter()
            .filter(|f| f.rule == engine::Rule::Gap)
            .count(),
        2
    );
    let id = start(&docs);
    let view = ready(&docs, id);
    assert_eq!((view.changed, view.total, view.skipped), (2, 2, 0));
    assert_eq!(view.coverage, Some(engine::gap::SceneCoverage::NoMedia));
    assert_eq!(value(&docs), before);
    let page = docs.gap_view(id, 1, 1).unwrap();
    assert_eq!(page.rows.len(), 1);
    assert_eq!(page.rows[0].cue_index, 1);
    assert!(docs.gap_view(id, 0, 201).is_err());
    assert!(docs.gap_accept(id, false).is_err());
    assert_eq!(value(&docs), before);
    let accepted = docs.gap_accept(id, true).unwrap();
    assert_eq!(accepted.revision, original.revision + 1);
    assert_eq!(
        accepted.cues.iter().map(|c| c.end_ms).collect::<Vec<_>>(),
        [1970, 3020, 4100]
    );
    assert!(accepted.dirty && accepted.can_undo && !accepted.requires_save_as);
    assert_eq!(docs.gap_view(id, 0, 100).unwrap().phase, Phase::Accepted);
    assert!(docs.gap_accept(id, true).is_err());
    let invalidated = docs
        .quality_read(crate::quality_check::Request {
            session: 0.try_into().unwrap(),
            generation: accepted.generation.try_into().unwrap(),
            revision: accepted.revision.try_into().unwrap(),
            run: false,
            restart: false,
            offset: 0,
            limit: 100,
            metrics_offset: 0,
            metrics_limit: 100,
            selected: None,
            rule: None,
            severity: None,
            draft: None,
        })
        .unwrap();
    assert!(invalidated.profile_hash.is_none() && invalidated.metrics.is_empty());
    assert!(
        !checked(&docs)
            .findings
            .iter()
            .any(|f| f.rule == engine::Rule::Gap)
    );
    let undone = docs
        .apply(Action::Undo, accepted.generation, accepted.revision)
        .unwrap();
    assert!(!undone.can_undo && undone.can_redo && !undone.dirty);
    assert_eq!(serde_json::to_value(&undone.cues).unwrap(), before["cues"]);
    let redone = docs
        .apply(Action::Redo, undone.generation, undone.revision)
        .unwrap();
    assert_eq!(
        serde_json::to_value(redone.cues).unwrap(),
        serde_json::to_value(accepted.cues).unwrap()
    );
}
#[test]
fn profile_history_and_document_replacement_retire_proposals_without_mutation() {
    let (_dir, docs) = fixture();
    let id = start(&docs);
    ready(&docs, id);
    docs.apply(
        Action::Edit {
            cue_id: 1,
            start_ms: 1000,
            end_ms: 2000,
            text: "Changed".into(),
        },
        1,
        0,
    )
    .unwrap();
    docs.apply(Action::Undo, 1, 1).unwrap();
    assert_eq!(docs.gap_view(id, 0, 100).unwrap().phase, Phase::Stale);
    let before = value(&docs);
    assert!(docs.gap_accept(id, true).is_err());
    assert_eq!(value(&docs), before);
    let id = start(&docs);
    ready(&docs, id);
    let context = docs.quality_context().unwrap();
    let mut timing = context.timing;
    timing.cue_gap_ms = 160;
    docs.apply_quality_defaults(
        context.epoch,
        Policy::legacy(timing, "test").unwrap(),
        || Ok(()),
    )
    .unwrap();
    assert_eq!(docs.gap_view(id, 0, 100).unwrap().phase, Phase::Stale);
    assert!(docs.gap_accept(id, true).is_err());
    let id = start(&docs);
    ready(&docs, id);
    let current = docs.apply(Action::Status, 0, 0).unwrap();
    docs.apply(
        Action::Close { discard: true },
        current.generation,
        current.revision,
    )
    .unwrap();
    assert!(docs.gap_accept(id, true).is_err());
    assert!(
        docs.gap_start(1, 0, Scenes::default().capture_quality(0, false).unwrap())
            .is_err()
    );
}
#[test]
fn gap_acceptance_preserves_recovery_resync_and_failed_save_protection() {
    let (dir, docs) = fixture();
    {
        let mut store = docs.0.lock().unwrap();
        let loaded = store.loaded_mut().unwrap();
        loaded.resynced = true;
        loaded.recovered = true;
    }
    let id = start(&docs);
    ready(&docs, id);
    let accepted = docs.gap_accept(id, true).unwrap();
    assert!(accepted.requires_save_as);
    assert!(
        docs.apply(
            Action::Save {
                path: Some(dir.path().to_string_lossy().into_owned())
            },
            accepted.generation,
            accepted.revision
        )
        .is_err()
    );
    let after = value(&docs);
    assert_eq!(after["requiresSaveAs"], true);
    assert_eq!(after["dirty"], true);
    assert_eq!(after["revision"], accepted.revision);
    let undone = docs
        .apply(Action::Undo, accepted.generation, accepted.revision)
        .unwrap();
    assert!(undone.requires_save_as);
    assert_eq!(undone.cues[0].end_ms, 2000);
}
#[test]
fn stale_worker_is_cancelled_cannot_publish_or_overlap_and_shutdown_joins() {
    let (_dir, docs) = fixture();
    let (entered, entered_rx) = mpsc::channel();
    let (release, release_rx) = mpsc::channel();
    let id = {
        let mut store = docs.0.lock().unwrap();
        let key = store.quality_key().unwrap();
        let scene = Scenes::default().capture_quality(0, false).unwrap().key;
        let profile = store.quality.context().profile;
        store
            .gaps
            .launch(key, scene, profile, move |cancel| {
                entered.send(cancel).unwrap();
                release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                Err("Obsolete failure".into())
            })
            .unwrap()
    };
    let cancel = entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    docs.apply(
        Action::Edit {
            cue_id: 1,
            start_ms: 1000,
            end_ms: 2000,
            text: "Edited".into(),
        },
        1,
        0,
    )
    .unwrap();
    assert!(cancel.load(Ordering::Relaxed));
    assert!(
        docs.gap_start(1, 1, Scenes::default().capture_quality(0, false).unwrap())
            .is_err()
    );
    release.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let view = docs.gap_view(id, 0, 100).unwrap();
        assert_eq!(view.phase, Phase::Stale);
        assert!(view.rows.is_empty() && view.error.is_none());
        if docs
            .0
            .lock()
            .unwrap()
            .gaps
            .job
            .as_ref()
            .unwrap()
            .worker
            .is_none()
        {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    let newer = start(&docs);
    ready(&docs, newer);
    docs.gap_cancel(id).unwrap();
    assert_eq!(
        docs.gap_view(newer, 0, 100).unwrap().phase,
        Phase::Reviewing
    );
    docs.gap_cancel(newer).unwrap();
    assert!(docs.gap_accept(newer, true).is_err());
    let stopped = Arc::new(AtomicBool::new(false));
    let signal = stopped.clone();
    let (entered, started) = mpsc::channel();
    {
        let mut store = docs.0.lock().unwrap();
        let key = store.quality_key().unwrap();
        let scene = Scenes::default().capture_quality(0, false).unwrap().key;
        let profile = store.quality.context().profile;
        store
            .gaps
            .launch(key, scene, profile, move |cancel| {
                entered.send(()).unwrap();
                let end = Instant::now() + Duration::from_secs(5);
                while !cancel.load(Ordering::Relaxed) && Instant::now() < end {
                    std::thread::yield_now();
                }
                signal.store(cancel.load(Ordering::Relaxed), Ordering::Release);
                Err("Shutdown".into())
            })
            .unwrap();
    }
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    docs.shutdown_quality().unwrap();
    assert!(stopped.load(Ordering::Acquire));
    assert!(
        docs.gap_start(1, 1, Scenes::default().capture_quality(0, false).unwrap())
            .is_err()
    );
    assert!(docs.0.lock().unwrap().gaps.job.is_none());
}
