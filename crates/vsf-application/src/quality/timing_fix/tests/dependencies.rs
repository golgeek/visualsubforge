use super::*;

#[derive(Deserialize)]
struct Case {
    case: u32,
    cues: Vec<(u64, u64, String)>,
    profile: Overrides,
    options: Options,
    families: Families,
    scenes: Vec<u64>,
    scope: Option<Vec<u64>>,
    format: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Overrides {
    maximum_cps: Option<u32>,
    scene_before_ms: u32,
    scene_after_ms: u32,
    speed_basis: SpeedBasis,
}

fn trial_for_case(number: u32) -> super::oracle::Trial {
    let cases: Vec<Case> = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/timing-parity/dependent-cases.json"
    ))
    .unwrap();
    let case = cases.into_iter().find(|case| case.case == number).unwrap();
    assert_eq!(case.format, "Srt");
    let mut profile = Profile::default().custom("regression".into(), "Regression".into());
    profile.speed_basis = case.profile.speed_basis;
    profile.rules.speed.maximum_cps = case.profile.maximum_cps;
    profile.timing.scene_before_ms = case.profile.scene_before_ms;
    profile.timing.scene_after_ms = case.profile.scene_after_ms;
    let rows = case
        .cues
        .iter()
        .map(|(start, end, text)| (*start, *end, text.as_str()))
        .collect::<Vec<_>>();
    let document = doc(&rows);
    let scenes = Scenes::Complete(case.scenes.into_iter().map(ms).collect());
    let request = Request {
        scope: case.scope.map(|ids| ids.into_iter().map(CueId).collect()),
        options: case.options,
        families: case.families,
    };
    super::oracle::Trial {
        document,
        profile,
        scenes,
        format: Format::Srt,
        request,
    }
}
fn check_case(number: u32) {
    let trial = trial_for_case(number);
    let proposal = trial.plan();
    let selected = proposal.default_units();
    let plan = proposal.select(&selected, || Ok(())).unwrap();
    assert!(plan.error.is_none());
    trial.check(&proposal, &selected, &plan.candidate);
}
#[test]
fn coupled_case_2499() {
    check_case(2499);
}
#[test]
fn coupled_case_2821() {
    check_case(2821);
}
#[test]
fn coupled_case_8513() {
    check_case(8513);
}
#[test]
fn coupled_case_8914() {
    check_case(8914);
}
#[test]
fn coupled_case_13186() {
    check_case(13186);
}

#[test]
fn manual_budget_sentences_distinguish_partial_and_singular() {
    let mut text_budget = Budget {
        effective_ms: 1000,
        max_counted: Some(10),
        shorten_by: Some(1),
        estimate: true,
        at: Endpoints {
            start_ms: 1000,
            end_ms: 2000,
        },
        at_partial: true,
    };
    assert_eq!(
        text_budget.description(),
        "Shorten by 1 counted character (maximum 10). Estimate with overlap-aware reading. At the unticked timing 00:00:01,000 → 00:00:02,000."
    );
    text_budget.shorten_by = Some(2);
    text_budget.estimate = false;
    text_budget.at_partial = false;
    assert_eq!(
        text_budget.description(),
        "Shorten by 2 counted characters (maximum 10)."
    );
    text_budget.max_counted = None;
    assert!(text_budget.description().contains("unavailable"));
}

