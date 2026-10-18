use super::*;

pub(super) const ASS: &str = include_str!("../../../tests/fixtures/subtitles/styles.ass");
pub(super) const SSA: &str = include_str!("../../../tests/fixtures/subtitles/styles.ssa");

#[test]
fn source_properties_share_owner_history_save_and_recovery_serialization() {
    for (extension, source) in [("ass", ASS), ("ssa", SSA)] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join(format!("original.{extension}"));
        fs::write(&path, source).unwrap();
        let mut store = Store::default();
        open(&mut store, &path);
        let updated = source
            .replace("sans-serif,36", "sans-serif,42")
            .replace("Red title", "New title");
        let candidate = subtitle_formats::parse(&updated, None).unwrap();
        let loaded = store.loaded.as_mut().unwrap();
        loaded
            .editor
            .apply_document(
                0,
                candidate.document,
                candidate.source,
                vsf_application::subtitle_file::validate_document,
            )
            .unwrap();
        assert_eq!(bytes(&store), updated);
        let snapshot = store.snapshot();
        assert_eq!(snapshot.revision, 1);
        assert!(snapshot.dirty && snapshot.can_undo);
        assert!(
            snapshot
                .format
                .unwrap()
                .styles
                .iter()
                .find(|s| s.name == "Title")
                .unwrap()
                .fields
                .iter()
                .any(|(name, value)| name == "Fontsize" && value == "42")
        );
        let record = crate::recovery_store::Record::new_document(
            store.loaded.as_ref().unwrap().source().format(),
            Some(path.to_string_lossy().into_owned()),
            source.into(),
            bytes(&store),
            1,
            1,
        )
        .unwrap();
        let recovery_root = root.path().join("recovery");
        let session = crate::recovery_store::Session::create(&recovery_root).unwrap();
        session.prepare(&record).unwrap().commit().unwrap();
        drop(session);
        let entries = crate::recovery_store::list(&recovery_root).unwrap();
        let mut recovered = Store {
            recovery_root: Some(recovery_root),
            ..Store::default()
        };
        recovered
            .apply(
                Action::Recover {
                    id: entries[0].id.clone(),
                    discard: true,
                },
                0,
                0,
            )
            .unwrap();
        assert_eq!(bytes(&recovered), updated);
        assert!(recovered.snapshot().requires_save_as);
        assert!(!recovered.snapshot().can_undo);
        assert!(
            apply(
                &mut store,
                Action::Save {
                    path: Some(
                        root.path()
                            .join("missing")
                            .join(format!("bad.{extension}"))
                            .to_string_lossy()
                            .into_owned()
                    )
                }
            )
            .is_err()
        );
        assert_eq!(bytes(&store), updated);
        assert_eq!(fs::read_to_string(&path).unwrap(), source);
        let saved = root.path().join(format!("saved.{extension}"));
        apply(
            &mut store,
            Action::Save {
                path: Some(saved.to_string_lossy().into_owned()),
            },
        )
        .unwrap();
        assert_eq!(fs::read_to_string(&saved).unwrap(), updated);
        apply(&mut store, Action::Undo).unwrap();
        assert_eq!(bytes(&store), source);
        assert!(store.snapshot().dirty);
        apply(&mut store, Action::Redo).unwrap();
        assert_eq!(bytes(&store), updated);
        assert!(!store.snapshot().dirty);
        assert_eq!(fs::read_to_string(&path).unwrap(), source);
    }
}

