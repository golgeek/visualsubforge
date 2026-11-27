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
        index_since: Some(0.try_into().unwrap()),
        after_worker: None,
        session: 0.try_into().unwrap(),
        generation: key.generation,
        revision: key.revision,
        run: true,
        restart: false,
        offset: 0,
        limit: 200,
        metrics_offset: 0,
        metrics_limit: 200,
        selected: Some(1.try_into().unwrap()),
        selected_offset: 0,
        rule: None,
        severity: None,
        draft: None,
    }
}

#[test]
fn scene_replacement_invalidates_reports_and_ignores_retired_worker_cancellation() {
    use std::sync::mpsc;
    let docs = fixture();
    let scenes = crate::scenes::Scenes::default();
    scenes.test_completed(7, vec![1.], false);
    let read = || {
        let mut input = request(&docs);
        input.session = 7.try_into().unwrap();
        docs.quality_read_scenes(input, scenes.capture_quality(7, true).unwrap())
            .unwrap()
    };
    let ready = || {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let view = read();
            if view.phase == Phase::Ready {
                break view;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
    };
    let before = ready();
    assert!(
        before
            .findings
            .iter()
            .any(|f| f.rule == engine::Rule::SceneStart)
    );
    let (entered, started) = mpsc::channel();
    let (release, waiting) = mpsc::channel();
    let id = {
        let mut store = docs.0.lock().unwrap();
        let key = store.quality_key().unwrap();
        store
            .checks
            .launch_with(key, None, move |cancel, _| {
                entered.send(cancel).unwrap();
                waiting.recv_timeout(Duration::from_secs(5)).unwrap();
                Err("retired scene failure".into())
            })
            .unwrap();
        store.checks.id
    };
    let token = started.recv_timeout(Duration::from_secs(5)).unwrap();
    scenes.test_completed(7, vec![99.], true);
    let pending = read();
    assert_eq!(pending.phase, Phase::Cancelling);
    assert!(pending.profile_hash.is_none() && pending.findings.is_empty());
    assert!(token.load(Ordering::Relaxed));
    docs.quality_cancel(id).unwrap();
    assert!(!docs.0.lock().unwrap().checks.cancelled);
    release.send(()).unwrap();
    let after = ready();
    assert_eq!(after.key, before.key);
    assert!(after.id.get() > id);
    assert_eq!(
        after.scene_coverage,
        Some(engine::scene::SceneCoverage::Reduced)
    );
    assert!(
        !after
            .findings
            .iter()
            .any(|f| matches!(f.rule, engine::Rule::SceneStart | engine::Rule::SceneEnd))
    );
    assert!(after.error.is_none());
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

#[tokio::test]
async fn completion_can_be_observed_before_or_after_a_worker_finishes() {
    let docs = fixture();
    let (release, held) = std::sync::mpsc::channel();
    let id = {
        let mut store = docs.0.lock().unwrap();
        let key = store.quality_key().unwrap();
        store.checks.synchronize(key);
        store.checks.scenes = Some(
            crate::scenes::Scenes::default()
                .capture_quality(0, false)
                .unwrap()
                .key,
        );
        store
            .checks
            .launch_with(key, None, move |_, _| {
                held.recv_timeout(Duration::from_secs(5)).unwrap();
                Err("test worker finished".into())
            })
            .unwrap();
        store.checks.id
    };
    let completion = docs.quality_completion(id).unwrap().unwrap();
    assert!(!completion.is_cancelled());
    assert!(docs.quality_completion(id + 1).unwrap().is_none());
    release.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(5), completion.cancelled())
        .await
        .unwrap();
    assert!(docs.quality_completion(id).unwrap().unwrap().is_cancelled());
    let mut query = request(&docs);
    query.run = false;
    let view = docs.quality_read(query).unwrap();
    assert_eq!(view.phase, Phase::Failed);
    assert!(docs.quality_completion(id).unwrap().is_none());
}

#[test]
fn compact_index_includes_related_cues_and_is_sent_once_per_report() {
    let docs = fixture();
    let view = ready(&docs);
    let index = view.issue_index.unwrap();
    let mut violations = std::collections::BTreeSet::new();
    let mut advisories = std::collections::BTreeSet::new();
    for finding in &view.findings {
        for id in std::iter::once(&finding.cue_id).chain(finding.related_cue_id.iter()) {
            if finding.severity == engine::Severity::Violation {
                violations.insert(id.clone());
            } else {
                advisories.insert(id.clone());
            }
        }
    }
    assert_eq!(
        index
            .violations
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>(),
        violations
    );
    assert_eq!(
        index
            .advisories
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>(),
        advisories.difference(&violations).cloned().collect()
    );
    let mut next = request(&docs);
    next.rule = Some(engine::Rule::Overlap);
    next.index_since = view.report_id;
    let filtered = docs.quality_read(next).unwrap();
    assert!(filtered.issue_index.is_none());
    assert_eq!(filtered.report_id, view.report_id);
    let mut first = request(&docs);
    first.rule = Some(engine::Rule::Empty);
    let first = docs.quality_read(first).unwrap();
    assert_eq!(first.filtered_count, 0);
    assert!(first.issue_index.is_some());
}

#[test]
fn finding_pages_preserve_valid_projection_cursors() {
    let docs = fixture();
    let sample = ready(&docs).findings.remove(0);
    for count in [0_u32, 1, 100, 101, 150, 199, 200, 201, 20_000, 50_000] {
        let mut query = request(&docs);
        query.selected = None;
        let mut store = docs.0.lock().unwrap();
        let report = store.checks.report.as_mut().unwrap();
        report.data.findings = (0..count)
            .map(|i| engine::Finding {
                cue_index: i,
                ..sample.clone()
            })
            .collect();
        report.index = ReportIndex::build(&report.data, || Ok(())).unwrap();
        let last_aligned = count.saturating_sub(1) / 200 * 200;
        let last_projection = if count <= 100 {
            0
        } else {
            100 + (count - 101) / 200 * 200
        };
        for (offset, limit, expected) in [
            (0, 100, 0),
            (100, 200, if count > 100 { 100 } else { last_aligned }),
            (last_projection, 200, last_projection),
            (count, 200, last_aligned),
            (u32::MAX, 200, last_aligned),
            (u32::MAX, 0, count.saturating_sub(1)),
        ] {
            query.offset = offset;
            query.limit = limit;
            let view = project_view(
                &store.checks,
                store.quality_key().unwrap(),
                store.quality.context().profile,
                &query,
            )
            .unwrap();
            assert_eq!(view.offset, expected, "count={count}, offset={offset}");
            let indexes: Vec<_> = view.findings.iter().map(|f| f.cue_index).collect();
            assert_eq!(
                indexes,
                (expected..count.min(expected + limit)).collect::<Vec<_>>()
            );
        }
    }
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
            .launch_with(key, None, move |cancel, _| {
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
            .launch_with(key, None, move |cancel, _| {
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

#[test]
fn selected_issues_group_speed_sort_errors_and_page_without_problem_filters() {
    use vsf_application::quality::{Finding, Rule, Severity};
    let finding = |rule, severity, cue: &str| Finding {
        rule,
        severity,
        cue_id: cue.into(),
        cue_index: 0,
        related_cue_id: None,
        measured: None,
        expected: "expected".into(),
        explanation: "detail".into(),
        language: None,
    };
    let mut findings = vec![
        finding(Rule::CpsHigh, Severity::Advisory, "1"),
        finding(Rule::RsHigh, Severity::Violation, "1"),
        finding(Rule::DurationLong, Severity::Violation, "2"),
    ];
    let mut related = finding(Rule::Overlap, Severity::Violation, "2");
    related.related_cue_id = Some("1".into());
    findings.push(related);
    findings.extend((0..205).map(|_| finding(Rule::Spelling, Severity::Advisory, "1")));
    let selected = |id: &str, offset| {
        cue_issues(
            findings
                .iter()
                .filter(|f| f.cue_id == id || f.related_cue_id.as_deref() == Some(id)),
            id,
            offset,
        )
    };
    let first = selected("1", 0);
    assert_eq!(first.count, 207);
    assert_eq!(first.issues.len(), 200);
    assert_eq!(first.issues[0].rule, Rule::RsHigh);
    assert_eq!(first.issues[0].findings.len(), 2);
    assert_eq!(first.issues[1].rule, Rule::Overlap);
    let tail = selected("1", 206);
    assert_eq!((tail.offset, tail.issues.len()), (206, 1));
    let last = selected("1", 200);
    assert_eq!(last.offset, 200);
    assert_eq!(last.issues.len(), 7);
    assert_eq!(selected("1", u32::MAX).offset, 200);
    assert!(selected("missing", 0).issues.is_empty());
}

#[test]
fn rule_counts_deduplicate_cues_and_related_findings_on_the_worker() {
    let docs = fixture();
    let view = ready(&docs);
    assert!(!view.rule_cues.is_empty());
    for count in &view.rule_cues {
        let mut request = request(&docs);
        request.rule = Some(count.rule);
        let filtered = docs.quality_read(request).unwrap();
        assert_eq!(count.cues, filtered.filtered_cues);
    }
    let mut store = docs.0.lock().unwrap();
    let report = &mut store.checks.report.as_mut().unwrap().data;
    report.findings.extend(view.findings.iter().cloned());
    let doubled = ReportIndex::build(report, || Ok(())).unwrap();
    assert_eq!(
        doubled
            .rule_counts
            .iter()
            .map(|c| (c.rule, c.cues))
            .collect::<Vec<_>>(),
        view.rule_cues
            .iter()
            .map(|c| (c.rule, c.cues))
            .collect::<Vec<_>>()
    );
    assert!(ReportIndex::build(report, || Err("cancelled".into())).is_err());
}

#[test]
fn recent_summary_uses_only_current_completed_checks_of_saved_content() {
    let docs = fixture();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("accepted.srt");
    let snapshot = docs.apply(Action::Status, 0, 0).unwrap();
    docs.apply(
        Action::Save {
            path: Some(path.to_string_lossy().into_owned()),
        },
        snapshot.generation,
        snapshot.revision,
    )
    .unwrap();
    let scenes = crate::scenes::Scenes::default();
    let key = || scenes.capture_quality(0, false).unwrap().key;
    let before = request(&docs);
    assert!(
        docs.recent_subtitles(before.generation.get(), before.revision.get(), key())
            .unwrap()
            .unwrap()
            .summary
            .is_none()
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    let report = loop {
        let view = docs.quality_read(request(&docs)).unwrap();
        if view.phase == Phase::Ready {
            break view;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    };
    let captured = docs
        .recent_subtitles(before.generation.get(), before.revision.get(), key())
        .unwrap()
        .unwrap();
    let summary = captured.summary.unwrap();
    assert_eq!(summary.cues, 3);
    assert!(
        !summary.complete,
        "Media-free scene coverage remains explicitly partial"
    );
    assert_eq!(summary.issues, report.findings_count);
    assert_eq!(Some(summary.profile_hash), report.profile_hash);
    assert_eq!(captured.bytes.unwrap(), fs::read(&path).unwrap());
    let recorded = summary.checked_at_ms;
    assert_eq!(
        docs.recent_subtitles(before.generation.get(), before.revision.get(), key())
            .unwrap()
            .unwrap()
            .summary
            .unwrap()
            .checked_at_ms,
        recorded
    );
    assert!(
        docs.recent_subtitles(before.generation.get(), before.revision.get() + 1, key())
            .is_err()
    );
    let other_scenes = scenes.capture_quality(7, true).unwrap().key;
    assert!(
        docs.recent_subtitles(before.generation.get(), before.revision.get(), other_scenes)
            .unwrap()
            .unwrap()
            .summary
            .is_none()
    );
    docs.apply(
        Action::Edit {
            cue_id: 1,
            start_ms: 1000,
            end_ms: 2000,
            text: "Unsaved".into(),
        },
        before.generation.get(),
        before.revision.get(),
    )
    .unwrap();
    let current = request(&docs);
    assert!(
        docs.recent_subtitles(current.generation.get(), current.revision.get(), key())
            .unwrap()
            .unwrap()
            .summary
            .is_none()
    );
}

#[test]
fn index_reports_missing_primary_and_related_cue_ids() {
    let docs = fixture();
    ready(&docs);
    let mut store = docs.0.lock().unwrap();
    let report = &mut store.checks.report.as_mut().unwrap().data;
    for related in [true, false] {
        let finding = &mut report.findings[0];
        if related {
            finding.related_cue_id = Some("9000".into());
        } else {
            finding.cue_id = "9000".into();
        }
        let error = ReportIndex::build(report, || Ok(()))
            .err()
            .expect("missing ID");
        assert!(error.contains("9000"));
        assert!(error.contains(if related { "related" } else { "primary" }));
    }
}

#[tokio::test]
async fn cancelled_worker_returns_its_cache_without_publishing_the_report() {
    let docs = fixture();
    ready(&docs);
    assert!(docs.0.lock().unwrap().checks.language_cache.is_some());
    let mut restart = request(&docs);
    restart.restart = true;
    let id = docs.quality_read(restart).unwrap().id.get();
    let completion = docs.quality_completion(id).unwrap().unwrap();
    // Cancel after the computation succeeds, before refresh can publish it.
    tokio::time::timeout(Duration::from_secs(5), completion.cancelled())
        .await
        .unwrap();
    docs.quality_cancel(id).unwrap();
    let mut store = docs.0.lock().unwrap();
    store.checks.refresh();
    assert!(store.checks.worker.is_none());
    assert!(store.checks.report.is_none());
    assert!(store.checks.language_cache.is_some());
}

#[test]
fn language_cache_survives_revision_but_clears_on_generation_or_policy_epoch() {
    let docs = fixture();
    ready(&docs);
    let mut store = docs.0.lock().unwrap();
    let mut key = store.quality_key().unwrap();
    key.revision = (key.revision.get() + 1).try_into().unwrap();
    store.checks.synchronize(key);
    assert!(store.checks.language_cache.is_some());
    key.generation = (key.generation.get() + 1).try_into().unwrap();
    store.checks.synchronize(key);
    assert!(store.checks.language_cache.is_none());
    store.checks.language_cache = Some(LanguageCache::default());
    key.epoch = (key.epoch.get() + 1).try_into().unwrap();
    store.checks.synchronize(key);
    assert!(store.checks.language_cache.is_none());
}

#[test]
fn draft_metrics_and_work_are_identical_with_language_on_or_off() {
    let docs = fixture();
    let store = docs.0.lock().unwrap();
    let doc = store.loaded.as_ref().unwrap().editor.document();
    let mut profile = engine::Profile::builtin("sw3-uk-vo-subset")
        .unwrap()
        .custom("draft-metrics".into(), "Draft metrics".into());
    let run = |profile: &engine::Profile| {
        let mut work = 0;
        let result = draft_metric(
            doc,
            profile.into(),
            &engine::scene::Scenes::NoMedia,
            3,
            |_, _| {
                work += 1;
                Ok(())
            },
        )
        .unwrap();
        (serde_json::to_vec(&result).unwrap(), work)
    };
    let off = run(&profile);
    profile.language_tools.spelling = true;
    profile.language_tools.capitalization = true;
    assert_eq!(off, run(&profile));
}

#[tokio::test]
async fn commit_recovers_completed_entries_from_an_interrupted_english_scan() {
    use std::{fmt::Write, sync::mpsc};
    let docs = Documents::default();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("unique.srt");
    let mut srt = String::new();
    for i in 1..=10_000 {
        writeln!(
            srt,
            "{i}\n00:00:00,000 --> 00:00:02,000\nDistinct subtitle number {i}!\n"
        )
        .unwrap();
    }
    std::fs::write(&file, srt).unwrap();
    docs.apply(
        Action::Open {
            path: file.to_str().unwrap().into(),
            discard: true,
        },
        0,
        0,
    )
    .unwrap();
    let mut policy = Policy::default();
    policy.profile = policy
        .profile
        .custom("cancel-sample".into(), "Cancel sample".into());
    policy.profile.language_tools.spelling = true;
    policy.profile.language_tools.capitalization = true;
    docs.initialize_quality(policy).unwrap();

    let (entered, halfway) = mpsc::channel();
    let (resume, wait) = mpsc::channel();
    let (key, done) = {
        let mut store = docs.0.lock().unwrap();
        let key = store.quality_key().unwrap();
        let document = store.loaded.as_ref().unwrap().editor.document().clone();
        let policy = store.quality.capture(subtitle_formats::Format::Srt);
        store
            .checks
            .launch_with(key, None, move |cancel, cache| {
                let mut visits = 0;
                let report = engine::evaluate_cached(
                    &document,
                    policy.evaluation(),
                    &engine::scene::Scenes::NoMedia,
                    cache,
                    |index, _| {
                        // Several token checkpoints into this cue, after the two linear passes.
                        if index == 5000 {
                            visits += 1;
                            if visits == 8 {
                                entered.send(()).unwrap();
                                wait.recv_timeout(Duration::from_secs(15)).unwrap();
                            }
                        }
                        if cancel.load(Ordering::Relaxed) {
                            Err(engine::Error::Cancelled)
                        } else {
                            Ok(())
                        }
                    },
                );
                assert_eq!(report.unwrap_err(), engine::Error::Cancelled);
                Err("cancelled".into())
            })
            .unwrap();
        (key, store.checks.worker.as_ref().unwrap().finished.clone())
    };
    halfway.recv_timeout(Duration::from_secs(15)).unwrap();
    docs.apply(
        Action::Edit {
            cue_id: 1,
            start_ms: 0,
            end_ms: 2000,
            text: "Edited cue!".into(),
        },
        key.generation.get(),
        key.revision.get(),
    )
    .unwrap();
    resume.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(5), done.cancelled())
        .await
        .unwrap();
    let mut store = docs.0.lock().unwrap();
    store.checks.refresh();
    assert!(store.checks.report.is_none());
    let mut cache = store
        .checks
        .language_cache
        .take()
        .expect("return the interrupted cache");
    let doc = store.loaded.as_ref().unwrap().editor.document();
    let policy = store.quality.capture(subtitle_formats::Format::Srt);
    let run = |cache: &mut LanguageCache| {
        let mut work = 0;
        let start = Instant::now();
        let report = engine::evaluate_cached(
            doc,
            policy.evaluation(),
            &engine::scene::Scenes::NoMedia,
            cache,
            |_, _| {
                work += 1;
                Ok(())
            },
        )
        .unwrap();
        (report, work, start.elapsed())
    };
    let (warm, reused, warm_time) = run(&mut cache);
    let (_, cold, cold_time) = run(&mut LanguageCache::default());
    let fresh = engine::evaluate(doc, policy.evaluation(), |_, _| Ok(())).unwrap();
    assert_eq!(
        serde_json::to_vec(&warm).unwrap(),
        serde_json::to_vec(&fresh).unwrap()
    );
    assert!(
        reused + 4999 * 3 <= cold,
        "completed first-half cues must skip token work"
    );
    eprintln!(
        "10k distinct English cues, commit during cue 5001: recovered {reused} checkpoints in {warm_time:?}, cold {cold} in {cold_time:?}"
    );
}

#[tokio::test]
async fn cancelled_cache_never_crosses_generation_or_quality_epoch() {
    for change_generation in [false, true] {
        let docs = fixture();
        let mut restart = request(&docs);
        restart.restart = true;
        let id = docs.quality_read(restart).unwrap().id.get();
        let done = docs.quality_completion(id).unwrap().unwrap();
        tokio::time::timeout(Duration::from_secs(5), done.cancelled())
            .await
            .unwrap();
        let mut store = docs.0.lock().unwrap();
        let mut key = store.quality_key().unwrap();
        let component = if change_generation {
            &mut key.generation
        } else {
            &mut key.epoch
        };
        *component = (component.get() + 1).try_into().unwrap();
        store.checks.synchronize(key);
        store.checks.refresh();
        assert!(store.checks.language_cache.is_none());
        assert!(store.checks.report.is_none());
    }
}

#[test]
fn start_review_invalidates_the_previous_check_worker() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("original.srt");
    fs::write(&source, "1\n00:00:01,000 --> 00:00:02,000\nOriginal\n").unwrap();
    let docs = Documents::default();
    docs.apply(
        Action::Open {
            path: source.to_string_lossy().into(),
            discard: false,
        },
        0,
        0,
    )
    .unwrap();
    let (release, gate) = std::sync::mpsc::channel();
    let review_key = docs.0.lock().unwrap().review_key().unwrap();
    let (token, old_key) = {
        let mut store = docs.0.lock().unwrap();
        let key = store.quality_key().unwrap();
        store
            .checks
            .launch_with(key, None, move |_, _| {
                let _ = gate.recv_timeout(Duration::from_secs(5));
                Err("obsolete result".into())
            })
            .unwrap();
        (store.checks.worker.as_ref().unwrap().cancel.clone(), key)
    };
    docs.review_action(review::ReviewAction::Start {
        key: review_key,
        path: dir.path().join("copy.srt").to_string_lossy().into(),
        discard: false,
    })
    .unwrap();
    release.send(()).unwrap();
    assert!(token.load(Ordering::Relaxed));
    let view = ready(&docs);
    assert_ne!(view.key.generation, old_key.generation);
    assert!(view.error.is_none());
}

fn draft_for_text(docs: &Documents, text: &str) -> View {
    let mut query = request(docs);
    query.draft = Some(DraftChange::Update {
        value: Draft {
            cue_id: 1.try_into().unwrap(),
            start_ms: 1000.try_into().unwrap(),
            end_ms: 3000.try_into().unwrap(),
            text: text.into(),
        },
    });
    docs.quality_read(query).unwrap();
    ready(docs)
}

#[test]
fn multiline_draft_matches_committed_metrics_for_each_format() {
    for (ext, source) in [
        ("srt", "1\n00:00:01,000 --> 00:00:03,000\nHi\n"),
        ("vtt", "WEBVTT\n\n00:01.000 --> 00:03.000\nHi\n"),
        ("ass", ass_tests::ASS),
        ("ssa", ass_tests::SSA),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(format!("working.{ext}"));
        fs::write(&path, source).unwrap();
        let docs = Documents::default();
        docs.apply(
            Action::Open {
                path: path.to_string_lossy().into(),
                discard: false,
            },
            0,
            0,
        )
        .unwrap();
        let draft = draft_for_text(&docs, "Hello\nWorld").draft.unwrap();
        docs.apply(
            Action::Edit {
                cue_id: 1,
                start_ms: 1000,
                end_ms: 3000,
                text: "Hello\nWorld".into(),
            },
            1,
            0,
        )
        .unwrap();
        assert_eq!(draft.data, ready(&docs).selected.unwrap().data, "{ext}");
    }
}

#[test]
fn rejected_format_draft_reports_the_commit_error() {
    for (ext, source, bad) in [
        (
            "srt",
            "1\n00:00:01,000 --> 00:00:03,000\nHi\n",
            "one\n\ntwo",
        ),
        (
            "vtt",
            "WEBVTT\n\n00:01.000 --> 00:03.000\nHi\n",
            "one --> two",
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(format!("working.{ext}"));
        fs::write(&path, source).unwrap();
        let docs = Documents::default();
        docs.apply(
            Action::Open {
                path: path.to_string_lossy().into(),
                discard: false,
            },
            0,
            0,
        )
        .unwrap();
        draft_for_text(&docs, "Valid draft");
        let view = draft_for_text(&docs, bad);
        let error = docs
            .apply(
                Action::Edit {
                    cue_id: 1,
                    start_ms: 1000,
                    end_ms: 3000,
                    text: bad.into(),
                },
                1,
                0,
            )
            .unwrap_err();
        assert_eq!(view.draft_error.as_deref(), Some(error.as_str()), "{ext}");
        assert!(view.draft.is_none());
    }
}
