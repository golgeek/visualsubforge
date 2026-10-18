//! Actual helper execution plus pure application validation and domain history.
//! All inline scripts are independently authored behavior probes, not legacy code.
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path, sync::atomic::AtomicBool};
use subtitle_core::{CueId, Editor, Milliseconds};
use subtitle_formats::{Format, Parsed, parse};
use vsf_application::{
    plugins::{
        Api, Capture, Context, Key, Manifest, Mode, Package, Preferences, Proposal, Request, Scenes,
    },
    subtitle_file,
};

fn fixture() -> Parsed {
    parse("1\n00:00:01,000 --> 00:00:01,900\n{\\i1}Été 😀{\\i0}  \nsecond\n\n2\n00:00:02,000 --> 00:00:02,900\nWorld\t\n\n3\n00:00:03,000 --> 00:00:03,900\n世界  \n",Some(Format::Srt)).unwrap()
}
fn request(p: &Parsed, mode: Mode) -> Request {
    Request {
        key: Key {
            generation: u64::MAX,
            revision: 0,
            context: 7,
        },
        mode,
        scope: p.document.cues().iter().map(|c| c.id()).collect(),
        settings: BTreeMap::new(),
        context: Context {
            selection: vec![
                p.document.cues()[0].id(),
                p.document.cues()[2.min(p.document.cues().len() - 1)].id(),
            ],
            cursor: Some(Milliseconds::new(1500).unwrap()),
            preferences: Preferences {
                minimum_blank_ms: 80,
                minimum_duration_ms: 500,
                maximum_duration_ms: 6000,
                cps_target: 20.0,
                rs_target_index: Some(1),
            },
            scenes: Some(Scenes {
                cuts_ms: vec![0, 1500, 3000],
                start_offset_ms: 0,
                stop_offset_ms: 130,
                filter_offset_ms: 250,
            }),
        },
    }
}
fn package(api: Api, script: &str) -> Package {
    Package {
        manifest: Manifest {
            version: 1,
            id: "test.probe".into(),
            name: "Probe".into(),
            api,
            entry: "main.js".into(),
            requires: vec![],
            settings: BTreeMap::new(),
        },
        files: BTreeMap::from([("main.js".into(), script.into())]),
    }
}
fn capture(p: &Parsed, package: Package, r: &Request) -> Result<Capture, String> {
    Capture::new(
        p.document.clone(),
        p.source.clone(),
        package,
        r.clone(),
        || Ok(()),
    )
}
fn output(c: &Capture) -> Result<Value, String> {
    let execution = c.execution(|| Ok(()))?;
    vsf_plugin_host::run(
        Path::new(env!("CARGO_BIN_EXE_vsf-plugin-worker")),
        &[],
        &vsf_plugin_host::Request::new(execution.script, execution.input),
        &AtomicBool::new(false),
    )
    .map_err(|e| e.to_string())
}
fn invoke(p: &Parsed, package: Package, r: &Request) -> Result<Proposal, String> {
    let c = capture(p, package, r)?;
    let value = output(&c)?;
    c.validate(value, || Ok(()))
}
fn apply(e: &mut Editor<subtitle_formats::Source>, p: &Proposal, r: &Request) {
    let edit = p.edit(e.document(), e.properties(), r).unwrap();
    e.apply_edit_with_properties(r.key.revision, edit, subtitle_file::validate_document)
        .unwrap();
}
fn vss(body: &str) -> Package {
    package(Api::Vss1, &format!("VSSPlugin={{Name:'Probe',{body}}};"))
}
fn modern(body: &str) -> Package {
    package(Api::Vsf1, &format!("VSFPlugin={{{body}}};"))
}

