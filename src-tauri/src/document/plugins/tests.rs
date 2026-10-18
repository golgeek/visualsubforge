use super::*;
use crate::{
    plugin_review::{Input, Mode, Phase},
    scenes::Scenes as SceneOwner,
};
use std::time::Instant;

pub(super) fn samples() -> Vec<(&'static str, &'static str, serde_json::Value)> {
    let f = Fixture::new("srt", false);
    let id = f.start(input());
    let mut review = ready(&f.docs, id);
    // Generated type fixtures must not depend on a random temporary directory.
    review.path = "/fixtures/plugin/manifest.json".into();
    let chunk = f.docs.plugin_text(id, 0, true, 0, 4096).unwrap();
    let mut invocation = input();
    invocation
        .settings
        .insert("flag".into(), view::Scalar::Boolean(true));
    invocation
        .settings
        .insert("name".into(), view::Scalar::Text("Été".into()));
    invocation.settings.insert(
        "number".into(),
        view::Scalar::Number(3.5.try_into().unwrap()),
    );
    vec![
        (
            "pluginInput",
            "PluginInput",
            serde_json::to_value(invocation).unwrap(),
        ),
        (
            "pluginReview",
            "PluginReview",
            serde_json::to_value(review).unwrap(),
        ),
        (
            "pluginTextChunk",
            "PluginTextChunk",
            serde_json::to_value(chunk).unwrap(),
        ),
    ]
}

