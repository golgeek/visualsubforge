use subtitle_core::{Cue, CueId, CueMetadata, CueUpdate, Document, Edit, Editor, Milliseconds};

fn ms(value: u64) -> Milliseconds {
    Milliseconds::new(value).unwrap()
}
fn fixture() -> Editor<String> {
    Editor::with_properties(
        Document::new(
            (1..=3)
                .map(|id| {
                    Cue::new(
                        CueId(id),
                        ms(id * 1000),
                        ms(id * 1000 + 900),
                        format!("{{\\i1}}Été {id} 😀"),
                    )
                    .unwrap()
                    .with_metadata(
                        CueMetadata::new(vec![
                            ("Style".into(), "Title".into()),
                            ("Extra".into(), "retained".into()),
                        ])
                        .unwrap(),
                    )
                })
                .collect(),
        )
        .unwrap(),
        "source properties".into(),
    )
}
fn updates(e: &Editor<String>) -> Vec<CueUpdate> {
    e.document()
        .cues()
        .iter()
        .map(|c| CueUpdate {
            id: c.id(),
            start: c.start(),
            end: c.end(),
            text: c.text().into(),
        })
        .collect()
}

#[test]
fn combined_batch_is_one_history_step_preserving_metadata_and_properties() {
    let mut e = fixture();
    let before = e.document().clone();
    let mut cues = updates(&e);
    cues[0].end = ms(1800);
    cues[0].text.push_str(" corrected");
    cues[2].start = ms(3100);
    cues[2].text = "第三行".into();
    e.apply_edit_with_properties(0, Edit::UpdateBatch { cues }, |d, p| {
        assert_eq!(p, "source properties");
        assert_eq!(d.cues()[0].end(), ms(1800));
        assert_eq!(d.cues()[2].text(), "第三行");
        Ok(())
    })
    .unwrap();
    let after = e.document().clone();
    for (a, b) in before.cues().iter().zip(after.cues()) {
        assert_eq!(a.id(), b.id());
        assert_eq!(a.metadata(), b.metadata());
    }
    assert_eq!(before.cues()[1], after.cues()[1]);
    e.mark_saved(1).unwrap();
    e.undo(1).unwrap();
    assert_eq!(e.document(), &before);
    assert!(e.dirty());
    assert!(!e.can_undo());
    e.redo(2).unwrap();
    assert_eq!(e.document(), &after);
    assert!(!e.dirty());
    assert!(!e.can_redo());
    assert_eq!(e.properties(), "source properties");
}

#[test]
fn invalid_late_entry_policy_staleness_and_noop_preserve_history() {
    let mut e = fixture();
    let mut changed = updates(&e);
    changed[0].text = "changed".into();
    e.apply_edit(
        0,
        Edit::UpdateBatch {
            cues: changed.clone(),
        },
        |_| Ok(()),
    )
    .unwrap();
    e.undo(1).unwrap();
    let before = e.document().clone();
    let mut cases = Vec::new();
    for invalid in ["NUL\0", "bare\rCR"] {
        let mut c = changed.clone();
        c[2].text = invalid.into();
        cases.push(c);
    }
    let mut c = changed.clone();
    c[2].end = c[2].start;
    cases.push(c);
    let mut c = changed.clone();
    c[2].id = c[1].id;
    cases.push(c);
    let mut c = changed.clone();
    c.pop();
    cases.push(c);
    let mut c = changed.clone();
    c.swap(0, 1);
    cases.push(c);
    let mut c = changed.clone();
    c[2].id = CueId(99);
    cases.push(c);
    for cues in cases {
        assert!(
            e.apply_edit(2, Edit::UpdateBatch { cues }, |_| panic!(
                "invalid domain candidate"
            ))
            .is_err()
        );
    }
    assert!(
        e.apply_edit(
            0,
            Edit::UpdateBatch {
                cues: changed.clone()
            },
            |_| Ok(())
        )
        .is_err()
    );
    assert!(
        e.apply_edit_with_properties(2, Edit::UpdateBatch { cues: changed }, |_, p| {
            assert_eq!(p, "source properties");
            Err("format/size policy".into())
        })
        .is_err()
    );
    let same = updates(&e);
    assert!(
        !e.apply_edit(2, Edit::UpdateBatch { cues: same }, |_| panic!("no-op"))
            .unwrap()
            .changed
    );
    assert_eq!(e.document(), &before);
    assert_eq!(e.revision(), 2);
    assert!(!e.dirty());
    assert!(e.can_redo());
    e.redo(2).unwrap();
    assert_eq!(e.document().cues()[0].text(), "changed");
}
