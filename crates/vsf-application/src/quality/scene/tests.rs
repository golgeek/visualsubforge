use super::*;
use crate::quality::{evaluate_with_scenes, profile::SpeedBasis};
use subtitle_core::{Cue, CueId, Document, Editor};
fn ms(value: u64) -> Milliseconds {
    Milliseconds::new(value).unwrap()
}
fn document(rows: &[(u64, u64, &str)]) -> Document {
    Document::new(
        rows.iter()
            .enumerate()
            .map(|(i, &(a, b, t))| Cue::new(CueId(i as u64 + 1), ms(a), ms(b), t.into()).unwrap())
            .collect(),
    )
    .unwrap()
}
fn profile() -> Profile {
    let mut p = Profile::default().custom("scene-test".into(), "Scene test".into());
    p.rules.speed.enabled = false;
    p.timing.scene_before_ms = 100;
    p.timing.scene_after_ms = 150;
    p
}
fn cuts(values: &[u64]) -> Scenes {
    Scenes::Complete(values.iter().copied().map(ms).collect())
}
fn plan(d: &Document, p: &Profile, s: &Scenes) -> Proposal {
    propose(d.clone(), p, s, || Ok(())).unwrap()
}
fn report(d: &Document, p: &Profile, s: &Scenes) -> super::super::Report {
    evaluate_with_scenes(d, p, s, |_, _| Ok(())).unwrap()
}
fn scene_rules(r: &super::super::Report) -> Vec<(u32, Rule)> {
    r.findings
        .iter()
        .filter(|f| matches!(f.rule, Rule::SceneStart | Rule::SceneEnd))
        .map(|f| (f.cue_index, f.rule))
        .collect()
}

#[test]
fn open_boundaries_crossings_zero_and_one_sided_buffers_are_exact() {
    let mut p = profile();
    let s = cuts(&[1000]);
    let d = document(&[
        (900, 1150, "Edges"),
        (0, 2000, "Crossing"),
        (901, 1149, "Inside"),
        (1000, 2000, "Cut"),
    ]);
    assert_eq!(
        scene_rules(&report(&d, &p, &s)),
        [
            (2, Rule::SceneStart),
            (2, Rule::SceneEnd),
            (3, Rule::SceneStart)
        ]
    );
    assert_eq!(
        scene_rules(&report(&document(&[(0, 2000, "Zero")]), &p, &cuts(&[0]))),
        [(0, Rule::SceneStart)]
    );
    p.timing.scene_before_ms = 0;
    assert!(scene_rules(&report(&document(&[(1000, 2000, "Cut")]), &p, &s)).is_empty());
    p.timing.scene_after_ms = 0;
    let r = report(&d, &p, &Scenes::Unavailable);
    assert_eq!(r.scene_coverage, SceneCoverage::NotRequired);
    assert!(r.complete);
}

#[test]
fn unavailable_partial_reduced_and_empty_complete_coverage_are_not_conflated() {
    let p = profile();
    let d = document(&[(950, 3000, "Text")]);
    for (s, c) in [
        (Scenes::NoMedia, SceneCoverage::NoMedia),
        (Scenes::Unavailable, SceneCoverage::Unavailable),
        (Scenes::Partial, SceneCoverage::Partial),
    ] {
        let r = report(&d, &p, &s);
        assert_eq!(r.scene_coverage, c);
        assert!(!r.complete);
        assert!(scene_rules(&r).is_empty());
        let proposal = plan(&d, &p, &s);
        assert_eq!(proposal.changed(), 0);
        assert!(proposal.edit(&d).is_err());
    }
    let reduced = Scenes::Reduced(vec![ms(1000)]);
    assert_eq!(
        report(&d, &p, &reduced).scene_coverage,
        SceneCoverage::Reduced
    );
    assert_eq!(plan(&d, &p, &reduced).changed(), 1);
    let empty = report(&d, &p, &cuts(&[]));
    assert!(empty.complete);
    assert_eq!(empty.scene_coverage, SceneCoverage::Checked);
}

