//! Frozen source, profile, cut times and applied endpoints survive removal of
//! the capture harness. Any changed mismatch invalidates its recorded receipt.
use serde_json::{Value, json};
use std::collections::BTreeMap;
use subtitle_core::{Edit, Milliseconds};
use vsf_application::{
    quality::{
        Evaluation, Profile,
        scene::Scenes,
        timing_fix::{self, Families, Request},
    },
    subtitle_file::parse_document,
};

#[test]
fn frozen_planners() {
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/timing-parity/legacy.json")).unwrap();
    let accepted: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/timing-parity/differences.json")).unwrap();
    let mut differences = Vec::new();
    for case in &cases {
        let parsed = parse_document(
            case["source"].as_str().unwrap().as_bytes(),
            case["format"].as_str(),
        )
        .unwrap();
        let profile: Profile = serde_json::from_value(case["profile"].clone()).unwrap();
        let context = Evaluation::from(&profile).with_format(parsed.source.format());
        let scenes = Scenes::Complete(
            case["cutsMs"]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| Milliseconds::new(x.as_u64().unwrap()).unwrap())
                .collect(),
        );
        for family in ["gaps", "speed", "shots"] {
            let request = Request {
                families: Families {
                    gaps: family == "gaps",
                    speed: family == "speed",
                    shots: family == "shots",
                },
                ..Request::default()
            };
            let plan = timing_fix::propose(
                parsed.document.clone(),
                context,
                &scenes,
                request,
                || Ok(()),
            )
            .unwrap();
            assert_eq!(plan.before().profile_hash, case["profileHash"]);
            let expected: BTreeMap<_, _> = case[family]["rows"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|r| !r["new"].is_null())
                .map(|r| (r["index"].as_u64().unwrap(), r["new"].clone()))
                .collect();
            let chosen = plan.select(&plan.default_units(), || Ok(())).unwrap();
            assert_eq!(chosen.error, None, "{} {family}", case["case"]);
            let mut actual = BTreeMap::new();
            if chosen.changed != 0 {
                let Edit::Retime { timings } = chosen.edit(&parsed.document).unwrap() else {
                    panic!("retime expected")
                };
                for (i, (t, c)) in timings.iter().zip(parsed.document.cues()).enumerate() {
                    if t.start != c.start() || t.end != c.end() {
                        actual.insert(i as u64, json!([t.start.get(), t.end.get()]));
                    }
                }
            }
            // Compare the priority tuple independently of candidate generation.
            // Legacy counts and endpoints are reference data, never live calls.
            let counts = |rules: &[&str], json: &Value| -> u64 {
                rules
                    .iter()
                    .map(|r| json.get(r).and_then(Value::as_u64).unwrap_or(0))
                    .sum()
            };
            let mut after = serde_json::Map::new();
            for f in &plan.after().findings {
                let rule = serde_json::to_value(f.rule).unwrap();
                let value = after.entry(rule.as_str().unwrap()).or_insert(json!(0));
                *value = json!(value.as_u64().unwrap() + 1);
            }
            let objective = |report: &Value, rows: &BTreeMap<u64, Value>| {
                let mut moved_start = 0;
                let mut moved_end = 0;
                for (index, t) in rows {
                    let c = &parsed.document.cues()[*index as usize];
                    moved_start += c.start().get().abs_diff(t[0].as_u64().unwrap());
                    moved_end += c.end().get().abs_diff(t[1].as_u64().unwrap());
                }
                let (first, second) = match family {
                    "speed" => (
                        counts(&["duration_short", "duration_long"], report),
                        counts(
                            &[
                                "cps_high",
                                "cps_low",
                                "rs_high",
                                "rs_low",
                                "speed_unavailable",
                            ],
                            report,
                        ),
                    ),
                    "gaps" => (counts(&["gap", "overlap"], report), 0),
                    _ => (counts(&["scene_start", "scene_end"], report), 0),
                };
                (first, second, moved_start, moved_end)
            };
            let old = objective(&case[family]["after"], &expected);
            let new = objective(&Value::Object(after), &actual);
            assert!(
                new <= old,
                "{} {family}: {new:?} exceeds legacy {old:?}",
                case["case"]
            );
            if actual != expected {
                differences.push(
                    json!({"case":case["case"],"family":family,"legacy":expected,"engine":actual}),
                );
            }
        }
    }
    let recorded: Vec<_> = accepted
        .into_iter()
        .map(|mut entry| {
            let fields = entry.as_object_mut().unwrap();
            assert!(
                fields
                    .remove("reason")
                    .is_some_and(|v| v.as_str().is_some_and(|s| !s.is_empty()))
            );
            assert!(fields.remove("category").is_some());
            entry
        })
        .collect();
    assert_eq!(
        differences,
        recorded,
        "{}",
        serde_json::to_string(&differences).unwrap()
    );
}
