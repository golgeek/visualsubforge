use media_analysis::alignment::{Alignment, Segment};
use std::time::Duration;
use subtitle_core::{Edit, Editor, Milliseconds};
use subtitle_formats::srt;
use vsf_application::resync::{
    Budget, Cancellation, CueOutcome, Error, Proposal, SubtitleSnapshot, UnresolvedReason,
};

fn section(s: u64, e: u64, t: u64, u: u64) -> Segment {
    Segment {
        source_start_us: s,
        source_end_us: e,
        target_start_us: t,
        target_end_us: u,
        anchors: 3,
        max_residual_us: 0.,
    }
}
fn alignment(segments: Vec<Segment>) -> Alignment {
    Alignment {
        segments,
        distinctive_matches: 6,
        ambiguous_source_anchors: 0,
        reordered: false,
    }
}
fn snapshot(text: &str) -> SubtitleSnapshot {
    let parsed = srt::parse(text).unwrap();
    SubtitleSnapshot::new(parsed.document, parsed.source).unwrap()
}
fn proposal(text: &str, map: Alignment) -> Proposal {
    Proposal::build(
        snapshot(text),
        map,
        &Budget::new(&Cancellation::default(), Duration::from_secs(5)),
    )
    .unwrap()
}
const SOURCE: &str = "\u{feff}007\r\n00:00:01,000 --> 00:00:03,000\r\n<i>世界</i>\r\n\r\n003\r\n00:00:02,500 --> 00:00:04,000\r\nOverlap\r\n\r\n";

#[test]
fn complete_proposal_preserves_format_text_order_overlap_and_one_undo() {
    let p = proposal(
        SOURCE,
        alignment(vec![section(0, 5_000_000, 500_000, 5_300_000)]),
    );
    assert!(p.is_complete());
    let mut editor = Editor::new(p.snapshot().document().clone());
    assert_eq!(editor.revision(), 0); // Building a proposal did not accept it.
    editor
        .apply_edit(0, p.edit(editor.document()).unwrap(), |_| Ok(()))
        .unwrap();
    assert_eq!(editor.document().cues()[0].start().get(), 1460);
    assert_eq!(editor.document().cues()[1].start().get(), 2900);
    assert_eq!(editor.document().cues()[0].end().get(), 3380);
    let parsed = srt::parse(SOURCE).unwrap();
    assert_eq!(
        srt::serialize(editor.document(), &parsed.source).unwrap(),
        p.srt().unwrap()
    );
    assert!(p.srt().unwrap().starts_with("\u{feff}007\r\n"));
    assert!(p.srt().unwrap().contains("<i>世界</i>\r\n\r\n003\r\n"));
    editor.undo(1).unwrap();
    assert_eq!(
        srt::serialize(editor.document(), &parsed.source).unwrap(),
        SOURCE
    );
    editor.redo(2).unwrap();
    assert_eq!(
        srt::serialize(editor.document(), &parsed.source).unwrap(),
        p.srt().unwrap()
    );
    assert!(matches!(
        p.edit(editor.document()),
        Err(Error::InputChanged(_))
    ));
    // A text-only edit also invalidates this immutable source snapshot.
    editor.undo(3).unwrap();
    let cue = &editor.document().cues()[0];
    editor
        .apply_edit(
            4,
            Edit::Text {
                id: cue.id(),
                start: cue.start(),
                end: cue.end(),
                text: "different".into(),
            },
            |_| Ok(()),
        )
        .unwrap();
    assert!(p.edit(editor.document()).is_err());
}

#[test]
fn gaps_and_boundary_crossings_never_export_mixed_timings() {
    let source = "1\n00:00:01,000 --> 00:00:02,000\nA\n\n2\n00:00:03,000 --> 00:00:07,000\nAcross ad\n\n3\n00:00:04,500 --> 00:00:05,000\nRemoved\n\n4\n00:00:07,000 --> 00:00:08,000\nB\n";
    let p = proposal(
        source,
        alignment(vec![
            section(0, 4_000_000, 0, 4_000_000),
            section(6_000_000, 10_000_000, 5_000_000, 9_000_000),
        ]),
    );
    assert!(!p.is_complete());
    assert!(p.srt().is_none());
    assert!(p.srt_sha256().is_none());
    assert!(p.edit(p.snapshot().document()).is_err());
    assert!(matches!(
        p.cues()[1].outcome,
        CueOutcome::Unresolved {
            reason: UnresolvedReason::CrossesDiscontinuity
        }
    ));
    assert!(matches!(
        p.cues()[2].outcome,
        CueOutcome::Unresolved {
            reason: UnresolvedReason::UnsupportedCoverage
        }
    ));
    assert!(matches!(
        p.cues()[3].outcome,
        CueOutcome::Mapped {
            start_ms: 6000,
            end_ms: 7000,
            ..
        }
    ));
    let p = proposal(
        source,
        Alignment {
            reordered: true,
            ..alignment(vec![])
        },
    );
    assert!(p.cues().iter().all(|c| matches!(
        c.outcome,
        CueOutcome::Unresolved {
            reason: UnresolvedReason::ReorderedContent
        }
    )));
}

