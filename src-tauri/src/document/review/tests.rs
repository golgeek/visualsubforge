use super::*;
const ORIGINAL: &str = "1\n00:00:01,000 --> 00:00:03,000\nOriginal\n\n";
fn snapshot(d: &Documents) -> Snapshot {
    d.apply(super::super::Action::Status, 0, 0).unwrap()
}
fn action(d: &Documents, a: super::super::Action) -> Snapshot {
    let s = snapshot(d);
    d.apply(a, s.generation, s.revision).unwrap()
}
fn key(d: &Documents) -> ReviewKey {
    d.review_action(ReviewAction::Status).unwrap().key
}
fn fixture() -> (tempfile::TempDir, Documents, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let original = dir.path().join("original.srt");
    let working = dir.path().join("working.srt");
    fs::write(&original, ORIGINAL).unwrap();
    fs::write(&working, ORIGINAL.replace("Original", "Revised")).unwrap();
    let d = Documents::default();
    action(
        &d,
        super::super::Action::Open {
            path: working.to_string_lossy().into(),
            discard: false,
        },
    );
    (dir, d, original, working)
}
fn attach(d: &Documents, p: &Path) {
    d.review_action(ReviewAction::Attach {
        discard: false,
        key: key(d),
        path: p.to_string_lossy().into(),
    })
    .unwrap();
}
fn ready(d: &Documents) -> ReviewReply {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let r = d
            .review_action(ReviewAction::Query {
                presentation: false,
                expanded: vec![],
                key: key(d),
                offset: 0,
                limit: 100,
                filter: None,
            })
            .unwrap();
        if !r.working {
            assert!(r.error.is_none(), "{:?}", r.error);
            return r;
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}
#[test]
fn independent_reference_context_worker_protection_and_history() {
    let (_dir, d, original, _) = fixture();
    let before = snapshot(&d);
    attach(&d, &original);
    let after = snapshot(&d);
    assert_eq!(after.revision, before.revision);
    assert_eq!(after.dirty, before.dirty);
    assert_eq!(after.can_undo, before.can_undo);
    assert_eq!(ready(&d).statistics.unwrap().changes, 1);
    let protected = d.apply(
        super::super::Action::Save {
            path: Some(original.to_string_lossy().into()),
        },
        after.generation,
        after.revision,
    );
    assert!(protected.is_err());
    assert_eq!(fs::read_to_string(&original).unwrap(), ORIGINAL);
    let stale = key(&d);
    d.review_action(ReviewAction::Options {
        key: stale,
        options: engine::Options {
            ignore_tags: true,
            ..engine::Options::default()
        },
    })
    .unwrap();
    assert!(
        d.review_action(ReviewAction::Detach {
            key: stale,
            discard: true
        })
        .is_err()
    );
    d.review_action(ReviewAction::Detach {
        key: key(&d),
        discard: true,
    })
    .unwrap();
    assert_eq!(snapshot(&d).revision, before.revision);
}
#[test]
fn start_review_rejects_incompatible_copy_before_adoption() {
    let (dir, d, original, _) = fixture();
    attach(&d, &original);
    let before = snapshot(&d);
    let identity = key(&d);
    let copy = dir.path().join("working.reviewed.ass");
    let result = d.review_action(ReviewAction::Start {
        key: identity,
        path: copy.to_string_lossy().into(),
        discard: false,
    });
    assert!(result.is_err());
    assert!(!copy.exists());
    assert_eq!(snapshot(&d).path, before.path);
    assert_eq!(key(&d), identity);
    assert_eq!(
        snapshot(&d).review.unwrap().path,
        before.review.unwrap().path
    );
}

#[test]
fn start_copy_and_recovery_keep_original_without_disk() {
    let (dir, d, original, working) = fixture();
    attach(&d, &original);
    let video = dir.path().join("source.mkv");
    fs::write(&video, b"synthetic").unwrap();
    d.0.lock().unwrap().loaded_mut().unwrap().extracted_from = Some(extraction::ExtractedSource {
        media_path: video.clone(),
        stream_index: 1,
        codec: "subrip".into(),
        language: None,
        title: None,
        suggested_name: "source.srt".into(),
    });
    let copy = dir.path().join("working.reviewed.srt");
    d.review_action(ReviewAction::Start {
        key: key(&d),
        path: copy.to_string_lossy().into(),
        discard: false,
    })
    .unwrap();
    assert_eq!(fs::read(&copy).unwrap(), fs::read(&working).unwrap());
    assert_eq!(
        snapshot(&d).path,
        Some(copy.canonicalize().unwrap().to_string_lossy().into())
    );
    assert!(snapshot(&d).extracted_from.is_none());
    assert!(
        d.0.lock()
            .unwrap()
            .loaded_mut()
            .unwrap()
            .extracted_from
            .is_some()
    );
    for protected in [&original, &video] {
        let current = snapshot(&d);
        assert!(
            d.apply(
                super::super::Action::Save {
                    path: Some(protected.to_string_lossy().into())
                },
                current.generation,
                current.revision
            )
            .is_err()
        );
    }
    // Reattaching a base must not erase the source of the reviewed copy.
    attach(&d, &original);
    let current = snapshot(&d);
    assert!(
        d.apply(
            super::super::Action::Save {
                path: Some(working.to_string_lossy().into())
            },
            current.generation,
            current.revision
        )
        .is_err()
    );
    let store = d.0.lock().unwrap();
    let loaded = store.loaded.as_ref().unwrap();
    let stored = loaded.review.stored().unwrap().unwrap();
    drop(store);
    fs::remove_file(&original).unwrap();
    let restored = stored.restore().unwrap();
    assert_eq!(
        restored
            .reference
            .reference
            .unwrap()
            .reference
            .original_bytes(),
        ORIGINAL.as_bytes()
    );
}
#[test]
fn review_project_description_checks_expected_fingerprint() {
    let (dir, d, original, _) = fixture();
    attach(&d, &original);
    let description =
        d.0.lock()
            .unwrap()
            .loaded
            .as_ref()
            .unwrap()
            .review
            .project_description(dir.path())
            .unwrap()
            .unwrap();
    let bytes = serde_json::to_vec(&description).unwrap();
    let description: ReviewProjectContext = serde_json::from_slice(&bytes).unwrap();
    assert!(
        description
            .load(dir.path(), &translation::ReferenceOverride::default())
            .is_ok()
    );
    fs::write(&original, ORIGINAL.replace("Original", "Changed")).unwrap();
    assert!(
        description
            .load(dir.path(), &translation::ReferenceOverride::default())
            .is_err()
    );
    fs::remove_file(&original).unwrap();
    assert!(
        description
            .load(dir.path(), &translation::ReferenceOverride::default())
            .is_err()
    );
}

#[test]
fn every_revert_kind_and_filtered_batch_are_one_undo_step() {
    let variants = [
        (
            "1\n00:00:01,000 --> 00:00:04,000\none two three\n\n",
            "1\n00:00:01,000 --> 00:00:04,000\none new three\n\n",
        ),
        (
            "1\n00:00:01,000 --> 00:00:04,000\none two three\n\n",
            "1\n00:00:01,100 --> 00:00:04,100\none two three\n\n",
        ),
        (
            "1\n00:00:01,000 --> 00:00:04,000\none two three\n\n",
            "1\n00:00:01,100 --> 00:00:04,100\none new three\n\n",
        ),
        (
            "1\n00:00:01,000 --> 00:00:04,000\none two three\n\n",
            "1\n00:00:01,000 --> 00:00:02,500\none\n\n2\n00:00:02,500 --> 00:00:04,000\ntwo three\n\n",
        ),
        (
            "1\n00:00:01,000 --> 00:00:04,000\none two three\n\n",
            "1\n00:00:01,000 --> 00:00:02,000\none\n\n2\n00:00:02,000 --> 00:00:03,000\ntwo\n\n3\n00:00:03,000 --> 00:00:04,000\nthree\n\n",
        ),
        ("1\n00:00:01,000 --> 00:00:03,000\nremoved\n\n", ""),
    ];
    for (original, revised) in variants.into_iter().flat_map(|(a, b)| [(a, b), (b, a)]) {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("original.srt");
        let b = dir.path().join("working.srt");
        fs::write(&a, original).unwrap();
        fs::write(&b, revised).unwrap();
        let d = Documents::default();
        action(
            &d,
            super::super::Action::Open {
                path: b.to_string_lossy().into(),
                discard: false,
            },
        );
        attach(&d, &a);
        let before = snapshot(&d);
        let compared = ready(&d);
        assert!(compared.total > 0);
        d.review_action(ReviewAction::Revert {
            key: key(&d),
            change_key: None,
            source_index: None,
            filter: None,
        })
        .unwrap();
        let restored = ready(&d);
        assert_eq!(
            restored.statistics.unwrap().changes,
            0,
            "{original:?} / {revised:?}"
        );
        let after = snapshot(&d);
        assert_eq!(after.revision, before.revision + 1);
        assert!(after.can_undo);
        let undone = action(&d, super::super::Action::Undo);
        assert_eq!(
            undone
                .cues
                .iter()
                .map(|c| (&c.text, c.start_ms, c.end_ms))
                .collect::<Vec<_>>(),
            before
                .cues
                .iter()
                .map(|c| (&c.text, c.start_ms, c.end_ms))
                .collect::<Vec<_>>()
        );
        assert_eq!(ready(&d).total, compared.total);
    }
}

#[test]
fn export_refuses_stale_keys_and_escapes_current_content() {
    let (dir, d, original, _) = fixture();
    attach(&d, &original);
    let compared = ready(&d);
    let output = dir.path().join("report.html");
    d.review_action(ReviewAction::Export {
        author: String::new(),
        output: None,
        utc_offset_minutes: 0,
        key: key(&d),
        path: output.to_string_lossy().into(),
        include_unchanged: true,
    })
    .unwrap();
    assert!(
        fs::read_to_string(&output)
            .unwrap()
            .contains(&compared.rows[0].key)
    );
    let old = key(&d);
    d.review_action(ReviewAction::Detach {
        key: old,
        discard: true,
    })
    .unwrap();
    assert!(
        d.review_action(ReviewAction::Export {
            author: String::new(),
            output: None,
            utc_offset_minutes: 0,
            key: old,
            path: dir.path().join("stale.html").to_string_lossy().into(),
            include_unchanged: false
        })
        .is_err()
    );
    assert!(!dir.path().join("stale.html").exists());
}

#[test]
fn recovery_record_reopens_review_after_original_is_deleted() {
    let (dir, d, original, working) = fixture();
    attach(&d, &original);
    let mut record = crate::recovery_store::Record::new_document(
        subtitle_formats::Format::Srt,
        Some(working.to_string_lossy().into()),
        fs::read_to_string(&working).unwrap(),
        fs::read_to_string(&working).unwrap(),
        1,
        0,
    )
    .unwrap();
    record.set_review(
        d.0.lock()
            .unwrap()
            .loaded
            .as_ref()
            .unwrap()
            .review
            .stored()
            .unwrap(),
    );
    assert_eq!(serde_json::to_value(&record).unwrap()["version"], 7);
    let recovery = dir.path().join("recovery");
    let session = crate::recovery_store::Session::create(&recovery).unwrap();
    session.prepare(&record).unwrap().commit().unwrap();
    drop(session);
    fs::remove_file(original).unwrap();
    let entries = crate::recovery_store::list(&recovery).unwrap();
    let mut restored = Store {
        recovery_root: Some(recovery),
        ..Store::default()
    };
    restored
        .apply(
            super::super::Action::Recover {
                id: entries[0].id.clone(),
                discard: true,
            },
            0,
            0,
        )
        .unwrap();
    let context = &restored.loaded.as_ref().unwrap().review;
    assert_eq!(
        context
            .reference
            .reference
            .as_ref()
            .unwrap()
            .reference
            .original_bytes(),
        ORIGINAL.as_bytes()
    );
    assert!(restored.snapshot().review.unwrap().path.is_some());
    let d = Documents::default();
    *d.0.lock().unwrap() = restored;
    let capture = || {
        d.0.lock()
            .unwrap()
            .loaded
            .as_ref()
            .unwrap()
            .review
            .stored()
            .unwrap()
    };
    record.set_review(capture());
    assert_eq!(serde_json::to_value(&record).unwrap()["version"], 7);
    let row = ready(&d).rows.remove(0);
    d.review_action(ReviewAction::Comment {
        key: key(&d),
        author: "Ana".into(),
        edit: CommentEdit::Post {
            change_key: row.key,
            source_index: row.original[0].source_index,
            text: "Recovered draft".into(),
        },
    })
    .unwrap();
    record.set_review(capture());
    assert_eq!(serde_json::to_value(&record).unwrap()["version"], 8);
    let session = crate::recovery_store::Session::create(dir.path()).unwrap();
    session.prepare(&record).unwrap().commit().unwrap();
}

#[test]
fn style_and_actor_revert_restore_metadata_and_undo() {
    for name in ["style", "actor"] {
        let dir = tempfile::tempdir().unwrap();
        let base = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/fixtures/comparison")
            .join(name);
        let original = dir.path().join("original.ass");
        let revised = dir.path().join("revised.ass");
        fs::copy(base.join("original.ass"), &original).unwrap();
        fs::copy(base.join("revised.ass"), &revised).unwrap();
        let d = Documents::default();
        action(
            &d,
            super::super::Action::Open {
                path: revised.to_string_lossy().into(),
                discard: false,
            },
        );
        attach(&d, &original);
        let before = ready(&d);
        assert_eq!(before.rows[0].kind, engine::Kind::Style);
        d.review_action(ReviewAction::Revert {
            key: key(&d),
            change_key: Some(before.rows[0].key.clone()),
            source_index: Some(0),
            filter: None,
        })
        .unwrap();
        assert_eq!(ready(&d).total, 0);
        action(&d, super::super::Action::Undo);
        assert_eq!(ready(&d).rows, before.rows);
    }
}

#[test]
fn filtered_batch_and_duplicate_single_revert_are_atomic() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("original.srt");
    let b = dir.path().join("revised.srt");
    let original = "1\n00:00:01,000 --> 00:00:03,000\nold\n\n2\n00:00:01,000 --> 00:00:03,000\nold\n\n3\n00:00:05,000 --> 00:00:07,000\ntiming\n\n";
    fs::write(&a, original).unwrap();
    fs::write(
        &b,
        original
            .replace("old", "new")
            .replace("00:00:05,000", "00:00:05,100"),
    )
    .unwrap();
    let d = Documents::default();
    action(
        &d,
        super::super::Action::Open {
            path: b.to_string_lossy().into(),
            discard: false,
        },
    );
    attach(&d, &a);
    let before = ready(&d);
    assert_eq!(before.total, 3);
    assert_eq!(before.rows[0].key, before.rows[1].key);
    let next = d
        .review_action(ReviewAction::Navigate {
            expanded: vec![],
            key: key(&d),
            from: Some(before.rows[1].key.clone()),
            cue_id: Some(before.rows[1].revised[0].cue_id.try_into().unwrap()),
            forward: true,
            filter: None,
        })
        .unwrap();
    assert_eq!(next.position, Some(3));
    assert_eq!(next.rows[0].kind, engine::Kind::Timing);
    d.review_action(ReviewAction::Revert {
        key: key(&d),
        change_key: Some(before.rows[1].key.clone()),
        source_index: Some(1),
        filter: None,
    })
    .unwrap();
    assert_eq!(snapshot(&d).cues[0].text, "new");
    assert_eq!(snapshot(&d).cues[1].text, "old");
    action(&d, super::super::Action::Undo);
    ready(&d);
    d.review_action(ReviewAction::Revert {
        key: key(&d),
        change_key: None,
        source_index: None,
        filter: Some(engine::Kind::Text),
    })
    .unwrap();
    assert_eq!(ready(&d).total, 1);
    action(&d, super::super::Action::Undo);
    assert_eq!(ready(&d).total, 3);
}