#[test]
fn authored_modern_and_vss_rules_have_reviewed_fixes_and_one_undo() {
    for (manifest, script) in [
        (
            include_str!("../../../examples/plugins/trim-trailing/manifest.json"),
            include_str!("../../../examples/plugins/trim-trailing/main.js"),
        ),
        (
            include_str!("../../../examples/plugins/vss-trim-trailing/manifest.json"),
            include_str!("../../../examples/plugins/vss-trim-trailing/main.js"),
        ),
    ] {
        let package = Package {
            manifest: serde_json::from_str(manifest).unwrap(),
            files: BTreeMap::from([("main.js".into(), script.into())]),
        };
        let p = fixture();
        let check = request(&p, Mode::Check);
        let report = invoke(&p, package.clone(), &check).unwrap();
        assert_eq!(report.findings().len(), 2);
        assert!(report.rows().is_empty());
        assert!(report.can_fix());
        let fix = request(&p, Mode::Fix);
        let proposal = invoke(&p, package, &fix).unwrap();
        assert_eq!(proposal.rows().len(), 2);
        assert!(proposal.rows().iter().all(|r| r.warning.is_none()));
        let mut e = Editor::with_properties(p.document.clone(), p.source.clone());
        apply(&mut e, &proposal, &fix);
        assert_eq!(e.document().cues()[1].text(), "World");
        assert_eq!(e.document().cues()[2].text(), "世界");
        assert_eq!(e.document().cues()[0], p.document.cues()[0]);
        let after = e.document().clone();
        e.undo(1).unwrap();
        assert_eq!(e.document(), &p.document);
        assert!(!e.can_undo());
        assert!(!e.dirty());
        e.redo(2).unwrap();
        assert_eq!(e.document(), &after);
        assert!(e.dirty());
    }
}

#[test]
fn neighbor_wrappers_are_stable_and_observe_all_transaction_writes() {
    let p = fixture();
    let mut r = request(&p, Mode::Fix);
    r.scope.truncate(1);
    let script = vss(
        "HasError(c,p,n){if(p!==null||n!==VSSCore.GetSubAt(1)||c!==VSSCore.GetFirst()||VSSCore.GetNext(c)!==n||VSSCore.GetPrevious(n)!==c)throw Error('wrapper identity');return 'repair'},FixError(c,p,n){c.Stop=1800;n.Start=1850;n.Text+=' corrected';if(VSSCore.GetNext(c).Start!==1850||n.Text!=='World\\t corrected')throw Error('read writes');}",
    );
    let proposal = invoke(&p, script, &r).unwrap();
    assert_eq!(proposal.rows().len(), 2);
    assert!(!proposal.rows()[0].outside_scope);
    assert!(proposal.rows()[1].outside_scope);
    let mut e = Editor::with_properties(p.document.clone(), p.source.clone());
    apply(&mut e, &proposal, &r);
    assert_eq!(e.document().cues()[0].end().get(), 1800);
    assert_eq!(e.document().cues()[1].start().get(), 1850);
    e.undo(1).unwrap();
    assert_eq!(e.document(), &p.document);
    assert!(!e.can_undo());
    // Every next check sees edits from the previous fix, in document order.
    let script = vss(
        "HasError(c,p){if(p&&p.Text!=='fixed')throw Error('earlier write');return 'repair'},FixError(c){c.Text='fixed'}",
    );
    assert_eq!(
        invoke(&p, script, &request(&p, Mode::Fix))
            .unwrap()
            .rows()
            .len(),
        3
    );
}

#[test]
fn utf16_crlf_stripping_alias_and_unclosed_tag_semantics_are_explicit() {
    let text = "{\\i1}<b>😀</b>{\\i0}\n&amp;\\N";
    let mut p = fixture();
    p.document = parse(
        &format!("1\n00:00:01,000 --> 00:00:02,000\n{text}\n"),
        Some(Format::Srt),
    )
    .unwrap()
    .document;
    let mut r = request(&fixture(), Mode::Fix);
    r.scope = vec![CueId(1)];
    r.context.selection = vec![CueId(1)];
    let script = vss(
        "HasError(c){if(c.StrippedText!=='😀\\r\\n&amp;\\\\N'||c.StrippedText.length!==11||c.Text.indexOf('\\r\\n')<0)throw Error('UTF16/CRLF');return 'strip'},FixError(c){c.StrippedText=c.StrippedText}",
    );
    let proposal = invoke(&p, script, &r).unwrap();
    assert_eq!(proposal.rows()[0].after.text, "😀\n&amp;\\N");
    assert!(proposal.rows()[0].warning.is_some());
    for (raw, expected) in [
        (
            "before {\\broken <b>after</b>",
            "before {\\broken <b>after</b>",
        ),
        ("before <broken {\\i1}after", "before <broken {\\i1}after"),
        ("{\\}", "{\\}"),
        ("{\\}x", "x"),
        ("{plain}<b>x</b>", "{plain}x"),
    ] {
        p.document = parse(
            &format!("1\n00:00:01,000 --> 00:00:02,000\n{raw}\n"),
            Some(Format::Srt),
        )
        .unwrap()
        .document;
        let body = format!(
            "HasError(c){{if(c.StrippedText!=={})throw Error('strip mismatch');return ''}}",
            json!(expected)
        );
        let mut check = r.clone();
        check.mode = Mode::Check;
        invoke(&p, vss(&body), &check).unwrap();
    }
}

