use super::*;
use crate::document::Action as DocumentAction;
use std::sync::mpsc;
const SRT: &str = "1\n00:00:01,000 --> 00:00:03,000\n{\\an8}<i>Été 世界</i>\n\n";

fn key(docs: &Documents) -> Key {
    docs.translation_action(Action::Status).unwrap().key
}
fn snapshot(docs: &Documents) -> Snapshot {
    docs.apply(DocumentAction::Status, 0, 0).unwrap()
}
fn doc_action(docs: &Documents, action: DocumentAction) -> Snapshot {
    let before = snapshot(docs);
    docs.apply(action, before.generation, before.revision)
        .unwrap()
}
fn fixture() -> (tempfile::TempDir, Documents, PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("target.srt");
    fs::write(&target, SRT).unwrap();
    let reference = root.path().join("reference.srt");
    fs::write(&reference, SRT).unwrap();
    let docs = Documents::default();
    doc_action(
        &docs,
        DocumentAction::Open {
            path: target.to_string_lossy().into(),
            discard: false,
        },
    );
    (root, docs, reference)
}
fn ready(docs: &Documents, id: u64) -> Reply {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let reply = docs
            .translation_action(Action::View {
                id: id.try_into().unwrap(),
            })
            .unwrap();
        if docs
            .0
            .lock()
            .unwrap()
            .translation
            .job
            .as_ref()
            .unwrap()
            .worker
            .is_none()
        {
            return reply;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn load(docs: &Documents, path: &Path, copy: bool) -> u64 {
    let reply = docs
        .translation_action(Action::Load {
            key: key(docs),
            path: path.to_string_lossy().into(),
            copy,
        })
        .unwrap();
    let id = reply.job.unwrap().id.get();
    assert!(matches!(ready(docs, id).job.unwrap().phase, Phase::Ready));
    id
}
fn attach(docs: &Documents, path: &Path) {
    let id = load(docs, path, false);
    docs.accept_reference(id, false).unwrap();
}
fn text(docs: &Documents, text: &str) {
    let cue = &snapshot(docs).cues[0];
    doc_action(
        docs,
        DocumentAction::Edit {
            cue_id: cue.id,
            start_ms: cue.start_ms,
            end_ms: cue.end_ms,
            text: text.into(),
        },
    );
}

#[test]
fn attach_offset_query_and_detach_preserve_target_history_and_reference_bytes() {
    let (_root, docs, reference) = fixture();
    text(&docs, "Translated");
    doc_action(&docs, DocumentAction::Undo);
    let before = snapshot(&docs);
    attach(&docs, &reference);
    let attached = snapshot(&docs);
    assert_eq!(
        (
            attached.revision,
            attached.can_undo,
            attached.can_redo,
            attached.dirty
        ),
        (
            before.revision,
            before.can_undo,
            before.can_redo,
            before.dirty
        )
    );
    assert!(attached.translation.unwrap().dirty);
    let old_key = key(&docs);
    docs.translation_action(Action::SetOffset {
        key: old_key,
        offset_ms: (-1500).try_into().unwrap(),
    })
    .unwrap();
    assert!(
        docs.translation_action(Action::Detach { key: old_key })
            .is_err()
    );
    let reply = docs
        .translation_action(Action::Query {
            key: key(&docs),
            target_cue_id: Some(1.try_into().unwrap()),
            offset: 0,
            limit: 200,
            selected: Some(0),
        })
        .unwrap();
    let page = ready(&docs, reply.job.unwrap().id.get())
        .job
        .unwrap()
        .page
        .unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.rows[0].start_ms.get(), -500);
    assert_eq!(page.rows[0].overlap_ms.unwrap().get(), 500);
    assert_eq!(page.selected.unwrap().reading.as_deref(), Some("Été 世界"));
    docs.translation_action(Action::Detach { key: key(&docs) })
        .unwrap();
    assert!(snapshot(&docs).translation.unwrap().reference.is_none());
    assert!(
        docs.apply(
            DocumentAction::Close { discard: false },
            before.generation,
            before.revision
        )
        .is_err()
    );
    assert_eq!(
        doc_action(&docs, DocumentAction::Redo).cues[0].text,
        "Translated"
    );
    assert_eq!(fs::read_to_string(reference).unwrap(), SRT);
    docs.shutdown_quality().unwrap();
}

#[test]
fn all_formats_copy_into_independent_target_and_protect_original_after_detach_and_undo() {
    for (extension, original) in [
        ("srt", SRT),
        ("ass", crate::document::ass_tests::ASS),
        ("ssa", crate::document::ass_tests::SSA),
        (
            "vtt",
            include_str!("../../../../tests/fixtures/subtitles/styles.vtt"),
        ),
    ] {
        let root = tempfile::tempdir().unwrap();
        let reference = root.path().join(format!("reference.{extension}"));
        fs::write(&reference, original).unwrap();
        let docs = Documents::default();
        let id = load(&docs, &reference, true);
        let accepted = docs.accept_reference(id, false).unwrap();
        assert!(matches!(accepted.job.unwrap().phase, Phase::Accepted));
        let target = accepted.document.unwrap();
        assert!(target.dirty && target.requires_save_as && target.path.is_none());
        text(&docs, "Translated");
        doc_action(&docs, DocumentAction::Undo);
        docs.translation_action(Action::Detach { key: key(&docs) })
            .unwrap();
        let before = snapshot(&docs);
        assert!(
            docs.apply(
                DocumentAction::Save {
                    path: Some(reference.to_string_lossy().into())
                },
                before.generation,
                before.revision
            )
            .is_err()
        );
        let output = root.path().join(format!("target.{extension}"));
        doc_action(
            &docs,
            DocumentAction::Save {
                path: Some(output.to_string_lossy().into()),
            },
        );
        assert_eq!(fs::read(&output).unwrap(), original.as_bytes());
        assert_eq!(fs::read(&reference).unwrap(), original.as_bytes());
        // Copy provenance remains after a successful Save As and detachment.
        let before = snapshot(&docs);
        assert!(
            docs.apply(
                DocumentAction::Save {
                    path: Some(reference.to_string_lossy().into())
                },
                before.generation,
                before.revision
            )
            .is_err()
        );
        doc_action(&docs, DocumentAction::Redo);
        assert_eq!(snapshot(&docs).cues[0].text, "Translated");
        docs.shutdown_quality().unwrap();
    }
}

#[test]
fn failed_and_stale_loads_preserve_current_target_context_and_originals() {
    let (root, docs, reference) = fixture();
    attach(&docs, &reference);
    let before = serde_json::to_value(snapshot(&docs)).unwrap();
    let bad = root.path().join("bad.srt");
    fs::write(&bad, [0xff]).unwrap();
    let start = docs
        .translation_action(Action::Load {
            key: key(&docs),
            path: bad.to_string_lossy().into(),
            copy: true,
        })
        .unwrap();
    let id = start.job.unwrap().id.get();
    assert!(matches!(ready(&docs, id).job.unwrap().phase, Phase::Failed));
    assert!(docs.accept_reference(id, true).is_err());
    assert_eq!(serde_json::to_value(snapshot(&docs)).unwrap(), before);
    let id = load(&docs, &reference, false);
    fs::write(&reference, SRT.replace("Été", "Changed")).unwrap();
    assert!(docs.accept_reference(id, false).is_err());
    assert_eq!(serde_json::to_value(snapshot(&docs)).unwrap(), before);
    fs::write(&reference, SRT).unwrap();
    let id = load(&docs, &reference, true);
    assert!(docs.accept_reference(id, false).is_err()); // context discard required
    text(&docs, "New accepted edit");
    assert!(docs.accept_reference(id, true).is_err());
    assert_eq!(snapshot(&docs).cues[0].text, "New accepted edit");
    assert!(matches!(ready(&docs, id).job.unwrap().phase, Phase::Stale));
    docs.shutdown_quality().unwrap();
}

#[test]
fn acceptance_rechecks_target_and_cancellation_after_off_lock_preparation() {
    for cancel in [false, true] {
        let (_root, docs, reference) = fixture();
        let id = load(&docs, &reference, true);
        let copy = docs.clone();
        let (reached, wait) = mpsc::channel();
        let (release, resume) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            copy.accept_reference_with(id, true, || {
                reached.send(()).unwrap();
                resume.recv_timeout(Duration::from_secs(10)).unwrap();
                Ok(())
            })
        });
        wait.recv_timeout(Duration::from_secs(10)).unwrap();
        assert!(docs.accept_reference(id, true).is_err());
        assert!(
            docs.translation_action(Action::Load {
                key: key(&docs),
                path: reference.to_string_lossy().into(),
                copy: false
            })
            .is_err()
        );
        if cancel {
            docs.translation_action(Action::Cancel {
                id: id.try_into().unwrap(),
            })
            .unwrap();
        } else {
            text(&docs, "New edit during acceptance");
        }
        release.send(()).unwrap();
        assert!(worker.join().unwrap().is_err());
        let current = snapshot(&docs);
        assert_eq!(current.generation, 1);
        assert!(current.translation.is_none());
        assert_eq!(
            current.cues[0].text,
            if cancel {
                "{\\an8}<i>Été 世界</i>"
            } else {
                "New edit during acceptance"
            }
        );
        docs.shutdown_quality().unwrap();
    }
}

