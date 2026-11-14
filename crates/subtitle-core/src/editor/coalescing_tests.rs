use super::*;

fn editor() -> Editor {
    Editor::new(
        Document::new(
            (1..=2)
                .map(|id| {
                    Cue::new(
                        CueId(id),
                        Milliseconds::new(id * 1000).unwrap(),
                        Milliseconds::new(id * 1000 + 500).unwrap(),
                        format!("Original {id}"),
                    )
                    .unwrap()
                })
                .collect(),
        )
        .unwrap(),
    )
}
fn edit(e: &Editor, id: u64, text: &str) -> Edit {
    let cue = e
        .document()
        .cues()
        .iter()
        .find(|c| c.id() == CueId(id))
        .unwrap();
    Edit::Text {
        id: cue.id(),
        start: cue.start(),
        end: cue.end(),
        text: text.into(),
    }
}
fn commit(e: &mut Editor, id: u64, text: &str, session: &str, at: Instant) {
    e.apply_text_commit(
        e.revision(),
        edit(e, id, text),
        TextCommit {
            session: TextEditSession(session.into()),
            at,
        },
        |_, _| Ok(()),
    )
    .unwrap();
}

#[test]
fn adjacent_text_commits_restore_exact_before_and_after_with_fresh_revisions() {
    let mut e = editor();
    let original = e.document().clone();
    let now = Instant::now();
    commit(&mut e, 1, "First", "focus-a", now);
    commit(
        &mut e,
        1,
        "Final é😀",
        "focus-a",
        now + Duration::from_secs(1),
    );
    assert_eq!(e.revision(), 2);
    assert_eq!(e.undo.len(), 1);
    let final_document = e.document().clone();
    e.undo(2).unwrap();
    assert_eq!(e.document(), &original);
    assert_eq!(e.revision(), 3);
    assert!(!e.dirty());
    e.redo(3).unwrap();
    assert_eq!(e.document(), &final_document);
    assert_eq!(e.revision(), 4);
    assert!(e.dirty());
}

#[test]
fn focus_cue_and_idle_boundaries_do_not_merge() {
    let now = Instant::now();
    for (id, session, at) in [
        (1, "focus-b", now + Duration::from_secs(1)),
        (2, "focus-a", now + Duration::from_secs(1)),
        (
            1,
            "focus-a",
            now + TEXT_COALESCE_WINDOW + Duration::from_millis(1),
        ),
        (1, "focus-a", now - Duration::from_millis(1)),
    ] {
        let mut e = editor();
        commit(&mut e, 1, "First", "focus-a", now);
        let first = e.document().clone();
        commit(&mut e, id, "Second", session, at);
        assert_eq!(e.undo.len(), 2);
        e.undo(2).unwrap();
        assert_eq!(e.document(), &first);
    }
}

#[test]
fn timing_and_ungrouped_commands_seal_text_history() {
    let now = Instant::now();
    let mut e = editor();
    commit(&mut e, 1, "First", "focus", now);
    e.apply_text_commit(
        e.revision(),
        Edit::Text {
            id: CueId(1),
            start: Milliseconds::new(1100).unwrap(),
            end: Milliseconds::new(1500).unwrap(),
            text: "Timed".into(),
        },
        TextCommit {
            session: TextEditSession("focus".into()),
            at: now,
        },
        |_, _| Ok(()),
    )
    .unwrap();
    commit(&mut e, 1, "Third", "focus", now);
    assert_eq!(e.undo.len(), 3);
    let unchanged = edit(&e, 1, "Third");
    e.apply_edit(e.revision(), unchanged, |_| Ok(())).unwrap();
    commit(&mut e, 1, "After explicit command", "focus", now);
    assert_eq!(e.undo.len(), 4);
    e.split(e.revision(), CueId(1), Milliseconds::new(1300).unwrap())
        .unwrap();
    commit(&mut e, 1, "After split", "focus", now);
    assert_eq!(e.undo.len(), 6);
}

#[test]
fn save_and_undo_redo_seal_groups_and_preserve_saved_checkpoint() {
    let now = Instant::now();
    let mut e = editor();
    commit(&mut e, 1, "Saved", "focus", now);
    e.mark_saved(1).unwrap();
    commit(&mut e, 1, "Later", "focus", now);
    assert_eq!(e.undo.len(), 2);
    e.undo(2).unwrap();
    assert!(!e.dirty());
    assert_eq!(e.document().cues()[0].text(), "Saved");
    e.redo(3).unwrap();
    commit(&mut e, 1, "After redo", "focus", now);
    assert_eq!(e.undo.len(), 3);
    e.undo(5).unwrap();
    assert_eq!(e.document().cues()[0].text(), "Later");
}

