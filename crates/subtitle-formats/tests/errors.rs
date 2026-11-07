use subtitle_core::{CueId, Edit, Editor, Milliseconds};
use subtitle_formats::{Error, Format, parse, srt};

#[test]
fn parse_failures_retain_line_timestamp_and_domain_context() {
    let error =
        srt::parse("1\n00:00:01,000 --> 00:00:02,000\nA\n\n2\n00:60:00,000 --> 00:00:03,000\nB")
            .unwrap_err();
    assert_eq!(
        error,
        Error::SrtLine {
            line: 5,
            source: Box::new(Error::SrtTimestampComponent {
                text: "00:60:00,000".into()
            })
        }
    );
    // Intentional desktop/CLI wording contract.
    assert_eq!(
        error.to_string(),
        "SRT line 5: Timestamp minutes and seconds must be below 60: 00:60:00,000"
    );
    assert_eq!(
        srt::parse_time("bad"),
        Err(Error::InvalidSrtTimestamp { text: "bad".into() })
    );
    let error = srt::parse("1\n00:00:02,000 --> 00:00:01,000\nA").unwrap_err();
    assert_eq!(
        error,
        Error::SrtLine {
            line: 1,
            source: Box::new(Error::Domain(subtitle_core::Error::InvalidTimeRange {
                start: Milliseconds::new(2000).unwrap(),
                end: Milliseconds::new(1000).unwrap()
            }))
        }
    );
    assert!(std::error::Error::source(&error).is_some());
}

#[test]
fn dispatch_separates_parse_validation_and_extension_errors() {
    let ass = "[Script Info]\nScriptType: v4.00+\n[V4+ Styles]\nFormat: Name, Fontname\nStyle: Default,Arial\n[Events]\nFormat: Start, End, Style, Text\nDialogue: 0:00:01.00,0:00:02.00,Default,Hi";
    assert_eq!(
        parse(ass, Some(Format::Ssa)).unwrap_err(),
        Error::FormatMismatch {
            expected: Format::Ssa,
            actual: Format::Ass
        }
    );
    let parsed = parse(ass, None).unwrap();
    assert_eq!(
        parsed.source.validate_save_extension(Some("srt")),
        Err(Error::SaveExtension {
            format: Format::Ass,
            extension: Some("srt".into())
        })
    );
    assert!(matches!(
        parse("bad", Some(Format::WebVtt)),
        Err(Error::Parse {
            format: Format::WebVtt,
            ..
        })
    ));
    assert!(matches!(
        parse("bad", Some(Format::Ass)),
        Err(Error::Parse {
            format: Format::Ass,
            ..
        })
    ));
    assert_eq!(srt::validate_text("a\n\nb"), Err(Error::SrtBlankLine));
    let vtt = parse("WEBVTT\n\n", Some(Format::WebVtt)).unwrap();
    assert!(matches!(
        vtt.source.prepare_text("a\n\nb".into()),
        Err(Error::Validation {
            format: Format::WebVtt,
            ..
        })
    ));
}

#[test]
fn format_rejection_preserves_editor_and_identity_allocation() {
    let parsed = parse("1\n00:00:01,000 --> 00:00:02,000\nA\n\n", None).unwrap();
    let mut editor = Editor::with_properties(parsed.document, parsed.source);
    let at = Milliseconds::new(1500).unwrap();
    editor.split(0, CueId(1), at).unwrap();
    editor.undo(1).unwrap();
    let before = editor.document().clone();
    let result = editor.apply_edit_with_properties(
        2,
        Edit::Create {
            after: None,
            start: Milliseconds::new(0).unwrap(),
            end: Milliseconds::new(1).unwrap(),
            text: "a\n\nb".into(),
            metadata: None,
        },
        |document, source| {
            source
                .serialized_len(document)
                .map(|_| ())
                .map_err(String::from)
        },
    );
    assert_eq!(
        result.err(),
        Some(subtitle_core::Error::ValidationRejected {
            message: Error::SrtBlankLine.to_string()
        })
    );
    assert_eq!(editor.document(), &before);
    assert_eq!(editor.revision(), 2);
    assert!(!editor.dirty());
    assert!(editor.can_redo());
    editor.redo(2).unwrap();
    editor.undo(3).unwrap();
    assert_eq!(editor.split(4, CueId(1), at).unwrap(), CueId(3));
}
