//! Blocking package/file operations. Desktop callers must use their owned worker.
//! The executor is injected; no JavaScript engine or Tauri dependency lives here.
use crate::review_files as input;
use input::protect;
pub use input::{Budget, Error, Result};
mod package;
mod report;
use crate::{
    plugins::{Capture, Context, Execution, Key, Mode, Preferences, Request, Scenes},
    quality::Profile,
    srt_limits, subtitle_file,
};
pub use input::FileIdentity;
use input::{CapturedFile, digest};
pub use package::LoadedPackage;
pub use report::{MAX_REPORT_BYTES, Report};
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashSet},
    path::{Path, PathBuf},
};
use subtitle_core::{CueId, Document, Editor, Milliseconds};

pub struct Options {
    pub mode: Mode,
    /// One-based file positions, not source labels. Empty means all cues.
    pub scope: Vec<u32>,
    pub selection: Vec<u32>,
    pub settings: BTreeMap<String, Value>,
    pub context_file: Option<PathBuf>,
}
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ContextFile {
    #[serde(default)]
    cursor_ms: Option<u64>,
    #[serde(default)]
    preferences: Option<Preferences>,
    #[serde(default)]
    scenes: Option<Scenes>,
}
pub struct FileRun {
    source: FileIdentity,
    package: LoadedPackage,
    context: Option<FileIdentity>,
    report: Report,
    json: Vec<u8>,
    output: Option<Vec<u8>>,
}

fn positions(positions: &[u32], document: &Document) -> Result<Vec<CueId>> {
    let mut seen = HashSet::new();
    positions
        .iter()
        .map(|p| {
            if *p == 0 || !seen.insert(p) {
                return Err(Error::Invalid(
                    "Plugin cue positions must be nonzero and unique".into(),
                ));
            }
            document
                .cues()
                .get((*p - 1) as usize)
                .map(|c| c.id())
                .ok_or_else(|| Error::Invalid(format!("Plugin cue position {p} is out of range")))
        })
        .collect()
}
fn default_preferences() -> Result<Preferences> {
    let p = Profile::default();
    let cps = p.rules.speed.maximum_cps.ok_or_else(|| {
        Error::Invalid("Default profile has no CPS target; provide explicit plugin context".into())
    })?;
    Ok(Preferences {
        minimum_blank_ms: u64::from(p.timing.cue_gap_ms),
        minimum_duration_ms: u64::from(p.rules.duration.minimum_ms),
        maximum_duration_ms: u64::from(p.rules.duration.maximum_ms),
        cps_target: f64::from(cps),
        rs_target_index: None,
    })
}