#[test]
fn draft_formatting_captures_style_defaults_and_rejects_stale_context() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("source.ass");
    fs::write(&path, ASS).unwrap();
    let docs = Documents::default();
    open(&mut docs.0.lock().unwrap(), &path);
    assert_eq!(docs.subtitle_extension().unwrap(), "ass");
    let mut context = crate::text_review::FormattingContext {
        generation: 1.try_into().unwrap(),
        revision: 0.try_into().unwrap(),
        cue_id: 1.try_into().unwrap(),
        style_name: "Title".into(),
    };
    use vsf_application::text::Format;
    assert!(
        docs.format_text("Hello tail", 0, 5, Format::Bold, None)
            .is_err()
    );
    let formatted = docs
        .format_text("Hello tail", 0, 5, Format::Bold, Some(&context))
        .unwrap();
    assert_eq!(formatted.text, r"{\b0}Hello{\b1} tail");
    context.style_name = "Default".into();
    assert_eq!(
        docs.format_text("Hello tail", 0, 5, Format::Bold, Some(&context))
            .unwrap()
            .text,
        r"{\b1}Hello{\b0} tail"
    );
    context.revision = 1.try_into().unwrap();
    assert!(
        docs.format_text("Hello tail", 0, 5, Format::Bold, Some(&context))
            .unwrap_err()
            .contains("changed")
    );
    assert_eq!(bytes(&docs.0.lock().unwrap()), ASS);
}

fn open(store: &mut Store, path: &Path) -> Snapshot {
    let previous = store.snapshot();
    store
        .apply(
            Action::Open {
                path: path.to_string_lossy().into_owned(),
                discard: true,
            },
            previous.generation,
            previous.revision,
        )
        .unwrap()
}
fn apply(store: &mut Store, action: Action) -> Result<Snapshot> {
    let previous = store.snapshot();
    store.apply(action, previous.generation, previous.revision)
}
fn bytes(store: &Store) -> String {
    let loaded = store.loaded.as_ref().unwrap();
    loaded.source().serialize(loaded.editor.document()).unwrap()
}
fn edit(style: &str, start_ms: u64, text: &str) -> Action {
    Action::EditStyled {
        cue_id: 1,
        start_ms,
        end_ms: 3000,
        text: text.into(),
        style: style.into(),
    }
}

#[test]
fn ass_ssa_edit_style_save_reopen_and_history_preserve_opaque_content() {
    for (extension, source) in [("ass", ASS), ("ssa", SSA)] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join(format!("original.{extension}"));
        fs::write(&path, source).unwrap();
        let mut store = Store::default();
        let snapshot = open(&mut store, &path);
        assert_eq!(snapshot.format.as_ref().unwrap().name, extension);
        assert_eq!(snapshot.format.as_ref().unwrap().styles.len(), 2);
        assert_eq!(snapshot.cues[0].style.as_deref(), Some("Title"));
        apply(&mut store, Action::Save { path: None }).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), source);
        let snapshot = apply(&mut store, edit("Default", 1010, "{\\b1}Edited\n世界")).unwrap();
        assert_eq!(snapshot.revision, 1);
        assert_eq!(snapshot.cues[0].text, "{\\b1}Edited\\N世界");
        assert_eq!(snapshot.cues[0].style.as_deref(), Some("Default"));
        let changed = bytes(&store);
        let expected = source.replace("Red title", "{\\b1}Edited\\N世界");
        let expected = if extension == "ass" {
            expected.replace(
                "Dialogue: Title,0:00:03.00,0:00:01.00,",
                "Dialogue: Default,0:00:03.00,0:00:01.01,",
            )
        } else {
            expected.replace(
                "Dialogue: Marked=0,0:00:01.00,0:00:03.00,Title,",
                "Dialogue: Marked=0,0:00:01.01,0:00:03.00,Default,",
            )
        };
        assert_eq!(changed, expected);
        assert!(changed.contains("Opaque: exact bytes,untouched"));
        assert!(changed.contains("Comment:"));
        apply(&mut store, Action::Undo).unwrap();
        assert_eq!(bytes(&store), source);
        apply(&mut store, Action::Redo).unwrap();
        assert_eq!(bytes(&store), changed);
        let copy = root.path().join(format!("copy.{extension}"));
        apply(
            &mut store,
            Action::Save {
                path: Some(copy.to_string_lossy().into_owned()),
            },
        )
        .unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), source);
        assert_eq!(fs::read_to_string(&copy).unwrap(), changed);
        open(&mut store, &copy);
        assert_eq!(bytes(&store), changed);
        assert_eq!(store.snapshot().cues[0].style.as_deref(), Some("Default"));
    }
}

