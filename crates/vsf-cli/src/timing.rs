//! Arguments and output for the shared timing review.
use crate::args::{Family, FixTiming, ShotEdge, TimingPriority};
use std::{
    io::{self, Write},
    sync::atomic::AtomicBool,
};
use vsf_application::quality::scene::SceneCoverage;
use vsf_application::{
    quality::timing_fix::{Families, Options, Priority, SceneEdges},
    timing_files::{self, Error, Result},
};

pub fn run(args: FixTiming, cancel: &AtomicBool) -> Result<u8> {
    let mut families = Families {
        gaps: false,
        speed: false,
        shots: false,
    };
    for family in args.fix {
        let flag = match family {
            Family::Gaps => &mut families.gaps,
            Family::Speed => &mut families.speed,
            Family::Shots => &mut families.shots,
        };
        if *flag {
            return Err(Error::Invalid("Repeated timing family".into()));
        }
        *flag = true;
    }
    let options = Options {
        priority: match args.priority {
            TimingPriority::ReadingSpeed => Priority::ReadingSpeed,
            TimingPriority::Spacing => Priority::Spacing,
        },
        start_tolerance_ms: args.start_tolerance_ms,
        scene_edges: match args.scene_edges {
            ShotEdge::Inward => SceneEdges::Inward,
            ShotEdge::Nearest => SceneEdges::Nearest,
        },
    };
    let review = timing_files::preview(
        timing_files::Request {
            subtitles: &args.subtitles,
            profile: &args.profile,
            scenes: args.scenes.as_deref(),
            without_scenes: args.without_scenes,
            families,
            options,
            cues: args.cue,
        },
        cancel,
    )?;
    let mut out = io::stdout().lock();
    let io_error = |e: io::Error| Error::Io(e.to_string());
    let written = match (args.output, args.accept) {
        (Some(path), Some(id)) => Some(review.publish(&path, &id, cancel)?),
        _ => None,
    };
    let report = review.report();
    if args.json {
        serde_json::to_writer(&mut out, report).map_err(|e| Error::Io(e.to_string()))?;
        writeln!(out).map_err(io_error)?;
    } else if let Some(ref path) = written {
        writeln!(out, "Wrote {}", path.display()).map_err(io_error)?;
    } else {
        if report.withheld_all {
            writeln!(
                out,
                "{}",
                vsf_application::quality::timing_fix::WITHHELD_ALL_MESSAGE
            )
            .map_err(io_error)?;
        }
        let coverage = match report.coverage {
            SceneCoverage::Checked => "complete",
            SceneCoverage::Reduced => "reduced (can miss cuts)",
            SceneCoverage::NoMedia => "no video",
            SceneCoverage::NotRequired => "not required (buffers are zero)",
            SceneCoverage::Partial => "incomplete",
            SceneCoverage::Unavailable => "unavailable",
        };
        writeln!(
            out,
            "Findings {} → {}.\nScene coverage: {coverage}",
            report.before.total, report.after.total
        )
        .map_err(io_error)?;
        for row in &report.changes {
            let flag = if row.selected { "x" } else { " " };
            write!(out, "[{flag}] Cue {}  {}", row.cue_label, row.delta).map_err(io_error)?;
            if row.unit.partial {
                write!(out, " · Partial").map_err(io_error)?;
            }
            if row.unit.trade_off {
                write!(out, " · Trade-off").map_err(io_error)?;
            }
            for label in row.needs.iter().chain(&row.replaces) {
                write!(out, " · {label}").map_err(io_error)?;
            }
            if !row.unit.partial {
                for other in &row.unit.conflicts_with {
                    write!(out, " · conflicts with cue {} partial", other + 1).map_err(io_error)?;
                }
            }
            writeln!(out).map_err(io_error)?;
            if !row.unit.effects_text.is_empty() {
                let segments: Vec<_> = row
                    .unit
                    .effects_text
                    .iter()
                    .map(|segment| segment.text.as_str())
                    .collect();
                writeln!(out, "  {}", segments.join(" · ")).map_err(io_error)?;
            }
        }
        for row in &report.manual {
            writeln!(
                out,
                "Cue {} needs manual work. {}",
                row.cue_number, row.detail.lead
            )
            .map_err(io_error)?;
            for line in row.detail.facts.iter().chain(&row.detail.budget_text) {
                writeln!(out, "  {line}").map_err(io_error)?;
            }
        }
        writeln!(out, "Review ID {}", report.review_id).map_err(io_error)?;
    }
    out.flush().map_err(io_error)?;
    Ok(if written.is_some() {
        0
    } else {
        review.status()
    })
}
