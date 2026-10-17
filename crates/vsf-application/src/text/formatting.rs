use super::{MAX_CUE_BYTES, Projection};
use crate::quality::{Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum Format {
    Bold,
    Italic,
    Underline,
}
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Formatted {
    pub text: String,
    pub start: u32,
    pub end: u32,
}

/// Pure draft transformation. Offsets are UTF-16 textarea boundaries. It does
/// not accept a document edit; the ordinary explicit Apply operation does that.
pub fn format_selection(source: &str, start: u32, end: u32, style: Format) -> Result<Formatted> {
    if source.len() > MAX_CUE_BYTES || source.contains(['\r', '\0']) {
        return Err(Error::Invalid(
            "Formatting requires a cue of at most 64 KiB without CR or NUL.".into(),
        ));
    }
    let byte = |target| {
        let mut units = 0;
        for (at, c) in source.char_indices() {
            if units == target {
                return Some(at);
            }
            units += c.len_utf16() as u32;
        }
        (units == target).then_some(source.len())
    };
    let a = byte(start)
        .ok_or_else(|| Error::Invalid("Selection splits a Unicode character.".into()))?;
    let b =
        byte(end).ok_or_else(|| Error::Invalid("Selection splits a Unicode character.".into()))?;
    if a >= b {
        return Err(Error::Invalid("Select the text to format first.".into()));
    }
    let p = Projection::new(source);
    if let Some(reason) = p.ambiguous {
        return Err(Error::Invalid(reason.into()));
    }
    if !p.safe_range(source, a..b) {
        return Err(Error::Invalid(
            "Select text within one formatting span, without tags or ASS escapes.".into(),
        ));
    }
    let style = match style {
        Format::Bold => 'b',
        Format::Italic => 'i',
        Format::Underline => 'u',
    };
    // Closing a new HTML span turns this style off in libass. Do not accidentally
    // disable an ASS style inherited by the rest of the cue.
    if p.protected.iter().any(|r| {
        let block = &source[r.clone()];
        block.starts_with("{\\")
            && block.split('\\').skip(1).any(|part| {
                part.starts_with(style)
                    && part.chars().nth(1).is_none_or(|c| !c.is_ascii_alphabetic())
            })
    }) {
        return Err(Error::Invalid(
            "This style is controlled by ASS overrides; edit those tags manually.".into(),
        ));
    }
    let open = format!("<{style}>");
    let close = format!("</{style}>");
    let wrapped = a >= open.len()
        && source
            .get(a - open.len()..a)
            .is_some_and(|s| s.eq_ignore_ascii_case(&open))
        && source
            .get(b..b + close.len())
            .is_some_and(|s| s.eq_ignore_ascii_case(&close));
    let same_style = p
        .protected
        .iter()
        .filter(|r| source[r.start..r.end].eq_ignore_ascii_case(&open))
        .count();
    if same_style > usize::from(wrapped) {
        return Err(Error::Invalid("This style already occurs elsewhere in the cue; select an exact styled span to toggle it.".into()));
    }
    let (text, start, end) = if wrapped {
        (
            format!(
                "{}{}{}",
                &source[..a - open.len()],
                &source[a..b],
                &source[b + close.len()..]
            ),
            start - 3,
            end - 3,
        )
    } else {
        (
            format!(
                "{}{open}{}{close}{}",
                &source[..a],
                &source[a..b],
                &source[b..]
            ),
            start + 3,
            end + 3,
        )
    };
    if text.len() > MAX_CUE_BYTES {
        return Err(Error::Invalid(
            "Formatted text would exceed the 64 KiB cue limit.".into(),
        ));
    }
    let after = Projection::new(&text);
    let old_tokens = p
        .protected
        .iter()
        .filter(|r| !wrapped || (r.start != a - open.len() && r.start != b))
        .map(|r| &source[r.clone()]);
    let new_tokens = after
        .protected
        .iter()
        .filter(|r| wrapped || (r.start != a && r.start != b + open.len()))
        .map(|r| &text[r.clone()]);
    if after.ambiguous.is_some() || !old_tokens.eq(new_tokens) {
        return Err(Error::Invalid(
            "Formatting would create or alter other markup; edit manually.".into(),
        ));
    }
    Ok(Formatted { text, start, end })
}
