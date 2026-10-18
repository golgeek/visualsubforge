//! Immutable, explicitly lossy export proposals. No accepted-state mutation or IO.
mod script;
mod text;

use serde::{Deserialize, Serialize};
use subtitle_core::{Cue, Document, Milliseconds, Result};
use subtitle_formats::{Format, Source, srt};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum ConversionFormat {
    Srt,
    Ass,
    Ssa,
}
impl ConversionFormat {
    pub fn format(self) -> Format {
        match self {
            Self::Srt => Format::Srt,
            Self::Ass => Format::Ass,
            Self::Ssa => Format::Ssa,
        }
    }
}
impl TryFrom<Format> for ConversionFormat {
    type Error = String;
    fn try_from(value: Format) -> Result<Self> {
        Ok(match value {
            Format::Srt => Self::Srt,
            Format::Ass => Self::Ass,
            Format::Ssa => Self::Ssa,
            Format::WebVtt => return Err("WebVTT conversion is not implemented; save this document as .vtt to preserve its settings, styles and timestamp markers".into()),
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum ConversionPrecision {
    Exact,
    /// Round each endpoint to the nearest 10 ms, with ties rounding upward.
    Nearest,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConversionOptions {
    pub target: ConversionFormat,
    pub precision: ConversionPrecision,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum ConversionSeverity {
    Notice,
    Loss,
    Blocked,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ConversionExample {
    /// Source cue identity; never a newly allocated export identity.
    pub cue_id: Option<String>,
    pub detail: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ConversionIssue {
    pub code: String,
    pub severity: ConversionSeverity,
    pub message: String,
    /// Exact affected count. Examples are capped, not a complete occurrence list.
    pub count: u32,
    pub examples: Vec<ConversionExample>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ConversionReport {
    pub source: ConversionFormat,
    pub options: ConversionOptions,
    pub cues: u32,
    pub output_bytes: Option<u32>,
    pub issues: Vec<ConversionIssue>,
}
impl ConversionReport {
    pub fn blocked(&self) -> bool {
        self.issues
            .iter()
            .any(|i| i.severity == ConversionSeverity::Blocked)
    }
    fn issue(
        &mut self,
        code: &str,
        severity: ConversionSeverity,
        message: &str,
        count: u32,
        cue: Option<&Cue>,
        detail: &str,
    ) {
        if count == 0 {
            return;
        }
        let index = self.issues.iter().position(|i| i.code == code);
        let issue = if let Some(index) = index {
            &mut self.issues[index]
        } else {
            self.issues.push(ConversionIssue {
                code: code.into(),
                severity,
                message: message.into(),
                count: 0,
                examples: Vec::new(),
            });
            self.issues.last_mut().expect("just inserted")
        };
        issue.count += count;
        if issue.examples.len() < 8 && !detail.is_empty() {
            let mut clipped: String = detail.chars().take(240).collect();
            if clipped.len() < detail.len() {
                clipped.push('…');
            }
            issue.examples.push(ConversionExample {
                cue_id: cue.map(|c| c.id().0.to_string()),
                detail: clipped,
            });
        }
    }
}

pub struct Proposal {
    report: ConversionReport,
    output: Option<String>,
}
impl Proposal {
    pub fn report(&self) -> &ConversionReport {
        &self.report
    }

    /// Only the exact reviewed bytes may be published. Blocked reports have none.
    pub fn output(&self) -> Option<&str> {
        self.output.as_deref()
    }
}

pub fn propose(
    document: &Document,
    source: &Source,
    options: ConversionOptions,
    mut checkpoint: impl FnMut() -> Result<()>,
) -> Result<Proposal> {
    checkpoint()?;
    if source.format() == options.target.format() {
        return Err("Choose a different format; use Save As for a same-format copy".into());
    }
    if document.cues().len() > 100_000 {
        return Err("Conversion supports at most 100,000 cues".into());
    }
    crate::subtitle_file::validate_document(document, source)?;
    checkpoint()?;
    let mut report = ConversionReport {
        source: source.format().try_into()?,
        options,
        cues: document.cues().len() as u32,
        output_bytes: None,
        issues: Vec::new(),
    };
    report.issue("canonical", ConversionSeverity::Notice,
        "The separate export uses UTF-8 without BOM, LF endings and canonical records. SRT labels are renumbered; source formatting is not retained.",
        1, None, "The open document, original file and Undo history remain unchanged.");
    let header = script::header(
        document,
        source,
        options.target,
        &mut report,
        &mut checkpoint,
    )?;
    let mut cues = Vec::with_capacity(document.cues().len());
    for cue in document.cues() {
        checkpoint()?;
        if cue.text().len() > crate::subtitle_markup::MAX_CUE_BYTES {
            report.issue(
                "cue_limit",
                ConversionSeverity::Blocked,
                "Conversion supports cue text up to 64 KiB.",
                1,
                Some(cue),
                "Reduce this cue before converting.",
            );
            continue;
        }
        let Some((start, end)) = endpoints(cue, source.format(), options, &mut report)? else {
            continue;
        };
        let text = text::convert(cue, source.format(), options.target, &mut report);
        let mut next = Cue::new(cue.id(), start, end, text)?;
        if options.target == ConversionFormat::Srt {
            if let Err(error) = srt::validate_text(next.text()) {
                report.issue(
                    "srt_text",
                    ConversionSeverity::Blocked,
                    "This text cannot be represented as one SRT cue.",
                    1,
                    Some(cue),
                    &error,
                );
                continue;
            }
            script::discard_metadata(cue, &mut report);
        } else {
            next = next.with_metadata(script::metadata(cue, options.target, &mut report)?);
        }
        cues.push(next);
    }
    checkpoint()?;
    let output = if report.blocked() {
        None
    } else {
        let document = Document::new(cues)?;
        let source = subtitle_formats::parse(&header, Some(options.target.format()))?.source;
        match crate::subtitle_file::validate_document(&document, &source) {
            Ok(()) => {
                checkpoint()?;
                let bytes = source.serialize(&document)?;
                report.output_bytes = Some(bytes.len() as u32);
                Some(bytes)
            }
            Err(error) => {
                report.issue(
                    "output_size",
                    ConversionSeverity::Blocked,
                    "The export exceeds the supported file size or format limits.",
                    1,
                    None,
                    &error,
                );
                None
            }
        }
    };
    checkpoint()?;
    Ok(Proposal { report, output })
}

fn endpoints(
    cue: &Cue,
    source: Format,
    options: ConversionOptions,
    report: &mut ConversionReport,
) -> Result<Option<(Milliseconds, Milliseconds)>> {
    let (start, end) = (cue.start().get(), cue.end().get());
    if options.target == ConversionFormat::Srt
        || (start.is_multiple_of(10) && end.is_multiple_of(10))
    {
        return Ok(Some((cue.start(), cue.end())));
    }
    let rounded = ((start + 5) / 10 * 10, (end + 5) / 10 * 10);
    let detail = format!("{start}–{end} ms → {}–{} ms", rounded.0, rounded.1);
    if options.precision == ConversionPrecision::Exact {
        report.issue("precision_required", ConversionSeverity::Blocked, "ASS/SSA requires 10 ms endpoints. Choose explicit nearest rounding or edit these times.", 1, Some(cue), &detail);
        return Ok(None);
    }
    if rounded.1 <= rounded.0 || rounded.1 > Milliseconds::MAX {
        report.issue(
            "invalid_rounding",
            ConversionSeverity::Blocked,
            "Rounding would collapse a cue or exceed the supported time range.",
            1,
            Some(cue),
            &detail,
        );
        return Ok(None);
    }
    if rounded.1 - rounded.0 != end - start
        && crate::subtitle_markup::retiming_is_ambiguous(cue, source)
    {
        report.issue("timed_effect", ConversionSeverity::Blocked, "Rounding would change the duration of timed effects or ambiguous markup. Edit the cue timing explicitly first.", 1, Some(cue), &detail);
        return Ok(None);
    }
    report.issue("rounded", ConversionSeverity::Loss, "Endpoints round to the nearest 10 ms; ties round upward. Cue duration, gaps or overlaps can change.", 1, Some(cue), &detail);
    Ok(Some((
        Milliseconds::new(rounded.0)?,
        Milliseconds::new(rounded.1)?,
    )))
}
