use super::*;
use crate::document::{Action, Documents};
use serde::Deserialize;
use serde_json::json;

const SRT: &str = "1\n00:00:01,000 --> 00:00:03,000\n{\\an8}<i>Été 世界</i>\n\n";

fn fixture(root: &Path, extension: &str, text: &str) -> (Project, PathBuf, PathBuf) {
    let target = root.join(format!("target.{extension}"));
    let reference = root.join(format!("reference.{extension}"));
    fs::write(&target, text).unwrap();
    fs::write(&reference, text).unwrap();
    let parsed = vsf_application::translation::Reference::parse(
        text.as_bytes().to_vec(),
        Some(extension),
        || Ok(()),
    )
    .unwrap();
    let mut project = tests::fixture(root);
    project.version = 2;
    project.media_path = None;
    project.media_identity = None;
    project.position_seconds = 0.;
    project.subtitle_path = Some(format!("target.{extension}"));
    project.subtitle_format = Some(extension.into());
    project.translation = Some(
        serde_json::from_value(json!({
            "reference": {"path": format!("reference.{extension}"), "format": extension,
                "sha256": parsed.fingerprint(), "offsetMs": -1500},
            "copiedFrom": {"path": format!("reference.{extension}"),
                "selected": format!("reference.{extension}")}
        }))
        .unwrap(),
    );
    (project, target, reference)
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Snapshot {
    generation: u64,
    revision: u64,
    dirty: bool,
    can_undo: bool,
    can_redo: bool,
    translation: Option<Summary>,
}
#[derive(Deserialize)]
struct Summary {
    epoch: u64,
    dirty: bool,
    reference: Option<Reference>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Reference {
    path: String,
    format: String,
    offset_ms: i64,
}
fn snapshot(docs: &Documents) -> Snapshot {
    serde_json::from_value(serde_json::to_value(docs.apply(Action::Status, 0, 0).unwrap()).unwrap())
        .unwrap()
}
fn open_target(docs: &Documents, target: &Path) -> Snapshot {
    let before = snapshot(docs);
    docs.apply(
        Action::Open {
            path: target.to_string_lossy().into(),
            discard: true,
        },
        before.generation,
        before.revision,
    )
    .unwrap();
    snapshot(docs)
}
fn attach(projects: &Projects, docs: &Documents, token: u64) -> Result<Attachment> {
    let before = snapshot(docs);
    docs.attach_quality_project(
        projects,
        token,
        before.generation,
        before.revision,
        None,
        before.translation.map_or(0, |context| context.epoch),
    )
}

#[test]
fn subtitle_only_translation_projects_roundtrip_all_formats_and_keep_target_history() {
    for (extension, text) in [
        ("srt", SRT),
        (
            "ass",
            include_str!("../../../tests/fixtures/subtitles/styles.ass"),
        ),
        (
            "ssa",
            include_str!("../../../tests/fixtures/subtitles/styles.ssa"),
        ),
        (
            "vtt",
            include_str!("../../../tests/fixtures/subtitles/styles.vtt"),
        ),
    ] {
        let root = tempfile::tempdir().unwrap();
        let root = root.path().canonicalize().unwrap();
        let (project, target, reference) = fixture(&root, extension, text);
        let path = root.join("translation.vsfproject");
        write(&path, &project).unwrap();
        let projects = Projects::default();
        let opened = projects.open(&path).unwrap();
        assert!(opened.token > 0);
        let docs = Documents::default();
        let before = open_target(&docs, &target);
        attach(&projects, &docs, opened.token).unwrap();
        let after = snapshot(&docs);
        assert_eq!(
            (after.revision, after.can_undo, after.can_redo, after.dirty),
            (
                before.revision,
                before.can_undo,
                before.can_redo,
                before.dirty
            )
        );
        let summary = after.translation.unwrap();
        assert!(!summary.dirty);
        let info = summary.reference.unwrap();
        assert_eq!(info.format, extension);
        assert_eq!(info.offset_ms, -1500);
        docs.with_project_subtitle(Some(&target), |_, format, context, _| {
            assert_eq!(format.unwrap().extension(), extension);
            let description = context.project_description(&root)?.unwrap();
            assert_eq!(
                serde_json::to_value(description).unwrap(),
                serde_json::to_value(project.translation.as_ref().unwrap()).unwrap()
            );
            assert!(context.protect(&reference).is_err());
            Ok(())
        })
        .unwrap();
        assert_eq!(fs::read(&reference).unwrap(), text.as_bytes());
        docs.shutdown_quality().unwrap();
    }
}

#[test]
fn missing_changed_and_relocated_references_require_explicit_choices_before_attachment() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    let (project, target, reference) = fixture(&root, "srt", SRT);
    let path = root.join("translation.vsfproject");
    write(&path, &project).unwrap();
    let projects = Projects::default();
    let original = projects.open(&path).unwrap();
    projects.attach(original.token).unwrap();
    fs::write(&reference, SRT.replace("Été", "Hiver")).unwrap();
    let changed = projects.open(&path).unwrap();
    assert_eq!(changed.token, 0);
    assert!(serde_json::to_value(changed).unwrap()["referenceIssue"]["canReload"] == true);
    assert!(projects.attach(0).is_err());
    let replacement = root.join("relocated.srt");
    fs::write(&replacement, SRT).unwrap();
    fs::remove_file(&reference).unwrap();
    let missing = projects.open(&path).unwrap();
    assert_eq!(missing.token, 0);
    assert!(serde_json::to_value(missing).unwrap()["referenceIssue"]["canReload"] == false);
    // A failed preflight does not invalidate the active save identity.
    projects
        .save(&path, Some(original.token), &project)
        .unwrap();
    let relocated: ReferenceOverride =
        serde_json::from_value(json!({"path": replacement})).unwrap();
    let opened = projects.open_with(&path, &relocated).unwrap();
    let docs = Documents::default();
    open_target(&docs, &target);
    attach(&projects, &docs, opened.token).unwrap();
    let summary = snapshot(&docs).translation.unwrap();
    assert!(summary.dirty);
    assert_eq!(
        summary.reference.unwrap().path,
        replacement.to_string_lossy()
    );
    // Original copy provenance remains protected even after deletion/relocation.
    docs.with_project_subtitle(Some(&target), |_, _, context, _| {
        assert!(context.protect(&reference).is_err());
        Ok(())
    })
    .unwrap();
    fs::write(&replacement, SRT.replace("Été", "Hiver")).unwrap();
    assert!(!serde_json::to_value(projects.open_with(&path, &relocated).unwrap()).unwrap()["referenceIssue"].is_null());
    let reload: ReferenceOverride = serde_json::from_value(json!({
        "path": replacement, "reload": true
    }))
    .unwrap();
    let opened = projects.open_with(&path, &reload).unwrap();
    attach(&projects, &docs, opened.token).unwrap();
    assert!(snapshot(&docs).translation.unwrap().dirty);
    docs.shutdown_quality().unwrap();
}

#[test]
fn staged_reference_is_captured_once_and_attach_checks_media_target_revision_and_epoch() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    let (project, target, reference) = fixture(&root, "srt", SRT);
    let path = root.join("translation.vsfproject");
    write(&path, &project).unwrap();
    let projects = Projects::default();
    let opened = projects.open(&path).unwrap();
    let docs = Documents::default();
    let other = root.join("other.srt");
    fs::write(&other, SRT).unwrap();
    open_target(&docs, &other);
    assert!(attach(&projects, &docs, opened.token).is_err());
    let before = open_target(&docs, &target);
    let epoch = before.translation.map_or(0, |context| context.epoch);
    assert!(
        docs.attach_quality_project(
            &projects,
            opened.token,
            before.generation,
            before.revision,
            Some(&root.join("clip.mkv")),
            epoch
        )
        .is_err()
    );
    assert!(
        docs.attach_quality_project(
            &projects,
            opened.token,
            before.generation,
            before.revision + 1,
            None,
            epoch
        )
        .is_err()
    );
    assert!(
        docs.attach_quality_project(
            &projects,
            opened.token,
            before.generation,
            before.revision,
            None,
            epoch + 1
        )
        .is_err()
    );
    fs::write(&reference, SRT.replace("Été", "Changed after preflight")).unwrap();
    attach(&projects, &docs, opened.token).unwrap();
    docs.with_project_subtitle(Some(&target), |_, _, context, _| {
        assert_eq!(
            context
                .reference
                .as_ref()
                .unwrap()
                .reference
                .original_bytes(),
            SRT.as_bytes()
        );
        assert!(context.verify_project_reference().is_err());
        Ok(())
    })
    .unwrap();
    docs.shutdown_quality().unwrap();
}

