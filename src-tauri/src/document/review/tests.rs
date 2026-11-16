use super::*;
const ORIGINAL: &str = "1\n00:00:01,000 --> 00:00:03,000\nOriginal\n\n";
fn snapshot(d: &Documents) -> Snapshot {
    d.apply(super::super::Action::Status, 0, 0).unwrap()
}
fn action(d: &Documents, a: super::super::Action) -> Snapshot {
    let s = snapshot(d);
    d.apply(a, s.generation, s.revision).unwrap()
}
fn key(d: &Documents) -> ReviewKey {
    d.review_action(ReviewAction::Status).unwrap().key
}
fn fixture() -> (tempfile::TempDir, Documents, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let original = dir.path().join("original.srt");
    let working = dir.path().join("working.srt");
    fs::write(&original, ORIGINAL).unwrap();
    fs::write(&working, ORIGINAL.replace("Original", "Revised")).unwrap();
    let d = Documents::default();
    action(
        &d,
        super::super::Action::Open {
            path: working.to_string_lossy().into(),
            discard: false,
        },
    );
    (dir, d, original, working)
}
fn attach(d: &Documents, p: &Path) {
    d.review_action(ReviewAction::Attach {
        key: key(d),
        path: p.to_string_lossy().into(),
    })
    .unwrap();
}
fn ready(d: &Documents) -> ReviewReply {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let r = d
            .review_action(ReviewAction::Query {
                key: key(d),
                offset: 0,
                limit: 100,
                filter: None,
            })
            .unwrap();
        if !r.working {
            assert!(r.error.is_none(), "{:?}", r.error);
            return r;
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}
#[test]
fn independent_reference_context_worker_protection_and_history() {
    let (_dir, d, original, _) = fixture();
    let before = snapshot(&d);
    attach(&d, &original);
    let after = snapshot(&d);
    assert_eq!(after.revision, before.revision);
    assert_eq!(after.dirty, before.dirty);
    assert_eq!(after.can_undo, before.can_undo);
    assert_eq!(ready(&d).statistics.unwrap().changes, 1);
    let protected = d.apply(
        super::super::Action::Save {
            path: Some(original.to_string_lossy().into()),
        },
        after.generation,
        after.revision,
    );
    assert!(protected.is_err());
    assert_eq!(fs::read_to_string(&original).unwrap(), ORIGINAL);
    let stale = key(&d);
    d.review_action(ReviewAction::Options {
        key: stale,
        options: engine::Options {
            ignore_tags: true,
            ..engine::Options::default()
        },
    })
    .unwrap();
    assert!(
        d.review_action(ReviewAction::Detach { key: stale })
            .is_err()
    );
    d.review_action(ReviewAction::Detach { key: key(&d) })
        .unwrap();
    assert_eq!(snapshot(&d).revision, before.revision);
}
#[test]
fn start_copy_and_recovery_keep_original_without_disk() {
    let (dir, d, original, working) = fixture();
    attach(&d, &original);
    let video = dir.path().join("source.mkv");
    fs::write(&video, b"synthetic").unwrap();
    d.0.lock().unwrap().loaded_mut().unwrap().extracted_from = Some(extraction::ExtractedSource {
        media_path: video.clone(),
        stream_index: 1,
        codec: "subrip".into(),
        language: None,
        title: None,
        suggested_name: "source.srt".into(),
    });
    let copy = dir.path().join("working.reviewed.srt");
    d.review_action(ReviewAction::Start {
        key: key(&d),
        path: copy.to_string_lossy().into(),
        discard: false,
    })
    .unwrap();
    assert_eq!(fs::read(&copy).unwrap(), fs::read(&working).unwrap());
    assert_eq!(
        snapshot(&d).path,
        Some(copy.canonicalize().unwrap().to_string_lossy().into())
    );
    assert!(snapshot(&d).extracted_from.is_none());
    assert!(
        d.0.lock()
            .unwrap()
            .loaded_mut()
            .unwrap()
            .extracted_from
            .is_some()
    );
    for protected in [&original, &video] {
        let current = snapshot(&d);
        assert!(
            d.apply(
                super::super::Action::Save {
                    path: Some(protected.to_string_lossy().into())
                },
                current.generation,
                current.revision
            )
            .is_err()
        );
    }
    // Reattaching a base must not erase the source of the reviewed copy.
    attach(&d, &original);
    let current = snapshot(&d);
    assert!(
        d.apply(
            super::super::Action::Save {
                path: Some(working.to_string_lossy().into())
            },
            current.generation,
            current.revision
        )
        .is_err()
    );
    let store = d.0.lock().unwrap();
    let loaded = store.loaded.as_ref().unwrap();
    let stored = loaded.review.stored().unwrap().unwrap();
    drop(store);
    fs::remove_file(&original).unwrap();
    let restored = stored.restore().unwrap();
    assert_eq!(
        restored
            .reference
            .reference
            .unwrap()
            .reference
            .original_bytes(),
        ORIGINAL.as_bytes()
    );
}
#[test]
fn review_project_description_checks_expected_fingerprint() {
    let (dir, d, original, _) = fixture();
    attach(&d, &original);
    let description =
        d.0.lock()
            .unwrap()
            .loaded
            .as_ref()
            .unwrap()
            .review
            .project_description(dir.path())
            .unwrap()
            .unwrap();
    let bytes = serde_json::to_vec(&description).unwrap();
    let description: ReviewProjectContext = serde_json::from_slice(&bytes).unwrap();
    assert!(
        description
            .load(dir.path(), &translation::ReferenceOverride::default())
            .is_ok()
    );
    fs::write(&original, ORIGINAL.replace("Original", "Changed")).unwrap();
    assert!(
        description
            .load(dir.path(), &translation::ReferenceOverride::default())
            .is_err()
    );
    fs::remove_file(&original).unwrap();
    assert!(
        description
            .load(dir.path(), &translation::ReferenceOverride::default())
            .is_err()
    );
}
