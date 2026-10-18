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
    /// Check subtitle authoring rules without changing the file.
    Check(Check),
    /// Summarize accepted subtitle metrics without changing the file.
    Stats(Stats),
    /// Transfer SRT timings between versions of the same programme.
    Resync(Resync),
}

#[derive(Args)]
#[command(
    after_help = "Profiles: vsf-default, sw3-uk-vo-subset, or a version-2 .vsfpreset path.\nThe archived SW profile is explicitly partial; unsupported checks never count as passing.\nNo media tools, GUI, or desktop settings are required. Input files are never modified.\nExit codes: 0 no findings and complete coverage, 1 findings, 2 invalid input/limit, 3 incomplete coverage (possibly with findings), 5 IO, 130 cancelled."
)]
pub struct Check {
    /// UTF-8 SRT, ASS or SSA to check.
    #[arg(long, value_name = "FILE")]
    pub subtitles: PathBuf,
    /// Explicit built-in ID or quality-preset path. Prefix a colliding filename with ./.
    #[arg(long, value_name = "ID_OR_FILE")]
    pub profile: String,
    /// Emit the complete versioned report as JSON instead of human-readable text.
    #[arg(long)]
    pub json: bool,
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

#[derive(Args)]
#[command(
    after_help = "Scope uses one-based source cue positions, not SRT labels. Metrics keep full-document reading context.\nExcluded/unavailable values have explicit counts and denominators. No media decoding or desktop settings.\nExit codes: 0 report with complete quality coverage, 3 partial coverage, 2 invalid input/limit, 5 IO, 130 cancelled. Findings do not change these codes."
)]
pub struct Stats {
    /// UTF-8 SRT, ASS or SSA to summarize.
    #[arg(long, value_name = "FILE")]
    pub subtitles: PathBuf,
    /// Built-in ID or version-2 .vsfpreset path.
    #[arg(long, value_name = "ID_OR_FILE")]
    pub profile: String,
    /// Inclusive first source cue position; requires --to-cue.
    #[arg(long, requires = "to_cue")]
    pub from_cue: Option<u32>,
    /// Inclusive last source cue position; requires --from-cue.
    #[arg(long, requires = "from_cue")]
    pub to_cue: Option<u32>,
    /// Excluded source cue positions within the scope; may be repeated or comma-separated.
    #[arg(long, value_delimiter = ',')]
    pub exclude: Vec<u32>,
    /// Stream the versioned summary and all scoped findings as JSON.
    #[arg(long)]
    pub json: bool,
}