#[test]
fn failed_project_publication_keeps_context_dirty_and_success_cleans_only_context() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    let (project, target, reference) = fixture(&root, "srt", SRT);
    let path = root.join("translation.vsfproject");
    write(&path, &project).unwrap();
    fs::write(&reference, SRT.replace("Été", "Hiver")).unwrap();
    let projects = Projects::default();
    let reload: ReferenceOverride = serde_json::from_value(json!({"reload": true})).unwrap();
    let opened = projects.open_with(&path, &reload).unwrap();
    let docs = Documents::default();
    let before = open_target(&docs, &target);
    attach(&projects, &docs, opened.token).unwrap();
    let failed: Result<()> =
        docs.with_project_subtitle(Some(&target), |_, _, _, _| Err("write failed".into()));
    assert!(failed.is_err());
    assert!(snapshot(&docs).translation.unwrap().dirty);
    let output = root.join("saved.vsfproject");
    docs.with_project_subtitle(Some(&target), |_, _, context, _| {
        context.verify_project_reference()?;
        let mut saved = project.clone();
        saved.translation = context.project_description(&root)?;
        projects.save(&output, Some(opened.token), &saved)
    })
    .unwrap();
    let after = snapshot(&docs);
    assert!(!after.translation.unwrap().dirty);
    assert_eq!(
        (after.revision, after.can_undo, after.can_redo),
        (before.revision, before.can_undo, before.can_redo)
    );
    assert!(read(&output).is_ok());
    docs.shutdown_quality().unwrap();
}

