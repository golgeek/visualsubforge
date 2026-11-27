use super::*;
use subtitle_core::{CueId, Milliseconds};
fn cue(i: u64, s: u64, e: u64, text: &str) -> Cue {
    Cue::new(
        CueId(i),
        Milliseconds::new(s).unwrap(),
        Milliseconds::new(e).unwrap(),
        text.into(),
    )
    .unwrap()
}
fn run(a: &[Cue], b: &[Cue], o: Options) -> Comparison {
    compare(a, Format::Srt, b, Format::Srt, o, || Ok(())).unwrap()
}
fn kinds(c: &Comparison) -> Vec<Kind> {
    c.changes.iter().map(|c| c.kind).collect()
}
#[test]
fn classifications_tolerances_options_and_symmetry() {
    let a = vec![cue(1, 1000, 4000, "one two three")];
    assert_eq!(kinds(&run(&a, &a, Options::default())), [Kind::Unchanged]);
    assert_eq!(
        kinds(&run(
            &a,
            &[cue(1, 1000, 4000, "one new three")],
            Options::default()
        )),
        [Kind::Text]
    );
    assert_eq!(
        kinds(&run(
            &a,
            &[cue(1, 1001, 4001, "one two three")],
            Options::default()
        )),
        [Kind::Unchanged]
    );
    assert_eq!(
        kinds(&run(
            &a,
            &[cue(1, 1002, 4002, "one two three")],
            Options::default()
        )),
        [Kind::Timing]
    );
    assert_eq!(
        kinds(&run(
            &a,
            &[cue(1, 1002, 4002, "one new three")],
            Options::default()
        )),
        [Kind::TextAndTiming]
    );
    for b in [
        vec![cue(1, 1000, 2500, "one"), cue(2, 2500, 4000, "two three")],
        vec![
            cue(1, 1000, 2000, "one"),
            cue(2, 2000, 3000, "two"),
            cue(3, 3000, 4000, "three"),
        ],
    ] {
        assert_eq!(kinds(&run(&a, &b, Options::default())), [Kind::Split]);
        assert_eq!(kinds(&run(&b, &a, Options::default())), [Kind::Merged]);
    }
    assert_eq!(kinds(&run(&a, &[], Options::default())), [Kind::Removed]);
    assert_eq!(kinds(&run(&[], &a, Options::default())), [Kind::Added]);
    let b = vec![cue(1, 1000, 4000, "one\ntwo three")];
    assert_eq!(kinds(&run(&a, &b, Options::default())), [Kind::Text]);
    assert_eq!(
        kinds(&run(
            &a,
            &b,
            Options {
                ignore_line_breaks: true,
                ..Options::default()
            }
        )),
        [Kind::Unchanged]
    );
    let b = vec![cue(1, 1000, 4000, "<i>one two three</i>")];
    assert_eq!(kinds(&run(&a, &b, Options::default())), [Kind::Text]);
    assert_eq!(
        kinds(&run(
            &a,
            &b,
            Options {
                ignore_tags: true,
                ..Options::default()
            }
        )),
        [Kind::Unchanged]
    );
    let different = vec![cue(1, 90000, 92000, "unrelated")];
    let c = run(&a, &different, Options::default());
    assert!(kinds(&c).contains(&Kind::Removed));
    assert!(kinds(&c).contains(&Kind::Added));
    assert_eq!(c.statistics.total_percent, 100.0);
    assert_eq!(c.statistics.added_cues, 1);
    assert_eq!(
        run(&[], &a, Options::default()).statistics.total_percent,
        0.0
    );
}
#[test]
fn ordering_metadata_content_keys_and_word_script() {
    let a = vec![cue(1, 1000, 4000, "A"), cue(2, 1500, 3500, "B")];
    let b = vec![a[1].clone(), a[0].clone()];
    assert_eq!(
        kinds(&run(&a, &b, Options::default())),
        [Kind::Unchanged, Kind::Unchanged]
    );
    let c = run(&a, &b, Options::default());
    assert_eq!(c, run(&a, &b, Options::default()));
    let base = cue(1, 1000, 4000, "A").with_metadata(
        subtitle_core::CueMetadata::new(vec![("Style".into(), "Default".into())]).unwrap(),
    );
    let changed = base
        .clone()
        .with_metadata(base.metadata().unwrap().with_style("Title".into()).unwrap());
    assert_eq!(
        kinds(&run(&[base], &[changed], Options::default())),
        [Kind::Style]
    );
    let parts = word_diff("Hello old <i>world</i>\n!", "Hello new <i>world</i>\n!");
    assert_eq!(
        parts,
        vec![
            TokenChange {
                kind: TokenKind::Equal,
                text: "Hello ".into()
            },
            TokenChange {
                kind: TokenKind::Removed,
                text: "old".into()
            },
            TokenChange {
                kind: TokenKind::Added,
                text: "new".into()
            },
            TokenChange {
                kind: TokenKind::Equal,
                text: " <i>world</i>\n!".into()
            }
        ]
    );
    for (a, b) in [
        ("", "abc"),
        ("abc", ""),
        ("<i>a</i>", "{\\an8}b"),
        ("a b c", "a c"),
        ("x", "y"),
    ] {
        let parts = word_diff(a, b);
        assert_eq!(
            parts
                .iter()
                .filter(|p| p.kind != TokenKind::Added)
                .map(|p| p.text.as_str())
                .collect::<String>(),
            a
        );
        assert_eq!(
            parts
                .iter()
                .filter(|p| p.kind != TokenKind::Removed)
                .map(|p| p.text.as_str())
                .collect::<String>(),
            b
        );
    }
}
#[test]
fn report_escapes_text_names_and_extensions_are_explicit() {
    let a = vec![cue(1, 0, 1000, "<script>&{\\fnArial}")];
    let c = run(&[], &a, Options::default());
    let report = html(
        &c,
        &ReportOptions {
            original_format: subtitle_formats::Format::Srt,
            revised_format: subtitle_formats::Format::Srt,
            original_name: "<original>",
            revised_name: "&revised",
            date: "today",
            include_unchanged: false,
        },
        &ExtraSections::default(),
    );
    assert!(!report.contains("<script>"));
    assert!(report.contains("&lt;script&gt;&amp;{\\fnArial}"));
    assert!(report.contains("&lt;original&gt;"));
    assert!(report.contains(&format!("id=\"change-{}\"", c.changes[0].key)));
    assert_eq!(
        report,
        html(
            &c,
            &ReportOptions {
                original_format: subtitle_formats::Format::Srt,
                revised_format: subtitle_formats::Format::Srt,
                original_name: "<original>",
                revised_name: "&revised",
                date: "today",
                include_unchanged: false
            },
            &ExtraSections::default()
        )
    );
}
#[test]
fn ten_thousand_five_percent_budget_and_cancellation() {
    let a: Vec<_> = (0..10000)
        .map(|i| cue(i + 1, i * 3000, i * 3000 + 2000, &format!("Unique cue {i}")))
        .collect();
    let b: Vec<_> = a
        .iter()
        .enumerate()
        .map(|(i, c)| {
            if i % 20 == 0 {
                cue(
                    c.id().0,
                    c.start().get() + 100,
                    c.end().get() + 100,
                    &format!("Changed cue {i}"),
                )
            } else {
                c.clone()
            }
        })
        .collect();
    let start = std::time::Instant::now();
    let c = run(&a, &b, Options::default());
    let elapsed = start.elapsed();
    eprintln!("10k / 5% comparison: {elapsed:?}; budget 2 seconds");
    assert_eq!(c.statistics.changes, 500);
    assert_eq!(c.statistics.total_percent, 5.0);
    assert!(elapsed < std::time::Duration::from_secs(2));
    assert!(
        compare(
            &a,
            Format::Srt,
            &b,
            Format::Srt,
            Options::default(),
            || Err("cancelled".into())
        )
        .is_err()
    );
}

