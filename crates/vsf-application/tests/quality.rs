use std::fs;
use subtitle_core::{Cue, CueId, Document, Edit, Editor, Milliseconds};
use vsf_application::quality::{
    self, Error, Profile, Rule, Severity,
    metrics::{self, Rate, Rating, Unavailable},
    profile::SpeedBasis,
};

fn cue(id: u64, start: u64, end: u64, text: &str) -> Cue {
    Cue::new(
        CueId(id),
        Milliseconds::new(start).unwrap(),
        Milliseconds::new(end).unwrap(),
        text.into(),
    )
    .unwrap()
}
fn policy() -> Profile {
    Profile::default().custom("test-policy".into(), "Test policy".into())
}
fn check(cues: Vec<Cue>, profile: &Profile) -> quality::Report {
    quality::evaluate(&Document::new(cues).unwrap(), profile, |_, _| Ok(())).unwrap()
}

#[test]
fn pinned_host_text_policy_handles_unicode_tags_entities_and_newlines() {
    // Expected counts are independently stated examples from the source audit,
    // including deliberate differences from the plugin's regex helper.
    for (input, count, lines, empty) in [
        ("A😀B", 4, vec![4], false),
        ("e\u{301}", 2, vec![2], false),
        ("<i>A</i>\nB", 4, vec![1, 1], false),
        ("<i>A</i>\r\nB", 4, vec![1, 1], false),
        ("<i>A</i>\rB", 4, vec![1, 1], false),
        ("A {note} B", 10, vec![10], false),
        ("A{\\an8}B", 2, vec![2], false),
        (r"A{\a6\fs32\t(0,800,\c&H0000FF&)\k20}B", 2, vec![2], false),
        // The named historical policy counts text escapes literally; rendering
        // interprets them. A visual-line metric would require a different policy.
        (r"A\NB\hC", 7, vec![7], false),
        ("&amp;", 5, vec![5], false),
        ("<font color=red>X</font>", 1, vec![1], false),
        ("A<broken", 8, vec![8], false),
        ("<bad {\\an8}", 11, vec![11], false),
        ("<i>\n</i>", 2, vec![0, 0], true),
        ("<i></i>", 0, vec![0], true),
        ("  \t", 3, vec![3], true),
        ("", 0, vec![0], true),
    ] {
        let result = metrics::text(input, || Ok(())).unwrap();
        assert_eq!(
            (result.count, result.lines, result.empty),
            (count, lines, empty),
            "{input:?}"
        );
    }
}

#[test]
fn rates_keep_short_and_empty_states_out_of_json_numbers() {
    assert_eq!(
        metrics::rates(u32::MAX, u64::MAX, false).2,
        Some(Rating::TooSlow)
    );
    for (count, ms, cps, rs) in [
        (40, 2500, 16.0, 20.0),
        (40, 2000, 20.0, 80.0 / 3.0),
        (40, 1500, 80.0 / 3.0, 40.0),
        (10, 501, 10000.0 / 501.0, 10000.0),
    ] {
        let values = metrics::rates(count, ms, false);
        assert_eq!(values.0.value(), Some(cps));
        assert_eq!(values.1.value(), Some(rs));
        serde_json::to_value(&values).unwrap();
    }
    for ms in [0, 499, 500] {
        let (cps, rs, rating) = metrics::rates(10, ms, false);
        assert!(matches!(rs, Rate::Unavailable { .. }));
        assert!(rating.is_none());
        assert_eq!(cps.value().is_some(), ms != 0);
        let json = serde_json::to_string(&(cps, rs)).unwrap();
        assert!(!json.contains("null"));
        assert!(!json.contains("Infinity"));
    }
    assert_eq!(
        metrics::rates(0, 500, true).1,
        Rate::Unavailable {
            reason: Unavailable::EmptyText
        }
    );
}

#[test]
fn all_rating_thresholds_are_unrounded_strict_upper_bounds() {
    let bands = [
        Rating::TooSlow,
        Rating::SlowAcceptable,
        Rating::ABitSlow,
        Rating::GoodSlow,
        Rating::Perfect,
        Rating::GoodFast,
        Rating::ABitFast,
        Rating::FastAcceptable,
        Rating::TooFast,
    ];
    for (index, boundary) in [5, 10, 13, 15, 23, 27, 31, 35].into_iter().enumerate() {
        // Duration 10.5 seconds leaves ten seconds for reading, so one character
        // changes RS by 0.1. Equality belongs to the following band.
        for (count, expected) in [
            (boundary * 10 - 1, bands[index]),
            (boundary * 10, bands[index + 1]),
            (boundary * 10 + 1, bands[index + 1]),
        ] {
            assert_eq!(metrics::rates(count, 10500, false).2, Some(expected));
        }
    }
}