#[test]
fn inconsistent_project_versions_and_reference_metadata_are_rejected() {
    let root = tempfile::tempdir().unwrap();
    let (project, _, reference) = fixture(root.path(), "srt", SRT);
    for (key, value) in [
        ("version", json!(1)),
        ("positionSeconds", json!(2)),
        ("subtitlePath", json!(null)),
        (
            "mediaIdentity",
            json!({"size": 0, "modified": "0", "sampleHash": "0"}),
        ),
    ] {
        let mut invalid = serde_json::to_value(&project).unwrap();
        invalid[key] = value;
        assert!(
            serde_json::from_value::<Project>(invalid)
                .unwrap()
                .validate()
                .is_err()
        );
    }
    for (key, value) in [
        ("sha256", json!("invalid")),
        ("format", json!("unknown")),
        ("path", json!("")),
    ] {
        let mut invalid = serde_json::to_value(&project).unwrap();
        invalid["translation"]["reference"][key] = value;
        assert!(
            serde_json::from_value::<Project>(invalid)
                .unwrap()
                .validate()
                .is_err()
        );
    }
    assert!(serialize(&reference, &project).is_err());
    fs::remove_file(&reference).unwrap();
    assert!(serialize(&reference, &project).is_err());
    assert!(!reference.exists());
}

#[test]
fn translation_layout_add_remove_saves_v3_only_while_needed() {
    for has_media in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let (mut project, _, _) = fixture(dir.path(), "srt", SRT);
        if has_media {
            let media = tests::fixture(dir.path());
            project.media_path = media.media_path;
            project.media_identity = media.media_identity;
        }
        let path = dir.path().join("translation.vsfproject");
        for (layout, expected) in [(None, 2), (Some(Default::default()), 3), (None, 2)] {
            project.options.layout = layout;
            project.version = project.version_for_save();
            write(&path, &project).unwrap();
            project = read(&path).unwrap();
            assert_eq!(project.version, expected);
            assert!(project.translation.is_some());
            assert_eq!(project.options.layout.is_some(), expected == 3);
        }
    }
}

#[test]
fn review_project_roundtrip_options_independent_translation_and_missing_original() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let (mut project, target, original) = fixture(&root, "srt", SRT);
    project.version = 4;
    project.review = Some(
        serde_json::from_value(json!({
            "reference": project.translation,
            "options": {"tolerance_ms": 42, "ignore_tags": true,
                "ignore_line_breaks": true, "ignore_whitespace": false}
        }))
        .unwrap(),
    );
    let path = root.join("review.vsfproject");
    write(&path, &project).unwrap();
    let projects = Projects::default();
    let opened = projects.open(&path).unwrap();
    let docs = Documents::default();
    open_target(&docs, &target);
    attach(&projects, &docs, opened.token).unwrap();
    docs.with_project_subtitle(Some(&target), |_, _, translation, review| {
        assert!(translation.project_description(&root)?.is_some());
        assert_eq!(
            serde_json::to_value(review.project_description(&root)?).unwrap(),
            serde_json::to_value(&project.review).unwrap()
        );
        Ok(())
    })
    .unwrap();
    // Resolve translation independently, so the review issue is independently visible.
    project.translation = None;
    write(&path, &project).unwrap();
    fs::write(&original, SRT.replace("Été", "Changed")).unwrap();
    let changed = projects.open(&path).unwrap();
    assert!(!serde_json::to_value(changed).unwrap()["reviewIssue"].is_null());
    fs::remove_file(&original).unwrap();
    let missing = projects.open(&path).unwrap();
    assert!(!serde_json::to_value(missing).unwrap()["reviewIssue"].is_null());
}

