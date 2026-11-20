use super::*;
use crate::comparison::{self, Kind};
use serde_json::{Value, json};
use subtitle_core::{Cue, CueId, Milliseconds};

const GOLD: &str = include_str!("../../tests/fixtures/review-v1.vsfreview");
fn fixture() -> Package {
    read(GOLD.as_bytes()).unwrap()
}
fn cue(i: u64, start: u64, end: u64, text: &str) -> Cue {
    Cue::new(
        CueId(i),
        Milliseconds::new(start).unwrap(),
        Milliseconds::new(end).unwrap(),
        text.into(),
    )
    .unwrap()
}
fn compare(a: &[Cue], b: &[Cue]) -> comparison::Comparison {
    comparison::compare(
        a,
        Format::Srt,
        b,
        Format::Srt,
        Options::default(),
        || Ok(()),
    )
    .unwrap()
}
fn positions(t: &[Thread], c: &comparison::Comparison) -> Vec<Option<usize>> {
    reanchor(t, c, Format::Srt, Format::Srt, || Ok(())).unwrap()
}
fn thread(c: &comparison::Change) -> Thread {
    Thread::new(Anchor::from_change(c, 0), "Ana".into(), 20, "Check".into()).unwrap()
}
#[test]
fn wire_fixture_and_untrusted_fields() {
    let p = fixture();
    assert_eq!(p.write().unwrap(), GOLD.trim().as_bytes());
    for (path, value, error) in [
        ("/version", json!(2), "newer VisualSubForge"),
        ("/options", json!({"irrelevant":1}), "unknown field"),
        ("/revised/bytes", json!("!!!!"), "base64"),
        ("/original/sha256", json!("wrong"), "fingerprint"),
        ("/threads/0/id", json!("wrong"), "Thread ID"),
        ("/threads/0/messages/0/author", json!("Other"), "Thread ID"),
        ("/threads/0/messages/0/text", json!("X"), "Thread ID"),
        ("/threads/0/anchor/kind", json!("time"), "unknown variant"),
    ] {
        let mut v: Value = serde_json::from_str(GOLD).unwrap();
        *v.pointer_mut(path).unwrap() = value;
        assert!(
            read(&serde_json::to_vec(&v).unwrap())
                .unwrap_err()
                .contains(error),
            "{path}"
        );
    }
    let mut p = p;
    let mut bytes = vec![255, 254];
    bytes.extend(
        String::from_utf8(p.original.decode().unwrap())
            .unwrap()
            .encode_utf16()
            .flat_map(u16::to_le_bytes),
    );
    p.original = Subtitle::capture("../../Sample.srt", Format::Srt, &bytes);
    assert_eq!(
        read(&p.write().unwrap())
            .unwrap()
            .original
            .decode()
            .unwrap(),
        bytes
    );
    assert_eq!(safe_name("C:\\..\\con.srt", "srt"), "Subtitles.srt");
}
#[test]
fn payload_size_boundaries() {
    assert!(
        read(&vec![b' '; MAX_BYTES + 1])
            .unwrap_err()
            .contains("64 MiB")
    );
    let mut p = fixture();
    let mut bytes = p.original.decode().unwrap();
    bytes.resize(srt_limits::MAX_BYTES, b'\n');
    p.original = Subtitle::capture("a", Format::Srt, &bytes);
    p.revised = p.original.clone();
    assert_eq!(
        read(&p.write().unwrap())
            .unwrap()
            .revised
            .decode()
            .unwrap()
            .len(),
        bytes.len()
    );
    bytes.push(b'\n');
    p.original = Subtitle::capture("a", Format::Srt, &bytes);
    assert!(p.original.parsed().unwrap_err().contains("16 MiB"));
}
#[test]
fn discussion_limits_and_event_integrity() {
    let p = fixture();
    let mut t = p.threads[0].clone();
    assert!(
        validate_threads(&vec![t.clone(); MAX_THREADS + 1])
            .unwrap_err()
            .contains("10,000")
    );
    for author in ["a".repeat(101), "a\nb".into()] {
        assert!(validate_name(&author).is_err())
    }
    for text in ["a".repeat(10_001), "a\0b".into()] {
        assert!(Message::new(&t.id, "A".into(), 20, Event::Comment, Some(text)).is_err())
    }
    let msg = Message::new(&t.id, "B".into(), 11, Event::Resolve, None).unwrap();
    t.messages.push(msg);
    t.messages[1].id = "wrong".into();
    assert!(
        validate_threads(&[t.clone()])
            .unwrap_err()
            .contains("message ID")
    );
    t.messages = vec![t.messages[0].clone(); MAX_MESSAGES + 1];
    assert!(validate_threads(&[t]).unwrap_err().contains("100,000"));
    let mut t = p.threads[0].clone();
    let Anchor::Cue { revised } = &mut t.anchor else {
        unreachable!()
    };
    revised[0].text = "x".repeat(MAX_TEXT_BYTES);
    assert!(validate_threads(&[t]).unwrap_err().contains("16 MiB"));
}
#[test]
fn merge_laws_for_all_subsets_of_concurrent_events() {
    let t = fixture().threads.remove(0);
    let mut states = Vec::new();
    for mask in 0..8 {
        let mut t = t.clone();
        for (bit, event) in [Event::Resolve, Event::Reopen, Event::Comment]
            .into_iter()
            .enumerate()
        {
            if mask & (1 << bit) != 0 {
                t.messages.push(
                    Message::new(
                        &t.id,
                        "Other".into(),
                        11 + bit as u64,
                        event,
                        (event == Event::Comment).then(|| "Reply".into()),
                    )
                    .unwrap(),
                )
            }
        }
        t.sort_messages();
        assert_eq!(t.resolved(), mask & 1 != 0 && mask & 2 == 0);
        states.push(vec![t]);
    }
    states.push(vec![]);
    for a in &states {
        assert_eq!(merge(a, a).unwrap(), *a);
        for b in &states {
            let ab = merge(a, b).unwrap();
            assert_eq!(ab, merge(b, a).unwrap());
            for c in &states {
                assert_eq!(
                    merge(&ab, c).unwrap(),
                    merge(a, &merge(b, c).unwrap()).unwrap()
                );
            }
        }
    }
}
#[test]
fn immutable_anchors_insert_revert_split_and_retime() {
    let a = cue(1, 2000, 4000, "one two");
    let b = cue(2, 2000, 4000, "one new");
    let changed = compare(std::slice::from_ref(&a), std::slice::from_ref(&b));
    let t = thread(&changed.changes[0]);
    let ins = cue(3, 0, 500, "early");
    let c = compare(&[ins.clone(), a.clone()], &[ins, b]);
    assert_eq!(positions(std::slice::from_ref(&t), &c), vec![Some(1)]);
    let c = compare(std::slice::from_ref(&a), std::slice::from_ref(&a));
    assert_eq!(positions(std::slice::from_ref(&t), &c), [None]);
    let unchanged = thread(&c.changes[0]);
    let c = compare(std::slice::from_ref(&a), &[cue(4, 2100, 4100, "one two")]);
    assert_eq!(positions(&[unchanged], &c), [Some(0)]);
    let c = compare(
        &[a],
        &[cue(2, 2000, 3000, "one"), cue(3, 3000, 4000, "two")],
    );
    assert_eq!(c.changes[0].kind, Kind::Split);
    // A text-edit anchor survives a new split whose content key differs.
    assert_ne!(changed.changes[0].key, c.changes[0].key);
    for _ in 0..2 {
        assert_eq!(positions(std::slice::from_ref(&t), &c), [Some(0)]);
    }
}
#[test]
fn report_block_is_inert_unique_and_optional() {
    let mut p = fixture();
    let parsed = p.original.parsed().unwrap();
    let c = compare(parsed.document.cues(), parsed.document.cues());
    p.threads = vec![
        Thread::new(
            p.threads[0].anchor.clone(),
            "<img>".into(),
            10,
            "</script>&\u{2028}\u{2029}".into(),
        )
        .unwrap(),
    ];
    let opts = comparison::ReportOptions {
        original_name: "A",
        revised_name: "B",
        date: "today",
        include_unchanged: false,
    };
    let render = |embed| comparison::html(&c, &opts, &sections(&p, &c, embed).unwrap());
    let html = render(true);
    assert_eq!(read(html.as_bytes()).unwrap(), p);
    assert!(html.contains("&lt;img&gt;") && !html.contains("<img>"));
    for page in [
        html.replace(
            OPEN_TAG,
            r#"<script type="text/javascript" id="vsf-review-package">"#,
        ),
        render(false),
    ] {
        assert!(
            read(page.as_bytes())
                .unwrap_err()
                .contains("doesn't contain review data")
        )
    }
    let mut oversized = OPEN_TAG.as_bytes().to_vec();
    oversized.resize(MAX_BYTES, b' ');
    oversized.extend_from_slice(b"</script>");
    assert!(read(&oversized).unwrap_err().contains("64 MiB"));
    for page in [
        format!("{html}{OPEN_TAG}"),
        html.replacen("visualsubforge-review", "</script>", 1),
    ] {
        assert!(read(page.as_bytes()).is_err())
    }
}
#[test]
fn ten_thousand_cues_one_thousand_threads_budget() {
    let a: Vec<_> = (0..10_000)
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
                    &format!("Updated cue {i}"),
                )
            } else {
                c.clone()
            }
        })
        .collect();
    let start = std::time::Instant::now();
    let c = compare(&a, &b);
    let comparison = start.elapsed();
    let threads: Vec<_> = c.changes.iter().step_by(10).map(thread).collect();
    let start = std::time::Instant::now();
    let found = positions(&threads, &c);
    let elapsed = start.elapsed();
    assert!(found.iter().enumerate().all(|(i, p)| *p == Some(i * 10)));
    eprintln!(
        "10k cues, 1k threads: comparison {comparison:?}; re-anchoring {elapsed:?}; margin 500 ms over comparison budget 2 s"
    );
    assert!(elapsed < std::time::Duration::from_millis(500));
    assert!(comparison + elapsed < std::time::Duration::from_millis(2500));
}