const SRT: &str = "7\n00:00:01,000 --> 00:00:02,000\n{\\i1}Été 😀{\\i0}\n\n9\n00:00:03,000 --> 00:00:04,000\n<i>世界</i>\n\n";
struct Fixture {
    root: tempfile::TempDir,
    docs: Documents,
    scenes: SceneOwner,
    path: String,
    original: String,
    source: PathBuf,
}
impl Fixture {
    fn new(extension: &str, legacy: bool) -> Self {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join(format!("source 世界.{extension}"));
        let original = match extension {
            "srt" => SRT,
            "ass" => include_str!("../../../../tests/fixtures/subtitles/resync.ass"),
            "ssa" => include_str!("../../../../tests/fixtures/subtitles/resync.ssa"),
            "vtt" => include_str!("../../../../tests/fixtures/subtitles/styles.vtt"),
            _ => unreachable!(),
        }
        .to_owned();
        fs::write(&source, &original).unwrap();
        let docs = Documents::default();
        docs.apply(
            Action::Open {
                path: source.to_string_lossy().into(),
                discard: false,
            },
            0,
            0,
        )
        .unwrap();
        let package = root.path().join("package 字幕");
        fs::create_dir(&package).unwrap();
        let manifest = package.join("manifest.json");
        fs::write(&manifest,serde_json::json!({"version":1,"id":"test.append","name":"Append marker","api":if legacy {"vss-1"} else {"vsf-1"},"entry":"main.js","requires":["rules","fixes"]}).to_string()).unwrap();
        fs::write(package.join("main.js"),if legacy {"VSSPlugin={Name:'Append marker',HasError(){return 'append'},FixError(c){c.Text+='!'}}"} else {"VSFPlugin={check(){return 'append'},fix(c,h){h.update(c.id,{text:c.text+'!'})}}"}).unwrap();
        let scenes = SceneOwner::default();
        scenes.test_completed(7, vec![1.5, 3.5], false);
        Self {
            root,
            docs,
            scenes,
            path: manifest.to_string_lossy().into(),
            original,
            source,
        }
    }
    fn start(&self, input: Input) -> u64 {
        let snapshot = self.docs.apply(Action::Status, 0, 0).unwrap();
        self.docs
            .plugin_start_with(
                snapshot.generation,
                snapshot.revision,
                self.scenes.capture_quality(7, true).unwrap(),
                self.path.clone(),
                input,
                local_engine,
            )
            .unwrap()
    }
    fn script(&self, text: &str) {
        fs::write(
            Path::new(&self.path).parent().unwrap().join("main.js"),
            text,
        )
        .unwrap();
    }
}
fn input() -> Input {
    Input {
        mode: Mode::Fix,
        scope: vec![],
        selection: vec![],
        cursor_ms: Some(1234.try_into().unwrap()),
        settings: BTreeMap::new(),
        cps_target: None,
        rs_target_index: None,
        scene_filter_offset_ms: 0,
    }
}
// Run the same engine/protocol without a child so admission/lifetime races can be
// controlled independently of OS scheduling. A separate opt-in test uses the
// installed CLI beside the desktop executable, including relocated packages.
fn local_engine(
    execution: engine::Execution,
    _: &AtomicBool,
    _: Duration,
) -> Result<serde_json::Value> {
    let request = vsf_plugin_host::Request::new(execution.script, execution.input);
    let mut bytes = vec![];
    vsf_plugin_host::serve(
        std::io::Cursor::new(serde_json::to_vec(&request).unwrap()),
        &mut bytes,
    )
    .map_err(|e| e.to_string())?;
    #[derive(Deserialize)]
    struct Reply {
        result: std::result::Result<serde_json::Value, vsf_plugin_host::Failure>,
    }
    serde_json::from_slice::<Reply>(&bytes)
        .unwrap()
        .result
        .map_err(|e| e.to_string())
}
fn finished(docs: &Documents, id: u64) -> view::View {
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        let v = docs.plugin_view(id, 0, 0, 200).unwrap();
        if v.phase != Phase::Preparing && v.phase != Phase::Cancelling {
            return v;
        }
        assert!(Instant::now() < until, "Plugin owner did not finish");
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn ready(docs: &Documents, id: u64) -> view::View {
    let v = finished(docs, id);
    assert_eq!(v.phase, Phase::Reviewing, "{:?}", v.error);
    v
}
fn value(docs: &Documents) -> serde_json::Value {
    serde_json::to_value(docs.apply(Action::Status, 0, 0).unwrap()).unwrap()
}
fn accepted(f: &Fixture, id: u64, request: &Input) -> Snapshot {
    let verified = f
        .docs
        .plugin_begin_accept(id, request)
        .unwrap()
        .verify()
        .unwrap();
    f.scenes
        .with_quality_current(verified.scene_key(), || f.docs.plugin_commit(&verified))
        .unwrap()
}

#[test]
fn modern_vss_four_format_fixes_use_one_history_entry_and_existing_save_recovery() {
    for legacy in [false, true] {
        for extension in ["srt", "ass", "ssa", "vtt"] {
            let f = Fixture::new(extension, legacy);
            let before = value(&f.docs);
            let (source_properties, original_document) = {
                let store = f.docs.0.lock().unwrap();
                let loaded = store.loaded.as_ref().unwrap();
                (loaded.source().clone(), loaded.editor.document().clone())
            };
            let id = f.start(input());
            let preview = ready(&f.docs, id);
            assert_eq!(preview.changes_total, 2);
            assert_eq!(preview.findings_total, 2);
            assert_eq!(value(&f.docs), before);
            let snapshot = accepted(&f, id, &input());
            assert_eq!(snapshot.revision, 1);
            assert!(snapshot.dirty && snapshot.can_undo);
            {
                let store = f.docs.0.lock().unwrap();
                let loaded = store.loaded.as_ref().unwrap();
                assert!(loaded.source() == &source_properties);
                for (before, after) in original_document
                    .cues()
                    .iter()
                    .zip(loaded.editor.document().cues())
                {
                    assert_eq!(before.metadata(), after.metadata());
                }
            }
            for (a, b) in snapshot.cues.iter().zip(before["cues"].as_array().unwrap()) {
                assert_eq!(a.text, format!("{}!", b["text"].as_str().unwrap()));
                assert_eq!(a.start_ms, b["startMs"].as_u64().unwrap());
                assert_eq!(a.end_ms, b["endMs"].as_u64().unwrap());
            }
            assert_eq!(
                f.docs.plugin_view(id, 0, 0, 20).unwrap().phase,
                Phase::Accepted
            );
            assert!(f.docs.plugin_begin_accept(id, &input()).is_err());
            let fixed = {
                let store = f.docs.0.lock().unwrap();
                let l = store.loaded.as_ref().unwrap();
                l.source().serialize(l.editor.document()).unwrap()
            };
            let undone = f.docs.apply(Action::Undo, 1, 1).unwrap();
            assert_eq!(
                serde_json::to_value(&undone).unwrap()["cues"],
                before["cues"]
            );
            assert!(!undone.can_undo && undone.can_redo);
            let redone = f.docs.apply(Action::Redo, 1, 2).unwrap();
            assert_eq!(redone.cues[0].text, snapshot.cues[0].text);
            let bad = f
                .root
                .path()
                .join("missing")
                .join(format!("out.{extension}"));
            let before_failure = value(&f.docs);
            assert!(
                f.docs
                    .apply(
                        Action::Save {
                            path: Some(bad.to_string_lossy().into())
                        },
                        1,
                        3
                    )
                    .is_err()
            );
            assert_eq!(value(&f.docs), before_failure);
            let recovery = f.root.path().join("recovery");
            let session = crate::recovery_store::Session::create(&recovery).unwrap();
            let format = subtitle_formats::Format::from_extension(extension).unwrap();
            let record = crate::recovery_store::Record::new_document(
                format,
                Some(f.source.to_string_lossy().into()),
                f.original.clone(),
                fixed.clone(),
                1,
                3,
            )
            .unwrap();
            session.prepare(&record).unwrap().commit().unwrap();
            drop(session);
            let entries = crate::recovery_store::list(&recovery).unwrap();
            let mut restored = Store {
                recovery_root: Some(recovery),
                ..Store::default()
            };
            restored
                .apply(
                    Action::Recover {
                        id: entries[0].id.clone(),
                        discard: true,
                    },
                    0,
                    0,
                )
                .unwrap();
            let l = restored.loaded.as_ref().unwrap();
            assert_eq!(l.source().serialize(l.editor.document()).unwrap(), fixed);
            assert!(restored.snapshot().requires_save_as);
            let output = f.root.path().join(format!("fixed.{extension}"));
            f.docs
                .apply(
                    Action::Save {
                        path: Some(output.to_string_lossy().into()),
                    },
                    1,
                    3,
                )
                .unwrap();
            assert_eq!(fs::read_to_string(&output).unwrap(), fixed);
            assert_eq!(fs::read_to_string(&f.source).unwrap(), f.original);
            let reopened = Documents::default();
            reopened
                .apply(
                    Action::Open {
                        path: output.to_string_lossy().into(),
                        discard: false,
                    },
                    0,
                    0,
                )
                .unwrap();
            assert_eq!(value(&reopened)["cues"], value(&f.docs)["cues"]);
        }
    }
}

#[test]
fn explicit_scene_selection_preferences_and_neighbor_review_are_captured() {
    let f = Fixture::new("srt", true);
    f.script("VSSPlugin={Name:'Neighbor',ParamMode:{Value:1},HasError(c){if(VSSCore.GetFirstSelected().Index!==2||VSSCore.GetSelectedCount()!==1||VSSCore.GetAudioCursorPosition()!==1234||VSSCore.MinimumBlank!==80||VSSCore.CpsTarget!==18||VSSCore.RsTargetIndex!==2||SceneChange.StartOffset!==130||SceneChange.StopOffset!==130||SceneChange.FilterOffset!==25||SceneChange.GetNext(1000)!==1500||this.ParamMode.Value!==3)throw Error('bad context');return 'neighbor'},FixError(c,p,n){n.Text+='!';ScriptLog('neighbor changed')}}");
    let mut i = input();
    i.scope = vec!["1".into()];
    i.selection = vec!["2".into()];
    i.cps_target = Some(18.0.try_into().unwrap());
    i.rs_target_index = Some(2);
    i.scene_filter_offset_ms = 25;
    i.settings.insert(
        "ParamMode".into(),
        view::Scalar::Number(3.0.try_into().unwrap()),
    );
    let id = f.start(i.clone());
    let v = ready(&f.docs, id);
    assert_eq!(v.scope_count, 1);
    assert_eq!(v.selection_count, 1);
    assert_eq!(v.scene_coverage, SceneCoverage::Checked);
    assert_eq!(v.rows[0].cue_id, "2");
    assert!(v.rows[0].outside_scope);
    assert_eq!(v.logs, ["neighbor changed"]);
    accepted(&f, id, &i);
    let mut check = input();
    check.mode = Mode::Check;
    f.script("VSSPlugin={Name:'Check',HasError(){return 'finding'}}");
    let before = value(&f.docs);
    let id = f.start(check.clone());
    let v = ready(&f.docs, id);
    assert_eq!(v.findings_total, 2);
    assert_eq!(v.changes_total, 0);
    assert!(f.docs.plugin_begin_accept(id, &check).is_err());
    assert_eq!(value(&f.docs), before);
}

#[test]
fn long_unicode_changes_are_paged_with_exact_full_text_chunks() {
    let f = Fixture::new("srt", false);
    let text = "Été😀".repeat(1500);
    f.docs
        .apply(
            Action::Edit {
                cue_id: 1,
                start_ms: 1000,
                end_ms: 2000,
                text: text.clone(),
            },
            1,
            0,
        )
        .unwrap();
    let id = f.start(input());
    let v = ready(&f.docs, id);
    assert!(v.rows[0].before.text.truncated);
    assert!(v.rows[0].before.text.text.len() <= PREVIEW_BYTES);
    let page = f.docs.plugin_view(id, 1, 1, 1).unwrap();
    assert_eq!(page.findings.len(), 1);
    assert_eq!(page.rows.len(), 1);
    assert_eq!(page.rows[0].cue_id, "2");
    for limit in [0, 201] {
        assert!(f.docs.plugin_view(id, 0, 0, limit).is_err());
    }
    let mut full = String::new();
    let mut offset = 0;
    loop {
        let part = f.docs.plugin_text(id, 0, true, offset, 4096).unwrap();
        full.push_str(&part.text);
        assert!(part.next_offset > offset);
        offset = part.next_offset;
        if offset == part.total_bytes {
            break;
        }
    }
    assert_eq!(full, text + "!");
    assert!(f.docs.plugin_text(id, 0, true, 1, 4096).is_err());
    assert!(f.docs.plugin_text(id, 0, true, 0, 65537).is_err());
    f.docs.plugin_cancel(id).unwrap();
    assert!(f.docs.plugin_text(id, 0, true, 0, 4096).is_err());
}

#[test]
fn stale_document_profile_scene_or_transient_context_cannot_accept() {
    for change in [
        "edit-undo",
        "profile",
        "replace",
        "close",
        "scene",
        "selection",
        "cursor",
        "settings",
    ] {
        let f = Fixture::new("srt", false);
        let id = f.start(input());
        ready(&f.docs, id);
        let mut invocation = input();
        match change {
            "edit-undo" => {
                f.docs
                    .apply(
                        Action::Edit {
                            cue_id: 1,
                            start_ms: 1000,
                            end_ms: 2000,
                            text: "changed".into(),
                        },
                        1,
                        0,
                    )
                    .unwrap();
                f.docs.apply(Action::Undo, 1, 1).unwrap();
            }
            "profile" => {
                let mut profile = vsf_application::quality::Profile::default()
                    .custom("custom".into(), "Custom".into());
                profile.rules.duration.minimum_ms += 1;
                f.docs
                    .initialize_quality(
                        crate::quality_profile::Policy::new(profile, Default::default()).unwrap(),
                    )
                    .unwrap();
            }
            "replace" => {
                f.docs
                    .apply(
                        Action::Open {
                            path: f.source.to_string_lossy().into(),
                            discard: true,
                        },
                        1,
                        0,
                    )
                    .unwrap();
            }
            "close" => {
                f.docs.apply(Action::Close { discard: true }, 1, 0).unwrap();
            }
            "scene" => f.scenes.test_completed(7, vec![2.0], false),
            "selection" => invocation.selection = vec!["1".into()],
            "cursor" => invocation.cursor_ms = Some(1235.try_into().unwrap()),
            "settings" => {
                invocation
                    .settings
                    .insert("new".into(), view::Scalar::Boolean(true));
            }
            _ => unreachable!(),
        }
        let before = value(&f.docs);
        if change == "scene" {
            let pending = f
                .docs
                .plugin_begin_accept(id, &invocation)
                .unwrap()
                .verify()
                .unwrap();
            assert!(
                f.scenes
                    .with_quality_current(pending.scene_key(), || f.docs.plugin_commit(&pending))
                    .is_err()
            );
            f.docs.plugin_invalidate(id).unwrap();
        } else {
            assert!(
                f.docs.plugin_begin_accept(id, &invocation).is_err(),
                "{change}"
            );
        }
        assert_eq!(value(&f.docs), before);
        assert_eq!(fs::read_to_string(&f.source).unwrap(), f.original);
    }
}

#[test]
fn reserved_acceptance_rechecks_package_document_cancel_and_shutdown_off_lock() {
    for change in ["package", "edit", "cancel", "shutdown", "expired"] {
        let f = Fixture::new("srt", false);
        let id = f.start(input());
        ready(&f.docs, id);
        let reservation = f.docs.plugin_begin_accept(id, &input()).unwrap();
        assert_eq!(
            f.docs.plugin_view(id, 0, 0, 10).unwrap().phase,
            Phase::Accepting
        );
        assert!(
            f.docs
                .plugin_start(
                    1,
                    0,
                    f.scenes.capture_quality(7, true).unwrap(),
                    f.path.clone(),
                    input()
                )
                .is_err()
        );
        if change == "package" {
            f.script("// changed");
            assert!(reservation.verify().is_err());
            assert!(!value(&f.docs)["dirty"].as_bool().unwrap());
            continue;
        }
        let mut verified = reservation.verify().unwrap();
        let mut shutdown = None;
        match change {
            "edit" => {
                f.docs
                    .apply(
                        Action::Edit {
                            cue_id: 1,
                            start_ms: 1000,
                            end_ms: 2000,
                            text: "external".into(),
                        },
                        1,
                        0,
                    )
                    .unwrap();
            }
            "cancel" => f.docs.plugin_cancel(id).unwrap(),
            "expired" => verified.deadline = Instant::now(),
            "shutdown" => {
                let closed = f.docs.clone();
                let (done, wait) = mpsc::channel();
                let handle = std::thread::spawn(move || {
                    closed.shutdown_plugins().unwrap();
                    done.send(()).unwrap();
                });
                let until = Instant::now() + Duration::from_secs(3);
                loop {
                    if f.docs.0.lock().unwrap().plugins.closed {
                        break;
                    }
                    assert!(Instant::now() < until);
                    std::thread::sleep(Duration::from_millis(1));
                }
                assert!(wait.try_recv().is_err());
                assert!(f.docs.0.try_lock().is_ok());
                shutdown = Some((handle, wait));
            }
            _ => unreachable!(),
        }
        let before = value(&f.docs);
        assert!(f.docs.plugin_commit(&verified).is_err());
        assert_eq!(value(&f.docs), before);
        drop(verified);
        if let Some((handle, wait)) = shutdown {
            wait.recv_timeout(Duration::from_secs(3)).unwrap();
            handle.join().unwrap();
        }
        assert_eq!(fs::read_to_string(&f.source).unwrap(), f.original);
    }
}

#[test]
fn production_media_scene_document_guard_refuses_replacement_and_stopping() {
    for stopping in [false, true] {
        let f = Fixture::new("srt", false);
        f.scenes.test_completed(1, vec![1.5], false);
        let media = crate::media::Media::test_selection(&f.source);
        let id = f
            .docs
            .plugin_start_with(
                1,
                0,
                f.scenes.capture_quality(1, true).unwrap(),
                f.path.clone(),
                input(),
                local_engine,
            )
            .unwrap();
        ready(&f.docs, id);
        let key = f.docs.plugin_context(id).unwrap();
        crate::plugin_review::with_current_owners(&media, &f.scenes, &f.docs, id, key, |docs| {
            docs.plugin_view(id, 0, 0, 10)
        })
        .unwrap();
        let pending = f
            .docs
            .plugin_begin_accept(id, &input())
            .unwrap()
            .verify()
            .unwrap();
        let current = if stopping {
            assert!(media.begin_shutdown());
            media
        } else {
            crate::media::Media::default()
        };
        let before = value(&f.docs);
        assert!(
            crate::plugin_review::with_current_owners(
                &current,
                &f.scenes,
                &f.docs,
                id,
                pending.scene_key(),
                |docs| docs.plugin_commit(&pending)
            )
            .is_err()
        );
        assert_eq!(value(&f.docs), before);
        assert_eq!(
            f.docs.plugin_view(id, 0, 0, 10).unwrap().phase,
            Phase::Stale
        );
    }
}

#[test]
fn running_admission_cancellation_and_shutdown_never_hold_document_mutex() {
    for shutdown in [false, true] {
        let f = Fixture::new("srt", false);
        let (entered, wait) = mpsc::channel();
        let id = f
            .docs
            .plugin_start_with(
                1,
                0,
                f.scenes.capture_quality(7, true).unwrap(),
                f.path.clone(),
                input(),
                move |_, cancel, _| {
                    entered.send(()).unwrap();
                    let until = Instant::now() + Duration::from_secs(3);
                    while !cancel.load(Ordering::Acquire) {
                        assert!(Instant::now() < until);
                        std::thread::sleep(Duration::from_millis(1));
                    }
                    Err("cancelled".into())
                },
            )
            .unwrap();
        wait.recv_timeout(Duration::from_secs(3)).unwrap();
        let before = value(&f.docs);
        assert!(
            f.docs
                .plugin_start(
                    1,
                    0,
                    f.scenes.capture_quality(7, true).unwrap(),
                    f.path.clone(),
                    input()
                )
                .is_err()
        );
        if shutdown {
            f.docs.shutdown_plugins().unwrap();
            assert!(
                f.docs
                    .plugin_start(
                        1,
                        0,
                        f.scenes.capture_quality(7, true).unwrap(),
                        f.path.clone(),
                        input()
                    )
                    .is_err()
            );
        } else {
            f.docs.plugin_cancel(id).unwrap();
            assert_eq!(finished(&f.docs, id).phase, Phase::Cancelled);
            let next = f.start(input());
            assert_ne!(id, next);
            ready(&f.docs, next);
            f.docs.plugin_cancel(id).unwrap();
            assert_eq!(
                f.docs.plugin_view(next, 0, 0, 20).unwrap().phase,
                Phase::Reviewing
            );
        }
        assert_eq!(value(&f.docs), before);
    }
}

#[test]
fn failed_engine_invalid_proposal_or_worker_panic_preserves_redo_and_all_state() {
    for failure in ["throw", "invalid", "panic", "changed"] {
        let f = Fixture::new("srt", false);
        f.docs
            .apply(
                Action::Edit {
                    cue_id: 1,
                    start_ms: 1000,
                    end_ms: 2000,
                    text: "change".into(),
                },
                1,
                0,
            )
            .unwrap();
        f.docs.apply(Action::Undo, 1, 1).unwrap();
        let before = value(&f.docs);
        assert_eq!(before["canRedo"], true);
        match failure {
            "throw"=>f.script("VSFPlugin={check(){return 'x'},fix(c,h){h.update(c.id,{text:'partial'});throw Error('abort')}}"),
            "invalid"=>f.script("VSFPlugin={check(){return 'x'},fix(c,h){h.update(c.id,{endMs:c.startMs})}}"),_=>{},
        }
        let path = f.path.clone();
        let id = f
            .docs
            .plugin_start_with(
                1,
                2,
                f.scenes.capture_quality(7, true).unwrap(),
                f.path.clone(),
                input(),
                move |e, c, t| {
                    if failure == "panic" {
                        panic!("controlled owner test");
                    }
                    let value = local_engine(e, c, t)?;
                    if failure == "changed" {
                        fs::write(path, "{}").unwrap();
                    }
                    Ok(value)
                },
            )
            .unwrap();
        let v = finished(&f.docs, id);
        assert_eq!(v.phase, Phase::Failed);
        assert!(v.error.is_some());
        assert!(f.docs.plugin_begin_accept(id, &input()).is_err());
        assert_eq!(value(&f.docs), before);
    }
}

#[test]
#[ignore = "Requires a freshly built desktop/CLI pair in VSF_PLUGIN_DESKTOP_EXECUTABLE"]
fn installed_cli_helper_runs_from_desktop_directory_and_relocated_packages() {
    let executable = std::env::var_os("VSF_PLUGIN_DESKTOP_EXECUTABLE")
        .expect("Set a freshly built desktop binary");
    let desktop = Path::new(&executable);
    let runner = runner_path(desktop).unwrap();
    for copy in 0..3 {
        let f = Fixture::new("srt", true);
        let package = f.root.path().join(format!("App copy {copy} 世界"));
        fs::create_dir(&package).unwrap();
        let installed = package.join(desktop.file_name().unwrap());
        fs::write(&installed, "desktop path fixture; it is never executed").unwrap();
        fs::copy(&runner, package.join(runner.file_name().unwrap())).unwrap();
        let id = f
            .docs
            .plugin_start_with(
                1,
                0,
                f.scenes.capture_quality(7, true).unwrap(),
                f.path.clone(),
                input(),
                move |e, c, t| execute_from(&installed, e, c, t),
            )
            .unwrap();
        ready(&f.docs, id);
        accepted(&f, id, &input());
        assert_eq!(value(&f.docs)["revision"], 1);
    }
}

#[test]
fn plugin_runner_uses_only_the_installation_sibling_and_rejects_missing_or_aliases() {
    let root = tempfile::tempdir().unwrap();
    let desktop = root.path().join("visualsubforge");
    assert!(runner_path(&desktop).is_err());
    let runner = root
        .path()
        .join(format!("vsf{}", std::env::consts::EXE_SUFFIX));
    fs::create_dir(&runner).unwrap();
    assert!(runner_path(&desktop).is_err());
    fs::remove_dir(&runner).unwrap();
    fs::write(&runner, "regular fixture").unwrap();
    assert_eq!(runner_path(&desktop).unwrap(), runner);
    #[cfg(unix)]
    {
        fs::remove_file(&runner).unwrap();
        fs::write(&desktop, "fixture").unwrap();
        std::os::unix::fs::symlink(&desktop, &runner).unwrap();
        assert!(runner_path(&desktop).is_err());
    }
}
