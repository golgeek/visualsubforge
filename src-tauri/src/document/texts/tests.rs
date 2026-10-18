use super::*;
use crate::quality_profile::Policy;

pub(super) fn fixture() -> (tempfile::TempDir, Documents) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source.srt");
    fs::write(&path, "1\n00:00:01,001 --> 00:00:02,999\n{\\an8}<i>ÉTÉ 😀 été</i>\n\n2\n00:00:03,000 --> 00:00:04,000\nété\n\n").unwrap();
    let docs = Documents::default();
    docs.apply(
        Action::Open {
            path: path.to_string_lossy().into_owned(),
            discard: false,
        },
        0,
        0,
    )
    .unwrap();
    (dir, docs)
}
fn operation() -> Operation {
    Operation::Replace {
        query: "été".into(),
        replacement: "summer".into(),
        case_sensitive: false,
    }
}

#[test]
fn language_review_is_atomic_undoable_and_invalidated_by_word_policy_changes() {
    use crate::language_tools::Change;
    use vsf_application::language::{Language, WordLists};
    let (_dir, docs) = fixture();
    let before = docs.apply(Action::Status, 0, 0).unwrap();
    let edited = docs
        .apply(
            Action::Edit {
                cue_id: 1,
                start_ms: 1001,
                end_ms: 2999,
                text: "😀 <i>helo helo</i>".into(),
            },
            before.generation,
            before.revision,
        )
        .unwrap();
    let mut profile =
        engine::Profile::default().custom("language-review".into(), "Language review".into());
    profile.language_tools.spelling = true;
    docs.initialize_preferences(
        Policy::new(profile, Default::default()).unwrap(),
        WordLists::default(),
    )
    .unwrap();
    let correction = || Operation::LanguageCorrection {
        cue_index: 0,
        start: 6,
        end: 10,
        original: "helo".into(),
        replacement: "hello".into(),
    };
    let id = docs
        .text_start(
            edited.generation,
            edited.revision,
            Operation::LanguageSuggestions {
                cue_index: 0,
                start: 6,
                end: 10,
                original: "helo".into(),
            },
        )
        .unwrap();
    assert!(ready(&docs, id).suggestions.iter().any(|w| w == "hello"));
    assert!(docs.text_accept(id, None).is_err());
    let id = docs
        .text_start(edited.generation, edited.revision, correction())
        .unwrap();
    assert_eq!(ready(&docs, id).changed, 1);
    let accepted = docs.text_accept(id, None).unwrap();
    assert_eq!(accepted.cues[0].text, "😀 <i>hello helo</i>");
    let undone = docs
        .apply(Action::Undo, accepted.generation, accepted.revision)
        .unwrap();
    assert_eq!(undone.cues[0].text, edited.cues[0].text);
    let redone = docs
        .apply(Action::Redo, undone.generation, undone.revision)
        .unwrap();
    assert_eq!(redone.cues[0].text, accepted.cues[0].text);
    let undone = docs
        .apply(Action::Undo, redone.generation, redone.revision)
        .unwrap();
    let id = docs
        .text_start(undone.generation, undone.revision, correction())
        .unwrap();
    ready(&docs, id);
    let epoch = docs.quality_context().unwrap().epoch;
    let state = value(&docs);
    assert!(
        docs.language_words_change(
            epoch,
            Change::Remember {
                language: Language::EnglishUs,
                word: "helo".into()
            },
            |_| Err("injected persistence failure".into())
        )
        .is_err()
    );
    assert_eq!(docs.quality_context().unwrap().epoch, epoch);
    assert!(
        docs.language_words()
            .unwrap()
            .personal
            .english_us
            .is_empty()
    );
    assert_eq!(ready(&docs, id).changed, 1);
    docs.language_words_change(
        epoch,
        Change::Ignore {
            language: Language::EnglishUs,
            word: "helo".into(),
        },
        |_| panic!("session ignores must not write preferences"),
    )
    .unwrap();
    assert_eq!(value(&docs), state);
    assert_eq!(docs.text_view(id, 0, 100).unwrap().phase, Phase::Stale);
    assert!(docs.text_accept(id, None).is_err());
    assert!(
        docs.language_words_change(
            epoch,
            Change::Remember {
                language: Language::EnglishUs,
                word: "name".into()
            },
            |_| panic!("stale changes must not persist")
        )
        .is_err()
    );
    let id = docs
        .text_start(undone.generation, undone.revision, correction())
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let view = docs.text_view(id, 0, 100).unwrap();
        if view.phase == Phase::Failed {
            assert!(view.error.unwrap().contains("finding changed"));
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    let epoch = docs.quality_context().unwrap().epoch;
    docs.language_words_change(epoch, Change::ClearIgnored, |_| {
        panic!("clearing ignores must not persist")
    })
    .unwrap();
    let id = docs
        .text_start(undone.generation, undone.revision, correction())
        .unwrap();
    assert_eq!(ready(&docs, id).changed, 1);
    docs.shutdown_texts().unwrap();
}
pub(super) fn start(docs: &Documents) -> u64 {
    let before = docs.apply(Action::Status, 0, 0).unwrap();
    docs.text_start(before.generation, before.revision, operation())
        .unwrap()
}
pub(super) fn ready(docs: &Documents, id: u64) -> TextView {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let view = docs.text_view(id, 0, 100).unwrap();
        if view.phase == Phase::Reviewing {
            return view;
        }
        assert_eq!(view.phase, Phase::Preparing, "{:?}", view.error);
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn value(docs: &Documents) -> serde_json::Value {
    serde_json::to_value(docs.apply(Action::Status, 0, 0).unwrap()).unwrap()
}
#[test]
fn preview_and_single_match_or_batch_acceptance_preserve_tags_history_and_save_protection() {
    let (dir, docs) = fixture();
    {
        let mut store = docs.0.lock().unwrap();
        let loaded = store.loaded_mut().unwrap();
        loaded.recovered = true;
        loaded.resynced = true;
    }
    let original = value(&docs);
    let id = start(&docs);
    let view = ready(&docs, id);
    assert_eq!((view.changed, view.total, view.matches), (2, 2, 3));
    assert_eq!(value(&docs), original);
    assert!(docs.text_view(id, 0, 201).is_err());
    assert!(docs.text_matches(id, 0, 0, 201).is_err());
    assert_eq!(
        docs.text_matches(id, 0, 1, 1).unwrap()[0].start,
        view.rows[0].matches[1].start
    );
    let accepted = docs
        .text_accept(
            id,
            Some(Target {
                cue_index: 0,
                match_index: 1,
            }),
        )
        .unwrap();
    assert_eq!(accepted.cues[0].text, "{\\an8}<i>ÉTÉ 😀 summer</i>");
    assert_eq!(accepted.cues[1].text, "été");
    assert!(accepted.requires_save_as);
    assert!(docs.text_accept(id, None).is_err());
    let undone = docs
        .apply(Action::Undo, accepted.generation, accepted.revision)
        .unwrap();
    assert_eq!(
        serde_json::to_value(&undone.cues).unwrap(),
        original["cues"]
    );
    assert!(!undone.can_undo && undone.requires_save_as);
    assert_eq!(undone.dirty, original["dirty"].as_bool().unwrap());
    let id = start(&docs);
    ready(&docs, id);
    let accepted = docs.text_accept(id, None).unwrap();
    assert_eq!(accepted.cues[0].text, "{\\an8}<i>summer 😀 summer</i>");
    assert_eq!(accepted.cues[1].text, "summer");
    for (cue, before) in accepted
        .cues
        .iter()
        .zip(original["cues"].as_array().unwrap())
    {
        assert_eq!(cue.start_ms, before["startMs"].as_u64().unwrap());
        assert_eq!(cue.end_ms, before["endMs"].as_u64().unwrap());
        assert_eq!(cue.id, before["id"].as_u64().unwrap());
    }
    assert!(
        docs.apply(
            Action::Save {
                path: Some(dir.path().to_string_lossy().into_owned())
            },
            accepted.generation,
            accepted.revision
        )
        .is_err()
    );
    assert_eq!(value(&docs)["revision"], accepted.revision);
    let undone = docs
        .apply(Action::Undo, accepted.generation, accepted.revision)
        .unwrap();
    assert_eq!(serde_json::to_value(undone.cues).unwrap(), original["cues"]);
    let redone = docs
        .apply(Action::Redo, undone.generation, undone.revision)
        .unwrap();
    assert_eq!(
        serde_json::to_value(redone.cues).unwrap(),
        serde_json::to_value(accepted.cues).unwrap()
    );
    let view = docs
        .quality_read(crate::quality_check::Request {
            session: 0.try_into().unwrap(),
            generation: redone.generation.try_into().unwrap(),
            revision: redone.revision.try_into().unwrap(),
            run: false,
            restart: false,
            offset: 0,
            limit: 100,
            metrics_offset: 0,
            metrics_limit: 100,
            selected: None,
            rule: None,
            severity: None,
            draft: None,
        })
        .unwrap();
    assert!(view.profile_hash.is_none() && view.metrics.is_empty());
}
#[test]
fn profile_revision_and_document_changes_retire_results_without_mutation() {
    let (_dir, docs) = fixture();
    let id = start(&docs);
    ready(&docs, id);
    let context = docs.quality_context().unwrap();
    let mut timing = context.timing;
    timing.cue_gap_ms = 190;
    docs.apply_quality_defaults(
        context.epoch,
        Policy::legacy(timing, "test").unwrap(),
        || Ok(()),
    )
    .unwrap();
    assert_eq!(docs.text_view(id, 0, 10).unwrap().phase, Phase::Stale);
    let before = value(&docs);
    assert!(docs.text_accept(id, None).is_err());
    assert_eq!(value(&docs), before);
    let id = start(&docs);
    ready(&docs, id);
    docs.apply(
        Action::Edit {
            cue_id: 1,
            start_ms: 1001,
            end_ms: 2999,
            text: "draft".into(),
        },
        1,
        0,
    )
    .unwrap();
    docs.apply(Action::Undo, 1, 1).unwrap();
    assert!(docs.text_accept(id, None).is_err());
    let id = start(&docs);
    ready(&docs, id);
    docs.apply(Action::Close { discard: true }, 1, 2).unwrap();
    assert!(docs.text_matches(id, 0, 0, 100).is_err());
    assert!(docs.text_accept(id, None).is_err());
    assert!(docs.text_start(1, 2, operation()).is_err());
}
#[test]
fn cancellation_reaps_workers_and_old_cancel_cannot_cancel_new_proposal() {
    let (_dir, docs) = fixture();
    let (entered, rx) = std::sync::mpsc::channel();
    let (release, wait) = std::sync::mpsc::channel();
    let id = {
        let mut store = docs.0.lock().unwrap();
        let key = store.quality_key().unwrap();
        let profile = store.quality.context().profile;
        store
            .texts
            .launch(key, operation(), profile, move |cancel| {
                entered.send(cancel).unwrap();
                wait.recv_timeout(Duration::from_secs(5)).unwrap();
                Err("Obsolete failure".into())
            })
            .unwrap()
    };
    let cancel = rx.recv_timeout(Duration::from_secs(5)).unwrap();
    docs.text_cancel(id).unwrap();
    assert!(cancel.load(Ordering::Relaxed));
    assert!(docs.text_start(1, 0, operation()).is_err());
    release.send(()).unwrap();
    let end = Instant::now() + Duration::from_secs(5);
    while docs.text_view(id, 0, 100).unwrap().phase == Phase::Cancelling {
        assert!(Instant::now() < end);
        std::thread::yield_now();
    }
    assert!(docs.text_accept(id, None).is_err());
    let newer = start(&docs);
    ready(&docs, newer);
    docs.text_cancel(id).unwrap();
    assert_eq!(
        docs.text_view(newer, 0, 100).unwrap().phase,
        Phase::Reviewing
    );
    docs.shutdown_quality().unwrap();
    assert!(docs.0.lock().unwrap().texts.job.is_none());
    assert!(docs.text_start(1, 0, operation()).is_err());
}

#[test]
fn accepted_batch_saves_exact_bom_crlf_bytes_and_reopens_without_touching_source() {
    let (dir, docs) = fixture();
    let source = dir.path().join("source.srt");
    docs.apply(Action::Close { discard: true }, 1, 0).unwrap();
    let bytes = format!(
        "\u{feff}{}",
        fs::read_to_string(&source).unwrap().replace('\n', "\r\n")
    );
    fs::write(&source, &bytes).unwrap();
    let closed = docs.apply(Action::Status, 0, 0).unwrap();
    docs.apply(
        Action::Open {
            path: source.to_string_lossy().into_owned(),
            discard: false,
        },
        closed.generation,
        closed.revision,
    )
    .unwrap();
    let id = start(&docs);
    ready(&docs, id);
    let accepted = docs.text_accept(id, None).unwrap();
    let copy = dir.path().join("copy.srt");
    let saved = docs
        .apply(
            Action::Save {
                path: Some(copy.to_string_lossy().into_owned()),
            },
            accepted.generation,
            accepted.revision,
        )
        .unwrap();
    assert!(!saved.dirty);
    assert_eq!(
        fs::read_to_string(&copy).unwrap(),
        bytes.replace("ÉTÉ", "summer").replace("été", "summer")
    );
    assert_eq!(fs::read_to_string(&source).unwrap(), bytes);
    let reopened = docs
        .apply(
            Action::Open {
                path: copy.to_string_lossy().into_owned(),
                discard: false,
            },
            saved.generation,
            saved.revision,
        )
        .unwrap();
    assert_eq!(
        serde_json::to_value(reopened.cues).unwrap(),
        serde_json::to_value(accepted.cues).unwrap()
    );
    assert!(!reopened.dirty && !reopened.can_undo);
}

#[test]
fn worker_failure_can_be_retried_and_shutdown_cancels_and_joins_outside_document_lock() {
    let (_dir, docs) = fixture();
    let id = docs
        .text_start(
            1,
            0,
            Operation::Find {
                query: String::new(),
                case_sensitive: false,
            },
        )
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let view = docs.text_view(id, 0, 100).unwrap();
        if view.phase == Phase::Failed {
            assert!(view.error.unwrap().contains("Find text"));
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert!(docs.text_accept(id, None).is_err());
    let newer = start(&docs);
    ready(&docs, newer);
    let stopped = Arc::new(AtomicBool::new(false));
    let signal = stopped.clone();
    let documents = docs.clone();
    let (entered, rx) = std::sync::mpsc::channel();
    {
        let mut store = docs.0.lock().unwrap();
        let key = store.quality_key().unwrap();
        let profile = store.quality.context().profile;
        store
            .texts
            .launch(key, operation(), profile, move |cancel| {
                entered.send(()).unwrap();
                let deadline = Instant::now() + Duration::from_secs(5);
                while !cancel.load(Ordering::Relaxed) && Instant::now() < deadline {
                    std::thread::yield_now();
                }
                signal.store(
                    cancel.load(Ordering::Relaxed) && documents.0.try_lock().is_ok(),
                    Ordering::Release,
                );
                Err("Stopped".into())
            })
            .unwrap();
    }
    rx.recv_timeout(Duration::from_secs(5)).unwrap();
    docs.shutdown_quality().unwrap();
    assert!(stopped.load(Ordering::Acquire));
    assert!(docs.0.lock().unwrap().texts.job.is_none());
}
