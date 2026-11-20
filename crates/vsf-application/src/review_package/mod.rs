//! Portable offline reviews. All inputs/outputs are bounded bytes, never paths.
mod anchors;
mod report;
mod threads;
use crate::{Result, comparison::Options, srt_limits, subtitle_file};
pub use anchors::reanchor;
use base64::{Engine, engine::general_purpose::STANDARD};
pub use report::{OPEN_TAG, sections};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use subtitle_formats::{Format, Parsed};
pub use threads::*;

// Two 16 MiB files take about 43 MiB in base64. Leave room for comments and HTML.
pub const MAX_BYTES: usize = 4 * srt_limits::MAX_BYTES;
pub fn check_report_size(n: usize) -> Result<()> {
    check_size(n)
        .map_err(|err| format!("{err}. Try Readable only or reduce the displayed content."))
}
pub fn check_size(n: usize) -> Result<()> {
    if n > MAX_BYTES {
        Err("Review exceeds the 64 MiB limit".into())
    } else {
        Ok(())
    }
}
fn validate_time(ms: u64) -> Result<()> {
    if ms > 8_640_000_000_000_000 {
        Err("Review timestamp outside the supported date range".into())
    } else {
        Ok(())
    }
}
pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Subtitle {
    pub name: String,
    pub format: String,
    pub sha256: String,
    pub bytes: String,
}
impl Subtitle {
    pub fn capture(name: &str, format: Format, bytes: &[u8]) -> Self {
        Self {
            name: safe_name(name, format.extension()),
            format: format.extension().into(),
            sha256: digest(bytes),
            bytes: STANDARD.encode(bytes),
        }
    }
    pub fn decode(&self) -> Result<Vec<u8>> {
        let n = self.bytes.len();
        if n > srt_limits::MAX_BYTES.div_ceil(3) * 4
            || (n / 4 * 3).saturating_sub(
                self.bytes
                    .as_bytes()
                    .iter()
                    .rev()
                    .take(2)
                    .take_while(|&&b| b == b'=')
                    .count(),
            ) > srt_limits::MAX_BYTES
        {
            return Err("Embedded subtitle exceeds 16 MiB".into());
        }
        let bytes = STANDARD
            .decode(&self.bytes)
            .map_err(|_| "Invalid review subtitle base64")?;
        srt_limits::validate_size(bytes.len())?;
        if digest(&bytes) != self.sha256 {
            return Err("Subtitle fingerprint mismatch".into());
        }
        Ok(bytes)
    }
    pub fn parsed(&self) -> Result<Parsed> {
        let bytes = self.decode()?;
        let format = Format::from_extension(&self.format)
            .filter(|f| f.extension() == self.format)
            .ok_or("Unknown review subtitle format")?;
        parse_embedded(&bytes, format)
    }
}
/// BOM-marked UTF-16 is decoded only for parsing. The envelope retains its exact bytes.
pub fn parse_embedded(bytes: &[u8], format: Format) -> Result<Parsed> {
    let text = embedded_text(bytes)?;
    subtitle_file::parse_document(text.as_ref(), Some(format.extension()))
}
/// UTF-8 recovery baseline, preserving source spelling and line endings.
pub fn embedded_text(bytes: &[u8]) -> Result<std::borrow::Cow<'_, [u8]>> {
    srt_limits::validate_size(bytes.len())?;
    if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        if !bytes.len().is_multiple_of(2) {
            return Err("Incomplete UTF-16 subtitle".into());
        }
        let le = bytes[0] == 0xff;
        let units = bytes[2..].as_chunks::<2>().0.iter().map(|p| {
            if le {
                u16::from_le_bytes(*p)
            } else {
                u16::from_be_bytes(*p)
            }
        });
        let text = char::decode_utf16(units)
            .collect::<std::result::Result<String, _>>()
            .map_err(|_| "Invalid UTF-16 subtitle")?;
        Ok(std::borrow::Cow::Owned(text.into_bytes()))
    } else {
        Ok(std::borrow::Cow::Borrowed(bytes))
    }
}
pub fn safe_name(name: &str, ext: &str) -> String {
    let leaf = name.rsplit(['/', '\\']).next().unwrap_or_default();
    let stem = leaf.rsplit_once('.').map_or(leaf, |(s, _)| s);
    let stem: String = stem
        .chars()
        .filter(|c| !c.is_control() && !r#"<>:"|?*"#.contains(*c))
        .take(48)
        .collect();
    let stem = stem.trim_matches([' ', '.']);
    let reserved = stem.split('.').next().unwrap_or("").to_ascii_uppercase();
    let device = matches!(reserved.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ["COM", "LPT"].iter().any(|p| {
            reserved
                .strip_prefix(p)
                .is_some_and(|n| n.len() == 1 && n.as_bytes()[0].is_ascii_digit())
        });
    format!(
        "{}.{}",
        if stem.is_empty() || device {
            "Subtitles"
        } else {
            stem
        },
        ext
    )
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Package {
    pub format: String,
    pub version: u32,
    pub round: u32,
    pub exported_by: String,
    pub exported_at_ms: u64,
    pub original: Subtitle,
    pub revised: Subtitle,
    pub options: Options,
    pub threads: Vec<Thread>,
}
impl Package {
    fn validate(&self) -> Result<()> {
        if self.version > 1 {
            return Err("This review package was made by a newer VisualSubForge".into());
        }
        if self.version != 1 || self.format != "visualsubforge-review" || self.round == 0 {
            return Err("Unsupported review package header".into());
        }
        if !self.exported_by.is_empty() {
            validate_name(&self.exported_by)?
        }
        validate_time(self.exported_at_ms)?;
        validate_threads(&self.threads)?;
        if [&self.original, &self.revised]
            .iter()
            .any(|s| s.name.chars().any(char::is_control))
        {
            return Err("Control characters in review filename".into());
        }
        self.original.parsed()?;
        self.revised.parsed()?;
        Ok(())
    }
    pub fn write(&self) -> Result<Vec<u8>> {
        self.write_bounded(check_size)
    }
    fn write_bounded(&self, check: fn(usize) -> Result<()>) -> Result<Vec<u8>> {
        self.validate()?;
        let bytes = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        check(bytes.len())?;
        Ok(bytes)
    }
}
pub fn read(bytes: &[u8]) -> Result<Package> {
    check_size(bytes.len())?;
    let input = std::str::from_utf8(bytes)
        .map_err(|_| "Review package must be UTF-8")?
        .trim_start();
    let json = if input.starts_with('<') {
        report::extract(input)?
    } else {
        input
    };
    #[derive(Deserialize)]
    struct Header {
        version: u64,
    }
    let header: Header =
        serde_json::from_str(json).map_err(|e| format!("Invalid review package: {e}"))?;
    if header.version > 1 {
        return Err("This review package was made by a newer VisualSubForge".into());
    }
    let mut package: Package =
        serde_json::from_str(json).map_err(|e| format!("Invalid review package: {e}"))?;
    package.validate()?;
    for s in [&mut package.original, &mut package.revised] {
        s.name = safe_name(&s.name, &s.format)
    }
    package.threads.sort_by(|a, b| a.id.cmp(&b.id));
    for t in &mut package.threads {
        t.sort_messages()
    }
    Ok(package)
}
#[cfg(test)]
mod tests;