#[test]
fn findings_use_exact_duration_speed_line_and_gap_limits() {
    let mut p = policy();
    p.rules.duration.minimum_ms = 1000;
    p.rules.duration.maximum_ms = 6000;
    p.rules.speed.enabled = false;
    let report = check(
        vec![
            cue(1, 0, 999, "A"),
            cue(2, 1100, 2100, "B"),
            cue(3, 2200, 8200, "C"),
            cue(4, 8300, 14301, "D"),
        ],
        &p,
    );
    assert_eq!(
        report.findings.iter().map(|f| f.rule).collect::<Vec<_>>(),
        vec![Rule::DurationShort, Rule::DurationLong]
    );
    p.rules.speed.enabled = true;
    p.rules.speed.minimum_rs = None;
    p.rules.speed.maximum_rs_exclusive = Some(35);
    p.rules.speed.minimum_cps = Some(20);
    p.rules.speed.maximum_cps = Some(20);
    let report = check(vec![cue(1, 0, 2000, &"x".repeat(40))], &p);
    assert!(report.findings.is_empty()); // exactly 20 CPS is allowed at either bound
    p.rules.speed.minimum_cps = None;
    p.rules.speed.maximum_cps = None;
    assert!(
        check(vec![cue(1, 0, 1500, &"x".repeat(35))], &p)
            .findings
            .iter()
            .any(|f| f.rule == Rule::RsHigh)
    );
    // 34.999 rounds to 35.0 for display but does not violate the RS bound.
    p.rules.duration.enabled = false;
    p.rules.lines.enabled = false;
    assert!(
        check(vec![cue(1, 0, 1_000_500, &"x".repeat(34_999))], &p)
            .findings
            .is_empty()
    );
    p.rules.speed.enabled = false;
    p.rules.lines.enabled = true;
    p.rules.lines.single_line_preferred = 37;
    p.rules.lines.maximum_characters = 40;
    for (length, severity) in [
        (37, None),
        (38, Some(Severity::Advisory)),
        (40, Some(Severity::Advisory)),
        (41, Some(Severity::Violation)),
    ] {
        let r = check(vec![cue(1, 0, 2500, &"x".repeat(length))], &p);
        assert_eq!(r.findings.first().map(|f| f.severity), severity);
    }
    assert!(
        check(vec![cue(1, 0, 2500, "A\nB\nC")], &p)
            .findings
            .iter()
            .any(|f| f.rule == Rule::LineCount)
    );
    for (gap, expected) in [
        (79, Some(Rule::Gap)),
        (80, None),
        (81, None),
        (-1, Some(Rule::Overlap)),
    ] {
        let r = check(
            vec![cue(1, 0, 1000, "A"), cue(2, (1000 + gap) as u64, 2000, "B")],
            &p,
        );
        assert_eq!(r.findings.first().map(|f| f.rule), expected);
    }
    p.timing.cue_gap_ms = 0;
    assert!(
        check(vec![cue(1, 0, 1000, "A"), cue(2, 1000, 2000, "B")], &p)
            .findings
            .is_empty()
    );
}

#[test]
fn overlap_chain_is_iterative_and_metrics_and_findings_use_the_same_basis() {
    let mut p = policy();
    p.speed_basis = SpeedBasis::VssOverlapV1;
    let r = check(
        vec![
            cue(2, 1000, 3000, &"b".repeat(40)),
            cue(1, 0, 2000, &"a".repeat(35)),
            cue(3, 2000, 4000, &"c".repeat(40)),
        ],
        &p,
    );
    assert_eq!(
        r.metrics
            .iter()
            .map(|m| m.cue_id.as_str())
            .collect::<Vec<_>>(),
        vec!["2", "1", "3"]
    );
    assert_eq!(r.metrics[0].reading_start_ms, 1502.0);
    assert_eq!(r.metrics[0].rs.value(), Some(40000.0 / 998.0));
    assert_eq!(r.metrics[0].cps.value(), Some(40000.0 / 1498.0));
    assert_eq!(r.metrics[2].reading_start_ms, 3000.0);
    assert!(r.findings.iter().any(|f| f.cue_id == "2"
        && f.rule == Rule::RsHigh
        && f.measured == r.metrics[0].rs.value()));
    assert_eq!(r.affected_cues, 3);
    let nested = check(
        vec![
            cue(1, 0, 10000, "Outer"),
            cue(2, 1000, 2000, "Inner"),
            cue(3, 3000, 4000, "Another"),
        ],
        &p,
    );
    assert!(nested.findings.iter().any(|f| f.rule == Rule::Overlap
        && f.cue_id == "1"
        && f.related_cue_id.as_deref() == Some("3")));
    let chain = check((0..10000).map(|i| cue(i + 1, i, 20000, "x")).collect(), &p);
    assert_eq!(chain.metrics.len(), 10000); // no recursive stack growth
}