#[test]
fn rejected_format_grid_style_open_and_save_preserve_history_and_files() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("source.ass");
    fs::write(&path, ASS).unwrap();
    let mut store = Store::default();
    open(&mut store, &path);
    apply(&mut store, edit("Default", 1010, "Change")).unwrap();
    apply(&mut store, Action::Undo).unwrap();
    for action in [edit("missing", 1000, "No"), edit("Title", 1001, "No")] {
        assert!(apply(&mut store, action).is_err());
        assert_eq!(bytes(&store), ASS);
        assert!(store.snapshot().can_redo);
    }
    apply(&mut store, Action::Redo).unwrap();
    let key = store.checkpoint_key();
    let accepted = bytes(&store);
    let wrong = root.path().join("destination.srt");
    fs::write(&wrong, "do not overwrite").unwrap();
    assert!(
        apply(
            &mut store,
            Action::Save {
                path: Some(wrong.to_string_lossy().into_owned())
            }
        )
        .unwrap_err()
        .contains(".ass")
    );
    assert!(
        apply(
            &mut store,
            Action::Open {
                path: wrong.to_string_lossy().into_owned(),
                discard: true
            }
        )
        .is_err()
    );
    assert_eq!(bytes(&store), accepted);
    assert!(store.checkpoint_key() == key);
    assert_eq!(fs::read_to_string(&wrong).unwrap(), "do not overwrite");
    assert_eq!(fs::read_to_string(&path).unwrap(), ASS);
    fs::write(&path, "external change").unwrap();
    assert!(
        apply(&mut store, Action::Save { path: None })
            .unwrap_err()
            .contains("changed on disk")
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), "external change");
}

#[test]
fn recovery_retains_both_formats_and_requires_explicit_save_as() {
    for (format, original) in [
        (subtitle_formats::Format::Ass, ASS),
        (subtitle_formats::Format::Ssa, SSA),
    ] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join(format!("source.{}", format.extension()));
        fs::write(&path, original).unwrap();
        let mut store = Store::default();
        open(&mut store, &path);
        apply(&mut store, edit("Default", 1000, "{\\i1}Recovered 世界")).unwrap();
        let accepted = bytes(&store);
        let recovery = root.path().join("recovery");
        let session = crate::recovery_store::Session::create(&recovery).unwrap();
        let record = crate::recovery_store::Record::new_document(
            format,
            Some(path.to_string_lossy().into_owned()),
            original.into(),
            accepted.clone(),
            1,
            1,
        )
        .unwrap();
        session.write(Some(&record)).unwrap();
        drop(session);
        let entries = crate::recovery_store::list(&recovery).unwrap();
        let mut restored = Store {
            recovery_root: Some(recovery.clone()),
            ..Store::default()
        };
        apply(
            &mut restored,
            Action::Recover {
                id: entries[0].id.clone(),
                discard: false,
            },
        )
        .unwrap();
        assert!(restored.snapshot().requires_save_as);
        assert_eq!(bytes(&restored), accepted);
        assert_eq!(restored.snapshot().format.unwrap().name, format.extension());
        assert!(apply(&mut restored, Action::Save { path: None }).is_err());
        let copy = root
            .path()
            .join(format!("recovered.{}", format.extension()));
        apply(
            &mut restored,
            Action::Save {
                path: Some(copy.to_string_lossy().into_owned()),
            },
        )
        .unwrap();
        assert_eq!(fs::read_to_string(copy).unwrap(), accepted);
        assert_eq!(fs::read_to_string(path).unwrap(), original);
        assert_eq!(crate::recovery_store::list(&recovery).unwrap().len(), 1);
    }
}

