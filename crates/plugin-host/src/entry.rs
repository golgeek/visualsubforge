//! An embedding CLI/desktop can launch its own executable in this early mode.
//! Call before argument parsing, signal handlers, UI or application initialization.
use std::{io, process::ExitCode};
pub const WORKER_ARGUMENT: &str = "--vsf-plugin-worker";
pub fn worker_entry() -> Option<ExitCode> {
    let mut args = std::env::args_os().skip(1);
    if args.next().is_none_or(|arg| arg != WORKER_ARGUMENT) {
        return None;
    }
    if args.next().is_some() {
        eprintln!("Plugin worker does not accept additional arguments");
        return Some(ExitCode::FAILURE);
    }
    Some(
        match crate::serve(io::stdin().lock(), io::stdout().lock()) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("Plugin worker: {error}");
                ExitCode::FAILURE
            }
        },
    )
}