#[test]
fn timing_and_text_batches_only_revert_matching_kinds() {
    use engine::Kind::*;
    let cases = [
        ("text", Text),
        ("over-tolerance", Timing),
        ("text-timing", TextAndTiming),
        ("insert", Added),
        ("delete", Removed),
        ("split-2", Split),
        ("merge-2", Merged),
        ("style", Style),
    ];
    for (name, kind) in cases {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/fixtures/comparison")
            .join(name);
        let ext = if kind == Style { "ass" } else { "srt" };
        let d = Documents::default();
        action(
            &d,
            super::super::Action::Open {
                path: dir.join(format!("revised.{ext}")).to_string_lossy().into(),
                discard: false,
            },
        );
        attach(&d, &dir.join(format!("original.{ext}")));
        for filter in [Timing, Text] {
            let before = ready(&d);
            assert!(before.rows.iter().any(|r| r.kind == kind));
            let applied = d.review_action(ReviewAction::Revert {
                key: key(&d),
                change_key: None,
                source_index: None,
                filter: Some(filter),
            });
            if kind == filter || kind == TextAndTiming {
                applied.unwrap();
                assert_eq!(ready(&d).matched, 0);
                action(&d, super::super::Action::Undo);
            } else {
                assert!(applied.is_err());
            }
            assert_eq!(ready(&d).rows, before.rows);
        }
    }
}

