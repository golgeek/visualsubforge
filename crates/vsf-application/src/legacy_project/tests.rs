use super::*;

// Independently authored data; no legacy sample or source code is redistributed.
const PROJECT: &str = "[VisualSubsync]\r\nVideoSource=media\\été 世界😀.mkv\r\nWAVFile=audio.wav\r\nSubtitlesFile=\"captions\\日本語 = #;.srt\"\r\nWAVDisplayerPositionStartMs=1234\r\nWAVDisplayerPositionStopMs=18765\r\nFocusedTimeMs=2501\r\n";
const SRT: &str = "\u{feff}007\r\n00:00:01,001 --> 00:00:02,002\r\n世界 café\r\n\r\n";

fn encode(text: &str, encoding: Encoding) -> Vec<u8> {
    match encoding {
        Encoding::Utf8 => text.as_bytes().to_vec(),
        Encoding::Utf16Le | Encoding::Utf16Be => {
            let mut bytes = if encoding == Encoding::Utf16Le {
                vec![0xff, 0xfe]
            } else {
                vec![0xfe, 0xff]
            };
            for word in text.encode_utf16() {
                bytes.extend(if encoding == Encoding::Utf16Le {
                    word.to_le_bytes()
                } else {
                    word.to_be_bytes()
                });
            }
            bytes
        }
    }
}

#[test]
fn utf8_and_both_utf16_orders_preserve_unicode_paths_and_millisecond_positions() {
    for encoding in [Encoding::Utf8, Encoding::Utf16Le, Encoding::Utf16Be] {
        let bytes = encode(PROJECT, encoding);
        let parsed = parse(&bytes).unwrap();
        assert_eq!(parsed.encoding, encoding);
        assert_eq!(
            parsed.reference(ReferenceKind::Video).unwrap().value,
            "media\\été 世界😀.mkv"
        );
        assert_eq!(
            parsed.reference(ReferenceKind::Subtitles).unwrap().value,
            "captions\\日本語 = #;.srt"
        );
        assert_eq!(
            parsed.view,
            View {
                start_ms: 1234,
                span_ms: 17531,
                focused_ms: Some(2501)
            }
        );
        assert!(
            parsed
                .warnings
                .iter()
                .any(|w| w.message.contains("Separate external audio"))
        );
        assert!(
            parsed
                .warnings
                .iter()
                .any(|w| w.message.contains("paused resume"))
        );
    }
    assert_eq!(
        parse(format!("\u{feff}{PROJECT}").as_bytes()).unwrap(),
        parse(PROJECT.as_bytes()).unwrap()
    );
}

#[test]
fn malformed_encoding_is_rejected_without_lossy_replacement_or_locale_guessing() {
    for bytes in [
        vec![0xff, 0xfe, 0],       // odd UTF-16
        vec![0xff, 0xfe, 0, 0xd8], // unpaired surrogate
        vec![0xfe, 0xff, 0xdc, 0],
        vec![0xff, 0xfe, 0, 0, b'[', 0, 0, 0], // UTF-32
        b"[VisualSubsync]\nVideoSource=\xe9.mkv".to_vec(),
        b"[VisualSubsync]\0\n".to_vec(),
    ] {
        assert!(parse(&bytes).is_err(), "{bytes:?}");
    }
}

#[test]
fn settings_are_case_insensitive_but_duplicates_and_malformed_sections_are_not_guessed() {
    let p = parse(b"; comment\r[VISUALSUBSYNC]\rSUBTITLESFILE='one.srt'\r# comment\r").unwrap();
    assert_eq!(
        p.reference(ReferenceKind::Subtitles).unwrap().value,
        "one.srt"
    );
    for text in [
        "[VisualSubsync]\nVideoSource=a\nvideosource=b",
        "[VisualSubsync]\nVideoSource=a\n[VISUALSUBSYNC]\nVideoSource=b",
        "VideoSource=a\n[VisualSubsync]",
        "[VisualSubsync] trailing\nVideoSource=a",
        "[VisualSubsync]\nnot an assignment",
        "[Other]\nVideoSource=a",
        "[]\nVideoSource=a",
    ] {
        assert!(parse(text.as_bytes()).is_err(), "{text}");
    }
}

