//! Thin synchronous CLI adapter. Shared application operations own analysis,
//! validation and file publication; this process never loads desktop settings.
mod args;
mod check;
mod stats;
use args::{Cli, Command, Resync};
use clap::Parser;
use media_analysis::Tools;
use serde::Serialize;
use std::{
    io::{self, Write},
    path::Path,
    process::ExitCode,
    time::Duration,
};
use vsf_application::resync::{
    self, Budget, Cancellation, Error, OutputPaths, Publication, Request, Stage, Subtitles,
};

fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            let code = if error.use_stderr() { 2 } else { 0 };
            let _ = error.print();
            return ExitCode::from(code);
        }
    };
    let cancel = Cancellation::default();
    let token = cancel.clone();
    if let Err(error) = ctrlc::set_handler(move || token.cancel()) {
        diagnostic(format!("Cannot install cancellation handler: {error}"));
        return ExitCode::from(5);
    }
    // Returning from main drops all owned state. Do not exit from a signal
    // handler: the current synchronous call must reap children/clean temporaries.
    let result = match cli.command {
        Command::Resync(args) => run(args, &cancel).map_err(|error| {
            let code = match &error {
                Error::InvalidInput(_) => 2,
                Error::Analysis(_) => 4,
                Error::Io(_) => 5,
                Error::InputChanged(_) => 6,
                Error::Timeout => 7,
                Error::Cancelled => 130,
            };
            (code, error.to_string())
        }),
        Command::Stats(args) => stats::run(args, &cancel)
            .map_err(|error| (check::error_code(&error), error.to_string())),
        Command::Check(args) => check::run(args, &cancel)
            .map_err(|error| (check::error_code(&error), error.to_string())),
    };
    match result {
        Ok(code) => ExitCode::from(code),
        Err((code, error)) => {
            diagnostic(format!("vsf: {error}"));
            ExitCode::from(code)
        }
    }
}

fn run(args: Resync, cancel: &Cancellation) -> resync::Result<u8> {
    let budget = Budget::new(cancel, Duration::from_secs(args.timeout_seconds));
    let paths = OutputPaths::new(&args.report, args.output.as_deref())?;
    let analysis = resync::analyze(
        &Tools::discover()?,
        Request {
            source_video: args.source_video,
            target_video: args.target_video,
            subtitles: Subtitles::File(args.source_subtitles),
        },
        cancel,
        budget.remaining()?,
        |stage| diagnostic(stage_label(stage)),
    )?;
    let complete = analysis.proposal().is_complete();
    let status = if !complete {
        "needs_review"
    } else if args.analysis_only {
        "analysis_only"
    } else {
        "complete"
    };
    #[derive(Serialize)]
    struct Summary<'a> {
        schema_version: u32,
        status: &'a str,
        report: &'a Path,
        output: Option<&'a Path>,
        unresolved_cues: usize,
    }
    // Prepare JSON before publishing, so serialization cannot leave a published
    // file followed by a path-encoding error. A closed stdout after publication
    // remains an IO error; published output is never removed to hide that failure.
    let json = if args.json {
        Some(
            serde_json::to_vec(&Summary {
                schema_version: 1,
                status,
                report: paths.report(),
                output: if complete { paths.subtitles() } else { None },
                unresolved_cues: analysis
                    .proposal()
                    .cues()
                    .iter()
                    .filter(|cue| matches!(cue.outcome, resync::CueOutcome::Unresolved { .. }))
                    .count(),
            })
            .map_err(|e| Error::InvalidInput(format!("Cannot encode completion summary: {e}")))?,
        )
    } else {
        None
    };
    let publication = analysis.publish(&paths, &budget)?;
    diagnostic(match publication {
        Publication::Complete => "Published report and retimed subtitles".to_string(),
        Publication::AnalysisOnly => "Published proposal report (analysis only)".to_string(),
        Publication::NeedsReview => {
            "Published report; unresolved cues require review. No subtitle file was created"
                .to_string()
        }
    });
    if let Some(json) = json {
        let mut stdout = io::stdout().lock();
        stdout
            .write_all(&json)
            .and_then(|_| stdout.write_all(b"\n"))
            .and_then(|_| stdout.flush())
            .map_err(|e| {
                Error::Io(format!(
                    "Outputs published, but cannot write stdout summary: {e}"
                ))
            })?;
    }
    Ok(if publication == Publication::NeedsReview {
        3
    } else {
        0
    })
}

fn diagnostic(message: impl std::fmt::Display) {
    // A broken progress stream must not panic while owning an analysis job.
    let _ = writeln!(io::stderr().lock(), "{message}");
}

fn stage_label(stage: Stage) -> &'static str {
    match stage {
        Stage::ReadInputs => "Reading and hashing inputs",
        Stage::ToolVersions => "Checking FFmpeg tools",
        Stage::SourceCuts => "Analyzing source scene cuts",
        Stage::SourceSamples => "Sampling source video",
        Stage::TargetCuts => "Analyzing destination scene cuts",
        Stage::TargetSamples => "Sampling destination video",
        Stage::Match => "Matching content blocks",
        Stage::VerifySamples => "Verifying supported intervals",
        Stage::Propose => "Proposing cue timings",
        Stage::VerifyInputs => "Verifying unchanged inputs",
    }
}
