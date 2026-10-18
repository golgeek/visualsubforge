use super::*;
const TTF: &[u8] = include_bytes!("../../../../../tests/fixtures/subtitles/fixture-font.ttf");
const OTF: &[u8] = include_bytes!("../../../../../tests/fixtures/subtitles/fixture-font-cff.otf");

fn fixture(extension: &str) -> (tempfile::TempDir, Documents, String) {
    let root = tempfile::tempdir().unwrap();
    let base = if extension == "ass" {
        crate::document::ass_tests::ASS
    } else {
        crate::document::ass_tests::SSA
    };
    let original = format!("\u{feff}{base}").replace('\n', "\r\n");
    let path = root.path().join(format!("source.{extension}"));
    fs::write(&path, &original).unwrap();
    fs::write(root.path().join("Fixture.ttf"), TTF).unwrap();
    fs::write(root.path().join("Changed.otf"), OTF).unwrap();
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
    (root, docs, original)
}
fn start(docs: &Documents, operation: FontOperation) -> u64 {
    let snap = docs.apply(Action::Status, 0, 0).unwrap();
    docs.font_start(snap.generation, snap.revision, operation)
        .unwrap()
}
fn ready(docs: &Documents, id: u64) -> FontReview {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let view = docs.font_view(id).unwrap();
        if !matches!(view.phase, Phase::Preparing | Phase::Cancelling) {
            return view;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn bytes(docs: &Documents) -> String {
    let store = docs.0.lock().unwrap();
    let loaded = store.loaded.as_ref().unwrap();
    loaded.source().serialize(loaded.editor.document()).unwrap()
}
fn action(docs: &Documents, action: Action) -> Snapshot {
    let snap = docs.apply(Action::Status, 0, 0).unwrap();
    docs.apply(action, snap.generation, snap.revision).unwrap()
}
fn add(root: &Path) -> FontOperation {
    FontOperation::Add {
        path: root.join("Fixture.ttf").to_string_lossy().into_owned(),
    }
}

#[test]
fn fonts_are_reviewed_and_share_exact_save_history_and_recovery_in_both_formats() {
    for extension in ["ass", "ssa"] {
        let (root, docs, original) = fixture(extension);
        let id = start(&docs, FontOperation::Inspect);
        assert_eq!(ready(&docs, id).inventory.unwrap().total, 0);
        assert!(docs.font_accept(id).is_err());
        assert_eq!(ready(&docs, id).inventory.unwrap().total, 0);
        let id = start(&docs, add(root.path()));
        let view = ready(&docs, id);
        assert_eq!(view.phase, Phase::Reviewing);
        let entry = view.summary.unwrap().after.unwrap();
        assert_eq!(entry.name, "Fixture.ttf");
        assert_eq!(entry.decoded_bytes, Some(TTF.len() as u32));
        assert_eq!(bytes(&docs), original);
        // Acceptance owns the captured bytes, not a mutable file reference.
        fs::write(root.path().join("Fixture.ttf"), b"changed after review").unwrap();
        let accepted = docs.font_accept(id).unwrap();
        assert_eq!(accepted.revision, 1);
        assert!(accepted.dirty && accepted.can_undo);
        let added = bytes(&docs);
        assert!(added.starts_with(&original));
        assert!(docs.font_accept(id).is_err());
        action(&docs, Action::Undo);
        assert_eq!(bytes(&docs), original);
        action(&docs, Action::Redo);
        assert_eq!(bytes(&docs), added);
        let replace = FontOperation::Replace {
            id: entry.id,
            path: root
                .path()
                .join("Changed.otf")
                .to_string_lossy()
                .into_owned(),
        };
        let id = start(&docs, replace.clone());
        assert_eq!(
            ready(&docs, id).summary.unwrap().before.unwrap().sha256,
            entry.sha256
        );
        docs.font_accept(id).unwrap();
        let changed = bytes(&docs);
        assert!(changed.contains("fontname: Changed.otf\r\n"));
        let id = start(&docs, FontOperation::Remove { id: entry.id });
        ready(&docs, id);
        docs.font_accept(id).unwrap();
        assert_eq!(bytes(&docs), original);
        action(&docs, Action::Undo);
        assert_eq!(bytes(&docs), changed);
        let noop = start(&docs, replace);
        assert!(!ready(&docs, noop).changed);
        let same = docs.font_accept(noop).unwrap();
        assert!(same.can_redo);
        assert_eq!(same.revision, 6);
        assert!(
            docs.apply(
                Action::Save {
                    path: Some(
                        root.path()
                            .join("missing/copy.ass")
                            .to_string_lossy()
                            .into_owned()
                    )
                },
                same.generation,
                same.revision
            )
            .is_err()
        );
        assert_eq!(bytes(&docs), changed);
        assert_eq!(
            fs::read_to_string(root.path().join(format!("source.{extension}"))).unwrap(),
            original
        );
        let record = crate::recovery_store::Record::new_document(
            docs.0
                .lock()
                .unwrap()
                .loaded
                .as_ref()
                .unwrap()
                .source()
                .format(),
            None,
            original.clone(),
            changed.clone(),
            same.generation,
            same.revision,
        )
        .unwrap();
        let recovery_root = root.path().join("recovery");
        let session = crate::recovery_store::Session::create(&recovery_root).unwrap();
        session.prepare(&record).unwrap().commit().unwrap();
        drop(session);
        let entries = crate::recovery_store::list(&recovery_root).unwrap();
        let mut recovered = Store {
            recovery_root: Some(recovery_root),
            ..Store::default()
        };
        recovered
            .apply(
                Action::Recover {
                    id: entries[0].id.clone(),
                    discard: true,
                },
                0,
                0,
            )
            .unwrap();
        let loaded = recovered.loaded.as_ref().unwrap();
        assert_eq!(
            loaded.source().serialize(loaded.editor.document()).unwrap(),
            changed
        );
        assert!(recovered.snapshot().requires_save_as);
        let copy = root.path().join(format!("copy.{extension}"));
        assert!(
            !action(
                &docs,
                Action::Save {
                    path: Some(copy.to_string_lossy().into_owned())
                }
            )
            .dirty
        );
        assert_eq!(fs::read_to_string(&copy).unwrap(), changed);
        action(
            &docs,
            Action::Open {
                path: copy.to_string_lossy().into_owned(),
                discard: false,
            },
        );
        let inspect = start(&docs, FontOperation::Inspect);
        assert_eq!(
            ready(&docs, inspect).inventory.unwrap().entries[0].decoded_bytes,
            Some(OTF.len() as u32)
        );
        docs.shutdown_texts().unwrap();
    }
}

#[test]
fn font_proposals_reject_stale_keys_and_cannot_cross_review_families() {
    let (root, docs, original) = fixture("ass");
    let id = start(&docs, add(root.path()));
    ready(&docs, id);
    assert!(docs.style_view(id).is_err());
    assert!(docs.style_accept(id).is_err());
    assert!(docs.text_view(id, 0, 10).is_err());
    assert!(docs.text_accept(id, None).is_err());
    docs.style_cancel(id).unwrap();
    docs.text_cancel(id).unwrap();
    assert_eq!(ready(&docs, id).phase, Phase::Reviewing);
    let snap = action(&docs, Action::Status);
    action(
        &docs,
        Action::Edit {
            cue_id: snap.cues[0].id,
            start_ms: snap.cues[0].start_ms,
            end_ms: snap.cues[0].end_ms,
            text: "changed".into(),
        },
    );
    assert_eq!(ready(&docs, id).phase, Phase::Stale);
    assert!(docs.font_accept(id).is_err());
    action(&docs, Action::Undo);
    assert_eq!(bytes(&docs), original);
    let id = start(&docs, add(root.path()));
    ready(&docs, id);
    let context = docs.quality_context().unwrap();
    let mut timing = context.timing;
    timing.cue_gap_ms = 190;
    docs.apply_quality_defaults(
        context.epoch,
        crate::quality_profile::Policy::legacy(timing, "test").unwrap(),
        || Ok(()),
    )
    .unwrap();
    assert_eq!(ready(&docs, id).phase, Phase::Stale);
    assert!(docs.font_accept(id).is_err());
    let style = style_engine::StyleOperation::Edit {
        name: "Title".into(),
        fields: vec![],
    };
    let snapshot = action(&docs, Action::Status);
    let style_id = docs
        .style_start(snapshot.generation, snapshot.revision, style)
        .unwrap();
    assert!(docs.font_view(style_id).is_err());
    assert!(docs.font_accept(style_id).is_err());
    docs.font_cancel(style_id).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while docs.style_view(style_id).unwrap().phase == Phase::Preparing {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert_eq!(docs.style_view(style_id).unwrap().phase, Phase::Reviewing);
    let id = start(&docs, add(root.path()));
    ready(&docs, id);
    action(&docs, Action::Close { discard: true });
    assert!(docs.font_accept(id).is_err());
    assert!(docs.font_start(1, 0, FontOperation::Inspect).is_err());
    docs.shutdown_texts().unwrap();
}

#[test]
fn bounded_font_reads_fail_cleanly_and_share_worker_admission_and_shutdown() {
    let (root, docs, original) = fixture("ass");
    for (name, content) in [
        ("invalid.ttf", b"invalid".as_slice()),
        ("empty.otf", b"".as_slice()),
        ("wrong.bin", TTF),
    ] {
        fs::write(root.path().join(name), content).unwrap();
    }
    fs::create_dir(root.path().join("directory.ttf")).unwrap();
    let huge = fs::File::create(root.path().join("huge.ttf")).unwrap();
    huge.set_len(MAX_FONT_BYTES as u64 + 1).unwrap();
    for name in [
        "invalid.ttf",
        "empty.otf",
        "wrong.bin",
        "missing.ttf",
        "directory.ttf",
        "huge.ttf",
    ] {
        let id = start(
            &docs,
            FontOperation::Add {
                path: root.path().join(name).to_string_lossy().into_owned(),
            },
        );
        assert_eq!(ready(&docs, id).phase, Phase::Failed, "{name}");
        assert!(docs.font_accept(id).is_err());
        assert_eq!(bytes(&docs), original);
    }
    let path = root.path().join("Fixture.ttf");
    let mut count = 0;
    assert_eq!(
        read_font(&path, &mut || {
            count += 1;
            if count == 2 {
                Err("cancelled".into())
            } else {
                Ok(())
            }
        }),
        Err("cancelled".into())
    );
    let (entered, rx) = std::sync::mpsc::channel();
    let (release, wait) = std::sync::mpsc::channel();
    let id = {
        let mut store = docs.0.lock().unwrap();
        let key = store.quality_key().unwrap();
        let profile = store.quality.context().profile;
        store
            .texts
            .launch(
                key,
                Request::Font(FontOperation::Inspect),
                profile,
                move |cancel| {
                    entered.send(cancel).unwrap();
                    wait.recv_timeout(Duration::from_secs(5)).unwrap();
                    Err("late error".into())
                },
            )
            .unwrap()
    };
    let cancel = rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(docs.font_start(1, 0, FontOperation::Inspect).is_err());
    assert!(docs.text_start(1, 0, Operation::JoinLines).is_err());
    assert!(
        docs.style_start(
            1,
            0,
            style_engine::StyleOperation::Edit {
                name: "Title".into(),
                fields: vec![]
            }
        )
        .is_err()
    );
    docs.font_cancel(id).unwrap();
    assert!(cancel.load(Ordering::Relaxed));
    assert_eq!(docs.font_view(id).unwrap().phase, Phase::Cancelling);
    release.send(()).unwrap();
    let view = ready(&docs, id);
    assert_eq!(view.phase, Phase::Cancelled);
    assert!(view.error.is_none());
    assert_eq!(bytes(&docs), original);
    let newer = start(&docs, add(root.path()));
    docs.font_cancel(id).unwrap();
    assert_eq!(ready(&docs, newer).phase, Phase::Reviewing);
    docs.shutdown_texts().unwrap();
    assert!(docs.0.lock().unwrap().texts.job.is_none());
    assert!(docs.font_start(1, 0, FontOperation::Inspect).is_err());
}

pub(crate) fn ipc_samples() -> Vec<(&'static str, &'static str, serde_json::Value)> {
    let (root, docs, _) = fixture("ass");
    let id = start(&docs, add(root.path()));
    let review = ready(&docs, id);
    docs.font_accept(id).unwrap();
    let id = start(&docs, FontOperation::Inspect);
    let inventory = ready(&docs, id);
    let mut review = serde_json::to_value(review).unwrap();
    review["operation"]["path"] = serde_json::json!("/fixture/Fixture.ttf");
    vec![
        ("fontReview", "FontReview", review),
        (
            "fontInventory",
            "FontReview",
            serde_json::to_value(inventory).unwrap(),
        ),
    ]
}
