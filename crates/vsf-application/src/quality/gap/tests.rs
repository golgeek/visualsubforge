use super::*;
use crate::quality::profile::SpeedBasis;
use subtitle_core::Cue;
use subtitle_core::{CueId, Editor};

fn ms(value: u64) -> Milliseconds {
    Milliseconds::new(value).unwrap()
}
fn document(rows: &[(u64, u64, &str)]) -> Document {
    Document::new(
        rows.iter()
            .enumerate()
            .map(|(i, &(start, end, text))| {
                Cue::new(CueId(i as u64 + 1), ms(start), ms(end), text.into()).unwrap()
            })
            .collect(),
    )
    .unwrap()
}
fn profile() -> Profile {
    let mut p = Profile::default().custom("gap-test".into(), "Gap test".into());
    p.rules.speed.enabled = false;
    p.timing.scene_before_ms = 0;
    p.timing.scene_after_ms = 0;
    p
}
fn plan(doc: &Document, profile: &Profile, scenes: &Scenes) -> Proposal {
    propose(doc.clone(), profile, scenes, || Ok(())).unwrap()
}
#[test]
fn proposal_preserves_order_text_starts_and_is_one_validated_history_edit() {
    let doc = document(&[
        (5000, 6000, "Third"),
        (1000, 2000, "{\\i1}First\\Nline"),
        (2050, 3000, "Second"),
    ]);
    let mut p = profile();
    p.timing.cue_gap_ms = 100;
    let proposal = plan(&doc, &p, &Scenes::NoMedia);
    assert_eq!(proposal.changed(), 1);
    assert_eq!(proposal.rows()[0].cue_index, 1);
    assert_eq!(proposal.rows()[0].next_cue_index, 2);
    assert_eq!(proposal.rows()[0].new_end, Some(ms(1950)));
    assert_eq!(proposal.before().metrics[1].end_ms, 2000.);
    assert_eq!(proposal.after().metrics[1].end_ms, 1950.);
    let mut editor = Editor::new(doc.clone());
    assert!(
        editor
            .apply_edit(0, proposal.edit(&doc, false).unwrap(), |_| Err(
                "Injected validation failure".into()
            ))
            .is_err()
    );
    assert_eq!(editor.document(), &doc);
    assert_eq!(editor.revision(), 0);
    editor
        .apply_edit(0, proposal.edit(&doc, false).unwrap(), |_| Ok(()))
        .unwrap();
    assert_eq!(editor.revision(), 1);
    assert_eq!(
        editor
            .document()
            .cues()
            .iter()
            .map(|c| c.end().get())
            .collect::<Vec<_>>(),
        [6000, 1950, 3000]
    );
    for (old, new) in doc.cues().iter().zip(editor.document().cues()) {
        assert_eq!(
            (old.id(), old.start(), old.text()),
            (new.id(), new.start(), new.text())
        );
    }
    assert!(proposal.edit(editor.document(), false).is_err());
    let accepted = editor.document().clone();
    assert!(editor.undo(1).unwrap());
    assert_eq!(editor.document(), &doc);
    assert!(!editor.undo(2).unwrap());
    assert!(editor.redo(2).unwrap());
    assert_eq!(editor.document(), &accepted);
}

#[test]
fn ordinary_overlaps_can_be_shortened_but_nested_and_equal_start_groups_are_skipped() {
    let p = profile();
    let doc = document(&[(0, 1200, "A"), (1100, 2300, "B"), (2200, 3500, "C")]);
    let proposal = plan(&doc, &p, &Scenes::NoMedia);
    assert_eq!(proposal.changed(), 2);
    assert_eq!(
        proposal
            .rows()
            .iter()
            .map(|r| r.new_end.unwrap().get())
            .collect::<Vec<_>>(),
        [1020, 2120]
    );
    assert!(
        !proposal
            .after()
            .findings
            .iter()
            .any(|f| matches!(f.rule, Rule::Overlap | Rule::Gap))
    );
    for doc in [
        document(&[
            (0, 3000, "Outer"),
            (1000, 2000, "Inner"),
            (2050, 4000, "Following"),
        ]),
        document(&[(1000, 2000, "A"), (1000, 3000, "B")]),
    ] {
        let proposal = plan(&doc, &p, &Scenes::NoMedia);
        assert_eq!(proposal.changed(), 0);
        assert!(
            proposal
                .rows()
                .iter()
                .all(|r| r.skipped == Some(Skip::AmbiguousOverlap))
        );
        assert!(proposal.edit(&doc, true).is_err());
    }
}

#[test]
fn minimum_duration_and_nonpositive_endpoints_are_never_silently_collapsed() {
    let mut p = profile();
    let collapse = document(&[(0, 60, "A"), (50, 2000, "B")]);
    assert_eq!(
        plan(&collapse, &p, &Scenes::NoMedia).rows()[0].skipped,
        Some(Skip::WouldCollapse)
    );
    p.timing.cue_gap_ms = 300;
    let short = document(&[(0, 1000, "A"), (1000, 2000, "B")]);
    assert_eq!(
        plan(&short, &p, &Scenes::NoMedia).rows()[0].skipped,
        Some(Skip::MinimumDuration)
    );
    p.rules.duration.enabled = false;
    assert_eq!(
        plan(&short, &p, &Scenes::NoMedia).rows()[0].new_end,
        Some(ms(700))
    );
    p.timing.cue_gap_ms = 200;
    p.rules.duration.enabled = true;
    assert_eq!(
        plan(&short, &p, &Scenes::NoMedia).rows()[0].new_end,
        Some(ms(800))
    );
    p.timing.cue_gap_ms = 0;
    assert!(plan(&short, &p, &Scenes::NoMedia).rows().is_empty());
}

