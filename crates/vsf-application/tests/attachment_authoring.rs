use subtitle_core::Editor;
use subtitle_formats::Source;
use vsf_application::{
    authoring::{
        attachments::{self, AttachmentAction, FontEdit},
        fonts,
    },
    subtitle_file::validate_document,
};

const TTF: &[u8] = include_bytes!("../../../tests/fixtures/subtitles/fixture-font.ttf");
const OTF: &[u8] = include_bytes!("../../../tests/fixtures/subtitles/fixture-font-cff.otf");
const CFF2: &[u8] = include_bytes!("../../../tests/fixtures/subtitles/fixture-font-cff2.otf");
const ASS: &str = include_str!("../../../tests/fixtures/subtitles/styles.ass");
const SSA: &str = include_str!("../../../tests/fixtures/subtitles/styles.ssa");

fn put<'a>(replace: Option<u32>, name: &'a str, bytes: &'a [u8]) -> FontEdit<'a> {
    FontEdit::Put {
        replace,
        name,
        bytes,
    }
}
fn with_font(base: &str, name: &str, bytes: &[u8]) -> String {
    format!(
        "{base}[Fonts]\nfontname: {name}\n{}\n",
        fonts::encode(bytes, || Ok(())).unwrap()
    )
}
fn inventory(source: &Source) -> attachments::AttachmentInventory {
    attachments::inspect(source, || Ok(())).unwrap()
}

#[test]
fn font_changes_preserve_unrelated_source_and_commit_as_one_history_entry() {
    for base in [ASS, SSA] {
        let original = with_font(
            &format!("\u{feff}{base}[Graphics]\nfilename: opaque.bmp\nOPAQUE!\n"),
            "Original.ttf",
            TTF,
        )
        .replace('\n', "\r\n");
        let parsed = subtitle_formats::parse(&original, None).unwrap();
        let list = inventory(&parsed.source);
        assert!(list.complete);
        assert_eq!(list.total, 2);
        assert!(
            list.entries[0]
                .problem
                .as_deref()
                .unwrap()
                .contains("read-only")
        );
        assert_eq!(list.font_bytes_upper_bound, TTF.len() as u32);
        assert_eq!(list.entries[1].decoded_bytes, Some(TTF.len() as u32));
        let id = list.entries[1].id;
        let replacement = attachments::propose(
            &parsed.document,
            &parsed.source,
            put(Some(id), "Changed.otf", OTF),
            || Ok(()),
        )
        .unwrap();
        assert!(replacement.changed);
        assert_eq!(replacement.summary.action, AttachmentAction::Replace);
        assert_eq!(
            replacement.summary.before.as_ref().unwrap().name,
            "Original.ttf"
        );
        assert_eq!(
            replacement.summary.after.as_ref().unwrap().decoded_bytes,
            Some(OTF.len() as u32)
        );
        let expected = original.replace(
            &format!(
                "fontname: Original.ttf\r\n{}\r\n",
                fonts::encode(TTF, || Ok(())).unwrap().replace('\n', "\r\n")
            ),
            &format!(
                "fontname: Changed.otf\r\n{}\r\n",
                fonts::encode(OTF, || Ok(())).unwrap().replace('\n', "\r\n")
            ),
        );
        assert_eq!(replacement.document(), &parsed.document);
        assert_eq!(
            replacement
                .source()
                .serialize(replacement.document())
                .unwrap(),
            expected
        );
        assert_eq!(replacement.summary.document_bytes as usize, expected.len());
        let mut editor = Editor::with_properties(parsed.document.clone(), parsed.source.clone());
        let (document, source) = replacement.into_parts();
        editor
            .apply_document(0, document, source, validate_document)
            .unwrap();
        editor.undo(1).unwrap();
        assert!(!editor.dirty());
        assert_eq!(
            editor.properties().serialize(editor.document()).unwrap(),
            original
        );
        let unchanged = attachments::propose(
            editor.document(),
            editor.properties(),
            put(Some(id), "Original.ttf", TTF),
            || Ok(()),
        )
        .unwrap();
        assert!(!unchanged.changed);
        let (document, source) = unchanged.into_parts();
        editor
            .apply_document(2, document, source, validate_document)
            .unwrap();
        assert!(editor.can_redo());
        assert_eq!(editor.revision(), 2);
        editor.redo(2).unwrap();
        assert_eq!(
            editor.properties().serialize(editor.document()).unwrap(),
            expected
        );
        let added = attachments::propose(
            editor.document(),
            editor.properties(),
            put(None, "Extra.otf", CFF2),
            || Ok(()),
        )
        .unwrap();
        assert_eq!(added.summary.action, AttachmentAction::Add);
        let added_id = added.summary.after.as_ref().unwrap().id;
        let (document, source) = added.into_parts();
        editor
            .apply_document(3, document, source, validate_document)
            .unwrap();
        let removed = attachments::propose(
            editor.document(),
            editor.properties(),
            FontEdit::Remove { id: added_id },
            || Ok(()),
        )
        .unwrap();
        assert_eq!(removed.summary.action, AttachmentAction::Remove);
        assert!(removed.summary.after.is_none());
        assert_eq!(
            removed.source().serialize(removed.document()).unwrap(),
            expected
        );
        let (document, source) = removed.into_parts();
        editor
            .apply_document(4, document, source, validate_document)
            .unwrap();
        editor.undo(5).unwrap();
        assert_eq!(inventory(editor.properties()).total, 3);
        editor.redo(6).unwrap();
        let saved = editor.properties().serialize(editor.document()).unwrap();
        let reopened = subtitle_formats::parse(&saved, None).unwrap();
        assert_eq!(reopened.document, parsed.document);
        assert_eq!(
            reopened.source.serialize(&reopened.document).unwrap(),
            expected
        );
        assert_eq!(
            inventory(&reopened.source).entries[1].sha256,
            inventory(editor.properties()).entries[1].sha256
        );
    }
}

