//! WebVTT text interpretation shared by source mapping and streaming metrics.
//! This is not a CSS renderer or a conformance checker. Unknown syntax stays
//! visible and makes automated changes require manual review.
use std::ops::Range;

pub(crate) enum Event {
    Visible(char, Range<usize>),
    Protected(Range<usize>),
}
#[derive(Default)]
pub(crate) struct Summary {
    pub ambiguous: Option<&'static str>,
    pub timed: bool,
    pub layout: bool,
}
const UNKNOWN: &str = "Unsupported or malformed WebVTT markup needs manual review.";
const UNBALANCED: &str = "Unbalanced WebVTT markup needs manual review.";

fn literal<E>(
    source: &str,
    at: usize,
    visit: &mut impl FnMut(Event) -> Result<(), E>,
) -> Result<(), E> {
    for (offset, c) in source.char_indices() {
        visit(Event::Visible(c, at + offset..at + offset + c.len_utf8()))?;
    }
    Ok(())
}

// Only terminated references are eligible for automated editing. The complete
// named-character table is supplied by pinned web_atoms (MIT OR Apache-2.0).
// Numeric references use HTML replacement values; no HTML parser is executed.
fn entity(source: &str) -> Option<(usize, [Option<char>; 2])> {
    let end = source.bytes().take(64).position(|b| b == b';')?;
    let name = &source[1..end];
    if let Some(number) = name.strip_prefix('#') {
        let (radix, number) = number
            .strip_prefix(['x', 'X'])
            .map_or((10, number), |hex| (16, hex));
        if number.is_empty()
            || !number.bytes().all(|b| match radix {
                16 => b.is_ascii_hexdigit(),
                _ => b.is_ascii_digit(),
            })
        {
            return None;
        }
        let value = u32::from_str_radix(number, radix).unwrap_or(u32::MAX);
        let c = match value {
            0 => '\u{fffd}',
            0x80..=0x9f => web_atoms::C1_REPLACEMENTS[value as usize - 0x80]
                .unwrap_or_else(|| char::from_u32(value).expect("C1 scalar")),
            _ => char::from_u32(value).unwrap_or('\u{fffd}'),
        };
        return Some((end + 1, [Some(c), None]));
    }
    let &(a, b) = web_atoms::NAMED_ENTITIES.get(&source[1..=end])?;
    Some((
        end + 1,
        [
            char::from_u32(a).filter(|c| *c != '\0'),
            char::from_u32(b).filter(|c| *c != '\0'),
        ],
    ))
}

fn tag(body: &str) -> Option<(&str, bool)> {
    if let Some(name) = body.strip_prefix('/') {
        return matches!(name, "b" | "i" | "u" | "c" | "v" | "lang" | "ruby" | "rt")
            .then_some((name, true));
    }
    let (head, annotation) = body.find([' ', '\t']).map_or((body, None), |at| {
        (&body[..at], Some(body[at..].trim_matches([' ', '\t'])))
    });
    let mut parts = head.split('.');
    let name = parts.next()?;
    if !matches!(name, "b" | "i" | "u" | "c" | "v" | "lang" | "ruby" | "rt")
        || parts.any(|class| class.is_empty() || class.contains(['\r', '\n', '&', '<', '>']))
        || body.contains(['\r', '\n', '<'])
    {
        return None;
    }
    if matches!(name, "v" | "lang") {
        if annotation.is_none_or(str::is_empty) {
            return None;
        }
    } else if annotation.is_some() {
        return None;
    }
    Some((name, false))
}

/// Stack depth is capped at 64. Long/unknown tags are emitted literally, keeping
/// memory bounded and allowing the caller to cancel while counting their text.
pub(crate) fn walk<E>(
    source: &str,
    mut visit: impl FnMut(Event) -> Result<(), E>,
) -> Result<Summary, E> {
    let mut summary = Summary::default();
    let mut stack = Vec::new();
    let mut at = 0;
    while at < source.len() {
        let rest = &source[at..];
        if rest.starts_with('<') {
            let Some(end) = rest.find('>') else {
                summary.ambiguous = Some(UNKNOWN);
                visit(Event::Protected(at..source.len()))?;
                literal(rest, at, &mut visit)?;
                break;
            };
            let token = &rest[..=end];
            let body = &rest[1..end];
            visit(Event::Protected(at..at + token.len()))?;
            if body.as_bytes().first().is_some_and(u8::is_ascii_digit) {
                summary.timed = true;
                summary.layout = true;
                if subtitle_formats::webvtt::parse_time(body).is_err() {
                    summary.ambiguous = Some(UNKNOWN);
                }
            } else if let Some((name, closing)) = (token.len() <= 4096).then(|| tag(body)).flatten()
            {
                if closing {
                    // The final rt end tag may be omitted before </ruby>.
                    if name == "ruby" && stack.last().is_some_and(|&(tag, _)| tag == "rt") {
                        stack.pop();
                    }
                    if stack.pop().is_none_or(|(tag, _)| tag != name) {
                        summary.ambiguous = Some(UNBALANCED);
                    }
                } else {
                    if stack.len() == 64 {
                        summary.ambiguous = Some(
                            "WebVTT nesting exceeds the 64-level text-tool limit; review manually.",
                        );
                        // This token is already protected; keep the rest bounded.
                        literal(token, at, &mut visit)?;
                        at += token.len();
                        visit(Event::Protected(at..source.len()))?;
                        literal(&source[at..], at, &mut visit)?;
                        break;
                    }
                    if name == "rt" && stack.last().is_none_or(|&(tag, _)| tag != "ruby") {
                        summary.ambiguous = Some(UNBALANCED);
                    }
                    summary.layout |= matches!(name, "ruby" | "rt");
                    stack.push((name, at));
                }
            } else {
                summary.ambiguous = Some(UNKNOWN);
                literal(token, at, &mut visit)?;
            }
            at += token.len();
            continue;
        }
        let annotation = stack.iter().any(|&(tag, _)| tag == "rt");
        if rest.starts_with('&') {
            if let Some((length, decoded)) = entity(rest) {
                visit(Event::Protected(at..at + length))?;
                if !annotation {
                    for c in decoded.into_iter().flatten() {
                        visit(Event::Visible(c, at..at + length))?;
                    }
                }
                at += length;
                continue;
            }
            summary.ambiguous = Some(UNKNOWN);
        }
        let c = rest.chars().next().expect("in bounds");
        let range = at..at + c.len_utf8();
        if annotation {
            visit(Event::Protected(range))?;
        } else {
            visit(Event::Visible(c, range))?;
        }
        at += c.len_utf8();
    }
    // A voice span covering the whole cue can omit its final closing tag.
    if !stack.is_empty() && stack != [("v", 0)] {
        summary.ambiguous = Some(UNBALANCED);
    }
    Ok(summary)
}