#[test]
fn global_retimes_and_interrupted_blocks() {
    for (count, spacing) in [(10000, 1000), (3000, 300)] {
        let side = |delta| {
            (0..count)
                .map(|i| {
                    cue(
                        i + 1,
                        i * spacing + delta,
                        i * spacing + spacing / 2 + delta,
                        &format!("Cue {i}"),
                    )
                })
                .collect::<Vec<_>>()
        };
        let (a, b) = (side(0), side(400));
        let start = std::time::Instant::now();
        let comparison = run(&a, &b, Options::default());
        let elapsed = start.elapsed();
        eprintln!("{count} cues at {spacing} ms, +400 ms: {elapsed:?}");
        assert!(elapsed < std::time::Duration::from_secs(2));
        assert_eq!(comparison.blocks.len(), 1);
        assert_eq!(comparison.blocks[0].end, count as u32);
    }
    let a: Vec<_> = (0..9)
        .map(|i| cue(i + 1, i * 2000, i * 2000 + 1000, &format!("line {i}")))
        .collect();
    let mut b = a
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let shift = if i >= 6 { 600 } else { 400 };
            cue(
                c.id().0,
                c.start().get() + shift,
                c.end().get() + shift,
                c.text(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(run(&a, &b, Options::default()).blocks.len(), 2);
    b[3] = cue(4, 6400, 7400, "edited line 3");
    b.insert(4, cue(10, 7500, 7900, "Inserted"));
    for (x, y) in [(&a, &b), (&b, &a)] {
        let c = run(x, y, Options::default());
        assert_eq!(
            c.blocks.iter().map(|b| b.end - b.first).collect::<Vec<_>>(),
            [3, 3]
        );
        assert_eq!(c, run(x, y, Options::default()));
        assert!(kinds(&c).contains(&Kind::TextAndTiming));
    }
}

// Test cue IDs identify the intended correspondence even when every word is
// identical. Production alignment must recover it from times and text alone.
fn assert_alignment(original: &[Cue], revised: &[Cue], expected_blocks: &[(u32, i64)]) {
    use std::collections::{HashMap, HashSet};
    let index = |side: &[Cue]| side.iter().map(|c| c.id().0).collect::<HashSet<_>>();
    for (a, b, sign) in [(original, revised, 1), (revised, original, -1)] {
        let result = run(a, b, Options::default());
        assert_eq!(result, run(a, b, Options::default()));
        let a_ids = index(a);
        let b_ids = index(b);
        let mut seen = HashMap::new();
        for c in &result.changes {
            let (id, expected) = match (c.original.as_slice(), c.revised.as_slice()) {
                ([x], [y]) => {
                    assert_eq!(x.cue_id, y.cue_id, "mispaired cue at {}", x.start_ms);
                    let retimed = (x.start_ms, x.end_ms) != (y.start_ms, y.end_ms);
                    (
                        x.cue_id,
                        match (x.text != y.text, retimed) {
                            (true, true) => Kind::TextAndTiming,
                            (false, true) => Kind::Timing,
                            (true, false) => Kind::Text,
                            (false, false) => Kind::Unchanged,
                        },
                    )
                }
                ([x], []) => {
                    assert!(!b_ids.contains(&x.cue_id), "unexpected removal {x:?}");
                    (x.cue_id, Kind::Removed)
                }
                ([], [y]) => {
                    assert!(!a_ids.contains(&y.cue_id));
                    (y.cue_id, Kind::Added)
                }
                _ => panic!("unexpected group {:?}", c.kind),
            };
            assert_eq!(c.kind, expected);
            assert!(seen.insert(id, expected).is_none());
        }
        assert_eq!(seen.len(), a_ids.union(&b_ids).count());
        assert_eq!(
            result
                .blocks
                .iter()
                .map(|b| (b.end - b.first, b.delta_ms))
                .collect::<Vec<_>>(),
            expected_blocks
                .iter()
                .map(|&(len, d)| (len, d * sign))
                .collect::<Vec<_>>()
        );
    }
}
fn shifted(c: &Cue, delta: u64) -> Cue {
    cue(
        c.id().0,
        c.start().get() + delta,
        c.end().get() + delta,
        c.text(),
    )
}
#[test]
fn repeated_lines_keep_the_confirmed_offset_through_insertions_and_deletions() {
    let chorus = [
        "Where are you going?",
        "♪ La la la ♪",
        "♪ La la la ♪",
        "♪ La la la ♪",
        "♪ La la la ♪",
        "I told you already.",
        "Yes.",
        "Yes.",
        "Fine, go.",
    ];
    let lines = |texts: &[&str]| {
        texts
            .iter()
            .enumerate()
            .map(|(i, t)| cue(i as u64 + 1, i as u64 * 2000, i as u64 * 2000 + 1500, t))
            .collect::<Vec<_>>()
    };
    let original = lines(&chorus);
    for delta in [300, 5000] {
        for deleted in 0..original.len() {
            let mut revised = original
                .iter()
                .map(|c| shifted(c, delta))
                .collect::<Vec<_>>();
            revised.remove(deleted);
            let blocks = [deleted, original.len() - deleted - 1]
                .into_iter()
                .filter(|&len| len >= 3)
                .map(|len| (len as u32, delta as i64))
                .collect::<Vec<_>>();
            assert_alignment(&original, &revised, &blocks);
        }
    }
    let original = lines(&["Yes."; 40]);
    let mut revised = original.iter().map(|c| shifted(c, 300)).collect::<Vec<_>>();
    revised.remove(20);
    assert_alignment(&original, &revised, &[(20, 300), (19, 300)]);
    let original = (0..10)
        .map(|i| cue(i + 1, i * 2000, i * 2000 + 1500, "Yes."))
        .collect::<Vec<_>>();
    let mut revised = original.iter().map(|c| shifted(c, 300)).collect::<Vec<_>>();
    revised.insert(5, cue(11, 9700, 11200, "Yes."));
    assert_alignment(&original, &revised, &[(5, 300), (5, 300)]);
}

#[test]
fn ad_breaks_empty_gaps_and_trimmed_scenes() {
    let original = (0..24)
        .map(|i| {
            cue(
                i + 1,
                i * 2000,
                i * 2000 + 1500,
                &format!("Dialogue line {i}"),
            )
        })
        .collect::<Vec<_>>();
    for delta in [5000, 30000, 120000] {
        let revised = original
            .iter()
            .enumerate()
            .map(|(i, c)| shifted(c, if i >= 12 { delta } else { 0 }))
            .collect::<Vec<_>>();
        // In reverse these are removals of empty advertising gaps.
        assert_alignment(&original, &revised, &[(12, delta as i64)]);
        if delta == 5000 {
            let mut with_ad = revised.clone();
            with_ad.insert(12, cue(25, 26000, 27000, "[Advertisement]"));
            assert_alignment(&original, &with_ad, &[(12, 5000)]);
            let mut cut = revised;
            cut.remove(12);
            cut.insert(12, cue(25, 27000, 27800, "[Ad break]"));
            assert_alignment(&original, &cut, &[(11, 5000)]);
        }
    }
    let mut shifted_file = original
        .iter()
        .map(|c| shifted(c, 5000))
        .collect::<Vec<_>>();
    shifted_file.remove(6);
    shifted_file.insert(13, cue(25, 32000, 32800, "[Advertisement]"));
    assert_alignment(
        &original,
        &shifted_file,
        &[(6, 5000), (7, 5000), (10, 5000)],
    );

    let mut revised = original.clone();
    for (i, c) in revised.iter_mut().enumerate().skip(8) {
        *c = shifted(c, if i < 16 { 5000 } else { 12000 });
        if i == 11 || i == 19 {
            *c = cue(
                c.id().0,
                c.start().get(),
                c.end().get(),
                &format!("Edited dialogue line {i}"),
            );
        }
    }
    assert_alignment(
        &original,
        &revised,
        &[(3, 5000), (4, 5000), (3, 12000), (4, 12000)],
    );
    let trimmed = original
        .iter()
        .enumerate()
        .filter(|(i, _)| !(8..13).contains(i))
        .map(|(i, c)| {
            let shift = if i >= 13 { 10000 } else { 0 };
            cue(
                c.id().0,
                c.start().get() - shift,
                c.end().get() - shift,
                c.text(),
            )
        })
        .collect::<Vec<_>>();
    assert_alignment(&original, &trimmed, &[(11, -10000)]);
}
#[test]
fn long_shift_edges_pair_edits_and_keep_only_the_actual_cuts() {
    let original = (0..60)
        .map(|i| cue(i + 1, i * 2000, i * 2000 + 1500, &format!("Scene line {i}")))
        .collect::<Vec<_>>();
    for delta in [5000, 30000, 120000] {
        let shifted_section = original
            .iter()
            .enumerate()
            .map(|(i, c)| shifted(c, if i >= 30 { delta } else { 0 }))
            .collect::<Vec<_>>();
        for (edits, cut, lengths) in [
            (&[30][..], None, &[29][..]),
            (&[30, 31], None, &[28]),
            (&[], Some(30), &[29]),
            (&[], Some(31), &[28]),
            (&[59], None, &[29]),
            (&[30, 44, 59], None, &[13, 14]),
            (&[29], None, &[30]),
        ] {
            let mut revised = shifted_section.clone();
            for &i in edits {
                let c: &Cue = &revised[i];
                revised[i] = cue(
                    c.id().0,
                    c.start().get(),
                    c.end().get(),
                    &format!("Revised scene line {i}"),
                );
            }
            if let Some(i) = cut {
                revised.remove(i);
            }
            let blocks = lengths
                .iter()
                .map(|&n| (n, delta as i64))
                .collect::<Vec<_>>();
            assert_alignment(&original, &revised, &blocks);
        }
    }
}
#[test]
fn a_shift_transition_uses_both_offsets_and_reacquires_repeated_lines() {
    let original = (0..20)
        .map(|i| {
            let text = if (11..15).contains(&i) {
                "Yes.".to_owned()
            } else {
                format!("Scene line {i}")
            };
            cue(i + 1, i * 2000, i * 2000 + 1500, &text)
        })
        .collect::<Vec<_>>();
    let mut revised = original
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let c = shifted(c, if i < 9 { 30000 } else { 120000 });
            if i == 8 || i == 9 {
                cue(
                    c.id().0,
                    c.start().get(),
                    c.end().get(),
                    &format!("Revised scene line {i}"),
                )
            } else {
                c
            }
        })
        .collect::<Vec<_>>();
    revised.remove(11);
    assert_alignment(&original, &revised, &[(8, 30000), (8, 120000)]);
}
#[test]
fn ten_thousand_cues_with_mid_file_ad_break() {
    let original = (0..10000)
        .map(|i| cue(i + 1, i * 1000, i * 1000 + 800, &format!("Dialogue {i}")))
        .collect::<Vec<_>>();
    let revised = original
        .iter()
        .enumerate()
        .map(|(i, c)| shifted(c, if i >= 5000 { 5000 } else { 0 }))
        .collect::<Vec<_>>();
    for (a, b, delta) in [(&original, &revised, 5000), (&revised, &original, -5000)] {
        let start = std::time::Instant::now();
        let c = run(a, b, Options::default());
        let elapsed = start.elapsed();
        eprintln!("10k mid-file {delta:+} ms: {elapsed:?}");
        assert!(elapsed < std::time::Duration::from_secs(2));
        assert_eq!(c.blocks.len(), 1);
        assert_eq!(
            (c.blocks[0].first, c.blocks[0].end, c.blocks[0].delta_ms),
            (5000, 10000, delta)
        );
        assert!(c.changes[..5000].iter().all(|c| c.kind == Kind::Unchanged));
        assert!(c.changes[5000..].iter().all(|c| c.kind == Kind::Timing));
    }
}
#[test]
fn overview_counts_once_per_cue_at_the_displayed_start() {
    let a = [cue(1, 500, 2500, "One"), cue(2, 40_000, 41_000, "Gone")];
    let b = [cue(1, 10_500, 12_500, "One more")];
    let mut c = run(&a, &b, Options::default());
    assert_eq!(c.changes[0].kind, Kind::TextAndTiming);
    c.changes[0].style_changed = true;
    let graph = overview(&c.changes);
    assert_eq!(graph.buckets[1].kinds, [0, 0, 0, 1, 0, 0]);
    assert_eq!(graph.buckets[4].kinds, [0, 1, 0, 0, 0, 0]);
}
#[test]
fn report_counts_cue_duration_bands_and_omits_empty_controls() {
    let opts = ReportOptions {
        original_name: "A",
        revised_name: "B",
        original_format: Format::Srt,
        revised_format: Format::Srt,
        date: "fixture",
        include_unchanged: false,
    };
    let draw = |a: &[Cue], b: &[Cue]| {
        html(
            &run(a, b, Options::default()),
            &opts,
            &ExtraSections::default(),
        )
    };
    let empty = draw(&[], &[]);
    assert!(empty.contains("No differences"));
    assert!(!empty.contains("<svg") && !empty.contains("<nav"));
    let page = draw(
        &[cue(1, 0, 1000, "one")],
        &[cue(1, 0, 1000, "one <script>")],
    );
    assert!(!page.contains("class=\"delta\""));
    assert!(page.contains("one &lt;script&gt;</title>"));
    assert!(!page.contains("+0 / −0") && !page.contains("of 0 threads"));
    assert!(!page.contains("id=\"f-added\"") && !page.contains("<span>Not rated</span>"));

    // The first two cues overlap. Each band uses the full 1500 ms cue duration.
    let a = [
        cue(1, 0, 1500, "one"),
        cue(2, 100, 1600, "123456"),
        cue(3, 2000, 3500, &"x".repeat(33)),
        cue(4, 4000, 5500, &"y".repeat(40)),
        cue(5, 6000, 6400, "z"),
    ];
    let mut b = a.clone();
    b[0] = cue(1, 0, 1500, "a perfect sentence");
    b[3] = cue(4, 4000, 5500, "b perfect sentence");
    let page = draw(&a, &b);
    assert!(page.contains("Cues per band, by cue duration."));
    assert!(page.contains("outer reading-speed bands (by cue duration)</dt><dd>-2</dd>"));
    for (name, original, revised) in [
        ("TOO SLOW", 1, 0),
        ("Slow, acceptable", 1, 1),
        ("Fast, acceptable", 1, 1),
        ("TOO FAST", 1, 0),
        ("Perfect", 0, 2),
        ("Not rated", 1, 1),
    ] {
        let (_, band) = page
            .split_once(&format!("<div class=\"band\"><span>{name}</span>"))
            .unwrap();
        let (band, _) = band.split_once("</div>").unwrap();
        assert!(band.starts_with(&format!("<span class=\"bar-l\">{original}<")));
        assert!(band.ends_with(&format!("</i>{revised}</span>")));
    }
    assert!(page.contains("TOO SLOW → Perfect") && page.contains("TOO FAST → Perfect"));
    let shifted = draw(
        &[cue(1, 0, 1500, "same words")],
        &[cue(1, 100, 1600, "same words")],
    );
    assert!(
        !shifted.contains("class=\"metrics\">was ")
            && !shifted.contains("<span class=\"band-change\">")
    );
}

