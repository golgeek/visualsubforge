use super::*;
use crate::quality::Profile;
use subtitle_core::{Cue, Editor};
use subtitle_formats::Format;

fn ms(n: u64) -> Milliseconds {
    Milliseconds::new(n).unwrap()
}
fn doc(rows: &[(u64, u64, &str)]) -> Document {
    Document::new(
        rows.iter()
            .enumerate()
            .map(|(i, &(s, e, t))| Cue::new(CueId(i as u64 + 1), ms(s), ms(e), t.into()).unwrap())
            .collect(),
    )
    .unwrap()
}
fn profile() -> Profile {
    let mut p = Profile::default().custom("timing".into(), "Timing test".into());
    p.rules.speed.minimum_rs = None;
    p.rules.speed.maximum_rs_exclusive = None;
    p.timing.scene_before_ms = 0;
    p.timing.scene_after_ms = 0;
    p
}
#[test]
fn spacing_can_trade_speed_only_to_fix_a_selected_family() {
    let d = doc(&[(0, 1000, "12345678901234567890"), (1000, 3000, "Next")]);
    let p = profile();
    for (priority, speed, trade) in [
        (Priority::ReadingSpeed, true, false),
        (Priority::Spacing, true, true),
        (Priority::Spacing, false, false),
    ] {
        let r = Request {
            options: Options {
                priority,
                ..Options::default()
            },
            families: Families {
                speed,
                ..Families::default()
            },
            ..Request::default()
        };
        let plan = propose(d.clone(), (&p).into(), &Scenes::NoMedia, r, || Ok(())).unwrap();
        assert_eq!(plan.units().iter().any(|u| u.trade_off), trade);
        assert_eq!(plan.default_units().len(), usize::from(trade));
        if trade {
            let effect = &plan.units()[0].effects[0];
            assert_eq!(effect.before, Some(20.));
            assert_eq!(effect.limit, 20);
            assert!(effect.after.unwrap() > 20.);
            let selected = plan.select(&[0], || Ok(())).unwrap();
            let mut editor = Editor::new(d.clone());
            editor
                .apply_edit(0, selected.edit(&d).unwrap(), |_| Ok(()))
                .unwrap();
            assert_eq!(editor.document().cues()[0].end(), ms(920));
            assert!(editor.undo(1).unwrap());
            assert_eq!(editor.document(), &d);
            assert!(editor.redo(2).unwrap());
        }
    }
}
#[test]
fn residual_budget_respects_inclusive_cps_exclusive_rs_and_allowance() {
    let mut p = profile();
    let d = doc(&[(0, 1000, "12345678901234567890")]);
    let ctx = Evaluation::from(&p);
    let m = evaluate_with_scenes(&d, ctx, &Scenes::NoMedia, |_, _| Ok(()))
        .unwrap()
        .metrics
        .remove(0);
    let at = Endpoints {
        start_ms: 0,
        end_ms: 1000,
    };
    assert_eq!(budget(&m, at, 1000, false, &p).max_counted, Some(20));
    p.rules.speed.maximum_rs_exclusive = Some(20);
    assert_eq!(budget(&m, at, 1000, false, &p).max_counted, Some(9));
    assert_eq!(budget(&m, at, 500, false, &p).max_counted, None);
}
#[test]
fn manual_text_budget_requires_a_remaining_high_rate() {
    let mut profile = Profile::default().custom("half-second".into(), "Half second".into());
    profile.rules.duration.minimum_ms = 500;
    for (text, high_cps) in [("Hi.", false), ("Too much text for half a second.", true)] {
        let document = doc(&[(0, 500, text), (580, 2500, "Next")]);
        let request = Request {
            scope: Some(vec![CueId(1)]),
            ..Request::default()
        };
        let proposal = propose(
            document,
            (&profile).into(),
            &Scenes::NoMedia,
            request,
            || Ok(()),
        )
        .unwrap();
        let [row] = proposal.manual() else {
            panic!("one manual cue expected")
        };
        assert!(row.rules.contains(&Rule::SpeedUnavailable));
        assert_eq!(row.rules.contains(&Rule::CpsHigh), high_cps);
        if high_cps {
            assert_eq!(row.budget.as_ref().unwrap().effective_ms, 500);
            assert_eq!(
                row.budget_text.as_deref(),
                Some("A text budget is unavailable within the 500 ms reading allowance.")
            );
        } else {
            assert_eq!(row.rules, [Rule::SpeedUnavailable]);
            assert!(row.budget.is_none());
            assert!(row.budget_text.is_none());
        }
    }
}
#[test]
fn partial_is_unchecked_and_budget_records_its_endpoint() {
    let d = doc(&[
        (0, 1000, "123456789012345678901234567890"),
        (1300, 2500, "Next"),
    ]);
    let p = profile();
    let plan = propose(
        d.clone(),
        (&p).into(),
        &Scenes::NoMedia,
        Request::default(),
        || Ok(()),
    )
    .unwrap();
    assert!(plan.default_units().is_empty());
    let row = &plan.manual()[0];
    let b = row.budget.as_ref().unwrap();
    assert!(b.at_partial);
    assert_eq!(
        (b.at.end_ms, b.max_counted, b.shorten_by),
        (1220, Some(24), Some(6))
    );
    assert_eq!(plan.select(&[0], || Ok(())).unwrap().changed, 1);
}
#[test]
fn scope_precision_markup_cut_zero_and_cancellation() {
    let p = Profile::default();
    let d = doc(&[(0, 1000, "{\\i1}first"), (1100, 2300, "{\\k10}timed")]);
    for f in [Format::Ass, Format::Ssa, Format::Srt, Format::WebVtt] {
        let context = Evaluation::from(&p).with_format(f);
        let scenes = Scenes::Complete(vec![ms(0), ms(1100)]);
        let request = Request {
            scope: Some(vec![CueId(1)]),
            ..Request::default()
        };
        let plan = propose(d.clone(), context, &scenes, request.clone(), || Ok(())).unwrap();
        for u in plan.units() {
            for c in &u.endpoints {
                assert_eq!(c.index, 0);
                assert_eq!(c.after.start_ms % f.time_step_ms(), 0);
                assert_eq!(c.after.end_ms % f.time_step_ms(), 0);
            }
        }
        let replay = propose(d.clone(), context, &scenes, request.clone(), || Ok(())).unwrap();
        assert_eq!(
            serde_json::to_vec(plan.units()).unwrap(),
            serde_json::to_vec(replay.units()).unwrap()
        );
        assert!(matches!(
            propose(d.clone(), context, &scenes, request, || Err(
                Error::Cancelled
            )),
            Err(Error::Cancelled)
        ));
    }
}
#[test]
fn coarse_grid_oracle_enumerates_the_linked_unit() {
    // The independent checker sees every feasible assignment of first start,
    // first end and second start. The ten-millisecond grid includes all
    // optima: maximum duration and T bound the three dimensions.
    let d = doc(&[(20, 80, "a"), (80, 160, "b")]);
    let mut p = profile();
    p.rules.duration.minimum_ms = 60;
    p.rules.duration.maximum_ms = 100;
    p.rules.speed.enabled = false;
    p.timing.cue_gap_ms = 30;
    let ctx = Evaluation::from(&p).with_format(Format::Ass);
    let opts = Options {
        start_tolerance_ms: 30,
        ..Options::default()
    };
    let plan = propose(
        d.clone(),
        ctx,
        &Scenes::NoMedia,
        Request {
            options: opts,
            ..Request::default()
        },
        || Ok(()),
    )
    .unwrap();
    let old = plan.before();
    let mut best = None;
    for s in (0u64..=50).step_by(10) {
        for e in (s + 10..=s + 100).step_by(10) {
            for ns in (80u64..=110).step_by(10) {
                if e > ns {
                    continue;
                }
                let candidate = doc(&[(s, e, "a"), (ns, 160, "b")]);
                let report =
                    evaluate_with_scenes(&candidate, ctx, &Scenes::NoMedia, |_, _| Ok(())).unwrap();
                let original = timing_validation::timing_findings(old);
                if report
                    .findings
                    .iter()
                    .any(|f| timing_validation::worsened(f, &original))
                {
                    continue;
                }
                let key = (
                    report.findings.len(),
                    s.abs_diff(20) + ns - 80,
                    e.abs_diff(80),
                    e - s,
                    s,
                    e,
                    ns,
                );
                best = Some(best.map_or(key, |v| std::cmp::min(v, key)));
            }
        }
    }
    let selected = plan.select(&plan.default_units(), || Ok(())).unwrap();
    let a = &selected.candidate.cues()[0];
    let b = &selected.candidate.cues()[1];
    let (s, e, ns) = (a.start().get(), a.end().get(), b.start().get());
    assert_eq!(
        Some((
            plan.after().findings.len(),
            s.abs_diff(20) + ns - 80,
            e.abs_diff(80),
            e - s,
            s,
            e,
            ns
        )),
        best
    );
}
#[test]
fn revalidation_only_allows_the_reviewed_rate_for_a_retained_spacing_fix() {
    let p = profile();
    let d = doc(&[(0, 1000, "12345678901234567890"), (1000, 3000, "Next")]);
    let request = Request {
        options: Options {
            priority: Priority::Spacing,
            ..Default::default()
        },
        ..Default::default()
    };
    let proposal = propose(d, (&p).into(), &Scenes::NoMedia, request, || Ok(())).unwrap();
    let chosen = proposal.default_units().into_iter().collect();
    let check = |end, next| {
        let candidate = doc(&[(0, end, "12345678901234567890"), (next, 3000, "Next")]);
        let report = evaluate_with_scenes(&candidate, &p, &Scenes::NoMedia, |_, _| Ok(())).unwrap();
        unsafe_indices(proposal.before(), &report, proposal.units(), &chosen)
    };
    assert!(check(920, 1000).is_empty());
    assert!(check(900, 1000).contains(&0)); // beyond the displayed CPS
    assert!(check(920, 950).contains(&0)); // still has a gap finding
}