#[test]
fn manual_causes_have_a_single_lead_and_at_most_two_facts() {
    let mut profile = Profile::default().custom("reasons".into(), "Reasons".into());
    profile.timing.scene_before_ms = 130;
    profile.timing.scene_after_ms = 130;
    let input = doc(&[
        (
            0,
            1000,
            "A longer line, to exercise both of the reading speed limits.",
        ),
        (1020, 2200, "Next"),
    ]);
    let report = evaluate_with_scenes(&input, &profile, &Scenes::NoMedia, |_, _| Ok(())).unwrap();
    let windows = Windows::new(
        &profile,
        &Scenes::Complete(vec![ms(40), ms(940)]),
        || Ok(()),
    )
    .unwrap();
    let mut context = manual::ReasonContext {
        families: Families {
            gaps: false,
            ..Default::default()
        },
        blocked: None,
        withheld: None,
        next: Some((1, 1020, true)),
        at: Endpoints {
            start_ms: 0,
            end_ms: 1000,
        },
        metric: &report.metrics[0],
        profile: &profile,
        windows: &windows,
        tolerance: 0,
    };
    let check = |context: &manual::ReasonContext<'_>,
                 rules: Vec<Rule>,
                 budget: Option<Budget>,
                 lead: &str| {
        if rules == [Rule::Gap] && lead.starts_with("Gap not fixed") {
            let mut overlap = Manual::new(0, vec![Rule::Overlap], budget.clone());
            overlap.explain(context);
            assert_eq!(overlap.lead, lead.replacen("Gap", "Overlap", 1));
        }
        let mut row = Manual::new(0, rules, budget);
        row.explain(context);
        assert_eq!(row.lead, lead);
        assert!(row.facts.len() <= 2);
        assert!(row.facts.iter().all(|fact| fact.ends_with('.')));
        row
    };
    let speed = check(
        &context,
        vec![Rule::Gap],
        None,
        "Gap not fixed: this cue would become too fast.",
    );
    assert!(speed.facts[0].contains("CPS") && speed.facts[0].contains("RS"));
    for blocked in [
        "Outside the selected scope.",
        "Finish the scene scan to fix this cue.",
        "Equal starts or nested cues need manual timing.",
        "Timed markup or an event effect needs manual timing.",
    ] {
        context.blocked = Some(blocked);
        check(&context, vec![], None, blocked);
    }
    context.blocked = None;
    context.withheld = Some(1);
    check(
        &context,
        vec![],
        None,
        "Depends on a change to cue 2 that would break a protected rule.",
    );
    context.withheld = None;
    let mut no_speed = profile.clone();
    no_speed.rules.speed.enabled = false;
    context.profile = &no_speed;
    context.next = Some((1, 700, true));
    check(
        &context,
        vec![Rule::Gap],
        None,
        "Gap not fixed: this cue would fall below the 800 ms minimum.",
    );
    context.next = Some((1, 1020, true));
    check(
        &context,
        vec![Rule::Gap],
        None,
        "Gap not fixed: the end would land inside a shot-change window.",
    );
    context.next = Some((1, 1200, false));
    check(
        &context,
        vec![Rule::DurationShort],
        None,
        "Cue 2 is outside the scope and holds this time.",
    );
    context.next = Some((1, 1200, true));
    check(
        &context,
        vec![Rule::DurationShort],
        None,
        "Too short: cue 2 starts at 00:00:01,200.",
    );
    let amount = budget(context.metric, context.at, 1000, false, &profile);
    check(
        &context,
        vec![Rule::CpsHigh],
        Some(amount),
        "Too fast: cue 2 starts at 00:00:01,200.",
    );
    context.next = Some((1, 100, true));
    check(
        &context,
        vec![Rule::SceneStart],
        None,
        "Not fixed: it would make gaps and overlaps worse.",
    );
    context.next = None;
    check(
        &context,
        vec![],
        None,
        "No allowed timing satisfies the enabled rules.",
    );
    assert_eq!(
        Manual::unticked(0).lead,
        "The change for this cue is unticked."
    );
}

#[test]
fn a_forced_candidate_limit_explains_whole_file_withholding() {
    let trial = trial_for_case(2499);
    let proposal = propose_with_limit(
        trial.document,
        Evaluation::from(&trial.profile).with_format(trial.format),
        &trial.scenes,
        trial.request,
        1,
        || Ok(()),
    )
    .unwrap();
    assert_eq!(proposal.evaluations, 1);
    assert!(proposal.withheld_all);
    assert!(proposal.units.is_empty());
    assert!(
        proposal
            .manual
            .iter()
            .any(|row| row.lead == WITHHELD_ALL_MESSAGE)
    );
    assert!(
        !proposal
            .manual
            .iter()
            .any(|row| row.lead.starts_with("Depends on a change"))
    );
}
