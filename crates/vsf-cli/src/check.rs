//! Output/argument adapter only; both application callers share the evaluator.
use crate::args::Check;
use std::{
    io::{self, Write},
    path::Path,
};
use vsf_application::{
    quality::{self, Error, Profile},
    resync::Cancellation,
};

pub fn error_code(error: &Error) -> u8 {
    match error {
        Error::Invalid(_) | Error::Limit(_) => 2,
        Error::Io(_) => 5,
        Error::Cancelled => 130,
    }
}

pub fn run(args: Check, cancel: &Cancellation) -> quality::Result<u8> {
    let profile = match Profile::builtin(&args.profile) {
        Some(profile) => profile,
        None => quality::read_profile(Path::new(&args.profile))?,
    };
    let report = quality::check_file(&args.subtitles, &profile, |_, _| {
        if cancel.is_cancelled() {
            Err(Error::Cancelled)
        } else {
            Ok(())
        }
    })?;
    let status = if !report.complete {
        3
    } else if report.findings.is_empty() {
        0
    } else {
        1
    };
    let mut stdout = io::stdout().lock();
    if args.json {
        serde_json::to_writer(&mut stdout, &report)
            .map_err(|e| Error::Io(format!("Cannot write quality report: {e}")))?;
        writeln!(stdout).map_err(|e| Error::Io(e.to_string()))?;
    } else {
        writeln!(
            stdout,
            "{} (revision {}) — {} cues, {} findings, {} affected cues",
            profile.name,
            profile.revision,
            report.metrics.len(),
            report.findings.len(),
            report.affected_cues
        )
        .map_err(|e| Error::Io(e.to_string()))?;
        writeln!(
            stdout,
            "Coverage: {}",
            if report.complete {
                "all requested rules checked"
            } else {
                "partial — unsupported rules or missing scene data"
            }
        )
        .map_err(|e| Error::Io(e.to_string()))?;
        writeln!(
            stdout,
            "Scene coverage: {:?} (completed visual detection remains heuristic)",
            report.scene_coverage
        )
        .map_err(|e| Error::Io(e.to_string()))?;
        for limitation in &profile.limitations {
            writeln!(stdout, "Not checked: {limitation}").map_err(|e| Error::Io(e.to_string()))?;
        }
        for finding in &report.findings {
            let measured = finding
                .measured
                .map_or_else(|| "unavailable".into(), |n| format!("{n:.3}"));
            writeln!(
                stdout,
                "Cue {} [{:?}/{:?}]: {} — measured {}; expected {}",
                finding.cue_index + 1,
                finding.severity,
                finding.rule,
                finding.explanation,
                measured,
                finding.expected
            )
            .map_err(|e| Error::Io(e.to_string()))?;
        }
    }
    stdout.flush().map_err(|e| Error::Io(e.to_string()))?;
    Ok(status)
}
