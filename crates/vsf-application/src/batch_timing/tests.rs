use super::*;
use subtitle_core::{Cue, CueMetadata, Editor};
fn ms(value: u64) -> Milliseconds {
    Milliseconds::new(value).unwrap()
}
fn fixture() -> Document {
    Document::new(
        [3000, 1000, 5000]
            .into_iter()
            .enumerate()
            .map(|(i, start)| {
                Cue::new(
                    CueId(i as u64 + 1),
                    ms(start),
                    ms(start + 800),
                    r"{\k20\t(0,300,\fs40)\move(1,2,3,4,0,500)}é😀".into(),
                )
                .unwrap()
                .with_metadata(
                    CueMetadata::new(vec![("Style".into(), " Original ".into())]).unwrap(),
                )
            })
            .collect(),
    )
    .unwrap()
}
fn plan(scope: Scope, change: Change, format: Format) -> Result<Proposal> {
    propose(fixture(), format, Request { scope, change }, || Ok(()))
}
#[test]
fn disjoint_shift_preserves_relative_effects_metadata_order_and_one_step_history() {
    let document = fixture();
    let p = plan(
        Scope::Selected(vec![CueId(3), CueId(1)]),
        Change::Shift(-123),
        Format::Srt,
    )
    .unwrap();
    assert_eq!(
        p.rows.iter().map(|r| r.cue_index).collect::<Vec<_>>(),
        [0, 2]
    );
    let mut editor = Editor::new(document.clone());
    editor
        .apply_edit(0, p.edit(&document).unwrap(), |_| Ok(()))
        .unwrap();
    for (i, (old, new)) in document
        .cues()
        .iter()
        .zip(editor.document().cues())
        .enumerate()
    {
        assert_eq!(
            (old.id(), old.text(), old.metadata()),
            (new.id(), new.text(), new.metadata())
        );
        assert_eq!(
            old.end().get() - old.start().get(),
            new.end().get() - new.start().get()
        );
        assert_eq!(
            new.start().get(),
            old.start().get() - if i == 1 { 0 } else { 123 }
        );
    }
    let changed = editor.document().clone();
    assert!(p.edit(&changed).is_err());
    editor.undo(1).unwrap();
    assert_eq!(editor.document(), &document);
    assert!(!editor.can_undo());
    editor.redo(2).unwrap();
    assert_eq!(editor.document(), &changed);
}
#[test]
fn scopes_distinguish_file_order_and_time_and_alignment_uses_earliest_selected() {
    let following = plan(Scope::Following(CueId(2)), Change::Shift(10), Format::Ass).unwrap();
    assert_eq!(
        following
            .rows
            .iter()
            .map(|r| r.cue_index)
            .collect::<Vec<_>>(),
        [1, 2]
    );
    let temporal = plan(Scope::AtOrAfter(ms(3000)), Change::Shift(10), Format::Ssa).unwrap();
    assert_eq!(
        temporal
            .rows
            .iter()
            .map(|r| r.cue_index)
            .collect::<Vec<_>>(),
        [0, 2]
    );
    let aligned = plan(
        Scope::Selected(vec![CueId(1), CueId(2)]),
        Change::Align(ms(1015)),
        Format::Ass,
    )
    .unwrap();
    assert_eq!(aligned.aligned_ms, Some(ms(1020)));
    assert_eq!(aligned.offset_ms, 20);
    assert_eq!(aligned.rows[0].after.start, ms(3020));
    assert_eq!(
        plan(Scope::All, Change::Align(ms(1015)), Format::Srt)
            .unwrap()
            .offset_ms,
        15
    );
}
#[test]
fn bad_scope_precision_overflow_and_negative_times_fail_before_any_edit() {
    for scope in [
        Scope::Selected(vec![]),
        Scope::Selected(vec![CueId(1), CueId(1)]),
        Scope::Selected(vec![CueId(4)]),
        Scope::Following(CueId(4)),
        Scope::AtOrAfter(ms(6000)),
    ] {
        assert!(plan(scope, Change::Shift(10), Format::Srt).is_err());
    }
    for offset in [0, -1001, i64::MIN, i64::MAX, Milliseconds::MAX as i64] {
        assert!(plan(Scope::All, Change::Shift(offset), Format::Srt).is_err());
    }
    for format in [Format::Ass, Format::Ssa] {
        assert!(plan(Scope::All, Change::Shift(1), format).is_err());
        assert!(plan(Scope::All, Change::Align(ms(Milliseconds::MAX)), format).is_err());
    }
    assert!(plan(Scope::All, Change::Shift(-1000), Format::Srt).is_ok());
}
#[test]
fn cancellation_is_observed_during_selection_and_endpoint_planning() {
    for stop in 1..=8 {
        let mut calls = 0;
        let result = propose(
            fixture(),
            Format::Srt,
            Request {
                scope: Scope::All,
                change: Change::Shift(1),
            },
            || {
                calls += 1;
                if calls == stop {
                    Err(Error::Cancelled)
                } else {
                    Ok(())
                }
            },
        );
        assert!(matches!(result, Err(Error::Cancelled)));
    }
}
