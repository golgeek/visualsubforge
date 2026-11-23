use super::*;
use crate::quality::{evaluate_with_scenes, timing_fix};
use subtitle_core::{Cue, CueId, Document};
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
fn plan(d: &Document, p: &Profile, s: &Scenes) -> timing_fix::Proposal {
    timing_fix::propose(
        d.clone(),
        p.into(),
        s,
        timing_fix::Request {
            families: timing_fix::Families {
                gaps: false,
                speed: false,
                shots: true,
            },
            ..Default::default()
        },
        || Ok(()),
    )
    .unwrap()
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
        assert_eq!(proposal.default_units().len(), 0);
        assert!(
            proposal
                .select(&proposal.default_units(), || Ok(()))
                .unwrap()
                .edit(&d)
                .is_err()
        );
    }
    let reduced = Scenes::Reduced(vec![ms(1000)]);
    assert_eq!(
        report(&d, &p, &reduced).scene_coverage,
        SceneCoverage::Reduced
    );
    assert_eq!(plan(&d, &p, &reduced).default_units().len(), 1);
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
    assert_eq!(first.units()[0].endpoints[0].after.start_ms, 1350);
    let second = plan(&document(&[(0, 1250, "Trim end")]), &p, &s);
    assert_eq!(second.units()[0].endpoints[0].after.end_ms, 900);
    assert_eq!(scene_rules(&report(&d, &p, &s)).len(), 2);
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
        assert!(
            timing_fix::propose(d.clone(), (&p).into(), &s, Default::default(), || Ok(())).is_err()
        );
    }
    let mut count = 0;
    let result = timing_fix::propose(d, (&p).into(), &cuts(&[1000]), Default::default(), || {
        count += 1;
        if count >= 5 {
            Err(Error::Cancelled)
        } else {
            Ok(())
        }
    });
    assert!(matches!(result, Err(Error::Cancelled)));
}
