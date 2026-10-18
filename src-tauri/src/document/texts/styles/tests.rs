use super::*;
use style_engine::StyleFieldValue;

fn fixture(extension: &str) -> (tempfile::TempDir, Documents, String) {
    let root = tempfile::tempdir().unwrap();
    let original = match extension {
        "ass" => crate::document::ass_tests::ASS,
        _ => crate::document::ass_tests::SSA,
    }
    .replace("Red title", "{\\rTitle}Red title");
    let source = format!("\u{feff}{}", original.replace('\n', "\r\n"));
    let path = root.path().join(format!("original.{extension}"));
    fs::write(&path, &source).unwrap();
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
    (root, docs, source)
}
fn edit(field: &str, value: &str) -> StyleOperation {
    StyleOperation::Edit {
        name: "Title".into(),
        fields: vec![StyleFieldValue {
            name: field.into(),
            value: value.into(),
        }],
    }
}
fn start(docs: &Documents, operation: StyleOperation) -> u64 {
    let snap = docs.apply(Action::Status, 0, 0).unwrap();
    docs.style_start(snap.generation, snap.revision, operation)
        .unwrap()
}
fn ready(docs: &Documents, id: u64) -> StyleReview {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let view = docs.style_view(id).unwrap();
        if view.phase != Phase::Preparing {
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

#[test]
fn reviewed_styles_share_cue_history_and_exact_save_protection_in_both_variants() {
    for extension in ["ass", "ssa"] {
        let (root, docs, original) = fixture(extension);
        let form = docs.style_form(1, 0, Some("Title")).unwrap();
        assert!(
            form.fields
                .iter()
                .any(|f| f.name == "Fontsize" && f.editable && f.value == "36")
        );
        assert!(docs.style_form(1, 1, None).is_err());
        let defaults = docs.style_form(1, 0, None).unwrap();
        assert_eq!(
            defaults.fields.len(),
            if extension == "ass" { 23 } else { 18 }
        );
        let before = docs.apply(Action::Status, 0, 0).unwrap();
        let id = start(&docs, edit("Name", "Heading"));
        let view = ready(&docs, id);
        assert_eq!(view.phase, Phase::Reviewing);
        assert_eq!(view.summary.unwrap().changed_cues, 1);
        assert_eq!(bytes(&docs), original);
        assert!(!docs.apply(Action::Status, 0, 0).unwrap().can_undo);
        assert!(docs.text_view(id, 0, 10).is_err());
        assert!(docs.text_accept(id, None).is_err());
        assert!(docs.text_matches(id, 0, 0, 10).is_err());
        docs.text_cancel(id).unwrap(); // wrong family cannot cancel the style
        assert_eq!(ready(&docs, id).phase, Phase::Reviewing);
        let accepted = docs.style_accept(id).unwrap();
        assert_eq!(accepted.revision, before.revision + 1);
        assert!(accepted.dirty && accepted.can_undo);
        assert_eq!(accepted.cues[0].style.as_deref(), Some("Heading"));
        assert_eq!(accepted.cues[0].text, "{\\rHeading}Red title");
        assert_eq!(accepted.cues[0].start_ms, before.cues[0].start_ms);
        assert_eq!(accepted.cues[0].end_ms, before.cues[0].end_ms);
        assert!(docs.style_accept(id).is_err());
        assert_eq!(ready(&docs, id).phase, Phase::Accepted);
        let changed = bytes(&docs);
        assert!(changed.contains("Style: Heading,"));
        assert!(changed.contains("Opaque: exact bytes,untouched\r\n"));
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
                accepted.generation,
                accepted.revision
            )
            .is_err()
        );
        assert_eq!(bytes(&docs), changed);
        assert_eq!(
            fs::read_to_string(root.path().join(format!("original.{extension}"))).unwrap(),
            original
        );
        action(&docs, Action::Undo);
        assert_eq!(bytes(&docs), original);
        let noop = start(&docs, edit("Fontsize", "36"));
        assert!(!ready(&docs, noop).changed);
        let unchanged = docs.style_accept(noop).unwrap();
        assert_eq!(unchanged.revision, 2);
        assert!(unchanged.can_redo);
        action(&docs, Action::Redo);
        assert_eq!(bytes(&docs), changed);
        let copy = root.path().join(format!("copy.{extension}"));
        let saved = action(
            &docs,
            Action::Save {
                path: Some(copy.to_string_lossy().into_owned()),
            },
        );
        assert!(!saved.dirty);
        assert_eq!(fs::read_to_string(&copy).unwrap(), changed);
        action(&docs, Action::Undo);
        assert!(docs.apply(Action::Status, 0, 0).unwrap().dirty);
        action(&docs, Action::Redo);
        assert!(!docs.apply(Action::Status, 0, 0).unwrap().dirty);
        action(
            &docs,
            Action::Open {
                path: copy.to_string_lossy().into_owned(),
                discard: false,
            },
        );
        assert_eq!(bytes(&docs), changed);
        let add = start(
            &docs,
            StyleOperation::Create {
                name: "Copied".into(),
                copy_from: Some("Heading".into()),
                fields: vec![],
            },
        );
        assert!(ready(&docs, add).summary.unwrap().created);
        docs.style_accept(add).unwrap();
        assert!(bytes(&docs).contains("Style: Copied,"));
        action(&docs, Action::Undo);
        assert_eq!(bytes(&docs), changed);
        docs.shutdown_texts().unwrap();
    }
}