#[test]
fn checking_preserves_document_history_and_disabled_rules_do_not_hide_empty_state() {
    let document = Document::new(vec![cue(7, 0, 500, "")]).unwrap();
    let mut editor = Editor::new(document.clone());
    editor
        .apply_edit(
            0,
            Edit::Text {
                id: CueId(7),
                start: Milliseconds::new(0).unwrap(),
                end: Milliseconds::new(500).unwrap(),
                text: "changed".into(),
            },
            |_| Ok(()),
        )
        .unwrap();
    editor.undo(1).unwrap();
    let r = quality::evaluate(editor.document(), &Profile::default(), |_, _| Ok(())).unwrap();
    assert!(r.findings.iter().any(|f| f.rule == Rule::Empty));
    assert_eq!(editor.document(), &document);
    assert_eq!(editor.revision(), 2);
    assert!(editor.can_redo());
    assert!(!editor.dirty());
    let mut p = policy();
    p.rules.speed.minimum_rs = None;
    p.rules.speed.maximum_rs_exclusive = None;
    p.rules.speed.minimum_cps = None;
    p.rules.speed.maximum_cps = None;
    assert!(
        !check(vec![cue(1, 0, 499, "x")], &p)
            .findings
            .iter()
            .any(|f| f.rule == Rule::SpeedUnavailable)
    );
}

#[test]
fn profiles_pin_partial_coverage_and_reject_unsupported_semantics_without_overwrite() {
    for p in Profile::builtins() {
        p.validate().unwrap();
        let bytes = serde_json::to_vec(&p).unwrap();
        assert_eq!(quality::parse_profile(&bytes).unwrap(), p);
        assert_eq!(
            p.fingerprint().unwrap(),
            quality::parse_profile(&bytes)
                .unwrap()
                .fingerprint()
                .unwrap()
        );
    }
    let sw = Profile::builtin("sw3-uk-vo-subset").unwrap();
    assert_eq!(sw.timing.cue_gap_ms, 160);
    assert_eq!(sw.rules.speed.maximum_rs_exclusive, Some(35));
    assert_eq!(sw.rules.speed.maximum_cps, None);
    let r = check(vec![cue(1, 0, 2500, &"x".repeat(35))], &sw);
    assert!(r.findings.is_empty());
    assert!(!r.complete);
    for (field, value) in [
        ("version", serde_json::json!(1)),
        ("textAlgorithm", serde_json::json!("guessed_unicode")),
        ("extra", serde_json::json!(true)),
        ("kind", serde_json::json!("timing")),
    ] {
        let mut json = serde_json::to_value(Profile::default()).unwrap();
        json[field] = value;
        assert!(quality::parse_profile(&serde_json::to_vec(&json).unwrap()).is_err());
    }
    assert!(quality::parse_profile(&vec![b' '; quality::MAX_PROFILE_BYTES + 1]).is_err());
    let mut p = Profile::default();
    p.timing.cue_gap_ms = 100;
    assert!(p.validate().unwrap_err().to_string().contains("immutable"));
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("custom.vsfpreset");
    let mut p = policy();
    quality::write_profile(&path, &p).unwrap();
    let original = fs::read(&path).unwrap();
    p.rules.duration.minimum_ms = 0;
    assert!(quality::write_profile(&path, &p).is_err());
    assert_eq!(fs::read(&path).unwrap(), original);
    fs::write(&path, b"not JSON").unwrap();
    assert!(quality::read_profile(&path).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"not JSON");
}

#[test]
fn cancellation_and_limits_fail_without_partial_success() {
    let document =
        Document::new(vec![cue(1, 0, 1000, &format!("<{}>", "x".repeat(100_000)))]).unwrap();
    let mut calls = 0;
    let result = quality::evaluate(&document, &Profile::default(), |_, _| {
        calls += 1;
        if calls == 5 {
            Err(Error::Cancelled)
        } else {
            Ok(())
        }
    });
    assert_eq!(result.unwrap_err(), Error::Cancelled);
    assert_eq!(calls, 5);
    let document = Document::new(
        (0..quality::MAX_CUES + 1)
            .map(|i| cue(i as u64, 0, 1000, "x"))
            .collect(),
    )
    .unwrap();
    assert!(matches!(
        quality::evaluate(&document, &Profile::default(), |_, _| Ok(())),
        Err(Error::Limit(_))
    ));
    assert!(matches!(
        metrics::text(&"x".repeat(quality::MAX_TEXT_BYTES + 1), || Ok(())),
        Err(Error::Limit(_))
    ));
}
