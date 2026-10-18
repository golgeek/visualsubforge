use super::*;
use media_analysis::alignment::{Alignment, Segment};
use std::{sync::atomic::Ordering, time::Duration};
use vsf_application::resync::{Budget, Cancellation};

const ORIGINAL: &str = "7\n00:00:01,000 --> 00:00:02,000\n<i>Original</i>\n\n9\n00:00:02,120 --> 00:00:03,000\n世界\n\n";

fn opened(path: &Path) -> Documents {
    opened_text(path, ORIGINAL)
}
fn opened_text(path: &Path, original: &str) -> Documents {
    fs::write(path, original).unwrap();
    let docs = Documents::default();
    docs.apply(
        Action::Open {
            path: path.to_string_lossy().into_owned(),
            discard: false,
        },
        0,
        0,
    )
    .unwrap();
    docs
}

fn proposal(capture: &Capture, offset_us: u64, supported: bool) -> Proposal {
    Proposal::build(
        capture.snapshot.clone(),
        Alignment {
            segments: if supported {
                vec![Segment {
                    source_start_us: 0,
                    source_end_us: 10_000_000,
                    target_start_us: offset_us,
                    target_end_us: 10_000_000 + offset_us,
                    anchors: 3,
                    max_residual_us: 0.,
                }]
            } else {
                vec![]
            },
            distinctive_matches: 3,
            ambiguous_source_anchors: 0,
            reordered: false,
        },
        &Budget::new(&Cancellation::default(), Duration::from_secs(5)),
    )
    .unwrap()
}

fn sources() -> [(&'static str, &'static str); 4] {
    [
        ("srt", ORIGINAL),
        ("ass", super::super::ass_tests::ASS),
        ("ssa", super::super::ass_tests::SSA),
        (
            "ass",
            include_str!("../../../../tests/fixtures/subtitles/embedded-font.ass"),
        ),
    ]
}

fn save(docs: &Documents, path: Option<&Path>, revision: u64) -> Result<Snapshot> {
    docs.apply(
        Action::Save {
            path: path.map(|p| p.to_string_lossy().into_owned()),
        },
        1,
        revision,
    )
}

#[test]
fn acceptance_is_one_undo_and_protects_source_through_undo_failed_save_and_save_as() {
    for (extension, original) in sources() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join(format!("source.{extension}"));
        let docs = opened_text(&source, original);
        let initial = docs.apply(Action::Status, 0, 0).unwrap();
        let capture = docs.capture_resync(1, 0).unwrap();
        let proposal = proposal(&capture, 500_000, true);
        let accepted = docs.accept_resync(&capture, &proposal).unwrap();
        assert!(accepted.dirty && accepted.requires_save_as && accepted.can_undo);
        assert_eq!(accepted.revision, 1);
        assert_eq!(
            (accepted.cues[0].start_ms, accepted.cues[1].end_ms),
            (initial.cues[0].start_ms + 500, initial.cues[1].end_ms + 500)
        );
        assert_eq!(fs::read_to_string(&source).unwrap(), original);
        assert!(save(&docs, None, 1).is_err());
        assert!(save(&docs, Some(&source), 1).is_err());
        assert!(save(&docs, Some(dir.path()), 1).is_err());
        let undone = docs.apply(Action::Undo, 1, 1).unwrap();
        assert!(undone.dirty && undone.requires_save_as && undone.can_redo);
        assert!(!undone.can_undo);
        assert_eq!(
            (undone.cues[0].start_ms, undone.cues[1].end_ms),
            (initial.cues[0].start_ms, initial.cues[1].end_ms)
        );
        assert!(save(&docs, Some(&source), 2).is_err());
        let restored = docs.0.lock().unwrap();
        let loaded = restored.loaded.as_ref().unwrap();
        assert_eq!(
            loaded.source().serialize(loaded.editor.document()).unwrap(),
            original
        );
        drop(restored);
        let redone = docs.apply(Action::Redo, 1, 2).unwrap();
        assert_eq!(
            (redone.cues[0].start_ms, redone.cues[1].end_ms),
            (initial.cues[0].start_ms + 500, initial.cues[1].end_ms + 500)
        );
        let copy = dir.path().join(format!("destination.{extension}"));
        let saved = save(&docs, Some(&copy), 3).unwrap();
        assert!(!saved.dirty && !saved.requires_save_as);
        assert_eq!(
            fs::read_to_string(&copy).unwrap(),
            proposal.subtitles().unwrap()
        );
        assert_eq!(fs::read_to_string(&source).unwrap(), original);
        assert!(save(&docs, None, 3).is_ok());
    }
}

