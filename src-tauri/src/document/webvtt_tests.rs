use super::*;

const VTT: &str = include_str!("../../../tests/fixtures/subtitles/styles.vtt");
fn apply(store: &mut Store, action: Action) -> Result<Snapshot> {
    let snapshot = store.snapshot();
    store.apply(action, snapshot.generation, snapshot.revision)
}
fn open(store: &mut Store, path: &Path) -> Snapshot {
    apply(
        store,
        Action::Open {
            path: path.to_string_lossy().into_owned(),
            discard: true,
        },
    )
    .unwrap()
}
fn bytes(store: &Store) -> String {
    let loaded = store.loaded.as_ref().unwrap();
    loaded.source().serialize(loaded.editor.document()).unwrap()
}
fn edit() -> Action {
    Action::Edit {
        cue_id: 1,
        start_ms: 1002,
        end_ms: 3999,
        text: "<v Alice>Bonjour &amp; 世界</v>".into(),
    }
}

#[test]
fn webvtt_edit_save_reopen_history_stale_rejection_and_native_transport() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("original.vtt");
    fs::write(&path, VTT).unwrap();
    let mut store = Store::default();
    let initial = open(&mut store, &path);
    let format = initial.format.unwrap();
    assert_eq!(format.name, "vtt");
    assert_eq!(format.time_step_ms, 1);
    assert!(format.styles.is_empty());
    let changed = apply(&mut store, edit()).unwrap();
    assert!(changed.dirty);
    let accepted = bytes(&store);
    assert!(accepted.contains("00:00:01.002 --> 00:03.999"));
    assert!(accepted.contains("intro,one"));
    assert!(accepted.contains("position:20%,line-left"));
    assert!(accepted.contains("STYLE\n::cue"));
    assert!(accepted.contains("REGION\n"));
    assert!(
        store
            .apply(Action::Delete { cue_id: 1 }, changed.generation, 0)
            .is_err()
    );
    assert_eq!(bytes(&store), accepted);
    let loaded = store.loaded.as_ref().unwrap();
    let preview =
        crate::subtitle_preview::serialize_source(loaded.editor.document(), loaded.source())
            .unwrap()
            .unwrap();
    assert!(preview.url.starts_with("memory://WEBVTT"));
    assert!(preview.url.contains("<v Alice>Bonjour &amp; 世界</v>"));
    assert!(preview.url.contains("<00:02.500>"));
    for target in [
        dir.path().join("bad.srt"),
        dir.path().join("missing/bad.vtt"),
    ] {
        assert!(
            apply(
                &mut store,
                Action::Save {
                    path: Some(target.to_string_lossy().into_owned())
                }
            )
            .is_err()
        );
        assert!(!target.exists());
        assert_eq!(bytes(&store), accepted);
        assert_eq!(fs::read_to_string(&path).unwrap(), VTT);
    }
    let copy = dir.path().join("saved.vtt");
    apply(
        &mut store,
        Action::Save {
            path: Some(copy.to_string_lossy().into_owned()),
        },
    )
    .unwrap();
    assert_eq!(fs::read_to_string(&copy).unwrap(), accepted);
    assert!(!store.snapshot().dirty);
    apply(&mut store, Action::Undo).unwrap();
    assert_eq!(bytes(&store), VTT);
    assert!(store.snapshot().dirty);
    apply(&mut store, Action::Redo).unwrap();
    assert_eq!(bytes(&store), accepted);
    assert!(!store.snapshot().dirty);
    open(&mut store, &copy);
    assert_eq!(bytes(&store), accepted);
    fs::write(&copy, "external change").unwrap();
    assert!(apply(&mut store, Action::Save { path: None }).is_err());
    assert_eq!(fs::read_to_string(&copy).unwrap(), "external change");
    assert_eq!(bytes(&store), accepted);
    assert_eq!(fs::read_to_string(&path).unwrap(), VTT);
}

