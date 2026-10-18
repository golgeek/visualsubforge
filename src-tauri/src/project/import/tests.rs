use super::*;
use crate::quality_profile::Policy;
use std::{fs, path::Path, sync::mpsc, time::Duration};

fn fixture(extension: &str) -> (tempfile::TempDir, ImportRequest, Vec<u8>) {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("clip.mkv"), b"synthetic media fingerprint").unwrap();
    let subtitles = match extension {
        "ass" => include_str!("../../../../tests/fixtures/subtitles/styles.ass"),
        "ssa" => include_str!("../../../../tests/fixtures/subtitles/styles.ssa"),
        "vtt" => include_str!("../../../../tests/fixtures/subtitles/styles.vtt"),
        _ => "\u{feff}7\r\n00:00:01,001 --> 00:00:02,002\r\n世界 café\r\n\r\n",
    }
    .as_bytes()
    .to_vec();
    fs::write(
        root.path().join(format!("captions.{extension}")),
        &subtitles,
    )
    .unwrap();
    let text = format!(
        "[VisualSubsync]\r\nVideoSource=clip.mkv\r\nSubtitlesFile=captions.{extension}\r\nFocusedTimeMs=1501\r\nWAVDisplayerPositionStartMs=1000\r\nWAVDisplayerPositionStopMs=16000\r\nPresets=legacy.ini\r\n"
    );
    let mut bytes = vec![0xff, 0xfe];
    for word in text.encode_utf16() {
        bytes.extend(word.to_le_bytes());
    }
    let path = root.path().join("original.vssprj");
    fs::write(&path, bytes).unwrap();
    (
        root,
        ImportRequest {
            path: path.to_string_lossy().into_owned(),
            media_path: None,
            subtitle_path: None,
        },
        subtitles,
    )
}
fn settled(importer: &Importer, id: u64) -> ImportReview {
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        let view = importer.view(id).unwrap();
        if !view.running {
            return view;
        }
        assert!(Instant::now() < end, "import worker did not finish");
        std::thread::sleep(Duration::from_millis(2));
    }
}
fn start(importer: &Importer, request: ImportRequest) -> u64 {
    let id = importer.start(request, Policy::default()).unwrap();
    assert_eq!(settled(importer, id).phase, ImportPhase::Reviewing);
    id
}

#[test]
fn exports_each_format_as_a_new_project_without_attaching_or_rewriting_inputs() {
    for extension in ["srt", "ass", "ssa", "vtt"] {
        let (root, request, subtitles) = fixture(extension);
        let original = fs::read(&request.path).unwrap();
        let projects = crate::project::Projects::default();
        let active_project = crate::project::tests::fixture(root.path());
        let active = root.path().join("active.vsfproject");
        crate::project::write(&active, &active_project).unwrap();
        let token = projects
            .attach(projects.open(&active).unwrap().token)
            .unwrap()
            .token;
        let active_bytes = fs::read(&active).unwrap();
        let id = start(&projects.imports, request.clone());
        let report = settled(&projects.imports, id).report.unwrap();
        assert!(report.can_publish);
        assert_eq!(report.encoding, "UTF-16LE");
        assert_eq!(report.subtitle_format.as_deref(), Some(extension));
        assert_eq!(report.position_ms.get(), 1501);
        assert_eq!(report.waveform_span_ms.get(), 15000);
        assert!(report.warnings.iter().any(|w| w.field.contains("presets")));
        let output = root.path().join("imported.vsfproject");
        projects
            .imports
            .publish(id, output.to_string_lossy().into_owned())
            .unwrap();
        let view = settled(&projects.imports, id);
        assert_eq!(view.phase, ImportPhase::Published, "{:?}", view.error);
        assert_eq!(
            view.output_path.as_deref(),
            output.canonicalize().unwrap().to_str()
        );
        let imported = crate::project::read(&output).unwrap();
        assert_eq!(imported.position_seconds, 1.501);
        assert_eq!(imported.options.waveform_start, 1.0);
        assert_eq!(imported.options.waveform_span, 15.0);
        assert_eq!(imported.subtitle_format.as_deref(), Some(extension));
        assert!(imported.scenes.is_none());
        let json: serde_json::Value = serde_json::from_slice(&fs::read(&output).unwrap()).unwrap();
        assert_eq!(json["mediaPath"], "clip.mkv");
        assert_eq!(json["subtitlePath"], format!("captions.{extension}"));
        assert_eq!(fs::read(&request.path).unwrap(), original);
        assert_eq!(
            fs::read(root.path().join(format!("captions.{extension}"))).unwrap(),
            subtitles
        );
        assert_eq!(fs::read(&active).unwrap(), active_bytes);
        // The pre-import attachment token still works. The new file was not adopted.
        projects
            .save(&active, Some(token), &active_project)
            .unwrap();
        projects.imports.cancel(id).unwrap();
        assert_eq!(settled(&projects.imports, id).phase, ImportPhase::Published);
        assert!(
            projects
                .imports
                .publish(
                    id,
                    root.path()
                        .join("duplicate.vsfproject")
                        .to_string_lossy()
                        .into_owned()
                )
                .is_err()
        );
        projects.imports.shutdown().unwrap();
        assert_eq!(
            projects.imports.view(id).unwrap().phase,
            ImportPhase::Published
        );
    }
}

