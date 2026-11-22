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
    /// List or extract embedded text subtitles to a new file.
    Extract(Extract),
    /// Compare two subtitle versions without modifying either input.
    Compare(Compare),
    /// Inspect or merge offline review files.
    #[command(subcommand)]
    Review(Review),
    /// Run local JavaScript subtitle checks and reviewable fixes.
    Plugin(Plugin),
    /// Prepare or accept reviewed subtitle fixes.
    #[command(subcommand)]
    Fix(Fix),
    /// Check subtitle authoring rules without changing the file.
    Check(Check),
    /// Summarize accepted subtitle metrics without changing the file.
    Stats(Stats),
    /// Transfer SRT/ASS/SSA/WebVTT timings between versions of the same programme.
    Resync(Resync),
}

#[derive(Subcommand)]
pub enum Fix {
    /// Review timing changes or write a previously reviewed selection to a new file.
    Timing(FixTiming),
}
#[derive(Clone, Copy, clap::ValueEnum)]
pub enum Family {
    Gaps,
    Speed,
    Shots,
}
#[derive(Clone, Copy, clap::ValueEnum)]
pub enum TimingPriority {
    ReadingSpeed,
    Spacing,
}
#[derive(Clone, Copy, clap::ValueEnum)]
pub enum ShotEdge {
    Inward,
    Nearest,
}

#[derive(Args)]
#[command(
    after_help = "Preview writes nothing. Nonzero shot buffers require a scene file or --without-scenes. Exit codes: 0 no remaining requested findings or output written, 1 remaining requested findings (with --without-scenes, only the other requested families count), 2 invalid input, 3 incomplete scene coverage, 5 IO, 6 changed input or review ID, 130 cancelled."
)]
pub struct FixTiming {
    #[arg(long, value_name = "FILE")]
    pub subtitles: PathBuf,
    /// Built-in profile ID or version-2 preset file.
    #[arg(long, value_name = "ID_OR_FILE")]
    pub profile: String,
    /// Families to fix. Other families stay protected.
    #[arg(
        long,
        value_enum,
        value_delimiter = ',',
        default_value = "gaps,speed,shots"
    )]
    pub fix: Vec<Family>,
    /// Version-1 JSON containing mapped integer cut timestamps in milliseconds.
    #[arg(long, conflicts_with = "without_scenes")]
    pub scenes: Option<PathBuf>,
    /// Skip shot changes and report findings in the other requested families.
    #[arg(long)]
    pub without_scenes: bool,
    #[arg(long, value_enum, default_value = "reading-speed")]
    pub priority: TimingPriority,
    #[arg(long,default_value_t=0,value_parser=clap::value_parser!(u32).range(0..=1000))]
    pub start_tolerance_ms: u32,
    #[arg(long, value_enum, default_value = "inward")]
    pub scene_edges: ShotEdge,
    /// One-based file positions. Omit for the whole document.
    #[arg(long, value_delimiter = ',')]
    pub cue: Vec<u32>,
    /// Write schema-1 JSON to stdout.
    #[arg(long)]
    pub json: bool,
    /// Recompute and match the previous review ID before writing.
    #[arg(long,requires="output",value_parser=parse_review_id)]
    pub accept: Option<String>,
    /// A new path with the source subtitle extension.
    #[arg(long, requires = "accept")]
    pub output: Option<PathBuf>,
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
        return Err("Use the 64-character lowercase review ID from preview".into());
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

#[derive(Args)]
pub struct Extract {
    pub video: PathBuf,
    #[arg(long, conflicts_with_all = ["track", "output"])]
    pub list: bool,
    #[arg(long, required_unless_present = "list")]
    pub track: Option<u32>,
    #[arg(short, long, required_unless_present = "list")]
    pub output: Option<PathBuf>,
    #[arg(long)]
    pub json: bool,
    #[arg(long, default_value_t = 1800, value_parser = clap::value_parser!(u64).range(1..=14400))]
    pub timeout_seconds: u64,
}

#[derive(Args)]
pub struct Compare {
    /// UTC offset in minutes for report dates (default UTC).
    #[arg(long,default_value_t=0,allow_hyphen_values=true,value_parser=clap::value_parser!(i16).range(-1439..=1439))]
    pub utc_offset: i16,
    /// Write a self-contained review package to a new file.
    #[arg(long)]
    pub package: Option<PathBuf>,
    pub original: PathBuf,
    pub revised: PathBuf,
    #[arg(long, visible_alias = "report")]
    pub html: Option<PathBuf>,
    /// Include both complete subtitle files so the HTML can be reopened in VSF.
    #[arg(long, requires = "html")]
    pub embed_package: bool,
    #[arg(long)]
    pub json: bool,
    #[arg(long)]
    pub tolerance_ms: Option<u32>,
    #[arg(long)]
    pub ignore_tags: bool,
    #[arg(long)]
    pub ignore_line_breaks: bool,
    #[arg(long)]
    pub include_unchanged: bool,
}

#[derive(Subcommand)]
pub enum Review {
    /// Show comparison statistics, discussion and anchor snapshots.
    Show {
        file: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Union discussions, retaining the revised subtitle from the latest round.
    Merge {
        #[arg(num_args=2.., required=true)]
        files: Vec<PathBuf>,
        #[arg(short, long)]
        output: PathBuf,
    },
}