#[test]
fn one_worker_admission_cancellation_and_shutdown_drain_without_document_lock() {
    let (_root, docs, reference) = fixture();
    let (reached, wait) = mpsc::channel();
    let (release, resume) = mpsc::channel();
    {
        let mut store = docs.0.lock().unwrap();
        let key = store.translation_key().unwrap();
        store
            .translation
            .launch(key, false, move |cancel| {
                reached.send(()).unwrap();
                resume.recv_timeout(Duration::from_secs(10)).unwrap();
                checkpoint(&cancel, Instant::now())?;
                Err("should have cancelled".into())
            })
            .unwrap();
    }
    wait.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(
        docs.translation_action(Action::Load {
            key: key(&docs),
            path: reference.to_string_lossy().into(),
            copy: false
        })
        .is_err()
    );
    let cloned = docs.clone();
    let worker = std::thread::spawn(move || cloned.shutdown_quality());
    let deadline = Instant::now() + Duration::from_secs(10);
    while !docs.0.lock().unwrap().translation.closed {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    // Shutdown is joining the worker, but the accepted document remains readable.
    assert_eq!(snapshot(&docs).cues.len(), 1);
    release.send(()).unwrap();
    worker.join().unwrap().unwrap();
    assert!(
        docs.translation_action(Action::Load {
            key: key(&docs),
            path: reference.to_string_lossy().into(),
            copy: true
        })
        .is_err()
    );
}

#[test]
fn shutdown_also_waits_for_acceptance_io_without_holding_the_document_lock() {
    let (_root, docs, reference) = fixture();
    let id = load(&docs, &reference, true);
    let accepting = docs.clone();
    let (reached, wait) = mpsc::channel();
    let (release, resume) = mpsc::channel();
    let accept = std::thread::spawn(move || {
        accepting.accept_reference_with(id, true, || {
            reached.send(()).unwrap();
            resume.recv_timeout(Duration::from_secs(10)).unwrap();
            Ok(())
        })
    });
    wait.recv_timeout(Duration::from_secs(10)).unwrap();
    let closing = docs.clone();
    let (finished, observed) = mpsc::channel();
    let close = std::thread::spawn(move || {
        let result = closing.shutdown_quality();
        finished.send(()).unwrap();
        result
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    while !docs.0.lock().unwrap().translation.closed {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert_eq!(snapshot(&docs).generation, 1);
    assert!(matches!(
        observed.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    release.send(()).unwrap();
    assert!(accept.join().unwrap().is_err());
    close.join().unwrap().unwrap();
    observed.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(snapshot(&docs).translation.is_none());
}

#[test]
fn mixed_formats_bounded_previews_invalid_pages_and_deadlines_are_explicit() {
    let (root, docs, _) = fixture();
    let original = format!(
        "WEBVTT\n\n00:00:01.000 --> 00:00:03.000\n<v Alice><b>{}</b></v>\n",
        "世".repeat(30_000)
    );
    let reference = root.path().join("long.vtt");
    fs::write(&reference, &original).unwrap();
    attach(&docs, &reference);
    for limit in [0, 201] {
        assert!(
            docs.translation_action(Action::Query {
                key: key(&docs),
                target_cue_id: None,
                offset: 0,
                limit,
                selected: None
            })
            .is_err()
        );
    }
    let start = docs
        .translation_action(Action::Query {
            key: key(&docs),
            target_cue_id: None,
            offset: 0,
            limit: 1,
            selected: Some(0),
        })
        .unwrap();
    let page = ready(&docs, start.job.unwrap().id.get())
        .job
        .unwrap()
        .page
        .unwrap();
    assert!(page.rows[0].truncated && page.rows[0].raw.len() <= 1024);
    assert!(page.rows[0].warning.as_ref().unwrap().contains("64 KiB"));
    let selected = page.selected.unwrap();
    assert!(selected.truncated && selected.raw.len() <= 65_536);
    assert_eq!(fs::read_to_string(&reference).unwrap(), original);
    assert!(
        docs.translation_action(Action::SetOffset {
            key: key(&docs),
            offset_ms: (Milliseconds::MAX as i64 + 1).try_into().unwrap()
        })
        .is_err()
    );
    assert!(
        checkpoint(
            &AtomicBool::new(false),
            Instant::now() - Duration::from_secs(11)
        )
        .is_err()
    );
    docs.shutdown_quality().unwrap();
}

#[test]
fn reference_save_protection_survives_deleted_names_and_symlink_choices() {
    let (root, docs, reference) = fixture();
    attach(&docs, &reference);
    let before = snapshot(&docs);
    assert!(
        docs.apply(
            DocumentAction::Save {
                path: Some(reference.to_string_lossy().into())
            },
            before.generation,
            before.revision
        )
        .is_err()
    );
    #[cfg(unix)]
    {
        let alias = root.path().join("alias.srt");
        std::os::unix::fs::symlink(&reference, &alias).unwrap();
        assert!(
            docs.apply(
                DocumentAction::Save {
                    path: Some(alias.to_string_lossy().into())
                },
                before.generation,
                before.revision
            )
            .is_err()
        );
        let id = load(&docs, &alias, true);
        fs::remove_file(&alias).unwrap();
        std::os::unix::fs::symlink(root.path().join("target.srt"), &alias).unwrap();
        assert!(docs.accept_reference(id, true).is_err());
    }
    fs::remove_file(&reference).unwrap();
    let alternate = root.path().join("REFERENCE.SRT");
    assert!(
        docs.apply(
            DocumentAction::Save {
                path: Some(alternate.to_string_lossy().into())
            },
            before.generation,
            before.revision
        )
        .is_err()
    );
    assert!(!alternate.exists());
    docs.shutdown_quality().unwrap();
}

#[test]
fn recovery_keeps_exact_missing_reference_and_rejects_corrupt_context_before_replacing_checkpoint()
{
    for extension in ["srt", "ass", "ssa", "vtt"] {
        let (root, docs, _) = fixture();
        let original = match extension {
            "ass" => crate::document::ass_tests::ASS,
            "ssa" => crate::document::ass_tests::SSA,
            "vtt" => include_str!("../../../../tests/fixtures/subtitles/styles.vtt"),
            _ => SRT,
        };
        let reference = root.path().join(format!("original.{extension}"));
        fs::write(&reference, original).unwrap();
        let id = load(&docs, &reference, true);
        docs.accept_reference(id, true).unwrap();
        docs.translation_action(Action::SetOffset {
            key: key(&docs),
            offset_ms: (-1500).try_into().unwrap(),
        })
        .unwrap();
        let context = docs
            .0
            .lock()
            .unwrap()
            .loaded
            .as_ref()
            .unwrap()
            .translation
            .clone();
        let mut record = crate::recovery_store::Record::new_document(
            subtitle_formats::Format::from_extension(extension).unwrap(),
            None,
            original.into(),
            original.into(),
            2,
            0,
        )
        .unwrap();
        record.set_translation(context.stored().unwrap());
        let recovery = root.path().join("recovery");
        fs::create_dir(&recovery).unwrap();
        let session = crate::recovery_store::Session::create(&recovery).unwrap();
        session.write(Some(&record)).unwrap();
        for (pointer, value) in [
            (
                "/translation/reference/sha256",
                serde_json::json!("changed"),
            ),
            (
                "/translation/reference/format",
                serde_json::json!("unknown"),
            ),
            (
                "/translation/reference/offset_ms",
                serde_json::json!(i64::MIN),
            ),
            (
                "/translation/reference/origin/path",
                serde_json::json!("relative.srt"),
            ),
            ("/version", serde_json::json!(4)),
        ] {
            let mut invalid = serde_json::to_value(&record).unwrap();
            *invalid.pointer_mut(pointer).unwrap() = value;
            let invalid = serde_json::from_value(invalid).unwrap();
            assert!(session.write(Some(&invalid)).is_err(), "{pointer}");
        }
        drop(session);
        fs::remove_file(&reference).unwrap();
        let entry = crate::recovery_store::list(&recovery).unwrap().remove(0);
        assert!(entry.error.is_none());
        let recovered = Documents::default();
        recovered.0.lock().unwrap().recovery_root = Some(recovery);
        let snapshot = doc_action(
            &recovered,
            DocumentAction::Recover {
                id: entry.id,
                discard: false,
            },
        );
        assert!(snapshot.dirty && snapshot.requires_save_as);
        let stored = recovered
            .0
            .lock()
            .unwrap()
            .loaded
            .as_ref()
            .unwrap()
            .translation
            .clone();
        let file = stored.reference.unwrap();
        assert_eq!(file.reference.original_bytes(), original.as_bytes());
        assert_eq!(file.offset.get(), -1500);
        let before = snapshot;
        assert!(
            recovered
                .apply(
                    DocumentAction::Save {
                        path: Some(reference.to_string_lossy().into())
                    },
                    before.generation,
                    before.revision
                )
                .is_err()
        );
        let output = root.path().join(format!("recovered.{extension}"));
        doc_action(
            &recovered,
            DocumentAction::Save {
                path: Some(output.to_string_lossy().into()),
            },
        );
        assert_eq!(fs::read(output).unwrap(), original.as_bytes());
        assert!(!reference.exists());
        docs.shutdown_quality().unwrap();
        recovered.shutdown_quality().unwrap();
    }
}

#[test]
fn context_only_change_invalidates_prepared_recovery_and_failed_save_preserves_it() {
    let (root, docs, reference) = fixture();
    attach(&docs, &reference);
    let session = crate::recovery_store::Session::create(&root.path().join("recovery")).unwrap();
    let (old_key, context) = {
        let store = docs.0.lock().unwrap();
        (
            store.checkpoint_key(),
            store.loaded.as_ref().unwrap().translation.clone(),
        )
    };
    let mut record = crate::recovery_store::Record::new(
        None,
        SRT.into(),
        SRT.into(),
        old_key.generation,
        old_key.revision,
    )
    .unwrap();
    record.set_translation(context.stored().unwrap());
    let prepared = session.prepare(&record).unwrap();
    docs.translation_action(Action::SetOffset {
        key: key(&docs),
        offset_ms: 20.try_into().unwrap(),
    })
    .unwrap();
    assert!(
        !docs
            .0
            .lock()
            .unwrap()
            .publish_checkpoint(&old_key, Some(prepared), &session)
            .unwrap()
    );
    let before = serde_json::to_value(snapshot(&docs)).unwrap();
    let snap = snapshot(&docs);
    assert!(
        docs.apply(
            DocumentAction::Save {
                path: Some(
                    root.path()
                        .join("missing")
                        .join("target.srt")
                        .to_string_lossy()
                        .into()
                )
            },
            snap.generation,
            snap.revision
        )
        .is_err()
    );
    assert_eq!(serde_json::to_value(snapshot(&docs)).unwrap(), before);
    assert!(
        docs.with_project_subtitle(snap.path.as_deref().map(Path::new), |_, _| -> Result<()> {
            panic!("context must not be silently omitted")
        })
        .is_err()
    );
    docs.shutdown_quality().unwrap();
}

#[test]
fn background_recovery_captures_context_only_changes_and_normal_quit_clears_it() {
    let (root, docs, reference) = fixture();
    let recovery = root.path().join("recovery");
    docs.start_recovery(recovery.clone()).unwrap();
    attach(&docs, &reference);
    for offset in [0, -1500] {
        if offset != 0 {
            docs.translation_action(Action::SetOffset {
                key: key(&docs),
                offset_ms: offset.try_into().unwrap(),
            })
            .unwrap();
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let captured = fs::read_dir(&recovery)
                .ok()
                .into_iter()
                .flatten()
                .filter_map(|e| e.ok())
                .filter_map(|e| fs::read(e.path().join("snapshot.json")).ok())
                .filter_map(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
                .any(|record| {
                    record["version"] == 5
                        && record["translation"]["reference"]["text"] == SRT
                        && record["translation"]["reference"]["offset_ms"] == offset
                });
            if captured {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "checkpoint did not capture reference-only change"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    let before = snapshot(&docs);
    assert!(!before.dirty && before.translation.unwrap().dirty);
    assert!(
        docs.approve_exit(before.generation, before.revision, false)
            .is_err()
    );
    docs.approve_exit(before.generation, before.revision, true)
        .unwrap();
    assert!(crate::recovery_store::list(&recovery).unwrap().is_empty());
    assert_eq!(fs::read_to_string(reference).unwrap(), SRT);
    docs.shutdown_quality().unwrap();
}
