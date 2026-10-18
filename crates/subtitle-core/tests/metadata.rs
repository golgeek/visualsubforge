use subtitle_core::{Cue, CueId, CueMetadata, Document, Edit, Editor, Milliseconds};
fn ms(t: u64) -> Milliseconds {
    Milliseconds::new(t).unwrap()
}
fn metadata(style: &str) -> CueMetadata {
    CueMetadata::new(vec![
        ("Style".into(), style.into()),
        ("Layer".into(), " 3".into()),
        ("Unknown".into(), "preserve me".into()),
    ])
    .unwrap()
}

#[test]
fn style_assignment_preserves_field_padding_and_other_metadata() {
    for (before, after) in [
        ("Title", "日本語"),
        (" Title", " 日本語"),
        (" \tTitle\t  ", " \t日本語\t  "),
    ] {
        let original = metadata(before);
        let changed = original.with_style("日本語".into()).unwrap();
        assert_eq!(changed, metadata(after));
        assert_eq!(original.get("Style"), Some(before));
    }
}

#[test]
fn metadata_is_accepted_state_and_follows_every_history_path() {
    let cue = Cue::new(CueId(1), ms(1000), ms(3000), "text".into())
        .unwrap()
        .with_metadata(metadata("Default"));
    let original = Document::new(vec![cue]).unwrap();
    let mut editor = Editor::new(original.clone());
    editor
        .apply_edit(
            0,
            Edit::Style {
                id: CueId(1),
                name: "Title".into(),
            },
            |_| Ok(()),
        )
        .unwrap();
    assert_eq!(
        editor.document().cues()[0].metadata().unwrap().get("STYLE"),
        Some("Title")
    );
    editor.mark_saved(1).unwrap();
    editor.undo(1).unwrap();
    assert_eq!(editor.document(), &original);
    assert!(editor.dirty());
    editor.redo(2).unwrap();
    assert!(!editor.dirty());
    editor
        .edit(3, CueId(1), ms(1100), ms(3100), "edited".into())
        .unwrap();
    let expected = metadata("Title");
    assert_eq!(editor.document().cues()[0].metadata(), Some(&expected));
    let second = editor.split(4, CueId(1), ms(2000)).unwrap();
    assert!(
        editor
            .document()
            .cues()
            .iter()
            .all(|c| c.metadata() == Some(&expected))
    );
    editor.merge(5, CueId(1), second).unwrap();
    assert_eq!(editor.document().cues()[0].metadata(), Some(&expected));
    editor.undo(6).unwrap();
    assert_eq!(editor.document().cues().len(), 2);
    editor.delete(7, second).unwrap();
    editor.undo(8).unwrap();
    assert_eq!(editor.document().cues()[1].metadata(), Some(&expected));
}
#[test]
fn rejected_style_and_incompatible_merge_preserve_history_and_state() {
    let rows = ["Default", "Title"]
        .into_iter()
        .enumerate()
        .map(|(i, s)| {
            Cue::new(
                CueId(i as u64 + 1),
                ms(i as u64 * 2000),
                ms(i as u64 * 2000 + 1000),
                "x".into(),
            )
            .unwrap()
            .with_metadata(metadata(s))
        })
        .collect();
    let original = Document::new(rows).unwrap();
    let mut editor = Editor::new(original.clone());
    assert!(editor.merge(0, CueId(1), CueId(2)).is_err());
    assert!(
        editor
            .apply_edit(
                0,
                Edit::Style {
                    id: CueId(1),
                    name: "bad,name".into()
                },
                |_| Ok(())
            )
            .is_err()
    );
    assert!(
        editor
            .apply_edit(
                0,
                Edit::Style {
                    id: CueId(1),
                    name: "Title".into()
                },
                |_| Err("rejected format".into())
            )
            .is_err()
    );
    assert_eq!(editor.document(), &original);
    assert_eq!(editor.revision(), 0);
    assert!(!editor.can_undo() && !editor.dirty());
    assert!(
        CueMetadata::new(vec![
            ("Style".into(), "a".into()),
            ("STYLE".into(), "b".into())
        ])
        .is_err()
    );
}
