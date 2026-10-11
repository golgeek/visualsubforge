use subtitle_core::{Cue, CueId, Document, Editor, Milliseconds};

fn ms(value: u64) -> Milliseconds {
    Milliseconds::new(value).unwrap()
}
fn cue(id: u64, start: u64, end: u64, text: &str) -> Cue {
    Cue::new(CueId(id), ms(start), ms(end), text.into()).unwrap()
}
fn editor() -> Editor {
    // Deliberately unsorted times and IDs, with Unicode and literal markup.
    Editor::new(
        Document::new(vec![
            cue(10, 3000, 5000, " <b>日本語</b> "),
            cue(2, 1000, 4000, "café\nSecond line"),
            cue(30, 9000, 10000, ""),
        ])
        .unwrap(),
    )
}

#[test]
fn every_structural_transaction_restores_exact_content_order_and_saved_state() {
    let mut e = editor();
    let mut states = vec![e.document().clone()];
    let created = e
        .create(0, Some(CueId(10)), ms(0), ms(2000), "New".into())
        .unwrap();
    assert_eq!(created, CueId(31));
    assert_eq!(e.document().cues()[1].id(), created);
    states.push(e.document().clone());
    let right = e.split(1, CueId(2), ms(2000)).unwrap();
    assert_eq!(right, CueId(32));
    assert_eq!(
        e.document().cues()[2],
        cue(2, 1000, 2000, "café\nSecond line")
    );
    assert_eq!(
        e.document().cues()[3],
        cue(32, 2000, 4000, "café\nSecond line")
    );
    states.push(e.document().clone());
    e.merge(2, CueId(10), created).unwrap();
    assert_eq!(
        e.document().cues()[0],
        cue(10, 0, 5000, " <b>日本語</b> \nNew")
    );
    states.push(e.document().clone());
    e.delete(3, CueId(2)).unwrap();
    states.push(e.document().clone());
    e.mark_saved(4).unwrap();
    for expected in states[..4].iter().rev() {
        assert!(e.undo(e.revision()).unwrap());
        assert_eq!(e.document(), expected);
        assert!(e.dirty());
    }
    assert!(!e.can_undo());
    for expected in &states[1..] {
        assert!(e.redo(e.revision()).unwrap());
        assert_eq!(e.document(), expected);
    }
    assert!(!e.dirty());
    assert!(!e.can_redo());
}

#[test]
fn rejected_operations_preserve_revision_content_and_redo() {
    let mut e = editor();
    e.delete(0, CueId(30)).unwrap();
    e.undo(1).unwrap();
    let before = e.document().clone();
    assert!(e.create(1, None, ms(0), ms(1), "".into()).is_err());
    assert!(
        e.create(2, Some(CueId(999)), ms(0), ms(1), "".into())
            .is_err()
    );
    assert!(e.create(2, None, ms(1), ms(1), "".into()).is_err());
    assert!(e.create(2, None, ms(0), ms(1), "\0".into()).is_err());
    assert!(e.delete(1, CueId(10)).is_err());
    assert!(e.delete(2, CueId(999)).is_err());
    assert!(e.split(1, CueId(10), ms(4000)).is_err());
    for at in [0, 3000, 5000, 6000] {
        assert!(e.split(2, CueId(10), ms(at)).is_err());
    }
    assert!(e.split(2, CueId(999), ms(4000)).is_err());
    assert!(e.merge(1, CueId(10), CueId(2)).is_err());
    for (a, b) in [(10, 30), (2, 10), (30, 30), (999, 2)] {
        assert!(e.merge(2, CueId(a), CueId(b)).is_err());
    }
    assert_eq!(e.document(), &before);
    assert_eq!(e.revision(), 2);
    assert!(!e.dirty());
    assert!(!e.can_undo());
    assert!(e.can_redo());
    e.redo(2).unwrap();
    assert_eq!(e.document().cues().len(), 2);
}

#[test]
fn identities_are_not_reused_after_undo_branch_or_delete() {
    let mut e = editor();
    let discarded = e.split(0, CueId(10), ms(4000)).unwrap();
    e.undo(1).unwrap();
    let new = e.create(2, None, ms(0), ms(1), "".into()).unwrap();
    assert_ne!(new, discarded);
    assert!(!e.can_redo());
    e.delete(3, new).unwrap();
    let next = e.create(4, None, ms(0), ms(1), "".into()).unwrap();
    assert_ne!(next, new);
    e.undo(5).unwrap();
    e.undo(6).unwrap();
    assert_eq!(e.document().cues()[0].id(), new);
}

#[test]
fn empty_document_last_delete_and_one_millisecond_split_boundaries() {
    let mut e = Editor::new(Document::new(vec![]).unwrap());
    let id = e.create(0, None, ms(0), ms(1), "".into()).unwrap();
    assert!(e.split(1, id, ms(0)).is_err());
    assert!(e.split(1, id, ms(1)).is_err());
    e.delete(1, id).unwrap();
    assert!(e.document().cues().is_empty());
    e.undo(2).unwrap();
    assert_eq!(e.document().cues(), &[cue(id.0, 0, 1, "")]);
    e.undo(3).unwrap();
    assert!(e.document().cues().is_empty());
    assert!(!e.dirty());
    let id = e
        .create(
            4,
            None,
            ms(Milliseconds::MAX - 2),
            ms(Milliseconds::MAX),
            "X".into(),
        )
        .unwrap();
    e.split(5, id, ms(Milliseconds::MAX - 1)).unwrap();
    assert_eq!(e.document().cues().len(), 2);
}

#[test]
fn merge_spans_unsorted_overlap_or_gap_and_omits_empty_text_operands() {
    for (left, right, text) in [
        ("", "", ""),
        ("", "B", "B"),
        ("A", "", "A"),
        (" A ", " B ", " A \n B "),
    ] {
        for (start, end) in [(1000, 4000), (7000, 8000)] {
            let mut e = Editor::new(
                Document::new(vec![cue(8, 3000, 5000, left), cue(3, start, end, right)]).unwrap(),
            );
            let original = e.document().clone();
            e.merge(0, CueId(8), CueId(3)).unwrap();
            assert_eq!(
                e.document().cues(),
                &[cue(8, start.min(3000), end.max(5000), text)]
            );
            e.undo(1).unwrap();
            assert_eq!(e.document(), &original);
        }
    }
}

#[test]
fn exhausted_identity_space_is_a_recoverable_atomic_error() {
    let mut e = Editor::new(Document::new(vec![cue(u64::MAX, 0, 2, "A")]).unwrap());
    assert!(e.create(0, None, ms(0), ms(1), "".into()).is_err());
    assert!(e.split(0, CueId(u64::MAX), ms(1)).is_err());
    assert_eq!(e.revision(), 0);
    assert!(!e.dirty());
    assert!(!e.can_undo());
}
