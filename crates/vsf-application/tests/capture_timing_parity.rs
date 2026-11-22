//! Regenerate only against the original planners. The ordinary parity test
//! consumes self-contained inputs and never calls this harness.
use serde::Serialize;
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path};
use subtitle_core::{Document, Milliseconds};
use subtitle_formats::Format;
use vsf_application::{
    quality::{Profile, Report, duration, gap, profile::SpeedBasis, scene},
    subtitle_file::parse_document,
};

fn counts(report: &Report) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for finding in &report.findings {
        let name = serde_json::to_value(finding.rule).unwrap();
        *counts.entry(name.as_str().unwrap().to_owned()).or_default() += 1;
    }
    counts
}

fn row(
    doc: &Document,
    index: usize,
    start: Option<Milliseconds>,
    end: Option<Milliseconds>,
    skip: impl Serialize,
) -> Value {
    let cue = &doc.cues()[index];
    json!({"index":index,"old":[cue.start().get(),cue.end().get()],
        "new":end.map(|end| [start.unwrap_or(cue.start()).get(),end.get()]),"skip":skip})
}

fn capture(name: &str, source: &str, format: Format, cuts: &[u64], profile: &Profile) -> Value {
    let doc = parse_document(source.as_bytes(), Some(format.extension()))
        .unwrap()
        .document;
    let scenes = scene::Scenes::Complete(
        cuts.iter()
            .map(|&c| Milliseconds::new(c).unwrap())
            .collect(),
    );
    let gaps = gap::propose_for_format(doc.clone(), format, profile, &scenes, || Ok(())).unwrap();
    let speed = duration::propose_for_format(
        doc.clone(),
        format,
        profile,
        &scenes,
        duration::Scope::Document,
        || Ok(()),
    )
    .unwrap();
    let shots =
        scene::propose_for_format(doc.clone(), format, profile, &scenes, || Ok(())).unwrap();
    let reference = |before: &Report, after: &Report, rows: Vec<Value>| json!({"before": counts(before), "after": counts(after), "rows": rows});
    json!({
        "case":name, "format":format.extension(),"source":source,"cutsMs":cuts,"profile":profile,"profileHash":gaps.before().profile_hash,
        "gaps": reference(gaps.before(),gaps.after(),gaps.rows().iter().map(|r| row(&doc,r.cue_index,None,r.new_end,r.skipped)).collect()),
        "speed": reference(speed.before(),speed.after(),speed.rows().iter().map(|r| row(&doc,r.cue_index,None,r.new_end,r.skipped)).collect()),
        "shots": reference(shots.before(),shots.after(),shots.rows().iter().map(|r| row(&doc,r.cue_index,r.new_start,r.new_end,r.skipped)).collect())
    })
}

fn generated(seed: u32, format: Format) -> (String, Vec<u64>) {
    use std::fmt::Write;
    let mut state = seed;
    let mut draw = |limit| {
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        (state >> 8) % limit
    };
    let mut source = match format {
        Format::Ass => "[Script Info]\nScriptType: v4.00+\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n".to_string(),
        _ => String::new(),
    };
    let mut start = 0;
    let mut cuts = vec![0];
    for index in 1..=8 {
        let end = start + 200 + u64::from(draw(190)) * 10;
        let text = [
            r"{\i1}日本語 😀",
            r"{\k20}Timed",
            "A twenty-character line",
            "Hi",
            "<i>Read</i> the second line",
        ][draw(5) as usize];
        let stamp = |t: u64| match format {
            Format::Ass => format!("0:00:{:02}.{:02}", t / 1000, t % 1000 / 10),
            _ => format!("00:00:{:02},{:03}", t / 1000, t % 1000),
        };
        if format == Format::Ass {
            writeln!(
                source,
                "Dialogue: 0,{},{},Default,,0,0,0,,{text}",
                stamp(start),
                stamp(end)
            )
            .unwrap();
        } else {
            writeln!(
                source,
                "{index}\n{} --> {}\n{text}\n",
                stamp(start),
                stamp(end)
            )
            .unwrap();
        }
        if index % 3 == 0 {
            cuts.push(end + 5);
        }
        start += 500 + u64::from(draw(120)) * 10;
    }
    cuts.sort_unstable();
    cuts.dedup();
    (source, cuts)
}

#[test]
#[ignore = "writes frozen legacy planner output"]
fn capture_timing_parity() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut cases = vec![];
    for file in [
        "editing.srt",
        "subtitles/resync.srt",
        "subtitles/resync.ass",
        "subtitles/resync.ssa",
        "subtitles/styles.vtt",
        "comparison/reordered-overlap/original.srt",
        "comparison/text-timing/revised.srt",
    ] {
        let source = fs::read_to_string(root.join("../../tests/fixtures").join(file)).unwrap();
        cases.push(capture(
            file,
            &source,
            Format::from_extension(file.rsplit('.').next().unwrap()).unwrap(),
            &[0, 1000, 3000],
            &Profile::default(),
        ));
    }
    let mut profile =
        Profile::default().custom("desktop-duration".into(), "Desktop duration".into());
    profile.rules.duration.maximum_ms = 3000;
    profile.rules.speed.minimum_cps = Some(5);
    profile.rules.speed.maximum_cps = Some(10);
    cases.push(capture("desktop-duration-small","1\n00:00:01,000 --> 00:00:01,400\n{\\i1}0123456789é😀abcdefg\n\n2\n00:00:05,000 --> 00:00:13,000\n0123456789\n\n3\n00:00:14,000 --> 00:00:14,400\n{\\k20}Timed\n",Format::Srt,&[3000,7000],&profile));
    let mut profile = Profile::default().custom("desktop-scene".into(), "Desktop scene".into());
    profile.rules.speed.enabled = false;
    cases.push(capture("desktop-scene-small","1\n00:00:01,000 --> 00:00:03,000\n{\\i1}First é 😀\n\n2\n00:00:04,050 --> 00:00:06,050\nSecond\n\n3\n00:00:08,000 --> 00:00:10,000\n{\\k20}Timed\n",Format::Srt,&[1000,4000,6000,8000],&profile));
    for seed in 1..=12 {
        for format in [Format::Srt, Format::Ass] {
            let (source, cuts) = generated(seed, format);
            let mut profile = Profile::default().custom("parity".into(), "Parity".into());
            if seed % 2 == 0 {
                profile.speed_basis = SpeedBasis::VssOverlapV1;
            }
            cases.push(capture(
                &format!("seed-{seed}-{}", format.extension()),
                &source,
                format,
                &cuts,
                &profile,
            ));
        }
    }
    fs::write(
        root.join("tests/fixtures/timing-parity/legacy.json"),
        serde_json::to_string_pretty(&cases).unwrap() + "\n",
    )
    .unwrap();
}
