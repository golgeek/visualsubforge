use super::*;
use crate::quality::profile::SpeedBasis;
use subtitle_core::Editor;
fn ms(value: u64) -> Milliseconds {
    Milliseconds::new(value).unwrap()
}
fn document(rows: &[(u64, u64, &str)]) -> Document {
    Document::new(
        rows.iter()
            .enumerate()
            .map(|(i, &(a, b, text))| {
                Cue::new(CueId(i as u64 + 1), ms(a), ms(b), text.into()).unwrap()
            })
            .collect(),
    )
    .unwrap()
}
fn profile() -> Profile {
    let mut p = Profile::default().custom("duration-test".into(), "Duration test".into());
    p.rules.speed.enabled = false;
    p.rules.lines.enabled = false;
    p.timing.scene_before_ms = 0;
    p.timing.scene_after_ms = 0;
    p
}
fn speed() -> Profile {
    let mut p = profile();
    p.rules.duration.enabled = false;
    p.rules.speed.enabled = true;
    p.rules.speed.minimum_cps = None;
    p.rules.speed.maximum_cps = None;
    p.rules.speed.minimum_rs = None;
    p.rules.speed.maximum_rs_exclusive = None;
    p
}
fn plan(d: &Document, p: &Profile, s: &Scenes, scope: Scope) -> Proposal {
    propose(d.clone(), p, s, scope, || Ok(())).unwrap()
}
fn end(p: &Profile, start: u64, stop: u64, text: &str) -> Option<u64> {
    let d = document(&[(start, stop, text)]);
    plan(&d, p, &Scenes::NoMedia, Scope::Document)
        .rows()
        .first()
        .and_then(|r| r.new_end)
        .map(|e| e.get())
}
#[test]
fn inclusive_cps_and_duration_and_exclusive_rs_have_exact_millisecond_edges() {
    let mut p = speed();
    p.rules.speed.maximum_cps = Some(10);
    assert_eq!(end(&p, 100, 1099, "0123456789"), Some(1100));
    assert_eq!(end(&p, 100, 1100, "0123456789"), None);
    p.rules.speed.maximum_cps = None;
    p.rules.speed.minimum_cps = Some(10);
    assert_eq!(end(&p, 100, 1101, "0123456789"), Some(1100));
    p.rules.speed.minimum_cps = None;
    p.rules.speed.maximum_rs_exclusive = Some(10);
    assert_eq!(end(&p, 100, 1600, "0123456789"), Some(1601));
    assert_eq!(end(&p, 100, 1601, "0123456789"), None);
    p.rules.speed.maximum_rs_exclusive = None;
    p.rules.speed.minimum_rs = Some(10);
    assert_eq!(end(&p, 100, 1601, "0123456789"), Some(1600));
    assert_eq!(end(&p, 100, 500, "0123456789"), Some(601));
    let p = profile();
    assert_eq!(end(&p, 100, 899, "x"), Some(900));
    assert_eq!(end(&p, 100, 7101, "x"), Some(7100));
    assert_eq!(end(&p, 100, 7100, "x"), None);
}
#[test]
fn all_enabled_limits_intersect_without_display_rounding_or_partial_repairs() {
    let mut p = speed();
    p.rules.duration.enabled = true;
    p.rules.duration.minimum_ms = 800;
    p.rules.duration.maximum_ms = 1500;
    p.rules.speed.maximum_cps = Some(10);
    p.rules.speed.maximum_rs_exclusive = Some(20);
    assert_eq!(end(&p, 0, 400, "0123456789"), Some(1001));
    p.rules.duration.maximum_ms = 1000;
    let d = document(&[(0, 400, "0123456789")]);
    let proposed = plan(&d, &p, &Scenes::NoMedia, Scope::Document);
    assert_eq!(proposed.rows()[0].skipped, Some(Skip::ConflictingBounds));
    assert_eq!(proposed.changed(), 0);
    assert!(proposed.edit(&d).is_err());
}
#[test]
fn closest_endpoint_matches_an_independent_enumerated_checker_oracle() {
    let mut profiles = Vec::new();
    let mut p = profile();
    p.rules.duration.minimum_ms = 600;
    p.rules.duration.maximum_ms = 1300;
    profiles.push(p);
    let mut p = speed();
    p.rules.speed.minimum_cps = Some(8);
    p.rules.speed.maximum_cps = Some(15);
    profiles.push(p);
    let mut p = speed();
    p.rules.speed.minimum_rs = Some(10);
    p.rules.speed.maximum_rs_exclusive = Some(25);
    profiles.push(p);
    for p in profiles {
        for text in ["abcde", "0123456789", "one two three"] {
            for old in [100, 599, 1000, 1500] {
                let d = document(&[(0, old, text)]);
                let proposal = plan(&d, &p, &Scenes::NoMedia, Scope::Document);
                let before_bad = proposal.before().findings.iter().any(|f| target(f.rule));
                if !before_bad {
                    assert!(proposal.rows().is_empty());
                    continue;
                }
                let oracle = (1..=2000)
                    .filter(|&new_end| {
                        let candidate = document(&[(0, new_end, text)]);
                        !evaluate_with_scenes(&candidate, &p, &Scenes::NoMedia, |_, _| Ok(()))
                            .unwrap()
                            .findings
                            .iter()
                            .any(|f| target(f.rule))
                    })
                    .min_by_key(|&v| (v.abs_diff(old), v));
                assert_eq!(
                    proposal.rows()[0].new_end.map(|v| v.get()),
                    oracle,
                    "{text} old={old} profile={p:?}"
                );
                assert!(!proposal.after().findings.iter().any(|f| target(f.rule)));
            }
        }
    }
}
#[test]
fn next_cue_gap_exact_touching_disabled_warning_and_last_cue_are_constrained() {
    let mut p = speed();
    p.rules.speed.maximum_cps = Some(20);
    p.rules.gaps = false;
    for (next, expected) in [(1080, Some(1000)), (1079, None)] {
        let d = document(&[(0, 400, "01234567890123456789"), (next, 3000, "next")]);
        let proposed = plan(&d, &p, &Scenes::NoMedia, Scope::Cue(CueId(1)));
        assert_eq!(proposed.rows()[0].new_end.map(|v| v.get()), expected);
        if expected.is_none() {
            assert_eq!(proposed.rows()[0].skipped, Some(Skip::NeighborConstraint));
        }
    }
    p.timing.cue_gap_ms = 0;
    let d = document(&[(0, 400, "01234567890123456789"), (1000, 3000, "next")]);
    assert_eq!(
        plan(&d, &p, &Scenes::NoMedia, Scope::Cue(CueId(1))).rows()[0].new_end,
        Some(ms(1000))
    );
    assert_eq!(end(&p, 4000, 4400, "01234567890123456789"), Some(5000));
}
#[test]
fn scene_windows_choose_nearest_permitted_outer_edge_and_explain_blocked_range() {
    let mut p = profile();
    p.timing.scene_before_ms = 100;
    p.timing.scene_after_ms = 100;
    p.rules.duration.minimum_ms = 1000;
    p.rules.duration.maximum_ms = 2000;
    let scenes = Scenes::Complete(vec![ms(1000)]);
    let d = document(&[(0, 800, "text")]);
    assert_eq!(
        plan(&d, &p, &scenes, Scope::Document).rows()[0].new_end,
        Some(ms(1100))
    );
    p.rules.duration.maximum_ms = 1050;
    assert_eq!(
        plan(&d, &p, &scenes, Scope::Document).rows()[0].skipped,
        Some(Skip::SceneExclusion)
    );
    p.rules.duration.minimum_ms = 800;
    p.rules.duration.maximum_ms = 1000;
    let d = document(&[(0, 2000, "text")]);
    assert_eq!(
        plan(&d, &p, &scenes, Scope::Document).rows()[0].new_end,
        Some(ms(900))
    );
    p.rules.duration.maximum_ms = 900;
    assert_eq!(
        plan(&d, &p, &scenes, Scope::Document).rows()[0].new_end,
        Some(ms(900))
    );
    // Touching exclusions leave their shared edge available.
    let scenes = Scenes::Complete(vec![ms(1000), ms(1200)]);
    p.rules.duration.minimum_ms = 1100;
    p.rules.duration.maximum_ms = 1300;
    let d = document(&[(0, 800, "text")]);
    assert_eq!(
        plan(&d, &p, &scenes, Scope::Document).rows()[0].new_end,
        Some(ms(1100))
    );
}
#[test]
fn scene_coverage_is_required_unless_both_buffers_are_disabled() {
    let mut p = profile();
    p.timing.scene_before_ms = 100;
    let d = document(&[(0, 400, "text")]);
    for s in [Scenes::NoMedia, Scenes::Unavailable, Scenes::Partial] {
        let proposed = plan(&d, &p, &s, Scope::Document);
        assert_eq!(proposed.rows()[0].skipped, Some(Skip::SceneDataUnavailable));
        assert!(proposed.edit(&d).is_err());
    }
    for s in [Scenes::Complete(vec![]), Scenes::Reduced(vec![])] {
        let proposed = plan(&d, &p, &s, Scope::Document);
        assert_eq!(proposed.changed(), 1);
        assert!(proposed.edit(&d).is_ok());
    }
    p.timing.scene_before_ms = 0;
    assert!(
        plan(&d, &p, &Scenes::Partial, Scope::Document)
            .edit(&d)
            .is_ok()
    );
}
#[test]
fn selection_and_unsorted_source_order_preserve_static_tags_and_atomic_history() {
    let p = profile();
    let d = document(&[
        (4000, 4400, "{\\pos(10,20)\\i1}é 😀\\Ntwo"),
        (0, 400, "<b>First</b>"),
    ]);
    let selected = plan(&d, &p, &Scenes::NoMedia, Scope::Cue(CueId(2)));
    assert_eq!(selected.rows().len(), 1);
    assert_eq!(selected.rows()[0].cue_index, 1);
    assert!(
        propose(
            d.clone(),
            &p,
            &Scenes::NoMedia,
            Scope::Cue(CueId(99)),
            || Ok(())
        )
        .is_err()
    );
    let proposed = plan(&d, &p, &Scenes::NoMedia, Scope::Document);
    assert_eq!(proposed.changed(), 2);
    let mut editor = Editor::new(d.clone());
    assert!(
        editor
            .apply_edit(0, proposed.edit(&d).unwrap(), |_| Err(
                "failed validation".into()
            ))
            .is_err()
    );
    assert_eq!(editor.document(), &d);
    editor
        .apply_edit(0, proposed.edit(&d).unwrap(), |_| Ok(()))
        .unwrap();
    let after = editor.document().clone();
    for (a, b) in after.cues().iter().zip(d.cues()) {
        assert_eq!((a.id(), a.start(), a.text()), (b.id(), b.start(), b.text()));
        assert_eq!(a.end().get(), b.start().get() + 800);
    }
    assert!(proposed.edit(&after).is_err());
    editor.undo(1).unwrap();
    assert_eq!(editor.document(), &d);
    editor.redo(2).unwrap();
    assert_eq!(editor.document(), &after);
}
#[test]
fn timed_unknown_malformed_and_oversized_markup_stay_manual() {
    let p = profile();
    for text in [
        "{\\k20}word",
        "{\\t(0,100,\\fs20)}word",
        "{\\fade(0,0,0,0,0,0,0)}word",
        "{\\unknown1}word",
        "<i>missing",
        "{\\i1",
    ] {
        let d = document(&[(0, 400, text)]);
        assert_eq!(
            plan(&d, &p, &Scenes::NoMedia, Scope::Document).rows()[0].skipped,
            Some(Skip::MarkupNeedsReview),
            "{text}"
        );
    }
    let huge = "a".repeat(65537);
    let d = document(&[(0, 400, &huge)]);
    assert_eq!(
        plan(&d, &p, &Scenes::NoMedia, Scope::Document).rows()[0].skipped,
        Some(Skip::MarkupNeedsReview)
    );
}
#[test]
fn ambiguous_groups_and_domain_exhaustion_have_explicit_skips() {
    let p = profile();
    for rows in [
        vec![(0, 400, "x"), (0, 1000, "y")],
        vec![(0, 1000, "x"), (200, 400, "y")],
    ] {
        let d = document(&rows);
        assert!(
            plan(&d, &p, &Scenes::NoMedia, Scope::Document)
                .rows()
                .iter()
                .all(|r| r.skipped == Some(Skip::AmbiguousOverlap))
        );
    }
    let d = document(&[(Milliseconds::MAX - 500, Milliseconds::MAX, "x")]);
    assert_eq!(
        plan(&d, &p, &Scenes::NoMedia, Scope::Document).rows()[0].skipped,
        Some(Skip::ConflictingBounds)
    );
}
#[test]
fn overlap_aware_selected_repair_cannot_worsen_an_unchanged_cue_but_batch_can_resolve_both() {
    let mut p = profile();
    p.speed_basis = SpeedBasis::VssOverlapV1;
    p.rules.duration.maximum_ms = 2000;
    p.timing.cue_gap_ms = 0;
    p.rules.speed = speed().rules.speed;
    p.rules.speed.minimum_cps = Some(2);
    let text = "a".repeat(50);
    let d = document(&[
        (0, 2500, &text),
        (1000, 4000, "abcde"),
        (6000, 6400, "safe"),
    ]);
    let selected = plan(&d, &p, &Scenes::NoMedia, Scope::Cue(CueId(1)));
    assert_eq!(selected.changed(), 0);
    assert_eq!(selected.rows()[0].skipped, Some(Skip::CoupledConstraints));
    let all = plan(&d, &p, &Scenes::NoMedia, Scope::Document);
    assert_eq!(all.changed(), 3);
    assert_eq!(
        all.rows()
            .iter()
            .map(|r| r.new_end.unwrap().get())
            .collect::<Vec<_>>(),
        [1000, 3000, 6800]
    );
    assert!(!all.after().findings.iter().any(|f| target(f.rule)));
}
#[test]
fn coupled_rejection_retains_independent_safe_groups() {
    let mut p = speed();
    p.speed_basis = SpeedBasis::VssOverlapV1;
    p.rules.speed.minimum_cps = Some(10);
    p.timing.cue_gap_ms = 0;
    let d = document(&[
        (0, 2000, "abcdefghij"),
        (500, 2500, "abcdefghijklmnopqrs"),
        (5000, 8000, "0123456789"),
    ]);
    // First trim removes an overlap, reducing the next cue's effective rate below
    // its minimum. That original overlap group is withheld; the last cue is safe.
    let proposed = plan(&d, &p, &Scenes::NoMedia, Scope::Document);
    assert_eq!(proposed.rows()[0].skipped, Some(Skip::CoupledConstraints));
    assert_eq!(proposed.rows().last().unwrap().new_end, Some(ms(6000)));
}
#[test]
fn cancellation_is_checked_during_targets_and_candidate_validation() {
    let d = document(&[(0, 400, "text"), (2000, 2400, "more")]);
    let p = profile();
    for stop in [1, 8, 20, 32] {
        let mut count = 0;
        let result = propose(d.clone(), &p, &Scenes::NoMedia, Scope::Document, || {
            count += 1;
            if count == stop {
                Err(Error::Cancelled)
            } else {
                Ok(())
            }
        });
        assert!(
            matches!(result, Err(Error::Cancelled)),
            "checkpoint {stop}, reached {count}"
        );
    }
    assert_eq!(plan(&d, &p, &Scenes::NoMedia, Scope::Document).changed(), 2);
}

#[test]
fn a_target_resolved_by_an_earlier_repair_is_reported_without_a_second_edit() {
    let mut p = profile();
    p.speed_basis = SpeedBasis::VssOverlapV1;
    p.rules.duration.maximum_ms = 2000;
    p.rules.speed = speed().rules.speed;
    p.rules.speed.maximum_rs_exclusive = Some(100);
    let text = "a".repeat(50);
    let d = document(&[(0, 2500, &text), (1100, 2600, "01234567890123456789")]);
    let proposed = plan(&d, &p, &Scenes::NoMedia, Scope::Document);
    assert_eq!(proposed.changed(), 1);
    assert_eq!(proposed.rows()[0].new_end, Some(ms(1020)));
    assert_eq!(proposed.rows()[1].skipped, Some(Skip::NoDirectChange));
    assert!(!proposed.after().findings.iter().any(|f| target(f.rule)));
}
