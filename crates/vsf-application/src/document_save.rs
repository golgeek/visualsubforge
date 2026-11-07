//! Save a loaded editor without replacing its history owner.
//!
//! This synchronous operation borrows all persistence state exclusively. Desktop
//! callers keep their document transaction locked and dispatch off the UI thread.
//! The destination lease spans baseline comparison, atomic write and publication.
use std::path::{Path, PathBuf};
use subtitle_core::Editor;
use subtitle_formats::Source;

/// Borrowed live state, not a second document/session owner. Protection flags are
/// independent of undo history; reference/copied-source paths remain caller-owned.
pub struct Session<'a> {
    pub editor: &'a mut Editor<Source>,
    /// Canonical loaded path, or no path for an untitled document.
    pub path: &'a mut Option<PathBuf>,
    pub saved_bytes: &'a mut Vec<u8>,
    pub recovered: &'a mut bool,
    pub resynced: &'a mut bool,
}

impl Session<'_> {
    /// `destination = None` is ordinary Save; an explicit path is Save As.
    /// Paths in `protected` include selected and canonical reference/provenance
    /// paths. They remain protected after successful saves and reference detach.
    pub fn save<'a>(
        self,
        revision: u64,
        destination: Option<&Path>,
        protected: impl IntoIterator<Item = &'a Path>,
    ) -> Result<(), String> {
        if revision != self.editor.revision() {
            return Err("Subtitle document changed; refresh before retrying this operation".into());
        }
        if *self.resynced && destination.is_none() {
            return Err("Resynchronized subtitles require Save As to a different file".into());
        }
        if *self.recovered && destination.is_none() {
            return Err("Recovered subtitles require Save As to choose a destination".into());
        }
        let target = match destination {
            Some(path) => resolve_path(path)?,
            None => self
                .path
                .clone()
                .ok_or("Choose a filename with Save As for untitled subtitles")?,
        };
        if *self.resynced && self.path.as_ref() == Some(&target) {
            return Err("Choose a different file to preserve the original subtitles".into());
        }
        for source in protected {
            crate::new_output::protect_source(&target, source).map_err(|e| e.to_string())?;
        }
        let source = self.editor.properties();
        source.validate_save_extension(target.extension().and_then(|s| s.to_str()))?;
        let bytes = source.serialize(self.editor.document())?.into_bytes();
        crate::srt_limits::validate_size(bytes.len())?;
        let _lease = crate::save_lease::acquire(&target)?;
        if self.path.as_ref() == Some(&target)
            && crate::subtitle_file::read(&target)? != *self.saved_bytes
        {
            return Err(
                "Subtitle file changed on disk. Use Save As to preserve both versions".into(),
            );
        }
        crate::atomic_file::write(&target, &bytes)?;
        // The exclusive borrow keeps the checked revision current through IO.
        // Only successful publication changes the saved identity or persistence fields.
        self.editor.mark_saved(revision)?;
        *self.path = Some(target);
        *self.saved_bytes = bytes;
        *self.recovered = false;
        *self.resynced = false;
        Ok(())
    }
}