#[test]
fn invalid_or_partial_positions_reset_with_diagnostics_and_never_wrap() {
    for values in [
        "WAVDisplayerPositionStartMs=2000\nWAVDisplayerPositionStopMs=1000",
        "WAVDisplayerPositionStartMs=1000",
        "WAVDisplayerPositionStartMs=-2\nWAVDisplayerPositionStopMs=NaN",
        "WAVDisplayerPositionStartMs=2147483648\nWAVDisplayerPositionStopMs=2147483649",
    ] {
        let p = parse(format!("[VisualSubsync]\n{values}\nFocusedTimeMs=-2").as_bytes()).unwrap();
        assert_eq!(p.view, View::default());
        assert!(p.warnings.len() >= 2);
    }
    let default = parse(b"[VisualSubsync]\nFocusedTimeMs=-1\nWAVDisplayerPositionStartMs=-1\nWAVDisplayerPositionStopMs=-1").unwrap();
    assert_eq!(default.view, View::default());
    assert!(default.warnings.is_empty());
    let max = parse(b"[VisualSubsync]\nFocusedTimeMs=2147483647\nWAVDisplayerPositionStartMs=0\nWAVDisplayerPositionStopMs=2147483647").unwrap();
    assert_eq!(max.view.span_ms, i32::MAX as u32);
    assert_eq!(max.view.focused_ms, Some(i32::MAX as u32));
}

#[test]
fn every_unsupported_setting_is_diagnosed_and_reference_paths_remain_available() {
    let p = parse(b"[VisualSubsync]\nPeakFile=old.peak\nSubtitlesVO=reference.srt\nTextPipeSource=script.vssscript\nTextPipePosition=4\nDictionary=fr-custom\nPresets=old.ini\nShowVO=1\nWAVMode=2\nUnknown=kept\n[Plugin]\nScript=never.js").unwrap();
    assert_eq!(p.warnings.len(), 10);
    assert_eq!(p.references.len(), 3);
    assert_eq!(
        p.reference(ReferenceKind::ReferenceSubtitles)
            .unwrap()
            .value,
        "reference.srt"
    );
    assert_eq!(
        p.reference(ReferenceKind::TextPipe).unwrap().value,
        "script.vssscript"
    );
    assert!(
        p.warnings
            .iter()
            .all(|w| w.line.is_some() && !w.field.is_empty())
    );
}

#[test]
fn parser_limits_bound_input_values_and_diagnostic_count() {
    assert!(
        parse(&vec![b' '; MAX_BYTES + 1])
            .unwrap_err()
            .contains("256 KiB")
    );
    let long = format!(
        "[VisualSubsync]\nVideoSource={}",
        "界".repeat(MAX_VALUE_CHARS + 1)
    );
    assert!(parse(long.as_bytes()).unwrap_err().contains("4096"));
    let mut text = String::from("[VisualSubsync]\n");
    for i in 0..MAX_SETTINGS {
        text.push_str(&format!("unknown{i}=1\n"));
    }
    assert_eq!(parse(text.as_bytes()).unwrap().warnings.len(), MAX_SETTINGS);
    text.push_str("extra=1");
    assert!(parse(text.as_bytes()).unwrap_err().contains("512"));
}

#[test]
fn path_syntax_never_guesses_windows_drives_unc_urls_or_current_directories() {
    for (path, expected) in [
        ("sub\\日本語.srt", PathKind::Relative),
        ("../a.srt", PathKind::Relative),
        ("/tmp/a.srt", PathKind::PosixAbsolute),
        ("C:\\a.srt", PathKind::WindowsAbsolute),
        ("c:/a.srt", PathKind::WindowsAbsolute),
        ("C:a.srt", PathKind::DriveRelative),
        ("\\a.srt", PathKind::WindowsRootRelative),
        ("\\\\server\\share\\a", PathKind::Unc),
        ("\\\\?\\C:\\a", PathKind::Device),
        ("\\\\.\\pipe\\x", PathKind::Device),
        ("https://host/a", PathKind::Url),
        ("file:///a", PathKind::Url),
    ] {
        assert_eq!(classify_path(path), expected, "{path}");
    }
    let base = std::env::current_dir().unwrap();
    assert_eq!(
        local_path(&base, "sub\\a.srt").unwrap(),
        base.join("sub/a.srt")
    );
    assert_eq!(
        local_path(&base, "../a.srt").unwrap(),
        base.join("../a.srt")
    );
    for path in ["C:a", "\\a", "\\\\?\\C:\\a", "file:///a", "a\0b"] {
        assert!(local_path(&base, path).is_err());
    }
    assert!(local_path(Path::new("relative-base"), "a.srt").is_err());
    assert_eq!(local_path(&base, "C:\\a.srt").is_ok(), cfg!(windows));
    assert_eq!(local_path(&base, "/tmp/a.srt").is_ok(), !cfg!(windows));
}

fn fixture(text: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("clip.mkv"), b"synthetic identity only").unwrap();
    fs::write(dir.path().join("captions.srt"), SRT).unwrap();
    let source = dir.path().join("sample.vssprj");
    fs::write(&source, encode(text, Encoding::Utf16Le)).unwrap();
    (dir, source)
}

