//! Test-only build of the CLI. Its self-spawned worker waits outside the JS VM
//! until the observing test has changed the subtitle source.
#[path = "../../src/main.rs"]
mod cli;
use cli::{args, review};

fn main() -> std::process::ExitCode {
    if std::env::args_os().nth(1).as_deref()
        == Some(std::ffi::OsStr::new(vsf_plugin_host::WORKER_ARGUMENT))
    {
        while std::path::Path::new("hold-worker").try_exists().unwrap() {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
    cli::main()
}
