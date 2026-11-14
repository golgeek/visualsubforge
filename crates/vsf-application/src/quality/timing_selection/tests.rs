use super::*;
use crate::quality::Profile;
use subtitle_core::{Cue, Editor, Milliseconds};

fn document() -> Document {
    Document::new(
        [(1000, 3000), (3100, 5100), (5400, 7400)]
            .into_iter()
            .enumerate()
            .map(|(index, (start, end))| {
                Cue::new(
                    CueId(index as u64 + 1),
                    Milliseconds::new(start).unwrap(),
                    Milliseconds::new(end).unwrap(),
                    format!("Cue {index}"),
                )
                .unwrap()
            })
            .collect(),
    )
    .unwrap()
}
fn profile() -> Profile {
    let mut profile =
        Profile::default().custom("timing-selection".into(), "Timing selection".into());
    profile.timing.scene_before_ms = 0;
    profile.timing.scene_after_ms = 0;
    profile.timing.cue_gap_ms = 200;
    profile.rules.speed.minimum_cps = None;
    profile.rules.speed.minimum_rs = None;
    profile
}
fn shifted(document: &Document, offset: u64) -> Edit {
    Edit::Retime {
        timings: document
            .cues()
            .iter()
            .map(|cue| CueTiming {
                id: cue.id(),
                start: Milliseconds::new(cue.start().get() + offset).unwrap(),
                end: Milliseconds::new(cue.end().get() + offset).unwrap(),
            })
            .collect(),
    }
}
#[test]
fn selected_repair_is_one_exact_history_step_and_preserves_other_cues() {
    let document = document();
    let profile = profile();
    let context = (&profile).into();
    let before =
        evaluate_with_scenes(&document, context, &scene::Scenes::NoMedia, |_, _| Ok(())).unwrap();
    let edit = Edit::Retime {
        timings: document
            .cues()
            .iter()
            .map(|cue| CueTiming {
                id: cue.id(),
                start: cue.start(),
                end: Milliseconds::new(cue.end().get() - 100).unwrap(),
            })
            .collect(),
    };
    let plan = prepare_repair(
        &document,
        &before,
        edit,
        Selection {
            scope: Some(vec![CueId(1), CueId(2)]),
            excluded: vec![CueId(2)],
        },
        context,
        &scene::Scenes::NoMedia,
        || Ok(()),
    )
    .unwrap();
    assert_eq!(plan.changed, 1);
    assert!(plan.error.is_none());
    assert!(!plan.selected(CueId(2)));
    assert!(!plan.in_scope(CueId(3)));
    let mut editor = Editor::new(document.clone());
    editor
        .apply_edit(0, plan.edit(&document).unwrap(), |_| Ok(()))
        .unwrap();
    let accepted = editor.document().clone();
    assert_eq!(accepted.cues()[0].end().get(), 2900);
    assert_eq!(&accepted.cues()[1..], &document.cues()[1..]);
    editor.undo(1).unwrap();
    assert_eq!(editor.document(), &document);
    assert!(!editor.can_undo());
    editor.redo(2).unwrap();
    assert_eq!(editor.document(), &accepted);
    assert!(plan.edit(&accepted).is_err());
}
#[test]
fn coupled_subset_remains_reviewable_but_cannot_be_accepted() {
    let document = document();
    let profile = profile();
    let context = (&profile).into();
    let before =
        evaluate_with_scenes(&document, context, &scene::Scenes::NoMedia, |_, _| Ok(())).unwrap();
    let selection = Selection {
        scope: Some(vec![CueId(1)]),
        excluded: vec![],
    };
    let plan = prepare_repair(
        &document,
        &before,
        shifted(&document, 500),
        selection.clone(),
        context,
        &scene::Scenes::NoMedia,
        || Ok(()),
    )
    .unwrap();
    assert_eq!(plan.changed, 1);
    assert!(plan.error.is_some());
    assert!(plan.edit(&document).is_err());
    let complete = prepare_repair(
        &document,
        &before,
        shifted(&document, 500),
        Selection::default(),
        context,
        &scene::Scenes::NoMedia,
        || Ok(()),
    )
    .unwrap();
    assert!(complete.error.is_none());
    assert_eq!(complete.changed, 3);
    // Explicit batch shifts deliberately allow new overlaps, unlike repairs.
    let shift = prepare_shift(&document, shifted(&document, 500), selection, || Ok(())).unwrap();
    assert!(shift.edit(&document).is_ok());
}
#[test]
fn invalid_empty_and_cancelled_selections_publish_no_edit() {
    let document = document();
    for selection in [
        Selection {
            scope: Some(vec![CueId(99)]),
            excluded: vec![],
        },
        Selection {
            scope: None,
            excluded: vec![CueId(1), CueId(1)],
        },
    ] {
        assert!(prepare_shift(&document, shifted(&document, 500), selection, || Ok(())).is_err());
    }
    let empty = prepare_shift(
        &document,
        shifted(&document, 500),
        Selection {
            scope: Some(vec![]),
            excluded: vec![],
        },
        || Ok(()),
    )
    .unwrap();
    assert_eq!(empty.changed, 0);
    assert!(empty.edit(&document).is_err());
    assert!(matches!(
        prepare_shift(
            &document,
            shifted(&document, 500),
            Selection::default(),
            || Err(Error::Cancelled)
        ),
        Err(Error::Cancelled)
    ));
    let plan = prepare_shift(
        &document,
        shifted(&document, 500),
        Selection::default(),
        || Ok(()),
    )
    .unwrap();
    let mut editor = Editor::new(document.clone());
    assert!(
        editor
            .apply_edit(0, plan.edit(&document).unwrap(), |_| Err(
                "Source format rejection".into()
            ))
            .is_err()
    );
    assert_eq!(editor.document(), &document);
    assert_eq!(editor.revision(), 0);
    assert!(!editor.can_undo());
}