#[test]
fn webvtt_recovery_retains_source_identity_blocks_and_requires_save_as() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("original.vtt");
    fs::write(&path, VTT).unwrap();
    let mut store = Store::default();
    open(&mut store, &path);
    apply(&mut store, edit()).unwrap();
    let accepted = bytes(&store);
    let record = crate::recovery_store::Record::new_document(
        subtitle_formats::Format::WebVtt,
        Some(path.to_string_lossy().into_owned()),
        VTT.into(),
        accepted.clone(),
        1,
        1,
    )
    .unwrap();
    let root = dir.path().join("recovery");
    let session = crate::recovery_store::Session::create(&root).unwrap();
    session.prepare(&record).unwrap().commit().unwrap();
    drop(session);
    let entries = crate::recovery_store::list(&root).unwrap();
    assert_eq!(entries.len(), 1);
    let mut recovered = Store {
        recovery_root: Some(root),
        ..Store::default()
    };
    apply(
        &mut recovered,
        Action::Recover {
            id: entries[0].id.clone(),
            discard: true,
        },
    )
    .unwrap();
    assert_eq!(bytes(&recovered), accepted);
    let snapshot = recovered.snapshot();
    assert_eq!(snapshot.format.unwrap().name, "vtt");
    assert!(snapshot.requires_save_as && snapshot.dirty && !snapshot.can_undo);
    assert!(apply(&mut recovered, Action::Save { path: None }).is_err());
    let saved = dir.path().join("recovered.vtt");
    apply(
        &mut recovered,
        Action::Save {
            path: Some(saved.to_string_lossy().into_owned()),
        },
    )
    .unwrap();
    assert_eq!(fs::read_to_string(&saved).unwrap(), accepted);
    assert_eq!(fs::read_to_string(&path).unwrap(), VTT);
}

#[test]
fn webvtt_structural_edits_and_formatting_keep_format_context() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("plain.vtt");
    let input = "WEBVTT\n\nNOTE retained\n\n00:01.001 --> 00:02.999 align:start\n<v Alice>one</v>\n\n00:03.001 --> 00:04.999\nsecond";
    fs::write(&path, input).unwrap();
    let docs = Documents::default();
    let snapshot = open(&mut docs.0.lock().unwrap(), &path);
    let context = crate::text_review::FormattingContext {
        generation: snapshot.generation.try_into().unwrap(),
        revision: snapshot.revision.try_into().unwrap(),
        cue_id: 1.try_into().unwrap(),
        style_name: String::new(),
    };
    let formatted = docs
        .format_text(
            "<v Alice>one</v>",
            9,
            12,
            vsf_application::text::Format::Bold,
            Some(&context),
        )
        .unwrap();
    assert_eq!(formatted.text, "<v Alice><b>one</b></v>");
    assert!(
        docs.format_text("one", 0, 3, vsf_application::text::Format::Bold, None)
            .is_err()
    );
    let mut store = docs.0.lock().unwrap();
    let split = apply(
        &mut store,
        Action::Split {
            cue_id: 1,
            at_ms: 1501,
        },
    )
    .unwrap();
    assert_eq!(split.cues.len(), 3);
    let split_bytes = bytes(&store);
    apply(
        &mut store,
        Action::Merge {
            first_id: 1,
            second_id: split.cues[1].id,
        },
    )
    .unwrap();
    assert_eq!(store.snapshot().cues.len(), 2);
    apply(&mut store, Action::Undo).unwrap();
    assert_eq!(bytes(&store), split_bytes);
    apply(&mut store, Action::Undo).unwrap();
    assert_eq!(bytes(&store), input);
    drop(store);
    assert!(
        docs.format_text(
            "one",
            0,
            3,
            vsf_application::text::Format::Bold,
            Some(&context)
        )
        .is_err()
    );
    fs::write(&path, VTT).unwrap();
    let mut store = docs.0.lock().unwrap();
    open(&mut store, &path);
    let before = store.snapshot().revision;
    assert!(
        apply(
            &mut store,
            Action::Split {
                cue_id: 1,
                at_ms: 1500
            }
        )
        .unwrap_err()
        .contains("identifiers")
    );
    assert_eq!(store.snapshot().revision, before);
    assert_eq!(bytes(&store), VTT);
}