fn pause() -> (
    impl FnOnce() -> Result<()> + Send + 'static,
    mpsc::Receiver<()>,
    mpsc::Sender<()>,
) {
    let (entered, wait_entered) = mpsc::channel();
    let (release, wait_release) = mpsc::channel();
    (
        move || {
            entered.send(()).map_err(|e| e.to_string())?;
            wait_release
                .recv_timeout(Duration::from_secs(5))
                .map_err(|e| e.to_string())
        },
        wait_entered,
        release,
    )
}
fn no_temporary(root: &Path) {
    assert!(!fs::read_dir(root).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".tmp")
    }));
}

#[test]
fn missing_references_can_be_relocated_and_old_review_ids_cannot_publish_or_cancel_new_work() {
    let (root, mut request, _) = fixture("srt");
    let source_before = fs::read(&request.path).unwrap();
    fs::rename(
        root.path().join("clip.mkv"),
        root.path().join("relocated.mkv"),
    )
    .unwrap();
    fs::rename(
        root.path().join("captions.srt"),
        root.path().join("relocated.srt"),
    )
    .unwrap();
    let importer = Importer::default();
    let old = start(&importer, request.clone());
    let report = importer.view(old).unwrap().report.unwrap();
    assert!(
        !report.can_publish && report.media.issue.is_some() && report.subtitles.issue.is_some()
    );
    let output = root
        .path()
        .join("result.vsfproject")
        .to_string_lossy()
        .into_owned();
    assert!(importer.publish(old, output.clone()).is_err());
    request.media_path = Some(
        root.path()
            .join("relocated.mkv")
            .to_string_lossy()
            .into_owned(),
    );
    request.subtitle_path = Some(
        root.path()
            .join("relocated.srt")
            .to_string_lossy()
            .into_owned(),
    );
    let id = start(&importer, request.clone());
    assert!(id > old && importer.view(old).is_err());
    importer.cancel(old).unwrap();
    assert!(importer.view(id).unwrap().report.unwrap().can_publish);
    assert!(importer.publish(old, output.clone()).is_err());
    importer.publish(id, output.clone()).unwrap();
    assert_eq!(settled(&importer, id).phase, ImportPhase::Published);
    let imported = crate::project::read(Path::new(&output)).unwrap();
    assert_eq!(
        imported.subtitle_path.as_deref(),
        Path::new(request.subtitle_path.as_ref().unwrap())
            .canonicalize()
            .unwrap()
            .to_str()
    );
    assert_eq!(fs::read(&request.path).unwrap(), source_before);
}