#[test]
fn vss_text_readback_preserves_unicode_and_normalizes_only_line_endings() {
    let p = fixture();
    let mut r = request(&p, Mode::Fix);
    r.scope = vec![CueId(1)];
    for (written, accepted) in [
        (
            "{\\i1}Été 世界 😀{\\i0}\\Nliteral",
            "{\\i1}Été 世界 😀{\\i0}\\Nliteral",
        ),
        ("Été\r\n世界\n😀", "Été\n世界\n😀"),
    ] {
        let visible = accepted.replace('\n', "\r\n");
        let script = vss(&format!(
            "HasError(){{return 'edit'}},FixError(c){{c.Text={};if(c.Text!=={}||VSSCore.GetSubAt(c.Index-1).Text!=={})throw Error('line readback')}}",
            json!(written),
            json!(visible),
            json!(visible),
        ));
        let proposal = invoke(&p, script, &r).unwrap();
        assert_eq!(proposal.rows().len(), 1);
        assert_eq!(proposal.rows()[0].after.text, accepted);
    }
    assert!(
        invoke(
            &p,
            vss("HasError(){return 'edit'},FixError(c){c.Text='bare\\rCR'}"),
            &r
        )
        .is_err()
    );
}

#[test]
fn exceptions_and_caught_invalid_mutations_publish_nothing() {
    let p = fixture();
    let r = request(&p, Mode::Fix);
    for body in [
        "HasError(){return 'fix'},FixError(c,p,n){c.Text='partial';n.Start+=1;throw Error('after write')}",
        "HasError(c){try{c.Text='bad'}catch(e){}return ''},FixError(){}",
        "HasError(){return 'fix'},FixError(c){c.Text='partial';try{c.Start=1.5}catch(e){}}",
        "HasError(){return 'fix'},FixError(c){c.Text='partial';try{VSSCore.MeasureStringWidth('x')}catch(e){}}",
        "HasError(){return 'fix'},FixError(c){c.Text='partial';return Promise.resolve()}",
        "HasError(){return 'fix'},FixError(c){c.Text='partial';Promise.resolve().then(()=>{c.Text='late'})}",
    ] {
        let e = Editor::with_properties(p.document.clone(), p.source.clone());
        assert!(invoke(&p, vss(body), &r).is_err(), "{body}");
        assert_eq!(e.document(), &p.document);
        assert_eq!(e.revision(), 0);
        assert!(!e.can_undo());
        assert!(!e.dirty());
    }
    for body in [
        "check(c,h){h.update(c.id,{text:'bad'});return ''}",
        "check(){return 'fix'},fix(c,h){h.update(c.id,{text:'partial',style:'bad'})}",
    ] {
        assert!(invoke(&p, modern(body), &r).is_err());
    }
}

#[test]
fn package_loading_globals_settings_and_fresh_regexp_state() {
    let p = fixture();
    let r = request(&p, Mode::Check);
    let mut pkg = package(
        Api::Vss1,
        "LoadScript('common/helper.js'); LoadScript('common/helper.js'); VSSPlugin={Name:'Probe',ParamFlag:{Value:false},HasError(c){if(loads!==2||deep!==7||!this.ParamFlag.Value)throw Error('settings/load');return expression.test('a')?'match':''}};",
    );
    pkg.files.insert("common/helper.js".into(),"var loads=(typeof loads==='undefined'?0:loads)+1;var expression=/a/g;LoadScript('../shared/deep.js');".into());
    pkg.files
        .insert("shared/deep.js".into(), "var deep=7;".into());
    pkg.manifest
        .settings
        .insert("ParamFlag".into(), json!(true));
    for _ in 0..2 {
        let report = invoke(&p, pkg.clone(), &r).unwrap();
        assert_eq!(
            report
                .findings()
                .iter()
                .map(|f| f.cue_id.as_str())
                .collect::<Vec<_>>(),
            ["1", "3"]
        );
        assert_eq!(report.settings()["ParamFlag"], true);
    }
    for path in [
        "missing.js",
        "../escape.js",
        "/absolute.js",
        "*.js",
        "C:\\other.js",
        "main.js",
    ] {
        let script = format!(
            "LoadScript({});VSSPlugin={{Name:'x',HasError(){{return ''}}}}",
            json!(path)
        );
        assert!(
            invoke(&p, package(Api::Vss1, &script), &r).is_err(),
            "{path}"
        );
    }
    let mut bad = r.clone();
    bad.settings.insert("ParamFlag".into(), json!("wrong type"));
    assert!(invoke(&p, pkg.clone(), &bad).is_err());
    bad.settings = BTreeMap::from([("unknown".into(), json!(true))]);
    assert!(invoke(&p, pkg, &bad).is_err());
    let mut pkg = modern("check(){return ''}");
    pkg.manifest.requires = vec!["gdi-text-width".into()];
    assert!(capture(&p, pkg, &r).err().unwrap().contains("Unsupported"));
}