#[test]
fn overlapping_windows_merge_but_touching_windows_keep_the_safe_boundary() {
    let p = profile();
    let s = cuts(&[1000, 1200, 2000, 2250]);
    let w = Windows::new(&p, &s, || Ok(())).unwrap();
    assert_eq!(w.containing(1150), Some((900, 1350)));
    assert_eq!(w.containing(2150), None);
    assert_eq!(w.containing(2151), Some((2150, 2400)));
    let d = document(&[(950, 3000, "Trim start"), (0, 1250, "Trim end")]);
    // Independently inspect safe target edges; overlapping ambiguity is tested below.
    let first = plan(&document(&[(950, 3000, "Trim start")]), &p, &s);
    assert_eq!(first.rows()[0].new_start, Some(ms(1350)));
    let second = plan(&document(&[(0, 1250, "Trim end")]), &p, &s);
    assert_eq!(second.rows()[0].new_end, Some(ms(900)));
    assert_eq!(scene_rules(&report(&d, &p, &s)).len(), 2);
}

#[test]
fn trims_preserve_tags_identity_source_order_and_atomic_history() {
    let p = profile();
    let s = cuts(&[1000, 5000, 8000]);
    let d = document(&[
        (7950, 10000, "{\\pos(10,20)\\i1}é 😀\\Ntwo"),
        (950, 5050, "<b>First</b>"),
    ]);
    let proposal = plan(&d, &p, &s);
    assert_eq!(proposal.changed(), 2);
    assert!(scene_rules(proposal.after()).is_empty());
    let mut editor = Editor::new(d.clone());
    assert!(
        editor
            .apply_edit(0, proposal.edit(&d).unwrap(), |_| Err(
                "injected policy failure".into()
            ))
            .is_err()
    );
    assert_eq!(editor.revision(), 0);
    assert_eq!(editor.document(), &d);
    editor
        .apply_edit(0, proposal.edit(&d).unwrap(), |_| Ok(()))
        .unwrap();
    let accepted = editor.document().clone();
    for (old, new) in d.cues().iter().zip(accepted.cues()) {
        assert_eq!((old.id(), old.text()), (new.id(), new.text()));
        assert!(new.start() >= old.start());
        assert!(new.end() <= old.end());
    }
    assert_eq!(
        (
            accepted.cues()[1].start().get(),
            accepted.cues()[1].end().get()
        ),
        (1150, 4900)
    );
    assert!(proposal.edit(&accepted).is_err());
    assert!(editor.undo(1).unwrap());
    assert_eq!(editor.document(), &d);
    assert!(!editor.undo(2).unwrap());
    assert!(editor.redo(2).unwrap());
    assert_eq!(editor.document(), &accepted);
}

#[test]
fn timed_unknown_and_malformed_markup_stays_manual_while_static_tags_survive() {
    let p = profile();
    let s = cuts(&[1000]);
    for text in [
        "{\\k20}Hi",
        "{\\K20}Hi",
        "{\\kt20}Hi",
        "{\\t(0,50,\\fs30)}Hi",
        "{\\move(0,0,20,20)}Hi",
        "{\\fad(200,200)}Hi",
        "{\\fade(0,255,0,0,20,40,60)}Hi",
        "{\\unknown3}Hi",
        "{\\i1Hi",
        "<b>Hi",
        "<font>Hi</font>",
    ] {
        let proposal = plan(&document(&[(950, 3000, text)]), &p, &s);
        assert_eq!(
            proposal.rows()[0].skipped,
            Some(Skip::MarkupNeedsReview),
            "{text}"
        );
    }
    for text in [
        "{\\i1}Hi",
        "{\\an8\\pos(20,30)}Hi",
        "<b>Hi</b>",
        "Plain C:\\name",
    ] {
        assert_eq!(
            plan(&document(&[(950, 3000, text)]), &p, &s).changed(),
            1,
            "{text}"
        );
    }
    assert_eq!(
        plan(&document(&[(950, 3000, &"x".repeat(65537))]), &p, &s).rows()[0].skipped,
        Some(Skip::MarkupNeedsReview)
    );
}

