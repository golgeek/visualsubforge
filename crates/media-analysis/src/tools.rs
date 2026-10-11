//! Shared development discovery and supervised tool identity queries.
use crate::{Cancellation, Error, Result, Tools, process};
use serde::Serialize;
use std::{
    path::PathBuf,
    process::Command,
    time::{Duration, Instant},
};

pub fn tool(name: &str) -> PathBuf {
    if let Some(path) = std::env::var_os(format!("VSF_{}", name.to_uppercase())) {
        return path.into();
    }
    // Preserve desktop discovery when a GUI launch does not inherit shell PATH.
    // Packaging still needs an explicit tested/bundled tool distribution.
    #[cfg(target_os = "macos")]
    for directory in ["/opt/homebrew/bin", "/usr/local/bin"] {
        let path = std::path::Path::new(directory).join(name);
        if path.is_file() {
            return path;
        }
    }
    name.into()
}

#[derive(Debug, Serialize)]
pub struct Versions {
    pub ffmpeg: String,
    pub ffprobe: String,
}

impl Tools {
    pub fn discover() -> Self {
        Self {
            ffmpeg: tool("ffmpeg"),
            ffprobe: tool("ffprobe"),
        }
    }

    /// Records actual version output, through the same cancellation/reap owner
    /// as decoding. This is diagnostic identity, not a cryptographic tool pin.
    pub fn versions(&self, cancel: &Cancellation, timeout: Duration) -> Result<Versions> {
        let started = Instant::now();
        let version = |path: &PathBuf| -> Result<String> {
            let mut command = Command::new(path);
            command.arg("-version");
            let bytes = process::run(
                command,
                cancel,
                timeout
                    .checked_sub(started.elapsed())
                    .ok_or(Error::Timeout)?,
                |reader| process::limited(reader, 65_536),
            )?;
            let text = String::from_utf8(bytes)
                .map_err(|_| Error::Message("Invalid tool version output".into()))?;
            if text.trim().is_empty() {
                return Err(Error::Message("Empty tool version output".into()));
            }
            Ok(text)
        };
        Ok(Versions {
            ffmpeg: version(&self.ffmpeg)?,
            ffprobe: version(&self.ffprobe)?,
        })
    }
}