#[test]
fn unsupported_or_partial_inventories_remain_preserved_and_read_only() {
    let original = format!(
        "{}fontname: Broken.ttf\n!!\"\n[Fonts]\n[Graphics]\nfilename: opaque.bmp\nDATA\n",
        with_font(ASS, "Good.ttf", TTF)
    );
    let parsed = subtitle_formats::parse(&original, None).unwrap();
    let list = inventory(&parsed.source);
    assert_eq!(list.total, 3);
    assert!(list.entries[1].problem.is_some());
    for edit in [
        put(None, "New.ttf", TTF),
        put(Some(list.entries[0].id), "New.ttf", TTF),
        FontEdit::Remove {
            id: list.entries[1].id,
        },
        FontEdit::Remove {
            id: list.entries[2].id,
        },
        FontEdit::Remove { id: u32::MAX },
    ] {
        assert!(attachments::propose(&parsed.document, &parsed.source, edit, || Ok(())).is_err());
    }
    // A supported block can be deliberately removed without modifying opaque peers.
    let removed = attachments::propose(
        &parsed.document,
        &parsed.source,
        FontEdit::Remove {
            id: list.entries[0].id,
        },
        || Ok(()),
    )
    .unwrap();
    let encoded = removed.source().serialize(removed.document()).unwrap();
    assert!(
        encoded.contains(
            "fontname: Broken.ttf\n!!\"\n[Fonts]\n[Graphics]\nfilename: opaque.bmp\nDATA\n"
        )
    );
    assert_eq!(parsed.source.serialize(&parsed.document).unwrap(), original);

    let many = format!(
        "{ASS}[Graphics]\n{}",
        (0..201)
            .map(|i| format!("filename: {i}.bmp\n!!!!\n"))
            .collect::<String>()
    );
    let parsed = subtitle_formats::parse(&many, None).unwrap();
    let list = inventory(&parsed.source);
    assert_eq!(
        (list.total, list.entries.len(), list.complete),
        (201, 200, false)
    );
    assert!(
        attachments::propose(
            &parsed.document,
            &parsed.source,
            put(None, "New.ttf", TTF),
            || Ok(())
        )
        .err()
        .unwrap()
        .contains("200")
    );
    let srt = subtitle_formats::parse("1\n00:00:00,000 --> 00:00:01,000\nText\n", None).unwrap();
    assert!(attachments::inspect(&srt.source, || Ok(())).is_err());
    assert!(
        attachments::propose(
            &srt.document,
            &srt.source,
            put(None, "New.ttf", TTF),
            || Ok(())
        )
        .is_err()
    );
}

