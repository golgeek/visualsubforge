use subtitle_core::{Cue, CueId, CueText, Document, Edit, Editor, Milliseconds};

fn fixture() -> Editor {
    Editor::new(
        Document::new(
            (1..=3)
                .map(|id| {
                    Cue::new(
                        CueId(id),
                        Milliseconds::new(id * 1000).unwrap(),
                        Milliseconds::new(id * 1000 + 999).unwrap(),
                        format!("{{\\b1}}日本語 {id}"),
                    )
                    .unwrap()
                })
                .collect(),
        )
        .unwrap(),
    )
}
fn batch(editor: &Editor, value: &str) -> Edit {
    Edit::TextBatch {
        texts: editor
            .document()
            .cues()
            .iter()
            .map(|c| CueText {
                id: c.id(),
                text: value.into(),
            })
            .collect(),
    }
}

#[test]
fn text_batch_is_one_transaction_preserving_timing_identity_and_saved_state() {
    let mut editor = fixture();
    let before = editor.document().clone();
    let edit = batch(&editor, "<i>Été 😀</i>\n第二行");
    editor.apply_edit(0, edit, |_| Ok(())).unwrap();
    let after = editor.document().clone();
    for (a, b) in before.cues().iter().zip(after.cues()) {
        assert_eq!((a.id(), a.start(), a.end()), (b.id(), b.start(), b.end()));
        assert_eq!(b.text(), "<i>Été 😀</i>\n第二行");
    }
    assert_eq!(editor.revision(), 1);
    editor.undo(1).unwrap();
    assert_eq!(editor.document(), &before);
    assert!(!editor.can_undo());
    assert!(!editor.dirty());
    editor.redo(2).unwrap();
    assert_eq!(editor.document(), &after);
    editor.mark_saved(3).unwrap();
    editor.undo(3).unwrap();
    assert!(editor.dirty());
    editor.redo(4).unwrap();
    assert!(!editor.dirty());
}

#[test]
fn invalid_stale_duplicate_missing_and_policy_rejection_preserve_redo() {
    let mut editor = fixture();
    editor
        .apply_edit(0, batch(&editor, "changed"), |_| Ok(()))
        .unwrap();
    editor.undo(1).unwrap();
    let before = editor.document().clone();
    assert!(
        editor
            .apply_edit(1, batch(&editor, "stale"), |_| Ok(()))
            .is_err()
    );
    assert!(
        editor
            .apply_edit(2, batch(&editor, "bad\0text"), |_| Ok(()))
            .is_err()
    );
    for ids in [vec![1, 2], vec![1, 2, 2], vec![2, 1, 3], vec![1, 2, 4]] {
        let edit = Edit::TextBatch {
            texts: ids
                .into_iter()
                .map(|id| CueText {
                    id: CueId(id),
                    text: "bad identity".into(),
                })
                .collect(),
        };
        assert!(editor.apply_edit(2, edit, |_| Ok(())).is_err());
    }
    assert!(
        editor
            .apply_edit(2, batch(&editor, "rejected"), |_| Err("Size policy".into()))
            .is_err()
    );
    let same = Edit::TextBatch {
        texts: before
            .cues()
            .iter()
            .map(|c| CueText {
                id: c.id(),
                text: c.text().into(),
            })
            .collect(),
    };
    assert!(
        !editor
            .apply_edit(2, same, |_| panic!("no-op"))
            .unwrap()
            .changed
    );
    assert_eq!(editor.document(), &before);
    assert_eq!(editor.revision(), 2);
    assert!(editor.can_redo());
    assert!(!editor.dirty());
    editor.redo(2).unwrap();
    assert!(
        editor
            .document()
            .cues()
            .iter()
            .all(|c| c.text() == "changed")
    );
}
