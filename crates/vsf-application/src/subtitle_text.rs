//! Shared text encoding for native presentation and deliberate format conversion.
//! This does not change accepted text or imply that all players support ASS in SRT.

pub fn has_markup(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut at = 0;
    while at + 1 < bytes.len() {
        if bytes[at..].starts_with(b"\\{") {
            at += 2;
        } else if bytes[at..].starts_with(b"{\\") {
            return text[at + 2..].contains('}');
        } else {
            at += 1;
        }
    }
    false
}

pub fn srt_to_ass(text: &str) -> String {
    let (result, balanced) = convert(text, true);
    if balanced {
        result
    } else {
        convert(text, false).0
    }
}

fn convert(text: &str, html: bool) -> (String, bool) {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    let mut styles = Vec::new();
    let mut balanced = true;
    let mut counts = [0usize; 3];
    // Preserve literal Windows paths/backslashes in ordinary SRT cues, even
    // when another cue selects this transport. A complete override block opts
    // this cue into ASS text escapes; {\r} is an explicit neutral opt-in.
    let ass_escapes = has_markup(text);
    while !rest.is_empty() {
        if rest.starts_with("{\\") {
            let Some(end) = rest.find('}') else {
                // Preserve an incomplete block literally, including its escapes.
                // Repeated unmatched openers require only this one scan.
                for c in rest.chars() {
                    match c {
                        '{' | '}' => {
                            out.push('\\');
                            out.push(c);
                        }
                        '\\' => out.push_str("\\\u{2060}"),
                        '\n' => out.push_str("\\N"),
                        '\r' => {}
                        _ => out.push(c),
                    }
                }
                break;
            };
            // Keep all override syntax for libass, including SSA alignment,
            // transforms, karaoke and drawing. Do not interpret HTML inside it.
            out.push_str(&rest[..=end]);
            rest = &rest[end + 1..];
            continue;
        }
        if html && rest.starts_with('<') {
            // Only the three supported HTML forms need recognition. Bounded
            // prefix checks avoid rescanning long unknown/unclosed angle tags.
            let tag = rest.get(..3).filter(|s| s.ends_with('>')).or_else(|| {
                rest.get(..4)
                    .filter(|s| s.starts_with("</") && s.ends_with('>'))
            });
            if let Some(tag) = tag {
                let tag = tag.to_ascii_lowercase();
                let closing = tag.starts_with("</");
                let style = tag.as_bytes()[if closing { 2 } else { 1 }];
                if let Some(kind) = b"biu".iter().position(|&s| s == style) {
                    if closing {
                        let popped = styles.pop();
                        balanced &= popped == Some(kind);
                        if let Some(popped) = popped {
                            counts[popped] -= 1;
                        }
                        if counts[kind] == 0 {
                            out.push_str(&format!("{{\\{}0}}", style as char));
                        }
                    } else {
                        if counts[kind] == 0 {
                            out.push_str(&format!("{{\\{}1}}", style as char));
                        }
                        counts[kind] += 1;
                        styles.push(kind);
                    }
                    rest = &rest[tag.len()..];
                    continue;
                }
            }
        }
        // Recognized ASS text escapes are intentional in the expanded workflow.
        // Braces in ordinary prose and incomplete override blocks stay literal.
        let mut chars = rest.chars();
        let c = chars.next().expect("nonempty valid UTF-8");
        rest = chars.as_str();
        match c {
            '\r' => {
                rest = rest.strip_prefix('\n').unwrap_or(rest);
                out.push_str("\\N");
            }
            '\n' => out.push_str("\\N"),
            '{' | '}' => {
                out.push('\\');
                out.push(c);
            }
            '\\' => {
                out.push('\\');
                if ass_escapes
                    && let Some(next) = rest.chars().next()
                    && matches!(next, 'N' | 'n' | 'h' | '{' | '}')
                {
                    out.push(next);
                    rest = &rest[next.len_utf8()..];
                } else {
                    // Break accidental escape formation when another source
                    // character is rewritten. A word joiner is invisible.
                    out.push('\u{2060}');
                }
            }
            _ => out.push(c),
        }
    }
    (out, balanced && styles.is_empty())
}