#[test]
fn rejected_and_stale_commits_preserve_group_content_history_and_revision() {
    let now = Instant::now();
    let mut e = editor();
    commit(&mut e, 1, "First", "focus", now);
    let accepted = e.document().clone();
    for expected in [0, 1] {
        assert!(
            e.apply_text_commit(
                expected,
                edit(&e, 1, "Rejected"),
                TextCommit {
                    session: TextEditSession("focus".into()),
                    at: now,
                },
                |_, _| Err("Format rejects candidate".into())
            )
            .is_err()
        );
        assert_eq!(e.document(), &accepted);
        assert_eq!(e.revision(), 1);
        assert_eq!(e.undo.len(), 1);
    }
    commit(&mut e, 1, "Last", "focus", now);
    assert_eq!(e.undo.len(), 1);
    e.undo(2).unwrap();
    assert_eq!(e.document().cues()[0].text(), "Original 1");
}

#[test]
fn metadata_and_property_changes_never_coalesce_as_text() {
    let now = Instant::now();
    let mut e = editor();
    e.document.cues[0].metadata =
        Some(CueMetadata::new(vec![("Style".into(), "Default".into())]).unwrap());
    commit(&mut e, 1, "First", "focus", now);
    e.apply_text_commit(
        e.revision(),
        Edit::StyledText {
            id: CueId(1),
            start: Milliseconds::new(1000).unwrap(),
            end: Milliseconds::new(1500).unwrap(),
            text: "Styled".into(),
            style: "Other".into(),
        },
        TextCommit {
            session: TextEditSession("focus".into()),
            at: now,
        },
        |_, _| Ok(()),
    )
    .unwrap();
    commit(&mut e, 1, "Later", "focus", now);
    assert_eq!(e.undo.len(), 3);
    let same = e.document().clone();
    e.apply_document(e.revision(), same, (), |_, _| Ok(()))
        .unwrap();
    commit(&mut e, 1, "After document command", "focus", now);
    assert_eq!(e.undo.len(), 4);
}

#[test]
fn net_zero_text_group_restores_saved_state_without_an_empty_undo_step() {
    let now = Instant::now();
    let mut e = editor();
    let original = e.document().clone();
    commit(&mut e, 1, "Original 1!", "focus", now);
    commit(&mut e, 1, "Original 1", "focus", now);
    assert_eq!(e.document(), &original);
    assert_eq!(e.revision(), 2);
    assert!(!e.can_undo());
    assert!(!e.can_redo());
    assert!(!e.dirty());
    assert!(e.text_group.is_none());
    commit(&mut e, 1, "New", "focus", now);
    assert_eq!(e.state, 3);
    e.mark_saved(3).unwrap();
    commit(&mut e, 1, "New!", "focus", now);
    e.undo(4).unwrap();
    assert!(!e.dirty());
    e.redo(5).unwrap();
    assert_eq!(e.document().cues()[0].text(), "New!");
    assert!(e.dirty());
}

#[test]
fn net_zero_group_preserves_prior_history_and_cannot_rejoin_it_or_revive_redo() {
    let now = Instant::now();
    for saved in [false, true] {
        let mut e = editor();
        commit(&mut e, 1, "Earlier", "earlier", now);
        if saved {
            e.mark_saved(e.revision()).unwrap();
        }
        let before = e.document().clone();
        let prior_state = e.state;
        commit(&mut e, 1, "Temporary", "focus", now);
        commit(&mut e, 1, "Earlier", "focus", now);
        assert_eq!(e.document(), &before);
        assert_eq!(e.undo.len(), 1);
        assert_eq!(e.state, prior_state);
        assert_eq!(e.dirty(), !saved);
        commit(&mut e, 1, "Following", "focus", now);
        assert_eq!(e.undo.len(), 2);
        e.undo(e.revision()).unwrap();
        assert_eq!(e.document(), &before);
        assert!(e.can_redo());
        commit(&mut e, 1, "New branch", "focus", now);
        commit(&mut e, 1, "Earlier", "focus", now);
        assert!(!e.can_redo());
        assert_eq!(e.undo.len(), 1);
        assert_eq!(e.dirty(), !saved);
        e.undo(e.revision()).unwrap();
        assert_eq!(e.document().cues()[0].text(), "Original 1");
        e.redo(e.revision()).unwrap();
        assert_eq!(e.document(), &before);
        assert_eq!(e.dirty(), !saved);
    }
}
