//! Editable source mapping, deliberately separate from historical quality counting.
use std::ops::Range;
pub(crate) mod webvtt;

pub(crate) const MAX_CUE_BYTES: usize = 64 * 1024;

/// Timing changes cannot safely reinterpret cue-relative effects or unknown tags.
pub(crate) fn retiming_is_ambiguous(
    cue: &subtitle_core::Cue,
    format: subtitle_formats::Format,
) -> bool {
    let source = cue.text();
    if cue
        .metadata()
        .and_then(|m| m.get("Effect"))
        .is_some_and(|v| !v.trim().is_empty())
    {
        return true;
    }
    if source.len() > MAX_CUE_BYTES {
        return true;
    }
    let projection = Projection::for_format(source, format);
    projection.timed || projection.ambiguous.is_some()
}

#[derive(Debug)]
pub(super) struct Unit {
    pub visible: Range<usize>,
    pub source: Range<usize>,
}
pub(super) struct Projection {
    pub visible: String,
    pub units: Vec<Unit>,
    pub protected: Vec<Range<usize>>,
    pub ambiguous: Option<&'static str>,
    pub layout: bool,
    pub escapes: bool,
    pub timed: bool,
    pub ass_document: bool,
    pub format: subtitle_formats::Format,
    unbalanced: bool,
}

