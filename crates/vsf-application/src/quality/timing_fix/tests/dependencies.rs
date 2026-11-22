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

fn check_case(number: u32) {
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
    let trial = super::oracle::Trial {
        document,
        profile,
        scenes,
        format: Format::Srt,
        request,
    };
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