#[test]
fn new_and_worsened_high_speed_findings_are_rejected() {
    let mut p = profile();
    p.rules.speed.enabled = true;
    p.rules.speed.minimum_rs = None;
    p.rules.speed.maximum_rs_exclusive = None;
    p.rules.speed.maximum_cps = Some(20);
    for count in [20, 22] {
        let text = "X".repeat(count);
        let doc = document(&[(0, 1000, &text), (1000, 2500, "Following")]);
        let proposal = plan(&doc, &p, &Scenes::NoMedia);
        assert_eq!(proposal.changed(), 0);
        assert_eq!(proposal.rows()[0].skipped, Some(Skip::SpeedWouldWorsen));
        assert_eq!(proposal.before(), proposal.after());
    }
    p.rules.speed.maximum_cps = None;
    p.rules.speed.maximum_rs_exclusive = Some(35);
    p.rules.duration.enabled = false;
    let doc = document(&[(0, 550, "X"), (550, 2000, "Following")]);
    assert_eq!(
        plan(&doc, &p, &Scenes::NoMedia).rows()[0].skipped,
        Some(Skip::SpeedWouldWorsen)
    );
}

#[test]
fn coupled_speed_on_an_unchanged_neighbor_rejects_only_its_overlap_group() {
    let mut p = profile();
    p.speed_basis = SpeedBasis::VssOverlapV1;
    p.rules.speed.enabled = true;
    p.rules.speed.minimum_rs = None;
    p.rules.speed.maximum_rs_exclusive = None;
    p.rules.speed.minimum_cps = Some(6);
    p.rules.speed.maximum_cps = None;
    let doc = document(&[
        (0, 1500, &"X".repeat(40)),
        (1000, 3000, "1234567890"),
        (5000, 7000, "12345678901234"),
        (7050, 9000, "12345678901234"),
    ]);
    let proposal = plan(&doc, &p, &Scenes::NoMedia);
    assert_eq!(proposal.before().metrics[1].effective_duration_ms, 1500.);
    assert_eq!(proposal.changed(), 1);
    assert_eq!(proposal.rows()[0].skipped, Some(Skip::SpeedWouldWorsen));
    assert_eq!(proposal.rows()[1].cue_index, 2);
    assert_eq!(proposal.rows()[1].new_end, Some(ms(6970)));
    assert_eq!(proposal.after().metrics[1].effective_duration_ms, 1500.);
    assert!(!proposal.after().findings.iter().any(|f| is_speed(f.rule)));
}

#[test]
fn scene_boundaries_missing_data_and_no_media_acknowledgment_are_explicit() {
    let doc = document(&[(0, 1200, "A"), (1080, 2000, "B")]);
    let mut p = profile();
    p.timing.scene_before_ms = 100;
    p.timing.scene_after_ms = 100;
    for (cut, allowed) in [(1100, true), (1099, false), (900, true), (901, false)] {
        let proposal = plan(&doc, &p, &Scenes::Complete(vec![ms(cut)]));
        assert_eq!(proposal.coverage(), SceneCoverage::Checked);
        assert_eq!(proposal.changed(), usize::from(allowed));
        if !allowed {
            assert_eq!(proposal.rows()[0].skipped, Some(Skip::SceneExclusion));
        }
    }
    let missing = plan(&doc, &p, &Scenes::Unavailable);
    assert_eq!(missing.coverage(), SceneCoverage::Unavailable);
    assert_eq!(missing.rows()[0].skipped, Some(Skip::SceneDataUnavailable));
    let partial = plan(&doc, &p, &Scenes::NoMedia);
    assert_eq!(partial.coverage(), SceneCoverage::NoMedia);
    assert_eq!(partial.changed(), 1);
    assert!(partial.edit(&doc, false).is_err());
    assert!(partial.edit(&doc, true).is_ok());
    assert!(
        propose(
            doc.clone(),
            &p,
            &Scenes::Complete(vec![ms(2), ms(1)]),
            || Ok(())
        )
        .is_err()
    );
    assert!(
        propose(
            doc.clone(),
            &p,
            &Scenes::Complete(vec![ms(2); 100_001]),
            || Ok(())
        )
        .is_err()
    );
    p.timing.scene_before_ms = 0;
    p.timing.scene_after_ms = 0;
    assert_eq!(
        plan(&doc, &p, &Scenes::Unavailable).coverage(),
        SceneCoverage::NotRequired
    );
}

#[test]
fn empty_single_cue_and_cancelled_proposals_do_not_create_history_work() {
    let p = profile();
    for doc in [document(&[]), document(&[(0, 1000, "Only")])] {
        let proposal = plan(&doc, &p, &Scenes::NoMedia);
        assert!(proposal.rows().is_empty());
        assert!(proposal.edit(&doc, true).is_err());
    }
    let doc = document(&[(0, 1200, "A"), (1100, 2300, "B")]);
    let mut calls = 0;
    let result = propose(doc.clone(), &p, &Scenes::NoMedia, || {
        calls += 1;
        if calls >= 15 {
            Err(Error::Cancelled)
        } else {
            Ok(())
        }
    });
    assert!(matches!(result, Err(Error::Cancelled)));
    assert_eq!(doc.cues()[0].end(), ms(1200));
}
