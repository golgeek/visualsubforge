//! Pure WebVTT parse/edit/serialize/reopen/history measurement. No desktop IO.
//! Input generation precedes the RSS baseline; parsed state is measured.
use sha2::{Digest, Sha256};
use std::{
    io::{self, Write},
    time::{Duration, Instant},
};
use subtitle_core::{Edit, Editor, Milliseconds};
use subtitle_formats::webvtt;

fn fixture(kind: &str) -> Result<String, String> {
    let mut raw = String::from("WEBVTT independent 10k fixture\n\n");
    if kind != "plain" {
        raw.push_str(
            "STYLE\n::cue(.accent) { color: lime; }\n\nREGION\nid:bottom\nwidth:80%\nlines:3\n\n",
        );
    }
    for i in 0..10_000 {
        let start = i * 3000 + 1;
        if kind != "plain" {
            raw.push_str(&format!("cue,{i}\n"));
        }
        raw.push_str(&format!(
            "{} --> {}",
            webvtt::format_time(Milliseconds::new(start)?),
            webvtt::format_time(Milliseconds::new(start + 2500)?)
        ));
        if kind != "plain" {
            raw.push_str(" line:10%,start position:20%,line-left region:bottom future:keep");
        }
        raw.push('\n');
        match kind {
            "plain" => raw.push_str(&format!("Bonjour 世界 😀 {i}\nSecond line")),
            "styled" => raw.push_str(&format!(
                "<v Alice><lang fr><c.accent>Été &amp; 世界 {i}</c></lang></v>\n<ruby>字<rt>じ</rt></ruby>"
            )),
            _ => raw.push_str(&format!(
                "<v Alice>Été &amp; 世界 {i} <{}>first <{}>second</v>",
                webvtt::format_time(Milliseconds::new(start + 500)?),
                webvtt::format_time(Milliseconds::new(start + 2000)?)
            )),
        }
        raw.push_str("\n\n");
        if i % 100 == 0 {
            raw.push_str("NOTE independent preservation sentinel\n\n");
        }
    }
    Ok(match kind {
        "styled" => format!("\u{feff}{}", raw.replace('\n', "\r\n")),
        "timed" => {
            let mut mixed = String::new();
            for (i, line) in raw.split_inclusive('\n').enumerate() {
                mixed.push_str(line.trim_end_matches('\n'));
                let newline = ["\r", "\n", "\r\n"][i % 3];
                // A bare CR followed by an empty LF line would coalesce into one
                // CRLF terminator and erase the required block separator.
                if mixed.ends_with('\r') && newline == "\n" {
                    mixed.push('\r');
                }
                mixed.push_str(newline);
            }
            mixed
        }
        _ => raw,
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let fixtures = ["plain", "styled", "timed"]
        .into_iter()
        .map(|kind| fixture(kind).map(|text| (kind, text)))
        .collect::<Result<Vec<_>, _>>()?;
    println!(
        "fixture_ready pid={} cues=10000 input_bytes={}",
        std::process::id(),
        fixtures.iter().map(|(_, text)| text.len()).sum::<usize>()
    );
    io::stdout().flush()?;
    std::thread::sleep(Duration::from_millis(500));
    let mut workloads = vec![];
    for (kind, original) in fixtures {
        println!("phase {kind}");
        io::stdout().flush()?;
        let mut times = vec![];
        for run in 0..21 {
            let start = Instant::now();
            let parsed = webvtt::parse(&original)?;
            assert_eq!(parsed.document.cues().len(), 10_000);
            let mut editor = Editor::with_properties(parsed.document, parsed.source);
            let last = editor.document().cues().last().ok_or("missing last cue")?;
            let edit = Edit::Text {
                id: last.id(),
                start: Milliseconds::new(last.start().get() + 1)?,
                end: Milliseconds::new(last.end().get() + 1)?,
                text: format!("{} — edited", last.text()),
            };
            editor.apply_edit_with_properties(0, edit, |d, s| {
                webvtt::serialized_len(d, s).map(|_| ())
            })?;
            let output = webvtt::serialize(editor.document(), editor.properties())?;
            assert_eq!(
                output.len(),
                webvtt::serialized_len(editor.document(), editor.properties())?
            );
            let reopened = webvtt::parse(&output)?;
            assert_eq!(&reopened.document, editor.document());
            editor.undo(editor.revision())?;
            assert_eq!(
                webvtt::serialize(editor.document(), editor.properties())?,
                original
            );
            assert!(!editor.dirty());
            editor.redo(editor.revision())?;
            assert_eq!(
                webvtt::serialize(editor.document(), editor.properties())?,
                output
            );
            let elapsed = start.elapsed().as_secs_f64() * 1000.;
            if run > 0 {
                times.push(elapsed);
            }
        }
        times.sort_by(f64::total_cmp);
        workloads.push(serde_json::json!({
            "workload": kind, "cues": 10_000, "bytes": original.len(),
            "sha256": format!("{:x}", Sha256::digest(&original)),
            "runs": 20, "p95_ms": times[18], "maximum_ms": times[19],
        }));
    }
    let passed = workloads
        .iter()
        .all(|workload| workload["p95_ms"].as_f64().unwrap() <= 500.);
    println!(
        "{}",
        serde_json::json!({"cpu_budget_ms": 500, "workloads": workloads, "passed": passed})
    );
    if !passed {
        return Err("WebVTT parse/edit/serialize/reopen/history exceeded its CPU budget".into());
    }
    Ok(())
}
