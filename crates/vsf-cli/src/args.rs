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
    /// Run local JavaScript subtitle checks and reviewable fixes.
    Plugin(Plugin),
    /// Check subtitle authoring rules without changing the file.
    Check(Check),
    /// Summarize accepted subtitle metrics without changing the file.
    Stats(Stats),
    /// Transfer SRT/ASS/SSA/WebVTT timings between versions of the same programme.
    Resync(Resync),
}

#[derive(Args)]
pub struct Plugin {
    #[command(subcommand)]
    pub command: PluginCommand,
}
#[derive(Subcommand)]
pub enum PluginCommand {
    /// Report plugin findings without changing files (exit 1 for findings).
    Check(PluginRun),
    /// Preview every proposed fix and obtain an exact review ID; no files change.
    Preview(PluginRun),
    /// Rerun a reviewed proposal and publish matching changes to a new file.
    Apply(PluginApply),
}
#[derive(Args)]
#[command(
    after_help = "Plugins are local code running with bounded time/memory and no filesystem/network host bindings. This is not an OS security sandbox.\nDefaults use the VSF default gap/duration/CPS policy; scenes, cursor and RS index are unavailable unless provided by --context.\nApply reruns the plugin: inputs, settings, findings and proposed changes must reproduce the preview's review ID. Outputs must be new same-format paths.\nExit codes: 0 complete, 1 check findings, 2 input/limit, 4 script/worker, 5 IO, 6 changed input/review, 7 timeout, 130 cancelled."
)]
pub struct PluginRun {
    /// Source UTF-8 SRT, ASS, SSA or WebVTT (never overwritten).
    #[arg(long, value_name = "FILE")]
    pub subtitles: PathBuf,
    /// JSON manifest in a local package containing at most 64 JavaScript files.
    #[arg(long, value_name = "MANIFEST")]
    pub package: PathBuf,
    /// One-based cue positions to check/fix; omitted means all. May be repeated.
    #[arg(long, value_delimiter = ',')]
    pub cue: Vec<u32>,
    /// One-based positions exposed as selection; omitted means no selection.
    #[arg(long, value_delimiter = ',')]
    pub select: Vec<u32>,
    /// Override a declared scalar setting, e.g. ParamMode=3 or removeTabs=false.
    #[arg(long,value_name="NAME=JSON",value_parser=parse_plugin_setting)]
    pub setting: Vec<(String, serde_json::Value)>,
    /// Explicit JSON preferences, cursorMs and/or completed mapped scenes.
    #[arg(long, value_name = "FILE")]
    pub context: Option<PathBuf>,
    /// Whole operation budget including IO and publication preparation.
    #[arg(long,default_value_t=10000,value_parser=clap::value_parser!(u32).range(1..=60000))]
    pub timeout_ms: u32,
    /// Maximum helper execution time, also bounded by the remaining whole budget.
    #[arg(long,default_value_t=1000,value_parser=clap::value_parser!(u32).range(1..=10000))]
    pub execution_ms: u32,
    /// Emit the complete versioned review as JSON.
    #[arg(long)]
    pub json: bool,
}
#[derive(Args)]
pub struct PluginApply {
    #[command(flatten)]
    pub run: PluginRun,
    /// Exact review ID printed by plugin preview with the same inputs/options.
    #[arg(long,value_parser=parse_review_id)]
    pub accept: String,
    /// New output file with the same subtitle extension.
    #[arg(long, value_name = "FILE")]
    pub output: PathBuf,
}
fn parse_plugin_setting(text: &str) -> Result<(String, serde_json::Value), String> {
    let (name, value) = text
        .split_once('=')
        .ok_or("Use NAME=JSON for a plugin setting")?;
    Ok((
        name.into(),
        serde_json::from_str(value).map_err(|e| format!("Invalid setting JSON: {e}"))?,
    ))
}
fn parse_review_id(text: &str) -> Result<String, String> {
    if text.len() != 64
        || !text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("Use the 64-character lowercase review ID from plugin preview".into());
    }
    Ok(text.into())
}

#[derive(Args)]
#[command(
    after_help = "Profiles: vsf-default, sw3-uk-vo-subset, or a version-2 .vsfpreset path.\nThe archived SW profile is explicitly partial; unsupported checks never count as passing.\nNo media tools, GUI, or desktop settings are required. Input files are never modified.\nExit codes: 0 no findings and complete coverage, 1 findings, 2 invalid input/limit, 3 incomplete coverage (possibly with findings), 5 IO, 130 cancelled."
)]
pub struct Check {
    /// UTF-8 SRT, ASS, SSA or WebVTT to check.
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
    after_help = "Matches content in order. Missing or ambiguous footage remains unresolved.\nEndpoints round directly to the source grid (1ms SRT; 10ms ASS/SSA), nearest with ties later.\nTags stay unchanged. Timed or ambiguous cues with changed durations require review, including ASS tags in SRT.\nOutputs must be new paths. The report is published first, then the subtitles; failures may leave only a report.\nInstalled releases use bundled FFmpeg tools. Development builds allow VSF_FFMPEG and VSF_FFPROBE overrides. The first video stream is used.\nExit codes: 0 complete/analysis-only, 2 input, 3 needs review, 4 analysis/tool, 5 IO, 6 changed input, 7 timeout, 130 cancelled."
)]
pub struct Resync {
    /// Video to which the existing subtitles are timed.
    #[arg(long, value_name = "FILE")]
    pub source_video: PathBuf,
    /// Correctly timed UTF-8 SRT, ASS, SSA or WebVTT for the source video.
    #[arg(long, value_name = "FILE")]
    pub source_subtitles: PathBuf,
    /// Destination version of the programme.
    #[arg(long, value_name = "FILE")]
    pub target_video: PathBuf,
    /// New subtitle path with the same format extension; subtitles publish only when every cue is resolved.
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
    /// UTF-8 SRT, ASS, SSA or WebVTT to summarize.
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