#[test]
fn scene_selection_preferences_cursor_and_missing_capabilities() {
    let p = fixture();
    let r = request(&p, Mode::Check);
    let script = vss(
        "HasError(c){if(SceneChange.GetCount()!==3||SceneChange.GetAt(-1)!==-1||SceneChange.GetAt(3)!==-1||SceneChange.GetNext(1500)!==1500||SceneChange.GetPrevious(1500)!==1500||!SceneChange.Contains(1500,1500)||SceneChange.Contains(1501,2999)||SceneChange.GetNext(3001)!==-1||SceneChange.GetPrevious(-1)!==-1||SceneChange.StartOffset!==0||SceneChange.StopOffset!==130||SceneChange.FilterOffset!==250)throw Error('scenes');if(VSSCore.GetSelectedCount()!==2||VSSCore.GetFirstSelected().Index!==1||VSSCore.GetNextSelected(VSSCore.GetFirstSelected()).Index!==3||VSSCore.GetNextSelected(VSSCore.GetSubAt(2))!==null||VSSCore.GetAudioCursorPosition()!==1500||VSSCore.MinimumBlank!==80||VSSCore.MinimumDuration!==500||VSSCore.MaximumDuration!==6000||VSSCore.CpsTarget!==20||VSSCore.RsTargetIndex!==1)throw Error('context');return ''}",
    );
    invoke(&p, script, &r).unwrap();
    let mut empty = r.clone();
    empty.context.scenes.as_mut().unwrap().cuts_ms.clear();
    invoke(&p,vss("HasError(){if(SceneChange.GetCount()!==0||SceneChange.GetNext(0)!==-1||SceneChange.GetPrevious(0)!==-1)throw Error('empty');return ''}"),&empty).unwrap();
    empty.context.scenes = None;
    assert!(
        invoke(
            &p,
            vss("HasError(){SceneChange.GetCount();return ''}"),
            &empty
        )
        .err()
        .unwrap()
        .contains("unavailable")
    );
    let mut required = modern("check(){return ''}");
    required.manifest.requires.push("scene-data".into());
    assert!(capture(&p, required, &empty).is_err());
    empty.context.cursor = None;
    empty.context.preferences.rs_target_index = None;
    for call in [
        "VSSCore.GetAudioCursorPosition()",
        "VSSCore.RsTargetIndex",
        "VSSCore.RegisterJavascriptAction()",
        "VSSCore.GetTextSelection()",
    ] {
        assert!(
            invoke(&p, vss(&format!("HasError(){{{call};return ''}}")), &empty)
                .err()
                .unwrap()
                .contains("unavailable")
        );
    }
    empty.context.cursor = Some(Milliseconds::new(i32::MAX as u64 + 1).unwrap());
    assert!(capture(&p, vss("HasError(){return ''}"), &empty).is_err());
    assert!(capture(&p, modern("check(){return ''}"), &empty).is_ok());
}

