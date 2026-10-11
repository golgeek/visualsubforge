use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "vsf",
    version,
    about = "VisualSubForge headless subtitle operations"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Transfer SRT timings between versions of the same programme.
    Resync(Resync),
}

#[derive(Args)]
#[command(
    after_help = "Matches content in order. Missing or ambiguous footage remains unresolved.\nOutputs must be new paths. The report is published first, then the SRT; failures may leave only a report.\nInstalled releases use bundled FFmpeg tools. Development builds allow VSF_FFMPEG and VSF_FFPROBE overrides. The first video stream is used.\nExit codes: 0 complete/analysis-only, 2 input, 3 needs review, 4 analysis/tool, 5 IO, 6 changed input, 7 timeout, 130 cancelled."
)]
pub struct Resync {
    /// Video to which the existing subtitles are timed.
    #[arg(long, value_name = "FILE")]
    pub source_video: PathBuf,
    /// Correctly timed UTF-8 SRT for the source video.
    #[arg(long, value_name = "FILE")]
    pub source_subtitles: PathBuf,
    /// Destination version of the programme.
    #[arg(long, value_name = "FILE")]
    pub target_video: PathBuf,
    /// New SRT path; no SRT is published while any cue remains unresolved.
    #[arg(
        long,
        value_name = "FILE",
        required_unless_present = "analysis_only",
        conflicts_with = "analysis_only"
    )]
    pub output: Option<PathBuf>,
    /// New path for the versioned JSON proposal report.
    #[arg(long, value_name = "FILE")]
    pub report: PathBuf,
    /// Publish only the report; unresolved cues still return exit code 3.
    #[arg(long)]
    pub analysis_only: bool,
    /// Total time budget, including input hashing and publication preparation.
    #[arg(long, default_value_t = 1800, value_parser = clap::value_parser!(u64).range(1..=14_400))]
    pub timeout_seconds: u64,
    /// Emit one JSON completion summary to stdout; progress stays on stderr.
    #[arg(long)]
    pub json: bool,
}
