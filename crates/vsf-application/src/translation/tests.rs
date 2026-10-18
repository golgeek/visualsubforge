use super::*;
use subtitle_core::{Edit, Editor};
use subtitle_formats::srt;

fn ms(value: u64) -> Milliseconds {
    Milliseconds::new(value).unwrap()
}
fn page(offset: usize, limit: usize) -> PageRequest {
    PageRequest { offset, limit }
}
fn srt_bytes(intervals: &[(u64, u64)]) -> Vec<u8> {
    intervals
        .iter()
        .enumerate()
        .map(|(i, &(start, end))| {
            format!(
                "{}\n{} --> {}\nReference {i} 世界 😀\n\n",
                i + 1,
                srt::format_time(ms(start)),
                srt::format_time(ms(end)),
            )
        })
        .collect::<String>()
        .into_bytes()
}
fn reference(intervals: &[(u64, u64)]) -> Reference {
    Reference::parse(srt_bytes(intervals), Some("srt"), || Ok(())).unwrap()
}
fn indices(p: &Page<'_>) -> Vec<usize> {
    p.rows.iter().map(|r| r.source_index).collect()
}

#[test]
fn every_format_keeps_an_immutable_reference_and_independent_editable_copy() {
    for (extension, bytes) in [
        (
            "srt",
            "\u{feff}7\r\n00:00:01,001 --> 00:00:03,999\r\n{\\an8}<i>World 世界</i>\r\n\r\n"
                .as_bytes(),
        ),
        (
            "ass",
            include_bytes!("../../../../tests/fixtures/subtitles/styles.ass").as_slice(),
        ),
        (
            "ssa",
            include_bytes!("../../../../tests/fixtures/subtitles/styles.ssa").as_slice(),
        ),
        (
            "vtt",
            include_bytes!("../../../../tests/fixtures/subtitles/styles.vtt").as_slice(),
        ),
    ] {
        let reference = Reference::parse(bytes.to_vec(), Some(extension), || Ok(())).unwrap();
        let shared = reference.clone();
        assert!(Arc::ptr_eq(&reference.0, &shared.0));
        let fingerprint = reference.fingerprint().to_owned();
        let original = reference.cues().to_vec();
        assert_eq!(reference.format().extension(), extension);
        let target = reference.copy_target();
        assert_eq!(
            target
                .source
                .serialize(&target.document)
                .unwrap()
                .as_bytes(),
            bytes
        );
        let mut editor = Editor::with_properties(target.document, target.source);
        let cue = editor.document().cues()[0].clone();
        editor
            .apply_edit_with_properties(
                0,
                Edit::Text {
                    id: cue.id(),
                    start: cue.start(),
                    end: cue.end(),
                    text: format!("{} traduit", cue.text()),
                },
                crate::subtitle_file::validate_document,
            )
            .unwrap();
        assert_ne!(
            editor.document().cues()[0].text(),
            reference.cues()[0].text()
        );
        let output = editor.properties().serialize(editor.document()).unwrap();
        let reparsed =
            crate::subtitle_file::parse_document(output.as_bytes(), Some(extension)).unwrap();
        assert_eq!(reparsed.document.cues(), editor.document().cues());
        assert_eq!(reference.cues(), original);
        assert_eq!(reference.original_bytes(), bytes);
        assert_eq!(shared.fingerprint(), fingerprint);
        editor.undo(editor.revision()).unwrap();
        assert_eq!(
            editor
                .properties()
                .serialize(editor.document())
                .unwrap()
                .as_bytes(),
            bytes
        );
        editor.redo(editor.revision()).unwrap();
        assert_eq!(
            editor.properties().serialize(editor.document()).unwrap(),
            output
        );
    }
}

#[test]
fn overlap_is_half_open_not_row_position_and_handles_unsorted_nested_cues() {
    let r = reference(&[
        (2000, 3000),
        (0, 10000),
        (1000, 2000),
        (2000, 2500),
        (4000, 5000),
    ]);
    let overlaps = r
        .matches(ms(2000), ms(2400), Offset::default(), page(0, 200), || {
            Ok(())
        })
        .unwrap();
    assert_eq!(indices(&overlaps), [1, 0, 3]);
    assert_eq!(overlaps.total, 3);
    assert!(overlaps.rows.iter().all(|r| r.overlap_ms == Some(400)));
    assert_eq!(
        indices(
            &r.matches(ms(2000), ms(2400), Offset::default(), page(1, 1), || Ok(()))
                .unwrap()
        ),
        [0]
    );
    assert_eq!(
        indices(
            &r.browse(Offset::default(), page(0, 200), || Ok(()))
                .unwrap()
        ),
        [0, 1, 2, 3, 4]
    );
    assert_eq!(
        indices(
            &r.matches(ms(5000), ms(10000), Offset::default(), page(0, 200), || Ok(
                ()
            ))
            .unwrap()
        ),
        [1]
    );
    assert_eq!(
        r.matches(
            ms(10000),
            ms(11000),
            Offset::default(),
            page(0, 200),
            || Ok(())
        )
        .unwrap()
        .total,
        0
    );
    let gap = reference(&[(1000, 2000), (4000, 5000)]);
    assert_eq!(
        gap.matches(ms(2000), ms(4000), Offset::default(), page(0, 200), || Ok(
            ()
        ))
        .unwrap()
        .total,
        0
    );
}