#[test]
fn structural_edits_retain_metadata_and_reject_timed_effects() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("source.ass");
    fs::write(&path, ASS).unwrap();
    let mut store = Store::default();
    open(&mut store, &path);
    apply(
        &mut store,
        Action::Split {
            cue_id: 1,
            at_ms: 2000,
        },
    )
    .unwrap();
    let cues = store.snapshot().cues;
    assert_eq!(cues[0].style, cues[1].style);
    let new_id = cues[1].id;
    apply(
        &mut store,
        Action::Merge {
            first_id: 1,
            second_id: new_id,
        },
    )
    .unwrap();
    assert_eq!(store.snapshot().cues[0].text, "Red title\\N{\\r}Red title");
    apply(&mut store, edit("Title", 1000, "{\\k40}Karaoke")).unwrap();
    let before = bytes(&store);
    assert!(
        apply(
            &mut store,
            Action::Split {
                cue_id: 1,
                at_ms: 2000
            }
        )
        .unwrap_err()
        .contains("cue-relative")
    );
    assert_eq!(bytes(&store), before);
    apply(
        &mut store,
        Action::Create {
            after_id: Some(2),
            start_ms: 5000,
            end_ms: 6000,
            text: "New\nline".into(),
        },
    )
    .unwrap();
    let snapshot = store.snapshot();
    assert_eq!(
        snapshot.cues.last().unwrap().style.as_deref(),
        Some("Default")
    );
    assert_eq!(snapshot.cues.last().unwrap().text, "New\\Nline");
}

#[test]
fn unchanged_style_retains_field_whitespace_and_pathless_projects_have_no_format_reference() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("source.ass");
    let original = ASS.replace("Dialogue: Title,", "Dialogue:  Title  ,");
    fs::write(&path, &original).unwrap();
    let docs = Documents::default();
    {
        let mut store = docs.0.lock().unwrap();
        open(&mut store, &path);
        apply(&mut store, edit("Title", 1000, "Changed")).unwrap();
        assert!(bytes(&store).contains("Dialogue:  Title  ,"));
    }
    docs.with_project_subtitle(Some(&path.canonicalize().unwrap()), |_, format, _| {
        assert_eq!(format, Some(subtitle_formats::Format::Ass));
        Ok(())
    })
    .unwrap();
    {
        let mut store = docs.0.lock().unwrap();
        apply(&mut store, Action::Close { discard: true }).unwrap();
        apply(
            &mut store,
            Action::Create {
                after_id: None,
                start_ms: 0,
                end_ms: 1000,
                text: "Untitled".into(),
            },
        )
        .unwrap();
    }
    docs.with_project_subtitle(None, |_, format, _| {
        assert!(format.is_none());
        Ok(())
    })
    .unwrap();
}