#[test]
fn invalid_or_stale_styles_preserve_state_and_cannot_cross_review_families() {
    let (_root, docs, original) = fixture("ass");
    let bad = start(&docs, edit("Fontsize", "0"));
    assert_eq!(ready(&docs, bad).phase, Phase::Failed);
    assert!(docs.style_accept(bad).is_err());
    assert_eq!(bytes(&docs), original);
    let id = start(&docs, edit("Fontsize", "40"));
    ready(&docs, id);
    let snap = docs.apply(Action::Status, 0, 0).unwrap();
    action(
        &docs,
        Action::Edit {
            cue_id: snap.cues[0].id,
            start_ms: snap.cues[0].start_ms,
            end_ms: snap.cues[0].end_ms,
            text: "Edited cue".into(),
        },
    );
    assert_eq!(ready(&docs, id).phase, Phase::Stale);
    assert!(docs.style_accept(id).is_err());
    action(&docs, Action::Undo);
    assert_eq!(bytes(&docs), original);
    let id = start(&docs, edit("Fontsize", "40"));
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
    assert!(docs.style_accept(id).is_err());
    let id = start(&docs, edit("Fontsize", "40"));
    ready(&docs, id);
    action(&docs, Action::Close { discard: true });
    assert!(docs.style_accept(id).is_err());
    assert!(docs.style_form(1, 0, None).is_err());
    let (_dir, text_docs) = super::super::tests::fixture();
    let text_id = super::super::tests::start(&text_docs);
    super::super::tests::ready(&text_docs, text_id);
    assert!(text_docs.style_view(text_id).is_err());
    assert!(text_docs.style_accept(text_id).is_err());
    text_docs.style_cancel(text_id).unwrap();
    assert_eq!(
        super::super::tests::ready(&text_docs, text_id).phase,
        Phase::Reviewing
    );
    assert!(text_docs.style_start(1, 0, edit("Fontsize", "40")).is_err());
    docs.shutdown_texts().unwrap();
    text_docs.shutdown_texts().unwrap();
}

#[test]
fn shared_admission_cancel_shutdown_and_late_failure_keep_one_worker() {
    let (_root, docs, original) = fixture("ass");
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
                Request::Style(edit("Fontsize", "40")),
                profile,
                move |cancel| {
                    entered.send(cancel).unwrap();
                    wait.recv_timeout(Duration::from_secs(5)).unwrap();
                    Err("Late failure".into())
                },
            )
            .unwrap()
    };
    let cancel = rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(docs.style_start(1, 0, edit("Fontsize", "42")).is_err());
    assert!(docs.text_start(1, 0, Operation::JoinLines).is_err());
    docs.style_cancel(id).unwrap();
    assert!(cancel.load(Ordering::Relaxed));
    assert_eq!(docs.style_view(id).unwrap().phase, Phase::Cancelling);
    release.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while docs.style_view(id).unwrap().phase == Phase::Cancelling {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert!(docs.style_view(id).unwrap().error.is_none());
    assert!(docs.style_accept(id).is_err());
    assert_eq!(bytes(&docs), original);
    let newer = start(&docs, edit("Fontsize", "42"));
    docs.style_cancel(id).unwrap();
    assert_eq!(ready(&docs, newer).phase, Phase::Reviewing);
    docs.shutdown_texts().unwrap();
    assert!(docs.0.lock().unwrap().texts.job.is_none());
    assert!(docs.style_start(1, 0, edit("Fontsize", "42")).is_err());
}

pub(crate) fn ipc_samples() -> Vec<(&'static str, &'static str, serde_json::Value)> {
    let (_root, docs, _) = fixture("ass");
    let form = docs.style_form(1, 0, Some("Title")).unwrap();
    let defaults = docs.style_form(1, 0, None).unwrap();
    let id = start(&docs, edit("Fontsize", "42"));
    let view = ready(&docs, id);
    vec![
        (
            "styleForm",
            "StyleForm",
            serde_json::to_value(form).unwrap(),
        ),
        (
            "styleDefaults",
            "StyleForm",
            serde_json::to_value(defaults).unwrap(),
        ),
        (
            "styleReview",
            "StyleReview",
            serde_json::to_value(view).unwrap(),
        ),
    ]
}