#[test]
fn signed_offsets_do_not_clamp_rewrite_or_overflow() {
    let r = reference(&[
        (0, 1000),
        (1000, 2000),
        (Milliseconds::MAX - 1000, Milliseconds::MAX),
    ]);
    let original = r.original_bytes().to_vec();
    let shifted = r
        .matches(
            ms(0),
            ms(1000),
            Offset::new(-1500).unwrap(),
            page(0, 200),
            || Ok(()),
        )
        .unwrap();
    assert_eq!(indices(&shifted), [1]);
    assert_eq!(
        (
            shifted.rows[0].mapped_start_ms,
            shifted.rows[0].mapped_end_ms,
            shifted.rows[0].overlap_ms
        ),
        (-500, 500, Some(500))
    );
    let positive = Offset::new(Milliseconds::MAX as i64).unwrap();
    assert_eq!(
        positive.map(ms(Milliseconds::MAX)),
        2 * Milliseconds::MAX as i64
    );
    assert_eq!(
        r.matches(ms(0), ms(Milliseconds::MAX), positive, page(0, 200), || Ok(
            ()
        ))
        .unwrap()
        .total,
        0
    );
    let negative = Offset::new(-(Milliseconds::MAX as i64)).unwrap();
    assert_eq!(
        r.matches(ms(0), ms(1), negative, page(0, 200), || Ok(()))
            .unwrap()
            .total,
        0
    );
    for invalid in [
        i64::MIN,
        i64::MAX,
        Milliseconds::MAX as i64 + 1,
        -(Milliseconds::MAX as i64) - 1,
    ] {
        assert!(Offset::new(invalid).is_err());
    }
    assert_eq!(r.original_bytes(), original);
}

