//! Blocking adapter for headless timing review. The pure engine owns all rows.
pub use crate::review_files::{Error, Result};
use crate::{
    quality::{
        self, Profile,
        scene::{SceneCoverage, Scenes},
        timing_fix::{
            self, Families, Options,
            review::{Counts, ManualRow, Row},
        },
    },
    review_files::{self, Budget, CapturedFile, FileIdentity, digest},
    srt_limits, subtitle_file,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};
use subtitle_core::{Editor, Milliseconds};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SceneFile {
    version: u32,
    cuts_ms: Vec<u64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub schema_version: u32,
    pub review_id: String,
    pub input_sha256: String,
    pub source_format: String,
    pub profile_sha256: String,
    pub profile_file_sha256: Option<String>,
    pub scene_input_sha256: Option<String>,
    pub families: Families,
    pub options: Options,
    pub cues: Vec<u32>,
    pub coverage: SceneCoverage,
    pub before: Counts,
    pub after: Counts,
    pub changes: Vec<Row>,
    pub manual: Vec<ManualRow>,
    pub result_sha256: String,
}

pub struct Prepared {
    report: Report,
    inputs: Vec<FileIdentity>,
    output: Vec<u8>,
}
pub struct Request<'a> {
    pub subtitles: &'a Path,
    pub profile: &'a str,
    pub scenes: Option<&'a Path>,
    pub without_scenes: bool,
    pub families: Families,
    pub options: Options,
    pub cues: Vec<u32>,
}
fn invalid(e: impl ToString) -> Error {
    Error::Invalid(e.to_string())
}
fn evaluation(error: quality::Error) -> Error {
    match error {
        quality::Error::Cancelled => Error::Cancelled,
        other => invalid(other),
    }
}
fn read_scenes(bytes: &[u8]) -> Result<Scenes> {
    let mut file: SceneFile = serde_json::from_slice(bytes).map_err(invalid)?;
    if file.version != 1 || file.cuts_ms.len() > 100_000 {
        return Err(invalid(
            "Scene input requires version 1 and at most 100,000 cut timestamps",
        ));
    }
    file.cuts_ms.sort_unstable();
    file.cuts_ms.dedup();
    Ok(Scenes::Complete(
        file.cuts_ms
            .into_iter()
            .map(|ms| Milliseconds::new(ms).map_err(invalid))
            .collect::<Result<_>>()?,
    ))
}
pub fn preview(mut request: Request<'_>, cancel: &AtomicBool) -> Result<Prepared> {
    let budget = Budget::cancellable(cancel);
    let mut inputs = Vec::new();
    let source = CapturedFile::read(request.subtitles, srt_limits::MAX_BYTES, &budget)?;
    let input_sha256 = source.identity.sha256.clone();
    let parsed = subtitle_file::parse_document(
        &source.bytes,
        request.subtitles.extension().and_then(|s| s.to_str()),
    )
    .map_err(invalid)?;
    inputs.push(source.identity);
    let (profile, profile_file_sha256) = if let Some(p) = Profile::builtin(request.profile) {
        (p, None)
    } else {
        let file = CapturedFile::read(
            Path::new(request.profile),
            quality::MAX_PROFILE_BYTES,
            &budget,
        )?;
        let p = quality::parse_profile(&file.bytes).map_err(evaluation)?;
        let hash = file.identity.sha256.clone();
        inputs.push(file.identity);
        (p, Some(hash))
    };
    let (scenes, scene_input_sha256) = match request.scenes {
        Some(_) if request.without_scenes => {
            return Err(invalid("Choose either a scene input or --without-scenes"));
        }
        Some(path) => {
            let file = CapturedFile::read(path, 4 * 1024 * 1024, &budget)?;
            let scenes = read_scenes(&file.bytes)?;
            let hash = file.identity.sha256.clone();
            inputs.push(file.identity);
            (scenes, Some(hash))
        }
        None if !request.without_scenes
            && (profile.timing.scene_before_ms != 0 || profile.timing.scene_after_ms != 0) =>
        {
            return Err(invalid(
                "Supply --scenes FILE or explicitly choose --without-scenes",
            ));
        }
        None => (Scenes::NoMedia, None),
    };
    request.cues.sort_unstable();
    if request.cues.windows(2).any(|p| p[0] == p[1]) {
        return Err(invalid("Repeated cue position"));
    }
    let scope = request
        .cues
        .iter()
        .map(|p| {
            p.checked_sub(1)
                .and_then(|i| parsed.document.cues().get(i as usize))
                .map(|c| c.id())
                .ok_or_else(|| invalid(format!("Cue position {p} is out of range")))
        })
        .collect::<Result<Vec<_>>>()?;
    let format = parsed.source.format();
    let context = quality::Evaluation::from(&profile).with_format(format);
    let engine_request = timing_fix::Request {
        scope: (!scope.is_empty()).then_some(scope),
        families: request.families,
        options: request.options,
    };
    let checkpoint = || budget.check().map_err(|_| quality::Error::Cancelled);
    let proposal = timing_fix::propose(
        parsed.document.clone(),
        context,
        &scenes,
        engine_request,
        checkpoint,
    )
    .map_err(evaluation)?;
    let coverage = proposal.before().scene_coverage;
    let review = proposal
        .review(&[], false, checkpoint)
        .map_err(evaluation)?;
    let output = if review.selected == 0 {
        source.bytes
    } else {
        let mut editor = Editor::with_properties(parsed.document, parsed.source);
        let edit = review.plan.edit(editor.document()).map_err(evaluation)?;
        editor
            .apply_edit_with_properties(0, edit, subtitle_file::validate_document)
            .map_err(invalid)?;
        editor
            .properties()
            .serialize(editor.document())
            .map_err(invalid)?
            .into_bytes()
    };
    let mut report = Report {
        schema_version: 1,
        review_id: String::new(),
        input_sha256,
        source_format: format.extension().to_owned(),
        profile_sha256: profile.fingerprint().map_err(evaluation)?,
        profile_file_sha256,
        scene_input_sha256,
        families: request.families,
        options: request.options,
        cues: request.cues,
        coverage,
        before: review.before,
        after: review.after,
        changes: review.changes,
        manual: review.manual,
        result_sha256: digest(&output),
    };
    let mut hash = Sha256::new();
    serde_json::to_writer(&mut hash, &report).map_err(invalid)?;
    report.review_id = format!("{:x}", hash.finalize());
    budget.check()?;
    Ok(Prepared {
        report,
        inputs,
        output,
    })
}
impl Prepared {
    pub fn report(&self) -> &Report {
        &self.report
    }

