use crate::Format;
use std::fmt;

/// Structured failures at format dispatch and the SRT adapter. ASS/SSA and
/// WebVTT internals retain their detailed policy messages inside Parse/Validation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Domain(subtitle_core::Error),
    Parse {
        format: Format,
        message: String,
    },
    Validation {
        format: Format,
        message: String,
    },
    FormatMismatch {
        expected: Format,
        actual: Format,
    },
    SaveExtension {
        format: Format,
        extension: Option<String>,
    },
    SrtLine {
        line: usize,
        source: Box<Error>,
    },
    InvalidSrtTimestamp {
        text: String,
    },
    SrtTimestampComponent {
        text: String,
    },
    InvalidTimestampNumber,
    SrtBlankLine,
    SrtNul,
    SrtLoneCarriageReturn,
    SrtMixedLineEndings,
    SrtMissingTiming,
    SrtInvalidLabel,
    SrtMissingArrow,
    SrtEventMetadata,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Domain(error) => error.fmt(f),
            Self::Parse { message, .. } | Self::Validation { message, .. } => f.write_str(message),
            Self::FormatMismatch { actual, .. } => write!(
                f,
                "File extension does not match the {} script; source was not changed",
                actual.extension().to_ascii_uppercase()
            ),
            Self::SaveExtension { format, .. } => write!(
                f,
                "Save this document as .{}; use an explicit conversion to change formats",
                format.extension()
            ),
            Self::SrtLine { line, source } => write!(f, "SRT line {line}: {source}"),
            Self::InvalidSrtTimestamp { text } => {
                write!(f, "Invalid SRT timestamp: {text}; expected HH:MM:SS,mmm")
            }
            Self::SrtTimestampComponent { text } => {
                write!(f, "Timestamp minutes and seconds must be below 60: {text}")
            }
            Self::InvalidTimestampNumber => f.write_str("Invalid timestamp number"),
            Self::SrtBlankLine => {
                f.write_str("SRT cue text cannot contain blank lines; use a separate cue")
            }
            Self::SrtNul => f.write_str("SRT contains NUL characters; UTF-8 text is required"),
            Self::SrtLoneCarriageReturn => f.write_str("Unsupported lone carriage return in SRT"),
            Self::SrtMixedLineEndings => {
                f.write_str("Mixed LF and CRLF line endings are unsupported")
            }
            Self::SrtMissingTiming => f.write_str("Cue needs a number and timing line"),
            Self::SrtInvalidLabel => f.write_str("Expected numeric cue number"),
            Self::SrtMissingArrow => f.write_str("Expected start --> end timing line"),
            Self::SrtEventMetadata => f.write_str(
                "SRT cannot preserve event metadata; explicit format conversion is required",
            ),
        }
    }
}
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Domain(error) => Some(error),
            Self::SrtLine { source, .. } => Some(source.as_ref()),
            _ => None,
        }
    }
}
impl From<subtitle_core::Error> for Error {
    fn from(error: subtitle_core::Error) -> Self {
        Self::Domain(error)
    }
}
impl From<Error> for String {
    fn from(error: Error) -> Self {
        error.to_string()
    }
}
