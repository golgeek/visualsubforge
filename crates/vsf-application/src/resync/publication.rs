//! Report-first publication. Each file uses no-replace publication; the pair
//! cannot be made atomic. A crash/failure can leave a report without an SRT.
use super::{Analysis, Budget, Error, Result};
use crate::atomic_file::{self, Prepared};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub struct OutputPaths {
    report: PathBuf,
    srt: Option<PathBuf>,
}
impl OutputPaths {
    /// Validate before expensive analysis. Existing paths, including dangling
    /// symlinks and input aliases, are refused; the same check runs at preparation.
    pub fn new(report: &Path, srt: Option<&Path>) -> Result<Self> {
        let report = new_path(report)?;
        let srt = srt.map(new_path).transpose()?;
        if srt.as_ref() == Some(&report) {
            return Err(Error::InvalidInput(
                "Report and subtitle outputs must be different paths".into(),
            ));
        }
        Ok(Self { report, srt })
    }
    pub fn report(&self) -> &Path {
        &self.report
    }
    pub fn srt(&self) -> Option<&Path> {
        self.srt.as_deref()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Publication {
    Complete,
    AnalysisOnly,
    NeedsReview,
}

impl Analysis {
    pub fn publish(&self, paths: &OutputPaths, budget: &Budget<'_>) -> Result<Publication> {
        budget.check()?;
        // Inputs must remain distinct from outputs even if an input was removed
        // and its former pathname is now absent. Verification below also fails.
        for path in [&paths.report].into_iter().chain(paths.srt.iter()) {
            for input in [&self.source_video, &self.target_video]
                .into_iter()
                .chain(self.subtitle_file.iter())
            {
                if path == &input.canonical_path || path == &input.requested_path {
                    return Err(Error::InvalidInput("An output aliases an input".into()));
                }
            }
            require_absent(path)?;
        }
        let report = self.report_json()?;
        budget.check()?;
        let pending_report = atomic_file::prepare(&paths.report, &report).map_err(Error::Io)?;
        budget.check()?;
        let pending_srt = match (paths.srt.as_deref(), self.proposal.srt()) {
            (Some(path), Some(srt)) => {
                Some(atomic_file::prepare(path, srt.as_bytes()).map_err(Error::Io)?)
            }
            _ => None,
        };
        // Rehash after both temporary files are ready, immediately before the
        // commit point. Ordinary IO may block; cancellation is checked between
        // chunks/operations, not by abandoning a worker that owns resources.
        self.verify_inputs(budget)?;
        budget.check()?;
        commit(pending_report, pending_srt, || {})?;
        Ok(if !self.proposal.is_complete() {
            Publication::NeedsReview
        } else if paths.srt.is_some() {
            Publication::Complete
        } else {
            Publication::AnalysisOnly
        })
    }
}

fn commit(report: Prepared, srt: Option<Prepared>, after_report: impl FnOnce()) -> Result<()> {
    // Cancellation after this point does not roll back or suppress publication.
    // Complete the requested sequence; never remove an already published file.
    report.commit_new().map_err(Error::Io)?;
    after_report();
    if let Some(srt) = srt {
        srt.commit_new()
            .map_err(|e| Error::Io(format!("Report published, but SRT publication failed: {e}")))?;
    }
    Ok(())
}

fn new_path(path: &Path) -> Result<PathBuf> {
    let absolute = std::path::absolute(path).map_err(|e| Error::Io(e.to_string()))?;
    let name = absolute
        .file_name()
        .ok_or_else(|| Error::InvalidInput("Output needs a file name".into()))?;
    let parent = absolute
        .parent()
        .ok_or_else(|| Error::InvalidInput("Output needs a parent directory".into()))?
        .canonicalize()
        .map_err(|e| Error::Io(format!("Cannot resolve output directory: {e}")))?;
    if !parent.is_dir() {
        return Err(Error::InvalidInput(
            "Output parent is not a directory".into(),
        ));
    }
    let path = parent.join(name);
    require_absent(&path)?;
    Ok(path)
}

fn require_absent(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(Error::Io(format!(
            "Cannot inspect output {}: {error}",
            path.display()
        ))),
        Ok(_) => Err(Error::InvalidInput(format!(
            "Output already exists: {}",
            path.display()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "Subprocess entry point for publication crash regression"]
    fn publication_crash_child() {
        let directory = PathBuf::from(std::env::var_os("VSF_PUBLICATION_CRASH_DIR").unwrap());
        let report = atomic_file::prepare(&directory.join("report.json"), b"report").unwrap();
        let srt = atomic_file::prepare(&directory.join("target.srt"), b"subtitles").unwrap();
        let _ = commit(report, Some(srt), || std::process::exit(23));
        panic!("Child did not exit at the publication boundary");
    }

    #[test]
    fn crash_after_report_preserves_inputs_and_retry_refuses_the_existing_report() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.srt");
        fs::write(&source, b"source bytes").unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "resync::publication::tests::publication_crash_child",
                "--ignored",
            ])
            .env("VSF_PUBLICATION_CRASH_DIR", dir.path())
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let started = std::time::Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if started.elapsed() > std::time::Duration::from_secs(10) {
                let _ = child.kill();
                let _ = child.wait();
                panic!("Crash helper timed out");
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        assert_eq!(status.code(), Some(23));
        assert_eq!(fs::read(&source).unwrap(), b"source bytes");
        assert_eq!(fs::read(dir.path().join("report.json")).unwrap(), b"report");
        assert!(!dir.path().join("target.srt").exists());
        assert!(
            OutputPaths::new(
                &dir.path().join("report.json"),
                Some(&dir.path().join("target.srt"))
            )
            .is_err()
        );
        // Abrupt exit can leave the owned SRT temporary. It is not a published
        // subtitle and is never found/imported as input by a later invocation.
    }

    #[test]
    fn competing_destination_cannot_be_overwritten_after_preparation() {
        let dir = tempfile::tempdir().unwrap();
        let report = dir.path().join("report.json");
        let srt = dir.path().join("target.srt");
        let a = atomic_file::prepare(&report, b"report").unwrap();
        let b = atomic_file::prepare(&srt, b"subtitles").unwrap();
        let result = commit(a, Some(b), || fs::write(&srt, b"competitor").unwrap());
        assert!(matches!(result, Err(Error::Io(e)) if e.contains("Report published")));
        assert_eq!(fs::read(&report).unwrap(), b"report");
        assert_eq!(fs::read(&srt).unwrap(), b"competitor");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
        assert!(OutputPaths::new(&report, Some(&srt)).is_err());
    }

    #[test]
    fn report_failure_leaves_subtitles_absent_and_cleans_owned_temporaries() {
        let dir = tempfile::tempdir().unwrap();
        let report = dir.path().join("report.json");
        let srt = dir.path().join("target.srt");
        let a = atomic_file::prepare(&report, b"report").unwrap();
        let b = atomic_file::prepare(&srt, b"subtitles").unwrap();
        fs::write(&report, b"existing report").unwrap();
        assert!(commit(a, Some(b), || panic!("Report should have failed")).is_err());
        assert_eq!(fs::read(&report).unwrap(), b"existing report");
        assert!(!srt.exists());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn identical_existing_and_directory_paths_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let new = dir.path().join("new.srt");
        assert!(OutputPaths::new(&new, Some(&new)).is_err());
        assert!(OutputPaths::new(dir.path(), None).is_err());
        fs::write(&new, b"existing").unwrap();
        let hardlink = dir.path().join("alias");
        fs::hard_link(&new, &hardlink).unwrap();
        assert!(OutputPaths::new(&hardlink, None).is_err());
        assert_eq!(fs::read(new).unwrap(), b"existing");
    }

    #[cfg(unix)]
    #[test]
    fn symlink_aliases_and_dangling_destinations_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let alias = dir.path().join("alias");
        std::os::unix::fs::symlink(dir.path(), &alias).unwrap();
        assert!(OutputPaths::new(&dir.path().join("new"), Some(&alias.join("new"))).is_err());
        let dangling = dir.path().join("dangling");
        std::os::unix::fs::symlink(dir.path().join("absent"), &dangling).unwrap();
        assert!(OutputPaths::new(&dangling, None).is_err());
    }
}
