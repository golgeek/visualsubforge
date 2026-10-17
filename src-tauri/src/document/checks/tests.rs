use super::*;
use crate::quality_profile::Policy;

pub(super) fn fixture() -> Documents {
    let docs = Documents::default();
    for (after_id, start_ms, end_ms, text) in [
        (
            None,
            1000,
            2000,
            "First subtitle, too long for its duration",
        ),
        (Some(1), 2050, 2450, "Second"),
        (Some(2), 2300, 4000, "Third"),
    ] {
        let before = docs.apply(Action::Status, 0, 0).unwrap();
        docs.apply(
            Action::Create {
                after_id,
                start_ms,
                end_ms,
                text: text.into(),
            },
            before.generation,
            before.revision,
        )
        .unwrap();
    }
    docs
}
fn request(docs: &Documents) -> Request {
    let key = docs.0.lock().unwrap().quality_key().unwrap();
    Request {
        generation: key.generation,
        revision: key.revision,
        run: true,
        restart: false,
        offset: 0,
        limit: 200,
        metrics_offset: 0,
        metrics_limit: 200,
        selected: Some(1.try_into().unwrap()),
        rule: None,
        severity: None,
        draft: None,
    }
}
pub(super) fn ready(docs: &Documents) -> View {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let view = docs.quality_read(request(docs)).unwrap();
        if view.phase == Phase::Ready && !view.draft_checking {
            return view;
        }
        assert_ne!(view.phase, Phase::Failed, "{:?}", view.error);
        assert!(Instant::now() < deadline, "quality worker did not finish");
        std::thread::sleep(Duration::from_millis(1));
    }
}
#[test]
fn desktop_pages_match_shared_checker_and_checking_preserves_history() {
    let docs = fixture();
    let before = serde_json::to_value(docs.apply(Action::Status, 0, 0).unwrap()).unwrap();
    let report = {
        let store = docs.0.lock().unwrap();
        engine::evaluate(
            store.loaded.as_ref().unwrap().editor.document(),
            &store.quality.context().profile,
            |_, _| Ok(()),
        )
        .unwrap()
    };
    let view = ready(&docs);
    assert_eq!(view.findings, report.findings);
    assert_eq!(view.findings_count as usize, report.findings.len());
    assert_eq!(view.affected_cues, report.affected_cues);
    assert_eq!(
        view.profile_hash.as_deref(),
        Some(report.profile_hash.as_str())
    );
    for (projected, expected) in view.metrics.iter().zip(&report.metrics) {
        assert_eq!(&projected.data, expected);
    }
    let mut query = request(&docs);
    query.rule = Some(engine::Rule::Gap);
    query.limit = 1;
    let filtered = docs.quality_read(query).unwrap();
    assert_eq!((filtered.filtered_count, filtered.filtered_cues), (1, 2));
    assert_eq!(filtered.next_cue.as_deref(), Some("2"));
    assert_eq!(filtered.previous_cue.as_deref(), Some("2"));
    let mut query = request(&docs);
    query.limit = 201;
    assert!(docs.quality_read(query).is_err());
    assert_eq!(
        serde_json::to_value(docs.apply(Action::Status, 0, 0).unwrap()).unwrap(),
        before
    );
    let before = docs.apply(Action::Status, 0, 0).unwrap();
    let undo = docs
        .apply(Action::Undo, before.generation, before.revision)
        .unwrap();
    assert_eq!(undo.cues.len(), 2);
    assert!(docs.0.lock().unwrap().checks.report.is_none());
    docs.apply(Action::Redo, undo.generation, undo.revision)
        .unwrap();
    assert_eq!(ready(&docs).metrics.len(), 3);
}

#[test]
fn draft_metrics_are_explicit_read_only_and_use_the_same_overlap_policy() {
    let docs = fixture();
    docs.apply_quality_defaults(
        0,
        Policy::new(
            engine::Profile::builtin("sw3-uk-vo-subset").unwrap(),
            crate::timing_preset::Settings {
                cue_gap_ms: 160,
                scene_before_ms: 120,
                scene_after_ms: 120,
                ..Default::default()
            },
        )
        .unwrap(),
        || Ok(()),
    )
    .unwrap();
    let original = ready(&docs);
    let before = serde_json::to_value(docs.apply(Action::Status, 0, 0).unwrap()).unwrap();
    let draft = Draft {
        cue_id: 3.try_into().unwrap(),
        start_ms: 2300.try_into().unwrap(),
        end_ms: 5000.try_into().unwrap(),
        text: "{\\i1}A new draft\\Nstill literal for counting".into(),
    };
    let mut query = request(&docs);
    query.draft = Some(DraftChange::Update {
        value: draft.clone(),
    });
    docs.quality_read(query).unwrap();
    let view = ready(&docs);
    assert_eq!(view.findings, original.findings);
    assert_eq!(
        view.selected.as_ref().unwrap().data,
        original.selected.unwrap().data
    );
    assert_eq!(view.draft.as_ref().unwrap().data.duration_ms, 2700.);
    assert!(view.draft.as_ref().unwrap().data.reading_start_ms > 2300.);
    let expected = {
        let store = docs.0.lock().unwrap();
        let mut editor = Editor::new(store.loaded.as_ref().unwrap().editor.document().clone());
        editor
            .edit(
                0,
                CueId(3),
                Milliseconds::new(2300).unwrap(),
                Milliseconds::new(5000).unwrap(),
                draft.text.clone(),
            )
            .unwrap();
        engine::evaluate(
            editor.document(),
            &store.quality.context().profile,
            |_, _| Ok(()),
        )
        .unwrap()
        .metrics
        .remove(2)
    };
    assert_eq!(view.draft.as_ref().unwrap().data, expected);
    assert_ne!(
        view.draft.as_ref().unwrap().data.characters,
        view.selected.unwrap().data.characters
    );
    assert_eq!(
        serde_json::to_value(docs.apply(Action::Status, 0, 0).unwrap()).unwrap(),
        before
    );
    let mut query = request(&docs);
    query.draft = Some(DraftChange::Update {
        value: Draft {
            end_ms: 999.try_into().unwrap(),
            ..draft
        },
    });
    docs.quality_read(query).unwrap();
    let view = ready(&docs);
    assert!(view.draft.is_none());
    assert!(view.draft_error.is_some());
    assert_eq!(view.findings, original.findings);
    let mut query = request(&docs);
    query.draft = Some(DraftChange::Clear);
    assert!(docs.quality_read(query).unwrap().draft_error.is_none());
}