#[test]
fn preparation_has_no_queue_and_cancellation_cannot_publish_a_late_result() {
    let (_root, request, _) = fixture("vtt");
    let importer = Importer::default();
    let (hook, entered, release) = pause();
    let id = importer
        .start_with(request.clone(), Policy::default(), hook)
        .unwrap();
    entered.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(
        importer
            .start(request.clone(), Policy::default())
            .unwrap_err()
            .contains("Wait")
    );
    importer.cancel(id).unwrap();
    assert_eq!(importer.view(id).unwrap().phase, ImportPhase::Cancelling);
    release.send(()).unwrap();
    let view = settled(&importer, id);
    assert_eq!(view.phase, ImportPhase::Cancelled);
    assert!(view.report.is_none());
    let next = start(&importer, request);
    importer.cancel(id).unwrap();
    assert_eq!(importer.view(next).unwrap().phase, ImportPhase::Reviewing);
}

#[test]
fn failed_destinations_retain_the_review_for_retry_and_never_replace_files() {
    let (root, request, _) = fixture("ass");
    let importer = Importer::default();
    let id = start(&importer, request);
    let existing = root.path().join("existing.vsfproject");
    fs::write(&existing, b"existing project").unwrap();
    for path in [
        existing.clone(),
        root.path().join("missing/copy.vsfproject"),
        root.path().join("wrong.srt"),
    ] {
        importer
            .publish(id, path.to_string_lossy().into_owned())
            .unwrap();
        let view = settled(&importer, id);
        assert_eq!(view.phase, ImportPhase::Reviewing);
        assert!(view.error.is_some() && view.report.unwrap().can_publish);
        assert_eq!(fs::read(&existing).unwrap(), b"existing project");
        no_temporary(root.path());
    }
    #[cfg(unix)]
    {
        let alias = root.path().join("alias.vsfproject");
        std::os::unix::fs::symlink(&existing, &alias).unwrap();
        importer
            .publish(id, alias.to_string_lossy().into_owned())
            .unwrap();
        assert_eq!(settled(&importer, id).phase, ImportPhase::Reviewing);
        assert_eq!(fs::read(alias).unwrap(), b"existing project");
    }
    let copy = root.path().join("copy.vsfproject");
    importer
        .publish(id, copy.to_string_lossy().into_owned())
        .unwrap();
    assert_eq!(settled(&importer, id).phase, ImportPhase::Published);
    assert!(crate::project::read(&copy).is_ok());
    assert_eq!(fs::read(&existing).unwrap(), b"existing project");
}

#[test]
fn changes_after_temporary_write_make_the_review_stale_without_publishing() {
    for changed in ["source", "subtitles", "media"] {
        let (root, request, _) = fixture("vtt");
        let importer = Importer::default();
        let id = start(&importer, request.clone());
        let path = match changed {
            "source" => PathBuf::from(&request.path),
            "subtitles" => root.path().join("captions.vtt"),
            _ => root.path().join("clip.mkv"),
        };
        let mut changed_bytes = fs::read(&path).unwrap();
        changed_bytes.extend(b"external change");
        let bytes = changed_bytes.clone();
        let changed_path = path.clone();
        let output = root.path().join("copy.vsfproject");
        importer
            .publish_with(id, output.to_string_lossy().into_owned(), move || {
                fs::write(changed_path, bytes).map_err(|e| e.to_string())
            })
            .unwrap();
        let view = settled(&importer, id);
        assert_eq!(
            view.phase,
            ImportPhase::Stale,
            "{changed}: {:?}",
            view.error
        );
        assert!(view.error.unwrap().contains("changed"));
        assert!(!output.exists());
        assert_eq!(fs::read(path).unwrap(), changed_bytes);
        assert!(
            importer
                .publish(id, output.to_string_lossy().into_owned())
                .is_err()
        );
        no_temporary(root.path());
    }
}

