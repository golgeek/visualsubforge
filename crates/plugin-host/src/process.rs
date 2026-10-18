use crate::{
    Failure, MAX_MESSAGE_BYTES, Request, VERSION,
    protocol::{MAX_DIAGNOSTIC_BYTES, Reply, encode_bounded, read_bounded},
};
use serde_json::Value;
use std::{
    ffi::OsString,
    io::Write,
    path::Path,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

// Drop runs inside the thread scope, before pipe threads are joined. Killing and
// reaping the child closes its pipe ends, including when request writing blocks.
struct ReapedChild(Child);
impl Drop for ReapedChild {
    fn drop(&mut self) {
        if !matches!(self.0.try_wait(), Ok(Some(_))) {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

enum Pipe {
    Input(Result<(), Failure>),
    Output(Result<Vec<u8>, Failure>),
    Error(Result<Vec<u8>, Failure>),
}

/// Run one helper synchronously on an application worker, never the UI thread.
/// The caller supplies a trusted executable and arguments, never plugin code as
/// a command. The helper is fresh per invocation and cannot retain script globals.
pub fn run(
    executable: &Path,
    args: &[OsString],
    request: &Request,
    cancel: &AtomicBool,
) -> Result<Value, Failure> {
    request.validate()?;
    if cancel.load(Ordering::Acquire) {
        return Err(Failure::Cancelled);
    }
    let started = Instant::now();
    let deadline = started + Duration::from_millis(u64::from(request.limits.timeout_ms));
    let bytes = encode_bounded(request).map_err(|e| Failure::InvalidRequest(e.to_string()))?;
    if cancel.load(Ordering::Acquire) {
        return Err(Failure::Cancelled);
    }
    if Instant::now() >= deadline {
        return Err(Failure::Timeout);
    }
    let mut command = Command::new(executable);
    command
        .args(args)
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
        if let Some(root) = std::env::var_os("SystemRoot") {
            command.env("SystemRoot", root);
        }
    }
    thread::scope(|scope| {
        let mut child = ReapedChild(command.spawn()?);
        let missing = || Failure::Worker("Plugin worker pipe was not created".into());
        let mut input = child.0.stdin.take().ok_or_else(missing)?;
        let output = child.0.stdout.take().ok_or_else(missing)?;
        let error = child.0.stderr.take().ok_or_else(missing)?;
        let (sender, receiver) = mpsc::channel();
        let tx = sender.clone();
        thread::Builder::new()
            .name("plugin-input".into())
            .spawn_scoped(scope, move || {
                let result = input.write_all(&bytes).map_err(Failure::from);
                drop(input); // EOF completes the single request.
                let _ = tx.send(Pipe::Input(result));
            })?;
        let tx = sender.clone();
        thread::Builder::new()
            .name("plugin-output".into())
            .spawn_scoped(scope, move || {
                let _ = tx.send(Pipe::Output(read_bounded(output, MAX_MESSAGE_BYTES)));
            })?;
        thread::Builder::new()
            .name("plugin-diagnostic".into())
            .spawn_scoped(scope, move || {
                let _ = sender.send(Pipe::Error(read_bounded(error, MAX_DIAGNOSTIC_BYTES)));
            })?;
        let mut written = false;
        let mut stdout = None;
        let mut stderr = None;
        let result = loop {
            if cancel.load(Ordering::Acquire) {
                break Err(Failure::Cancelled);
            }
            if Instant::now() >= deadline {
                break Err(Failure::Timeout);
            }
            let message = receiver.recv_timeout(Duration::from_millis(5));
            match message {
                Ok(Pipe::Input(Ok(()))) => written = true,
                Ok(Pipe::Output(Ok(bytes))) => stdout = Some(bytes),
                Ok(Pipe::Error(Ok(bytes))) => stderr = Some(bytes),
                Ok(Pipe::Input(Err(e)) | Pipe::Output(Err(e)) | Pipe::Error(Err(e))) => {
                    break Err(e);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    thread::sleep(Duration::from_millis(5))
                }
            }
            match child.0.try_wait() {
                Err(e) => break Err(e.into()),
                Ok(Some(status)) => {
                    if !status.success() {
                        break Err(Failure::Worker(format!(
                            "Plugin worker exited unsuccessfully: {status}"
                        )));
                    }
                    if written && stdout.is_some() && stderr.is_some() {
                        let bytes = stdout.take().unwrap_or_default();
                        let reply: Reply = match serde_json::from_slice(&bytes) {
                            Ok(reply) => reply,
                            Err(e) => {
                                break Err(Failure::Protocol(format!("Invalid plugin reply: {e}")));
                            }
                        };
                        if reply.version != VERSION {
                            break Err(Failure::Protocol(
                                "Unsupported plugin reply version".into(),
                            ));
                        }
                        break reply.result;
                    }
                }
                Ok(None) => {}
            }
        };
        // No successful result escapes after cancellation or a deadline that
        // elapsed while decoding the bounded reply.
        drop(child);
        if cancel.load(Ordering::Acquire) {
            return Err(Failure::Cancelled);
        }
        if Instant::now() >= deadline {
            return Err(Failure::Timeout);
        }
        result
    })
}