#[test]
fn four_formats_retain_metadata_and_original_bytes_after_undo() {
    for raw in [
        "1\r\n00:00:01,000 --> 00:00:02,000\r\n{\\b1}😀{\\b0}\r\n\r\n2\r\n00:00:03,000 --> 00:00:04,000\r\nsecond\r\n\r\n",
        include_str!("../../../tests/fixtures/subtitles/styles.ass"),
        include_str!("../../../tests/fixtures/subtitles/styles.ssa"),
        include_str!("../../../tests/fixtures/subtitles/embedded-font.ass"),
        include_str!("../../../tests/fixtures/subtitles/styles.vtt"),
    ] {
        let p = parse(raw, None).unwrap();
        let mut r = request(&p, Mode::Fix);
        r.context.selection.clear();
        let proposal = invoke(
            &p,
            modern("check(){return 'append'},fix(c,h){h.update(c.id,{text:c.text+'!'})}"),
            &r,
        )
        .unwrap();
        let mut e = Editor::with_properties(p.document.clone(), p.source.clone());
        apply(&mut e, &proposal, &r);
        for (old, new) in p.document.cues().iter().zip(e.document().cues()) {
            assert_eq!(old.id(), new.id());
            assert_eq!(old.metadata(), new.metadata());
            assert_eq!(old.start(), new.start());
            assert_eq!(old.end(), new.end());
        }
        assert_eq!(e.properties(), &p.source);
        let saved = e.properties().serialize(e.document()).unwrap();
        let reopened = parse(&saved, Some(p.source.format())).unwrap();
        assert_eq!(&reopened.document, e.document());
        e.undo(1).unwrap();
        assert_eq!(e.properties().serialize(e.document()).unwrap(), raw);
        e.redo(2).unwrap();
        assert_eq!(e.properties().serialize(e.document()).unwrap(), saved);
    }
}

#[test]
fn format_precision_timed_effects_invalid_text_and_order_are_atomic() {
    let fix = modern("check(){return 'time'},fix(c,h){h.update(c.id,{endMs:c.endMs+1})}");
    for raw in [
        include_str!("../../../tests/fixtures/subtitles/styles.ass"),
        include_str!("../../../tests/fixtures/subtitles/styles.ssa"),
    ] {
        let p = parse(raw, None).unwrap();
        assert!(
            invoke(&p, fix.clone(), &request(&p, Mode::Fix))
                .err()
                .unwrap()
                .contains("precision")
        );
    }
    for text in ["{\\k20}song", "{\\t(0,100,\\fs30)}x", "{\\broken"] {
        let p=parse(&format!("1\n00:00:01,000 --> 00:00:02,000\n{text}\n\n2\n00:00:03,000 --> 00:00:04,000\nother\n"),None).unwrap();
        let r = request(&p, Mode::Fix);
        assert!(
            invoke(&p, fix.clone(), &r)
                .err()
                .unwrap()
                .contains("timed effects")
        );
        // Removing tags in the same proposal cannot bypass the old-text guard.
        assert!(
            invoke(
                &p,
                modern(
                    "check(){return 'time'},fix(c,h){h.update(c.id,{endMs:c.endMs+1,text:'plain'})}"
                ),
                &r
            )
            .is_err()
        );
        invoke(&p,modern("check(){return 'shift'},fix(c,h){h.update(c.id,{startMs:c.startMs+10,endMs:c.endMs+10})}"),&r).unwrap();
    }
    let p = parse(
        include_str!("../../../tests/fixtures/subtitles/styles.vtt"),
        None,
    )
    .unwrap();
    assert!(invoke(&p,modern("check(){return 'shift'},fix(c,h){h.update(c.id,{startMs:c.startMs+10,endMs:c.endMs+10})}"),&request(&p,Mode::Fix)).err().unwrap().contains("timed effects"));
    let p = fixture();
    let r = request(&p, Mode::Fix);
    for patch in [
        "{endMs:c.startMs}",
        "{text:'bad\\0text'}",
        "{text:'bare\\rCR'}",
        "{startMs:1.5}",
        "{startMs:-1}",
        "{startMs:c.index===1?2500:c.startMs,endMs:3500}",
    ] {
        assert!(
            invoke(
                &p,
                modern(&format!(
                    "check(){{return 'fix'}},fix(c,h){{h.update(c.id,{patch})}}"
                )),
                &r
            )
            .is_err(),
            "{patch}"
        );
    }
}

