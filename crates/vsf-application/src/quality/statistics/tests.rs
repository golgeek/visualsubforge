use super::*;
use crate::quality::{self, profile::SpeedBasis, scene::Scenes};
use subtitle_core::{Cue, CueId, Document, Editor, Milliseconds};
fn doc(rows: &[(u64, u64, &str)]) -> Document {
    Document::new(
        rows.iter()
            .enumerate()
            .map(|(i, (a, b, text))| {
                Cue::new(
                    CueId(i as u64 + 1),
                    Milliseconds::new(*a).unwrap(),
                    Milliseconds::new(*b).unwrap(),
                    (*text).into(),
                )
                .unwrap()
            })
            .collect(),
    )
    .unwrap()
}
fn profile() -> Profile {
    let mut p = Profile::default().custom("statistics-test".into(), "Statistics test".into());
    p.timing.scene_before_ms = 0;
    p.timing.scene_after_ms = 0;
    p
}
fn stats(d: &Document, selection: Selection) -> Report {
    evaluate(d, &profile(), &Scenes::NoMedia, selection, || Ok(())).unwrap()
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} != {b}");
}
#[test]
fn empty_single_and_all_excluded_have_explicit_denominators_and_nulls() {
    let empty = stats(&doc(&[]), Selection::default());
    assert_eq!(empty.summary.included_cues, 0);
    assert_eq!(empty.summary.duration_ms, Aggregate::default());
    assert_eq!(empty.summary.ratings.len(), 9);
    assert!(
        empty
            .summary
            .ratings
            .iter()
            .all(|r| r.count == 0 && r.percentage.is_none())
    );
    let d = doc(&[(0, 1500, "0")]);
    let one = stats(&d, Selection::default()).summary;
    assert_eq!(
        (
            one.included_cues,
            one.empty_text_cues,
            one.cps.count,
            one.rs.count
        ),
        (1, 0, 1, 1)
    );
    assert_eq!(one.rs.mean, Some(1.));
    assert_eq!(one.gaps_ms.count, 0);
    let none = stats(
        &d,
        Selection {
            range: None,
            excluded: vec![1, 1],
        },
    )
    .summary;
    assert_eq!(
        (none.included_cues, none.excluded_cues, none.scope_cues),
        (0, 1, 1)
    );
    assert_eq!(none.selection.excluded, vec![1]);
    assert!(none.rs.mean.is_none());
    let json = serde_json::to_string(&empty).unwrap();
    assert!(!json.contains("NaN") && !json.contains("Infinity"));
}
#[test]
fn scope_is_source_order_validated_and_exclusions_are_explicit() {
    let d = doc(&[
        (4000, 5000, "x"),
        (0, 1000, "y"),
        (2000, 3000, "z"),
        (6000, 7000, "a"),
    ]);
    let selection = Selection {
        range: Some(Range { first: 2, last: 3 }),
        excluded: vec![3, 3],
    };
    let report = stats(&d, selection).summary;
    assert_eq!(
        (
            report.total_cues,
            report.scope_cues,
            report.outside_scope_cues,
            report.excluded_cues,
            report.included_cues
        ),
        (4, 2, 2, 1, 1)
    );
    assert_eq!(report.duration_ms.minimum.unwrap().cue_id, "2");
    for range in [
        Range { first: 0, last: 2 },
        Range { first: 3, last: 2 },
        Range { first: 1, last: 5 },
    ] {
        assert!(
            evaluate(
                &d,
                &profile(),
                &Scenes::NoMedia,
                Selection {
                    range: Some(range),
                    excluded: vec![]
                },
                || Ok(())
            )
            .is_err()
        );
    }
    for exclude in [0, 1, 4, 5] {
        assert!(
            evaluate(
                &d,
                &profile(),
                &Scenes::NoMedia,
                Selection {
                    range: Some(Range { first: 2, last: 3 }),
                    excluded: vec![exclude]
                },
                || Ok(())
            )
            .is_err()
        );
    }
    assert!(stats(&d, Selection::default()).summary.complete);
    assert!(
        Selection {
            range: None,
            excluded: vec![1]
        }
        .validate(0)
        .is_err()
    );
}
#[test]
fn finite_rates_have_independent_means_unavailable_counts_and_tag_policy() {
    let d = doc(&[
        (0, 1500, "{\\i1}<b>A😀</b>\nB"),
        (2000, 2500, "0"),
        (3000, 4000, "  "),
    ]);
    let before = d.clone();
    let s = stats(&d, Selection::default()).summary;
    assert_eq!(d, before);
    assert_eq!(
        (s.included_cues, s.empty_text_cues, s.cps.count, s.rs.count),
        (3, 1, 2, 1)
    );
    assert_eq!(
        s.unavailable_cps,
        UnavailableCounts {
            total: 1,
            empty_text: 1,
            ..Default::default()
        }
    );
    assert_eq!(
        s.unavailable_rs,
        UnavailableCounts {
            total: 2,
            empty_text: 1,
            reading_allowance: 1,
            ..Default::default()
        }
    );
    near(s.cps.mean.unwrap(), 3.); // tagged cue N=6 => CPS4; literal zero CPS2
    near(s.rs.mean.unwrap(), 6.);
    near(s.duration_ms.mean.unwrap(), 1000.);
    assert_eq!(s.line_count.maximum.unwrap().value, 2.);
    assert_eq!(s.longest_line.maximum.unwrap().value, 3.); // A + surrogate pair
    near(s.line_count.mean.unwrap(), 4. / 3.);
}
#[test]
fn categories_share_exact_existing_boundaries_and_finite_denominator() {
    let lengths = [4, 5, 10, 13, 15, 23, 27, 31, 35];
    let texts: Vec<_> = lengths.iter().map(|n| "x".repeat(*n)).collect();
    let rows: Vec<_> = texts
        .iter()
        .enumerate()
        .map(|(i, s)| (i as u64 * 2000, i as u64 * 2000 + 1500, s.as_str()))
        .chain([(20000, 20100, "short"), (22000, 23000, "")])
        .collect();
    let s = stats(&doc(&rows), Selection::default()).summary;
    assert_eq!(s.rs.count, 9);
    assert!(s.ratings.iter().all(|r| r.count == 1));
    for r in s.ratings {
        near(r.percentage.unwrap(), 100. / 9.);
    }
    assert_eq!(s.unavailable_rs.total, 2);
}
#[test]
fn subset_keeps_full_document_reading_context_even_for_excluded_predecessor() {
    let d = doc(&[
        (0, 2500, "This preceding text requires time"),
        (1000, 3000, "next"),
        (3500, 5000, "end"),
    ]);
    let mut p = profile();
    p.speed_basis = SpeedBasis::VssOverlapV1;
    let full = quality::evaluate(&d, &p, |_, _| Ok(())).unwrap();
    let selected = evaluate(
        &d,
        &p,
        &Scenes::NoMedia,
        Selection {
            range: Some(Range { first: 2, last: 2 }),
            excluded: vec![],
        },
        || Ok(()),
    )
    .unwrap();
    assert_eq!(selected.summary.rs.mean, full.metrics[1].rs.value());
    assert_eq!(selected.summary.cps.mean, full.metrics[1].cps.value());
    let only = doc(&[(1000, 3000, "next")]);
    assert_ne!(
        selected.summary.rs.mean,
        stats(&only, Selection::default()).summary.rs.mean
    );
    assert_eq!(selected.summary.gaps_ms.count, 0);
}
#[test]
fn gap_statistics_count_zero_negative_and_positive_without_bridging_exclusions() {
    let d = doc(&[
        (3000, 4000, "c"),
        (0, 1500, "a"),
        (1000, 2000, "b"),
        (4000, 4500, "d"),
    ]);
    let s = stats(&d, Selection::default()).summary;
    assert_eq!(
        (
            s.gaps_ms.count,
            s.negative_gaps,
            s.touching_gaps,
            s.positive_gaps
        ),
        (3, 1, 1, 1)
    );
    assert_eq!(s.gaps_ms.minimum.as_ref().unwrap().value, -500.);
    assert_eq!(
        s.gaps_ms.minimum.unwrap().related_cue_id.as_deref(),
        Some("3")
    );
    near(s.gaps_ms.mean.unwrap(), 500. / 3.);
    let s = stats(
        &d,
        Selection {
            range: None,
            excluded: vec![3],
        },
    )
    .summary;
    assert_eq!((s.gaps_ms.count, s.excluded_neighbor_pairs), (1, 2));
    assert_eq!(s.gaps_ms.mean, Some(0.));
}
#[test]
fn overlap_sweep_matches_all_pairs_oracle_including_nested_and_equal_starts() {
    for seed in 0..20u64 {
        let rows: Vec<_> = (0..30)
            .map(|i| {
                let start = (i * 97 + seed * 31) % 1100;
                (start, start + 100 + (i * 11) % 400, "x")
            })
            .collect();
        let d = doc(&rows);
        let selection = Selection {
            range: None,
            excluded: (1..=30).filter(|i| i % 4 == 0).collect(),
        };
        let s = stats(&d, selection.clone()).summary;
        let mut pairs = 0;
        let mut affected = HashSet::new();
        for (i, a) in rows.iter().enumerate() {
            for (j, b) in rows.iter().enumerate().skip(i + 1) {
                if selection.includes(i as u32)
                    && selection.includes(j as u32)
                    && a.0 < b.1
                    && b.0 < a.1
                {
                    pairs += 1;
                    affected.insert(i);
                    affected.insert(j);
                }
            }
        }
        assert_eq!(s.overlapping_pairs, pairs as f64);
        assert_eq!(s.overlapping_cues, affected.len() as u32);
    }
    let s = stats(
        &doc(&[
            (0, 10000, "outer"),
            (1000, 2000, "inside"),
            (3000, 4000, "inside"),
            (10000, 11000, "touch"),
        ]),
        Selection::default(),
    )
    .summary;
    assert_eq!((s.overlapping_pairs, s.overlapping_cues), (2., 3));
    assert_eq!(s.negative_gaps, 1); // adjacent gaps cannot count all nested overlaps
}
#[test]
fn tied_extrema_choose_first_source_cue_and_quality_findings_keep_context() {
    let d = doc(&[(2000, 2400, "x"), (0, 400, "y"), (2300, 3000, "z")]);
    let all = stats(&d, Selection::default());
    assert_eq!(all.summary.duration_ms.minimum.unwrap().cue_id, "1");
    let selected = stats(
        &d,
        Selection {
            range: Some(Range { first: 1, last: 1 }),
            excluded: vec![],
        },
    );
    assert!(selected.findings.iter().all(|f| f.cue_id == "1"));
    assert!(
        selected
            .findings
            .iter()
            .any(|f| f.rule == Rule::Overlap && f.related_cue_id.as_deref() == Some("3"))
    );
    assert_eq!(selected.summary.affected_cues, 1);
    assert_eq!(
        selected.summary.findings_count,
        selected.summary.rules.iter().map(|r| r.count).sum::<u32>()
    );
}
#[test]
fn disabled_rules_do_not_hide_metrics_and_coverage_is_not_invented() {
    let d = doc(&[(0, 400, "x"), (200, 800, "y")]);
    let mut p = profile();
    p.rules.gaps = false;
    p.rules.speed.enabled = false;
    p.rules.duration.enabled = false;
    p.rules.lines.enabled = false;
    p.rules.empty = false;
    let s = evaluate(&d, &p, &Scenes::NoMedia, Selection::default(), || Ok(()))
        .unwrap()
        .summary;
    assert_eq!(s.findings_count, 0);
    assert_eq!(s.overlapping_pairs, 1.);
    assert_eq!(s.cps.count, 2);
    p.timing.scene_before_ms = 130;
    for (scenes, coverage) in [
        (Scenes::NoMedia, SceneCoverage::NoMedia),
        (Scenes::Partial, SceneCoverage::Partial),
        (Scenes::Unavailable, SceneCoverage::Unavailable),
    ] {
        let s = evaluate(&d, &p, &scenes, Selection::default(), || Ok(()))
            .unwrap()
            .summary;
        assert!(!s.complete);
        assert_eq!(s.scene_coverage, coverage);
        assert_eq!(s.included_cues, 2);
    }
    let s = evaluate(
        &d,
        &p,
        &Scenes::Reduced(vec![]),
        Selection::default(),
        || Ok(()),
    )
    .unwrap()
    .summary;
    assert!(s.complete);
    assert_eq!(s.scene_coverage, SceneCoverage::Reduced);
}
#[test]
fn cancellation_and_read_only_evaluation_preserve_history_and_unavailable_zero_duration() {
    let d = doc(&[
        (0, 3000, "lots of preceding text"),
        (0, 400, "inside"),
        (4000, 5500, "next"),
    ]);
    let editor = Editor::new(d.clone());
    let mut p = profile();
    p.speed_basis = SpeedBasis::VssOverlapV1;
    let s = evaluate(&d, &p, &Scenes::NoMedia, Selection::default(), || Ok(()))
        .unwrap()
        .summary;
    assert_eq!(s.unavailable_cps.invalid_duration, 1);
    assert_eq!(s.unavailable_rs.invalid_duration, 1);
    for checkpoint in [1, 5, 15] {
        let mut n = 0;
        assert_eq!(
            evaluate(
                &d,
                &profile(),
                &Scenes::NoMedia,
                Selection::default(),
                || {
                    n += 1;
                    if n == checkpoint {
                        Err(Error::Cancelled)
                    } else {
                        Ok(())
                    }
                }
            ),
            Err(Error::Cancelled)
        );
    }
    let full = quality::evaluate(&d, &profile(), |_, _| Ok(())).unwrap();
    assert_eq!(
        summarize(full, Selection::default(), || Err(Error::Cancelled)),
        Err(Error::Cancelled)
    );
    assert_eq!(editor.document(), &d);
    assert_eq!(editor.revision(), 0);
}
