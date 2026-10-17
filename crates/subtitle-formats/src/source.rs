//! Source-format dispatch. The accepted document is never silently converted.
use crate::{ass, srt};
use subtitle_core::{CueMetadata, Document, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Srt,
    Ass,
    Ssa,
}
impl Format {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Srt => "srt",
            Self::Ass => "ass",
            Self::Ssa => "ssa",
        }
    }
    pub fn from_extension(extension: &str) -> Option<Self> {
        match extension.to_ascii_lowercase().as_str() {
            "srt" => Some(Self::Srt),
            "ass" => Some(Self::Ass),
            "ssa" => Some(Self::Ssa),
            _ => None,
        }
    }
    pub fn time_step_ms(self) -> u64 {
        if self == Self::Srt { 1 } else { 10 }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    Srt(srt::Source),
    Ass(ass::Source),
}
impl From<srt::Source> for Source {
    fn from(source: srt::Source) -> Self {
        Self::Srt(source)
    }
}
#[derive(Debug, PartialEq, Eq)]
pub struct Parsed {
    pub document: Document,
    pub source: Source,
}

/// A known extension constrains interpretation. Content sniffing is only for
/// callers without format identity; inline ASS tags never change an SRT format.
pub fn parse(input: &str, expected: Option<Format>) -> Result<Parsed> {
    let first = input
        .trim_start_matches('\u{feff}')
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with(';'))
        .unwrap_or_default();
    let styled = matches!(expected, Some(Format::Ass | Format::Ssa))
        || (expected.is_none() && first.starts_with('['));
    if styled {
        let parsed = ass::parse(input)?;
        let source = Source::Ass(parsed.source);
        if expected.is_some_and(|format| format != source.format()) {
            return Err(format!(
                "File extension does not match the {} script; source was not changed",
                source.format().extension().to_ascii_uppercase()
            ));
        }
        Ok(Parsed {
            document: parsed.document,
            source,
        })
    } else {
        let parsed = srt::parse(input)?;
        Ok(Parsed {
            document: parsed.document,
            source: Source::Srt(parsed.source),
        })
    }
}
impl Source {
    pub fn format(&self) -> Format {
        match self {
            Self::Srt(_) => Format::Srt,
            Self::Ass(source) => match source.variant() {
                ass::Variant::Ass => Format::Ass,
                ass::Variant::Ssa => Format::Ssa,
            },
        }
    }
    pub fn serialize(&self, document: &Document) -> Result<String> {
        match self {
            Self::Srt(source) => srt::serialize(document, source),
            Self::Ass(source) => ass::serialize(document, source),
        }
    }
    pub fn serialized_len(&self, document: &Document) -> Result<usize> {
        match self {
            Self::Srt(source) => srt::serialized_len(document, source),
            Self::Ass(source) => ass::serialized_len(document, source),
        }
    }
    pub fn ass(&self) -> Option<&ass::Source> {
        match self {
            Self::Ass(source) => Some(source),
            _ => None,
        }
    }
    pub fn default_metadata(&self) -> Result<Option<CueMetadata>> {
        match self {
            Self::Srt(_) => Ok(None),
            Self::Ass(source) => source.default_metadata().map(Some),
        }
    }
    /// Textarea line breaks have a lossless explicit ASS representation. Existing
    /// escape sequences and tags are left untouched, including in SRT.
    pub fn prepare_text(&self, text: String) -> Result<String> {
        match self {
            Self::Srt(_) => {
                srt::validate_text(&text)?;
                Ok(text)
            }
            Self::Ass(_) => Ok(text.replace('\n', "\\N")),
        }
    }
    pub fn validate_save_extension(&self, extension: Option<&str>) -> Result<()> {
        let selected = extension.and_then(Format::from_extension);
        if selected == Some(self.format()) || (self.format() == Format::Srt && selected.is_none()) {
            return Ok(());
        }
        Err(format!(
            "Save this document as .{}; cross-format conversion is not available",
            self.format().extension()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const ASS: &str = "[Script Info]\nScriptType: v4.00+\n[V4+ Styles]\nFormat: Name, Fontname\nStyle: Default,Arial\n[Events]\nFormat: Start, End, Style, Text\nDialogue: 0:00:01.00,0:00:02.00,Default,{\\r}Hi\\Nthere";
    #[test]
    fn dispatch_retains_format_and_rejects_implicit_conversion() {
        let ass = parse(ASS, Some(Format::Ass)).unwrap();
        assert_eq!(ass.source.format(), Format::Ass);
        assert_eq!(ass.source.serialize(&ass.document).unwrap(), ASS);
        assert!(parse(ASS, Some(Format::Ssa)).is_err());
        assert!(parse(ASS, Some(Format::Srt)).is_err());
        assert_eq!(parse(ASS, None).unwrap(), ass);
        assert!(ass.source.validate_save_extension(Some("srt")).is_err());
        assert!(ass.source.validate_save_extension(Some("ASS")).is_ok());
        assert_eq!(
            ass.source.prepare_text("{\\r}a\nb\\Nstay".into()).unwrap(),
            "{\\r}a\\Nb\\Nstay"
        );
        let srt = parse(
            "1\n00:00:01,001 --> 00:00:02,999\n{\\an8}<i>hi</i>\\Nthere\n\n",
            None,
        )
        .unwrap();
        assert_eq!(srt.source.format(), Format::Srt);
        assert_eq!(srt.source.format().time_step_ms(), 1);
        assert!(srt.source.serialize(&ass.document).is_err());
        assert!(srt.source.validate_save_extension(Some("ass")).is_err());
        assert_eq!(
            srt.source.prepare_text("{\\r}a\nb".into()).unwrap(),
            "{\\r}a\nb"
        );
    }
}