impl Projection {
    pub fn new(source: &str) -> Self {
        let mut result = Self::parse(source, true, false);
        if result.unbalanced {
            result = Self::parse(source, false, false);
            result.ambiguous = Some("Unbalanced HTML formatting needs manual review.");
        }
        result
    }
    pub fn for_ass_document(source: &str) -> Self {
        Self::parse(source, false, true)
    }
    pub fn for_format(source: &str, format: subtitle_formats::Format) -> Self {
        match format {
            subtitle_formats::Format::Srt => Self::new(source),
            subtitle_formats::Format::Ass | subtitle_formats::Format::Ssa => {
                Self::for_ass_document(source)
            }
            subtitle_formats::Format::WebVtt => Self::for_webvtt(source),
        }
    }
    pub fn for_webvtt(source: &str) -> Self {
        let mut result = Self {
            visible: String::new(),
            units: vec![],
            protected: vec![],
            ambiguous: None,
            layout: false,
            escapes: false,
            timed: false,
            ass_document: false,
            format: subtitle_formats::Format::WebVtt,
            unbalanced: false,
        };
        let summary = webvtt::walk(source, |event| -> Result<(), std::convert::Infallible> {
            match event {
                webvtt::Event::Visible(c, range) => result.push(c, range),
                webvtt::Event::Protected(range) => result.protected.push(range),
            }
            Ok(())
        });
        let summary = match summary {
            Ok(summary) => summary,
            Err(never) => match never {},
        };
        result.ambiguous = summary.ambiguous;
        result.layout = summary.layout;
        result.timed = summary.timed;
        result
    }
    fn parse(source: &str, hide_html: bool, ass_document: bool) -> Self {
        let mut result = Self {
            visible: String::new(),
            units: vec![],
            protected: vec![],
            ambiguous: None,
            layout: false,
            escapes: false,
            timed: false,
            ass_document,
            format: if ass_document {
                subtitle_formats::Format::Ass
            } else {
                subtitle_formats::Format::Srt
            },
            unbalanced: false,
        };
        let mut at = 0;
        let mut styles = vec![];
        // Match preview's cue-local ASS opt-in, ignoring escaped open braces.
        let mut ass = ass_document;
        while !ass_document && at < source.len() {
            let rest = &source[at..];
            if rest.starts_with("\\{") {
                at += 2;
            } else if rest.starts_with("{\\") {
                ass = rest.contains('}');
                break;
            } else {
                at += rest.chars().next().expect("in bounds").len_utf8();
            }
        }
        at = 0;
        let mut no_angle_closer = false;
        while at < source.len() {
            let rest = &source[at..];
            if rest.starts_with("{\\") || (ass_document && rest.starts_with('{')) {
                if let Some(end) = rest.find('}') {
                    let block = &rest[..=end];
                    result.inspect_override(block);
                    result.protected.push(at..at + end + 1);
                    at += end + 1;
                    continue;
                }
                result.ambiguous = Some("An incomplete ASS override needs manual review.");
                for (offset, c) in rest.char_indices() {
                    result.push(c, at + offset..at + offset + c.len_utf8());
                }
                break;
            }
            if hide_html && let Some((length, style, closing)) = html(rest) {
                if closing {
                    if styles.pop() != Some(style) {
                        result.ambiguous = Some("Unbalanced HTML formatting needs manual review.");
                        result.unbalanced = true;
                    }
                } else {
                    styles.push(style);
                }
                result.protected.push(at..at + length);
                at += length;
                continue;
            }
            // Unknown HTML is visible in preview, but edits must not alter its syntax.
            if !ass_document && rest.starts_with('<') && !no_angle_closer {
                if let Some(end) = rest.find('>') {
                    result.ambiguous = Some("Unsupported HTML syntax needs manual review.");
                    result.protected.push(at..at + end + 1);
                    for (offset, c) in rest[..=end].char_indices() {
                        result.push(c, at + offset..at + offset + c.len_utf8());
                    }
                    at += end + 1;
                    continue;
                } else {
                    no_angle_closer = true;
                }
            }
            let c = rest.chars().next().expect("in bounds");
            let mut length = c.len_utf8();
            let mut visible = c;
            if c == '\\'
                && let Some(next) = rest.chars().nth(1)
            {
                // Escaped braces must not become accidental override openers,
                // including in otherwise ordinary SRT text.
                if next == '{' || (ass && matches!(next, 'N' | 'n' | 'h' | '}')) {
                    length += next.len_utf8();
                    if ass {
                        visible = match next {
                            'N' | 'n' => '\n',
                            'h' => '\u{a0}',
                            _ => next,
                        };
                        result.escapes = true;
                    } else {
                        result.push(c, at..at + 1);
                        visible = next;
                    }
                    result.protected.push(at..at + length);
                }
            }
            result.push(visible, at..at + length);
            at += length;
        }
        if !styles.is_empty() {
            result.ambiguous = Some("Unbalanced HTML formatting needs manual review.");
            result.unbalanced = true;
        }
        result
    }
    fn push(&mut self, c: char, source: Range<usize>) {
        let start = self.visible.len();
        self.visible.push(c);
        self.units.push(Unit {
            visible: start..self.visible.len(),
            source,
        });
    }
    fn inspect_override(&mut self, block: &str) {
        for command in block.split('\\').skip(1) {
            let name = command
                .trim_start_matches(['1', '2', '3', '4'])
                .chars()
                .take_while(|c| c.is_ascii_alphabetic())
                .collect::<String>();
            self.timed |= matches!(
                name.as_str(),
                "t" | "k" | "K" | "kf" | "ko" | "kt" | "move" | "fad" | "fade"
            );
            match name.as_str() {
                "b" | "i" | "u" | "s" | "fs" | "fscx" | "fscy" | "fsp" | "c" | "a" | "alpha"
                | "bord" | "xbord" | "ybord" | "shad" | "xshad" | "yshad" | "blur" | "be" | "r" => {
                }
                // Font and named reset arguments have no delimiter after the tag name.
                _ if command.starts_with("fn") || command.starts_with('r') => {}
                "an" | "pos" | "move" | "org" | "clip" | "iclip" | "frx" | "fry" | "frz" | "fr"
                | "fax" | "fay" | "fad" | "fade" | "q" => {
                    self.layout = true;
                }
                _ => {
                    self.ambiguous = Some(
                        "Karaoke, drawings, transforms or unknown overrides need manual review.",
                    )
                }
            }
            // SSA alignment is layout, even though \1a is alpha.
            if command.starts_with('a') && name == "a" {
                self.layout = true;
            }
        }
    }
    pub fn source_range(&self, range: Range<usize>) -> Range<usize> {
        let first = self.units.partition_point(|u| u.visible.end <= range.start);
        let last = self.units.partition_point(|u| u.visible.start < range.end);
        self.units[first].source.start..self.units[last - 1].source.end
    }
    pub fn safe_range(&self, source: &str, range: Range<usize>) -> bool {
        let next = self.protected.partition_point(|p| p.end <= range.start);
        source.is_char_boundary(range.start)
            && source.is_char_boundary(range.end)
            && self
                .protected
                .get(next)
                .is_none_or(|p| p.start >= range.end)
    }
}
pub(super) fn html(rest: &str) -> Option<(usize, char, bool)> {
    let bytes = rest.as_bytes();
    if bytes.first() != Some(&b'<') {
        return None;
    }
    let closing = bytes.get(1) == Some(&b'/');
    let at = if closing { 2 } else { 1 };
    let style = bytes.get(at)?.to_ascii_lowercase();
    (b"biu".contains(&style) && bytes.get(at + 1) == Some(&b'>')).then_some((
        at + 2,
        char::from(style),
        closing,
    ))
}
