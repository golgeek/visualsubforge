//! Pure, loss-preserving style records. Application proposals validate field
//! semantics and cue/reset references before accepting a source/cue pair.
use super::{Authoring, Line, Result, Source, Style, Variant, replace_value};
use std::{borrow::Cow, collections::HashSet, sync::Arc};

pub const MAX_STYLES: usize = 1024;
pub const MAX_STYLE_FIELDS: usize = 64;
pub const MAX_STYLE_VALUE_BYTES: usize = 1024;

impl Style {
    /// Set an existing field while preserving its padding and every other byte.
    pub fn with_field(&self, name: &str, value: &str) -> Result<Self> {
        if value.len() > MAX_STYLE_VALUE_BYTES || value.contains(['\r', '\n', '\0', ',']) {
            return Err(
                "Style values must fit 1024 UTF-8 bytes without commas or line breaks".into(),
            );
        }
        let mut style = self.clone();
        let (_, old) = style
            .fields
            .iter_mut()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .ok_or_else(|| format!("Style does not declare field {name:?}"))?;
        *old = replace_value(old, value);
        if name.eq_ignore_ascii_case("Name") {
            style.name = value.into();
        }
        Ok(style)
    }
}
impl Source {
    /// Replace a definition or append a new one. Existing field order/names are
    /// invariant; unknown columns stay untouched unless deliberately supplied.
    /// Rename reference validation belongs to the application transaction, not
    /// this source-record adapter. No cue is changed by this method.
    pub fn with_style(&self, index: Option<usize>, style: Style) -> Result<Self> {
        if self.styles().len() > MAX_STYLES
            || (index.is_none() && self.styles().len() == MAX_STYLES)
        {
            return Err("Style authoring supports at most 1024 definitions".into());
        }
        let mut fields = HashSet::new();
        if style.fields.is_empty()
            || style.fields.len() > MAX_STYLE_FIELDS
            || style.fields.iter().any(|(name, value)| {
                name.is_empty()
                    || name.trim() != name
                    || name.contains(['\r', '\n', '\0', ','])
                    || !fields.insert(name.to_ascii_lowercase())
                    || value.len() > MAX_STYLE_VALUE_BYTES
                    || value.contains(['\r', '\n', '\0', ','])
            })
        {
            return Err("Style fields are invalid, duplicate or exceed authoring limits".into());
        }
        if style.name.is_empty()
            || style.name.trim() != style.name
            || style.name.len() > 128
            || style.name.starts_with('*')
            || (style.name.eq_ignore_ascii_case("Default") && style.name != "Default")
            || style.name.chars().any(char::is_control)
            || style.name.contains([',', '\\', '{', '}', '(', ')'])
            || style
                .fields
                .iter()
                .find(|(n, _)| n.eq_ignore_ascii_case("Name"))
                .map(|(_, v)| v.trim())
                != Some(style.name.as_str())
        {
            return Err("Style name must match its Name field, fit 128 UTF-8 bytes and avoid controls, commas, braces, parentheses, backslashes, leading stars and case variants of Default".into());
        }
        if self
            .styles()
            .iter()
            .enumerate()
            .any(|(i, s)| Some(i) != index && s.name.trim_start_matches('*') == style.name)
        {
            return Err(format!("Style {:?} already exists", style.name));
        }
        if let Some(index) = index {
            let old = self.styles().get(index).ok_or("Style no longer exists")?;
            if !old
                .fields
                .iter()
                .map(|(n, _)| n)
                .eq(style.fields.iter().map(|(n, _)| n))
            {
                return Err("Editing a style must preserve its declared fields and order".into());
            }
            if old == &style {
                return Ok(self.clone());
            }
        }
        let mut styles = self.styles().to_vec();
        if let Some(index) = index {
            styles[index] = style;
        } else {
            styles.push(style);
        }
        Ok(Self(
            Arc::clone(&self.0),
            Arc::new(Authoring {
                styles: Some(styles),
                ..self.1.as_ref().clone()
            }),
        ))
    }
    /// Update valid Comment event references for an explicit style rename.
    /// Opaque/malformed comments remain readable and saveable; renaming refuses
    /// them because their references cannot be proven safe. The application
    /// supplies its same markup-aware text transformation used for Dialogue.
    pub fn rename_comment_references(
        &self,
        old: &str,
        new: &str,
        mut text: impl FnMut(&str) -> Result<String>,
        mut checkpoint: impl FnMut() -> Result<()>,
    ) -> Result<(Self, usize)> {
        let mut records = self.1.records.clone();
        let mut in_events = false;
        let mut format = None;
        let mut changed = 0;
        for (index, line) in self.0.lines.iter().enumerate() {
            checkpoint()?;
            let raw = self.1.records.get(&index).unwrap_or(&line.raw);
            let body = raw
                .strip_suffix("\r\n")
                .or_else(|| raw.strip_suffix('\n'))
                .unwrap_or(raw);
            let ending = &raw[body.len()..];
            let trimmed = body.trim_start_matches('\u{feff}').trim();
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                in_events = trimmed.eq_ignore_ascii_case("[Events]");
            }
            if let Some(columns) = &line.format {
                format = Some(columns);
            }
            if !in_events {
                continue;
            }
            let Some((prefix, fields)) = body.split_once(':') else {
                if trimmed.to_ascii_lowercase().starts_with("comment") {
                    return Err("Cannot rename a style while a Comment event is malformed".into());
                }
                continue;
            };
            if !prefix.trim().eq_ignore_ascii_case("Comment") {
                continue;
            }
            let columns = format.ok_or("Cannot rename a style: Comment has no Format")?;
            let mut values = super::values(fields, columns, true)
                .map_err(|_| "Cannot rename a style while a Comment event is malformed")?;
            let mut dirty = false;
            for (column, value) in columns.iter().zip(&mut values) {
                if column.eq_ignore_ascii_case("Style")
                    && value.trim().trim_start_matches('*') == old
                {
                    let stars = value.trim().len() - value.trim().trim_start_matches('*').len();
                    *value = replace_value(value, &format!("{}{new}", "*".repeat(stars)));
                    dirty = true;
                } else if column.eq_ignore_ascii_case("Text") {
                    let updated = text(value)?;
                    dirty |= *value != updated;
                    *value = updated;
                }
            }
            if dirty {
                records.insert(index, format!("{prefix}:{}{ending}", values.join(",")));
                changed += 1;
            }
        }
        Ok((
            Self(
                Arc::clone(&self.0),
                Arc::new(Authoring {
                    records,
                    ..self.1.as_ref().clone()
                }),
            ),
            changed,
        ))
    }
    pub(super) fn style_line<'a>(&'a self, line: &'a Line) -> Cow<'a, str> {
        let Some(index) = line.style else {
            return Cow::Borrowed(&line.raw);
        };
        let current = &self.styles()[index];
        if current == &self.0.styles[index] {
            return Cow::Borrowed(&line.raw);
        }
        let ending = if line.raw.ends_with("\r\n") {
            "\r\n"
        } else if line.raw.ends_with('\n') {
            "\n"
        } else {
            ""
        };
        // The parser accepted this Style record, including its colon and fields.
        let prefix = line
            .raw
            .split_once(':')
            .map_or("Style", |(prefix, _)| prefix);
        Cow::Owned(format!(
            "{prefix}:{}{ending}",
            current
                .fields
                .iter()
                .map(|(_, v)| v.as_str())
                .collect::<Vec<_>>()
                .join(",")
        ))
    }
    pub(super) fn write_added_styles(
        &self,
        position: usize,
        ended: &mut bool,
        write: &mut impl FnMut(&str),
    ) {
        let added = &self.styles()[self.0.styles.len()..];
        if added.is_empty() || position != self.0.styles_end.unwrap_or(self.0.events_start) {
            return;
        }
        let newline = self.0.newline;
        if !*ended {
            write(newline);
        }
        if self.0.styles_end.is_none() {
            write(match self.variant() {
                Variant::Ass => "[V4+ Styles]",
                Variant::Ssa => "[V4 Styles]",
            });
            write(newline);
        }
        // Each appended record declares its own columns. Existing Format lines,
        // comments and record ordering are left exactly as they were.
        for style in added {
            write("Format: ");
            write(
                &style
                    .fields
                    .iter()
                    .map(|(n, _)| n.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
            );
            write(newline);
            write("Style:");
            write(
                &style
                    .fields
                    .iter()
                    .map(|(_, v)| v.as_str())
                    .collect::<Vec<_>>()
                    .join(","),
            );
            write(newline);
        }
        *ended = true;
    }
}
