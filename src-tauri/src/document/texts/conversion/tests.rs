use super::*;
use engine::{ConversionFormat, ConversionOptions, ConversionPrecision};
use std::{fs, sync::mpsc};

fn fixture(extension: &str) -> (tempfile::TempDir, Documents, String) {
    let root = tempfile::tempdir().unwrap();
    let original = match extension {
        "ass" => crate::document::ass_tests::ASS,
        "ssa" => crate::document::ass_tests::SSA,
        _ => "1\n00:00:01,000 --> 00:00:02,000\n{\\an8}<i>世界</i>\n\n",
    }
    .to_owned();
    let path = root.path().join(format!("source.{extension}"));
    fs::write(&path, &original).unwrap();
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
    (root, docs, original)
}
fn snapshot(docs: &Documents) -> Snapshot {
    docs.apply(Action::Status, 0, 0).unwrap()
}
fn action(docs: &Documents, action: Action) -> Snapshot {
    let before = snapshot(docs);
    docs.apply(action, before.generation, before.revision)
        .unwrap()
}
fn start(docs: &Documents, target: ConversionFormat) -> u64 {
    let before = snapshot(docs);
    docs.conversion_start(
        before.generation,
        before.revision,
        ConversionOptions {
            target,
            precision: ConversionPrecision::Exact,
        },
    )
    .unwrap()
}
fn ready(docs: &Documents, id: u64) -> ConversionReview {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let view = docs.conversion_view(id).unwrap();
        if !matches!(
            view.phase,
            ConversionPhase::Preparing | ConversionPhase::Publishing | ConversionPhase::Cancelling
        ) && docs
            .0
            .lock()
            .unwrap()
            .texts
            .job
            .as_ref()
            .is_some_and(|job| job.worker.is_none())
        {
            // Stale blocks publication immediately, but cleanup can still be
            // draining. Filesystem assertions below require the worker reaped.
            return view;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn expected(docs: &Documents, id: u64) -> String {
    let store = docs.0.lock().unwrap();
    let job = store.texts.job.as_ref().unwrap();
    assert_eq!(job.id, id);
    let Some(Prepared::Conversion(prepared)) = &job.proposal else {
        panic!("Missing proposal")
    };
    prepared.proposal.output().unwrap().to_owned()
}
fn recovery_image(docs: &Documents) -> (CheckpointKey, Vec<u8>, String, bool) {
    let store = docs.0.lock().unwrap();
    let loaded = store.loaded.as_ref().unwrap();
    (
        store.checkpoint_key(),
        loaded.saved_bytes.clone(),
        loaded.source().serialize(loaded.editor.document()).unwrap(),
        loaded.recovered,
    )
}
fn blocked_publication(
    docs: &Documents,
    id: u64,
    path: &Path,
) -> (mpsc::Receiver<()>, mpsc::Sender<()>) {
    let (reached, wait) = mpsc::channel();
    let (release, resume) = mpsc::channel();
    docs.publish_conversion(id, path.to_str().unwrap(), move || {
        reached.send(()).unwrap();
        resume
            .recv_timeout(Duration::from_secs(10))
            .map_err(|e| e.to_string())
    })
    .unwrap();
    (wait, release)
}

#[test]
fn newly_attached_translation_reference_is_protected_at_final_export_commit() {
    let (root, docs, original) = fixture("srt");
    let id = start(&docs, ConversionFormat::Ass);
    ready(&docs, id);
    let output = root.path().join("later-reference.ass");
    let (wait, release) = blocked_publication(&docs, id, &output);
    wait.recv_timeout(Duration::from_secs(10)).unwrap();
    fs::write(&output, crate::document::ass_tests::ASS).unwrap();
    let request = |value| {
        serde_json::to_value(
            docs.translation_action(serde_json::from_value(value).unwrap())
                .unwrap(),
        )
        .unwrap()
    };
    let current = request(serde_json::json!({"kind":"status"}));
    let load =
        request(serde_json::json!({"kind":"load","key":current["key"],"path":output,"copy":false}));
    let reference_id = load["job"]["id"].as_u64().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let view = request(serde_json::json!({"kind":"view","id":reference_id}));
        if view["job"]["phase"] == "ready" {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    request(serde_json::json!({"kind":"accept","id":reference_id,"discard":false}));
    // Delete the reference after capture. The destination is absent again, but
    // its accepted provenance must still block the already-prepared conversion.
    fs::remove_file(&output).unwrap();
    release.send(()).unwrap();
    let failed = ready(&docs, id);
    assert!(matches!(failed.phase, ConversionPhase::Failed));
    assert!(failed.error.unwrap().contains("preserve the original"));
    assert!(!output.exists());
    assert_eq!(
        fs::read_to_string(root.path().join("source.srt")).unwrap(),
        original
    );
    assert!(!snapshot(&docs).dirty);
    docs.shutdown_quality().unwrap();
}

#[test]
fn reviewed_export_uses_exact_bytes_and_preserves_dirty_history_path_and_recovery() {
    for extension in ["srt", "ass", "ssa"] {
        let (root, docs, original) = fixture(extension);
        let first = &snapshot(&docs).cues[0];
        action(
            &docs,
            Action::Edit {
                cue_id: first.id,
                start_ms: first.start_ms,
                end_ms: first.end_ms,
                text: "accepted change".into(),
            },
        );
        action(
            &docs,
            Action::Edit {
                cue_id: first.id,
                start_ms: first.start_ms,
                end_ms: first.end_ms,
                text: "second change".into(),
            },
        );
        // Retain dirty accepted state and a redo branch while exporting.
        action(&docs, Action::Undo);
        let before = serde_json::to_value(snapshot(&docs)).unwrap();
        let recovery = recovery_image(&docs);
        for target in [
            ConversionFormat::Srt,
            ConversionFormat::Ass,
            ConversionFormat::Ssa,
        ] {
            if target.format().extension() == extension {
                continue;
            }
            let id = start(&docs, target);
            assert_eq!(ready(&docs, id).phase, ConversionPhase::Reviewing);
            let bytes = expected(&docs, id);
            let path = root
                .path()
                .join(format!("converted.{}", target.format().extension()));
            docs.conversion_publish(id, path.to_str().unwrap()).unwrap();
            let view = ready(&docs, id);
            assert_eq!(view.phase, ConversionPhase::Published, "{:?}", view.error);
            assert_eq!(fs::read_to_string(&path).unwrap(), bytes);
            let reopened = vsf_application::subtitle_file::parse_document(
                bytes.as_bytes(),
                Some(target.format().extension()),
            )
            .unwrap();
            assert_eq!(reopened.document.cues().len(), snapshot(&docs).cues.len());
            assert!(
                docs.conversion_publish(id, root.path().join("again.ass").to_str().unwrap())
                    .is_err()
            );
            docs.conversion_cancel(id).unwrap();
            assert_eq!(ready(&docs, id).phase, ConversionPhase::Published);
            assert_eq!(serde_json::to_value(snapshot(&docs)).unwrap(), before);
        }
        assert_eq!(
            fs::read_to_string(root.path().join(format!("source.{extension}"))).unwrap(),
            original
        );
        assert!(recovery == recovery_image(&docs));
        let record = crate::recovery_store::Record::new_document(
            subtitle_formats::Format::from_extension(extension).unwrap(),
            recovery
                .0
                .path
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned()),
            String::from_utf8(recovery.1).unwrap(),
            recovery.2.clone(),
            recovery.0.generation,
            recovery.0.revision,
        )
        .unwrap();
        let recovery_root = root.path().join("recovery");
        let session = crate::recovery_store::Session::create(&recovery_root).unwrap();
        session.prepare(&record).unwrap().commit().unwrap();
        drop(session);
        let entries = crate::recovery_store::list(&recovery_root).unwrap();
        assert_eq!(
            crate::recovery_store::recover(&recovery_root, &entries[0].id)
                .unwrap()
                .record
                .accepted_srt,
            recovery.2
        );
        assert!(action(&docs, Action::Redo).dirty);
    }
}

#[test]
fn failures_preserve_review_for_retry_and_competing_outputs_are_never_replaced() {
    let (root, docs, original) = fixture("srt");
    let id = start(&docs, ConversionFormat::Ass);
    ready(&docs, id);
    let bytes = expected(&docs, id);
    for path in [
        root.path().join("missing/out.ass"),
        root.path().join("bad.ssa"),
        root.path().join("source.srt"),
    ] {
        docs.conversion_publish(id, path.to_str().unwrap()).unwrap();
        let view = ready(&docs, id);
        assert_eq!(view.phase, ConversionPhase::Failed);
        assert!(view.report.is_some() && view.error.is_some());
        assert_eq!(expected(&docs, id), bytes);
    }
    let path = root.path().join("competitor.ass");
    let (wait, release) = blocked_publication(&docs, id, &path);
    wait.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(
        docs.conversion_view(id).unwrap().phase,
        ConversionPhase::Publishing
    );
    assert!(docs.conversion_publish(id, path.to_str().unwrap()).is_err());
    assert!(
        docs.conversion_start(
            snapshot(&docs).generation,
            snapshot(&docs).revision,
            ConversionOptions {
                target: ConversionFormat::Ssa,
                precision: ConversionPrecision::Exact
            }
        )
        .is_err()
    );
    fs::write(&path, b"competitor").unwrap();
    release.send(()).unwrap();
    assert_eq!(ready(&docs, id).phase, ConversionPhase::Failed);
    assert_eq!(fs::read(&path).unwrap(), b"competitor");
    let retry = root.path().join("retry.ass");
    docs.conversion_publish(id, retry.to_str().unwrap())
        .unwrap();
    assert_eq!(ready(&docs, id).phase, ConversionPhase::Published);
    assert_eq!(fs::read_to_string(retry).unwrap(), bytes);
    assert_eq!(
        fs::read_to_string(root.path().join("source.srt")).unwrap(),
        original
    );
}

#[test]
fn cancellation_document_policy_path_changes_and_shutdown_prevent_late_publication() {
    for change in ["cancel", "edit", "policy", "path", "close", "shutdown"] {
        let (root, docs, original) = fixture("srt");
        let id = start(&docs, ConversionFormat::Ass);
        ready(&docs, id);
        let path = root.path().join("late.ass");
        let (wait, release) = blocked_publication(&docs, id, &path);
        wait.recv_timeout(Duration::from_secs(5)).unwrap();
        let mut shutdown = None;
        match change {
            "cancel" => docs.conversion_cancel(id).unwrap(),
            "edit" => {
                let first = &snapshot(&docs).cues[0];
                action(
                    &docs,
                    Action::Edit {
                        cue_id: first.id,
                        start_ms: first.start_ms,
                        end_ms: first.end_ms,
                        text: "new text".into(),
                    },
                );
            }
            "policy" => {
                let context = docs.quality_context().unwrap();
                let mut timing = context.timing;
                timing.cue_gap_ms = 190;
                docs.apply_quality_defaults(
                    context.epoch,
                    crate::quality_profile::Policy::legacy(timing, "conversion-test").unwrap(),
                    || Ok(()),
                )
                .unwrap();
            }
            "path" => {
                action(
                    &docs,
                    Action::Save {
                        path: Some(root.path().join("copy.srt").to_string_lossy().into_owned()),
                    },
                );
            }
            "close" => {
                action(&docs, Action::Close { discard: true });
            }
            _ => {
                let clone = docs.clone();
                shutdown = Some(std::thread::spawn(move || clone.shutdown_texts().unwrap()));
                let deadline = Instant::now() + Duration::from_secs(5);
                while !docs.0.lock().unwrap().texts.closed {
                    assert!(Instant::now() < deadline);
                    std::thread::yield_now();
                }
            }
        }
        if change != "shutdown" {
            let held = docs.conversion_view(id).unwrap();
            assert!(matches!(
                held.phase,
                ConversionPhase::Stale | ConversionPhase::Cancelling
            ));
            assert!(
                docs.0
                    .lock()
                    .unwrap()
                    .texts
                    .job
                    .as_ref()
                    .unwrap()
                    .worker
                    .is_some()
            );
        }
        release.send(()).unwrap();
        if let Some(handle) = shutdown {
            handle.join().unwrap();
        } else {
            let view = ready(&docs, id);
            assert!(matches!(
                view.phase,
                ConversionPhase::Cancelled | ConversionPhase::Stale | ConversionPhase::Failed
            ));
        }
        assert!(!path.exists(), "{change}");
        assert_eq!(
            fs::read_to_string(root.path().join("source.srt")).unwrap(),
            original
        );
        assert!(
            !fs::read_dir(root.path()).unwrap().any(|entry| {
                entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".tmp")
            }),
            "Temporary remained after {change} worker completion"
        );
    }
}

#[test]
fn blocked_and_wrong_family_reviews_cannot_publish_or_cancel_other_work() {
    let (root, docs, _) = fixture("srt");
    let first = &snapshot(&docs).cues[0];
    action(
        &docs,
        Action::Edit {
            cue_id: first.id,
            start_ms: 1001,
            end_ms: 2000,
            text: first.text.clone(),
        },
    );
    let id = start(&docs, ConversionFormat::Ass);
    assert!(ready(&docs, id).report.unwrap().blocked());
    assert!(
        docs.conversion_publish(id, root.path().join("blocked.ass").to_str().unwrap())
            .is_err()
    );
    assert!(docs.font_view(id).is_err());
    docs.font_cancel(id).unwrap();
    assert_eq!(ready(&docs, id).phase, ConversionPhase::Reviewing);
    docs.conversion_cancel(id).unwrap();
    assert_eq!(ready(&docs, id).phase, ConversionPhase::Cancelled);
    let (root2, other, _) = fixture("ass");
    let snap = snapshot(&other);
    let font = other
        .font_start(
            snap.generation,
            snap.revision,
            crate::font_review::FontOperation::Inspect,
        )
        .unwrap();
    assert!(other.conversion_view(font).is_err());
    other.conversion_cancel(font).unwrap();
    assert!(
        other
            .conversion_publish(font, root2.path().join("wrong.srt").to_str().unwrap())
            .is_err()
    );
    other.shutdown_texts().unwrap();
}

#[test]
fn dropping_the_last_document_owner_cancels_and_reaps_an_export() {
    let (root, docs, _) = fixture("srt");
    let id = start(&docs, ConversionFormat::Ass);
    ready(&docs, id);
    let token = docs
        .0
        .lock()
        .unwrap()
        .texts
        .job
        .as_ref()
        .unwrap()
        .cancel
        .clone();
    let weak = Arc::downgrade(&docs.0);
    let path = root.path().join("abandoned.ass");
    let (wait, release) = blocked_publication(&docs, id, &path);
    wait.recv_timeout(Duration::from_secs(5)).unwrap();
    let dropped = std::thread::spawn(move || drop(docs));
    let deadline = Instant::now() + Duration::from_secs(5);
    while !token.load(Ordering::Relaxed) {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    release.send(()).unwrap();
    dropped.join().unwrap();
    assert!(weak.upgrade().is_none());
    assert!(!path.exists());
}

#[test]
fn source_disk_changes_never_replace_captured_output_and_writer_leases_allow_retry() {
    let (root, docs, _) = fixture("srt");
    let id = start(&docs, ConversionFormat::Ass);
    ready(&docs, id);
    let bytes = expected(&docs, id);
    fs::write(root.path().join("source.srt"), b"external change").unwrap();
    let path = root.path().join("leased.ass");
    let lease = vsf_application::save_lease::acquire(&path).unwrap();
    docs.conversion_publish(id, path.to_str().unwrap()).unwrap();
    let view = ready(&docs, id);
    assert_eq!(view.phase, ConversionPhase::Failed);
    assert!(view.error.unwrap().contains("another instance"));
    assert!(!path.exists());
    drop(lease);
    docs.conversion_publish(id, path.to_str().unwrap()).unwrap();
    assert_eq!(ready(&docs, id).phase, ConversionPhase::Published);
    assert_eq!(fs::read_to_string(path).unwrap(), bytes);
    assert_eq!(
        fs::read(root.path().join("source.srt")).unwrap(),
        b"external change"
    );

    // Recovery provenance can name an absent original independently of the
    // accepted format. Protect that path without relying on extension mismatch.
    let original = root.path().join("deleted.ass");
    docs.0.lock().unwrap().loaded.as_mut().unwrap().path = Some(original.clone());
    let id = start(&docs, ConversionFormat::Ass);
    ready(&docs, id);
    docs.conversion_publish(id, original.to_str().unwrap())
        .unwrap();
    let view = ready(&docs, id);
    assert_eq!(view.phase, ConversionPhase::Failed);
    assert!(view.error.unwrap().contains("preserve the original"));
    assert!(!original.exists());
}

pub(crate) fn ipc_samples() -> Vec<(&'static str, &'static str, serde_json::Value)> {
    let (_root, docs, _) = fixture("srt");
    let id = start(&docs, ConversionFormat::Ass);
    let reviewing = serde_json::to_value(ready(&docs, id)).unwrap();
    let first = &snapshot(&docs).cues[0];
    action(
        &docs,
        Action::Edit {
            cue_id: first.id,
            start_ms: 1001,
            end_ms: 2000,
            text: first.text.clone(),
        },
    );
    let id = start(&docs, ConversionFormat::Ssa);
    let blocked = serde_json::to_value(ready(&docs, id)).unwrap();
    vec![
        ("conversionReview", "ConversionReview", reviewing),
        ("conversionBlocked", "ConversionReview", blocked),
    ]
}
