use vsf_application::authoring::fonts::{self, FontKind};
const TTF: &[u8] = include_bytes!("../../../tests/fixtures/subtitles/fixture-font.ttf");
const OTF: &[u8] = include_bytes!("../../../tests/fixtures/subtitles/fixture-font-cff.otf");
const CFF2: &[u8] = include_bytes!("../../../tests/fixtures/subtitles/fixture-font-cff2.otf");
const ASS: &str = include_str!("../../../tests/fixtures/subtitles/embedded-font.ass");

#[test]
fn original_font_fixture_decodes_exactly_and_has_valid_structural_metadata() {
    let parsed = subtitle_formats::ass::parse(ASS).unwrap();
    let inventory = parsed.source.attachments(200);
    assert_eq!(inventory.total, 1);
    let decoded = fonts::decode(&parsed.source, inventory.entries[0].id, || Ok(())).unwrap();
    assert_eq!(decoded, TTF);
    for (font, kind) in [
        (TTF, FontKind::TrueType),
        (OTF, FontKind::OpenTypeCff),
        (CFF2, FontKind::OpenTypeCff2),
    ] {
        let info = fonts::inspect(font, || Ok(())).unwrap();
        assert_eq!(info.kind, kind);
        assert_eq!(info.family.as_deref(), Some("VSF Fixture Font"));
        assert_eq!(info.glyphs, 3);
    }
    let encoded = fonts::encode(TTF, || Ok(())).unwrap();
    assert!(encoded.lines().all(|line| line.len() <= 80));
    let copy = parsed.source.with_font(None, "Copy.ttf", &encoded).unwrap();
    let id = copy.attachments(200).entries[1].id;
    assert_eq!(fonts::decode(&copy, id, || Ok(())).unwrap(), TTF);
    assert_eq!(
        subtitle_formats::ass::serialize(&parsed.document, &parsed.source).unwrap(),
        ASS
    );
}

#[test]
fn codec_all_remainders_invalid_padding_bounds_and_cancellation() {
    let parsed = subtitle_formats::ass::parse(ASS).unwrap();
    let id = parsed.source.attachments(1).entries[0].id;
    for length in [1, 2, 3, 4, 59, 60, 61, 257, 4097] {
        let bytes: Vec<_> = (0..length).map(|i| (i % 256) as u8).collect();
        let encoded = fonts::encode(&bytes, || Ok(())).unwrap();
        let source = parsed
            .source
            .with_font(Some(id), "Data.ttf", &encoded)
            .unwrap();
        assert_eq!(fonts::decode(&source, id, || Ok(())).unwrap(), bytes);
    }
    for encoded in ["!", "!!\"", "!\""] {
        let source = parsed
            .source
            .with_font(Some(id), "Bad.ttf", encoded)
            .unwrap();
        assert!(fonts::decode(&source, id, || Ok(())).is_err());
    }
    assert!(fonts::encode(&[], || Ok(())).is_err());
    assert!(fonts::encode(&vec![0; fonts::MAX_FONT_BYTES + 1], || Ok(())).is_err());
    let huge = format!(
        "{}fontname: Large.ttf\n{}",
        ASS,
        "!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!\n"
            .repeat(fonts::MAX_FONT_BYTES / 60 + 1)
    );
    let huge = subtitle_formats::ass::parse(&huge).unwrap();
    let large = huge.source.attachments(200).entries[1].id;
    assert!(
        fonts::decode(&huge.source, large, || Ok(()))
            .unwrap_err()
            .contains("4 MiB")
    );
    assert_eq!(
        fonts::decode(&parsed.source, id, || Err("Cancelled".into())),
        Err("Cancelled".into())
    );
    assert_eq!(
        fonts::encode(TTF, || Err("Cancelled".into())),
        Err("Cancelled".into())
    );
    assert_eq!(
        fonts::inspect(TTF, || Err("Cancelled".into())),
        Err("Cancelled".into())
    );
}

#[test]
fn sfnt_rejects_collections_truncation_invalid_tables_checksums_and_trailing_data() {
    for length in [0, 1, 4, 11, 12, 24, TTF.len() - 1] {
        assert!(fonts::inspect(&TTF[..length], || Ok(())).is_err());
    }
    let mut collection = TTF.to_vec();
    collection[..4].copy_from_slice(b"ttcf");
    assert!(
        fonts::inspect(&collection, || Ok(()))
            .unwrap_err()
            .contains("collections")
    );
    for (at, value) in [
        (4, 0xff),
        (6, 0xff),
        (12, 0),
        (20, 0xff),
        (24, 0xff),
        (TTF.len() / 2, 0xff),
    ] {
        let mut invalid = TTF.to_vec();
        invalid[at] = value;
        assert!(fonts::inspect(&invalid, || Ok(())).is_err(), "offset {at}");
    }
    let mut trailing = TTF.to_vec();
    trailing.extend_from_slice(b"extra!");
    assert!(fonts::inspect(&trailing, || Ok(())).is_err());
    assert!(fonts::inspect(TTF, || Ok(())).is_ok());
}
