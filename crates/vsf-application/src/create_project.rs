//! First project creation uses two new output names and reports each publication.
use crate::new_output;
use std::path::{Component, Path, PathBuf};

pub struct Targets {
    pub project: PathBuf,
    pub companion: Option<PathBuf>,
}

#[derive(Debug, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Outcome<T> {
    Created {
        attachment: T,
    },
    SubtitleSavedProjectFailed {
        subtitle_path: String,
        error: String,
    },
    NothingWritten {
        error: String,
    },
}

impl Targets {
    pub fn new(
        folder: &Path,
        project_name: &str,
        companion_name: Option<&str>,
    ) -> Result<Self, String> {
        if !folder.is_absolute() || !folder.is_dir() {
            return Err("Choose an existing destination folder".into());
        }
        let output = |name: &str| {
            if name.trim().is_empty()
                || name.contains(['/', '\\'])
                || !matches!(
                    Path::new(name).components().collect::<Vec<_>>().as_slice(),
                    [Component::Normal(_)]
                )
            {
                return Err("Enter a filename inside the chosen folder".to_owned());
            }
            new_output::resolve(&folder.join(name)).map_err(|e| e.to_string())
        };
        if !project_name.ends_with(".vsfproject") {
            return Err("Project filename must end in .vsfproject".into());
        }
        let project = output(project_name)?;
        let companion = companion_name.map(output).transpose()?;
        if companion
            .as_ref()
            .is_some_and(|p| new_output::equivalent_name(p, &project))
        {
            return Err("Choose different project and companion filenames".into());
        }
        Ok(Self { project, companion })
    }

    /// A saved reference returns None; a subtitle published in this operation
    /// returns its path, including when an ordinary dirty subtitle was saved.
    pub fn publish<S, T>(
        &self,
        state: &mut S,
        subtitle: impl FnOnce(&mut S) -> Result<Option<PathBuf>, String>,
        project: impl FnOnce(&mut S) -> Result<T, String>,
    ) -> Outcome<T> {
        match subtitle(state) {
            Err(error) => Outcome::NothingWritten { error },
            Ok(written) => match project(state) {
                Ok(attachment) => Outcome::Created { attachment },
                Err(error) => match written {
                    Some(path) => Outcome::SubtitleSavedProjectFailed {
                        subtitle_path: path.to_string_lossy().into_owned(),
                        error,
                    },
                    None => Outcome::NothingWritten { error },
                },
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atomic_file;
    use std::fs;
    #[test]
    fn collisions_are_refused_before_either_write_and_partial_retries_only_the_project() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path();
        for name in ["sample.fr.srt", "sample.vsfproject"] {
            let path = folder.join(name);
            fs::write(&path, b"existing").unwrap();
            assert!(
                Targets::new(folder, "sample.vsfproject", Some("sample.fr.srt"))
                    .err()
                    .unwrap()
                    .contains(name)
            );
            assert_eq!(fs::read(&path).unwrap(), b"existing");
            assert_eq!(fs::read_dir(folder).unwrap().count(), 1);
            fs::remove_file(path).unwrap();
        }
        let targets = Targets::new(folder, "sample.vsfproject", Some("sample.fr.srt")).unwrap();
        let path = targets.companion.as_ref().unwrap();
        let result = targets.publish(
            &mut (),
            |_| {
                atomic_file::prepare(path, b"speaker")?.commit_new()?;
                Ok(Some(path.clone()))
            },
            |_| {
                fs::write(&targets.project, b"competing writer").unwrap();
                atomic_file::prepare(&targets.project, b"project")?.commit_new()
            },
        );
        assert!(
            matches!(result, Outcome::SubtitleSavedProjectFailed { subtitle_path, .. } if Path::new(&subtitle_path) == path)
        );
        assert_eq!(fs::read(&targets.project).unwrap(), b"competing writer");
        let retry = Targets::new(folder, "other.vsfproject", None).unwrap();
        assert!(matches!(
            retry.publish(
                &mut (),
                |_| Ok(None),
                |_| atomic_file::prepare(&retry.project, b"project")?.commit_new()
            ),
            Outcome::Created { .. }
        ));
        assert_eq!(fs::read(path).unwrap(), b"speaker");
    }
}