#[test]
fn invalid_inputs_and_transient_cancellation_never_produce_partial_proposals() {
    let original = with_font(ASS, "Original.ttf", TTF);
    let parsed = subtitle_formats::parse(&original, None).unwrap();
    for name in [
        "bad.bin",
        "bad.ttc",
        "../bad.ttf",
        "bad:font.ttf",
        " bad.ttf",
        "bad\n.ttf",
        "ORIGINAL.TTF",
    ] {
        assert!(
            attachments::propose(
                &parsed.document,
                &parsed.source,
                put(None, name, TTF),
                || Ok(())
            )
            .is_err(),
            "{name}"
        );
    }
    assert!(
        attachments::propose(
            &parsed.document,
            &parsed.source,
            put(None, "bad.ttf", b"bad font"),
            || Ok(())
        )
        .is_err()
    );
    let mut calls = 0;
    attachments::inspect(&parsed.source, || {
        calls += 1;
        Ok(())
    })
    .unwrap();
    // A one-shot interruption anywhere inside either parser must abort, rather
    // than becoming an unsupported row that a later checkpoint could hide.
    for stop in 1..=calls {
        let mut count = 0;
        assert_eq!(
            attachments::inspect(&parsed.source, || {
                count += 1;
                if count == stop {
                    Err("interrupted".into())
                } else {
                    Ok(())
                }
            }),
            Err("interrupted".into()),
            "checkpoint {stop}"
        );
    }
    let mut calls = 0;
    attachments::propose(
        &parsed.document,
        &parsed.source,
        put(None, "new.otf", OTF),
        || {
            calls += 1;
            Ok(())
        },
    )
    .unwrap();
    for stop in 1..=calls {
        let mut count = 0;
        assert_eq!(
            attachments::propose(
                &parsed.document,
                &parsed.source,
                put(None, "new.otf", OTF),
                || {
                    count += 1;
                    if count == stop {
                        Err("interrupted".into())
                    } else {
                        Ok(())
                    }
                }
            )
            .err(),
            Some("interrupted".into()),
            "checkpoint {stop}"
        );
    }
    assert_eq!(parsed.source.serialize(&parsed.document).unwrap(), original);
}

