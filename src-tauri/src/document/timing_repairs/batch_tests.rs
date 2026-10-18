use super::*;

pub(super) fn ready(docs: &Documents, id: u64) -> BatchView {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let v = docs.batch_timing_view(id, 0, 100).unwrap();
        if v.phase != Phase::Preparing {
            return v;
        }
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
}
pub(super) fn start(docs: &Documents, scenes: &crate::scenes::Scenes, offset: i64) -> u64 {
    let d = docs.apply(Action::Status, 0, 0).unwrap();
    docs.batch_timing_start(
        d.generation,
        d.revision,
        scenes.capture_quality(7, true).unwrap(),
        batch::Request {
            scope: batch::Scope::All,
            change: batch::Change::Shift(offset),
        },
    )
    .unwrap()
}
#[test]
fn batch_owner_pages_history_kind_guards_and_atomic_failures() {
    let (_dir, docs, scenes) = tests::fixture();
    let original = docs.apply(Action::Status, 0, 0).unwrap();
    let id = start(&docs, &scenes, 123);
    let view = ready(&docs, id);
    assert_eq!(view.phase, Phase::Reviewing, "{:?}", view.error);
    assert_eq!((view.total, view.changed), (3, 3));
    assert!(docs.scene_repair_accept(id).is_err());
    assert!(docs.duration_repair_view(id, 0, 100).is_err());
    assert!(docs.batch_timing_view(id, 0, 201).is_err());
    assert_eq!(
        docs.batch_timing_view(id, 1, 1).unwrap().rows[0].cue_index,
        1
    );
    let accepted = docs.batch_timing_accept(id).unwrap();
    assert_eq!(accepted.revision, original.revision + 1);
    for (old, new) in original.cues.iter().zip(&accepted.cues) {
        assert_eq!(new.start_ms, old.start_ms + 123);
        assert_eq!(new.end_ms, old.end_ms + 123);
        assert_eq!(new.text, old.text);
    }
    assert!(docs.batch_timing_accept(id).is_err());
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
    let before = serde_json::to_value(&redone).unwrap();
    let invalid = start(&docs, &scenes, -99999);
    assert_eq!(ready(&docs, invalid).phase, Phase::Failed);
    assert!(docs.batch_timing_accept(invalid).is_err());
    assert_eq!(
        serde_json::to_value(docs.apply(Action::Status, 0, 0).unwrap()).unwrap(),
        before
    );
}

#[test]
fn batch_cancel_revision_replacement_and_shutdown_reject_late_acceptance() {
    let (_dir, docs, scenes) = tests::fixture();
    let id = start(&docs, &scenes, 123);
    ready(&docs, id);
    docs.timing_repair_cancel(id, Purpose::Batch).unwrap();
    assert_eq!(ready(&docs, id).phase, Phase::Cancelled);
    assert!(docs.batch_timing_accept(id).is_err());
    let id = start(&docs, &scenes, 123);
    ready(&docs, id);
    let doc = docs.apply(Action::Status, 0, 0).unwrap();
    let cue = &doc.cues[0];
    docs.apply(
        Action::Edit {
            cue_id: cue.id,
            start_ms: cue.start_ms,
            end_ms: cue.end_ms,
            text: "Changed".into(),
        },
        doc.generation,
        doc.revision,
    )
    .unwrap();
    assert_eq!(ready(&docs, id).phase, Phase::Stale);
    assert!(docs.batch_timing_accept(id).is_err());
    let replacement = start(&docs, &scenes, 100);
    ready(&docs, replacement);
    assert!(docs.batch_timing_view(id, 0, 100).is_err());
    docs.shutdown_timing_repairs().unwrap();
    assert!(docs.batch_timing_accept(replacement).is_err());
}