#[test]
fn styled_timing_workers_capture_format_and_accept_serializable_undoable_proposals() {
    use crate::{quality_profile::Policy, scenes::Scenes, timing_review::Phase};
    use std::time::{Duration, Instant};
    use vsf_application::quality::{Profile, duration::Scope};
    fn policy(p: Profile) -> Policy {
        let timing = crate::timing_preset::Settings {
            cue_gap_ms: p.timing.cue_gap_ms,
            scene_before_ms: p.timing.scene_before_ms,
            scene_after_ms: p.timing.scene_after_ms,
            ..Default::default()
        };
        Policy::new(p, timing).unwrap()
    }
    for (extension, source) in [("ass", ASS), ("ssa", SSA)] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join(format!("timing.{extension}"));
        let parsed = subtitle_formats::parse(source, None).unwrap();
        let cues = parsed
            .document
            .cues()
            .iter()
            .zip([(1000, 2000), (2050, 3000)])
            .map(|(c, (start, end))| {
                subtitle_core::Cue::new(
                    c.id(),
                    Milliseconds::new(start).unwrap(),
                    Milliseconds::new(end).unwrap(),
                    c.text().into(),
                )
                .unwrap()
                .with_metadata(c.metadata().unwrap().clone())
            })
            .collect();
        let original = parsed
            .source
            .serialize(&subtitle_core::Document::new(cues).unwrap())
            .unwrap();
        fs::write(&path, &original).unwrap();
        let docs = Documents::default();
        open(&mut docs.0.lock().unwrap(), &path);
        let mut p = Profile::default().custom("ass-owner-timing".into(), "ASS owner timing".into());
        p.rules.duration.enabled = false;
        p.rules.speed.enabled = false;
        p.timing.cue_gap_ms = 83;
        p.timing.scene_before_ms = 0;
        p.timing.scene_after_ms = 0;
        docs.apply_quality_defaults(
            docs.quality_context().unwrap().epoch,
            policy(p.clone()),
            || Ok(()),
        )
        .unwrap();
        let scenes = Scenes::default();
        let captured = scenes.capture_quality(0, false).unwrap();
        let id = docs.gap_start(1, 0, captured).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let v = docs.gap_view(id, 0, 100).unwrap();
            if v.phase == crate::gap_review::Phase::Reviewing {
                assert_eq!(v.changed, 1);
                break;
            }
            assert_eq!(
                v.phase,
                crate::gap_review::Phase::Preparing,
                "{:?}",
                v.error
            );
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert_eq!(docs.gap_accept(id, false).unwrap().cues[0].end_ms, 1960);
        apply(&mut docs.0.lock().unwrap(), Action::Undo).unwrap();
        assert_eq!(bytes(&docs.0.lock().unwrap()), original);

        p.rules.duration.enabled = true;
        p.rules.duration.minimum_ms = 1103;
        docs.apply_quality_defaults(
            docs.quality_context().unwrap().epoch,
            policy(p.clone()),
            || Ok(()),
        )
        .unwrap();
        let snapshot = docs.apply(Action::Status, 0, 0).unwrap();
        let id = docs
            .duration_repair_start(
                snapshot.generation,
                snapshot.revision,
                scenes.capture_quality(0, false).unwrap(),
                Scope::Document,
            )
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let v = docs.duration_repair_view(id, 0, 100).unwrap();
            if v.phase == Phase::Reviewing {
                assert_eq!(v.changed, 1);
                break;
            }
            assert_eq!(v.phase, Phase::Preparing, "{:?}", v.error);
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert_eq!(
            docs.duration_repair_accept(id).unwrap().cues[1].end_ms,
            3160
        );
        apply(&mut docs.0.lock().unwrap(), Action::Undo).unwrap();
        assert_eq!(bytes(&docs.0.lock().unwrap()), original);

        p.rules.duration.enabled = false;
        p.timing.scene_before_ms = 2;
        p.timing.scene_after_ms = 3;
        docs.apply_quality_defaults(docs.quality_context().unwrap().epoch, policy(p), || Ok(()))
            .unwrap();
        scenes.test_completed(7, vec![1.0], false);
        let snapshot = docs.apply(Action::Status, 0, 0).unwrap();
        let id = docs
            .scene_repair_start(
                snapshot.generation,
                snapshot.revision,
                scenes.capture_quality(7, true).unwrap(),
            )
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let v = docs.scene_repair_view(id, 0, 100).unwrap();
            if v.phase == Phase::Reviewing {
                assert_eq!(v.changed, 1);
                break;
            }
            assert_eq!(v.phase, Phase::Preparing, "{:?}", v.error);
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert_eq!(docs.scene_repair_accept(id).unwrap().cues[0].start_ms, 1010);
        apply(&mut docs.0.lock().unwrap(), Action::Undo).unwrap();
        assert_eq!(bytes(&docs.0.lock().unwrap()), original);
    }
}
