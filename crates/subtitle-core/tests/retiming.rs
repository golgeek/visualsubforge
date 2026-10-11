use subtitle_core::{Cue, CueId, CueTiming, Document, Edit, Editor, Milliseconds};

fn ms(value: u64) -> Milliseconds {
    Milliseconds::new(value).unwrap()
}
fn editor() -> Editor {
    Editor::new(
        Document::new(vec![
            Cue::new(CueId(42), ms(1000), ms(2400), "First\n日本語".into()).unwrap(),
            Cue::new(CueId(9), ms(2000), ms(3000), "<i>Overlapping</i>".into()).unwrap(),
        ])
        .unwrap(),
    )
}
fn shifted(editor: &Editor, delta: u64) -> Vec<CueTiming> {
    editor
        .document()
        .cues()
        .iter()
        .map(|cue| CueTiming {
            id: cue.id(),
            start: ms(cue.start().get() + delta),
            end: ms(cue.end().get() + delta),
        })
        .collect()
}

#[test]
fn one_retime_preserves_text_ids_and_order_and_undo_restores_every_cue() {
    let mut editor = editor();
    let before = editor.document().clone();
    let timings = shifted(&editor, 500);
    let outcome = editor
        .apply_edit(0, Edit::Retime { timings }, |_| Ok(()))
        .unwrap();
    assert!(outcome.changed);
    assert_eq!(outcome.created, None);
    assert_eq!(editor.revision(), 1);
    let after = editor.document().clone();
    for (old, new) in before.cues().iter().zip(after.cues()) {
        assert_eq!((old.id(), old.text()), (new.id(), new.text()));
        assert_eq!(old.start().get() + 500, new.start().get());
        assert_eq!(old.end().get() + 500, new.end().get());
    }
    assert!(editor.dirty());
    assert!(editor.undo(1).unwrap());
    assert_eq!(editor.document(), &before);
    assert!(!editor.dirty());
    assert!(!editor.can_undo());
    assert!(editor.redo(2).unwrap());
    assert_eq!(editor.document(), &after);
    editor.mark_saved(3).unwrap();
    assert!(!editor.dirty());
    editor.undo(3).unwrap();
    assert!(editor.dirty());
    editor.redo(4).unwrap();
    assert!(!editor.dirty());
}

#[test]
fn all_rejections_and_noop_preserve_revision_saved_state_and_redo() {
    let mut editor = editor();
    let timings = shifted(&editor, 500);
    editor
        .apply_edit(0, Edit::Retime { timings }, |_| Ok(()))
        .unwrap();
    editor.undo(1).unwrap();
    let before = editor.document().clone();
    let valid = shifted(&editor, 250);
    let mut invalid = valid.clone();
    invalid[1].end = invalid[1].start;
    let mut duplicate = valid.clone();
    duplicate[1].id = duplicate[0].id;
    let mut reversed = valid.clone();
    reversed.reverse();
    let mut unknown = valid.clone();
    unknown[1].id = CueId(77);
    for timings in [invalid, duplicate, reversed, unknown, valid[..1].to_vec()] {
        assert!(
            editor
                .apply_edit(2, Edit::Retime { timings }, |_| Ok(()))
                .is_err()
        );
        assert_eq!(editor.document(), &before);
        assert_eq!(editor.revision(), 2);
        assert!(editor.can_redo());
        assert!(!editor.dirty());
    }
    assert!(
        editor
            .apply_edit(
                1,
                Edit::Retime {
                    timings: valid.clone()
                },
                |_| Ok(())
            )
            .is_err()
    );
    assert!(
        editor
            .apply_edit(2, Edit::Retime { timings: valid }, |proposal| {
                assert_eq!(proposal.cues()[1].start(), ms(2250));
                Err("Application policy rejects the complete proposal".into())
            })
            .is_err()
    );
    let timings = shifted(&editor, 0);
    assert!(
        !editor
            .apply_edit(2, Edit::Retime { timings }, |_| Ok(()))
            .unwrap()
            .changed
    );
    assert_eq!(editor.document(), &before);
    assert_eq!(editor.revision(), 2);
    assert!(editor.can_redo());
    assert!(!editor.dirty());
    // Retiming never consumes a cue identity, including after a rejected edit.
    let created = editor
        .create(2, None, ms(10), ms(20), String::new())
        .unwrap();
    assert_eq!(created, CueId(43));
}

#[test]
fn generated_retiming_history_restores_each_accepted_state() {
    let mut editor = editor();
    let mut states = vec![editor.document().clone()];
    let mut seed = 76123u32;
    for _ in 0..64 {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let timings = shifted(&editor, u64::from(seed % 999) + 1);
        editor
            .apply_edit(editor.revision(), Edit::Retime { timings }, |_| Ok(()))
            .unwrap();
        states.push(editor.document().clone());
    }
    for state in states[..states.len() - 1].iter().rev() {
        editor.undo(editor.revision()).unwrap();
        assert_eq!(editor.document(), state);
    }
    assert!(!editor.dirty());
    for state in &states[1..] {
        editor.redo(editor.revision()).unwrap();
        assert_eq!(editor.document(), state);
    }
}
