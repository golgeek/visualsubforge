//! Accepted policy shares the document mutex so capture and later acceptance can
//! guard one document/profile revision atomically. It is not subtitle undo state.
use super::*;
use crate::quality_profile::{Context, Policy};
use vsf_application::language::{Lexicon, WordLists};

pub(super) struct Capture {
    profile: vsf_application::quality::Profile,
    format: subtitle_formats::Format,
    lexicon: Arc<Lexicon>,
}
impl Capture {
    pub(super) fn evaluation(&self) -> vsf_application::quality::Evaluation<'_> {
        vsf_application::quality::Evaluation {
            profile: &self.profile,
            format: self.format,
            lexicon: Some(&self.lexicon),
        }
    }
}

#[derive(Default)]
pub(super) struct State {
    initialized: bool,
    epoch: u64,
    defaults: Policy,
    current: Policy,
    personal: WordLists,
    ignored: WordLists,
    lexicon: Arc<Lexicon>,
}
impl State {
    pub(super) fn capture(&self, format: subtitle_formats::Format) -> Capture {
        Capture {
            profile: self.current.profile.clone(),
            format,
            lexicon: self.lexicon(),
        }
    }
    pub(super) fn lexicon(&self) -> Arc<Lexicon> {
        self.lexicon.clone()
    }
    pub(super) fn epoch(&self) -> u64 {
        self.epoch
    }
    fn next_epoch(&self) -> Result<u64> {
        self.epoch
            .checked_add(1)
            .filter(|&n| n <= crate::ipc_number::MAX_SAFE_INTEGER)
            .ok_or_else(|| "Quality policy identities exhausted".into())
    }
    pub(super) fn context(&self) -> Context {
        Context {
            epoch: self.epoch,
            profile: self.current.profile.clone(),
            timing: self.current.timing.clone(),
        }
    }
}

