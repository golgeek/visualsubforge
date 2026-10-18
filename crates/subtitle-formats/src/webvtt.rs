//! Standalone UTF-8 WebVTT source preservation. No IO, CSS execution or renderer.
//! Accepted cue metadata owns identifiers/settings; source ranges retain everything
//! else. Cloning a source for history shares its original bytes and index.
use std::{
    collections::{HashMap, HashSet},
    ops::Range,
    sync::Arc,
};
use subtitle_core::{Cue, CueId, CueMetadata, Document, Milliseconds, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockKind {
    Note,
    Style,
    Region,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source(Arc<SourceData>);
#[derive(Debug, PartialEq, Eq)]
struct SourceData {
    raw: String,
    header_end: usize,
    newline: &'static str,
    blocks: Vec<Block>,
    originals: HashMap<CueId, usize>,
}
#[derive(Debug, PartialEq, Eq)]
struct Block {
    raw: Range<usize>,
    content: Content,
}
#[derive(Debug, PartialEq, Eq)]
enum Content {
    Opaque(BlockKind),
    Cue(Original),
}
#[derive(Debug, PartialEq, Eq)]
struct Original {
    id: CueId,
    start: Milliseconds,
    end: Milliseconds,
    identifier: Option<Range<usize>>,
    timing: Line,
    start_token: Range<usize>,
    end_token: Range<usize>,
    settings: Range<usize>,
    payload: Range<usize>,
}
#[derive(Debug, PartialEq, Eq)]
pub struct Parsed {
    pub document: Document,
    pub source: Source,
}

// A streaming line iterator accepts every WebVTT line terminator without a
// per-line allocation or a whole-file normalization copy, including mixed endings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Line {
    start: usize,
    body_end: usize,
    end: usize,
}
impl Line {
    fn body(self, input: &str) -> &str {
        &input[self.start..self.body_end]
    }
}
struct Lines<'a> {
    input: &'a str,
    at: usize,
}
impl<'a> Lines<'a> {
    fn new(input: &'a str) -> Self {
        Self { input, at: 0 }
    }
}
impl Iterator for Lines<'_> {
    type Item = Line;
    fn next(&mut self) -> Option<Line> {
        if self.at == self.input.len() {
            return None;
        }
        let start = self.at;
        let body_end = self.input[start..]
            .find(['\r', '\n'])
            .map_or(self.input.len(), |offset| start + offset);
        let ending = if self.input[body_end..].starts_with("\r\n") {
            2
        } else {
            usize::from(body_end < self.input.len())
        };
        self.at = body_end + ending;
        Some(Line {
            start,
            body_end,
            end: self.at,
        })
    }
}

pub fn parse_time(value: &str) -> Result<Milliseconds> {
    let bad = || "Invalid WebVTT timestamp; expected MM:SS.mmm or HH:MM:SS.mmm".to_owned();
    let (clock, fraction) = value.split_once('.').ok_or_else(bad)?;
    let mut fields = clock.split(':');
    let first = fields.next().ok_or_else(bad)?;
    let second = fields.next().ok_or_else(bad)?;
    let third = fields.next();
    if fields.next().is_some()
        || fraction.len() != 3
        || !fraction.bytes().all(|c| c.is_ascii_digit())
        || [Some(first), Some(second), third]
            .into_iter()
            .flatten()
            .any(|v| v.is_empty() || !v.bytes().all(|c| c.is_ascii_digit()))
    {
        return Err(bad());
    }
    let (hours, minutes, seconds) = if let Some(third) = third {
        if first.len() < 2 {
            return Err(bad());
        }
        (first.parse::<u64>().map_err(|_| bad())?, second, third)
    } else {
        (0, first, second)
    };
    if minutes.len() != 2 || seconds.len() != 2 {
        return Err(bad());
    }
    let minutes: u64 = minutes.parse().map_err(|_| bad())?;
    let seconds: u64 = seconds.parse().map_err(|_| bad())?;
    let fraction: u64 = fraction.parse().map_err(|_| bad())?;
    if minutes >= 60 || seconds >= 60 {
        return Err(bad());
    }
    let milliseconds = hours
        .checked_mul(3_600_000)
        .and_then(|n| n.checked_add(minutes * 60_000 + seconds * 1000 + fraction))
        .ok_or_else(bad)?;
    Milliseconds::new(milliseconds)
}
pub fn format_time(time: Milliseconds) -> String {
    let t = time.get();
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        t / 3_600_000,
        t / 60_000 % 60,
        t / 1000 % 60,
        t % 1000
    )
}
fn horizontal(c: u8) -> bool {
    matches!(c, b' ' | b'\t')
}
fn note(value: &str) -> bool {
    value == "NOTE"
        || value
            .strip_prefix("NOTE")
            .is_some_and(|s| s.starts_with([' ', '\t']))
}
fn block_kind(value: &str) -> Option<BlockKind> {
    if note(value) {
        Some(BlockKind::Note)
    } else {
        match value.trim_end_matches([' ', '\t']) {
            "STYLE" => Some(BlockKind::Style),
            "REGION" => Some(BlockKind::Region),
            _ => None,
        }
    }
}

