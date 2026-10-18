//! CLI formatting only; aggregation and denominators belong to the shared engine.
use crate::args::Stats;
use std::{
    io::{self, Write},
    path::Path,
    time::{Duration, Instant},
};
use vsf_application::{
    quality::{
        self, Error, Profile,
        statistics::{self, Range, Selection},
    },
    resync::Cancellation,
};
pub fn run(args: Stats, cancel: &Cancellation) -> quality::Result<u8> {
    let began = Instant::now();
    let checkpoint = || {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        if began.elapsed() > Duration::from_secs(10) {
            return Err(Error::Limit(
                "Statistics exceeded its 10 second deadline".into(),
            ));
        }
        Ok(())
    };
    checkpoint()?;
    let profile = match Profile::builtin(&args.profile) {
        Some(p) => p,
        None => quality::read_profile(Path::new(&args.profile))?,
    };
    let quality = quality::check_file(&args.subtitles, &profile, |_, _| checkpoint())?;
    let report = statistics::summarize(
        quality,
        Selection {
            range: args
                .from_cue
                .zip(args.to_cue)
                .map(|(first, last)| Range { first, last }),
            excluded: args.exclude,
        },
        checkpoint,
    )?;
    let summary = &report.summary;
    let mut stdout = io::stdout().lock();
    let io_error = |e: io::Error| Error::Io(format!("Cannot write statistics: {e}"));
    if args.json {
        serde_json::to_writer(&mut stdout, &report)
            .map_err(|e| Error::Io(format!("Cannot write statistics JSON: {e}")))?;
        writeln!(stdout).map_err(io_error)?;
    } else {
        writeln!(
            stdout,
            "{} (revision {}) — {} included / {} total cues",
            summary.profile.name,
            summary.profile.revision,
            summary.included_cues,
            summary.total_cues
        )
        .map_err(io_error)?;
        if let Some(interpretation) = &summary.text_interpretation {
            writeln!(stdout, "Text interpretation: {interpretation}").map_err(io_error)?;
        }
        writeln!(
            stdout,
            "Scope: {} cues; {} explicitly excluded; {} outside scope; {} empty text",
            summary.scope_cues,
            summary.excluded_cues,
            summary.outside_scope_cues,
            summary.empty_text_cues
        )
        .map_err(io_error)?;
        writeln!(
            stdout,
            "Quality coverage: {}; scenes: {:?}",
            if summary.complete {
                "complete for requested rules"
            } else {
                "partial"
            },
            summary.scene_coverage
        )
        .map_err(io_error)?;
        for limitation in &summary.profile.limitations {
            writeln!(stdout, "Not checked: {limitation}").map_err(io_error)?;
        }
        for (label, value) in [
            ("Duration ms", &summary.duration_ms),
            ("CPS", &summary.cps),
            ("RS", &summary.rs),
            ("Lines per cue", &summary.line_count),
            ("Longest line UTF-16 units", &summary.longest_line),
            ("Original-neighbor signed gap ms", &summary.gaps_ms),
        ] {
            let number =
                |v: Option<f64>| v.map_or_else(|| "unavailable".into(), |v| format!("{v:.3}"));
            writeln!(
                stdout,
                "{label}: n={}, min={}, max={}, mean={}",
                value.count,
                number(value.minimum.as_ref().map(|v| v.value)),
                number(value.maximum.as_ref().map(|v| v.value)),
                number(value.mean)
            )
            .map_err(io_error)?;
        }
        for (name, count) in [
            ("CPS", &summary.unavailable_cps),
            ("RS", &summary.unavailable_rs),
        ] {
            writeln!(
                stdout,
                "Unavailable {name}: {} ({} empty text, {} invalid duration, {} reading allowance)",
                count.total, count.empty_text, count.invalid_duration, count.reading_allowance
            )
            .map_err(io_error)?;
        }
        writeln!(
            stdout,
            "RS distribution denominator: {} finite cues",
            summary.rs.count
        )
        .map_err(io_error)?;
        for bucket in &summary.ratings {
            writeln!(
                stdout,
                "{}: {} ({})",
                bucket.rating.label(),
                bucket.count,
                bucket
                    .percentage
                    .map_or_else(|| "unavailable".into(), |p| format!("{p:.2}%"))
            )
            .map_err(io_error)?;
        }
        writeln!(
            stdout,
            "Original-neighbor gaps: {} negative, {} touching, {} positive; {} pairs excluded",
            summary.negative_gaps,
            summary.touching_gaps,
            summary.positive_gaps,
            summary.excluded_neighbor_pairs
        )
        .map_err(io_error)?;
        writeln!(
            stdout,
            "All included interval overlaps: {} pairs, {} cues (touching excluded)",
            summary.overlapping_pairs, summary.overlapping_cues
        )
        .map_err(io_error)?;
        writeln!(
            stdout,
            "Quality findings: {} ({} violations, {} advisories); {} included affected cues",
            summary.findings_count, summary.violations, summary.advisories, summary.affected_cues
        )
        .map_err(io_error)?;
    }
    stdout.flush().map_err(io_error)?;
    Ok(if summary.complete { 0 } else { 3 })
}
