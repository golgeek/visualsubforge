//! Real Serde samples and boundary tests, independent of generated declarations.
use super::*;
use crate::ipc_number::MAX_SAFE_INTEGER;
use serde_json::{Value, json};

pub(crate) fn samples() -> Vec<(&'static str, &'static str, Value)> {
    let actions = json!([
        {"kind":"status"},
        {"kind":"create","afterId":null,"startMs":1000,"endMs":2000,"text":"A\nB"},
        {"kind":"create","startMs":2100,"endMs":3000,"text":"Missing optional ID"},
        {"kind":"edit","cueId":1,"startMs":1100,"endMs":2200,"text":"Edited"},
        {"kind":"editStyled","cueId":1,"startMs":1100,"endMs":2200,"text":"Styled","style":"Default"},
        {"kind":"delete","cueId":1},
        {"kind":"split","cueId":1,"atMs":1500},
        {"kind":"merge","firstId":1,"secondId":2},
        {"kind":"save","path":null},
        {"kind":"save","path":"copy.srt"},
        {"kind":"open","path":"source.srt","discard":false},
        {"kind":"recover","id":"session-example","discard":true},
        {"kind":"close","discard":false},
        {"kind":"undo"},
        {"kind":"redo"}
    ]);
    for value in actions.as_array().unwrap() {
        serde_json::from_value::<Action>(value.clone()).unwrap();
    }
    let mut store = Store::default();
    let empty = serde_json::to_value(store.snapshot()).unwrap();
    let created = store
        .apply(serde_json::from_value(actions[1].clone()).unwrap(), 0, 0)
        .unwrap();
    let recovery = RecoveryStatus {
        entries: vec![
            crate::recovery_store::Entry {
                id: "session-example".into(),
                source_path: None,
                captured_ms: Some(MAX_SAFE_INTEGER),
                revision: Some(42),
                error: None,
            },
            crate::recovery_store::Entry {
                id: "session-corrupt".into(),
                source_path: None,
                captured_ms: None,
                revision: None,
                error: Some("Corrupt record".into()),
            },
        ],
        error: None,
    };
    let parsed =
        subtitle_formats::parse(super::ass_tests::ASS, Some(subtitle_formats::Format::Ass))
            .unwrap();
    store.loaded = Some(Loaded {
        editor: Editor::with_properties(parsed.document, parsed.source),
        path: None,
        saved_bytes: super::ass_tests::ASS.as_bytes().to_vec(),
        recovered: true,
        resynced: false,
        translation: translation::Context::default(),
    });
    vec![
        ("documentActions", "DocumentAction[]", actions),
        ("emptyDocument", "DocumentSnapshot", empty),
        (
            "styledDocument",
            "DocumentSnapshot",
            serde_json::to_value(store.snapshot()).unwrap(),
        ),
        (
            "createdDocument",
            "DocumentSnapshot",
            serde_json::to_value(created).unwrap(),
        ),
        (
            "recovery",
            "RecoveryStatus",
            serde_json::to_value(recovery).unwrap(),
        ),
    ]
}

#[test]
fn input_codecs_reject_lossy_numbers_and_keep_optional_ids_optional() {
    for mut action in samples()[0].2.as_array().unwrap().clone() {
        for field in [
            "cueId", "afterId", "startMs", "endMs", "firstId", "secondId", "atMs",
        ] {
            if action.get(field).is_none() {
                continue;
            }
            let original = action[field].clone();
            for invalid in [json!(MAX_SAFE_INTEGER + 1), json!(-1), json!(1.5)] {
                action[field] = invalid;
                assert!(
                    serde_json::from_value::<Action>(action.clone()).is_err(),
                    "{action}"
                );
            }
            action[field] = json!(MAX_SAFE_INTEGER);
            assert!(serde_json::from_value::<Action>(action.clone()).is_ok());
            action[field] = original;
        }
    }
}

#[test]
fn response_codecs_reject_lossy_ids_times_revisions_and_optional_metadata() {
    let valid = CueView {
        id: MAX_SAFE_INTEGER,
        start_ms: 1,
        end_ms: 2,
        text: "A".into(),
        style: None,
    };
    assert_eq!(
        serde_json::to_value(&valid).unwrap()["id"],
        MAX_SAFE_INTEGER
    );
    for cue in [
        CueView {
            id: MAX_SAFE_INTEGER + 1,
            ..valid
        },
        CueView {
            id: 1,
            start_ms: MAX_SAFE_INTEGER + 1,
            end_ms: 2,
            text: "A".into(),
            style: None,
        },
        CueView {
            id: 1,
            start_ms: 1,
            end_ms: MAX_SAFE_INTEGER + 1,
            text: "A".into(),
            style: None,
        },
    ] {
        assert!(serde_json::to_value(cue).is_err());
    }
    let mut snapshot = Store::default().snapshot();
    snapshot.generation = MAX_SAFE_INTEGER + 1;
    assert!(serde_json::to_value(&snapshot).is_err());
    snapshot.generation = 1;
    snapshot.revision = MAX_SAFE_INTEGER + 1;
    assert!(serde_json::to_value(snapshot).is_err());
    let mut entry = crate::recovery_store::Entry {
        id: "session-example".into(),
        source_path: None,
        captured_ms: Some(MAX_SAFE_INTEGER + 1),
        revision: None,
        error: None,
    };
    assert!(serde_json::to_value(&entry).is_err());
    entry.captured_ms = None;
    entry.revision = Some(MAX_SAFE_INTEGER + 1);
    assert!(serde_json::to_value(entry).is_err());
}