#[test]
fn malformed_results_are_validated_independently_of_the_adapter() {
    let p = fixture();
    let r = request(&p, Mode::Fix);
    let pkg = modern("check(){return 'fix'},fix(c,h){h.update(c.id,{text:c.text+'!'})}");
    let good = output(&capture(&p, pkg.clone(), &r).unwrap()).unwrap();
    let mut bad = Vec::new();
    let mut v = good.clone();
    v["key"]["revision"] = json!("1");
    bad.push(v);
    let mut v = good.clone();
    v["fingerprint"] = json!("wrong");
    bad.push(v);
    let mut v = good.clone();
    v["extra"] = json!(true);
    bad.push(v);
    let mut v = good.clone();
    v["edits"][0]["style"] = json!("injected");
    bad.push(v);
    let mut v = good.clone();
    v["edits"][0]["cueId"] = json!("01");
    bad.push(v);
    let mut v = good.clone();
    v["edits"][1]["cueId"] = json!("1");
    bad.push(v);
    let mut v = good.clone();
    v["edits"][0]["startMs"] = json!(1.5);
    bad.push(v);
    let mut v = good.clone();
    v["edits"][0]["startMs"] = json!(-1);
    bad.push(v);
    let mut v = good.clone();
    v["edits"][0]["endMs"] = Value::Null;
    bad.push(v);
    let mut v = good.clone();
    v["findings"][0]["cueId"] = json!("99");
    bad.push(v);
    let mut v = good.clone();
    v["findings"][1]["cueId"] = json!("1");
    bad.push(v);
    for v in bad {
        assert!(
            capture(&p, pkg.clone(), &r)
                .unwrap()
                .validate(v, || Ok(()))
                .is_err()
        );
    }
    let mut check = r.clone();
    check.mode = Mode::Check;
    assert!(
        capture(&p, pkg.clone(), &check)
            .unwrap()
            .validate(good.clone(), || Ok(()))
            .is_err()
    );
    let mut oversized = good;
    oversized["edits"][0]["text"] = json!("x".repeat(16 * 1024 * 1024));
    assert!(
        capture(&p, pkg, &r)
            .unwrap()
            .validate(oversized, || Ok(()))
            .err()
            .unwrap()
            .contains("16 MiB")
    );
}

#[test]
fn every_captured_context_change_and_undo_revision_invalidates_review() {
    let p = fixture();
    let r = request(&p, Mode::Fix);
    let pkg = modern("check(){return 'fix'},fix(c,h){h.update(c.id,{text:c.text+'!'})}");
    let proposal = invoke(&p, pkg, &r).unwrap();
    let mut variants = Vec::new();
    let mut v = r.clone();
    v.key.generation -= 1;
    variants.push(v);
    let mut v = r.clone();
    v.key.revision += 1;
    variants.push(v);
    let mut v = r.clone();
    v.key.context += 1;
    variants.push(v);
    let mut v = r.clone();
    v.scope.pop();
    variants.push(v);
    let mut v = r.clone();
    v.context.selection.clear();
    variants.push(v);
    let mut v = r.clone();
    v.context.cursor = None;
    variants.push(v);
    let mut v = r.clone();
    v.context.preferences.minimum_blank_ms += 1;
    variants.push(v);
    let mut v = r.clone();
    v.context.scenes.as_mut().unwrap().cuts_ms.push(4000);
    variants.push(v);
    let mut v = r.clone();
    v.settings.insert("changed".into(), json!(true));
    variants.push(v);
    for v in variants {
        assert!(proposal.edit(&p.document, &p.source, &v).is_err());
    }
    let different = parse("1\n00:00:01,000 --> 00:00:02,000\nchanged\n", None).unwrap();
    assert!(proposal.edit(&different.document, &p.source, &r).is_err());
    let different_source = parse(
        &p.source
            .serialize(&p.document)
            .unwrap()
            .replace('\n', "\r\n"),
        None,
    )
    .unwrap()
    .source;
    assert!(proposal.edit(&p.document, &different_source, &r).is_err());
    let mut e = Editor::with_properties(p.document.clone(), p.source.clone());
    apply(&mut e, &proposal, &r);
    e.undo(1).unwrap();
    assert_eq!(e.document(), &p.document);
    let mut current = r.clone();
    current.key.revision = e.revision();
    assert!(
        proposal
            .edit(e.document(), e.properties(), &current)
            .is_err()
    );
    // Even a caller that mistakenly reused the old request cannot bypass Editor.
    let stale = proposal.edit(e.document(), e.properties(), &r).unwrap();
    assert!(
        e.apply_edit_with_properties(0, stale, subtitle_file::validate_document)
            .is_err()
    );
    assert!(e.can_redo());
    assert!(!e.dirty());
}