#[test]
fn impossible_short_long_and_ambiguous_intervals_have_specific_skips() {
    let p = profile();
    let s = cuts(&[1000]);
    for (d, skip) in [
        (document(&[(950, 1050, "Collapse")]), Skip::WouldCollapse),
        (document(&[(950, 1900, "Short")]), Skip::DurationConstraint),
        (document(&[(950, 10000, "Long")]), Skip::DurationConstraint),
        (
            document(&[(950, 3000, "Outer"), (1000, 2000, "Inner")]),
            Skip::AmbiguousOverlap,
        ),
        (
            document(&[(950, 3000, "A"), (950, 3500, "B")]),
            Skip::AmbiguousOverlap,
        ),
    ] {
        assert_eq!(plan(&d, &p, &s).rows()[0].skipped, Some(skip));
    }
    let near_max = document(&[(Milliseconds::MAX - 50, Milliseconds::MAX, "Last")]);
    assert_eq!(
        plan(&near_max, &p, &cuts(&[Milliseconds::MAX])).rows()[0].skipped,
        Some(Skip::WouldCollapse)
    );
}

#[test]
fn whole_document_speed_rejection_retains_independent_safe_changes() {
    let mut p = profile();
    p.rules.speed.enabled = true;
    p.rules.speed.minimum_rs = None;
    p.rules.speed.maximum_rs_exclusive = None;
    p.rules.speed.maximum_cps = Some(20);
    let s = cuts(&[1000, 5000]);
    for count in [20, 24] {
        let text = "x".repeat(count);
        let d = document(&[(950, 1950, &text), (4950, 7000, "Safe")]);
        let proposal = plan(&d, &p, &s);
        assert_eq!(proposal.changed(), 1);
        assert_eq!(proposal.rows()[0].skipped, Some(Skip::TimingWouldWorsen));
        assert_eq!(proposal.rows()[1].new_start, Some(ms(5150)));
        assert_eq!(proposal.after().metrics[0].start_ms, 950.);
    }
}

#[test]
fn ordinary_overlap_groups_recheck_coupled_effective_reading_duration() {
    let mut p = profile();
    p.speed_basis = SpeedBasis::VssOverlapV1;
    p.rules.speed.enabled = true;
    p.rules.speed.minimum_rs = None;
    p.rules.speed.maximum_rs_exclusive = None;
    p.rules.speed.maximum_cps = Some(20);
    let d = document(&[
        (950, 2500, "a"),
        (2000, 3500, "bbbbbbbbbbbbbbbbbbbb"),
        (4950, 7000, "Safe"),
    ]);
    let proposal = plan(&d, &p, &cuts(&[1000, 5000]));
    assert_eq!(proposal.changed(), 2);
    assert_eq!(
        proposal.after().metrics[1].effective_duration_ms,
        proposal.before().metrics[1].effective_duration_ms
    );
    assert_eq!(
        proposal.after().metrics[1].cps,
        proposal.before().metrics[1].cps
    );
}

#[test]
fn ordered_bounded_scene_inputs_and_cancellation_are_enforced() {
    let p = profile();
    let d = document(&[(950, 3000, "Text")]);
    for s in [
        cuts(&[1000, 1000]),
        cuts(&[2000, 1000]),
        cuts(&(0..100001).collect::<Vec<_>>()),
    ] {
        assert!(propose(d.clone(), &p, &s, || Ok(())).is_err());
    }
    let mut count = 0;
    let result = propose(d, &p, &cuts(&[1000]), || {
        count += 1;
        if count >= 5 {
            Err(Error::Cancelled)
        } else {
            Ok(())
        }
    });
    assert!(matches!(result, Err(Error::Cancelled)));
}

#[test]
fn a_trim_that_worsens_an_unchanged_overlap_neighbor_is_withheld() {
    let mut p = profile();
    p.speed_basis = SpeedBasis::VssOverlapV1;
    p.rules.speed.enabled = true;
    p.rules.speed.minimum_rs = None;
    p.rules.speed.maximum_rs_exclusive = None;
    p.rules.speed.maximum_cps = Some(20);
    let d = document(&[
        (950, 2500, "aaaaaaaaaaaaaaaaaaaa"),
        (2000, 3500, "bbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
        (4950, 7000, "Safe"),
    ]);
    let proposal = plan(&d, &p, &cuts(&[1000, 5000]));
    assert_eq!(proposal.before().metrics[1].effective_duration_ms, 1477.);
    assert_eq!(proposal.changed(), 1);
    assert_eq!(proposal.rows()[0].skipped, Some(Skip::TimingWouldWorsen));
    assert_eq!(proposal.after().metrics[1].effective_duration_ms, 1477.);
    assert_eq!(proposal.rows()[1].new_start, Some(ms(5150)));
}
