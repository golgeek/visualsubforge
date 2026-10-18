//! Loss-preserving UTF-8 ASS v4+ / SSA v4 source records. No IO or renderer.
//! Unedited records are retained verbatim. Authoring overlays share the original
//! records so history does not copy an entire script for a style change.
mod styles;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use subtitle_core::{Cue, CueId, CueMetadata, Document, Milliseconds, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Variant {
    Ass,
    Ssa,
}
impl Variant {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Ass => "ass",
            Self::Ssa => "ssa",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Style {
    pub name: String,
    pub fields: Vec<(String, String)>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source(Arc<SourceData>, Arc<Authoring>);
#[derive(Debug, Default, PartialEq, Eq)]
struct Authoring {
    styles: Option<Vec<Style>>,
}
#[derive(Debug, PartialEq, Eq)]
struct SourceData {
    variant: Variant,
    lines: Vec<Line>,
    originals: HashMap<CueId, Original>,
    styles: Vec<Style>,
    script_info: Vec<(String, String)>,
    format: Vec<String>,
    events_end: usize,
    events_start: usize,
    styles_end: Option<usize>,
    newline: &'static str,
}
#[derive(Debug, PartialEq, Eq)]
struct Line {
    raw: String,
    event: Option<CueId>,
    format: Option<Vec<String>>,
    style: Option<usize>,
}
#[derive(Debug, PartialEq, Eq)]
struct Original {
    cue: Cue,
    prefix: String,
    values: Vec<String>,
    format: Vec<String>,
    ending: String,
}
#[derive(Debug, PartialEq, Eq)]
pub struct Parsed {
    pub document: Document,
    pub source: Source,
}

pub fn parse_time(value: &str) -> Result<Milliseconds> {
    let bad = || format!("Invalid ASS/SSA timestamp {value:?}; expected H:MM:SS.cc");
    let (clock, fraction) = value.trim().split_once('.').ok_or_else(bad)?;
    let parts: Vec<_> = clock.split(':').collect();
    if parts.len() != 3
        || !(1..=4).contains(&parts[0].len())
        || parts[1].len() != 2
        || parts[2].len() != 2
        || fraction.len() != 2
        || parts
            .iter()
            .copied()
            .chain([fraction])
            .any(|p| !p.bytes().all(|c| c.is_ascii_digit()))
    {
        return Err(bad());
    }
    let hours: u64 = parts[0].parse().map_err(|_| bad())?;
    let minutes: u64 = parts[1].parse().map_err(|_| bad())?;
    let seconds: u64 = parts[2].parse().map_err(|_| bad())?;
    let hundredths: u64 = fraction.parse().map_err(|_| bad())?;
    if minutes >= 60 || seconds >= 60 {
        return Err(bad());
    }
    Milliseconds::new(hours * 3_600_000 + minutes * 60_000 + seconds * 1000 + hundredths * 10)
}
pub fn format_time(time: Milliseconds) -> Result<String> {
    let t = time.get();
    if !t.is_multiple_of(10) {
        return Err(
            "ASS/SSA cue times must use 10 ms increments; no rounding is performed on save".into(),
        );
    }
    Ok(format!(
        "{}:{:02}:{:02}.{:02}",
        t / 3_600_000,
        (t / 60_000) % 60,
        (t / 1000) % 60,
        (t % 1000) / 10
    ))
}
fn fields(value: &str, events: bool) -> Result<Vec<String>> {
    let columns: Vec<String> = value.split(',').map(|s| s.trim().to_owned()).collect();
    let mut seen = HashSet::new();
    if columns.iter().any(|s| {
        s.is_empty() || s.contains(['\r', '\n', '\0']) || !seen.insert(s.to_ascii_lowercase())
    }) {
        return Err("Format has empty, invalid or duplicate columns".into());
    }
    for required in if events {
        &["Start", "End", "Style", "Text"][..]
    } else {
        &["Name"][..]
    } {
        if !columns.iter().any(|s| s.eq_ignore_ascii_case(required)) {
            return Err(format!("Format is missing required {required} column"));
        }
    }
    if events
        && !columns
            .last()
            .is_some_and(|s| s.eq_ignore_ascii_case("Text"))
    {
        return Err("ASS/SSA event Text must be the last Format column".into());
    }
    Ok(columns)
}
fn values(value: &str, columns: &[String], text_last: bool) -> Result<Vec<String>> {
    let parts: Vec<String> = if text_last {
        value
            .splitn(columns.len(), ',')
            .map(str::to_owned)
            .collect()
    } else {
        value.split(',').map(str::to_owned).collect()
    };
    if parts.len() != columns.len() {
        return Err("Record does not match its declared Format columns".into());
    }
    Ok(parts)
}
fn lookup<'a>(names: &[String], values: &'a [String], name: &str) -> Option<&'a str> {
    names
        .iter()
        .position(|n| n.eq_ignore_ascii_case(name))
        .map(|i| values[i].as_str())
}
fn endpoint_or_text(name: &str) -> bool {
    ["Start", "End", "Text"]
        .iter()
        .any(|n| name.eq_ignore_ascii_case(n))
}
pub fn parse(input: &str) -> Result<Parsed> {
    if input.contains('\0') {
        return Err("ASS/SSA contains NUL characters; UTF-8 text is required".into());
    }
    let mut section = String::new();
    let mut variant = None;
    let mut style_variant = None;
    let mut style_format = None;
    let mut event_format = None;
    let mut seen_info = false;
    let mut seen_events = false;
    let mut events_end = None;
    let mut events_start = 0;
    let mut styles_end = None;
    let mut lines = Vec::new();
    let mut cues = Vec::new();
    let mut originals = HashMap::new();
    let mut styles = Vec::new();
    let mut style_names = HashSet::new();
    let mut script_info = Vec::new();
    let newline = if input.contains("\r\n") { "\r\n" } else { "\n" };
    for (i, raw) in input.split_inclusive('\n').enumerate() {
        let body = raw
            .strip_suffix("\r\n")
            .or_else(|| raw.strip_suffix('\n'))
            .unwrap_or(raw);
        let ending = &raw[body.len()..];
        let body = if i == 0 {
            body.strip_prefix('\u{feff}').unwrap_or(body)
        } else {
            body
        };
        if body.contains('\r') {
            return Err(format!("ASS/SSA line {}: lone carriage return", i + 1));
        }
        let trimmed = body.trim();
        let mut line = Line {
            raw: raw.into(),
            event: None,
            format: None,
            style: None,
        };
        let result: Result<()> = (|| {
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                let next = trimmed.to_ascii_lowercase();
                if section == "[events]" && next != section {
                    events_end = Some(i);
                }
                if matches!(section.as_str(), "[v4+ styles]" | "[v4 styles]") && next != section {
                    styles_end = Some(i);
                }
                match next.as_str() {
                    "[script info]" => {
                        if seen_info {
                            return Err("Duplicate Script Info section is ambiguous".into());
                        }
                        seen_info = true;
                    }
                    "[events]" => {
                        if seen_events {
                            return Err("Duplicate Events section is ambiguous".into());
                        }
                        seen_events = true;
                        events_start = i;
                    }
                    "[v4+ styles]" | "[v4 styles]" => {
                        if style_variant.is_some() {
                            return Err("Multiple style sections are ambiguous".into());
                        }
                        style_variant = Some(if next == "[v4+ styles]" {
                            Variant::Ass
                        } else {
                            Variant::Ssa
                        });
                    }
                    "[v4++ styles]" => {
                        return Err("ASS v4++ is not supported; source was not changed".into());
                    }
                    _ => {}
                }
                section = next;
                return Ok(());
            }
            if trimmed.is_empty() || trimmed.starts_with(';') {
                return Ok(());
            }
            let Some((key, value)) = body.split_once(':') else {
                if section == "[events]" && trimmed.to_ascii_lowercase().starts_with("dialogue") {
                    return Err("Malformed Dialogue record: missing colon".into());
                }
                return Ok(());
            };
            let name = key.trim();
            if section == "[events]"
                && trimmed
                    .split([' ', '\t', ':'])
                    .next()
                    .is_some_and(|n| n.eq_ignore_ascii_case("Dialogue"))
                && !name.eq_ignore_ascii_case("Dialogue")
            {
                return Err("Malformed Dialogue record: missing colon after event type".into());
            }
            match section.as_str() {
                "[script info]" => {
                    script_info.push((name.into(), value.trim().into()));
                    if name.eq_ignore_ascii_case("ScriptType") {
                        if variant.is_some() {
                            return Err("Duplicate ScriptType is ambiguous".into());
                        }
                        variant = Some(match value.trim().to_ascii_lowercase().as_str() {
                            "v4.00+" => Variant::Ass,
                            "v4.00" => Variant::Ssa,
                            _ => return Err("Only ASS v4.00+ and SSA v4.00 are supported".into()),
                        });
                    }
                }
                "[v4+ styles]" | "[v4 styles]" => {
                    if name.eq_ignore_ascii_case("Format") {
                        style_format = Some(fields(value, false)?);
                    } else if name.eq_ignore_ascii_case("Style") {
                        let format = style_format
                            .as_ref()
                            .ok_or("Style requires a preceding Format line")?;
                        let parts = values(value, format, false)?;
                        let name = lookup(format, &parts, "Name").unwrap_or_default().trim();
                        if name.is_empty() || !style_names.insert(name.to_owned()) {
                            return Err("Style names must be nonempty and unique".into());
                        }
                        styles.push(Style {
                            name: name.into(),
                            fields: format.iter().cloned().zip(parts).collect(),
                        });
                        line.style = Some(styles.len() - 1);
                    }
                }
                "[events]" => {
                    if name.eq_ignore_ascii_case("Format") {
                        let format = fields(value, true)?;
                        line.format = Some(format.clone());
                        event_format = Some(format);
                    } else if name.eq_ignore_ascii_case("Dialogue") {
                        let format = event_format
                            .as_ref()
                            .ok_or("Dialogue requires a preceding Format line")?;
                        let parts = values(value, format, true)?;
                        let get = |n| lookup(format, &parts, n).unwrap_or_default();
                        let id = CueId(cues.len() as u64 + 1);
                        let metadata = CueMetadata::new(
                            format
                                .iter()
                                .cloned()
                                .zip(parts.iter().cloned())
                                .filter(|(n, _)| !endpoint_or_text(n))
                                .collect(),
                        )?;
                        let cue = Cue::new(
                            id,
                            parse_time(get("Start"))?,
                            parse_time(get("End"))?,
                            get("Text").into(),
                        )?
                        .with_metadata(metadata);
                        originals.insert(
                            id,
                            Original {
                                cue: cue.clone(),
                                prefix: format!("{key}:"),
                                values: parts,
                                format: format.clone(),
                                ending: ending.into(),
                            },
                        );
                        cues.push(cue);
                        line.event = Some(id);
                    }
                }
                _ => {}
            }
            Ok(())
        })();
        result.map_err(|error| format!("ASS/SSA line {}: {error}", i + 1))?;
        lines.push(line);
    }
    let variant = variant.ok_or("ASS/SSA requires ScriptType in Script Info")?;
    if style_variant.is_some_and(|v| v != variant) {
        return Err("ScriptType and style section disagree".into());
    }
    if !seen_info || !seen_events {
        return Err("ASS/SSA requires Script Info and Events sections".into());
    }
    let format = event_format.ok_or("ASS/SSA requires an Events Format line")?;
    let events_end = events_end.unwrap_or(lines.len());
    let styles_end = style_variant.map(|_| styles_end.unwrap_or(lines.len()));
    Ok(Parsed {
        document: Document::new(cues)?,
        source: Source(
            Arc::new(SourceData {
                variant,
                lines,
                originals,
                styles,
                script_info,
                format,
                events_end,
                events_start,
                styles_end,
                newline,
            }),
            Arc::new(Authoring::default()),
        ),
    })
}

