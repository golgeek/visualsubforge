use super::*;

pub(super) const ASS: &str = include_str!("../../../tests/fixtures/subtitles/styles.ass");
pub(super) const SSA: &str = include_str!("../../../tests/fixtures/subtitles/styles.ssa");

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
    loaded.source.serialize(loaded.editor.document()).unwrap()
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
    docs.with_project_subtitle(Some(&path.canonicalize().unwrap()), |_, format| {
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
    docs.with_project_subtitle(None, |_, format| {
        assert!(format.is_none());
        Ok(())
    })
    .unwrap();
}
