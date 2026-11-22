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
        Default::default(),
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
fn safe_fix_after_typing_retains_one_cue_and_separate_undo() {
    let mut cues = document("Draft").cues().to_vec();
    cues.insert(
        0,
        Cue::new(
            CueId(3),
            Milliseconds::new(0).unwrap(),
            Milliseconds::new(500).unwrap(),
            "Previous".into(),
        )
        .unwrap(),
    );
    let before_typing = Document::new(cues).unwrap();
    let mut editor = Editor::new(before_typing.clone());
    let selected = &before_typing.cues()[1];
    editor
        .apply_text_commit(
            0,
            Edit::Text {
                id: selected.id(),
                start: selected.start(),
                end: selected.end(),
                text: "This subtitle has enough words to need a sensible line break here.".into(),
            },
            subtitle_core::TextCommit {
                session: subtitle_core::TextEditSession("typing".into()),
                at: std::time::Instant::now(),
            },
            |_, _| Ok(()),
        )
        .unwrap();
    let typed = editor.document().clone();
    let plan = propose(
        typed.clone(),
        (&profile()).into(),
        &scene::Scenes::NoMedia,
        selected.id(),
        Default::default(),
        || Ok(()),
    )
    .unwrap();
    let fix = plan.edit(&typed).unwrap();
    assert!(matches!(fix, Edit::Text { id, .. } if id == selected.id()));
    editor.apply_edit(1, fix, |_| Ok(())).unwrap();
    let fixed = editor.document().clone();
    assert_eq!(fixed.cues()[0], typed.cues()[0]);
    assert_eq!(fixed.cues()[2], typed.cues()[2]);
    assert_ne!(fixed.cues()[1], typed.cues()[1]);
    assert!(plan.edit(&fixed).is_err());
    editor.undo(2).unwrap();
    assert_eq!(editor.document(), &typed);
    editor.redo(3).unwrap();
    assert_eq!(editor.document(), &fixed);
    editor.undo(4).unwrap();
    editor.undo(5).unwrap();
    assert_eq!(editor.document(), &before_typing);
}

#[test]
fn safe_fix_rejects_missing_selection_and_neighbor_changes() {
    let before = document("original");
    let mut plan = Proposal {
        before: before.clone(),
        after: Document::new(vec![before.cues()[1].clone()]).unwrap(),
        cue_id: CueId(1),
        fixed: 1,
        remaining: 0,
    };
    assert!(matches!(plan.edit(&before), Err(Error::Invalid(_))));
    plan.after = document("changed neighbor");
    plan.cue_id = CueId(2);
    assert!(matches!(plan.edit(&before), Err(Error::Invalid(_))));
}
#[test]
fn no_media_fixes_match_current_cue_and_incomplete_video_blocks() {
    let document = document("Short");
    let mut profile = profile();
    profile.timing.scene_before_ms = 100;
    profile.timing.scene_after_ms = 100;
    let plan = propose(
        document.clone(),
        (&profile).into(),
        &scene::Scenes::NoMedia,
        CueId(1),
        Default::default(),
        || Ok(()),
    )
    .unwrap();
    let context = (&profile).into();
    let request = timing_fix::Request {
        scope: Some(vec![CueId(1)]),
        ..Default::default()
    };
    let dock = timing_fix::propose(
        document.clone(),
        context,
        &scene::Scenes::NoMedia,
        request,
        || Ok(()),
    )
    .unwrap();
    assert_eq!(
        plan.after,
        dock.select(&dock.default_units(), || Ok(()))
            .unwrap()
            .candidate
    );
    assert!(plan.fixed > 0);
    let blocked = propose(
        document.clone(),
        context,
        &scene::Scenes::Partial,
        CueId(1),
        Default::default(),
        || Ok(()),
    )
    .unwrap();
    assert!(blocked.edit(&document).is_err());
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
            Default::default(),
            || Err(Error::Cancelled)
        ),
        Err(Error::Cancelled)
    ));
    let plan = propose(
        document.clone(),
        (&profile).into(),
        &scene::Scenes::NoMedia,
        CueId(1),
        Default::default(),
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

#[test]
fn current_cue_obeys_saved_spacing_priority_even_when_a_speed_finding_remains() {
    let mut profile = profile();
    profile.rules.speed.maximum_rs_exclusive = None;
    profile.timing.cue_gap_ms = 80;
    let document = Document::new(vec![
        Cue::new(
            CueId(1),
            Milliseconds::new(0).unwrap(),
            Milliseconds::new(1000).unwrap(),
            "12345678901234567890".into(),
        )
        .unwrap(),
        Cue::new(
            CueId(2),
            Milliseconds::new(1000).unwrap(),
            Milliseconds::new(3000).unwrap(),
            "Next".into(),
        )
        .unwrap(),
    ])
    .unwrap();
    for priority in [
        timing_fix::Priority::ReadingSpeed,
        timing_fix::Priority::Spacing,
    ] {
        let options = timing_fix::Options {
            priority,
            ..Default::default()
        };
        let plan = propose(
            document.clone(),
            (&profile).into(),
            &scene::Scenes::NoMedia,
            CueId(1),
            options,
            || Ok(()),
        )
        .unwrap();
        let request = timing_fix::Request {
            scope: Some(vec![CueId(1)]),
            options,
            ..Default::default()
        };
        let dock = timing_fix::propose(
            document.clone(),
            (&profile).into(),
            &scene::Scenes::NoMedia,
            request,
            || Ok(()),
        )
        .unwrap();
        let selected = dock.select(&dock.default_units(), || Ok(())).unwrap();
        assert_eq!(plan.after, selected.candidate);
        assert_eq!(
            plan.edit(&document).is_ok(),
            priority == timing_fix::Priority::Spacing
        );
        assert_eq!(plan.after.cues()[1], document.cues()[1]);
    }
}
