use super::*;
use crate::{quality_profile::Policy, scenes::Scenes};

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
    docs.timing_fix_start(
        before.generation,
        before.revision,
        Scenes::default().capture_quality(0, false).unwrap(),
        Input::preset(true, false, false),
    )
    .unwrap()
}
pub(super) fn ready(docs: &Documents, id: u64) -> FixView {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let view = docs.timing_fix_view(id, 0, 100).unwrap();
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
fn selected_gap_rows_keep_bounded_ghosts_and_one_atomic_history_entry() {
    let (_dir, docs) = fixture();
    let original = docs.apply(Action::Status, 0, 0).unwrap();
    let id = docs
        .timing_fix_start(
            original.generation,
            original.revision,
            Scenes::default().capture_quality(0, false).unwrap(),
            Input {
                toggled: vec![1],
                ..Input::preset(true, false, false)
            },
        )
        .unwrap();
    let view = ready(&docs, id);
    assert!(view.selection_error.is_none(), "{:?}", view.selection_error);
    assert_eq!(view.changed, 1);
    assert!(view.rows[0].selected);
    assert!(!view.rows[1].selected);
    let visible = HashSet::from([CueId(original.cues[1].id)]);
    let projected = docs
        .timing_fix_view_for_cues(id, 0, 200, Section::Changes, Some(&visible))
        .unwrap();
    assert_eq!(projected.rows.len(), 1);
    assert_eq!(projected.rows[0].cue_index, 1);
    assert!(!projected.rows[0].selected);
    let accepted = docs.timing_repair_accept(id, Purpose::Fix).unwrap();
    assert_ne!(accepted.cues[0].end_ms, original.cues[0].end_ms);
    assert_eq!(
        serde_json::to_value(&accepted.cues[1..]).unwrap(),
        serde_json::to_value(&original.cues[1..]).unwrap()
    );
    assert!(docs.timing_repair_accept(id, Purpose::Fix).is_err());
    let undone = docs
        .apply(Action::Undo, accepted.generation, accepted.revision)
        .unwrap();
    assert_eq!(
        serde_json::to_value(&undone.cues).unwrap(),
        serde_json::to_value(&original.cues).unwrap()
    );
    assert!(!undone.can_undo);
    let redone = docs
        .apply(Action::Redo, undone.generation, undone.revision)
        .unwrap();
    assert_eq!(
        serde_json::to_value(&redone.cues).unwrap(),
        serde_json::to_value(&accepted.cues).unwrap()
    );
}
fn checked(docs: &Documents) -> crate::quality_check::View {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let current = docs.apply(Action::Status, 0, 0).unwrap();
        let report = docs
            .quality_read(crate::quality_check::Request {
                index_since: None,
                after_worker: None,
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
                selected_offset: 0,
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
    assert_eq!((view.changed, view.total, view.manual_count), (2, 2, 0));
    assert_eq!(view.coverage, Some(engine::scene::SceneCoverage::NoMedia));
    assert_eq!(value(&docs), before);
    let page = docs.timing_fix_view(id, 1, 1).unwrap();
    assert_eq!(page.rows.len(), 1);
    assert_eq!(page.rows[0].cue_index, 1);
    assert!(docs.timing_fix_view(id, 0, 201).is_err());
    assert_eq!(value(&docs), before);
    let accepted = docs.timing_repair_accept(id, Purpose::Fix).unwrap();
    assert_eq!(accepted.revision, original.revision + 1);
    assert_eq!(
        accepted.cues.iter().map(|c| c.end_ms).collect::<Vec<_>>(),
        [1970, 3020, 4100]
    );
    assert!(accepted.dirty && accepted.can_undo && !accepted.requires_save_as);
    assert_eq!(
        docs.timing_fix_view(id, 0, 100).unwrap().phase,
        Phase::Accepted
    );
    assert!(docs.timing_repair_accept(id, Purpose::Fix).is_err());
    let invalidated = docs
        .quality_read(crate::quality_check::Request {
            index_since: None,
            after_worker: None,
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
            selected_offset: 0,
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
    assert_eq!(
        docs.timing_fix_view(id, 0, 100).unwrap().phase,
        Phase::Stale
    );
    let before = value(&docs);
    assert!(docs.timing_repair_accept(id, Purpose::Fix).is_err());
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
    assert_eq!(
        docs.timing_fix_view(id, 0, 100).unwrap().phase,
        Phase::Stale
    );
    assert!(docs.timing_repair_accept(id, Purpose::Fix).is_err());
    let id = start(&docs);
    ready(&docs, id);
    let current = docs.apply(Action::Status, 0, 0).unwrap();
    docs.apply(
        Action::Close { discard: true },
        current.generation,
        current.revision,
    )
    .unwrap();
    assert!(docs.timing_repair_accept(id, Purpose::Fix).is_err());
    assert!(
        docs.timing_fix_start(
            1,
            0,
            Scenes::default().capture_quality(0, false).unwrap(),
            Input::preset(true, false, false)
        )
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
    let accepted = docs.timing_repair_accept(id, Purpose::Fix).unwrap();
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