// A valid fixture with one additional opaque zero-filled table. Rebuilding the
// directory and checksums keeps large-size tests independent of invalid-font
// rejection and avoids storing multi-megabyte fixtures in Git.
fn large_font(length: usize) -> Vec<u8> {
    fn u32_at(bytes: &[u8], at: usize) -> usize {
        u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as usize
    }
    fn sum(bytes: &[u8]) -> u32 {
        bytes.chunks(4).fold(0u32, |sum, part| {
            let mut word = [0; 4];
            word[..part.len()].copy_from_slice(part);
            sum.wrapping_add(u32::from_be_bytes(word))
        })
    }
    let old_count = u16::from_be_bytes(TTF[4..6].try_into().unwrap()) as usize;
    let mut tables: Vec<([u8; 4], Vec<u8>)> = (0..old_count)
        .map(|i| {
            let at = 12 + i * 16;
            let offset = u32_at(TTF, at + 8);
            let length = u32_at(TTF, at + 12);
            (
                TTF[at..at + 4].try_into().unwrap(),
                TTF[offset..offset + length].to_vec(),
            )
        })
        .collect();
    let count = old_count + 1;
    let base_len = 12
        + count * 16
        + tables
            .iter()
            .map(|(_, data)| data.len().next_multiple_of(4))
            .sum::<usize>();
    tables.push((*b"ZZZZ", vec![0; length - base_len]));
    tables.sort_by_key(|(tag, _)| *tag);
    let mut out = vec![0u8; 12 + count * 16];
    out[..4].copy_from_slice(&TTF[..4]);
    for (at, value) in [
        (4, count),
        (6, (1 << count.ilog2()) * 16),
        (8, count.ilog2() as usize),
        (10, count * 16 - (1 << count.ilog2()) * 16),
    ] {
        out[at..at + 2].copy_from_slice(&(value as u16).to_be_bytes());
    }
    let mut head = 0;
    for (i, (tag, mut bytes)) in tables.into_iter().enumerate() {
        if &tag == b"head" {
            bytes[8..12].fill(0);
            head = out.len();
        }
        let at = 12 + i * 16;
        let offset = out.len();
        out[at..at + 4].copy_from_slice(&tag);
        for (at, value) in [
            (at + 4, sum(&bytes)),
            (at + 8, offset as u32),
            (at + 12, bytes.len() as u32),
        ] {
            out[at..at + 4].copy_from_slice(&value.to_be_bytes());
        }
        out.extend_from_slice(&bytes);
        out.resize(out.len().next_multiple_of(4), 0);
    }
    let adjustment = 0xb1b0_afbau32.wrapping_sub(sum(&out));
    out[head + 8..head + 12].copy_from_slice(&adjustment.to_be_bytes());
    assert_eq!(out.len(), length);
    fonts::inspect(&out, || Ok(())).unwrap();
    out
}

#[test]
fn font_count_aggregate_and_document_limits_are_independent() {
    let parsed = subtitle_formats::parse(ASS, None).unwrap();
    let encoded = fonts::encode(TTF, || Ok(())).unwrap();
    let mut source = parsed.source.ass().unwrap().clone();
    for i in 0..32 {
        source = source
            .with_font(None, &format!("{i}.ttf"), &encoded)
            .unwrap();
    }
    assert!(
        attachments::propose(
            &parsed.document,
            &Source::Ass(source),
            put(None, "extra.ttf", TTF),
            || Ok(())
        )
        .err()
        .unwrap()
        .contains("32")
    );

    let large = large_font(fonts::MAX_FONT_BYTES);
    let first = attachments::propose(
        &parsed.document,
        &parsed.source,
        put(None, "large.ttf", &large),
        || Ok(()),
    )
    .unwrap();
    assert!(first.summary.preview_size_exceeded);
    assert_eq!(
        inventory(first.source()).font_bytes_upper_bound as usize,
        large.len()
    );
    let second = attachments::propose(
        first.document(),
        first.source(),
        put(None, "second.ttf", &large),
        || Ok(()),
    )
    .unwrap();
    assert_eq!(
        inventory(second.source()).font_bytes_upper_bound as usize,
        fonts::MAX_TOTAL_FONT_BYTES
    );
    assert!(
        attachments::propose(
            second.document(),
            second.source(),
            put(None, "extra.ttf", TTF),
            || Ok(())
        )
        .err()
        .unwrap()
        .contains("8 MiB")
    );
    let id = second.summary.after.as_ref().unwrap().id;
    assert!(
        attachments::propose(
            second.document(),
            second.source(),
            put(Some(id), "second.ttf", &large),
            || Ok(())
        )
        .is_ok()
    );

    let oversized = format!(
        ";{}\n{ASS}",
        "x".repeat(vsf_application::srt_limits::MAX_BYTES - ASS.len() - 500)
    );
    let parsed = subtitle_formats::parse(&oversized, None).unwrap();
    validate_document(&parsed.document, &parsed.source).unwrap();
    assert!(
        attachments::propose(
            &parsed.document,
            &parsed.source,
            put(None, "extra.ttf", TTF),
            || Ok(())
        )
        .err()
        .unwrap()
        .contains("16 MiB")
    );
    assert_eq!(
        parsed.source.serialize(&parsed.document).unwrap(),
        oversized
    );
}