#[test]
fn batch_format_roundtrips_recovery_and_failed_save_preserve_source_and_effects() {
    for (extension, bytes, offset) in [
        (
            "ass",
            include_str!("../../../../tests/fixtures/subtitles/styles.ass"),
            120,
        ),
        (
            "ssa",
            include_str!("../../../../tests/fixtures/subtitles/styles.ssa"),
            120,
        ),
        (
            "ass",
            include_str!("../../../../tests/fixtures/subtitles/embedded-font.ass"),
            120,
        ),
        (
            "srt",
            "\u{feff}1\r\n00:00:01,001 --> 00:00:02,003\r\n{\\k20\\move(1,2,3,4,0,400)}é😀\r\n\r\n",
            123,
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(format!("original.{extension}"));
        fs::write(&path, bytes).unwrap();
        let docs = Documents::default();
        let original = docs
            .apply(
                Action::Open {
                    path: path.to_string_lossy().into(),
                    discard: false,
                },
                0,
                0,
            )
            .unwrap();
        let scenes = crate::scenes::Scenes::default();
        let id = docs
            .batch_timing_start(
                original.generation,
                original.revision,
                scenes.capture_quality(0, false).unwrap(),
                batch::Request {
                    scope: batch::Scope::All,
                    change: batch::Change::Shift(offset),
                },
            )
            .unwrap();
        assert_eq!(ready(&docs, id).phase, Phase::Reviewing);
        let accepted = docs.batch_timing_accept(id).unwrap();
        let saved_bytes = {
            let store = docs.0.lock().unwrap();
            let loaded = store.loaded.as_ref().unwrap();
            loaded.source.serialize(loaded.editor.document()).unwrap()
        };
        let parsed = subtitle_formats::parse(
            &saved_bytes,
            subtitle_formats::Format::from_extension(extension),
        )
        .unwrap();
        for (old, new) in original.cues.iter().zip(parsed.document.cues()) {
            assert_eq!(old.text, new.text());
            assert_eq!(old.start_ms + offset as u64, new.start().get());
            assert_eq!(old.end_ms + offset as u64, new.end().get());
        }
        // The source header, style definitions, attachments and unknown records
        // are byte-identical, even though timings are serialized anew.
        if extension != "srt" {
            assert_eq!(
                bytes.split("[Events]").next(),
                saved_bytes.split("[Events]").next()
            );
            if let Some((_, fonts)) = bytes.split_once("[Fonts]") {
                assert_eq!(saved_bytes.split_once("[Fonts]").unwrap().1, fonts);
            }
        }
        let recovery = dir.path().join("recovery");
        let session = crate::recovery_store::Session::create(&recovery).unwrap();
        let record = crate::recovery_store::Record::new_document(
            subtitle_formats::Format::from_extension(extension).unwrap(),
            Some(path.to_string_lossy().into()),
            bytes.into(),
            saved_bytes.clone(),
            accepted.generation,
            accepted.revision,
        )
        .unwrap();
        session.write(Some(&record)).unwrap();
        drop(session);
        let entries = crate::recovery_store::list(&recovery).unwrap();
        let mut restored = Store {
            recovery_root: Some(recovery),
            ..Store::default()
        };
        let recovered = restored
            .apply(
                Action::Recover {
                    id: entries[0].id.clone(),
                    discard: false,
                },
                0,
                0,
            )
            .unwrap();
        assert!(recovered.requires_save_as);
        let loaded = restored.loaded.as_ref().unwrap();
        assert_eq!(
            loaded.source.serialize(loaded.editor.document()).unwrap(),
            saved_bytes
        );
        assert!(
            restored
                .apply(
                    Action::Save { path: None },
                    recovered.generation,
                    recovered.revision
                )
                .is_err()
        );
        let before_failure = serde_json::to_value(&accepted).unwrap();
        assert!(
            docs.apply(
                Action::Save {
                    path: Some(
                        dir.path()
                            .join("missing")
                            .join(format!("copy.{extension}"))
                            .to_string_lossy()
                            .into()
                    )
                },
                accepted.generation,
                accepted.revision
            )
            .is_err()
        );
        assert_eq!(
            serde_json::to_value(docs.apply(Action::Status, 0, 0).unwrap()).unwrap(),
            before_failure
        );
        let output = dir.path().join(format!("copy.{extension}"));
        docs.apply(
            Action::Save {
                path: Some(output.to_string_lossy().into()),
            },
            accepted.generation,
            accepted.revision,
        )
        .unwrap();
        assert_eq!(fs::read_to_string(&output).unwrap(), saved_bytes);
        assert_eq!(fs::read_to_string(&path).unwrap(), bytes);
        let undone = docs
            .apply(Action::Undo, accepted.generation, accepted.revision)
            .unwrap();
        assert_eq!(
            serde_json::to_value(&undone.cues).unwrap(),
            serde_json::to_value(&original.cues).unwrap()
        );
        let restored = {
            let store = docs.0.lock().unwrap();
            let loaded = store.loaded.as_ref().unwrap();
            loaded.source.serialize(loaded.editor.document()).unwrap()
        };
        assert_eq!(restored, bytes);
        let reopened = Documents::default()
            .apply(
                Action::Open {
                    path: output.to_string_lossy().into(),
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
}