#[test]
fn cancellation_during_export_removes_only_its_temporary_and_retry_needs_a_new_review() {
    let (root, request, subtitles) = fixture("srt");
    let original = fs::read(&request.path).unwrap();
    let importer = Importer::default();
    let id = start(&importer, request.clone());
    let output = root.path().join("copy.vsfproject");
    let (hook, entered, release) = pause();
    importer
        .publish_with(id, output.to_string_lossy().into_owned(), hook)
        .unwrap();
    entered.recv_timeout(Duration::from_secs(2)).unwrap();
    importer.cancel(id).unwrap();
    assert_eq!(importer.view(id).unwrap().phase, ImportPhase::Cancelling);
    release.send(()).unwrap();
    assert_eq!(settled(&importer, id).phase, ImportPhase::Cancelled);
    assert!(!output.exists());
    no_temporary(root.path());
    assert_eq!(fs::read(&request.path).unwrap(), original);
    assert_eq!(
        fs::read(root.path().join("captions.srt")).unwrap(),
        subtitles
    );
    assert!(
        importer
            .publish(id, output.to_string_lossy().into_owned())
            .is_err()
    );
    let id = start(&importer, request);
    importer
        .publish(id, output.to_string_lossy().into_owned())
        .unwrap();
    assert_eq!(settled(&importer, id).phase, ImportPhase::Published);
}

#[test]
fn a_competing_file_created_after_preparation_wins_without_partial_overwrite() {
    let (root, request, _) = fixture("ssa");
    let importer = Importer::default();
    let id = start(&importer, request);
    let output = root.path().join("copy.vsfproject");
    let competing = output.clone();
    importer
        .publish_with(id, output.to_string_lossy().into_owned(), move || {
            fs::write(competing, b"competing writer").map_err(|e| e.to_string())
        })
        .unwrap();
    let view = settled(&importer, id);
    assert_eq!(view.phase, ImportPhase::Reviewing);
    assert!(view.error.unwrap().contains("publish"));
    assert_eq!(fs::read(output).unwrap(), b"competing writer");
    no_temporary(root.path());
}

#[test]
fn shutdown_cancels_and_joins_pending_publication_outside_the_owner_lock() {
    let (root, request, _) = fixture("vtt");
    let importer = Arc::new(Importer::default());
    let id = start(&importer, request.clone());
    let output = root.path().join("copy.vsfproject");
    let (hook, entered, release) = pause();
    importer
        .publish_with(id, output.to_string_lossy().into_owned(), hook)
        .unwrap();
    entered.recv_timeout(Duration::from_secs(2)).unwrap();
    let (done, wait_done) = mpsc::channel();
    let closing = importer.clone();
    let thread = std::thread::spawn(move || {
        closing.shutdown().unwrap();
        done.send(()).unwrap();
    });
    assert!(wait_done.recv_timeout(Duration::from_millis(20)).is_err());
    // The worker is still blocked, but the owner mutex remains available.
    assert_eq!(importer.view(id).unwrap().phase, ImportPhase::Closed);
    release.send(()).unwrap();
    wait_done.recv_timeout(Duration::from_secs(2)).unwrap();
    thread.join().unwrap();
    assert!(!output.exists());
    assert!(importer.start(request, Policy::default()).is_err());
    no_temporary(root.path());
}

#[test]
fn failed_workers_deadlines_and_exhausted_ids_are_explicit_and_do_not_destroy_a_good_review() {
    let (_root, request, _) = fixture("srt");
    let importer = Importer::default();
    let id = importer
        .start_with(request.clone(), Policy::default(), || {
            panic!("injected import failure")
        })
        .unwrap();
    assert_eq!(settled(&importer, id).phase, ImportPhase::Failed);
    let id = start(&importer, request.clone());
    importer.state.lock().unwrap().next_id = crate::ipc_number::MAX_SAFE_INTEGER;
    assert!(
        importer
            .start(request, Policy::default())
            .unwrap_err()
            .contains("identities exhausted")
    );
    assert_eq!(importer.view(id).unwrap().phase, ImportPhase::Reviewing);
    assert!(
        checkpoint(&AtomicBool::new(false), Instant::now() - DEADLINE)
            .unwrap_err()
            .contains("deadline")
    );
    assert!(
        checkpoint(&AtomicBool::new(true), Instant::now())
            .unwrap_err()
            .contains("cancelled")
    );
}