#[test]
fn unspaced_script_edits_pair_split_merge_and_highlight() {
    for (left, right, old, new, tail) in [
        ("今日は", "晴れです", "晴", "雨", "れです"),
        ("วันนี้", "ร้อนมาก", "ร้", "ช้", "อนมาก"),
    ] {
        let original = format!("{left}{right}");
        let revised = format!("{left}{new}{tail}");
        let a = [cue(1, 1000, 4000, &original)];
        let changed = [cue(1, 3500, 6500, &revised)];
        assert_eq!(
            kinds(&run(&a, &changed, Options::default())),
            [Kind::TextAndTiming]
        );
        let parts = [cue(1, 1000, 2500, left), cue(2, 2500, 4000, right)];
        assert_eq!(kinds(&run(&a, &parts, Options::default())), [Kind::Split]);
        assert_eq!(kinds(&run(&parts, &a, Options::default())), [Kind::Merged]);
        let diff = word_diff(&original, &revised);
        assert_eq!(
            diff.iter()
                .map(|t| (t.kind, t.text.as_str()))
                .collect::<Vec<_>>(),
            [
                (TokenKind::Equal, left),
                (TokenKind::Removed, old),
                (TokenKind::Added, new),
                (TokenKind::Equal, tail)
            ]
        );
    }
}

#[test]
fn reading_tokens_keep_spaced_scripts_and_marks_intact() {
    for text in ["Hello, italic-word!", "안녕, 세계", "cafe\u{301} déjà"] {
        assert_eq!(
            words::reading_tokens(text).collect::<Vec<_>>(),
            text.split_whitespace().collect::<Vec<_>>()
        );
    }
    assert_eq!(
        words::reading_tokens("abc漢かカคำກັកាကာ").collect::<Vec<_>>(),
        ["abc", "漢", "か", "カ", "ค", "ำ", "ກັ", "កា", "ကာ"]
    );
}
