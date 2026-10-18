//! Argument/output adapter; the application owns capture/review/publication.
use crate::args::PluginCommand;
use std::{
    collections::BTreeMap,
    ffi::OsString,
    io::{self, Write},
    sync::atomic::AtomicBool,
    time::Duration,
};
use vsf_application::{
    plugin_files::{self, Budget, Error, Options},
    plugins::Mode,
};

pub fn error_code(error: &Error) -> u8 {
    match error {
        Error::Invalid(_) => 2,
        Error::Execution(_) => 4,
        Error::Io(_) => 5,
        Error::Changed(_) => 6,
        Error::Timeout => 7,
        Error::Cancelled => 130,
    }
}
pub fn run(command: PluginCommand, cancel: &AtomicBool) -> plugin_files::Result<u8> {
    let (args, mode, publish) = match command {
        PluginCommand::Check(a) => (a, Mode::Check, None),
        PluginCommand::Preview(a) => (a, Mode::Fix, None),
        PluginCommand::Apply(a) => (a.run, Mode::Fix, Some((a.output, a.accept))),
    };
    let budget = Budget::new(cancel, Duration::from_millis(u64::from(args.timeout_ms)));
    let mut settings = BTreeMap::new();
    for (name, value) in args.setting {
        if settings.insert(name, value).is_some() {
            return Err(Error::Invalid(
                "Plugin setting supplied more than once".into(),
            ));
        }
    }
    let worker = std::env::current_exe().map_err(|e| Error::Io(e.to_string()))?;
    let prepared = plugin_files::prepare(
        &args.subtitles,
        &args.package,
        Options {
            mode,
            scope: args.cue,
            selection: args.select,
            settings,
            context_file: args.context,
        },
        &budget,
        |execution| {
            let mut request = vsf_plugin_host::Request::new(execution.script, execution.input);
            request.limits.timeout_ms = args.execution_ms.min(
                budget
                    .remaining()?
                    .as_millis()
                    .clamp(1, u128::from(u32::MAX)) as u32,
            );
            vsf_plugin_host::run(
                &worker,
                &[OsString::from(vsf_plugin_host::WORKER_ARGUMENT)],
                &request,
                cancel,
            )
            .map_err(|e| match e {
                vsf_plugin_host::Failure::Cancelled => Error::Cancelled,
                vsf_plugin_host::Failure::Timeout => Error::Timeout,
                vsf_plugin_host::Failure::InvalidRequest(m) => Error::Invalid(m),
                vsf_plugin_host::Failure::Io(m) => Error::Io(m),
                other => Error::Execution(other.to_string()),
            })
        },
    )?;
    let output = publish
        .map(|(path, accept)| prepared.publish(&path, &accept, &budget))
        .transpose()?;
    let report = prepared.report();
    let mut stdout = io::stdout().lock();
    let io_error = |e: io::Error| Error::Io(format!("Cannot write plugin result: {e}"));
    if args.json {
        if let Some(path) = &output {
            serde_json::to_writer(&mut stdout,&serde_json::json!({"schemaVersion":1,"status":"published","output":path,"reviewId":report.review_id,"outputSha256":report.output_sha256})).map_err(|e|Error::Io(e.to_string()))?;
        } else {
            stdout.write_all(prepared.json()).map_err(io_error)?;
        }
        writeln!(stdout).map_err(io_error)?;
    } else {
        writeln!(
            stdout,
            "{} — {} findings, {} proposed cue changes",
            report.name,
            report.findings.len(),
            report.changes.len()
        )
        .map_err(io_error)?;
        writeln!(
            stdout,
            "Source: {}\nPackage: {} ({})",
            report.source.canonical_path.display(),
            report.package.id,
            report.package.fingerprint
        )
        .map_err(io_error)?;
        writeln!(
            stdout,
            "Context: {}",
            serde_json::to_string(&report.context).map_err(|e| Error::Invalid(e.to_string()))?
        )
        .map_err(io_error)?;
        writeln!(
            stdout,
            "Effective settings: {}",
            serde_json::to_string(&report.settings).map_err(|e| Error::Invalid(e.to_string()))?
        )
        .map_err(io_error)?;
        for finding in &report.findings {
            writeln!(stdout, "Cue {}: {}", finding.cue_id, finding.message).map_err(io_error)?;
        }
        for row in &report.changes {
            writeln!(
                stdout,
                "Cue {}{}\n  Before {}–{} ms: {:?}\n  After  {}–{} ms: {:?}",
                row.cue,
                if row.outside_scope {
                    " (outside requested scope)"
                } else {
                    ""
                },
                row.before.start_ms,
                row.before.end_ms,
                row.before.text,
                row.after.start_ms,
                row.after.end_ms,
                row.after.text
            )
            .map_err(io_error)?;
            if let Some(warning) = &row.warning {
                writeln!(stdout, "  Review: {warning}").map_err(io_error)?;
            }
        }
        for log in &report.logs {
            writeln!(stdout, "Plugin log: {log}").map_err(io_error)?;
        }
        writeln!(stdout, "Review ID: {}", report.review_id).map_err(io_error)?;
        if let Some(path) = &output {
            writeln!(stdout, "Published {}", path.display()).map_err(io_error)?;
        }
    }
    stdout.flush().map_err(io_error)?;
    Ok(if mode == Mode::Check && !report.findings.is_empty() {
        1
    } else {
        0
    })
}
