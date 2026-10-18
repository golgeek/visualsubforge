use std::{io, process::ExitCode};

fn main() -> ExitCode {
    match vsf_plugin_host::serve(io::stdin().lock(), io::stdout().lock()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Plugin worker: {error}");
            ExitCode::FAILURE
        }
    }
}