    pub fn status(&self) -> u8 {
        if self.report.coverage == SceneCoverage::NoMedia {
            3
        } else if self.report.manual.is_empty() {
            0
        } else {
            1
        }
    }
    pub fn publish(&self, path: &Path, accept: &str, cancel: &AtomicBool) -> Result<PathBuf> {
        let budget = Budget::cancellable(cancel);
        if self.report.review_id != accept {
            return Err(Error::Changed(
                "review ID does not match the recomputed timing proposal".into(),
            ));
        }
        review_files::publish(
            path,
            &self.report.source_format,
            &self.output,
            &budget,
            |out| {
                self.inputs
                    .iter()
                    .try_for_each(|input| review_files::protect(out, input))
            },
            || {
                self.inputs
                    .iter()
                    .try_for_each(|input| input.verify(&budget))
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, sync::atomic::Ordering};

    #[test]
    fn cancellation_and_changes_after_capture_leave_no_output() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.srt");
        fs::write(&source, "1\n00:00:01,000 --> 00:00:02,000\nBefore\n").unwrap();
        let output = dir.path().join("fixed.srt");
        let cancel = AtomicBool::new(false);
        let request = || Request {
            subtitles: &source,
            profile: "vsf-default",
            scenes: None,
            without_scenes: true,
            families: Default::default(),
            options: Default::default(),
            cues: vec![],
        };
        let review = preview(request(), &cancel).unwrap();
        let id = &review.report().review_id;
        cancel.store(true, Ordering::Release);
        assert!(matches!(preview(request(), &cancel), Err(Error::Cancelled)));
        assert!(matches!(
            review.publish(&output, id, &cancel),
            Err(Error::Cancelled)
        ));
        assert!(!output.exists());
        cancel.store(false, Ordering::Release);
        fs::write(&source, "1\n00:00:01,000 --> 00:00:02,000\nEdited\n").unwrap();
        assert!(matches!(
            review.publish(&output, id, &cancel),
            Err(Error::Changed(_))
        ));
        assert!(!output.exists());
    }

    #[test]
    fn scene_inputs_are_sorted_and_deduplicated() {
        let Scenes::Complete(cuts) =
            read_scenes(br#"{"version":1,"cutsMs":[19,2,0,19,2]}"#).unwrap()
        else {
            panic!("expected completed scenes")
        };
        assert_eq!(cuts.iter().map(|m| m.get()).collect::<Vec<_>>(), [0, 2, 19]);
    }
}
