//! Arguments and output for the shared timing review.
use crate::args::{Family, FixTiming, ShotEdge, TimingPriority};
use std::{
    io::{self, Write},
    sync::atomic::AtomicBool,
};
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
        writeln!(
            out,
            "Findings {} → {}. Scene coverage {:?}.",
            report.before.total, report.after.total, report.coverage
        )
        .map_err(io_error)?;
        for row in &report.changes {
            writeln!(
                out,
                "[{}] Cue {}  {}  {}{}",
                if row.selected { "x" } else { " " },
                row.cue_label,
                row.delta,
                row.metric_effect,
                if row.unit.partial { " (partial)" } else { "" }
            )
            .map_err(io_error)?;
            if row.unit.trade_off {
                for e in &row.unit.effects {
                    writeln!(out, "  Trade-off {:?}, limit {}", e.rule, e.limit)
                        .map_err(io_error)?;
                }
            }
        }
        for row in &report.manual {
            writeln!(
                out,
                "Cue {} needs manual work. {}",
                row.cue_number, row.detail.reason
            )
            .map_err(io_error)?;
            if let Some(b) = &row.detail.budget {
                writeln!(
                    out,
                    "  Budget at {}–{} ms{}{}",
                    b.at.start_ms,
                    b.at.end_ms,
                    if b.at_partial {
                        " (unticked timing)"
                    } else {
                        ""
                    },
                    if b.estimate {
                        " (overlap-aware estimate)"
                    } else {
                        ""
                    }
                )
                .map_err(io_error)?;
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
