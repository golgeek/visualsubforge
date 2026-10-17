//! The document owner captures accepted state and commits an immutable proposal.
//! Media/job identity is checked by the resync owner before taking this lock.
use super::*;
use vsf_application::resync::{Proposal, SubtitleSnapshot};

#[derive(Clone)]
pub(crate) struct Capture {
    key: CheckpointKey,
    pub snapshot: SubtitleSnapshot,
}

impl Documents {
    pub(crate) fn capture_resync(&self, generation: u64, revision: u64) -> Result<Capture> {
        let store = self.0.lock().map_err(|_| "Document worker unavailable")?;
        let key = store.checkpoint_key();
        if key.generation != generation || key.revision != revision {
            return Err("Subtitle document changed before analysis started".into());
        }
        let loaded = store.loaded.as_ref().ok_or("Open subtitles first")?;
        let snapshot =
            SubtitleSnapshot::new(loaded.editor.document().clone(), loaded.source.clone())
                .map_err(|e| e.to_string())?;
        Ok(Capture { key, snapshot })
    }

    pub(crate) fn resync_is_current(&self, capture: &Capture) -> bool {
        self.0
            .lock()
            .is_ok_and(|store| store.checkpoint_key() == capture.key)
    }

    pub(crate) fn accept_resync(&self, capture: &Capture, proposal: &Proposal) -> Result<Snapshot> {
        let mut store = self.0.lock().map_err(|_| "Document worker unavailable")?;
        if store.checkpoint_key() != capture.key
            || proposal.snapshot().sha256() != capture.snapshot.sha256()
        {
            return Err("Subtitles changed after analysis; analyze again".into());
        }
        let loaded = store.loaded_mut()?;
        let edit = proposal
            .edit(loaded.editor.document())
            .map_err(|e| e.to_string())?;
        // Reuse the domain transaction and accepted-SRT size policy. Only a
        // successful edit may change save protection; a no-op still requires
        // a new destination, but does not manufacture an undo entry.
        loaded.apply_edit(capture.key.revision, edit)?;
        loaded.resynced = true;
        store.invalidate_checks();
        Ok(store.snapshot())
    }

    /// Serialize project publication with document replacement, Save As and
    /// retiming. Call on a blocking worker; lock order is document then project.
    /// A successful project write must refer to the current saved subtitle path,
    /// never the protected source of an unexported resync proposal.
    pub(crate) fn with_project_subtitle<T>(
        &self,
        path: Option<&Path>,
        publish: impl FnOnce(&crate::quality_profile::Context) -> Result<T>,
    ) -> Result<T> {
        let store = self.0.lock().map_err(|_| "Document worker unavailable")?;
        if store.loaded.as_ref().is_some_and(|loaded| loaded.resynced) {
            return Err(
                "Save resynchronized subtitles to a new file before saving the project".into(),
            );
        }
        if store
            .loaded
            .as_ref()
            .and_then(|loaded| loaded.path.as_deref())
            != path
        {
            return Err(
                "Subtitle path changed before project save; retry with the current document".into(),
            );
        }
        publish(&store.quality.context())
    }
}

#[cfg(test)]
mod tests;