#[test]
fn shift_block_is_one_stop_and_one_undo_step() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/comparison/block");
    let d = Documents::default();
    action(
        &d,
        super::super::Action::Open {
            path: dir.join("revised.srt").to_string_lossy().into(),
            discard: false,
        },
    );
    attach(&d, &dir.join("original.srt"));
    let before = ready(&d);
    assert_eq!((before.total, before.matched), (1, 5));
    let expanded = d
        .review_action(ReviewAction::Query {
            presentation: false,
            key: key(&d),
            expanded: vec![0],
            offset: 2,
            limit: 2,
            filter: Some(engine::Kind::Timing),
        })
        .unwrap();
    assert_eq!(expanded.total, 5);
    assert_eq!(expanded.rows[0].revised[0].source_index, 2);
    let cues = serde_json::to_value(snapshot(&d).cues).unwrap();
    d.review_action(ReviewAction::RevertBlock {
        key: key(&d),
        first: 0,
    })
    .unwrap();
    assert_eq!(ready(&d).matched, 0);
    let undone = action(&d, super::super::Action::Undo);
    assert_eq!(serde_json::to_value(undone.cues).unwrap(), cues);
}
#[test]
fn discussion_is_independent_of_editor_and_survives_project_and_recovery() {
    let (dir, d, original, _) = fixture();
    attach(&d, &original);
    let row = ready(&d).rows.remove(0);
    let allocation = || {
        d.0.lock()
            .unwrap()
            .review
            .job
            .as_ref()
            .unwrap()
            .result
            .as_ref()
            .unwrap()
            .changes
            .as_ptr() as usize
    };
    let initial_comparison = allocation();
    let started = now_ms().unwrap();
    let before = snapshot(&d);
    let post = |edit| {
        d.review_action(ReviewAction::Comment {
            key: key(&d),
            author: "Ana".into(),
            edit,
        })
    };
    post(CommentEdit::Post {
        change_key: row.key,
        source_index: row.original[0].source_index,
        text: "Check".into(),
    })
    .unwrap();
    let after = snapshot(&d);
    assert_eq!(
        (after.revision, after.dirty, after.can_undo),
        (before.revision, before.dirty, before.can_undo)
    );
    assert_eq!(after.cues[0].text, before.cues[0].text);
    assert!(after.review.unwrap().dirty);
    let t = ready(&d)
        .discussion
        .unwrap()
        .rows
        .into_values()
        .next()
        .unwrap()
        .remove(0);
    assert!((started..=now_ms().unwrap()).contains(&t.messages[0].at_ms));
    assert_eq!(allocation(), initial_comparison);
    post(CommentEdit::Reply {
        thread_id: t.id.clone(),
        event: review_package::Event::Resolve,
        text: None,
    })
    .unwrap();
    assert_eq!(allocation(), initial_comparison);
    assert!(
        d.0.lock()
            .unwrap()
            .review
            .job
            .as_ref()
            .unwrap()
            .threads
            .is_some()
    );
    let restored = {
        let mut s = d.0.lock().unwrap();
        let c = &mut s.loaded_mut().unwrap().review;
        c.discussion.exported.insert(t.first_message_id.clone());
        let recovery = c.stored().unwrap().unwrap().restore().unwrap();
        let project = c.project_description(dir.path()).unwrap().unwrap();
        let r: ReviewProjectContext =
            serde_json::from_slice(&serde_json::to_vec(&project).unwrap()).unwrap();
        let reopened = r.load(dir.path(), &Default::default()).ok().unwrap();
        assert!(reopened.discussion == c.discussion && recovery.discussion == c.discussion);
        reopened.discussion
    };
    assert!(
        post(CommentEdit::Delete {
            thread_id: t.id.clone(),
            message_id: t.first_message_id
        })
        .is_err()
    );
    let event = restored.threads[0]
        .messages
        .iter()
        .find(|m| m.event == review_package::Event::Resolve)
        .unwrap();
    post(CommentEdit::Delete {
        thread_id: t.id,
        message_id: event.id.clone(),
    })
    .unwrap();
    assert_eq!(ready(&d).discussion.unwrap().open, 1);
}