#[test]
fn v4_review_saves_as_v4_until_discussion_and_missing_package_preserves_v5() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let (mut project, target, _) = fixture(&root, "srt", SRT);
    let path = root.join("review.vsfproject");
    let reference = project.translation.take().unwrap();
    let options = vsf_application::comparison::Options::default();
    project.review =
        Some(serde_json::from_value(json!({"reference":reference,"options":options})).unwrap());
    project.version = 4;
    write(&path, &project).unwrap();
    let mut loaded = read(&path).unwrap();
    assert_eq!(loaded.version_for_save(), 4);
    let pack = vsf_application::review_package::read(include_bytes!(
        "../../../crates/vsf-application/tests/fixtures/review-v1.vsfreview"
    ))
    .unwrap();
    let mut review = serde_json::to_value(&loaded.review).unwrap();
    review["discussion"] = json!({"threads":pack.threads,"round":0,"exported":[]});
    loaded.review = Some(serde_json::from_value(review.clone()).unwrap());
    assert_eq!(loaded.version_for_save(), 5);
    loaded.version = loaded.version_for_save();
    write(&path, &loaded).unwrap();
    assert_eq!(read(&path).unwrap().version, 5);

    let source = root.join("round.vsfreview");
    fs::write(&source, pack.write().unwrap()).unwrap();
    review["reference"] = json!({"reference":{"path":"round.vsfreview","sha256":pack.original.sha256,"offsetMs":0,"format":"srt"}});
    review["package"] = json!({"path":"round.vsfreview","sha256":pack.original.sha256,"original_name":pack.original.name,"revised_name":pack.revised.name,"utf16_le":null});
    loaded.review = Some(serde_json::from_value(review).unwrap());
    write(&path, &loaded).unwrap();
    let projects = Projects::default();
    let opened = projects.open(&path).unwrap();
    assert!(serde_json::to_value(&opened).unwrap()["reviewIssue"].is_null());
    let docs = Documents::default();
    open_target(&docs, &target);
    let token = attach(&projects, &docs, opened.token).unwrap().token;
    let previous = fs::read(&path).unwrap();
    fs::rename(&source, root.join("moved.vsfreview")).unwrap();
    loaded.options.follow_playback = !loaded.options.follow_playback;
    assert!(
        projects
            .save_document(
                &docs,
                Some(&target),
                &path,
                Some(token),
                loaded.clone(),
                false
            )
            .is_err()
    );
    assert_eq!(fs::read(&path).unwrap(), previous);
    fs::rename(root.join("moved.vsfreview"), &source).unwrap();
    projects
        .save_document(&docs, Some(&target), &path, Some(token), loaded, false)
        .unwrap();
    assert_ne!(fs::read(&path).unwrap(), previous);
}

#[test]
fn changed_subtitle_save_requires_consent_and_preserves_project_bytes() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    let (mut project, target, _) = fixture(&root, "srt", SRT);
    let path = root.join("episode.vsfproject");
    write(&path, &project).unwrap();
    let previous = fs::read(&path).unwrap();
    let projects = Projects::default();
    let token = projects.open(&path).unwrap().token;
    projects.attach(token).unwrap();
    let captions = root.join("subs");
    fs::create_dir(&captions).unwrap();
    let replacement = captions.join("other.srt");
    fs::copy(&target, &replacement).unwrap();
    let docs = Documents::default();
    open_target(&docs, &replacement);
    let relative = portable(&root, &replacement);
    assert_eq!(relative, "subs/other.srt");
    project.subtitle_path = Some(relative);
    let outcome = projects
        .save_document(
            &docs,
            Some(&replacement),
            &path,
            Some(token),
            project.clone(),
            false,
        )
        .unwrap();
    assert!(
        matches!(outcome, SaveOutcome::SubtitleReferenceChanged {from, to} if from.as_deref()==target.to_str() && to.as_deref()==replacement.to_str())
    );
    assert_eq!(fs::read(&path).unwrap(), previous);
    let copy = root.join("copy.vsfproject");
    assert!(matches!(
        projects
            .save_document(
                &docs,
                Some(&replacement),
                &copy,
                Some(token),
                project.clone(),
                false
            )
            .unwrap(),
        SaveOutcome::SubtitleReferenceChanged { .. }
    ));
    assert!(!copy.exists());
    assert!(matches!(
        projects
            .save_document(&docs, Some(&replacement), &path, Some(token), project, true)
            .unwrap(),
        SaveOutcome::Saved { .. }
    ));
    assert_eq!(
        read(&path).unwrap().subtitle_path.as_deref(),
        replacement.to_str()
    );
}