#[test]
fn stale_or_incomplete_proposals_preserve_content_history_and_save_policy() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.srt");
    let docs = opened(&source);
    assert!(docs.capture_resync(0, 0).is_err());
    assert!(docs.capture_resync(1, 1).is_err());
    let capture = docs.capture_resync(1, 0).unwrap();
    let incomplete = proposal(&capture, 0, false);
    let before = serde_json::to_value(docs.apply(Action::Status, 0, 0).unwrap()).unwrap();
    assert!(docs.accept_resync(&capture, &incomplete).is_err());
    assert_eq!(
        serde_json::to_value(docs.apply(Action::Status, 0, 0).unwrap()).unwrap(),
        before
    );
    let complete = proposal(&capture, 500_000, true);
    docs.apply(
        Action::Edit {
            cue_id: 1,
            start_ms: 1000,
            end_ms: 2000,
            text: "Changed".into(),
        },
        1,
        0,
    )
    .unwrap();
    assert!(docs.accept_resync(&capture, &complete).is_err());
    docs.apply(Action::Undo, 1, 1).unwrap();
    assert!(docs.accept_resync(&capture, &complete).is_err()); // same bytes, new revision
    let before = serde_json::to_value(docs.apply(Action::Status, 0, 0).unwrap()).unwrap();
    let copy = dir.path().join("copy.srt");
    let capture = docs.capture_resync(1, 2).unwrap();
    let complete = proposal(&capture, 500_000, true);
    save(&docs, Some(&copy), 2).unwrap();
    assert!(docs.accept_resync(&capture, &complete).is_err()); // Save As has no editor revision
    let snapshot = docs.apply(Action::Status, 0, 0).unwrap();
    assert_eq!(
        serde_json::to_value(&snapshot).unwrap()["cues"],
        before["cues"]
    );
    assert!(snapshot.can_redo && !snapshot.requires_save_as);
    assert_eq!(fs::read_to_string(source).unwrap(), ORIGINAL);
}

#[test]
fn style_assignment_and_undo_invalidate_styled_resync_capture() {
    for (extension, original) in sources().into_iter().filter(|(ext, _)| *ext != "srt") {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join(format!("source.{extension}"));
        let docs = opened_text(&source, original);
        let capture = docs.capture_resync(1, 0).unwrap();
        let proposed = proposal(&capture, 500_000, true);
        let cue = &capture.snapshot.document().cues()[0];
        docs.apply(
            Action::EditStyled {
                cue_id: cue.id().0,
                start_ms: cue.start().get(),
                end_ms: cue.end().get(),
                text: cue.text().into(),
                style: "Default".into(),
            },
            1,
            0,
        )
        .unwrap();
        assert!(docs.accept_resync(&capture, &proposed).is_err());
        let undone = docs.apply(Action::Undo, 1, 1).unwrap();
        assert!(undone.can_redo && !undone.requires_save_as);
        assert!(docs.accept_resync(&capture, &proposed).is_err());
        assert_eq!(fs::read_to_string(&source).unwrap(), original);
        let restored = docs.capture_resync(1, 2).unwrap();
        assert_eq!(restored.snapshot.sha256(), capture.snapshot.sha256());
        let current = proposal(&restored, 500_000, true);
        assert!(docs.accept_resync(&restored, &current).is_ok());
    }
}