#[test]
fn package_exchange_is_guarded_and_one_edit_restores_previous_cues() {
    use exchange::{ImportCase, ImportChoice, OutputKind};
    let (dir, reviewer, original, working) = fixture();
    attach(&reviewer, &original);
    let row = ready(&reviewer).rows.remove(0);
    reviewer
        .review_action(ReviewAction::Comment {
            key: key(&reviewer),
            author: "Ana".into(),
            edit: CommentEdit::Post {
                change_key: row.key,
                source_index: 0,
                text: "Review".into(),
            },
        })
        .unwrap();
    let path = dir.path().join("round1.vsfreview");
    let publish = |d: &Documents, path: &Path, kind, author| {
        d.publish_review(key(d), path, kind, author, 0, false)
            .unwrap()
    };
    ready(&reviewer);
    let original_key = key(&reviewer);
    let readable = dir.path().join("readable.html");
    publish(&reviewer, &readable, OutputKind::Readable, "");
    assert_eq!(key(&reviewer), original_key);
    assert!(vsf_application::review_file::read(&readable).is_err());
    let page = ready(&reviewer).discussion.unwrap();
    assert!(page.exported.is_empty() && !page.rows.is_empty());
    assert!(snapshot(&reviewer).review.unwrap().round.is_none());
    publish(&reviewer, &path, OutputKind::Package, "Ana");
    let import = |d: &Documents, path: &Path, case, choice| {
        let info = d
            .import_review(key(d), path, None)
            .unwrap()
            .inspection
            .unwrap();
        assert_eq!(info.case, case);
        d.import_review(key(d), path, Some((&info.digest, choice, true)))
            .unwrap()
    };
    let author = Documents::default();
    import(&author, &path, ImportCase::New, ImportChoice::Open);
    let imported_key = key(&author);
    attach(&author, &path);
    assert_eq!(key(&author), imported_key);
    let store = author.0.lock().unwrap();
    let context = &store.loaded.as_ref().unwrap().review;
    assert_eq!(
        context.package.as_ref().unwrap().path,
        path.canonicalize().unwrap().to_string_lossy()
    );
    assert_eq!(context.discussion.round, 1);
    assert_eq!(context.discussion.exported.len(), 1);
    assert_eq!(
        context.discussion.threads[0].messages[0].text.as_deref(),
        Some("Review")
    );
    drop(store);
    let s = snapshot(&author);
    assert!(s.path.is_none() && s.dirty && !s.can_undo);
    assert_eq!(
        s.suggested_save_path.unwrap().file_name().unwrap(),
        "working.srt"
    );
    import(&author, &path, ImportCase::Same, ImportChoice::Merge);
    let t = ready(&author)
        .discussion
        .unwrap()
        .rows
        .into_values()
        .next()
        .unwrap()
        .remove(0);
    assert!(
        author
            .review_action(ReviewAction::Comment {
                key: key(&author),
                author: "Ana".into(),
                edit: CommentEdit::Delete {
                    thread_id: t.id.clone(),
                    message_id: t.first_message_id.clone()
                }
            })
            .is_err()
    );
    author
        .review_action(ReviewAction::Comment {
            key: key(&author),
            author: "Sam".into(),
            edit: CommentEdit::Reply {
                thread_id: t.id,
                event: review_package::Event::Resolve,
                text: None,
            },
        })
        .unwrap();
    ready(&author);
    author
        .review_action(ReviewAction::Revert {
            key: key(&author),
            change_key: None,
            source_index: None,
            filter: None,
        })
        .unwrap();
    let detached = ready(&author).discussion.unwrap().detached;
    assert!(detached.len() == 1 && detached[0].resolved());
    let out = dir.path().join("round2.html");
    publish(&author, &out, OutputKind::Report, "Sam");
    let content = vsf_application::review_file::read(&out).unwrap();
    assert_eq!((content.round, content.exported_by.as_str()), (2, "Sam"));
    let before = snapshot(&reviewer);
    import(&reviewer, &out, ImportCase::Different, ImportChoice::Keep);
    assert_eq!(snapshot(&reviewer).revision, before.revision);
    let r = import(
        &reviewer,
        &out,
        ImportCase::Different,
        ImportChoice::UseTheirs,
    );
    assert_eq!(r.document.unwrap().cues[0].text, "Original");
    let u = action(&reviewer, super::super::Action::Undo);
    assert_eq!(
        (u.cues[0].text.as_str(), u.path.as_deref()),
        ("Revised", working.canonicalize().unwrap().to_str())
    );
    assert!(!u.can_undo);
    let (stored, project) = {
        let mut s = author.0.lock().unwrap();
        let c = &s.loaded_mut().unwrap().review;
        (
            c.stored().unwrap().unwrap(),
            c.project_description(dir.path()).unwrap().unwrap(),
        )
    };
    let moved = dir.path().join("moved.vsfreview");
    fs::rename(&path, &moved).unwrap();
    assert!(project.load(dir.path(), &Default::default()).is_err());
    let relocated = serde_json::from_value(serde_json::json!({"path":moved})).unwrap();
    let restored = project.load(dir.path(), &relocated).ok().unwrap();
    assert!(restored.discussion == stored.restore().unwrap().discussion);
    assert_eq!(
        restored.package.unwrap().path,
        moved.canonicalize().unwrap().to_string_lossy()
    );
    assert!(
        author
            .import_review(
                key(&author),
                &moved,
                Some(("changed", ImportChoice::Keep, true))
            )
            .is_err()
    );
}
#[test]
fn untouched_package_bytes_survive_untitled_import_with_bom_and_noncanonical_spacing() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("external.vsfreview");
    let raw = ORIGINAL.replace(" --> ", "  -->  ");
    let mut bytes = vec![255, 254];
    bytes.extend(raw.encode_utf16().flat_map(u16::to_le_bytes));
    let subtitle =
        review_package::Subtitle::capture("sample.srt", subtitle_formats::Format::Srt, &bytes);
    let p = review_package::Package {
        format: "visualsubforge-review".into(),
        version: 1,
        round: 1,
        exported_by: "Ana".into(),
        exported_at_ms: 10,
        original: subtitle.clone(),
        revised: subtitle,
        options: Default::default(),
        threads: vec![],
    };
    fs::write(&source, p.write().unwrap()).unwrap();
    let d = Documents::default();
    let inspect = || {
        d.import_review(key(&d), &source, None)
            .unwrap()
            .inspection
            .unwrap()
    };
    let first = inspect();
    d.import_review(
        key(&d),
        &source,
        Some((&first.digest, exchange::ImportChoice::Open, true)),
    )
    .unwrap();
    assert_eq!(inspect().case, exchange::ImportCase::Same);
    let copy = dir.path().join("round2.vsfreview");
    d.publish_review(
        key(&d),
        &copy,
        exchange::OutputKind::Package,
        "Sam",
        0,
        false,
    )
    .unwrap();
    let exported = vsf_application::review_file::read(&copy).unwrap();
    assert_eq!(exported.revised.decode().unwrap(), bytes);
    let restored =
        d.0.lock()
            .unwrap()
            .loaded_mut()
            .unwrap()
            .review
            .stored()
            .unwrap()
            .unwrap()
            .restore()
            .unwrap();
    assert_eq!(
        restored
            .reference
            .reference
            .unwrap()
            .reference
            .original_bytes(),
        bytes
    );
}

