use super::*;

const ORIGINAL: &str = "1\n00:00:01,000 --> 00:00:02,000\nOriginal\n\n";

fn edited(path: &Path, text: &str) -> Store {
    let mut store = Store::default();
    store
        .apply(
            Action::Open {
                path: path.to_string_lossy().into_owned(),
                discard: false,
            },
            0,
            0,
        )
        .unwrap();
    store
        .apply(
            Action::Edit {
                cue_id: 1,
                start_ms: 1100,
                end_ms: 2300,
                text: text.into(),
            },
            1,
            0,
        )
        .unwrap();
    store
}

#[test]
fn competing_save_preserves_history_recovery_and_both_versions_via_save_as() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("shared.srt");
    fs::write(&path, ORIGINAL).unwrap();
    let mut a = edited(&path, "Writer A");
    let mut b = edited(&path, "Writer B");
    let recovery_root = directory.path().join("recovery");
    let session = crate::recovery_store::Session::create(&recovery_root).unwrap();
    let loaded = b.loaded.as_ref().unwrap();
    let accepted = loaded.source.serialize(loaded.editor.document()).unwrap();
    let record = crate::recovery_store::Record::new(
        Some(path.to_string_lossy().into_owned()),
        ORIGINAL.into(),
        accepted.clone(),
        1,
        1,
    )
    .unwrap();
    let key = b.checkpoint_key();
    b.publish_checkpoint(&key, Some(session.prepare(&record).unwrap()), &session)
        .unwrap();

    // Hold the actual protocol lease so overlap is deterministic, with no timing
    // hook in document acceptance or atomic writes. A save must not bypass it.
    let lease = crate::save_lease::acquire(&path).unwrap();
    assert!(
        b.apply(Action::Save { path: None }, 1, 1)
            .unwrap_err()
            .contains("another instance")
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), ORIGINAL);
    assert!(b.checkpoint_key() == key);
    drop(lease);

    a.apply(Action::Save { path: None }, 1, 1).unwrap();
    let winner = fs::read(&path).unwrap();
    assert!(
        b.apply(Action::Save { path: None }, 1, 1)
            .unwrap_err()
            .contains("changed on disk")
    );
    assert_eq!(fs::read(&path).unwrap(), winner);
    let rejected = b.snapshot();
    assert!(rejected.dirty && rejected.can_undo && !rejected.can_redo);
    assert_eq!(rejected.revision, 1);
    assert_eq!(rejected.cues[0].text, "Writer B");
    assert!(b.checkpoint_key() == key);
    drop(session);
    let entries = crate::recovery_store::list(&recovery_root).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(
        crate::recovery_store::recover(&recovery_root, &entries[0].id)
            .unwrap()
            .accepted_srt,
        accepted
    );

    let undone = b.apply(Action::Undo, 1, 1).unwrap();
    assert_eq!(undone.cues[0].text, "Original");
    let redone = b.apply(Action::Redo, 1, 2).unwrap();
    assert!(redone.dirty);
    assert_eq!(redone.cues[0].text, "Writer B");
    let copy = directory.path().join("writer-b.srt");
    assert!(
        !b.apply(
            Action::Save {
                path: Some(copy.to_string_lossy().into_owned())
            },
            1,
            3
        )
        .unwrap()
        .dirty
    );
    assert_eq!(fs::read_to_string(copy).unwrap(), accepted);
    assert_eq!(fs::read(&path).unwrap(), winner);
}

#[test]
fn simultaneous_saves_cannot_both_mark_different_edits_clean() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("shared.srt");
    fs::write(&path, ORIGINAL).unwrap();
    let a = edited(&path, "Writer A");
    let b = edited(&path, "Writer B");
    let barrier = std::sync::Barrier::new(2);
    let results = std::thread::scope(|scope| {
        let save = |mut store: Store| {
            barrier.wait();
            let result = store.apply(Action::Save { path: None }, 1, 1);
            (store, result)
        };
        let a = scope.spawn(move || save(a));
        let b = scope.spawn(move || save(b));
        [a.join().unwrap(), b.join().unwrap()]
    });
    assert_eq!(
        results.iter().filter(|(_, result)| result.is_ok()).count(),
        1
    );
    let disk = fs::read(path).unwrap();
    for (store, result) in results {
        assert_eq!(store.snapshot().dirty, result.is_err());
        if result.is_ok() {
            assert_eq!(store.loaded.unwrap().saved_bytes, disk);
        }
    }
}

#[cfg(unix)]
#[test]
fn saving_through_a_symlink_uses_the_target_lease_and_preserves_the_link() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("shared.srt");
    let alias = directory.path().join("alias.srt");
    fs::write(&path, ORIGINAL).unwrap();
    std::os::unix::fs::symlink(&path, &alias).unwrap();
    let mut store = edited(&alias, "Alias edit");
    let lease = crate::save_lease::acquire(&path).unwrap();
    assert!(
        store
            .apply(
                Action::Save {
                    path: Some(alias.to_string_lossy().into_owned())
                },
                1,
                1
            )
            .unwrap_err()
            .contains("another instance")
    );
    assert!(alias.is_symlink());
    drop(lease);
    store.apply(Action::Save { path: None }, 1, 1).unwrap();
    assert!(alias.is_symlink());
    assert!(fs::read_to_string(path).unwrap().contains("Alias edit"));
}
