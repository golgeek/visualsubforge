//! Blocking package/file operations. Desktop callers must use their owned worker.
//! The executor is injected; no JavaScript engine or Tauri dependency lives here.
mod input;
mod package;
mod report;
use crate::{
    atomic_file, new_output,
    plugins::{Capture, Context, Execution, Key, Mode, Preferences, Request, Scenes},
    quality::Profile,
    save_lease, srt_limits, subtitle_file,
};
pub use input::FileIdentity;
use input::{CapturedFile, digest};
pub use package::LoadedPackage;
pub use report::{MAX_REPORT_BYTES, Report};
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashSet},
    fmt,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
use subtitle_core::{CueId, Document, Editor, Milliseconds};

pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug)]
pub enum Error {
    Invalid(String),
    Io(String),
    Changed(String),
    Execution(String),
    Cancelled,
    Timeout,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(m) | Self::Io(m) | Self::Execution(m) => f.write_str(m),
            Self::Changed(m) => write!(f, "Plugin input or review changed; preview again: {m}"),
            Self::Cancelled => f.write_str("Plugin operation cancelled"),
            Self::Timeout => f.write_str("Plugin operation exceeded its time limit"),
        }
    }
}
impl std::error::Error for Error {}
pub struct Budget<'a> {
    cancel: &'a AtomicBool,
    deadline: Instant,
}
impl<'a> Budget<'a> {
    pub fn new(cancel: &'a AtomicBool, duration: Duration) -> Self {
        Self {
            cancel,
            deadline: Instant::now() + duration,
        }
    }
    pub fn remaining(&self) -> Result<Duration> {
        if self.cancel.load(Ordering::Acquire) {
            return Err(Error::Cancelled);
        }
        self.deadline
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
            .ok_or(Error::Timeout)
    }
    pub fn check(&self) -> Result<()> {
        self.remaining().map(|_| ())
    }
    fn checkpoint(&self) -> std::result::Result<(), String> {
        self.check().map_err(|e| e.to_string())
    }
    fn pure<T>(&self, value: std::result::Result<T, String>) -> Result<T> {
        self.check()?;
        value.map_err(Error::Invalid)
    }
}
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
                .map_err(Error::Invalid)?,
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
        if path
            .extension()
            .and_then(|e| e.to_str())
            .is_none_or(|e| !e.eq_ignore_ascii_case(&self.report.source_format))
        {
            return Err(Error::Invalid(format!(
                "Output must keep the .{} source format",
                self.report.source_format
            )));
        }
        let output = new_output::resolve(path).map_err(output_error)?;
        self.protect(&output)?;
        let _lease = save_lease::acquire(&output).map_err(Error::Io)?;
        new_output::require_absent(&output).map_err(output_error)?;
        let pending = atomic_file::prepare_with_checkpoint(&output, bytes, || budget.checkpoint())
            .map_err(|e| budget.check().err().unwrap_or(Error::Io(e)))?;
        self.verify(budget)?;
        if new_output::resolve(path).map_err(output_error)? != output {
            return Err(Error::Changed("output directory changed".into()));
        }
        self.protect(&output)?;
        budget.check()?;
        pending.commit_new().map_err(Error::Io)?;
        // A cancellation arriving after commit cannot retract successful output.
        Ok(output)
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
fn protect(output: &Path, input: &FileIdentity) -> Result<()> {
    for path in [&input.requested_path, &input.canonical_path] {
        new_output::protect_source(output, path).map_err(output_error)?;
    }
    Ok(())
}
fn output_error(error: new_output::Error) -> Error {
    match error {
        new_output::Error::InvalidInput(e) => Error::Invalid(e),
        new_output::Error::Io(e) => Error::Io(e),
    }
}
