use super::*;
use crate::quality::Profile;
use subtitle_core::{Editor, Milliseconds};
fn document(text: &str) -> Document {
    Document::new(vec![
        Cue::new(
            CueId(1),
            Milliseconds::new(1000).unwrap(),
            Milliseconds::new(1500).unwrap(),
            text.into(),
        )
        .unwrap(),
        Cue::new(
            CueId(2),
            Milliseconds::new(10000).unwrap(),
            Milliseconds::new(10500).unwrap(),
            "Neighbor".into(),
        )
        .unwrap(),
    ])
    .unwrap()
}
fn profile() -> Profile {
    let mut profile = Profile::default().custom("cue-fixes".into(), "Cue fixes".into());
    profile.timing.scene_before_ms = 0;
    profile.timing.scene_after_ms = 0;
    profile.rules.speed.minimum_cps = None;
    profile.rules.speed.minimum_rs = None;
    profile.rules.speed.maximum_cps = Some(20);
    profile.rules.speed.maximum_rs_exclusive = Some(35);
    profile
}
#[test]
fn combined_reflow_and_timing_are_one_exact_undo_without_changing_neighbors() {
    let document = document("This subtitle has enough words to need a sensible line break here.");
    let profile = profile();
    let plan = propose(
        document.clone(),
        (&profile).into(),
        &scene::Scenes::NoMedia,
        CueId(1),
        || Ok(()),
    )
    .unwrap();
    assert!(
        plan.fixed >= 2,
        "fixed {}, remaining {}",
        plan.fixed,
        plan.remaining
    );
    let mut editor = Editor::new(document.clone());
    editor
        .apply_edit(0, plan.edit(&document).unwrap(), |_| Ok(()))
        .unwrap();
    let accepted = editor.document().clone();
    assert!(accepted.cues()[0].text().contains('\n'));
    assert!(accepted.cues()[0].end() > document.cues()[0].end());
    assert_eq!(accepted.cues()[1], document.cues()[1]);
    assert_eq!(accepted.cues().len(), document.cues().len());
    editor.undo(1).unwrap();
    assert_eq!(editor.document(), &document);
    assert!(!editor.can_undo());
    editor.redo(2).unwrap();
    assert_eq!(editor.document(), &accepted);
    assert!(plan.edit(&accepted).is_err());
}
#[test]
fn missing_scene_coverage_cannot_be_acknowledged_by_quick_fix() {
    let document = document("Short");
    let mut profile = profile();
    profile.timing.scene_before_ms = 100;
    profile.timing.scene_after_ms = 100;
    let plan = propose(
        document.clone(),
        (&profile).into(),
        &scene::Scenes::NoMedia,
        CueId(1),
        || Ok(()),
    )
    .unwrap();
    assert_eq!(plan.fixed, 0);
    assert!(plan.edit(&document).is_err());
}
#[test]
fn cancellation_and_format_validation_publish_no_partial_changes() {
    let document = document("This subtitle has enough words to need a sensible line break here.");
    let profile = profile();
    assert!(matches!(
        propose(
            document.clone(),
            (&profile).into(),
            &scene::Scenes::NoMedia,
            CueId(1),
            || Err(Error::Cancelled)
        ),
        Err(Error::Cancelled)
    ));
    let plan = propose(
        document.clone(),
        (&profile).into(),
        &scene::Scenes::NoMedia,
        CueId(1),
        || Ok(()),
    )
    .unwrap();
    let mut editor = Editor::new(document.clone());
    assert!(
        editor
            .apply_edit(0, plan.edit(&document).unwrap(), |_| Err(
                "Format rejected".into()
            ))
            .is_err()
    );
    assert_eq!(editor.document(), &document);
    assert!(!editor.can_undo());
    assert_eq!(editor.revision(), 0);
}