#[test]
fn reverted_similar_line_detaches_but_edits_in_the_same_span_follow() {
    let original = [
        cue(1, 1000, 2500, "Come on, let's go."),
        cue(2, 6000, 7500, "Come on, let's go now."),
    ];
    let mut revised = [
        cue(1, 1000, 2500, "Come on, let’s go!"),
        cue(2, 6000, 7500, "Come on, let's go!"),
    ];
    let t = thread(&compare(&original, &revised).changes[0]);
    revised[0] = cue(1, 1000, 2500, "Let's go!");
    assert_eq!(
        positions(std::slice::from_ref(&t), &compare(&original, &revised)),
        [Some(0)]
    );
    revised[0] = original[0].clone();
    assert_eq!(positions(&[t], &compare(&original, &revised)), [None]);
}

#[test]
fn identical_changes_keep_the_commented_occurrence_across_exchange() {
    let mut c = compare(&[cue(1, 10, 20, "old")], &[cue(1, 10, 20, "new")]);
    c.changes.push(c.changes[0].clone());
    let second = Thread::new(
        Anchor::from_change(&c.changes[1], 1),
        "Ana".into(),
        1,
        "Second".into(),
    )
    .unwrap();
    let mut package = fixture();
    package.threads = vec![second];
    assert_eq!(
        positions(&read(&package.write().unwrap()).unwrap().threads, &c),
        [Some(1)]
    );
}