#[test]
fn stale_workers_cannot_publish_or_overlap_and_shutdown_reaps_work() {
    use std::sync::mpsc;
    let docs = fixture();
    let (entered, entered_rx) = mpsc::channel();
    let (release, release_rx) = mpsc::channel();
    let key;
    {
        let mut store = docs.0.lock().unwrap();
        key = store.quality_key().unwrap();
        store.checks.synchronize(key);
        store
            .checks
            .launch_with(key, None, move |cancel| {
                entered.send(cancel.clone()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                Err("obsolete worker failure".into())
            })
            .unwrap();
    }
    let token = entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let id = docs.0.lock().unwrap().checks.id;
    docs.apply(
        Action::Edit {
            cue_id: 1,
            start_ms: 1000,
            end_ms: 2100,
            text: "Accepted edit".into(),
        },
        key.generation.get(),
        key.revision.get(),
    )
    .unwrap();
    assert!(token.load(Ordering::Relaxed));
    // A delayed close/cancel from the previous document must not pause the new
    // context while its old worker is draining and before a new ID is admitted.
    docs.quality_cancel(id).unwrap();
    assert!(!docs.0.lock().unwrap().checks.cancelled);
    let view = docs.quality_read(request(&docs)).unwrap();
    assert_eq!(view.phase, Phase::Cancelling);
    assert_eq!(view.id.get(), id);
    assert!(view.metrics.is_empty());
    release.send(()).unwrap();
    let view = ready(&docs);
    assert!(view.id.get() > id);
    assert!(view.error.is_none());
    docs.quality_cancel(id).unwrap(); // delayed cancellation cannot stop the new work
    assert_eq!(
        docs.quality_read(request(&docs)).unwrap().phase,
        Phase::Ready
    );
    let epoch = docs.quality_context().unwrap().epoch;
    docs.apply_quality_defaults(
        epoch,
        Policy::legacy(
            crate::timing_preset::Settings {
                cue_gap_ms: 167,
                ..Default::default()
            },
            "test",
        )
        .unwrap(),
        || Ok(()),
    )
    .unwrap();
    assert!(docs.0.lock().unwrap().checks.report.is_none());
    assert_ne!(ready(&docs).key.epoch, key.epoch);
    let exited = Arc::new(AtomicBool::new(false));
    let stopped = exited.clone();
    let (entered, started) = mpsc::channel();
    {
        let mut store = docs.0.lock().unwrap();
        let key = store.quality_key().unwrap();
        store
            .checks
            .launch_with(key, None, move |cancel| {
                entered.send(()).unwrap();
                let deadline = Instant::now() + Duration::from_secs(5);
                while !cancel.load(Ordering::Relaxed) && Instant::now() < deadline {
                    std::thread::yield_now();
                }
                stopped.store(cancel.load(Ordering::Relaxed), Ordering::Release);
                Err("cancelled at shutdown".into())
            })
            .unwrap();
    }
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    docs.shutdown_quality().unwrap();
    assert!(exited.load(Ordering::Acquire));
    assert_eq!(
        docs.quality_read(request(&docs)).unwrap().phase,
        Phase::Closed
    );
    assert!(docs.0.lock().unwrap().checks.worker.is_none());
}

#[test]
fn cancelled_check_requires_explicit_restart_and_line_projection_is_bounded() {
    let docs = fixture();
    let view = ready(&docs);
    docs.quality_cancel(view.id.get()).unwrap();
    assert_eq!(
        docs.quality_read(request(&docs)).unwrap().phase,
        Phase::Cancelled
    );
    let mut query = request(&docs);
    query.restart = true;
    docs.quality_read(query).unwrap();
    assert_eq!(ready(&docs).phase, Phase::Ready);
    let mut row = view.metrics[0].data.clone();
    row.line_characters = vec![7; 100_000];
    row.line_characters[99_999] = 300;
    let metric = Metric::from(&row);
    assert_eq!(metric.line_count, 100_000);
    assert_eq!(metric.longest_line, 300);
    assert!(metric.lines_truncated);
    assert_eq!(metric.data.line_characters.len(), 20);
    assert!(serde_json::to_vec(&metric).unwrap().len() < 1500);
}
