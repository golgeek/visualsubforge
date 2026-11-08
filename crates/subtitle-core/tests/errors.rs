use subtitle_core::{Cue, CueId, Document, Edit, Editor, Error, Milliseconds};

fn ms(value: u64) -> Milliseconds {
    Milliseconds::new(value).unwrap()
}

#[test]
fn typed_rejections_preserve_redo_revision_saved_state_and_id_allocation() {
    let mut editor = Editor::new(
        Document::new(vec![
            Cue::new(CueId(1), ms(1000), ms(2000), "Original".into()).unwrap(),
        ])
        .unwrap(),
    );
    editor.split(0, CueId(1), ms(1500)).unwrap();
    editor.undo(1).unwrap();
    let original = editor.document().clone();
    let failures = [
        (
            1,
            Edit::Delete { id: CueId(1) },
            Error::StaleRevision {
                expected: 1,
                actual: 2,
            },
        ),
        (
            2,
            Edit::Delete { id: CueId(999) },
            Error::MissingCue { id: CueId(999) },
        ),
        (
            2,
            Edit::Create {
                after: None,
                start: ms(2000),
                end: ms(1000),
                text: "Bad".into(),
                metadata: None,
            },
            Error::InvalidTimeRange {
                start: ms(2000),
                end: ms(1000),
            },
        ),
        (
            2,
            Edit::Split {
                id: CueId(1),
                at: ms(2000),
            },
            Error::InvalidSplit {
                id: CueId(1),
                at: ms(2000),
                start: ms(1000),
                end: ms(2000),
            },
        ),
    ];
    for (revision, edit, expected) in failures {
        assert_eq!(
            editor
                .apply_edit(revision, edit, |_| panic!(
                    "invalid domain edit must not reach policy"
                ))
                .err(),
            Some(expected)
        );
        assert_eq!(editor.document(), &original);
        assert_eq!(editor.revision(), 2);
        assert!(!editor.dirty());
        assert!(!editor.can_undo());
        assert!(editor.can_redo());
    }
    let rejected = editor.apply_edit(
        2,
        Edit::Split {
            id: CueId(1),
            at: ms(1500),
        },
        |_| Err("Application size policy".into()),
    );
    assert_eq!(
        rejected.err(),
        Some(Error::ValidationRejected {
            message: "Application size policy".into()
        })
    );
    assert_eq!(editor.document(), &original);
    assert_eq!(editor.revision(), 2);
    assert!(!editor.dirty());
    assert!(editor.can_redo());
    assert_eq!(
        editor.mark_saved(0),
        Err(Error::StaleRevision {
            expected: 0,
            actual: 2
        })
    );
    editor.redo(2).unwrap();
    assert_eq!(editor.document().cues()[1].id(), CueId(2));
    editor.undo(3).unwrap();
    assert!(!editor.dirty());
    assert_eq!(editor.split(4, CueId(1), ms(1500)).unwrap(), CueId(3));
}

#[test]
fn document_validation_rejection_retains_properties_and_history() {
    let original = Document::new(vec![]).unwrap();
    let mut editor = Editor::with_properties(original.clone(), 0);
    editor
        .apply_document(0, original.clone(), 1, |_, _| Ok(()))
        .unwrap();
    editor.mark_saved(1).unwrap();
    editor.undo(1).unwrap();
    assert_eq!(
        editor.apply_document(2, original.clone(), 2, |_, _| Err("Format policy".into())),
        Err(Error::ValidationRejected {
            message: "Format policy".into()
        })
    );
    assert_eq!(editor.document(), &original);
    assert_eq!(editor.properties(), &0);
    assert_eq!(editor.revision(), 2);
    assert!(editor.dirty());
    assert!(editor.can_redo());
    editor.redo(2).unwrap();
    assert_eq!(editor.properties(), &1);
    assert!(!editor.dirty());
}

#[test]
fn construction_failures_retain_identity_and_time_context() {
    assert_eq!(
        Milliseconds::new(Milliseconds::MAX + 1),
        Err(Error::TimeOutOfRange {
            value: Milliseconds::MAX + 1
        })
    );
    let cue = Cue::new(CueId(42), ms(0), ms(1), "".into()).unwrap();
    assert_eq!(
        Document::new(vec![cue.clone(), cue]),
        Err(Error::DuplicateCue { id: CueId(42) })
    );
}
