use crate::args::Compare;
use std::io::{self, Write};
use vsf_application::{
    comparison::{self, Options, ReportOptions},
    resync::Cancellation,
    review_package::{Package, Subtitle, sections},
    subtitle_file,
};
pub fn run(args: Compare, cancel: &Cancellation) -> Result<u8, (u8, String)> {
    let checkpoint = || {
        if cancel.is_cancelled() {
            Err("Comparison cancelled".into())
        } else {
            Ok(())
        }
    };
    let resolve = |path: &std::path::Path| {
        vsf_application::new_output::resolve(path).map_err(|e| (5, e.to_string()))
    };
    let package_path = args.package.as_deref().map(resolve).transpose()?;
    let html_path = args.html.as_deref().map(resolve).transpose()?;
    if let (Some(a), Some(b)) = (&package_path, &html_path)
        && vsf_application::new_output::equivalent_name(a, b)
    {
        return Err((2, "Choose distinct package and HTML output names".into()));
    }
    let read = |p: &std::path::Path| {
        let bytes = subtitle_file::read(p).map_err(|e| (5, e))?;
        let parsed = subtitle_file::parse_document(&bytes, p.extension().and_then(|s| s.to_str()))
            .map_err(|e| (2, e))?;
        Ok::<_, (u8, String)>((parsed, bytes))
    };
    let (a, a_bytes) = read(&args.original)?;
    let (b, b_bytes) = read(&args.revised)?;
    let mut options = Options::for_format(a.source.format());
    if let Some(ms) = args.tolerance_ms {
        options.tolerance_ms = ms;
    }
    options.ignore_tags = args.ignore_tags;
    options.ignore_line_breaks = args.ignore_line_breaks;
    let result = comparison::compare(
        a.document.cues(),
        a.source.format(),
        b.document.cues(),
        b.source.format(),
        options,
        checkpoint,
    )
    .map_err(|e| (if cancel.is_cancelled() { 130 } else { 2 }, e))?;
    let package = (args.package.is_some() || args.embed_package).then(|| Package {
        format: "visualsubforge-review".into(),
        version: 1,
        round: 1,
        exported_by: String::new(),
        exported_at_ms: (time::OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000) as u64,
        original: Subtitle::capture(
            &args
                .original
                .file_name()
                .unwrap_or_default()
                .to_string_lossy(),
            a.source.format(),
            &a_bytes,
        ),
        revised: Subtitle::capture(
            &args
                .revised
                .file_name()
                .unwrap_or_default()
                .to_string_lossy(),
            b.source.format(),
            &b_bytes,
        ),
        options: result.options.clone(),
        threads: vec![],
    });
    let prepared_package = package_path
        .as_ref()
        .zip(package.as_ref())
        .map(|(path, package)| crate::review::prepare_package(path, package, cancel))
        .transpose()?;
    let prepared_html = if let Some(output) = html_path {
        let date = vsf_application::review_package::report_date(
            (time::OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000) as u64,
            args.utc_offset,
        )
        .map_err(|e| (2, e))?;
        let extra = if let Some(p) = package.as_ref().filter(|_| args.embed_package) {
            sections(p, &result, true, args.utc_offset).map_err(|e| (2, e))?
        } else {
            Default::default()
        };
        let html = comparison::html(
            &result,
            &ReportOptions {
                original_format: a.source.format(),
                revised_format: b.source.format(),
                original_name: &args
                    .original
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy(),
                revised_name: &args
                    .revised
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy(),
                date: &date,
                include_unchanged: args.include_unchanged,
            },
            &extra,
        );
        vsf_application::review_package::check_report_size(html.len()).map_err(|e| (2, e))?;
        Some(
            vsf_application::atomic_file::prepare_with_checkpoint(
                &output,
                html.as_bytes(),
                checkpoint,
            )
            .map_err(|e| (if cancel.is_cancelled() { 130 } else { 5 }, e))?,
        )
    } else {
        None
    };
    checkpoint().map_err(|e| (130, e))?;
    if let Some(file) = prepared_package {
        file.commit_new().map_err(|e| (5, e))?;
    }
    if let Some(file) = prepared_html {
        // Each commit is no-clobber. A concurrent writer can still win the
        // second name; unlinking the first could delete someone else's file.
        file.commit_new().map_err(|e| {
            (
                5,
                match package_path {
                    Some(path) => format!("{e}. Package published at {}", path.display()),
                    None => e,
                },
            )
        })?;
    }
    let mut stdout = io::stdout().lock();
    if args.json {
        serde_json::to_writer_pretty(&mut stdout, &result).map_err(|e| (5, e.to_string()))?;
        writeln!(stdout).map_err(|e| (5, e.to_string()))?;
    } else {
        let s = &result.statistics;
        writeln!(
            stdout,
            "{} change{} · text {:.1}% · timing {:.1}% · total {:.1}% · {} added cue{}",
            s.changes,
            if s.changes == 1 { "" } else { "s" },
            s.text_percent,
            s.timing_percent,
            s.total_percent,
            s.added_cues,
            if s.added_cues == 1 { "" } else { "s" }
        )
        .map_err(|e| (5, e.to_string()))?;
    }
    Ok(0)
}