#[test]
fn rational_mapping_rounds_once_ties_up_and_rejects_collapse() {
    let source = "1\n00:00:00,001 --> 00:00:00,003\nA\n";
    // 0.4995 ms rounds to zero. Rounding to microseconds first would give 1 ms.
    let p = proposal(source, alignment(vec![section(0, 2_000_000, 0, 999_000)]));
    assert!(matches!(
        p.cues()[0].outcome,
        CueOutcome::Mapped {
            start_ms: 0,
            end_ms: 1,
            ..
        }
    ));
    let p = proposal(source, alignment(vec![section(0, 2_000_000, 0, 1_000_000)]));
    assert!(matches!(
        p.cues()[0].outcome,
        CueOutcome::Mapped {
            start_ms: 1,
            end_ms: 2,
            ..
        }
    ));
    let p = proposal(
        "1\n00:00:00,000 --> 00:00:00,001\nA\n",
        alignment(vec![section(0, 3_000_000, 0, 1_000_000)]),
    );
    assert!(matches!(
        p.cues()[0].outcome,
        CueOutcome::Unresolved {
            reason: UnresolvedReason::InvalidMappedRange
        }
    ));
}

#[test]
fn invalid_maps_limits_and_cancellation_fail_without_a_proposal() {
    let cancel = Cancellation::default();
    let budget = Budget::new(&cancel, Duration::from_secs(5));
    for segments in [
        vec![section(1, 1, 0, 2)],
        vec![section(0, 2, 1, 1)],
        vec![section(0, u64::MAX, 0, 2)],
        vec![section(0, 2, 0, u64::MAX)],
        vec![section(0, 5, 0, 5), section(4, 8, 5, 9)],
        vec![section(0, 5, 5, 10), section(6, 9, 0, 3)],
        vec![Segment {
            max_residual_us: f64::NAN,
            ..section(0, 5, 0, 5)
        }],
    ] {
        assert!(matches!(
            Proposal::build(snapshot(SOURCE), alignment(segments), &budget),
            Err(Error::InvalidInput(_))
        ));
    }
    assert!(
        SubtitleSnapshot::new(
            srt::parse("").unwrap().document,
            srt::parse("").unwrap().source
        )
        .is_err()
    );
    assert!(matches!(
        Proposal::build(
            snapshot(SOURCE),
            alignment(vec![]),
            &Budget::new(&cancel, Duration::ZERO)
        ),
        Err(Error::Timeout)
    ));
    cancel.cancel();
    assert!(matches!(
        Proposal::build(snapshot(SOURCE), alignment(vec![]), &budget),
        Err(Error::Cancelled)
    ));
    assert!(Milliseconds::new(u64::MAX).is_err());
}

#[test]
fn touching_sections_do_not_bridge_a_jump_even_when_both_endpoints_map() {
    let p = proposal(
        "1\n00:00:01,000 --> 00:00:03,000\nCrosses\n\n2\n00:00:02,000 --> 00:00:03,000\nStarts at right boundary\n",
        alignment(vec![
            section(0, 2_000_000, 0, 2_000_000),
            section(2_000_000, 4_000_000, 3_000_000, 5_000_000),
        ]),
    );
    assert!(matches!(
        p.cues()[0].outcome,
        CueOutcome::Unresolved {
            reason: UnresolvedReason::CrossesDiscontinuity
        }
    ));
    assert!(matches!(
        p.cues()[1].outcome,
        CueOutcome::Mapped {
            start_ms: 3000,
            end_ms: 4000,
            ..
        }
    ));
}