pub fn prepare(
    subtitles: &Path,
    manifest: &Path,
    options: Options,
    budget: &Budget<'_>,
    execute: impl FnOnce(Execution) -> Result<Value>,
) -> Result<FileRun> {
    budget.check()?;
    let source = CapturedFile::read(subtitles, srt_limits::MAX_BYTES, budget)?;
    let parsed = budget.pure(subtitle_file::parse_document(
        &source.bytes,
        source
            .identity
            .requested_path
            .extension()
            .and_then(|e| e.to_str()),
    ))?;
    let package = LoadedPackage::read(manifest, budget)?;
    let context_input = options
        .context_file
        .as_ref()
        .map(|p| CapturedFile::read(p, 2 * 1024 * 1024, budget))
        .transpose()?;
    let context: ContextFile = context_input
        .as_ref()
        .map(|f| {
            serde_json::from_slice(&f.bytes)
                .map_err(|e| Error::Invalid(format!("Invalid plugin context: {e}")))
        })
        .transpose()?
        .unwrap_or_default();
    let preferences = match context.preferences {
        Some(p) => p,
        None => default_preferences()?,
    };
    let scope = if options.scope.is_empty() {
        (1..=parsed.document.cues().len() as u32).collect()
    } else {
        options.scope.clone()
    };
    let request = Request {
        key: Key {
            generation: 1,
            revision: 0,
            context: 1,
        },
        mode: options.mode,
        scope: positions(&scope, &parsed.document)?,
        settings: options.settings.clone(),
        context: Context {
            selection: positions(&options.selection, &parsed.document)?,
            cursor: context
                .cursor_ms
                .map(Milliseconds::new)
                .transpose()
                .map_err(|error| Error::Invalid(error.into()))?,
            preferences: preferences.clone(),
            scenes: context.scenes.clone(),
        },
    };
    let capture = budget.pure(Capture::new(
        parsed.document.clone(),
        parsed.source.clone(),
        package.package.clone(),
        request.clone(),
        || budget.checkpoint(),
    ))?;
    let execution = budget.pure(capture.execution(|| budget.checkpoint()))?;
    let value = execute(execution)?;
    budget.check()?;
    let proposal = budget.pure(capture.validate(value, || budget.checkpoint()))?;
    let output = if options.mode == Mode::Fix && !proposal.rows().is_empty() {
        let edit = budget.pure(proposal.edit(&parsed.document, &parsed.source, &request))?;
        let mut editor = Editor::with_properties(parsed.document, parsed.source.clone());
        budget.pure(editor.apply_edit_with_properties(
            0,
            edit,
            subtitle_file::validate_document,
        ))?;
        Some(
            budget
                .pure(editor.properties().serialize(editor.document()))?
                .into_bytes(),
        )
    } else {
        None
    };
    let mut report = Report {
        schema_version: 1,
        review_id: String::new(),
        mode: options.mode,
        source: source.identity.clone(),
        source_format: parsed.source.format().extension().into(),
        package: report::PackageInfo {
            id: package.package.manifest.id.clone(),
            api: package.package.manifest.api,
            fingerprint: package.fingerprint.clone(),
            files: package.files.clone(),
        },
        context: report::context(
            &options,
            scope,
            preferences,
            context.scenes,
            context.cursor_ms,
            context_input.as_ref().map(|f| f.identity.clone()),
        ),
        name: proposal.name().into(),
        can_fix: proposal.can_fix(),
        settings: proposal.settings().clone(),
        findings: proposal
            .findings()
            .iter()
            .map(|f| report::Finding {
                cue_id: f.cue_id.clone(),
                message: f.message.clone(),
            })
            .collect(),
        changes: Report::changes(&proposal),
        logs: proposal.logs().to_vec(),
        output_sha256: output.as_ref().map(|b| digest(b)),
    };
    let json = report.seal()?;
    budget.check()?;
    let run = FileRun {
        source: source.identity,
        package,
        context: context_input.map(|f| f.identity),
        report,
        json,
        output,
    };
    run.verify(budget)?;
    Ok(run)
}
impl FileRun {
    pub fn report(&self) -> &Report {
        &self.report
    }
    pub fn json(&self) -> &[u8] {
        &self.json
    }
    fn verify(&self, budget: &Budget<'_>) -> Result<()> {
        self.source.verify(budget)?;
        self.package.verify(budget)?;
        if let Some(c) = &self.context {
            c.verify(budget)?;
        }
        budget.check()
    }
    pub fn publish(&self, path: &Path, accept: &str, budget: &Budget<'_>) -> Result<PathBuf> {
        budget.check()?;
        if accept != self.report.review_id {
            return Err(Error::Changed(
                "review ID does not match this exact proposal".into(),
            ));
        }
        let bytes = self
            .output
            .as_ref()
            .ok_or_else(|| Error::Invalid("Plugin proposed no changes to publish".into()))?;
        crate::review_files::publish(
            path,
            &self.report.source_format,
            bytes,
            budget,
            |output| self.protect(output),
            || self.verify(budget),
        )
    }
    fn protect(&self, output: &Path) -> Result<()> {
        protect(output, &self.source)?;
        self.package.protect(output)?;
        if let Some(c) = &self.context {
            protect(output, c)?;
        }
        Ok(())
    }
}