#[test]
fn unchanged_projection_does_not_crowd_out_later_removed_cues() {
    let (_dir, d, original, working) = fixture();
    let cue = |i: u32| {
        format!(
            "{}\n00:{:02}:{:02},000 --> 00:{:02}:{:02},500\nLine {i}\n\n",
            i + 1,
            i / 60,
            i % 60,
            i / 60,
            i % 60
        )
    };
    let identical: String = (0..200).map(cue).collect();
    fs::write(&working, &identical).unwrap();
    fs::write(&original, identical + &cue(200)).unwrap();
    let doc = action(
        &d,
        super::super::Action::Open {
            path: working.to_string_lossy().into(),
            discard: true,
        },
    );
    attach(&d, &original);
    ready(&d);
    let page = d
        .review_action(ReviewAction::Projection {
            key: key(&d),
            ids: doc.cues.iter().map(|c| c.id.try_into().unwrap()).collect(),
            start_ms: 0u64.try_into().unwrap(),
            end_ms: 201000u64.try_into().unwrap(),
        })
        .unwrap();
    assert_eq!(page.rows.len(), 201);
    assert_eq!(page.rows.last().unwrap().kind, engine::Kind::Removed);
}

#[test]
fn projections_fetch_consecutive_removals_by_identity_and_number_survivors_from_revised() {
    let (_dir, d, original, working) = fixture();
    fs::write(&original, "1\n00:00:00,000 --> 00:00:01,000\nGone one\n\n2\n00:00:02,000 --> 00:00:03,000\nGone two\n\n3\n00:01:00,000 --> 00:01:02,000\nThe survivor\n").unwrap();
    fs::write(
        &working,
        "1\n00:01:00,000 --> 00:01:02,000\nThe survivor!\n",
    )
    .unwrap();
    let doc = action(
        &d,
        super::super::Action::Open {
            path: working.to_string_lossy().into(),
            discard: true,
        },
    );
    attach(&d, &original);
    ready(&d);
    let key = key(&d);
    let result = d
        .review_action(ReviewAction::Query {
            key,
            presentation: true,
            expanded: vec![],
            offset: 0,
            limit: 1,
            filter: None,
        })
        .unwrap();
    let summary = serde_json::to_value(result.presentation.unwrap()).unwrap();
    let rows = summary["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 3);
    for (i, row) in rows[..2].iter().enumerate() {
        assert_eq!(row["nextCue"], doc.cues[0].id);
        assert_eq!(row["sourceIndex"], i);
        assert!(row.get("original").is_none());
    }
    assert_eq!(rows[2]["sourceIndex"], 0);
    let project = |indices| d.review_action(ReviewAction::ProjectRemovals { key, indices });
    let result = project(vec![0, 1]).unwrap();
    assert_eq!(result.rows[0].original[0].text, "Gone one");
    assert_eq!(result.rows[1].original[0].text, "Gone two");
    assert!(project(vec![2]).is_err());
    assert!(project(vec![0; 201]).is_err());
}

#[test]
fn dense_table_filter_uses_identities_and_fetches_only_visible_diffs() {
    let (_dir, d, original, working) = fixture();
    let make = |edit: &str| {
        let mut text = String::new();
        use std::fmt::Write;
        for i in 0..10_000u64 {
            let start = Milliseconds::new(i * 250).unwrap();
            let end = Milliseconds::new(i * 250 + 500).unwrap();
            let suffix = if i % 10 == 0 { edit } else { "" };
            writeln!(
                text,
                "{}\n{} --> {}\nLine {i}{suffix}\n",
                i + 1,
                srt::format_time(start),
                srt::format_time(end)
            )
            .unwrap();
        }
        text
    };
    fs::write(&original, make(" original")).unwrap();
    fs::write(&working, make(" revised")).unwrap();
    action(
        &d,
        super::super::Action::Open {
            path: working.to_string_lossy().into(),
            discard: true,
        },
    );
    attach(&d, &original);
    ready(&d);
    let key = key(&d);
    let page = d
        .review_action(ReviewAction::Query {
            key,
            presentation: true,
            expanded: vec![],
            offset: 0,
            limit: 1,
            filter: None,
        })
        .unwrap();
    assert_eq!(page.rows.len(), 1);
    let index = serde_json::to_value(page.presentation.unwrap()).unwrap();
    let rows = index["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1000);
    assert!(
        rows.iter()
            .all(|r| r.get("words").is_none() && r.get("text").is_none())
    );
    let ids = rows
        .iter()
        .skip(500)
        .take(25)
        .map(|r| r["revised"][0].as_u64().unwrap().try_into().unwrap())
        .collect();
    let window = d
        .review_action(ReviewAction::Projection {
            key,
            ids,
            start_ms: 0u64.try_into().unwrap(),
            end_ms: 0u64.try_into().unwrap(),
        })
        .unwrap();
    assert_eq!(window.rows.len(), 25);
    assert!(window.presentation.is_none());
}

#[test]
fn reattach_preserves_comment_options_and_exported_round() {
    let (dir, d, original, _) = fixture();
    attach(&d, &original);
    d.review_action(ReviewAction::Options {
        key: key(&d),
        options: engine::Options {
            ignore_tags: true,
            ..engine::Options::default()
        },
    })
    .unwrap();
    let row = ready(&d).rows.remove(0);
    d.review_action(ReviewAction::Comment {
        key: key(&d),
        author: "Ana".into(),
        edit: CommentEdit::Post {
            change_key: row.key,
            source_index: row.original[0].source_index,
            text: "Check timing".into(),
        },
    })
    .unwrap();
    ready(&d);
    d.publish_review(
        key(&d),
        &dir.path().join("round.vsfreview"),
        OutputKind::Package,
        "Ana",
        0,
        false,
    )
    .unwrap();
    let before = key(&d);
    let exported = ready(&d).discussion.unwrap().exported;
    assert_eq!(exported.len(), 1);
    attach(&d, &dir.path().join(".").join("original.srt"));
    let page = ready(&d);
    let discussion = page.discussion.unwrap();
    assert_eq!(discussion.open, 1);
    let threads: Vec<_> = discussion.rows.values().flatten().collect();
    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0].messages[0].text.as_deref(), Some("Check timing"));
    assert_eq!(discussion.exported, exported);
    let summary = page.context.unwrap();
    assert_eq!(summary.round, Some(1));
    assert!(summary.options.ignore_tags);
    assert_eq!(page.key, before);
}