#[test]
fn capture_and_proposal_checkpoints_cancel_without_edits() {
    let p = fixture();
    let r = request(&p, Mode::Fix);
    let pkg = modern("check(){return 'fix'},fix(c,h){h.update(c.id,{text:'changed'})}");
    assert!(
        Capture::new(
            p.document.clone(),
            p.source.clone(),
            pkg.clone(),
            r.clone(),
            || Err("cancelled".into())
        )
        .is_err()
    );
    let c = capture(&p, pkg.clone(), &r).unwrap();
    let mut calls = 0;
    assert!(
        c.execution(|| {
            calls += 1;
            if calls == 2 {
                Err("cancelled".into())
            } else {
                Ok(())
            }
        })
        .is_err()
    );
    let value = output(&c).unwrap();
    let mut calls = 0;
    assert!(
        c.validate(value, || {
            calls += 1;
            if calls == 4 {
                Err("cancelled".into())
            } else {
                Ok(())
            }
        })
        .is_err()
    );
    assert_eq!(invoke(&p, pkg, &r).unwrap().rows().len(), 3);
    let empty = parse("", Some(Format::Srt)).unwrap();
    let mut empty_request = r;
    empty_request.scope.clear();
    empty_request.context.selection.clear();
    let c = capture(&empty, modern("check(){return ''}"), &empty_request).unwrap();
    assert!(
        c.execution(|| Err("cancelled before an empty scope".into()))
            .is_err()
    );
}

#[test]
fn modern_snapshots_settings_and_diagnostic_limits() {
    let p = fixture();
    let mut r = request(&p, Mode::Check);
    let mut pkg = modern(
        "check(c,h){if(!Object.isFrozen(c)||!Object.isFrozen(h.getAllCues())||!Object.isFrozen(h.settings)||h.getAllCues().length!==3||h.getSelection().map(c=>c.id).join(',')!=='1,3'||h.getPrevious('1')!==null||h.getNext('3')!==null||h.cursorMs!==1500||h.getSceneCuts().join(',')!=='0,1500,3000'||h.sourceFormat!=='srt'||h.settings.flag!==false||h.settings.label.length!==4096)throw Error('snapshot/settings');return ''}",
    );
    pkg.manifest.settings = BTreeMap::from([
        ("flag".into(), json!(true)),
        ("label".into(), json!("x".repeat(4096))),
    ]);
    r.settings.insert("flag".into(), json!(false));
    let report = invoke(&p, pkg, &r).unwrap();
    assert_eq!(report.settings()["flag"], false);
    let r = request(&p, Mode::Check);
    for script in [
        "check(){return 'x'.repeat(1025)}",
        "check(c,h){h.log('x'.repeat(1025));return ''}",
        "check(c,h){for(let i=0;i<101;i++)h.log('');return ''}",
        "check(c,h){for(let i=0;i<5;i++)h.log('x'.repeat(1024));return ''}",
    ] {
        assert!(invoke(&p, modern(script), &r).is_err(), "{script}");
    }
    let report = invoke(&p, modern("check(c,h){h.log('cue '+c.id);return ''}"), &r).unwrap();
    assert_eq!(report.logs(), ["cue 1", "cue 2", "cue 3"]);
}

#[test]
fn bounded_fix_loop_returns_no_proposal_and_next_invocation_still_works() {
    let p = fixture();
    let r = request(&p, Mode::Fix);
    for pkg in [
        modern("check(){return 'fix'},fix(c,h){h.update(c.id,{text:'partial'});while(true){}}"),
        vss("HasError(){return 'fix'},FixError(c){c.Text='partial';while(true){c.Stop+=1}}"),
    ] {
        let capture = capture(&p, pkg, &r).unwrap();
        let execution = capture.execution(|| Ok(())).unwrap();
        let mut invocation = vsf_plugin_host::Request::new(execution.script, execution.input);
        invocation.limits.timeout_ms = 150;
        assert_eq!(
            vsf_plugin_host::run(
                Path::new(env!("CARGO_BIN_EXE_vsf-plugin-worker")),
                &[],
                &invocation,
                &AtomicBool::new(false)
            ),
            Err(vsf_plugin_host::Failure::Timeout)
        );
    }
    assert_eq!(
        invoke(
            &p,
            modern("check(){return 'fix'},fix(c,h){h.update(c.id,{text:'completed'})}"),
            &r
        )
        .unwrap()
        .rows()
        .len(),
        3
    );
}

