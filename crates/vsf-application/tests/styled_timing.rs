//! Format-aware repairs exercise public application APIs, serialization and history.
use subtitle_core::{Cue, Document, Edit, Editor, Milliseconds};
use subtitle_formats::{Format, Source};
use vsf_application::quality::{self, Profile, duration, gap, scene};
use vsf_application::subtitle_file;

fn ms(value: u64) -> Milliseconds {
    Milliseconds::new(value).unwrap()
}
fn fixture(extension: &str, rows: &[(u64, u64, &str)]) -> (Document, Source) {
    let text = if extension == "ass" {
        include_str!("../../../tests/fixtures/subtitles/styles.ass")
    } else {
        include_str!("../../../tests/fixtures/subtitles/styles.ssa")
    };
    let parsed = subtitle_file::parse_document(text.as_bytes(), Some(extension)).unwrap();
    let cues = rows
        .iter()
        .zip(parsed.document.cues())
        .map(|(&(start, end, text), cue)| {
            Cue::new(cue.id(), ms(start), ms(end), text.into())
                .unwrap()
                .with_metadata(cue.metadata().unwrap().clone())
        })
        .collect();
    (Document::new(cues).unwrap(), parsed.source)
}
fn profile() -> Profile {
    let mut p = Profile::default().custom("styled-timing".into(), "Styled timing".into());
    p.rules.speed.enabled = false;
    p.rules.duration.enabled = false;
    p.rules.lines.enabled = false;
    p.timing.scene_before_ms = 0;
    p.timing.scene_after_ms = 0;
    p
}
fn accept_and_undo(document: Document, source: &Source, edit: Edit, expected: &[(u64, u64)]) {
    let before = source.serialize(&document).unwrap();
    let mut editor = Editor::new(document.clone());
    editor
        .apply_edit(0, edit, |d| source.serialize(d).map(|_| ()))
        .unwrap();
    assert_eq!(
        editor
            .document()
            .cues()
            .iter()
            .map(|c| (c.start().get(), c.end().get()))
            .collect::<Vec<_>>(),
        expected
    );
    for (old, new) in document.cues().iter().zip(editor.document().cues()) {
        assert_eq!(
            (old.id(), old.text(), old.metadata()),
            (new.id(), new.text(), new.metadata())
        );
    }
    let after = source.serialize(editor.document()).unwrap();
    assert_ne!(before, after);
    editor.undo(1).unwrap();
    assert_eq!(source.serialize(editor.document()).unwrap(), before);
    editor.redo(2).unwrap();
    assert_eq!(source.serialize(editor.document()).unwrap(), after);
}
#[test]
fn gap_duration_and_scene_repairs_preserve_both_formats_and_use_centiseconds() {
    for extension in ["ass", "ssa"] {
        let (d, source) = fixture(
            extension,
            &[(1000, 2000, r"{\i1}日本語\N😀"), (2050, 3000, "Second")],
        );
        let mut p = profile();
        p.timing.cue_gap_ms = 83;
        let plan = gap::propose_for_format(
            d.clone(),
            source.format(),
            &p,
            &scene::Scenes::NoMedia,
            || Ok(()),
        )
        .unwrap();
        assert_eq!(plan.changed(), 1);
        let edit = plan.edit(&d, false).unwrap();
        accept_and_undo(d, &source, edit, &[(1000, 1960), (2050, 3000)]);

        let (d, source) = fixture(extension, &[(1000, 1500, r"First\Nline")]);
        p.rules.duration.enabled = true;
        p.rules.duration.minimum_ms = 803;
        let plan = duration::propose_for_format(
            d.clone(),
            source.format(),
            &p,
            &scene::Scenes::NoMedia,
            duration::Scope::Document,
            || Ok(()),
        )
        .unwrap();
        let edit = plan.edit(&d).unwrap();
        accept_and_undo(d, &source, edit, &[(1000, 1810)]);

        let (d, source) = fixture(extension, &[(1010, 3000, r"{comment}<i>Literal</i>\Nline")]);
        p.rules.duration.enabled = false;
        p.timing.scene_before_ms = 1;
        p.timing.scene_after_ms = 2;
        // Several safe gaps contain no representable timestamp. Their expanded
        // exclusion windows must merge so the chosen edge is actually safe.
        let scenes =
            scene::Scenes::Complete([1001, 1005, 1009, 2999].into_iter().map(ms).collect());
        let plan =
            scene::propose_for_format(d.clone(), source.format(), &p, &scenes, || Ok(())).unwrap();
        assert_eq!(plan.changed(), 1);
        let edit = plan.edit(&d).unwrap();
        accept_and_undo(d, &source, edit, &[(1020, 2990)]);
    }
}
#[test]
fn duration_grid_choice_matches_independent_enumeration_including_scene_edges() {
    let mut p = profile();
    p.rules.duration.enabled = true;
    p.rules.duration.minimum_ms = 803;
    p.rules.duration.maximum_ms = 1207;
    p.timing.scene_before_ms = 7;
    p.timing.scene_after_ms = 14;
    for old in [400, 800, 900, 1210, 1600] {
        for cuts in [
            vec![],
            vec![805, 825, 849],
            vec![1199, 1220],
            vec![810, 831, 852],
        ] {
            let (d, source) = fixture("ass", &[(0, old, "Plain")]);
            let scenes = scene::Scenes::Complete(cuts.iter().copied().map(ms).collect());
            let plan = duration::propose_for_format(
                d,
                source.format(),
                &p,
                &scenes,
                duration::Scope::Document,
                || Ok(()),
            )
            .unwrap();
            if (803..=1207).contains(&old) {
                assert!(plan.rows().is_empty());
                continue;
            }
            let oracle = (10u64..=2000)
                .step_by(10)
                .filter(|&v| {
                    (803..=1207).contains(&v) && cuts.iter().all(|&c| !(c - 7 < v && v < c + 14))
                })
                .min_by_key(|&v| (v.abs_diff(old), v));
            assert_eq!(
                plan.rows()[0].new_end.map(|m| m.get()),
                oracle,
                "old={old}, cuts={cuts:?}"
            );
        }
    }
    let (d, source) = fixture("ass", &[(0, 400, "Plain")]);
    p.rules.duration.maximum_ms = 809;
    let plan = duration::propose_for_format(
        d,
        source.format(),
        &p,
        &scene::Scenes::Complete(vec![]),
        duration::Scope::Document,
        || Ok(()),
    )
    .unwrap();
    assert_eq!(
        plan.rows()[0].skipped,
        Some(duration::Skip::FormatPrecision)
    );
}
#[test]
fn timed_tags_and_event_effects_are_manual_for_all_timing_repairs() {
    for format in [Format::Srt, Format::Ass, Format::Ssa] {
        for text in [
            r"{\k20}Timed",
            r"{\move(0,0,100,100)}Moved",
            r"{\t(\fs50)}Animated",
            r"{\unknown}Opaque",
        ] {
            let (d, _) = fixture("ass", &[(1000, 2000, text), (2050, 3000, "Second")]);
            let mut p = profile();
            p.timing.cue_gap_ms = 83;
            let plan =
                gap::propose_for_format(d.clone(), format, &p, &scene::Scenes::NoMedia, || Ok(()))
                    .unwrap();
            assert_eq!(plan.rows()[0].skipped, Some(gap::Skip::MarkupNeedsReview));
            p.rules.duration.enabled = true;
            p.rules.duration.minimum_ms = 1100;
            let plan = duration::propose_for_format(
                d.clone(),
                format,
                &p,
                &scene::Scenes::NoMedia,
                duration::Scope::Document,
                || Ok(()),
            )
            .unwrap();
            assert_eq!(
                plan.rows()[0].skipped,
                Some(duration::Skip::MarkupNeedsReview)
            );
            p.timing.scene_before_ms = 10;
            p.timing.scene_after_ms = 10;
            let plan = scene::propose_for_format(
                d,
                format,
                &p,
                &scene::Scenes::Complete(vec![ms(1000)]),
                || Ok(()),
            )
            .unwrap();
            assert_eq!(plan.rows()[0].skipped, Some(scene::Skip::MarkupNeedsReview));
        }
    }
    let (d, _) = fixture("ass", &[(1000, 2000, "Plain"), (2050, 3000, "Second")]);
    // Build Effect explicitly; the adapter is the authority for field placement.
    let cue = &d.cues()[0];
    let fields = cue
        .metadata()
        .unwrap()
        .fields()
        .iter()
        .map(|(k, v)| {
            (
                k.clone(),
                if k.eq_ignore_ascii_case("Effect") {
                    "Banner;0;0;0".into()
                } else {
                    v.clone()
                },
            )
        })
        .collect();
    let mut cues = d.cues().to_vec();
    cues[0] = cue
        .clone()
        .with_metadata(subtitle_core::CueMetadata::new(fields).unwrap());
    let d = Document::new(cues).unwrap();
    let mut p = profile();
    p.timing.cue_gap_ms = 83;
    assert_eq!(
        gap::propose_for_format(d.clone(), Format::Ass, &p, &scene::Scenes::NoMedia, || Ok(
            ()
        ))
        .unwrap()
        .rows()[0]
            .skipped,
        Some(gap::Skip::MarkupNeedsReview)
    );
    p.rules.duration.enabled = true;
    p.rules.duration.minimum_ms = 1100;
    assert_eq!(
        duration::propose_for_format(
            d.clone(),
            Format::Ass,
            &p,
            &scene::Scenes::NoMedia,
            duration::Scope::Document,
            || Ok(())
        )
        .unwrap()
        .rows()[0]
            .skipped,
        Some(duration::Skip::MarkupNeedsReview)
    );
    p.timing.scene_before_ms = 10;
    p.timing.scene_after_ms = 10;
    assert_eq!(
        scene::propose_for_format(
            d,
            Format::Ass,
            &p,
            &scene::Scenes::Complete(vec![ms(1000)]),
            || Ok(())
        )
        .unwrap()
        .rows()[0]
            .skipped,
        Some(scene::Skip::MarkupNeedsReview)
    );
}
#[test]
fn scene_grid_window_construction_remains_cancellable() {
    let (d, source) = fixture("ass", &[(1000, 3000, "Text")]);
    let mut p = profile();
    p.timing.scene_before_ms = 1;
    p.timing.scene_after_ms = 2;
    let scenes = scene::Scenes::Complete((1..=100_000).map(|n| ms(n * 4)).collect());
    let mut calls = 0;
    let result = scene::propose_for_format(d, source.format(), &p, &scenes, || {
        calls += 1;
        if calls > 20 {
            Err(quality::Error::Cancelled)
        } else {
            Ok(())
        }
    });
    assert!(matches!(result, Err(quality::Error::Cancelled)));
}
