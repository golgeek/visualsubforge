//! Six accepted 10k conversions and two exact-precision refusal workloads.
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::{
    io::{self, Write},
    time::{Duration, Instant},
};
use subtitle_core::Milliseconds;
use subtitle_formats::{Format, srt};
use vsf_application::{
    conversion::{self, ConversionFormat, ConversionOptions, ConversionPrecision},
    subtitle_file,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/generated/ass-ssa");
    let mut inputs = vec![
        ("ass", std::fs::read(root.join("10000.ass"))?),
        ("ssa", std::fs::read(root.join("10000.ssa"))?),
    ];
    // Independent synthetic SRT, with all endpoints requiring explicit rounding.
    // Both endpoints shift equally, including the timed fan-sub case.
    let mut srt_bytes = String::new();
    for i in 0..10_000 {
        let text = match i % 4 {
            0 => format!("<i>Cue {i:05} — 日本語 😀</i>\nSecond line"),
            1 => format!("{{\\an8\\k10}}Cue {i:05}\\NSecond line"),
            2 => format!(r"Cue {i:05} C:\new\here"),
            _ => format!("Plain cue {i:05} — été"),
        };
        writeln!(
            srt_bytes,
            "{}\n{} --> {}\n{text}\n",
            i + 1,
            srt::format_time(Milliseconds::new(i * 1500 + 1)?),
            srt::format_time(Milliseconds::new(i * 1500 + 1001)?)
        )?;
    }
    inputs.push(("srt", srt_bytes.into_bytes()));
    println!("fixture_ready pid={} cues=10000", std::process::id());
    io::stdout().flush()?;
    std::thread::sleep(Duration::from_millis(500));
    let mut workloads = vec![];
    for (extension, bytes) in inputs {
        let parsed = subtitle_file::parse_document(&bytes, Some(extension))?;
        assert_eq!(parsed.document.cues().len(), 10_000);
        for target in [
            ConversionFormat::Srt,
            ConversionFormat::Ass,
            ConversionFormat::Ssa,
        ] {
            if target.format() == parsed.source.format() {
                continue;
            }
            let policies = if parsed.source.format() == Format::Srt {
                vec![ConversionPrecision::Nearest, ConversionPrecision::Exact]
            } else {
                vec![ConversionPrecision::Exact]
            };
            for precision in policies {
                let blocked = parsed.source.format() == Format::Srt
                    && precision == ConversionPrecision::Exact;
                println!(
                    "phase {extension}-{}-{precision:?}",
                    target.format().extension()
                );
                io::stdout().flush()?;
                let mut times = vec![];
                for run in 0..21 {
                    let began = Instant::now();
                    let proposal = conversion::propose(
                        &parsed.document,
                        &parsed.source,
                        ConversionOptions { target, precision },
                        || Ok(()),
                    )?;
                    let elapsed = began.elapsed().as_secs_f64() * 1000.;
                    assert_eq!(proposal.report().blocked(), blocked);
                    assert_eq!(proposal.output().is_none(), blocked);
                    if let Some(output) = proposal.output() {
                        let result = subtitle_file::parse_document(
                            output.as_bytes(),
                            Some(target.format().extension()),
                        )?;
                        assert_eq!(result.document.cues().len(), 10_000);
                        for (before, after) in
                            parsed.document.cues().iter().zip(result.document.cues())
                        {
                            let delta = u64::from(extension == "srt");
                            assert_eq!(after.start().get(), before.start().get() - delta);
                            assert_eq!(after.end().get(), before.end().get() - delta);
                        }
                    }
                    if run > 0 {
                        times.push(elapsed);
                    }
                }
                times.sort_by(f64::total_cmp);
                assert_eq!(parsed.source.serialize(&parsed.document)?.as_bytes(), bytes);
                workloads.push(serde_json::json!({"source": extension, "target": target, "precision": precision, "blocked": blocked, "cues": 10000, "input_sha256": format!("{:x}", Sha256::digest(&bytes)), "bytes": bytes.len(), "runs": 20, "p95_ms": times[18], "maximum_ms": times[19]}));
            }
        }
    }
    let passed = workloads
        .iter()
        .all(|w| w["p95_ms"].as_f64().unwrap() <= 500.);
    println!(
        "{}",
        serde_json::json!({"cpu_budget_ms": 500, "workloads": workloads, "passed": passed})
    );
    if !passed {
        return Err("Conversion CPU budget exceeded".into());
    }
    Ok(())
}
