//! Read-only availability for a captured selection. Trial edits run outside
//! the store lock on private documents, through the same acceptance boundary.
use super::{Documents, Result};
use crate::ipc_number::SafeU64;
use subtitle_core::CueId;
use vsf_application::subtitle_operations::{GroupActions, group_actions};

impl Documents {
    fn group_actions(
        &self,
        generation: u64,
        revision: u64,
        ids: Vec<SafeU64>,
    ) -> Result<GroupActions> {
        let lock = || {
            self.0
                .lock()
                .map_err(|_| "Document worker unavailable".to_string())
        };
        let (doc, source) = {
            let store = lock()?;
            store
                .check_version(generation, revision)
                .map_err(|e| e.message().to_owned())?;
            let loaded = store.loaded.as_ref().ok_or("Open subtitles first")?;
            (loaded.editor.document().clone(), loaded.source().clone())
        };
        let ids: Vec<_> = ids.into_iter().map(|id| CueId(id.get())).collect();
        let actions = group_actions(&doc, &source, &ids);
        lock()?
            .check_version(generation, revision)
            .map_err(|e| e.message().to_owned())?;
        Ok(actions)
    }
}

#[tauri::command]
#[specta::specta]
pub async fn cue_group_actions(
    documents: tauri::State<'_, Documents>,
    generation: SafeU64,
    revision: SafeU64,
    ids: Vec<SafeU64>,
) -> Result<GroupActions> {
    let documents = documents.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        documents.group_actions(generation.get(), revision.get(), ids)
    })
    .await
    .map_err(|error| format!("Cannot check cue group actions: {error}"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Action;

    fn srt(texts: &[&str]) -> String {
        texts
            .iter()
            .enumerate()
            .map(|(i, text)| format!("{}\n00:00:01,000 --> 00:00:03,000\n{text}\n\n", i + 1))
            .collect()
    }

    #[test]
    fn group_reasons_match_actual_edits_and_preserve_the_captured_revision() {
        let ass = super::super::ass_tests::ASS;
        let vtt = "WEBVTT\n\n00:01.000 --> 00:02.000\n<v Alice>One\n\n00:02.000 --> 00:03.000\n<v Bob>Two\n";
        let large = "X".repeat(33_000);
        type Fixture<'a> = (&'a str, String, &'a [u64], [Option<&'a str>; 3]);
        let fixtures: Vec<Fixture> = vec![
            ("srt", srt(&["A", "B"]), &[1, 2], [None, None, None]),
            (
                "srt",
                srt(&["A", "B", "C"]),
                &[1, 3],
                [Some("not adjacent"), Some("not adjacent"), None],
            ),
            (
                "ass",
                ass.to_owned(),
                &[1, 2],
                [Some("metadata"), Some("metadata"), None],
            ),
            (
                "srt",
                srt(&[r"{\k10}A", "B"]),
                &[1, 2],
                [Some("effects"), Some("effects"), None],
            ),
            (
                "vtt",
                vtt.to_owned(),
                &[1, 2],
                [Some("markup scope"), Some("markup scope"), None],
            ),
            (
                "srt",
                srt(&["<i></i>", "B"]),
                &[1, 2],
                [None, Some("text in both cues"), None],
            ),
            (
                "srt",
                srt(&["A", "B", "C"]),
                &[1, 2, 3],
                [None, Some("exactly two"), None],
            ),
            (
                "srt",
                srt(&[&format!("<i>{large}</i>"), &large, "C"]),
                &[1, 2, 3],
                [Some("64 KiB"), Some("exactly two"), None],
            ),
            (
                "srt",
                srt(&["A", "B"]),
                &[1, 1],
                [Some("not adjacent"), Some("not adjacent"), Some("unique")],
            ),
        ];
        let dir = tempfile::tempdir().unwrap();
        for (format, source, selected, expected) in fixtures {
            let path = dir.path().join(format!("sample.{format}"));
            std::fs::write(&path, source).unwrap();
            let ids: Vec<_> = selected
                .iter()
                .map(|&id| SafeU64::try_from(id).unwrap())
                .collect();
            for (i, expected) in expected.into_iter().enumerate() {
                let docs = Documents::default();
                let snapshot = docs
                    .0
                    .lock()
                    .unwrap()
                    .apply(
                        Action::Open {
                            path: path.to_string_lossy().into_owned(),
                            discard: false,
                        },
                        0,
                        0,
                    )
                    .unwrap();
                let generation = snapshot.generation;
                let revision = snapshot.revision;
                assert!(
                    docs.group_actions(generation, revision + 1, ids.clone())
                        .is_err()
                );
                let answers = docs
                    .group_actions(generation, revision, ids.clone())
                    .unwrap();
                let reason = [
                    answers.merge_run,
                    answers.merge_as_dialog,
                    answers.delete_many,
                ]
                .into_iter()
                .nth(i)
                .unwrap();
                let action = if i == 2 {
                    Action::DeleteMany { ids: ids.clone() }
                } else {
                    Action::MergeRun {
                        ids: ids.clone(),
                        dialogue: i == 1,
                    }
                };
                let result = docs.0.lock().unwrap().apply(action, generation, revision);
                assert_eq!(reason, result.err());
                assert!(
                    match expected {
                        Some(fragment) => reason.as_deref().is_some_and(|r| r.contains(fragment)),
                        None => reason.is_none(),
                    },
                    "{format} {selected:?} operation {i}: {reason:?}, expected {expected:?}"
                );
            }
        }
    }
}
