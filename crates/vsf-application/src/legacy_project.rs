//! Data-only VisualSubSync project import. Parsing has no IO; inspection reads
//! bounded local files and must run off the UI thread. It never writes originals.
mod parser;
mod paths;
#[cfg(test)]
mod tests;

pub use parser::parse;
pub use paths::{PathKind, classify_path, local_path};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

pub const MAX_BYTES: usize = 256 * 1024;
pub const MAX_SETTINGS: usize = 512;
pub const MAX_VALUE_CHARS: usize = 4096;
type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoding {
    Utf8,
    Utf16Le,
    Utf16Be,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReferenceKind {
    Video,
    Audio,
    Peaks,
    Subtitles,
    ReferenceSubtitles,
    TextPipe,
}
impl ReferenceKind {
    pub fn key(self) -> &'static str {
        match self {
            Self::Video => "VideoSource",
            Self::Audio => "WAVFile",
            Self::Peaks => "PeakFile",
            Self::Subtitles => "SubtitlesFile",
            Self::ReferenceSubtitles => "SubtitlesVO",
            Self::TextPipe => "TextPipeSource",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reference {
    pub kind: ReferenceKind,
    pub value: String,
    pub line: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Warning {
    pub field: String,
    pub line: Option<usize>,
    pub value: Option<String>,
    pub message: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct View {
    pub start_ms: u32,
    pub span_ms: u32,
    pub focused_ms: Option<u32>,
}
impl Default for View {
    fn default() -> Self {
        Self {
            start_ms: 0,
            span_ms: 15_000,
            focused_ms: None,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyProject {
    pub encoding: Encoding,
    pub references: Vec<Reference>,
    pub view: View,
    pub warnings: Vec<Warning>,
}
impl LegacyProject {
    pub fn reference(&self, kind: ReferenceKind) -> Option<&Reference> {
        self.references.iter().find(|r| r.kind == kind)
    }
}

#[derive(Default)]
pub struct Overrides {
    pub media: Option<PathBuf>,
    pub subtitles: Option<PathBuf>,
}
#[derive(Debug)]
pub struct ReferenceStatus {
    pub field: &'static str,
    pub original: Option<String>,
    pub candidate: Option<PathBuf>,
    pub resolved: Option<PathBuf>,
    pub overridden: bool,
    pub issue: Option<String>,
}
/// Owned immutable review inputs. The project service must check these bytes and
/// its selected media identity again before publishing any output.
pub struct Inspection {
    pub source_selection: PathBuf,
    pub source_path: PathBuf,
    pub source_bytes: Vec<u8>,
    pub project: LegacyProject,
    pub media: ReferenceStatus,
    pub subtitles: ReferenceStatus,
    pub subtitle_bytes: Option<Vec<u8>>,
    pub subtitle_format: Option<subtitle_formats::Format>,
    pub subtitle_cues: Option<usize>,
}
impl Inspection {
    pub fn ready(&self) -> bool {
        self.media.issue.is_none()
            && self.subtitles.issue.is_none()
            && self.media.resolved.is_some()
            && self.subtitles.resolved.is_some()
            && self.subtitle_format.is_some()
            && self.subtitle_bytes.is_some()
    }
}

pub fn read(path: &Path) -> Result<Vec<u8>> {
    if !fs::metadata(path)
        .map_err(|e| format!("Cannot read legacy project: {e}"))?
        .is_file()
    {
        return Err("Legacy project must be a regular file".into());
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|e| format!("Cannot read legacy project: {e}"))?
        .take(MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > MAX_BYTES {
        return Err("Legacy project exceeds the 256 KiB limit".into());
    }
    Ok(bytes)
}

fn inspect_reference(
    base: &Path,
    kind: ReferenceKind,
    reference: Option<&Reference>,
    override_path: Option<&Path>,
) -> ReferenceStatus {
    let mut status = ReferenceStatus {
        field: kind.key(),
        original: reference.map(|r| r.value.clone()),
        candidate: None,
        resolved: None,
        overridden: override_path.is_some(),
        issue: None,
    };
    let inspected = (|| -> Result<PathBuf> {
        let candidate = if let Some(path) = override_path {
            if !path.is_absolute() {
                return Err("Choose an absolute local file path".into());
            }
            path.to_path_buf()
        } else {
            local_path(
                base,
                &reference
                    .ok_or("No file is specified; choose a local file")?
                    .value,
            )?
        };
        status.candidate = Some(candidate.clone());
        let resolved = candidate
            .canonicalize()
            .map_err(|e| format!("File is unavailable: {e}. Choose its current location"))?;
        if !fs::metadata(&resolved)
            .map_err(|e| e.to_string())?
            .is_file()
        {
            return Err("Reference must be a regular file; choose another file".into());
        }
        Ok(resolved)
    })();
    match inspected {
        Ok(path) => status.resolved = Some(path),
        Err(error) => status.issue = Some(error),
    }
    status
}

/// Checkpoints support cancellation/deadlines between bounded steps. Filesystem
/// calls and one format parse are not preemptible. Errors from checkpoints are
/// propagated, never converted into a missing-reference warning.
pub fn inspect(
    path: &Path,
    overrides: &Overrides,
    mut checkpoint: impl FnMut() -> Result<()>,
) -> Result<Inspection> {
    checkpoint()?;
    if !path
        .extension()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.eq_ignore_ascii_case("vssprj"))
    {
        return Err("Choose a VisualSubSync .vssprj project".into());
    }
    let source_selection = std::path::absolute(path).map_err(|e| e.to_string())?;
    let source_path = source_selection
        .canonicalize()
        .map_err(|e| format!("Cannot open legacy project: {e}"))?;
    let source_bytes = read(&source_path)?;
    checkpoint()?;
    let project = parse(&source_bytes)?;
    let base = source_path
        .parent()
        .ok_or("Project has no parent directory")?;
    let media_kind = if project.reference(ReferenceKind::Video).is_some() {
        ReferenceKind::Video
    } else {
        ReferenceKind::Audio
    };
    checkpoint()?;
    let media = inspect_reference(
        base,
        media_kind,
        project.reference(media_kind),
        overrides.media.as_deref(),
    );
    checkpoint()?;
    let mut subtitles = inspect_reference(
        base,
        ReferenceKind::Subtitles,
        project.reference(ReferenceKind::Subtitles),
        overrides.subtitles.as_deref(),
    );
    let mut subtitle_bytes = None;
    let mut subtitle_format = None;
    let mut subtitle_cues = None;
    checkpoint()?;
    if let Some(path) = &subtitles.resolved {
        let validation = (|| {
            let bytes = crate::subtitle_file::read(path)?;
            let parsed = crate::subtitle_file::parse_document(
                &bytes,
                path.extension().and_then(|s| s.to_str()),
            )?;
            Ok::<_, String>((bytes, parsed.source.format(), parsed.document.cues().len()))
        })();
        match validation {
            Ok((bytes, format, cues)) => {
                subtitle_bytes = Some(bytes);
                subtitle_format = Some(format);
                subtitle_cues = Some(cues);
            }
            Err(error) => {
                subtitles.issue = Some(format!(
                    "Invalid subtitles: {error}. Choose a supported UTF-8 subtitle file"
                ))
            }
        }
    }
    checkpoint()?;
    Ok(Inspection {
        source_selection,
        source_path,
        source_bytes,
        project,
        media,
        subtitles,
        subtitle_bytes,
        subtitle_format,
        subtitle_cues,
    })
}

/// Revalidate the reviewed text inputs, including changed symlink selections.
/// The project owner must separately recheck its captured media identity and
/// serialize final cancellation/publication. This function performs no writes.
pub fn verify_text_inputs(
    inspection: &Inspection,
    mut checkpoint: impl FnMut() -> Result<()>,
) -> Result<()> {
    checkpoint()?;
    if inspection
        .source_selection
        .canonicalize()
        .map_err(|e| format!("Legacy project is unavailable: {e}"))?
        != inspection.source_path
        || read(&inspection.source_path)? != inspection.source_bytes
    {
        return Err("Legacy project changed after review; inspect it again".into());
    }
    checkpoint()?;
    let candidate = inspection
        .subtitles
        .candidate
        .as_ref()
        .ok_or("Choose valid subtitles before importing")?;
    let path = inspection
        .subtitles
        .resolved
        .as_ref()
        .ok_or("Choose valid subtitles before importing")?;
    let bytes = inspection
        .subtitle_bytes
        .as_ref()
        .ok_or("Choose valid subtitles before importing")?;
    if candidate
        .canonicalize()
        .map_err(|e| format!("Reviewed subtitles are unavailable: {e}"))?
        != *path
        || crate::subtitle_file::read(path)? != *bytes
    {
        return Err("Subtitles changed after review; inspect the project again".into());
    }
    checkpoint()
}
