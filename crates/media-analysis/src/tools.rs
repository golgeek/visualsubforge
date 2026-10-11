//! Shared packaged/development discovery and supervised tool identity queries.
use crate::{Cancellation, Error, Result, Tools, process};
use serde::Serialize;
use std::{
    path::PathBuf,
    process::Command,
    time::{Duration, Instant},
};

#[cfg(not(feature = "bundled-tools"))]
fn development_tool(name: &str) -> PathBuf {
    if let Some(path) = std::env::var_os(format!("VSF_{}", name.to_uppercase())) {
        return path.into();
    }
    // Preserve desktop discovery when a GUI launch does not inherit shell PATH.
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
    /// Packaged builds resolve tools only beside the real executable. They never
    /// fall back to PATH, Homebrew, or development environment overrides. Both
    /// desktop and CLI packages use this layout; call on an analysis worker.
    pub fn discover() -> Result<Self> {
        #[cfg(feature = "bundled-tools")]
        {
            bundled_tools(&std::env::current_exe()?, std::env::consts::EXE_SUFFIX)
        }
        #[cfg(not(feature = "bundled-tools"))]
        {
            Ok(Self {
                ffmpeg: development_tool("ffmpeg"),
                ffprobe: development_tool("ffprobe"),
            })
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

#[cfg(any(test, feature = "bundled-tools"))]
fn bundled_tools(executable: &std::path::Path, suffix: &str) -> Result<Tools> {
    let directory = executable.parent().ok_or_else(|| {
        Error::Message("Cannot locate the VisualSubForge installation directory".into())
    })?;
    let tool = |name: &str| -> Result<PathBuf> {
        let path = directory.join(format!("{name}{suffix}"));
        if !path.is_file() {
            return Err(Error::Message(format!(
                "VisualSubForge installation is incomplete: missing bundled tool {}. Reinstall the application.",
                path.display()
            )));
        }
        Ok(path)
    };
    Ok(Tools {
        ffmpeg: tool("ffmpeg")?,
        ffprobe: tool("ffprobe")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packaged_tools_stay_with_the_executable_after_relocation() {
        let temporary = tempfile::tempdir().unwrap();
        for suffix in ["", ".exe"] {
            let directory = temporary.path().join(format!("Moved application {suffix}"));
            std::fs::create_dir(&directory).unwrap();
            for name in ["ffmpeg", "ffprobe"] {
                std::fs::write(directory.join(format!("{name}{suffix}")), b"fixture").unwrap();
            }
            let tools = bundled_tools(&directory.join(format!("vsf{suffix}")), suffix).unwrap();
            assert_eq!(tools.ffmpeg, directory.join(format!("ffmpeg{suffix}")));
            assert_eq!(tools.ffprobe, directory.join(format!("ffprobe{suffix}")));
        }
    }

    #[test]
    fn incomplete_bundle_is_an_error_instead_of_system_fallback() {
        let temporary = tempfile::tempdir().unwrap();
        std::fs::write(temporary.path().join("ffmpeg"), b"fixture").unwrap();
        let error = bundled_tools(&temporary.path().join("vsf"), "")
            .err()
            .unwrap();
        assert!(error.to_string().contains("missing bundled tool"));
        assert!(error.to_string().contains("ffprobe"));
        // A directory with the right name is not an executable either.
        std::fs::create_dir(temporary.path().join("ffprobe")).unwrap();
        assert!(bundled_tools(&temporary.path().join("vsf"), "").is_err());
    }
}
