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
fn generated_documents_keep_all_pairs_safe_in_both_modes() {
    for seed in 0..24u64 {
        let text = ["{\\k10}timed", "1234567890123456789", "abc", "a😀\nb"];
        let rows: Vec<_> = (0..4)
            .map(|i| {
                let s = i * (30 + seed % 6) * 10;
                (
                    s,
                    s + (20 + (seed * 7 + i * 11) % 37) * 10,
                    text[(i as usize + seed as usize) % 4],
                )
            })
            .collect();
        let d = doc(&rows);
        let mut p = profile();
        p.rules.duration.minimum_ms = 310;
        p.rules.speed.maximum_cps = Some(30);
        p.timing.scene_before_ms = 30;
        p.timing.scene_after_ms = 60;
        p.speed_basis = SpeedBasis::VssOverlapV1;
        for priority in [Priority::Spacing, Priority::ReadingSpeed] {
            for edges in [SceneEdges::Inward, SceneEdges::Nearest] {
                let request = Request {
                    options: Options {
                        priority,
                        start_tolerance_ms: 100,
                        scene_edges: edges,
                    },
                    ..Request::default()
                };
                let plan = propose(
                    d.clone(),
                    Evaluation::from(&p).with_format(Format::Ass),
                    &Scenes::Complete(vec![ms(0), ms(330)]),
                    request,
                    || Ok(()),
                )
                .unwrap();
                let selected = plan.select(&plan.default_units(), || Ok(())).unwrap();
                assert!(
                    selected.error.is_none(),
                    "seed {seed}: {:?}",
                    selected.error
                );
                let cues = selected.candidate.cues();
                for (i, c) in cues.iter().enumerate() {
                    assert!(c.start() < c.end());
                    assert_eq!(c.start().get() % 10, 0);
                    assert_eq!(c.end().get() % 10, 0);
                    for (j, n) in cues.iter().enumerate().skip(i + 1) {
                        let overlap = |a: &Cue, b: &Cue| {
                            a.end()
                                .get()
                                .min(b.end().get())
                                .saturating_sub(a.start().get().max(b.start().get()))
                        };
                        assert!(overlap(c, n) <= overlap(&d.cues()[i], &d.cues()[j]));
                    }
                }
            }
        }
    }
}
