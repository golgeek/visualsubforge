//! The browser replays snapshots from the same Rust projection as desktop IPC.
//! Set VSF_UPDATE_TIMING_EPISODE=1 to regenerate and review the fixture.
use super::*;
use serde_json::{Value, json};
use std::path::Path;

fn snapshot(proposal: &Proposal, toggled: &[u32]) -> Value {
    let review = proposal.review(toggled, false, || Ok(())).unwrap();
    json!({"withheldAll":review.withheld_all,"coverage":proposal.before().scene_coverage,"rows":review.changes,"manual":review.manual,
           "before":review.before,"after":review.after,"changed":review.selected,"trades":review.trades,
           "withoutTradesToggles":review.without_trades_toggles,"selectionError":review.plan.error,"selectionConflicts":review.plan.selection_conflicts})
}

#[test]
fn browser_episode_matches_rust_review() {
    let document = doc(&[
        (
            1000,
            2500,
            "Hello there, how are you doing\ntoday, my dear old friend?",
        ),
        (2520, 4000, "Fine, thanks."),
        (4100, 4600, "Hi."),
        (6000, 8200, "We should leave before the rain starts."),
        (8000, 10000, "I know, I know."),
        (10500, 11950, "Did you hear that?"),
        (12030, 13500, "It came from the cellar."),
        (
            14000,
            15000,
            "This is a rather long line that needs time\nto be read comfortably by everyone here.",
        ),
        (15080, 16500, "Then read it slowly."),
        (20000, 21000, "<i>Somewhere far away...</i>"),
    ]);
    let profile = Profile::default();
    let cases = [
        ("default", Priority::ReadingSpeed, 0, SceneEdges::Inward),
        ("spacing", Priority::Spacing, 0, SceneEdges::Inward),
        ("starts", Priority::Spacing, 300, SceneEdges::Inward),
        ("nearest", Priority::Spacing, 300, SceneEdges::Nearest),
    ];
    let mut snapshots = serde_json::Map::new();
    for (key, priority, start_tolerance_ms, scene_edges) in cases {
        let proposal = propose(
            document.clone(),
            (&profile).into(),
            &Scenes::Complete(vec![ms(12000), ms(17990)]),
            Request {
                options: Options {
                    priority,
                    start_tolerance_ms,
                    scene_edges,
                },
                ..Default::default()
            },
            || Ok(()),
        )
        .unwrap();
        snapshots.insert(key.into(), snapshot(&proposal, &[]));
        if key == "default" {
            let off = snapshot(&proposal, &[5]);
            assert!(
                off["rows"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|r| r["cueIndex"] == 6 && r["selected"] == true)
            );
            snapshots.insert("defaultOff".into(), off);
        }
        if key == "nearest" {
            let off = snapshot(&proposal, &[2]);
            assert_eq!(off["changed"], 4);
            for id in [3, 5] {
                assert!(
                    off["rows"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|r| r["cueIndex"] == id && r["selected"] == true)
                );
            }
            snapshots.insert("untick".into(), off);
            snapshots.insert("retick".into(), snapshot(&proposal, &[2, 2]));
            let partial = snapshot(&proposal, &[7]);
            assert!(
                partial["rows"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|r| r["cueIndex"] == 9 && r["selected"] == true)
            );
            snapshots.insert("partial".into(), partial);
        }
    }
    let incomplete = propose(
        document.clone(),
        (&profile).into(),
        &Scenes::Partial,
        Request::default(),
        || Ok(()),
    )
    .unwrap();
    snapshots.insert("incomplete".into(), snapshot(&incomplete, &[]));
    let cues:Vec<_>=document.cues().iter().map(|cue|json!({"id":cue.id().0,"startMs":cue.start().get(),"endMs":cue.end().get(),"text":cue.text()})).collect();
    let actual = json!({"cues":cues,"views":snapshots});
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/frontend/scene-quality/timing-episode.json");
    let bytes = serde_json::to_string_pretty(&actual).unwrap() + "\n";
    if std::env::var_os("VSF_UPDATE_TIMING_EPISODE").is_some() {
        std::fs::write(&path, &bytes).unwrap();
    }
    assert!(
        std::fs::read_to_string(path).unwrap() == bytes,
        "regenerate with VSF_UPDATE_TIMING_EPISODE=1"
    );
}