#[test]
fn replacing_or_ending_review_requires_current_discard_consent() {
    let (dir, d, original, _) = fixture();
    attach(&d, &original);
    let before_post = key(&d);
    let row = ready(&d).rows.remove(0);
    d.review_action(ReviewAction::Comment {
        key: before_post,
        author: "Ana".into(),
        edit: CommentEdit::Post {
            change_key: row.key,
            source_index: 0,
            text: "Keep me".into(),
        },
    })
    .unwrap();
    // A saved project still holds discussion even after its dirty flag clears.
    d.0.lock()
        .unwrap()
        .loaded_mut()
        .unwrap()
        .review
        .reference
        .dirty = false;
    let current = key(&d);
    let other = dir.path().join("other.srt");
    fs::write(&other, ORIGINAL).unwrap();
    let replace = |path: &Path, key, discard| {
        d.review_action(ReviewAction::Attach {
            key,
            discard,
            path: path.to_string_lossy().into(),
        })
    };
    assert!(replace(&other, current, false).is_err());
    fs::write(&original, ORIGINAL.replace("Original", "Updated")).unwrap();
    assert!(replace(&original, current, false).is_err());
    assert!(replace(&other, before_post, true).is_err());
    assert!(
        d.review_action(ReviewAction::Detach {
            key: current,
            discard: false
        })
        .is_err()
    );
    assert_eq!(key(&d), current);
    assert_eq!(ready(&d).discussion.unwrap().open, 1);
    replace(&other, current, true).unwrap();
    assert_eq!(ready(&d).discussion.unwrap().open, 0);
    assert_eq!(
        snapshot(&d).review.unwrap().path.as_deref(),
        other.canonicalize().unwrap().to_str()
    );
    d.review_action(ReviewAction::Detach {
        key: key(&d),
        discard: true,
    })
    .unwrap();
    assert!(snapshot(&d).review.unwrap().path.is_none());
}

