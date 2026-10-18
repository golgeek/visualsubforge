//! Boundary regressions use actual SRT bytes and application transactions.
use super::*;
use crate::srt_limits::MAX_BYTES;

fn sized_srt(size: usize, crlf: bool, cues: usize) -> String {
    let newline = if crlf { "\r\n" } else { "\n" };
    let cue = format!("0{newline}00:00:00,000 --> 00:00:01,000{newline}x{newline}{newline}");
    let base = format!(
        "{}{text}",
        if crlf { "\u{feff}" } else { "" },
        text = cue.repeat(cues)
    );
    base.replacen('x', &"x".repeat(size - base.len() + 1), 1)
}

fn apply(store: &mut Store, action: Action) -> Result<Snapshot> {
    let snapshot = store.snapshot();
    store.apply(action, snapshot.generation, snapshot.revision)
}

fn snapshot_bytes(store: &Store) -> Vec<u8> {
    serde_json::to_vec(&store.snapshot()).unwrap()
}

#[test]
fn accepted_boundary_edits_remain_recoverable_saveable_and_reopenable() {
    for crlf in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("boundary.srt");
        let original = sized_srt(MAX_BYTES - 1, crlf, 1);
        fs::write(&path, &original).unwrap();
        let mut store = Store::default();
        apply(
            &mut store,
            Action::Open {
                path: path.to_string_lossy().into(),
                discard: false,
            },
        )
        .unwrap();
        let text = store.snapshot().cues[0].text.clone();
        let before = snapshot_bytes(&store);
        let error = apply(
            &mut store,
            Action::Edit {
                cue_id: 1,
                start_ms: 0,
                end_ms: 1000,
                text: format!("{text}abcd"),
            },
        )
        .unwrap_err();
        assert!(error.contains("16 MiB"));
        assert_eq!(snapshot_bytes(&store), before);
        assert_eq!(fs::read(&path).unwrap(), original.as_bytes());

        let accepted = apply(
            &mut store,
            Action::Edit {
                cue_id: 1,
                start_ms: 0,
                end_ms: 1000,
                text: format!("{text}a"),
            },
        )
        .unwrap();
        let loaded = store.loaded.as_ref().unwrap();
        let serialized = loaded.source.serialize(loaded.editor.document()).unwrap();
        assert_eq!(serialized.len(), MAX_BYTES);
        crate::recovery_store::Record::new(
            Some(path.to_string_lossy().into_owned()),
            original,
            serialized.clone(),
            accepted.generation,
            accepted.revision,
        )
        .unwrap();
        assert!(
            !apply(&mut store, Action::Save { path: None })
                .unwrap()
                .dirty
        );
        assert_eq!(read_file(&path).unwrap(), serialized.as_bytes());
        assert!(apply(&mut store, Action::Undo).unwrap().dirty);
        assert!(!apply(&mut store, Action::Redo).unwrap().dirty);
        let reopened = apply(
            &mut store,
            Action::Open {
                path: path.to_string_lossy().into(),
                discard: false,
            },
        )
        .unwrap();
        assert_eq!(reopened.cues[0].text, format!("{text}a"));
    }
}

#[test]
fn structural_growth_and_label_renumbering_are_rejected_atomically() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("structural.srt");
    fs::write(&path, sized_srt(MAX_BYTES - 1, false, 100)).unwrap();
    let mut store = Store::default();
    apply(
        &mut store,
        Action::Open {
            path: path.to_string_lossy().into(),
            discard: false,
        },
    )
    .unwrap();
    let before = snapshot_bytes(&store);
    // Even deletion/merge can grow SRT: duplicated one-digit imported labels
    // become consecutive numbers when the original identity/order changes.
    for action in [
        Action::Create {
            after_id: Some(100),
            start_ms: 0,
            end_ms: 1000,
            text: String::new(),
        },
        Action::Split {
            cue_id: 1,
            at_ms: 500,
        },
        Action::Delete { cue_id: 100 },
        Action::Merge {
            first_id: 99,
            second_id: 100,
        },
    ] {
        assert!(apply(&mut store, action).unwrap_err().contains("16 MiB"));
        assert_eq!(snapshot_bytes(&store), before);
    }
}

#[test]
fn failed_first_cue_does_not_publish_state_or_consume_an_identity() {
    let mut store = Store::default();
    let before = snapshot_bytes(&store);
    assert!(
        apply(
            &mut store,
            Action::Create {
                after_id: None,
                start_ms: 0,
                end_ms: 1000,
                text: "x".repeat(MAX_BYTES)
            }
        )
        .is_err()
    );
    assert_eq!(snapshot_bytes(&store), before);
    let created = apply(
        &mut store,
        Action::Create {
            after_id: None,
            start_ms: 0,
            end_ms: 1000,
            text: "small".into(),
        },
    )
    .unwrap();
    assert_eq!(
        (created.generation, created.revision, created.cues[0].id),
        (1, 1, 1)
    );
}

#[test]
fn normalization_growth_is_rejected_on_open_and_recovery() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("normalization.srt");
    let input = sized_srt(MAX_BYTES + 2, false, 1)
        .trim_end_matches('\n')
        .to_owned();
    assert_eq!(input.len(), MAX_BYTES);
    fs::write(&path, &input).unwrap();
    let mut store = Store::default();
    let before = snapshot_bytes(&store);
    assert!(
        apply(
            &mut store,
            Action::Open {
                path: path.to_string_lossy().into(),
                discard: false
            }
        )
        .unwrap_err()
        .contains("16 MiB")
    );
    assert_eq!(snapshot_bytes(&store), before);
    assert!(crate::recovery_store::Record::new(None, String::new(), input, 1, 0).is_err());
}

#[test]
fn save_guard_preserves_original_and_dirty_state_for_an_invalid_internal_document() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("saved.srt");
    let original = sized_srt(100, false, 1);
    fs::write(&path, &original).unwrap();
    let mut store = Store::default();
    apply(
        &mut store,
        Action::Open {
            path: path.to_string_lossy().into(),
            discard: false,
        },
    )
    .unwrap();
    // Bypass application acceptance deliberately to exercise the independent
    // last defense before disk replacement, e.g. against a future caller error.
    store
        .loaded
        .as_mut()
        .unwrap()
        .editor
        .edit(
            0,
            CueId(1),
            Milliseconds::new(0).unwrap(),
            Milliseconds::new(1000).unwrap(),
            "x".repeat(MAX_BYTES),
        )
        .unwrap();
    let before = snapshot_bytes(&store);
    assert!(
        apply(&mut store, Action::Save { path: None })
            .unwrap_err()
            .contains("16 MiB")
    );
    assert_eq!(snapshot_bytes(&store), before);
    assert!(store.snapshot().dirty);
    assert_eq!(fs::read(path).unwrap(), original.as_bytes());
}