#[test]
fn independent_checker_ranks_current_cue_assignments_for_both_priorities() {
    let original = doc(&[(100, 300, "1234"), (320, 560, "aa"), (650, 850, "bb")]);
    let mut p = profile();
    p.timing.cue_gap_ms = 40;
    p.timing.scene_before_ms = 10;
    p.timing.scene_after_ms = 10;
    p.rules.duration.minimum_ms = 120;
    p.rules.duration.maximum_ms = 300;
    p.rules.speed.minimum_cps = None;
    p.rules.speed.maximum_cps = Some(20);
    let context = Evaluation::from(&p).with_format(Format::Ass);
    for cut in [100, 300, 600] {
        let scenes = Scenes::Complete(vec![ms(cut)]);
        let baseline = evaluate_with_scenes(&original, context, &scenes, |_, _| Ok(())).unwrap();
        let old = timing_validation::timing_findings(&baseline);
        for priority in [Priority::ReadingSpeed, Priority::Spacing] {
            for edges in [SceneEdges::Inward, SceneEdges::Nearest] {
                let options = Options {
                    priority,
                    start_tolerance_ms: 30,
                    scene_edges: edges,
                };
                let request = Request {
                    scope: Some(vec![CueId(1)]),
                    options,
                    ..Default::default()
                };
                let plan = propose(original.clone(), context, &scenes, request, || Ok(())).unwrap();
                let checked = plan.select(&plan.default_units(), || Ok(())).unwrap();
                assert!(checked.error.is_none());
                let mut best = None;
                for s in (70..=130u64).step_by(10) {
                    for e in (s + 120..=s + 300).step_by(10) {
                        if (e > 300 && e + 40 > 320)
                            || (s != 100 && (cut - 10 < s && s < cut + 10))
                            || (e != 300 && cut - 10 < e && e < cut + 10)
                            || (edges == SceneEdges::Inward
                                && ((cut == 100 && s < 100) || (cut == 300 && e > 300)))
                        {
                            continue;
                        }
                        let candidate = doc(&[(s, e, "1234"), (320, 560, "aa"), (650, 850, "bb")]);
                        let result =
                            evaluate_with_scenes(&candidate, context, &scenes, |_, _| Ok(()))
                                .unwrap();
                        let counts = |r: &Report| {
                            let mut c = [0i32; 4];
                            for finding in &r.findings {
                                let group = match finding.rule {
                                    Rule::DurationShort | Rule::DurationLong => 0,
                                    Rule::CpsHigh
                                    | Rule::CpsLow
                                    | Rule::RsHigh
                                    | Rule::RsLow
                                    | Rule::SpeedUnavailable => 1,
                                    Rule::Gap | Rule::Overlap => 2,
                                    Rule::SceneStart | Rule::SceneEnd => 3,
                                    _ => continue,
                                };
                                c[group] += 1;
                            }
                            c
                        };
                        let before = counts(&baseline);
                        let left = counts(&result);
                        let spacing_wins = priority == Priority::Spacing
                            && (left[2] < before[2] || left[3] < before[3]);
                        if result.findings.iter().any(|f| {
                            timing_validation::worsened(f, &old)
                                && !(spacing_wins && matches!(f.rule, Rule::CpsHigh | Rule::RsHigh))
                        }) {
                            continue;
                        }
                        let ranked = if priority == Priority::Spacing {
                            [left[0], left[2], left[3], left[1]]
                        } else {
                            left
                        };
                        let key = (ranked, s.abs_diff(100), e.abs_diff(300), e - s, s, e);
                        best = Some(best.map_or(key, |k| std::cmp::min(k, key)));
                    }
                }
                let (_, _, _, _, s, e) = best.unwrap();
                let cue = &checked.candidate.cues()[0];
                assert_eq!(
                    (cue.start().get(), cue.end().get()),
                    (s, e),
                    "{priority:?}, {edges:?}, cut {cut}"
                );
            }
        }
    }
}

mod dependencies;
mod episode;
mod oracle;