#[test]
fn cross_format_reverts_preserve_history_and_review_context() {
    for (working_ext, working, original_ext, original) in [
        ("ass", ass_tests::ASS, "srt", ORIGINAL),
        ("ssa", ass_tests::SSA, "ass", ass_tests::ASS),
        (
            "srt",
            ORIGINAL,
            "vtt",
            "WEBVTT\n\n00:01.000 --> 00:03.000\nOther\n",
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join(format!("working.{working_ext}"));
        let base = dir.path().join(format!("base.{original_ext}"));
        fs::write(&source, working).unwrap();
        fs::write(&base, original).unwrap();
        let d = Documents::default();
        action(
            &d,
            super::super::Action::Open {
                path: source.to_string_lossy().into(),
                discard: false,
            },
        );
        attach(&d, &base);
        ready(&d);
        let before = serde_json::to_value(snapshot(&d)).unwrap();
        let key = key(&d);
        for command in [
            ReviewAction::Revert {
                key,
                change_key: None,
                source_index: None,
                filter: None,
            },
            ReviewAction::RevertBlock { key, first: 0 },
        ] {
            let reason = d
                .review_action(command)
                .err()
                .expect("mixed formats must be rejected");
            assert!(reason.contains("same format"), "{reason}");
            assert_eq!(reason.contains("Export As"), original_ext != "vtt");
            assert_eq!(before["review"]["revertDisabledReason"], reason);
            assert_eq!(serde_json::to_value(snapshot(&d)).unwrap(), before);
        }
    }
}

#[test]
fn package_format_mismatch_prevents_use_theirs_before_mutation() {
    let (dir, d, original, _) = fixture();
    attach(&d, &original);
    let path = dir.path().join("package.vsfreview");
    let package = review_package::Package {
        format: "visualsubforge-review".into(),
        version: 1,
        round: 1,
        exported_by: "Ana".into(),
        exported_at_ms: 10,
        threads: vec![],
        options: Default::default(),
        original: review_package::Subtitle::capture(
            "original.srt",
            subtitle_formats::Format::Srt,
            ORIGINAL.as_bytes(),
        ),
        revised: review_package::Subtitle::capture(
            "working.ass",
            subtitle_formats::Format::Ass,
            ass_tests::ASS.as_bytes(),
        ),
    };
    fs::write(&path, package.write().unwrap()).unwrap();
    let info = d
        .import_review(key(&d), &path, None)
        .unwrap()
        .inspection
        .unwrap();
    assert_eq!(info.case, exchange::ImportCase::Different);
    let before = serde_json::to_value(snapshot(&d)).unwrap();
    let error = d
        .import_review(
            key(&d),
            &path,
            Some((&info.digest, exchange::ImportChoice::UseTheirs, true)),
        )
        .err()
        .expect("mixed formats must be rejected");
    assert!(
        error.contains("same format") && error.contains("Export As"),
        "{error}"
    );
    assert_eq!(
        serde_json::to_value(&info).unwrap()["useTheirsDisabledReason"],
        error
    );
    assert_eq!(serde_json::to_value(snapshot(&d)).unwrap(), before);
    assert!(
        d.import_review(
            key(&d),
            &path,
            Some((&info.digest, exchange::ImportChoice::Open, false))
        )
        .is_err()
    );
    let opened = d
        .import_review(
            key(&d),
            &path,
            Some((&info.digest, exchange::ImportChoice::Open, true)),
        )
        .unwrap()
        .document
        .unwrap();
    assert!(opened.path.is_none() && !opened.can_undo);
    assert_eq!(opened.cues[0].text, "Red title");
    assert_eq!(
        d.0.lock()
            .unwrap()
            .loaded
            .as_ref()
            .unwrap()
            .source()
            .format(),
        subtitle_formats::Format::Ass
    );
}
