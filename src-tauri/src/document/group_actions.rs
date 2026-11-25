//! Read-only availability for a captured selection. Trial edits run outside
//! the store lock on private documents, through the same acceptance boundary.
use super::{Documents, Editor, Result, Source};
use crate::ipc_number::SafeU64;
use subtitle_core::CueId;
use vsf_application::subtitle_operations::{GroupActions, group_actions};

impl Documents {
    fn inspect_cues<T>(
        &self,
        generation: u64,
        revision: u64,
        inspect: impl FnOnce(Editor<Source>) -> Result<T>,
    ) -> Result<T> {
        let lock = || {
            self.0
                .lock()
                .map_err(|_| "Document worker unavailable".to_string())
        };
        let editor = {
            let store = lock()?;
            store
                .check_version(generation, revision)
                .map_err(|e| e.message().to_owned())?;
            let loaded = store.loaded.as_ref().ok_or("Open subtitles first")?;
            loaded.editor.trial_copy()
        };
        let result = inspect(editor);
        lock()?
            .check_version(generation, revision)
            .map_err(|e| e.message().to_owned())?;
        result
    }

    fn group_actions(
        &self,
        generation: u64,
        revision: u64,
        ids: Vec<SafeU64>,
    ) -> Result<GroupActions> {
        self.inspect_cues(generation, revision, |editor| {
            let ids: Vec<_> = ids.into_iter().map(|id| CueId(id.get())).collect();
            Ok(group_actions(editor.document(), editor.properties(), &ids))
        })
    }

    fn split_midpoint(&self, generation: u64, revision: u64, id: CueId) -> Result<SafeU64> {
        self.inspect_cues(generation, revision, |mut editor| {
            let cue = editor
                .document()
                .cues()
                .iter()
                .find(|cue| cue.id() == id)
                .ok_or("Cue no longer exists")?;
            let at = vsf_application::subtitle_operations::split_midpoint(
                cue,
                editor.properties().format(),
            )?;
            editor.apply_edit_with_properties(
                0,
                subtitle_core::Edit::Split { id, at },
                vsf_application::subtitle_file::validate_document,
            )?;
            Ok(SafeU64::try_from(at.get())?)
        })
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

#[tauri::command]
#[specta::specta]
pub async fn cue_split_midpoint(
    documents: tauri::State<'_, Documents>,
    generation: SafeU64,
    revision: SafeU64,
    cue_id: SafeU64,
) -> Result<SafeU64> {
    let documents = documents.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        documents.split_midpoint(generation.get(), revision.get(), CueId(cue_id.get()))
    })
    .await
    .map_err(|error| format!("Cannot check cue midpoint: {error}"))?
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
    fn midpoint_plan_and_split_round_to_source_ticks_and_undo_the_whole_edit() {
        use super::super::ass_tests::{ASS, SSA};
        let root = tempfile::tempdir().unwrap();
        for (format, source, expected_ms) in [
            (
                "srt",
                "1\n00:00:01,001 --> 00:00:01,006\n<i>A</i>\nB\n".into(),
                1004,
            ),
            (
                "vtt",
                "WEBVTT\n\n00:01.001 --> 00:01.006\n<i>A</i>\nB\n".into(),
                1004,
            ),
            ("ass", ASS.replace("0:00:03.00", "0:00:01.05"), 1030),
            ("ssa", SSA.replace("0:00:03.00", "0:00:01.05"), 1030),
            ("srt", "1\n00:00:01,001 --> 00:00:01,003\nA\n".into(), 1002),
        ] {
            let path = root.path().join(format!("split.{format}"));
            std::fs::write(&path, &source).unwrap();
            let docs = Documents::default();
            let before = docs
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
            let original = &before.cues[0];
            let at = docs
                .split_midpoint(before.generation, before.revision, CueId(original.id))
                .unwrap()
                .get();
            assert_eq!(at, expected_ms, "{format}");
            let mut store = docs.0.lock().unwrap();
            let split = store
                .apply(
                    Action::Split {
                        cue_id: original.id,
                        at_ms: at,
                    },
                    before.generation,
                    before.revision,
                )
                .unwrap();
            let (left, right) = (&split.cues[0], &split.cues[1]);
            assert_eq!(
                (left.start_ms, left.end_ms, right.start_ms, right.end_ms),
                (original.start_ms, at, at, original.end_ms)
            );
            assert_eq!((&left.text, &right.text), (&original.text, &original.text));
            let restored = store
                .apply(Action::Undo, split.generation, split.revision)
                .unwrap();
            assert!(!restored.dirty && !restored.can_undo);
            assert_eq!(
                serde_json::to_value(restored.cues).unwrap(),
                serde_json::to_value(before.cues).unwrap()
            );
        }
    }

    #[test]
    fn midpoint_trial_uses_the_same_new_identity_as_the_accepted_split() {
        // Reusing the deleted ID would restore imported SRT label order,
        // including the padded label, and incorrectly exceed the byte limit.
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("padded.srt");
        let text = "a".repeat(crate::srt_limits::MAX_BYTES / 2 - 40);
        std::fs::write(&path, format!("{}\n00:00:00,000 --> 00:00:02,000\n{text}\n\n2\n00:00:03,000 --> 00:00:04,000\nEnd\n", "0".repeat(128))).unwrap();
        let docs = Documents::default();
        let mut store = docs.0.lock().unwrap();
        let opened = store
            .apply(
                Action::Open {
                    path: path.to_string_lossy().into_owned(),
                    discard: false,
                },
                0,
                0,
            )
            .unwrap();
        let before = store
            .apply(
                Action::Delete { cue_id: 2 },
                opened.generation,
                opened.revision,
            )
            .unwrap();
        drop(store);
        let at = docs.split_midpoint(before.generation, before.revision, CueId(1));
        let split = docs
            .0
            .lock()
            .unwrap()
            .apply(
                Action::Split {
                    cue_id: 1,
                    at_ms: 1000,
                },
                before.generation,
                before.revision,
            )
            .unwrap();
        assert_eq!(split.cues[1].id, 3);
        assert_eq!(at.unwrap().get(), 1000);
    }

    #[test]
    fn midpoint_refuses_one_tick_and_unsafe_format_without_history() {
        for (extension, source, reason) in [
            (
                "srt",
                "1\n00:00:01,000 --> 00:00:01,001\nOne tick\n".to_string(),
                "too short",
            ),
            (
                "ass",
                super::super::ass_tests::ASS.replace("0:00:03.00", "0:00:01.01"),
                "too short",
            ),
            (
                "vtt",
                "WEBVTT\n\nnamed\n00:01.000 --> 00:03.000\nNamed\n".to_string(),
                "identifiers must stay unique",
            ),
        ] {
            let root = tempfile::tempdir().unwrap();
            let path = root.path().join(format!("split.{extension}"));
            std::fs::write(&path, source).unwrap();
            let docs = Documents::default();
            let before = docs
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
            let id = CueId(before.cues[0].id);
            assert!(
                docs.split_midpoint(before.generation, before.revision + 1, id)
                    .is_err()
            );
            assert!(
                docs.split_midpoint(before.generation, before.revision, id)
                    .unwrap_err()
                    .contains(reason)
            );
            let after = docs.0.lock().unwrap().snapshot();
            assert_eq!(before.revision, after.revision);
            assert!(!after.can_undo);
        }
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
