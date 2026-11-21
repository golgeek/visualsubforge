use crate::args::Review;
use std::{
    io::{self, Write},
    path::Path,
};
use vsf_application::{
    comparison,
    resync::Cancellation,
    review_file,
    review_package::{self, Package},
};

type Result<T> = std::result::Result<T, (u8, String)>;
pub(super) fn prepare_package(
    path: &Path,
    p: &Package,
    cancel: &Cancellation,
) -> Result<vsf_application::atomic_file::Prepared> {
    let bytes = p.write().map_err(|e| (2, e))?;
    vsf_application::atomic_file::prepare_with_checkpoint(path, &bytes, || {
        if cancel.is_cancelled() {
            Err("Cancelled".into())
        } else {
            Ok(())
        }
    })
    .map_err(|e| (if cancel.is_cancelled() { 130 } else { 5 }, e))
}
pub fn run(args: Review, cancel: &Cancellation) -> Result<u8> {
    let read = |p: &Path| review_file::read(p).map_err(|e| (2, e));
    let mut out = io::stdout().lock();
    match args {
        Review::Show { file, json } => {
            let p = read(&file)?;
            let (a, b) = (
                p.original.parsed().map_err(|e| (2, e))?,
                p.revised.parsed().map_err(|e| (2, e))?,
            );
            let c = comparison::compare(
                a.document.cues(),
                a.source.format(),
                b.document.cues(),
                b.source.format(),
                p.options,
                || {
                    if cancel.is_cancelled() {
                        Err("Cancelled".into())
                    } else {
                        Ok(())
                    }
                },
            )
            .map_err(|e| (if cancel.is_cancelled() { 130 } else { 2 }, e))?;
            let threads: Vec<_> = p
                .threads
                .iter()
                .map(|t| serde_json::json!({"resolved":t.resolved(),"thread":t}))
                .collect();
            if json {
                serde_json::to_writer_pretty(&mut out,&serde_json::json!({"round":p.round,"statistics":c.statistics,"threads":threads})).map_err(|e|(5,e.to_string()))?;
            } else {
                let changes = c.statistics.changes;
                let total = threads.len();
                writeln!(
                    out,
                    "Round {} · {changes} change{} · {total} thread{}",
                    p.round,
                    if changes == 1 { "" } else { "s" },
                    if total == 1 { "" } else { "s" },
                )
                .map_err(|e| (5, e.to_string()))?;
                for t in threads {
                    writeln!(
                        out,
                        "{}",
                        serde_json::to_string_pretty(&t).map_err(|e| (2, e.to_string()))?
                    )
                    .map_err(|e| (5, e.to_string()))?;
                }
            }
        }
        Review::Merge { files, output } => {
            let mut best = read(&files[0])?;
            let mut source = &files[0];
            for path in &files[1..] {
                if cancel.is_cancelled() {
                    return Err((130, "Cancelled".into()));
                }
                let incoming = read(path)?;
                if best.original.sha256 != incoming.original.sha256 {
                    return Err((2, "Original fingerprints differ".into()));
                }
                let threads =
                    review_package::merge(&best.threads, &incoming.threads).map_err(|e| (2, e))?;
                let rank = |p: &Package| {
                    (
                        p.round,
                        p.exported_at_ms,
                        p.revised.sha256.clone(),
                        p.exported_by.clone(),
                        p.original.name.clone(),
                        p.revised.name.clone(),
                        serde_json::to_string(&p.options).expect("scalar options serialize"),
                        p.original.format.clone(),
                        p.revised.format.clone(),
                    )
                };
                if rank(&incoming) > rank(&best) {
                    best = incoming;
                    source = path
                }
                best.threads = threads;
            }
            let path =
                vsf_application::new_output::resolve(&output).map_err(|e| (5, e.to_string()))?;
            prepare_package(&path, &best, cancel)?
                .commit_new()
                .map_err(|e| (5, e))?;
            writeln!(
                out,
                "Revised subtitles from {} (round {})",
                source.display(),
                best.round
            )
            .map_err(|e| (5, e.to_string()))?;
        }
    }
    Ok(0)
}
