use super::*;
use vsf_application::create_project::{Outcome, Targets};

impl Documents {
    pub(crate) fn create_project<T>(
        &self,
        version: (u64, u64),
        original: Option<&Path>,
        targets: &Targets,
        publish: impl FnOnce(
            &crate::quality_profile::Context,
            Option<subtitle_formats::Format>,
            &translation::Context,
            &review::Context,
        ) -> Result<T>,
    ) -> Result<Outcome<T>> {
        let mut store = self.0.lock().map_err(|_| "Document worker unavailable")?;
        let (generation, revision) = version;
        store
            .check_version(generation, revision)
            .map_err(|e| e.message().to_owned())?;
        if store.loaded.as_ref().and_then(|d| d.path.as_deref()) != original {
            return Err("Subtitle path changed before creating the project".into());
        }
        let result = targets.publish(
            &mut *store,
            |store| {
                let Some(loaded) = &mut store.loaded else {
                    return if targets.companion.is_none() {
                        Ok(None)
                    } else {
                        Err("No subtitle document is open".into())
                    };
                };
                let needs_companion = loaded.path.is_none() || loaded.recovered || loaded.resynced;
                if needs_companion != targets.companion.is_some() {
                    return Err("Subtitle save requirements changed; reopen Create Project".into());
                }
                if needs_companion || loaded.subtitle_dirty() {
                    loaded.save_subtitles(
                        revision,
                        targets.companion.as_deref(),
                        needs_companion,
                    )?;
                    return Ok(loaded.path.clone());
                }
                Ok(None)
            },
            |store| store.with_project_subtitle(targets.companion.as_deref().or(original), publish),
        );
        Ok(result)
    }
}