#[test]
fn import_uses_captured_defaults_without_changing_current_project_policy() {
    let (root, request, _) = fixture("srt");
    let docs = crate::document::Documents::default();
    let defaults = Policy::legacy(
        crate::timing_preset::Settings {
            cue_gap_ms: 167,
            ..Default::default()
        },
        "import-test",
    )
    .unwrap();
    docs.initialize_quality(defaults.clone()).unwrap();
    let projects = crate::project::Projects::default();
    let active = root.path().join("active.vsfproject");
    let project = crate::project::tests::fixture(root.path());
    crate::project::write(&active, &project).unwrap();
    let token = projects.open(&active).unwrap().token;
    docs.attach_quality_project(
        &projects,
        token,
        0,
        0,
        &root.path().join("clip.mkv").canonicalize().unwrap(),
    )
    .unwrap();
    assert_ne!(
        docs.quality_context().unwrap().timing.cue_gap_ms,
        defaults.timing.cue_gap_ms
    );
    assert_eq!(docs.default_quality_policy().unwrap(), defaults);
    let id = projects
        .imports
        .start(request, docs.default_quality_policy().unwrap())
        .unwrap();
    assert_eq!(
        settled(&projects.imports, id)
            .report
            .unwrap()
            .quality_profile_name,
        defaults.profile.name
    );
    let output = root.path().join("imported.vsfproject");
    projects
        .imports
        .publish(id, output.to_string_lossy().into_owned())
        .unwrap();
    assert_eq!(settled(&projects.imports, id).phase, ImportPhase::Published);
    let imported = crate::project::read(&output).unwrap();
    assert_eq!(imported.options.timing, defaults.timing);
    assert_eq!(imported.options.quality, Some(defaults.profile));
    assert_eq!(docs.quality_context().unwrap().timing.cue_gap_ms, 80);
}

#[cfg(unix)]
#[test]
fn retargeting_a_media_alias_invalidates_export_even_with_identical_contents() {
    use std::os::unix::fs::symlink;
    let (root, mut request, _) = fixture("srt");
    let first = root.path().join("clip.mkv");
    let second = root.path().join("other.mkv");
    fs::copy(&first, &second).unwrap();
    let selected = root.path().join("selected.mkv");
    symlink(&first, &selected).unwrap();
    request.media_path = Some(selected.to_string_lossy().into_owned());
    let importer = Importer::default();
    let id = start(&importer, request);
    let output = root.path().join("copy.vsfproject");
    let (hook, entered, release) = pause();
    importer
        .publish_with(id, output.to_string_lossy().into_owned(), hook)
        .unwrap();
    entered.recv_timeout(Duration::from_secs(2)).unwrap();
    fs::remove_file(&selected).unwrap();
    symlink(&second, &selected).unwrap();
    release.send(()).unwrap();
    let view = settled(&importer, id);
    assert_eq!(view.phase, ImportPhase::Stale);
    assert!(!view.report.unwrap().can_publish);
    assert!(!output.exists());
    assert_eq!(fs::read_link(selected).unwrap(), second);
    no_temporary(root.path());
}

#[test]
fn missing_optional_reference_names_are_protected_and_temporary_write_failure_is_retryable() {
    let (root, request, _) = fixture("srt");
    let source = "[VisualSubSync]\nVideoSource=clip.mkv\nSubtitlesFile=captions.srt\nTextPipeSource=reserved.vsfproject\n";
    fs::write(&request.path, source).unwrap();
    let importer = Importer::default();
    let id = start(&importer, request.clone());
    let protected = root.path().join("reserved.vsfproject");
    importer
        .publish(id, protected.to_string_lossy().into_owned())
        .unwrap();
    let view = settled(&importer, id);
    assert_eq!(view.phase, ImportPhase::Reviewing);
    assert!(view.error.unwrap().contains("preserve all original inputs"));
    assert!(!protected.exists());
    let output = root.path().join("copy.vsfproject");
    importer
        .publish_with(id, output.to_string_lossy().into_owned(), || {
            Err("Injected write failure".into())
        })
        .unwrap();
    let view = settled(&importer, id);
    assert_eq!(view.phase, ImportPhase::Reviewing);
    assert!(view.report.unwrap().can_publish);
    assert!(!output.exists());
    no_temporary(root.path());
    importer
        .publish(id, output.to_string_lossy().into_owned())
        .unwrap();
    assert_eq!(settled(&importer, id).phase, ImportPhase::Published);
    assert_eq!(fs::read_to_string(request.path).unwrap(), source);
}