/// Raw settings are preserved, not interpreted as CSS or claimed to conform to
/// a particular renderer's supported values. Boundary injection is never allowed.
pub fn cue_metadata(identifier: String, settings: String) -> Result<CueMetadata> {
    if identifier.contains("-->") || (!identifier.is_empty() && block_kind(&identifier).is_some()) {
        return Err(
            "WebVTT cue identifier must not contain --> or use NOTE/STYLE/REGION block syntax"
                .into(),
        );
    }
    if settings.contains("-->") || settings.trim_matches([' ', '\t']) != settings {
        return Err(
            "WebVTT cue settings must not contain --> or leading/trailing whitespace".into(),
        );
    }
    CueMetadata::new(vec![
        ("Identifier".into(), identifier),
        ("Settings".into(), settings),
    ])
}
pub fn default_metadata() -> Result<CueMetadata> {
    cue_metadata(String::new(), String::new())
}
pub struct Metadata<'a> {
    pub identifier: &'a str,
    pub settings: &'a str,
}
pub fn metadata(cue: &Cue) -> Result<Metadata<'_>> {
    let fields = cue
        .metadata()
        .ok_or("WebVTT cue is missing identifier/settings metadata")?;
    if fields.fields().len() != 2 {
        return Err(
            "WebVTT cannot preserve another format's cue metadata; explicit conversion is required"
                .into(),
        );
    }
    let identifier = fields
        .get("Identifier")
        .ok_or("WebVTT cue is missing Identifier")?;
    let settings = fields
        .get("Settings")
        .ok_or("WebVTT cue is missing Settings")?;
    // Do not allocate a second metadata value during size validation.
    if identifier.contains("-->") || (!identifier.is_empty() && block_kind(identifier).is_some()) {
        return Err("WebVTT identifier conflicts with a block boundary".into());
    }
    if settings.contains("-->") || settings.trim_matches([' ', '\t']) != settings {
        return Err("WebVTT settings have invalid boundaries or outer whitespace".into());
    }
    Ok(Metadata {
        identifier,
        settings,
    })
}