#[test]
fn inspection_validates_each_format_and_preserves_exact_original_files() {
    for (extension, subtitles) in [
        ("srt", SRT),
        (
            "ass",
            include_str!("../../../../tests/fixtures/subtitles/styles.ass"),
        ),
        (
            "ssa",
            include_str!("../../../../tests/fixtures/subtitles/styles.ssa"),
        ),
        (
            "vtt",
            include_str!("../../../../tests/fixtures/subtitles/styles.vtt"),
        ),
    ] {
        let (dir, source) = fixture(&format!(
            "[VisualSubsync]\nVideoSource=clip.mkv\nSubtitlesFile=字幕é😀.{extension}"
        ));
        let target = dir.path().join(format!("字幕é😀.{extension}"));
        fs::write(&target, subtitles).unwrap();
        let before = fs::read(&source).unwrap();
        let inspected = inspect(&source, &Overrides::default(), || Ok(())).unwrap();
        assert!(inspected.ready());
        assert_eq!(inspected.subtitle_format.unwrap().extension(), extension);
        assert_eq!(inspected.subtitle_bytes.unwrap(), subtitles.as_bytes());
        assert_eq!(inspected.source_bytes, before);
        assert_eq!(fs::read(&source).unwrap(), before);
        assert_eq!(fs::read(&target).unwrap(), subtitles.as_bytes());
    }
}

#[test]
fn missing_and_foreign_references_need_explicit_relocation_without_basename_fallback() {
    let (dir, source) = fixture(
        "[VisualSubsync]\nVideoSource=missing/clip.mkv\nWAVFile=clip.mkv\nSubtitlesFile=Z:\\captions.srt",
    );
    let broken = inspect(&source, &Overrides::default(), || Ok(())).unwrap();
    assert!(!broken.ready());
    assert!(broken.media.issue.unwrap().contains("current location"));
    assert!(broken.subtitles.issue.is_some());
    let repaired = inspect(
        &source,
        &Overrides {
            media: Some(dir.path().join("clip.mkv")),
            subtitles: Some(dir.path().join("captions.srt")),
        },
        || Ok(()),
    )
    .unwrap();
    assert!(repaired.ready());
    assert!(repaired.media.overridden && repaired.subtitles.overridden);
    assert_eq!(repaired.media.original.as_deref(), Some("missing/clip.mkv"));
    assert_eq!(
        repaired.subtitles.original.as_deref(),
        Some("Z:\\captions.srt")
    );
    assert_eq!(
        parse(&fs::read(&source).unwrap())
            .unwrap()
            .reference(ReferenceKind::Video)
            .unwrap()
            .value,
        "missing/clip.mkv"
    );
}

#[test]
fn audio_only_uses_explicit_wav_reference_and_invalid_subtitles_remain_blocking() {
    let (dir, source) =
        fixture("[VisualSubsync]\nVideoSource=\nWAVFile=clip.mkv\nSubtitlesFile=captions.srt");
    assert_eq!(
        inspect(&source, &Overrides::default(), || Ok(()))
            .unwrap()
            .media
            .field,
        "WAVFile"
    );
    fs::write(dir.path().join("captions.srt"), "broken").unwrap();
    let bad = inspect(&source, &Overrides::default(), || Ok(())).unwrap();
    assert!(!bad.ready());
    assert!(bad.subtitles.issue.unwrap().contains("Invalid subtitles"));
    let relative = inspect(
        &source,
        &Overrides {
            media: Some("clip.mkv".into()),
            ..Overrides::default()
        },
        || Ok(()),
    )
    .unwrap();
    assert!(relative.media.issue.unwrap().contains("absolute"));
    let folder = inspect(
        &source,
        &Overrides {
            media: Some(dir.path().into()),
            ..Overrides::default()
        },
        || Ok(()),
    )
    .unwrap();
    assert!(folder.media.issue.unwrap().contains("regular file"));
}

#[test]
fn cancellation_at_every_inspection_boundary_never_mutates_inputs() {
    let (_dir, source) =
        fixture("[VisualSubsync]\nVideoSource=clip.mkv\nSubtitlesFile=captions.srt");
    let before = fs::read(&source).unwrap();
    let mut count = 0;
    assert!(
        inspect(&source, &Overrides::default(), || {
            count += 1;
            Ok(())
        })
        .unwrap()
        .ready()
    );
    for cancelled in 1..=count {
        let mut step = 0;
        let result = inspect(&source, &Overrides::default(), || {
            step += 1;
            if step == cancelled {
                Err("cancelled by test".into())
            } else {
                Ok(())
            }
        });
        assert!(matches!(result, Err(e) if e == "cancelled by test"));
        assert_eq!(step, cancelled);
        assert_eq!(fs::read(&source).unwrap(), before);
    }
}