impl Source {
    pub fn variant(&self) -> Variant {
        self.0.variant
    }
    pub fn styles(&self) -> &[Style] {
        self.1.styles.as_deref().unwrap_or(&self.0.styles)
    }
    pub fn script_info(&self) -> &[(String, String)] {
        &self.0.script_info
    }
    /// Header for a packet renderer. Events are supplied separately with their
    /// own timestamps; retained comments/unknown data remain in saved source.
    /// This does not import or resolve external resources.
    pub fn preview_header(&self) -> String {
        let mut header = String::new();
        let mut events = false;
        for i in 0..=self.0.lines.len() {
            let mut ended = header.is_empty() || header.ends_with('\n');
            self.write_added_styles(i, &mut ended, &mut |part| header.push_str(part));
            let Some(line) = self.0.lines.get(i) else {
                break;
            };
            let trimmed = line.raw.trim_start_matches('\u{feff}').trim();
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                events = trimmed.eq_ignore_ascii_case("[Events]");
            }
            if !events {
                header.push_str(&self.style_line(line));
            }
        }
        header.push_str("\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n");
        header
    }
    pub fn default_metadata(&self) -> Result<CueMetadata> {
        let style = self
            .styles()
            .iter()
            .find(|s| s.name == "Default")
            .or_else(|| self.styles().first())
            .ok_or("Create requires an existing ASS/SSA style")?;
        CueMetadata::new(
            self.0
                .format
                .iter()
                .filter(|n| !endpoint_or_text(n))
                .map(|name| {
                    let value = match name.to_ascii_lowercase().as_str() {
                        "style" => style.name.clone(),
                        "layer" | "marginl" | "marginr" | "marginv" => "0".into(),
                        "marked" => "Marked=0".into(),
                        _ => String::new(),
                    };
                    (name.clone(), value)
                })
                .collect(),
        )
    }
    pub fn validate_style(&self, name: &str) -> Result<()> {
        if self.styles().iter().any(|s| s.name == name) {
            Ok(())
        } else {
            Err(format!("Style {name:?} is not defined in this document"))
        }
    }
}
fn replace_value(original: &str, value: &str) -> String {
    let left = original.len() - original.trim_start_matches([' ', '\t']).len();
    let right = original.trim_end_matches([' ', '\t']).len().max(left);
    format!("{}{value}{}", &original[..left], &original[right..])
}
fn event(cue: &Cue, original: Option<&Original>, format: &[String]) -> Result<String> {
    let metadata = cue
        .metadata()
        .ok_or("ASS/SSA cue is missing event metadata")?;
    if cue.text().contains('\n') {
        return Err("ASS/SSA cue text uses literal \\N hard breaks; convert editor newlines before committing".into());
    }
    let start = format_time(cue.start())?;
    let end = format_time(cue.end())?;
    if metadata
        .fields()
        .iter()
        .any(|(name, _)| !format.iter().any(|f| f.eq_ignore_ascii_case(name)))
    {
        return Err(
            "Event metadata cannot be represented by this insertion point's Format line".into(),
        );
    }
    let mut output = original.map_or_else(|| "Dialogue:".into(), |o| o.prefix.clone());
    for (i, name) in format.iter().enumerate() {
        if i > 0 {
            output.push(',');
        }
        let old = original.map(|o| o.values[i].as_str());
        let value = match name.to_ascii_lowercase().as_str() {
            "start" => {
                if original.is_some_and(|o| cue.start() == o.cue.start()) {
                    old.unwrap_or_default().into()
                } else {
                    replace_value(old.unwrap_or_default(), &start)
                }
            }
            "end" => {
                if original.is_some_and(|o| cue.end() == o.cue.end()) {
                    old.unwrap_or_default().into()
                } else {
                    replace_value(old.unwrap_or_default(), &end)
                }
            }
            "text" => cue.text().into(),
            _ => metadata
                .get(name)
                .ok_or_else(|| format!("Cue metadata is missing {name}"))?
                .to_owned(),
        };
        output.push_str(&value);
    }
    Ok(output)
}
fn write_document(document: &Document, source: &Source, mut write: impl FnMut(&str)) -> Result<()> {
    let data = &source.0;
    let mut previous = 0;
    for cue in document.cues() {
        if data.originals.contains_key(&cue.id()) {
            if cue.id().0 <= previous {
                return Err("ASS/SSA source events cannot be reordered implicitly".into());
            }
            previous = cue.id().0;
        }
    }
    let mut pending = document.cues().iter().peekable();
    let mut format: &[String] = &data.format;
    let mut ended = true;
    for i in 0..=data.lines.len() {
        source.write_added_styles(i, &mut ended, &mut write);
        let line = data.lines.get(i);
        if line.is_some_and(|l| l.event.is_some()) || i == data.events_end {
            while pending
                .peek()
                .is_some_and(|c| !data.originals.contains_key(&c.id()))
            {
                let cue = pending.next().expect("peeked cue");
                let text = event(cue, None, format)?;
                if !ended {
                    write(data.newline);
                }
                write(&text);
                write(data.newline);
                ended = true;
            }
        }
        let Some(line) = line else {
            break;
        };
        if let Some(next) = &line.format {
            format = next;
        }
        if let Some(id) = line.event {
            if pending.peek().is_none_or(|c| c.id() != id) {
                continue;
            }
            let cue = pending.next().expect("peeked original");
            let original = &data.originals[&id];
            if cue == &original.cue {
                write(&line.raw);
            } else {
                write(&event(cue, Some(original), &original.format)?);
                write(&original.ending);
            }
            ended = !original.ending.is_empty();
        } else {
            let raw = source.style_line(line);
            write(&raw);
            ended = raw.ends_with('\n');
        }
    }
    if pending.next().is_some() {
        return Err("Cannot place ASS/SSA events without changing source order".into());
    }
    Ok(())
}
pub fn serialize(document: &Document, source: &Source) -> Result<String> {
    let mut output = String::new();
    write_document(document, source, |part| output.push_str(part))?;
    Ok(output)
}
pub fn serialized_len(document: &Document, source: &Source) -> Result<usize> {
    let mut count = 0usize;
    let mut overflow = false;
    write_document(document, source, |part| {
        if let Some(next) = count.checked_add(part.len()) {
            count = next;
        } else {
            overflow = true;
        }
    })?;
    if overflow {
        Err("ASS/SSA size exceeds addressable memory".into())
    } else {
        Ok(count)
    }
}

#[cfg(test)]
mod tests;