pub fn validate_payload(text: &str) -> Result<()> {
    if text.contains(['\r', '\0']) || text.contains("-->") {
        return Err(
            "WebVTT cue text cannot contain CR, NUL or -->; escape literal arrows as --&gt;".into(),
        );
    }
    if !text.is_empty() && text.split('\n').any(str::is_empty) {
        return Err("WebVTT cue text cannot contain empty lines; use a separate cue".into());
    }
    Ok(())
}
fn validate_timestamps(cue: &Cue) -> Result<()> {
    let mut rest = cue.text();
    let mut previous = cue.start();
    while let Some(open) = rest.find('<') {
        rest = &rest[open + 1..];
        let Some(close) = rest.find('>') else {
            break;
        };
        let token = &rest[..close];
        if token.as_bytes().first().is_some_and(u8::is_ascii_digit) {
            let time = parse_time(token)?;
            if time <= previous || time >= cue.end() {
                return Err("WebVTT internal timestamps must increase strictly inside the cue interval; edit the markers together with its timing".into());
            }
            previous = time;
        }
        rest = &rest[close + 1..];
    }
    Ok(())
}
struct Timing {
    start: Milliseconds,
    end: Milliseconds,
    start_token: Range<usize>,
    end_token: Range<usize>,
    settings: Range<usize>,
}
fn timing(input: &str, line: Line) -> Result<Timing> {
    let body = line.body(input);
    let bytes = body.as_bytes();
    let mut at = 0;
    while bytes.get(at).is_some_and(|c| horizontal(*c)) {
        at += 1;
    }
    let start = at;
    while bytes.get(at).is_some_and(|c| !horizontal(*c)) {
        at += 1;
    }
    let start_end = at;
    if start_end == start {
        return Err("Missing WebVTT cue start timestamp".into());
    }
    while bytes.get(at).is_some_and(|c| horizontal(*c)) {
        at += 1;
    }
    if at == start_end || !body[at..].starts_with("-->") {
        return Err(
            "WebVTT timing needs start --> end with spaces or tabs around the arrow".into(),
        );
    }
    at += 3;
    if !bytes.get(at).is_some_and(|c| horizontal(*c)) {
        return Err("WebVTT timing needs whitespace after -->".into());
    }
    while bytes.get(at).is_some_and(|c| horizontal(*c)) {
        at += 1;
    }
    let end = at;
    while bytes.get(at).is_some_and(|c| !horizontal(*c)) {
        at += 1;
    }
    let end_end = at;
    let end_time = parse_time(&body[end..end_end])?;
    while bytes.get(at).is_some_and(|c| horizontal(*c)) {
        at += 1;
    }
    let settings_end = body.trim_end_matches([' ', '\t']).len().max(at);
    Ok(Timing {
        start: parse_time(&body[start..start_end])?,
        end: end_time,
        start_token: line.start + start..line.start + start_end,
        end_token: line.start + end..line.start + end_end,
        settings: line.start + at..line.start + settings_end,
    })
}

pub fn parse(input: &str) -> Result<Parsed> {
    if input.contains('\0') {
        return Err("WebVTT contains NUL characters; UTF-8 text is required".into());
    }
    let mut lines = Lines::new(input).enumerate().peekable();
    let (_, first) = lines.next().ok_or("WebVTT requires a WEBVTT signature")?;
    let signature = first
        .body(input)
        .strip_prefix('\u{feff}')
        .unwrap_or(first.body(input));
    if !signature
        .strip_prefix("WEBVTT")
        .is_some_and(|s| s.is_empty() || s.starts_with([' ', '\t']))
    {
        return Err("WebVTT requires WEBVTT at the beginning of the file".into());
    }
    let newline = match &input[first.body_end..first.end] {
        "\r\n" => "\r\n",
        "\r" => "\r",
        _ => "\n",
    };
    if let Some((_, next)) = lines.peek()
        && !next.body(input).is_empty()
    {
        if next.body(input).starts_with("X-TIMESTAMP-MAP") {
            return Err("Segmented WebVTT X-TIMESTAMP-MAP is unsupported; export a standalone WebVTT file before editing".into());
        }
        return Err(
            "WebVTT header must be followed by an empty line before cue or metadata blocks".into(),
        );
    }
    while lines.peek().is_some_and(|(_, l)| l.body(input).is_empty()) {
        lines.next();
    }
    let header_end = lines.peek().map_or(input.len(), |(_, l)| l.start);
    let mut blocks = Vec::new();
    let mut originals = HashMap::new();
    let mut cues: Vec<Cue> = Vec::new();
    let mut identifiers = HashSet::new();
    while let Some((line_number, first)) = lines.next() {
        let parsed: Result<Content> = (|| {
            if let Some(kind) = block_kind(first.body(input)) {
                if kind != BlockKind::Note && !cues.is_empty() {
                    return Err("WebVTT STYLE/REGION blocks must precede the first cue".into());
                }
                if first.body(input).contains("-->") {
                    return Err("WebVTT metadata blocks cannot contain -->".into());
                }
                while lines.peek().is_some_and(|(_, l)| !l.body(input).is_empty()) {
                    let (_, line) = lines.next().expect("peeked line");
                    if line.body(input).contains("-->") {
                        return Err("WebVTT metadata blocks cannot contain -->".into());
                    }
                }
                return Ok(Content::Opaque(kind));
            }
            let (identifier, timing_line) = if first.body(input).contains("-->") {
                (None, first)
            } else {
                let (_, time) = lines
                    .next()
                    .ok_or("WebVTT cue identifier needs a following timing line")?;
                (Some(first.start..first.body_end), time)
            };
            let t = timing(input, timing_line)?;
            if cues.last().is_some_and(|cue| cue.start() > t.start) {
                return Err(
                    "WebVTT cues must be ordered by start time; source order was not changed"
                        .into(),
                );
            }
            let identifier_text = identifier
                .as_ref()
                .map_or("", |range| &input[range.clone()]);
            if !identifier_text.is_empty() && !identifiers.insert(identifier_text) {
                return Err(
                    "Duplicate WebVTT cue identifier; give each named cue a unique identifier"
                        .into(),
                );
            }
            let mut text = String::new();
            let mut payload_end = timing_line.end;
            let mut count = 0;
            while lines.peek().is_some_and(|(_, l)| !l.body(input).is_empty()) {
                let (_, line) = lines.next().expect("peeked payload");
                if count > 0 {
                    text.push('\n');
                }
                text.push_str(line.body(input));
                payload_end = line.body_end;
                count += 1;
            }
            let id = CueId(cues.len() as u64 + 1);
            let cue = Cue::new(id, t.start, t.end, text)?.with_metadata(cue_metadata(
                identifier_text.into(),
                input[t.settings.clone()].into(),
            )?);
            validate_payload(cue.text())?;
            validate_timestamps(&cue)?;
            cues.push(cue);
            originals.insert(id, blocks.len());
            Ok(Content::Cue(Original {
                id,
                start: t.start,
                end: t.end,
                identifier,
                timing: timing_line,
                start_token: t.start_token,
                end_token: t.end_token,
                settings: t.settings,
                payload: timing_line.end..payload_end,
            }))
        })();
        let content = parsed.map_err(|e| format!("WebVTT line {}: {e}", line_number + 1))?;
        while lines.peek().is_some_and(|(_, l)| l.body(input).is_empty()) {
            lines.next();
        }
        let end = lines.peek().map_or(input.len(), |(_, line)| line.start);
        blocks.push(Block {
            raw: first.start..end,
            content,
        });
    }
    Ok(Parsed {
        document: Document::new(cues)?,
        source: Source(Arc::new(SourceData {
            raw: input.into(),
            header_end,
            newline,
            blocks,
            originals,
        })),
    })
}
impl Source {
    pub fn header(&self) -> &str {
        &self.0.raw[..self.0.header_end]
    }
    pub fn blocks(&self, kind: BlockKind) -> impl Iterator<Item = &str> {
        self.0
            .blocks
            .iter()
            .filter(move |block| matches!(block.content, Content::Opaque(value) if value == kind))
            .map(|block| &self.0.raw[block.raw.clone()])
    }
}