#[test]
fn revalidation_detects_both_input_changes_and_propagates_cancellation() {
    let (dir, source) =
        fixture("[VisualSubsync]\nVideoSource=clip.mkv\nSubtitlesFile=captions.srt");
    let inspected = inspect(&source, &Overrides::default(), || Ok(())).unwrap();
    verify_text_inputs(&inspected, || Ok(())).unwrap();
    fs::write(
        &source,
        encode("[VisualSubsync]\nVideoSource=other.mkv", Encoding::Utf16Le),
    )
    .unwrap();
    assert!(
        verify_text_inputs(&inspected, || Ok(()))
            .unwrap_err()
            .contains("Legacy project changed")
    );
    fs::write(&source, &inspected.source_bytes).unwrap();
    let target = dir.path().join("captions.srt");
    fs::write(&target, SRT.replace("café", "new edit")).unwrap();
    assert!(
        verify_text_inputs(&inspected, || Ok(()))
            .unwrap_err()
            .contains("Subtitles changed")
    );
    fs::write(&target, SRT).unwrap();
    for cancel_at in 1..=3 {
        let mut step = 0;
        assert_eq!(
            verify_text_inputs(&inspected, || {
                step += 1;
                if step == cancel_at {
                    Err("cancelled".into())
                } else {
                    Ok(())
                }
            })
            .unwrap_err(),
            "cancelled"
        );
        assert_eq!(step, cancel_at);
    }
    assert_eq!(fs::read(&source).unwrap(), inspected.source_bytes);
    assert_eq!(fs::read(&target).unwrap(), SRT.as_bytes());
}

#[cfg(unix)]
#[test]
fn revalidation_rejects_retargeted_symlinks_even_when_bytes_match() {
    use std::os::unix::fs::symlink;
    let (dir, source) =
        fixture("[VisualSubsync]\nVideoSource=clip.mkv\nSubtitlesFile=selected.srt");
    let subtitle = dir.path().join("captions.srt");
    let selected = dir.path().join("selected.srt");
    symlink(&subtitle, &selected).unwrap();
    let project_link = dir.path().join("selected.vssprj");
    symlink(&source, &project_link).unwrap();
    let inspected = inspect(&project_link, &Overrides::default(), || Ok(())).unwrap();
    assert!(inspected.ready());
    verify_text_inputs(&inspected, || Ok(())).unwrap();
    let other_project = dir.path().join("other.vssprj");
    fs::write(&other_project, &inspected.source_bytes).unwrap();
    fs::remove_file(&project_link).unwrap();
    symlink(other_project, &project_link).unwrap();
    assert!(
        verify_text_inputs(&inspected, || Ok(()))
            .unwrap_err()
            .contains("Legacy project changed")
    );
    fs::remove_file(&project_link).unwrap();
    symlink(&source, &project_link).unwrap();
    let other_subtitle = dir.path().join("other.srt");
    fs::write(&other_subtitle, SRT).unwrap();
    fs::remove_file(&selected).unwrap();
    symlink(other_subtitle, &selected).unwrap();
    assert!(
        verify_text_inputs(&inspected, || Ok(()))
            .unwrap_err()
            .contains("Subtitles changed")
    );
    assert_eq!(fs::read(source).unwrap(), inspected.source_bytes);
    assert_eq!(fs::read(subtitle).unwrap(), SRT.as_bytes());
}

#[test]
fn source_size_regular_file_and_explicit_project_type_are_enforced() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("large.vssprj");
    fs::write(&source, vec![b' '; MAX_BYTES + 1]).unwrap();
    assert!(
        matches!(inspect(&source, &Overrides::default(), || Ok(())), Err(e) if e.contains("256 KiB"))
    );
    let folder = dir.path().join("folder.vssprj");
    fs::create_dir(&folder).unwrap();
    assert!(
        matches!(inspect(&folder, &Overrides::default(), || Ok(())), Err(e) if e.contains("regular file"))
    );
    let other = dir.path().join("not-a-project.ini");
    fs::write(&other, "[VisualSubsync]").unwrap();
    assert!(
        matches!(inspect(&other, &Overrides::default(), || Ok(())), Err(e) if e.contains(".vssprj"))
    );
    let upper = dir.path().join("empty.VSSPRJ");
    fs::write(&upper, "[VisualSubsync]").unwrap();
    let empty = inspect(&upper, &Overrides::default(), || Ok(())).unwrap();
    assert!(!empty.ready());
    assert!(empty.media.issue.unwrap().contains("No file"));
    assert!(empty.subtitles.issue.unwrap().contains("No file"));
}
