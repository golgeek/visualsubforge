use media_analysis::alignment::{Alignment, Segment};
use std::time::Duration;
use subtitle_core::{Edit, Editor, Milliseconds};
use subtitle_formats::srt;
use vsf_application::resync::{
    Budget, Cancellation, CueOutcome, Error, GapOutcome, Proposal, SubtitleSnapshot,
    UnresolvedReason,
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
fn gaps_and_durations_follow_content_without_changing_source_order() {
    // Intentionally out of document order. Chronological neighbor lookup must
    // not reorder the SRT or change imported labels/identities.
    let source =
        "9\n00:00:02,100 --> 00:00:03,000\nLater\n\n7\n00:00:01,000 --> 00:00:02,000\nEarlier\n";
    for (target_end, expected_start, expected_end, gap) in [
        (3_840_000, 2016, 1920, 96),
        (4_160_000, 2184, 2080, 104),
        (4_000_000, 2100, 2000, 100),
    ] {
        let p = proposal(
            source,
            alignment(vec![section(0, 4_000_000, 0, target_end)]),
        );
        assert!(p.is_complete());
        let parsed = srt::parse(p.srt().unwrap()).unwrap();
        assert_eq!(parsed.document.cues()[0].start().get(), expected_start);
        assert_eq!(parsed.document.cues()[1].end().get(), expected_end);
        assert_eq!(expected_start - expected_end, gap);
        assert!(
            matches!(p.cues()[1].following_gap.as_ref().unwrap().outcome,
            GapOutcome::Mapped { target_ms } if target_ms == gap)
        );
        assert!(p.srt().unwrap().starts_with("9\n"));
        assert!(p.cues()[0].following_gap.is_none());
        let mut editor = Editor::new(p.snapshot().document().clone());
        let accepted = editor
            .apply_edit(0, p.edit(editor.document()).unwrap(), |_| Ok(()))
            .unwrap();
        if accepted.changed {
            editor.undo(1).unwrap();
            assert_eq!(editor.document(), p.snapshot().document());
            editor.redo(2).unwrap();
        } else {
            assert_eq!(editor.revision(), 0);
            assert!(!editor.can_undo());
        }
        assert_eq!(editor.document().cues()[1].end().get(), expected_end);
    }
}

#[test]
fn ad_gaps_follow_separate_sections_and_long_pauses_cannot_collapse_preceding_cues() {
    let source = "1\n00:00:01,000 --> 00:00:02,000\nA\n\n2\n00:00:02,100 --> 00:00:03,000\nB\n";
    let p = proposal(
        source,
        alignment(vec![
            section(0, 2_050_000, 0, 1_968_000),
            section(2_050_000, 4_000_000, 2_968_000, 4_840_000),
        ]),
    );
    assert!(matches!(
        p.cues()[0].outcome,
        CueOutcome::Mapped { end_ms: 1920, .. }
    ));
    assert!(matches!(
        p.cues()[0].following_gap.as_ref().unwrap().outcome,
        GapOutcome::AcrossSections
    ));
    let p = proposal(
        "1\n00:00:01,000 --> 00:00:01,100\nShort\n\n2\n00:00:20,000 --> 00:00:21,000\nAfter long gap\n",
        alignment(vec![section(0, 30_000_000, 0, 28_800_000)]),
    );
    assert!(p.is_complete());
    assert!(matches!(
        p.cues()[0].outcome,
        CueOutcome::Mapped {
            start_ms: 960,
            end_ms: 1056,
            ..
        }
    ));
    assert!(matches!(
        p.cues()[0].following_gap.as_ref().unwrap().outcome,
        GapOutcome::Mapped { target_ms: 18144 }
    ));
}

#[test]
fn every_endpoint_maps_independently_without_accumulated_rounding_drift() {
    let mut source = String::new();
    let mut start = 1000;
    let mut starts = Vec::new();
    for index in 0..256 {
        let gap = [0, 1, 120, 160, 240, 1_000, 10_000][index % 7];
        starts.push(start);
        source.push_str(&format!(
            "{}\n{} --> {}\nCue {}\n\n",
            index + 1,
            srt::format_time(Milliseconds::new(start).unwrap()),
            srt::format_time(Milliseconds::new(start + 200).unwrap()),
            index + 1
        ));
        start += 200 + gap;
    }
    for percent in 80..=125 {
        let p = proposal(
            &source,
            alignment(vec![section(0, 600_000_000, 0, 6_000_000 * percent)]),
        );
        assert!(p.is_complete());
        let parsed = srt::parse(p.srt().unwrap()).unwrap();
        for (index, cue) in parsed.document.cues().iter().enumerate() {
            assert_eq!(cue.start().get(), (starts[index] * percent + 50) / 100);
            assert_eq!(
                cue.end().get(),
                ((starts[index] + 200) * percent + 50) / 100
            );
            assert!(cue.end() > cue.start());
            if let Some(next) = parsed.document.cues().get(index + 1) {
                let target_ms = next.start().get() - cue.end().get();
                assert!(
                    matches!(p.cues()[index].following_gap.as_ref().unwrap().outcome,
                    GapOutcome::Mapped { target_ms: reported } if reported == target_ms)
                );
            }
        }
    }
}

#[test]
fn measured_pal_speed_change_scales_norm_gaps_and_long_pauses_in_both_directions() {
    for (source_gap, target_gap) in [(120, 125), (160, 167), (240, 250), (1000, 1042)] {
        let source = format!(
            "1\n00:00:00,000 --> 00:00:01,000\nA\n\n2\n{} --> 00:00:03,000\nB\n",
            srt::format_time(Milliseconds::new(1000 + source_gap).unwrap())
        );
        let p = proposal(
            &source,
            alignment(vec![section(0, 24_000_000, 0, 25_025_000)]),
        );
        assert!(p.is_complete());
        assert!(
            matches!(p.cues()[0].following_gap.as_ref().unwrap().outcome,
            GapOutcome::Mapped { target_ms } if target_ms == target_gap)
        );
        let reverse = proposal(
            p.srt().unwrap(),
            alignment(vec![section(0, 25_025_000, 0, 24_000_000)]),
        );
        // Millisecond serialization is lossy, so the reverse need not restore
        // every possible input exactly; these explicit examples do round-trip.
        assert_eq!(
            srt::parse(reverse.srt().unwrap()).unwrap().document,
            srt::parse(&source).unwrap().document
        );
    }
}

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