struct Writer<F> {
    sink: F,
    trailing_lines: u8,
    last_cr: bool,
}
impl<F: FnMut(&str)> Writer<F> {
    fn write(&mut self, value: &str) {
        for byte in value.bytes() {
            match byte {
                b'\r' => self.trailing_lines = (self.trailing_lines + 1).min(2),
                b'\n' if self.last_cr => {}
                b'\n' => self.trailing_lines = (self.trailing_lines + 1).min(2),
                _ => self.trailing_lines = 0,
            }
            self.last_cr = byte == b'\r';
        }
        (self.sink)(value);
    }
    fn separate(&mut self, newline: &str) {
        while self.trailing_lines < 2 {
            self.write(newline);
        }
    }
    fn payload(&mut self, text: &str, newline: &str) {
        for (i, line) in text.split('\n').enumerate() {
            if i > 0 {
                self.write(newline);
            }
            self.write(line);
        }
    }
}
fn same_payload(raw: &str, normalized: &str) -> bool {
    (raw.is_empty() && normalized.is_empty())
        || Lines::new(raw)
            .map(|line| line.body(raw))
            .eq(normalized.split('\n'))
}
fn write_cue(
    cue: &Cue,
    original: Option<&Original>,
    source: &SourceData,
    out: &mut Writer<impl FnMut(&str)>,
) -> Result<()> {
    let m = metadata(cue)?;
    if let Some(old) = original {
        let old_id = old
            .identifier
            .as_ref()
            .map_or("", |range| &source.raw[range.clone()]);
        if m.identifier == old_id {
            if let Some(range) = &old.identifier {
                out.write(&source.raw[range.start..old.timing.start]);
            }
        } else if !m.identifier.is_empty() {
            out.write(m.identifier);
            out.write(source.newline);
        }
        out.write(&source.raw[old.timing.start..old.start_token.start]);
        if cue.start() == old.start {
            out.write(&source.raw[old.start_token.clone()]);
        } else {
            out.write(&format_time(cue.start()));
        }
        out.write(&source.raw[old.start_token.end..old.end_token.start]);
        if cue.end() == old.end {
            out.write(&source.raw[old.end_token.clone()]);
        } else {
            out.write(&format_time(cue.end()));
        }
        if m.settings == &source.raw[old.settings.clone()] {
            out.write(&source.raw[old.end_token.end..old.timing.body_end]);
        } else if !m.settings.is_empty() {
            out.write(" ");
            out.write(m.settings);
        }
        out.write(&source.raw[old.timing.body_end..old.timing.end]);
        if old.timing.body_end == old.timing.end && !cue.text().is_empty() {
            out.write(source.newline);
        }
        let raw = &source.raw[old.payload.clone()];
        if same_payload(raw, cue.text()) {
            out.write(raw);
        } else {
            out.payload(cue.text(), source.newline);
        }
    } else {
        if !m.identifier.is_empty() {
            out.write(m.identifier);
            out.write(source.newline);
        }
        out.write(&format_time(cue.start()));
        out.write(" --> ");
        out.write(&format_time(cue.end()));
        if !m.settings.is_empty() {
            out.write(" ");
            out.write(m.settings);
        }
        out.write(source.newline);
        out.payload(cue.text(), source.newline);
        out.separate(source.newline);
    }
    Ok(())
}
fn write_document(document: &Document, source: &Source, sink: impl FnMut(&str)) -> Result<()> {
    let data = &source.0;
    let mut identifiers = HashSet::new();
    let mut previous_time = None;
    let mut previous_original = None;
    for cue in document.cues() {
        let m = metadata(cue)?;
        validate_payload(cue.text())?;
        validate_timestamps(cue)?;
        if !m.identifier.is_empty() && !identifiers.insert(m.identifier) {
            return Err(
                "WebVTT cue identifiers must remain unique; the edit was not accepted".into(),
            );
        }
        if previous_time.is_some_and(|time| time > cue.start()) {
            return Err(
                "WebVTT cues must remain ordered by start time; the edit was not accepted".into(),
            );
        }
        previous_time = Some(cue.start());
        if let Some(&index) = data.originals.get(&cue.id()) {
            if previous_original.is_some_and(|previous| previous >= index) {
                return Err("WebVTT source cues cannot be reordered implicitly".into());
            }
            previous_original = Some(index);
        }
    }
    let mut out = Writer {
        sink,
        trailing_lines: 0,
        last_cr: false,
    };
    out.write(source.header());
    let mut pending = document.cues().iter().peekable();
    for block in &data.blocks {
        if let Content::Cue(old) = &block.content {
            while pending
                .peek()
                .is_some_and(|cue| !data.originals.contains_key(&cue.id()))
            {
                let cue = pending.next().expect("peeked new cue");
                out.separate(data.newline);
                write_cue(cue, None, data, &mut out)?;
            }
            if pending.peek().is_none_or(|cue| cue.id() != old.id) {
                continue;
            }
            let cue = pending.next().expect("peeked original cue");
            out.separate(data.newline);
            write_cue(cue, Some(old), data, &mut out)?;
            out.write(&data.raw[old.payload.end..block.raw.end]);
        } else {
            out.separate(data.newline);
            out.write(&data.raw[block.raw.clone()]);
        }
    }
    for cue in pending {
        if data.originals.contains_key(&cue.id()) {
            return Err("Cannot place WebVTT cues without changing source order".into());
        }
        out.separate(data.newline);
        write_cue(cue, None, data, &mut out)?;
    }
    Ok(())
}
pub fn serialize(document: &Document, source: &Source) -> Result<String> {
    let mut output = String::new();
    write_document(document, source, |part| output.push_str(part))?;
    Ok(output)
}
pub fn serialized_len(document: &Document, source: &Source) -> Result<usize> {
    let mut count = Some(0usize);
    write_document(document, source, |part| {
        count = count.and_then(|n| n.checked_add(part.len()));
    })?;
    count.ok_or("WebVTT output exceeds addressable memory".into())
}

#[cfg(test)]
mod tests;