impl Documents {
    pub(crate) fn language_words(&self) -> Result<crate::language_tools::Words> {
        let store = self.0.lock().map_err(|_| "Document state unavailable")?;
        Ok(crate::language_tools::Words {
            epoch: store.quality.epoch.try_into()?,
            personal: store.quality.personal.clone(),
            ignored: store.quality.ignored.clone(),
        })
    }
    pub(crate) fn language_words_change(
        &self,
        epoch: u64,
        change: crate::language_tools::Change,
        persist: impl FnOnce(&WordLists) -> Result<()>,
    ) -> Result<()> {
        use crate::language_tools::Change;
        let mut store = self.0.lock().map_err(|_| "Document state unavailable")?;
        if store.quality.epoch != epoch {
            return Err("Language settings changed; refresh before changing words".into());
        }
        if !store.quality.initialized {
            return Err("Load user settings before changing language words".into());
        }
        let mut personal = store.quality.personal.clone();
        let mut ignored = store.quality.ignored.clone();
        match change {
            Change::Ignore { language, word } => {
                ignored.insert(language, &word).map_err(|e| e.to_string())?
            }
            Change::Remember { language, word } => personal
                .insert(language, &word)
                .map_err(|e| e.to_string())?,
            Change::Forget { language, word } => personal
                .remove(language, &word)
                .map_err(|e| e.to_string())?,
            Change::ClearIgnored => ignored = WordLists::default(),
        }
        if personal == store.quality.personal && ignored == store.quality.ignored {
            return Ok(());
        }
        let next = store.quality.next_epoch()?;
        let lexicon = Lexicon::new(&personal, &ignored).map_err(|e| e.to_string())?;
        if personal != store.quality.personal {
            persist(&personal)?;
        }
        store.quality.personal = personal;
        store.quality.ignored = ignored;
        store.quality.lexicon = Arc::new(lexicon);
        store.quality.epoch = next;
        store.invalidate_checks();
        Ok(())
    }
    pub(crate) fn quality_context(&self) -> Result<Context> {
        Ok(self
            .0
            .lock()
            .map_err(|_| "Document state unavailable")?
            .quality
            .context())
    }
    pub(crate) fn default_quality_policy(&self) -> Result<Policy> {
        Ok(self
            .0
            .lock()
            .map_err(|_| "Document state unavailable")?
            .quality
            .defaults
            .clone())
    }
    pub(crate) fn initialize_preferences(&self, policy: Policy, personal: WordLists) -> Result<()> {
        let mut store = self.0.lock().map_err(|_| "Document state unavailable")?;
        if !store.quality.initialized {
            let lexicon =
                Lexicon::new(&personal, &store.quality.ignored).map_err(|e| e.to_string())?;
            store.quality.epoch = store.quality.next_epoch()?;
            store.quality.current = policy.clone();
            store.quality.defaults = policy;
            store.quality.initialized = true;
            store.quality.personal = personal;
            store.quality.lexicon = Arc::new(lexicon);
            store.invalidate_checks();
        }
        Ok(())
    }
    /// Called on a blocking worker. Reserve the identity and perform persistence
    /// before changing either defaults or the current workspace. Locking serializes
    /// this with project attach, checks and edits; failed writes leave all intact.
    pub(crate) fn apply_preferences(
        &self,
        epoch: u64,
        policy: Policy,
        personal: WordLists,
        persist: impl FnOnce() -> Result<()>,
    ) -> Result<()> {
        let mut store = self.0.lock().map_err(|_| "Document state unavailable")?;
        if store.quality.epoch != epoch {
            return Err(
                "Workspace settings changed; reopen Settings before applying this draft".into(),
            );
        }
        let next = store.quality.next_epoch()?;
        let lexicon = Lexicon::new(&personal, &store.quality.ignored).map_err(|e| e.to_string())?;
        persist()?;
        store.quality.defaults = policy.clone();
        store.quality.current = policy;
        store.quality.epoch = next;
        store.quality.initialized = true;
        store.quality.personal = personal;
        store.quality.lexicon = Arc::new(lexicon);
        store.invalidate_checks();
        Ok(())
    }
    #[cfg(test)]
    pub(crate) fn initialize_quality(&self, policy: Policy) -> Result<()> {
        self.initialize_preferences(policy, WordLists::default())
    }
    #[cfg(test)]
    pub(crate) fn apply_quality_defaults(
        &self,
        epoch: u64,
        policy: Policy,
        persist: impl FnOnce() -> Result<()>,
    ) -> Result<()> {
        self.apply_preferences(epoch, policy, WordLists::default(), persist)
    }
    pub(crate) fn reset_quality(&self) -> Result<()> {
        let mut store = self.0.lock().map_err(|_| "Document state unavailable")?;
        let next = store.quality.next_epoch()?;
        store.quality.current = store.quality.defaults.clone();
        store.quality.epoch = next;
        store.invalidate_checks();
        Ok(())
    }
    pub(crate) fn attach_quality_project(
        &self,
        projects: &crate::project::Projects,
        token: u64,
        generation: u64,
        revision: u64,
        media_path: Option<&Path>,
        reference_epoch: u64,
    ) -> Result<crate::project::Attachment> {
        let mut store = self.0.lock().map_err(|_| "Document state unavailable")?;
        if store.generation != generation
            || store.loaded.as_ref().map_or(0, |d| d.editor.revision()) != revision
            || store.loaded.as_ref().map_or(0, |_| store.translation.epoch) != reference_epoch
        {
            return Err("Document changed during project restoration".into());
        }
        let next = store.quality.next_epoch()?;
        let translation_epoch = store.translation.next_epoch()?;
        let subtitle_path = store.loaded.as_ref().and_then(|loaded| loaded.path.clone());
        // Existing publication order is document -> project. The callback only
        // assigns validated staged data and cannot fail after attachment changes.
        let attached = projects.attach_with(
            token,
            media_path,
            subtitle_path.as_deref(),
            |policy, translation| {
                store.quality.current = policy.clone();
                store.quality.epoch = next;
                if let Some(loaded) = &mut store.loaded {
                    loaded.translation = translation;
                }
                store.translation.epoch = translation_epoch;
            },
        )?;
        store.invalidate_checks();
        Ok(attached)
    }
    pub(crate) fn detach_quality_project(&self, projects: &crate::project::Projects) -> Result<()> {
        let mut store = self.0.lock().map_err(|_| "Document state unavailable")?;
        let next = store.quality.next_epoch()?;
        projects.detach()?;
        store.quality.current = store.quality.defaults.clone();
        store.quality.epoch = next;
        store.invalidate_checks();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quality_profile::Profile;

    fn custom() -> Policy {
        let mut profile = Profile::default().custom("mine".into(), "Mine".into());
        profile.rules.lines.maximum_characters = 48;
        Policy::new(profile, Default::default()).unwrap()
    }

    #[test]
    fn failed_apply_preserves_policy_epoch_defaults_and_document_history() {
        let docs = Documents::default();
        docs.initialize_quality(Policy::default()).unwrap();
        let before = docs.quality_context().unwrap();
        assert!(
            docs.apply_quality_defaults(before.epoch, custom(), || Err(
                "injected write failure".into()
            ))
            .is_err()
        );
        let failed = docs.quality_context().unwrap();
        assert_eq!(failed.epoch, before.epoch);
        assert_eq!(failed.profile, before.profile);
        docs.reset_quality().unwrap();
        assert_eq!(docs.quality_context().unwrap().profile, before.profile);
        let epoch = docs.quality_context().unwrap().epoch;
        docs.apply_quality_defaults(epoch, custom(), || Ok(()))
            .unwrap();
        // A delayed startup read cannot overwrite an already applied choice.
        docs.initialize_quality(Policy::default()).unwrap();
        assert_eq!(docs.quality_context().unwrap().profile.name, "Mine");
        let snapshot = docs.0.lock().unwrap().snapshot();
        assert_eq!(
            (
                snapshot.generation,
                snapshot.revision,
                snapshot.dirty,
                snapshot.can_undo
            ),
            (0, 0, false, false)
        );
    }

    #[test]
    fn concurrent_apply_rejects_stale_drafts_before_persistence_without_touching_history() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("cue.srt");
        std::fs::write(&file, "1\n00:00:01,000 --> 00:00:03,000\nBefore\n\n").unwrap();
        let docs = Documents::default();
        docs.apply(
            Action::Open {
                path: file.to_string_lossy().into_owned(),
                discard: false,
            },
            0,
            0,
        )
        .unwrap();
        docs.apply(
            Action::Edit {
                cue_id: 1,
                start_ms: 1000,
                end_ms: 3000,
                text: "After".into(),
            },
            1,
            0,
        )
        .unwrap();
        docs.apply(Action::Undo, 1, 1).unwrap();
        let before = serde_json::to_value(docs.apply(Action::Status, 1, 2).unwrap()).unwrap();
        let epoch = docs.quality_context().unwrap().epoch;
        let barrier = std::sync::Barrier::new(2);
        let writes = std::sync::atomic::AtomicUsize::new(0);
        let results = std::thread::scope(|scope| {
            let apply = || {
                barrier.wait();
                docs.apply_quality_defaults(epoch, custom(), || {
                    writes.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    Ok(())
                })
            };
            let first = scope.spawn(apply);
            let second = scope.spawn(apply);
            [first.join().unwrap(), second.join().unwrap()]
        });
        assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
        assert_eq!(writes.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(
            serde_json::to_value(docs.apply(Action::Status, 1, 2).unwrap()).unwrap(),
            before
        );
        let redo = docs.apply(Action::Redo, 1, 2).unwrap();
        assert_eq!(redo.cues[0].text, "After");
        assert!(redo.dirty);
    }

    #[test]
    fn project_policy_is_staged_validated_and_does_not_replace_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let media = dir.path().join("clip.mkv");
        std::fs::write(&media, b"fixture").unwrap();
        let media = media.canonicalize().unwrap();
        let identity = crate::project::identify(&media).unwrap();
        let profile = Profile::builtin("sw3-uk-vo-subset").unwrap();
        let timing = crate::timing_preset::Settings {
            cue_gap_ms: 160,
            scene_before_ms: 120,
            scene_after_ms: 120,
            ..Default::default()
        };
        let path = dir.path().join("policy.vsfproject");
        let value = serde_json::json!({
            "version":1, "mediaPath":"clip.mkv", "subtitlePath":null, "mediaIdentity":identity,
            "positionSeconds":0, "scenes":null, "options": {
                "waveformStart":0, "waveformSpan":20, "followPlayback":false,
                "followSubtitle":false,"showScenes":true,"timing":timing,"quality":profile
            }
        });
        std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        let docs = Documents::default();
        docs.apply_quality_defaults(0, custom(), || Ok(())).unwrap();
        let projects = crate::project::Projects::default();
        let opened = projects.open(&path).unwrap();
        let epoch = docs.quality_context().unwrap().epoch;
        assert!(
            docs.attach_quality_project(&projects, opened.token, 99, 0, Some(&media), 0)
                .is_err()
        );
        assert!(
            docs.attach_quality_project(
                &projects,
                opened.token,
                0,
                0,
                Some(&dir.path().join("other.mkv")),
                0,
            )
            .is_err()
        );
        assert_eq!(docs.quality_context().unwrap().epoch, epoch);
        // External changes after staging cannot silently replace the staged policy.
        std::fs::write(&path, b"changed after read").unwrap();
        docs.attach_quality_project(&projects, opened.token, 0, 0, Some(&media), 0)
            .unwrap();
        let context = docs.quality_context().unwrap();
        assert_eq!(context.profile, profile);
        assert_eq!(context.timing, timing);
        assert!(!context.profile.limitations.is_empty());
        assert!(
            docs.apply_quality_defaults(epoch, custom(), || panic!("stale Apply must not write"))
                .is_err()
        );
        docs.detach_quality_project(&projects).unwrap();
        assert_eq!(docs.quality_context().unwrap().profile.name, "Mine");
        assert!(
            docs.attach_quality_project(&projects, opened.token, 0, 0, Some(&media), 0)
                .is_err()
        );
    }
}