#[test]
fn indexed_queries_agree_with_an_independent_brute_force_oracle() {
    let mut seed = 73_u64;
    let mut next = || {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        seed >> 32
    };
    let intervals: Vec<_> = (0..2000)
        .map(|_| {
            let start = next() % 100_000;
            (start, start + 1 + next() % 15_000)
        })
        .collect();
    let r = reference(&intervals);
    for _ in 0..100 {
        let start = next() % 100_000;
        let end = start + 1 + next() % 20_000;
        let offset = next() as i64 % 30_000 - 15_000;
        let skip = (next() % 250) as usize;
        let limit = (next() % 200 + 1) as usize;
        let mut expected: Vec<_> = intervals
            .iter()
            .enumerate()
            .filter_map(|(i, &(a, b))| {
                let shifted_start = a as i64 + offset;
                let shifted_end = b as i64 + offset;
                let overlap = shifted_end.min(end as i64) - shifted_start.max(start as i64);
                (overlap > 0).then_some((a, i, overlap as u64))
            })
            .collect();
        expected.sort_by_key(|&(start, i, _)| (start, i));
        let actual = r
            .matches(
                ms(start),
                ms(end),
                Offset::new(offset).unwrap(),
                page(skip, limit),
                || Ok(()),
            )
            .unwrap();
        assert_eq!(actual.total, expected.len());
        assert_eq!(
            actual
                .rows
                .iter()
                .map(|row| (row.source_index, row.overlap_ms.unwrap()))
                .collect::<Vec<_>>(),
            expected
                .iter()
                .skip(skip)
                .take(limit)
                .map(|&(_, i, overlap)| (i, overlap))
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn dense_paging_retains_all_matches_and_cancellation_is_non_mutating() {
    let r = reference(&vec![(1000, 5000); 10000]);
    let mut all = vec![];
    for offset in (0..10000).step_by(200) {
        let result = r
            .matches(
                ms(2000),
                ms(3000),
                Offset::default(),
                page(offset, 200),
                || Ok(()),
            )
            .unwrap();
        assert_eq!(result.total, 10000);
        assert_eq!(result.rows.len(), 200);
        all.extend(indices(&result));
    }
    assert_eq!(all, (0..10000).collect::<Vec<_>>());
    let mut checkpoints = 0;
    let cancelled = r.matches(ms(2000), ms(3000), Offset::default(), page(0, 200), || {
        checkpoints += 1;
        if checkpoints == 3 {
            Err("cancelled".into())
        } else {
            Ok(())
        }
    });
    assert_eq!(cancelled.unwrap_err(), "cancelled");
    assert_eq!(
        r.matches(
            ms(2000),
            ms(3000),
            Offset::default(),
            page(9999, 200),
            || Ok(())
        )
        .unwrap()
        .rows
        .len(),
        1
    );
    assert_eq!(r.cues().len(), 10000);
}

#[test]
fn invalid_pages_intervals_and_cancelled_preparation_return_errors() {
    let r = reference(&[(1000, 2000)]);
    for invalid in [page(0, 0), page(0, MAX_PAGE_ROWS + 1)] {
        assert!(r.browse(Offset::default(), invalid, || Ok(())).is_err());
        assert!(
            r.matches(ms(0), ms(1000), Offset::default(), invalid, || Ok(()))
                .is_err()
        );
    }
    for (start, end) in [(1, 1), (2, 1)] {
        assert!(
            r.matches(ms(start), ms(end), Offset::default(), page(0, 1), || Ok(()))
                .is_err()
        );
    }
    assert!(
        r.browse(Offset::default(), page(usize::MAX, 200), || Ok(()))
            .unwrap()
            .rows
            .is_empty()
    );
    assert!(
        r.matches(
            ms(1000),
            ms(2000),
            Offset::default(),
            page(usize::MAX, 200),
            || Ok(())
        )
        .unwrap()
        .rows
        .is_empty()
    );
    assert_eq!(
        Reference::parse(srt_bytes(&[(0, 1)]), Some("srt"), || Err(
            "cancelled before parse".into()
        ))
        .unwrap_err()
        .to_string(),
        "cancelled before parse"
    );
    let mut checks = 0;
    let cancelled = Reference::parse(srt_bytes(&[(0, 1)]), Some("srt"), || {
        checks += 1;
        if checks == 2 {
            Err("cancelled after parse".into())
        } else {
            Ok(())
        }
    });
    assert_eq!(cancelled.unwrap_err().to_string(), "cancelled after parse");
    assert!(Reference::parse(vec![0xff], Some("srt"), || Ok(())).is_err());
    assert!(Reference::parse(b"not WebVTT".to_vec(), Some("vtt"), || Ok(())).is_err());
}

#[test]
fn reading_projection_preserves_format_distinctions_and_exposes_limits() {
    let srt = b"1\n00:00:01,000 --> 00:00:03,000\n{\\an8\\k20}<i>Bonjour</i>\\NWorld\n\n";
    let r = Reference::parse(srt.to_vec(), Some("srt"), || Ok(())).unwrap();
    let reading = r.reading(0).unwrap();
    assert_eq!(reading.text, "Bonjour\nWorld");
    assert!(reading.timed);
    assert_eq!(r.original_bytes(), srt);
    let vtt = "WEBVTT\n\n00:00:01.000 --> 00:00:03.000\n<v Alice><b>Été &amp; 世界</b></v> <00:00:02.000>{\\an8}literal\n";
    let r = Reference::parse(vtt.as_bytes().to_vec(), Some("vtt"), || Ok(())).unwrap();
    let reading = r.reading(0).unwrap();
    assert_eq!(reading.text, "Été & 世界 {\\an8}literal");
    assert!(reading.timed);
    assert_eq!(r.original_bytes(), vtt.as_bytes());
    let unknown = "1\n00:00:01,000 --> 00:00:03,000\n<img src=x onerror=alert(1)>世界\n\n";
    let r = Reference::parse(unknown.as_bytes().to_vec(), Some("srt"), || Ok(())).unwrap();
    assert_eq!(
        r.reading(0).unwrap().text,
        "<img src=x onerror=alert(1)>世界"
    );
    assert!(r.reading(0).unwrap().warning.is_some());
    assert!(r.reading(1).is_err());
    let long = format!(
        "1\n00:00:01,000 --> 00:00:03,000\n{}\n\n",
        "世".repeat(MAX_CUE_BYTES / 3 + 1)
    );
    let r = Reference::parse(long.as_bytes().to_vec(), Some("srt"), || Ok(())).unwrap();
    assert!(r.reading(0).is_err());
    assert_eq!(r.original_bytes(), long.as_bytes());
}

#[test]
fn exact_byte_identity_keeps_encoding_and_line_ending_differences() {
    let bytes = srt_bytes(&[(1000, 2000)]);
    let plain = Reference::parse(bytes.clone(), Some("srt"), || Ok(())).unwrap();
    let crlf = String::from_utf8(bytes.clone())
        .unwrap()
        .replace('\n', "\r\n")
        .into_bytes();
    let windows = Reference::parse(crlf, Some("srt"), || Ok(())).unwrap();
    let mut bom = vec![0xef, 0xbb, 0xbf];
    bom.extend(&bytes);
    let marked = Reference::parse(bom, Some("srt"), || Ok(())).unwrap();
    assert_eq!(plain.cues(), windows.cues());
    assert_eq!(plain.cues(), marked.cues());
    assert_ne!(plain.fingerprint(), windows.fingerprint());
    assert_ne!(plain.fingerprint(), marked.fingerprint());
    assert_eq!(plain.fingerprint().len(), 64);
}
