use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use subtitle_core::{Cue, Document, Result};
use subtitle_formats::{
    Source,
    ass::{Style, Variant},
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StyleFieldValue {
    pub name: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum StyleOperation {
    Create {
        name: String,
        copy_from: Option<String>,
        fields: Vec<StyleFieldValue>,
    },
    Edit {
        name: String,
        fields: Vec<StyleFieldValue>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct StyleFieldChange {
    pub name: String,
    pub before: Option<String>,
    pub after: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct StyleSummary {
    pub created: bool,
    pub old_name: Option<String>,
    pub name: String,
    pub fields: Vec<StyleFieldChange>,
    pub changed_cues: u32,
    pub changed_comments: u32,
    pub affected_cues: u32,
    pub reference_review_cues: u32,
}
pub struct Proposal {
    document: Document,
    source: Source,
    pub summary: StyleSummary,
    pub changed: bool,
}
impl Proposal {
    pub fn document(&self) -> &Document {
        &self.document
    }
    pub fn source(&self) -> &Source {
        &self.source
    }
    pub fn into_parts(self) -> (Document, Source) {
        (self.document, self.source)
    }
}

pub fn propose(
    document: &Document,
    source: &Source,
    operation: &StyleOperation,
    mut checkpoint: impl FnMut() -> Result<()>,
) -> Result<Proposal> {
    checkpoint()?;
    let script = source
        .ass()
        .ok_or("Style authoring requires an ASS/SSA document")?;
    if document.cues().len() > 100_000 {
        return Err("Style authoring supports at most 100,000 cues".into());
    }
    let (index, original, changes, created) = match operation {
        StyleOperation::Create {
            name,
            copy_from,
            fields,
        } => {
            if fields.iter().any(|f| f.name.eq_ignore_ascii_case("Name")) {
                return Err("Specify the new style name once".into());
            }
            let base = if let Some(name) = copy_from {
                script
                    .styles()
                    .iter()
                    .find(|s| &s.name == name)
                    .ok_or("The copied style no longer exists")?
                    .clone()
            } else {
                default_style(script.variant())
            };
            (None, base.with_field("Name", name)?, fields, true)
        }
        StyleOperation::Edit { name, fields } => {
            let index = script
                .styles()
                .iter()
                .position(|s| &s.name == name)
                .ok_or("Style no longer exists")?;
            (Some(index), script.styles()[index].clone(), fields, false)
        }
    };
    if changes.len() > 64 {
        return Err("Style edits support at most 64 fields".into());
    }
    if !created
        && (original.name.starts_with('*')
            || script
                .styles()
                .iter()
                .filter(|s| s.name.trim_start_matches('*') == original.name)
                .count()
                > 1)
    {
        return Err("This legacy starred style definition has ambiguous renderer identity; create a uniquely named copy".into());
    }
    let mut seen = HashSet::new();
    let mut style = original.clone();
    for field in changes {
        checkpoint()?;
        if !seen.insert(field.name.to_ascii_lowercase()) {
            return Err("Duplicate style field change".into());
        }
        let old = style
            .fields
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(&field.name))
            .ok_or_else(|| {
                format!(
                    "This style does not declare {:?}; create a new complete style to add fields",
                    field.name
                )
            })?;
        if old.1.trim() == field.value {
            continue;
        }
        validate_field(script.variant(), &field.name, &field.value)?;
        style = style.with_field(&field.name, &field.value)?;
    }
    if created {
        for (name, value) in &style.fields {
            if supported_field(script.variant(), name) {
                validate_field(script.variant(), name, value.trim())?;
            }
        }
    }
    let renamed = !created && style.name != original.name;
    if renamed && (original.name.eq_ignore_ascii_case("Default") || style.name == "Default") {
        return Err(
            "Keep the Default style name for renderer fallback; create a named copy instead".into(),
        );
    }
    let mut updated = script.with_style(index, style.clone())?;
    let mut cues = Vec::with_capacity(document.cues().len());
    let mut changed_cues = 0;
    let mut affected_cues = 0;
    let mut reference_review_cues = 0;
    for cue in document.cues() {
        checkpoint()?;
        let assigned = cue.metadata().and_then(|m| m.get("Style")).map(str::trim);
        let assignment_matches = assigned.is_some_and(|name| {
            let name = name.trim_start_matches('*');
            name == original.name
                || (original.name == "Default" && name.eq_ignore_ascii_case("Default"))
        });
        // Count exact assignments and reset references, without claiming to
        // predict animated rendering or fallback from unresolved original names.
        let resets = reset_ranges(cue.text(), &original.name);
        if resets.is_err() {
            reference_review_cues += 1;
        }
        if assignment_matches || resets.as_ref().is_ok_and(|r| !r.is_empty()) {
            affected_cues += 1;
        }
        let next = if renamed {
            let text = rename_resets(cue.text(), &original.name, &style.name)?;
            let metadata = cue.metadata().ok_or("ASS/SSA cue has no event fields")?;
            let metadata = if assignment_matches {
                let stars =
                    assigned.map_or(0, |name| name.len() - name.trim_start_matches('*').len());
                metadata.with_style(format!("{}{}", "*".repeat(stars), style.name))?
            } else {
                metadata.clone()
            };
            Cue::new(cue.id(), cue.start(), cue.end(), text)?.with_metadata(metadata)
        } else {
            cue.clone()
        };
        changed_cues += u32::from(next != *cue);
        cues.push(next);
    }
    let changed_comments = if renamed {
        let (next, count) = updated.rename_comment_references(
            &original.name,
            &style.name,
            |text| rename_resets(text, &original.name, &style.name),
            &mut checkpoint,
        )?;
        updated = next;
        u32::try_from(count).map_err(|_| "Too many commented events")?
    } else {
        0
    };
    let source_after = Source::Ass(updated);
    let document_after = Document::new(cues)?;
    checkpoint()?;
    crate::subtitle_file::validate_document(&document_after, &source_after)?;
    let fields = style
        .fields
        .iter()
        .enumerate()
        .filter_map(|(i, (name, value))| {
            let before = (!created).then(|| original.fields[i].1.clone());
            (before.as_ref() != Some(value)).then(|| StyleFieldChange {
                name: name.clone(),
                before,
                after: value.clone(),
            })
        })
        .collect();
    let changed = document != &document_after || source != &source_after;
    checkpoint()?;
    Ok(Proposal {
        document: document_after,
        source: source_after,
        changed,
        summary: StyleSummary {
            created,
            old_name: (!created).then_some(original.name),
            name: style.name,
            fields,
            changed_cues,
            changed_comments,
            affected_cues,
            reference_review_cues,
        },
    })
}

pub fn supported_field(variant: Variant, name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    matches!(
        name.as_str(),
        "name"
            | "fontname"
            | "fontsize"
            | "primarycolour"
            | "secondarycolour"
            | "backcolour"
            | "bold"
            | "italic"
            | "borderstyle"
            | "outline"
            | "shadow"
            | "alignment"
            | "marginl"
            | "marginr"
            | "marginv"
            | "encoding"
    ) || (variant == Variant::Ass
        && matches!(
            name.as_str(),
            "outlinecolour" | "underline" | "strikeout" | "scalex" | "scaley" | "spacing" | "angle"
        ))
        || (variant == Variant::Ssa && matches!(name.as_str(), "tertiarycolour" | "alphalevel"))
}
fn validate_field(variant: Variant, name: &str, value: &str) -> Result<()> {
    if !supported_field(variant, name) {
        return Err(format!(
            "Unsupported style field {name:?} is preserved read-only"
        ));
    }
    if value.len() > 1024 || value.chars().any(char::is_control) || value.contains(',') {
        return Err(format!(
            "{name}: value exceeds 1024 UTF-8 bytes or contains control characters/comma"
        ));
    }
    let invalid = || format!("{name}: value is outside the supported authoring range");
    let number = || {
        value
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .ok_or_else(invalid)
    };
    let integer = || value.trim().parse::<i64>().map_err(|_| invalid());
    let between = |low: f64, high: f64| {
        let n = number()?;
        if n >= low && n <= high {
            Ok(())
        } else {
            Err(invalid())
        }
    };
    match name.to_ascii_lowercase().as_str() {
        "name" => Ok(()), // Source validates the common record/name constraints.
        "fontname" => {
            if value.is_empty() || value.trim() != value || value.len() > 256 {
                Err(invalid())
            } else {
                Ok(())
            }
        }
        "fontsize" => {
            if number()? > 0.0 {
                between(0.0, 1000.0)
            } else {
                Err(invalid())
            }
        }
        "scalex" | "scaley" | "outline" | "shadow" => between(0.0, 1000.0),
        "spacing" => between(-1000.0, 1000.0),
        "angle" => between(-360000.0, 360000.0),
        "bold" | "italic" | "underline" | "strikeout" => {
            if [-1, 0, 1].contains(&integer()?) {
                Ok(())
            } else {
                Err(invalid())
            }
        }
        "borderstyle" => {
            if [1, 3].contains(&integer()?) {
                Ok(())
            } else {
                Err(invalid())
            }
        }
        "alignment" => {
            if (if variant == Variant::Ass {
                &[1, 2, 3, 4, 5, 6, 7, 8, 9][..]
            } else {
                &[1, 2, 3, 5, 6, 7, 9, 10, 11][..]
            })
            .contains(&integer()?)
            {
                Ok(())
            } else {
                Err(invalid())
            }
        }
        "marginl" | "marginr" | "marginv" => {
            if (0..=99999).contains(&integer()?) {
                Ok(())
            } else {
                Err(invalid())
            }
        }
        "alphalevel" | "encoding" => {
            if (0..=255).contains(&integer()?) {
                Ok(())
            } else {
                Err(invalid())
            }
        }
        "primarycolour" | "secondarycolour" | "outlinecolour" | "tertiarycolour" | "backcolour" => {
            let v = value.trim();
            if let Some(hex) = v.strip_prefix("&H").or_else(|| v.strip_prefix("&h")) {
                let hex = hex.strip_suffix('&').unwrap_or(hex);
                if hex.is_empty() || hex.len() > 8 || !hex.bytes().all(|c| c.is_ascii_hexdigit()) {
                    Err(invalid())
                } else {
                    Ok(())
                }
            } else if (-2147483648..=4294967295).contains(&integer()?) {
                Ok(())
            } else {
                Err(invalid())
            }
        }
        _ => Err(format!(
            "Unsupported style field {name:?} is preserved read-only"
        )),
    }
}

pub fn default_style(variant: Variant) -> Style {
    let common = "Name,Fontname,Fontsize,PrimaryColour,SecondaryColour";
    let (names, values) = if variant == Variant::Ass {
        (
            format!(
                "{common},OutlineColour,BackColour,Bold,Italic,Underline,StrikeOut,ScaleX,ScaleY,Spacing,Angle,BorderStyle,Outline,Shadow,Alignment,MarginL,MarginR,MarginV,Encoding"
            ),
            "Default,sans-serif,24,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,0,0,2,10,10,15,1",
        )
    } else {
        (
            format!(
                "{common},TertiaryColour,BackColour,Bold,Italic,BorderStyle,Outline,Shadow,Alignment,MarginL,MarginR,MarginV,AlphaLevel,Encoding"
            ),
            "Default,sans-serif,24,16777215,255,0,0,0,0,1,0,0,2,10,10,15,0,1",
        )
    };
    Style {
        name: "Default".into(),
        fields: names
            .split(',')
            .zip(values.split(','))
            .map(|(n, v)| (n.into(), v.into()))
            .collect(),
    }
}

// A precise reset token ends at the next override introducer or closing brace.
// Keep plain text/escaped braces and other names (including prefixes) intact.
fn rename_resets(text: &str, old: &str, new: &str) -> Result<String> {
    let ranges = reset_ranges(text, old)?;
    let mut result = text.to_owned();
    for range in ranges.into_iter().rev() {
        result.replace_range(range, new);
    }
    Ok(result)
}
fn reset_ranges(text: &str, old: &str) -> Result<Vec<std::ops::Range<usize>>> {
    if !text
        .split('\\')
        .skip(1)
        .any(|s| s.trim_start_matches([' ', '\t']).starts_with('r'))
    {
        return Ok(Vec::new());
    }
    if text.len() > crate::subtitle_markup::MAX_CUE_BYTES {
        return Err("Style rename needs manual review of reset-bearing text above 64 KiB".into());
    }
    let mut ranges = Vec::new();
    let mut at = 0;
    while at < text.len() {
        let rest = &text[at..];
        if rest.starts_with("\\{") {
            at += 2;
            continue;
        }
        if rest.starts_with('{') {
            let end = rest
                .find('}')
                .ok_or("Style rename needs manual review of an incomplete override")?;
            let block = &rest[1..end];
            if block.contains("\\r") && (block.contains('{') || block.contains("\\t")) {
                return Err(
                    "Style rename needs manual review of nested or transformed resets".into(),
                );
            }
            let mut tokens = block.split('\\');
            let mut offset = at + 1 + tokens.next().unwrap_or_default().len();
            for token in tokens {
                offset += 1;
                let trimmed = token.trim_start_matches([' ', '\t']);
                if trimmed.starts_with('r') && (trimmed != token || trimmed.contains(['(', ')'])) {
                    return Err(
                        "Style rename needs manual review of spaced or parenthesized reset syntax"
                            .into(),
                    );
                }
                if let Some(argument) = token.strip_prefix('r') {
                    if argument == old {
                        ranges.push(offset + 1..offset + token.len());
                    } else if argument.trim() == old {
                        return Err(
                            "Style rename needs manual review of a padded reset name".into()
                        );
                    }
                }
                offset += token.len();
            }
            at += end + 1;
        } else {
            at += rest.chars().next().expect("source boundary").len_utf8();
        }
    }
    Ok(ranges)
}