#[test]
fn unchanged_proposal_still_changes_checkpoint_identity_and_blocks_stale_project_references() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.srt");
    let docs = opened(&source);
    // Start dirty, so adding protection must be represented separately from the
    // checkpoint's dirty bit and the editor revision.
    docs.apply(
        Action::Edit {
            cue_id: 1,
            start_ms: 1000,
            end_ms: 2000,
            text: "Changed".into(),
        },
        1,
        0,
    )
    .unwrap();
    let key = docs.0.lock().unwrap().checkpoint_key();
    let capture = docs.capture_resync(1, 1).unwrap();
    let unchanged = proposal(&capture, 0, true);
    let accepted = docs.accept_resync(&capture, &unchanged).unwrap();
    assert_eq!(accepted.revision, 1);
    assert!(accepted.requires_save_as);
    assert!(docs.0.lock().unwrap().checkpoint_key() != key);
    assert!(
        docs.with_project_subtitle::<()>(Some(&source), |_, _| panic!("Must not publish"))
            .is_err()
    );
    let copy = dir.path().join("copy.srt");
    save(&docs, Some(&copy), 1).unwrap();
    assert!(
        docs.with_project_subtitle::<()>(Some(&source), |_, _| panic!(
            "Stale source must not publish"
        ))
        .is_err()
    );
    let copy = copy.canonicalize().unwrap();
    assert_eq!(
        docs.with_project_subtitle(Some(&copy), |_, _| Ok(42))
            .unwrap(),
        42
    );
}

#[test]
fn actual_checkpoint_and_recovery_keep_retimed_content_and_source_protection() {
    for (extension, original) in sources() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join(format!("source.{extension}"));
        let docs = opened_text(&source, original);
        let capture = docs.capture_resync(1, 0).unwrap();
        let proposed = proposal(&capture, 500_000, true);
        docs.accept_resync(&capture, &proposed).unwrap();
        let root = dir.path().join("recovery");
        docs.start_recovery(root.clone()).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            let published = fs::read_dir(&root).is_ok_and(|entries| {
                entries
                    .flatten()
                    .any(|entry| entry.path().join("snapshot.json").is_file())
            });
            if published {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "Recovery did not publish"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        // Model abrupt app loss without the normal document-close/discard path.
        docs.1.stop.store(true, Ordering::Release);
        let worker = docs.1.thread.lock().unwrap().take().unwrap();
        worker.thread().unpark();
        worker.join().unwrap();
        let entries = crate::recovery_store::list(&root).unwrap();
        assert_eq!(entries.len(), 1);
        let record = crate::recovery_store::recover(&root, &entries[0].id).unwrap();
        assert!(record.resynced);
        let recovered = Documents::default();
        recovered.0.lock().unwrap().recovery_root = Some(root);
        let snapshot = recovered
            .apply(
                Action::Recover {
                    id: entries[0].id.clone(),
                    discard: false,
                },
                0,
                0,
            )
            .unwrap();
        assert!(snapshot.requires_save_as && snapshot.dirty);
        assert_eq!(snapshot.cues[0].start_ms, 1500);
        assert!(save(&recovered, Some(&source), 0).is_err());
        save(
            &recovered,
            Some(&dir.path().join(format!("recovered.{extension}"))),
            0,
        )
        .unwrap();
        assert_eq!(
            fs::read_to_string(dir.path().join(format!("recovered.{extension}"))).unwrap(),
            proposed.subtitles().unwrap()
        );
        assert_eq!(fs::read_to_string(source).unwrap(), original);
    }
}

#[cfg(unix)]
#[test]
fn save_as_cannot_bypass_original_protection_through_a_symlink() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.srt");
    let docs = opened(&source);
    let capture = docs.capture_resync(1, 0).unwrap();
    docs.accept_resync(&capture, &proposal(&capture, 500_000, true))
        .unwrap();
    let alias = dir.path().join("alias.srt");
    std::os::unix::fs::symlink(&source, &alias).unwrap();
    assert!(save(&docs, Some(&alias), 1).is_err());
    assert!(alias.is_symlink());
    assert_eq!(fs::read_to_string(source).unwrap(), ORIGINAL);
}