#[test]
fn package_identity_paths_capabilities_and_settings_are_bounded() {
    let base = modern("check(){return ''}");
    let fingerprint = base.fingerprint().unwrap();
    let mut changed = base.clone();
    changed.files.get_mut("main.js").unwrap().push(' ');
    assert_ne!(changed.fingerprint().unwrap(), fingerprint);
    for path in [
        "../outside.js",
        "/root.js",
        "x/./y.js",
        "x//y.js",
        "a\\b.js",
        "c:file.js",
        "*.js",
        "main.JS",
    ] {
        let mut p = base.clone();
        p.files.insert(path.into(), String::new());
        assert!(p.validate().is_err(), "{path}");
    }
    let mut p = base.clone();
    p.files.insert("MAIN.js".into(), String::new());
    assert!(p.validate().is_err());
    let mut p = base.clone();
    p.files.insert("huge.js".into(), "x".repeat(1024 * 1024));
    assert!(p.validate().is_err());
    for value in [
        json!(null),
        json!([]),
        json!({}),
        json!(9_007_199_254_740_992u64),
    ] {
        let mut p = base.clone();
        p.manifest.settings.insert("setting".into(), value);
        assert!(p.validate().is_err());
    }
    for name in ["__proto__", "constructor", "prototype", "bad-name"] {
        let mut p = base.clone();
        p.manifest.settings.insert(name.into(), json!(true));
        assert!(p.validate().is_err());
    }
    let mut p = base.clone();
    p.manifest.requires = vec!["rules".into(), "rules".into()];
    assert!(p.validate().is_err());
    let mut p = base;
    p.manifest.version = 2;
    assert!(p.validate().is_err());
}

#[test]
#[ignore = "requires the separately acquired, hash-verified legacy file; no source redistribution"]
fn unchanged_archived_overlap_rule_and_fix_run_locally() {
    use sha2::{Digest, Sha256};
    let path = std::env::var_os("VSF_LEGACY_OVERLAP_SCRIPT")
        .expect("Set VSF_LEGACY_OVERLAP_SCRIPT to the verified archive's jsplugin/overlapping.js");
    let bytes = std::fs::read(path).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        "56a698e5569fdce0d9103d25708a992aa299a68f5879fc8c5186be655b85b278"
    );
    let script = String::from_utf8(bytes).unwrap();
    let pkg = package(Api::Vss1, &script);
    let p=parse("1\n00:00:01,000 --> 00:00:01,900\n{\\i1}first\n\n2\n00:00:01,850 --> 00:00:02,900\nsecond\n\n3\n00:00:03,000 --> 00:00:03,900\nthird\n",None).unwrap();
    let report = invoke(&p, pkg.clone(), &request(&p, Mode::Check)).unwrap();
    assert_eq!(report.findings().len(), 1);
    assert_eq!(report.findings()[0].message, "50ms overlap");
    for (mode, stop, start) in [(1, 1835, 1915), (2, 1770, 1850), (3, 1900, 1980)] {
        let mut r = request(&p, Mode::Fix);
        r.scope.truncate(1);
        r.settings.insert("ParamMode".into(), json!(mode));
        let proposal = invoke(&p, pkg.clone(), &r).unwrap();
        let mut e = Editor::with_properties(p.document.clone(), p.source.clone());
        apply(&mut e, &proposal, &r);
        assert_eq!(e.document().cues()[0].end().get(), stop);
        assert_eq!(e.document().cues()[1].start().get(), start);
        assert_eq!(e.document().cues()[0].text(), p.document.cues()[0].text());
        if mode != 2 {
            assert!(proposal.rows().iter().any(|row| row.outside_scope));
        }
        e.undo(1).unwrap();
        assert_eq!(e.document(), &p.document);
        assert!(!e.can_undo());
    }
    // Deliberate subset boundary: no guessed Delphi fractional-number coercion.
    let odd = parse(
        "1\n00:00:01,000 --> 00:00:01,901\nfirst\n\n2\n00:00:01,850 --> 00:00:02,900\nsecond\n",
        None,
    )
    .unwrap();
    let mut r = request(&odd, Mode::Fix);
    r.settings.insert("ParamMode".into(), json!(1));
    assert!(
        invoke(&odd, pkg, &r)
            .err()
            .unwrap()
            .contains("exact supported integer")
    );
}
