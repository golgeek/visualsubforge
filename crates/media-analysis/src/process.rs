//! Owns each direct child until it is reaped. No shell or detached process is used.
use crate::{Cancellation, Error, Result};
use std::{
    io::Read,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

struct Reap(Child);
impl Drop for Reap {
    fn drop(&mut self) {
        // Also covers cancellation, reader failures, and unwinding.
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub(crate) fn run<T: Send>(
    command: Command,
    cancellation: &Cancellation,
    timeout: Duration,
    read: impl FnOnce(Box<dyn Read + Send>) -> Result<T> + Send,
) -> Result<T> {
    run_pipe(command, cancellation, timeout, false, read)
}

// trace_headers writes packet metadata to stderr; drain the unused pipe too.
pub(crate) fn run_stderr<T: Send>(
    command: Command,
    cancellation: &Cancellation,
    timeout: Duration,
    read: impl FnOnce(Box<dyn Read + Send>) -> Result<T> + Send,
) -> Result<T> {
    run_pipe(command, cancellation, timeout, true, read)
}

fn run_pipe<T: Send>(
    mut command: Command,
    cancellation: &Cancellation,
    timeout: Duration,
    use_stderr: bool,
    read: impl FnOnce(Box<dyn Read + Send>) -> Result<T> + Send,
) -> Result<T> {
    if cancellation.is_cancelled() {
        return Err(Error::Cancelled);
    }
    let label = command.get_program().to_string_lossy().into_owned();
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    thread::scope(|scope| {
        // Declared inside the scope so early returns kill the child BEFORE scoped
        // reader threads are joined. Otherwise a blocked read could deadlock cleanup.
        let mut child = Reap(
            command
                .spawn()
                .map_err(|e| Error::Message(format!("{label}: {e}")))?,
        );
        let stdout = child
            .0
            .stdout
            .take()
            .ok_or_else(|| Error::Message("Missing stdout pipe".into()))?;
        let stderr = child
            .0
            .stderr
            .take()
            .ok_or_else(|| Error::Message("Missing stderr pipe".into()))?;
        let (selected, other): (Box<dyn Read + Send>, Box<dyn Read + Send>) = if use_stderr {
            (Box::new(stderr), Box::new(stdout))
        } else {
            (Box::new(stdout), Box::new(stderr))
        };
        let output = scope.spawn(move || read(selected));
        let diagnostic = scope.spawn(move || {
            // Keep a bounded diagnostic, but drain everything to avoid a full pipe.
            let mut reader = other;
            let mut prefix = Vec::new();
            let _ = (&mut reader).take(16_384).read_to_end(&mut prefix);
            let _ = std::io::copy(&mut reader, &mut std::io::sink());
            prefix
        });
        let start = Instant::now();
        let status = loop {
            if cancellation.is_cancelled() {
                return Err(Error::Cancelled);
            }
            if start.elapsed() >= timeout {
                return Err(Error::Timeout);
            }
            if let Some(status) = child.0.try_wait()? {
                break status;
            }
            thread::sleep(Duration::from_millis(10));
        };
        let output = output
            .join()
            .map_err(|_| Error::Message("Output reader panicked".into()))?;
        let diagnostic = diagnostic
            .join()
            .map_err(|_| Error::Message("Diagnostic reader panicked".into()))?;
        if cancellation.is_cancelled() {
            return Err(Error::Cancelled);
        }
        // Preserve parse/limit errors rather than replacing them with a broken-pipe diagnostic.
        let output = output?;
        if !status.success() {
            return Err(Error::Message(format!(
                "{label} exited {status}: {}",
                String::from_utf8_lossy(&diagnostic)
            )));
        }
        Ok(output)
    })
}

pub(crate) fn limited(mut reader: impl Read, limit: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    (&mut reader).take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        // Returning closes the pipe; the supervisor will reap or time out the child.
        return Err(Error::Message("Probe output exceeds limit".into()));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Spawn the test binary itself so lifecycle tests need no FFmpeg or shell.
    #[test]
    fn child() {
        if let Ok(mode) = std::env::var("VSF_PROCESS_TEST_CHILD") {
            match mode.as_str() {
                "sleep" => thread::sleep(Duration::from_secs(60)),
                "diagnostic" => {
                    use std::io::Write;
                    std::io::stderr().write_all(&vec![b'x'; 100_000]).unwrap();
                    std::process::exit(7);
                }
                _ => panic!("Unknown test child mode"),
            }
        }
    }

    fn child_command(mode: &str) -> Command {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command.args(["--exact", "process::tests::child", "--nocapture"]);
        command.env("VSF_PROCESS_TEST_CHILD", mode);
        command
    }

    #[test]
    fn timeout_reaps_a_child_with_blocked_readers() {
        let start = Instant::now();
        let result = run(
            child_command("sleep"),
            &Cancellation::default(),
            Duration::from_millis(100),
            |r| limited(r, 1024),
        );
        assert!(matches!(result, Err(Error::Timeout)));
        assert!(start.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn cancellation_reaps_a_child_with_blocked_readers() {
        let token = Cancellation::default();
        thread::scope(|scope| {
            scope.spawn(|| {
                thread::sleep(Duration::from_millis(100));
                token.cancel();
            });
            let start = Instant::now();
            let result = run(
                child_command("sleep"),
                &token,
                Duration::from_secs(10),
                |r| limited(r, 1024),
            );
            assert!(matches!(result, Err(Error::Cancelled)));
            assert!(start.elapsed() < Duration::from_secs(5));
        });
    }

    #[test]
    fn large_diagnostics_do_not_deadlock_or_grow_without_bound() {
        let error = run(
            child_command("diagnostic"),
            &Cancellation::default(),
            Duration::from_secs(5),
            |r| limited(r, 1024),
        )
        .unwrap_err();
        assert!(error.to_string().contains("exited"));
        assert!(error.to_string().len() < 17_000);
    }
}