/// Resolve the parent and any existing symlink before destination comparisons.
pub fn resolve_path(path: &Path) -> Result<PathBuf, String> {
    let name = path.file_name().ok_or("Choose a subtitle filename")?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let target = parent
        .canonicalize()
        .map_err(|e| format!("Cannot access save directory: {e}"))?
        .join(name);
    // Resolve an existing symlink, preserving its meaning when saving through it.
    if target.exists() {
        target.canonicalize().map_err(|e| e.to_string())
    } else {
        Ok(target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use subtitle_core::{CueId, Edit, Milliseconds};

    const ORIGINAL: &str = "1\n00:00:01,000 --> 00:00:02,000\nOriginal\n\n";

    struct State {
        editor: Editor<Source>,
        path: Option<PathBuf>,
        bytes: Vec<u8>,
        recovered: bool,
        resynced: bool,
    }
    impl State {
        fn new(path: Option<PathBuf>) -> Self {
            let parsed =
                crate::subtitle_file::parse_document(ORIGINAL.as_bytes(), Some("srt")).unwrap();
            let mut editor = Editor::with_properties(parsed.document, parsed.source);
            editor
                .apply_edit(
                    0,
                    Edit::Text {
                        id: CueId(1),
                        start: Milliseconds::new(1000).unwrap(),
                        end: Milliseconds::new(2000).unwrap(),
                        text: "Edited".into(),
                    },
                    |_| Ok(()),
                )
                .unwrap();
            Self {
                editor,
                path,
                bytes: ORIGINAL.as_bytes().to_vec(),
                recovered: false,
                resynced: false,
            }
        }
        fn save(
            &mut self,
            revision: u64,
            target: Option<&Path>,
            protected: &[&Path],
        ) -> Result<(), String> {
            Session {
                editor: &mut self.editor,
                path: &mut self.path,
                saved_bytes: &mut self.bytes,
                recovered: &mut self.recovered,
                resynced: &mut self.resynced,
            }
            .save(revision, target, protected.iter().copied())
        }
        fn persistence(&self) -> (Option<PathBuf>, Vec<u8>, bool, bool) {
            (
                self.path.clone(),
                self.bytes.clone(),
                self.recovered,
                self.resynced,
            )
        }
    }

    #[test]
    fn ordinary_save_and_save_as_keep_history_and_saved_identity() {
        let directory = tempfile::tempdir().unwrap();
        let dir = directory.path().canonicalize().unwrap();
        let source = dir.join("source.srt");
        fs::write(&source, ORIGINAL).unwrap();
        let mut state = State::new(Some(source.clone()));
        state.save(1, None, &[]).unwrap();
        let saved = state.bytes.clone();
        assert_eq!(fs::read(&source).unwrap(), saved);
        assert!(!state.editor.dirty());
        state.editor.undo(1).unwrap();
        assert!(state.editor.dirty());
        state.editor.redo(2).unwrap();
        assert!(!state.editor.dirty());
        let copy = dir.join("copy.srt");
        state.save(3, Some(&copy), &[]).unwrap();
        assert_eq!(state.path.as_ref(), Some(&copy));
        assert_eq!(fs::read(copy).unwrap(), saved);
        assert!(state.editor.can_undo());
    }

    #[test]
    fn protected_and_untitled_states_require_explicit_destination() {
        let directory = tempfile::tempdir().unwrap();
        let dir = directory.path().canonicalize().unwrap();
        let source = dir.join("source.srt");
        fs::write(&source, ORIGINAL).unwrap();
        for (path, recovered, resynced) in [
            (None, false, false),
            (Some(source.clone()), true, false),
            (Some(source.clone()), false, true),
            (Some(source.clone()), true, true),
        ] {
            let mut state = State::new(path);
            state.recovered = recovered;
            state.resynced = resynced;
            let before = state.persistence();
            assert!(state.save(1, None, &[]).is_err());
            if resynced {
                assert!(state.save(1, Some(&source), &[]).is_err());
            }
            assert_eq!(state.persistence(), before);
            let output = dir.join("output.srt");
            state.save(1, Some(&output), &[]).unwrap();
            assert!(!state.recovered && !state.resynced && !state.editor.dirty());
            assert_eq!(state.path.as_ref(), Some(&output));
            assert_eq!(fs::read(output).unwrap(), state.bytes);
        }
        // Recovery permits explicitly choosing an unchanged original.
        let mut state = State::new(Some(source.clone()));
        state.recovered = true;
        state.save(1, Some(&source), &[]).unwrap();
    }

    #[test]
    fn reference_and_deleted_copy_provenance_remain_protected_after_save() {
        let directory = tempfile::tempdir().unwrap();
        let dir = directory.path().canonicalize().unwrap();
        let reference = dir.join("reference.srt");
        let copied = dir.join("copied.srt");
        fs::write(&reference, ORIGINAL).unwrap();
        let mut state = State::new(None);
        for path in [&reference, &copied] {
            assert!(state.save(1, Some(path), &[&reference, &copied]).is_err());
        }
        let output = dir.join("output.srt");
        state
            .save(1, Some(&output), &[&reference, &copied])
            .unwrap();
        for path in [&reference, &copied] {
            assert!(state.save(1, Some(path), &[&reference, &copied]).is_err());
        }
        assert_eq!(fs::read_to_string(reference).unwrap(), ORIGINAL);
        assert!(!copied.exists());
        assert_eq!(state.path, Some(output));
    }

    #[test]
    fn stale_conflicting_and_failed_saves_preserve_persistence_and_redo() {
        let directory = tempfile::tempdir().unwrap();
        let dir = directory.path().canonicalize().unwrap();
        let source = dir.join("source.srt");
        fs::write(&source, ORIGINAL).unwrap();
        let mut state = State::new(Some(source.clone()));
        state.recovered = true;
        state.editor.undo(1).unwrap();
        let before = state.persistence();
        let output = dir.join("output.srt");
        assert!(state.save(1, Some(&output), &[]).is_err());
        assert!(!output.exists());
        assert!(
            state
                .save(2, Some(&dir.join("missing/output.srt")), &[])
                .is_err()
        );
        let directory = dir.join("directory.srt");
        fs::create_dir(&directory).unwrap();
        assert!(state.save(2, Some(&directory), &[]).is_err());
        fs::write(&source, "external").unwrap();
        assert!(
            state
                .save(2, Some(&source), &[])
                .unwrap_err()
                .contains("changed on disk")
        );
        assert_eq!(state.persistence(), before);
        assert_eq!(state.editor.revision(), 2);
        assert!(!state.editor.dirty() && state.editor.can_redo());
        state.editor.redo(2).unwrap();
        assert_eq!(state.editor.document().cues()[0].text(), "Edited");
        assert_eq!(fs::read_to_string(source).unwrap(), "external");
    }
}
