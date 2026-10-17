//! Presentation-only ASS packets. Accepted text and SRT persistence are untouched.
//! A minimal Matroska subtitle stream carries integer millisecond timestamps;
//! serializing an ASS file would round them to centiseconds. mpv's public hex://
//! memory stream copies the bytes and owns their lifetime with the subtitle track.
use subtitle_core::Document;

const MAX_BYTES: usize = 4 * 1024 * 1024;
// SRT has no style sheet or script resolution. These are explicit preview defaults,
// matching the conventional 384x288 ASS canvas. Inline overrides take precedence.
const HEADER: &str = "[Script Info]\nScriptType: v4.00+\nPlayResX: 384\nPlayResY: 288\nWrapStyle: 0\nScaledBorderAndShadow: yes\nYCbCr Matrix: None\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Default,sans-serif,20,&H00FFFFFF,&H0000FFFF,&H00000000,&H80000000,0,0,0,0,100,100,0,0,1,1,0,2,10,10,10,1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n";

pub(super) fn has_markup(text: &str) -> bool {
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

/// The bounded source URL is the only object handed to the existing command
/// boundary. No disk files, custom native callbacks or background resources.
pub(super) fn serialize(document: &Document) -> Result<Option<String>, String> {
    let mut cues: Vec<_> = document
        .cues()
        .iter()
        .enumerate()
        .filter(|(_, cue)| !cue.text().trim().is_empty())
        .collect();
    if cues.is_empty() {
        return Ok(None);
    }
    if cues.iter().map(|(_, cue)| cue.text().len()).sum::<usize>() > MAX_BYTES / 2 {
        return Err(limit());
    }
    cues.sort_by_key(|(index, cue)| (cue.start().get(), *index));
    let mut ebml = Vec::new();
    for (id, value) in [(0x4286, 1), (0x42f7, 1), (0x42f2, 4), (0x42f3, 8)] {
        uint(id, value, &mut ebml);
    }
    element(0x4282, b"matroska", &mut ebml);
    uint(0x4287, 4, &mut ebml);
    uint(0x4285, 2, &mut ebml);
    let mut output = Vec::new();
    element(0x1a45dfa3, &ebml, &mut output);

    let mut segment = Vec::new();
    let mut info = Vec::new();
    uint(0x2ad7b1, 1_000_000, &mut info); // one timestamp tick = one millisecond
    element(0x4d80, b"VisualSubForge", &mut info);
    element(0x5741, b"VisualSubForge", &mut info);
    let duration = cues.iter().map(|(_, c)| c.end().get()).max().unwrap_or(0) as f64;
    element(0x4489, &duration.to_be_bytes(), &mut info);
    element(0x1549a966, &info, &mut segment);

    let mut track = Vec::new();
    for (id, value) in [(0xd7, 1), (0x73c5, 1), (0x83, 17), (0x9c, 0)] {
        uint(id, value, &mut track);
    }
    element(0x86, b"S_TEXT/ASS", &mut track);
    // Preserve the established Mac preview font when Default is authoritative.
    // Explicit font overrides still win.
    let header = if cfg!(target_os = "macos") {
        HEADER.replace("Default,sans-serif,", "Default,Hiragino Sans,")
    } else {
        HEADER.to_owned()
    };
    element(0x63a2, header.as_bytes(), &mut track);
    let mut tracks = Vec::new();
    element(0xae, &track, &mut tracks);
    element(0x1654ae6b, &tracks, &mut segment);

    for (index, cue) in cues {
        let mut block = vec![0x81, 0, 0, 0]; // track 1, relative timestamp 0, no lacing
        block.extend_from_slice(format!("{index},0,Default,,0,0,0,,").as_bytes());
        block.extend_from_slice(preview_text(cue.text()).as_bytes());
        let mut group = Vec::new();
        element(0xa1, &block, &mut group);
        uint(0x9b, cue.end().get() - cue.start().get(), &mut group);
        let mut cluster = Vec::new();
        uint(0xe7, cue.start().get(), &mut cluster);
        element(0xa0, &group, &mut cluster);
        element(0x1f43b675, &cluster, &mut segment);
        // Account for the hex expansion and small EBML/segment wrappers before
        // constructing the URL. Input and intermediate allocations are bounded.
        if segment.len() * 2 + 256 > MAX_BYTES {
            return Err(limit());
        }
    }
    element(0x18538067, &segment, &mut output);
    let mut url = String::with_capacity(6 + output.len() * 2);
    url.push_str("hex://");
    const HEX: &[u8] = b"0123456789abcdef";
    for byte in output {
        url.push(HEX[(byte >> 4) as usize] as char);
        url.push(HEX[(byte & 15) as usize] as char);
    }
    Ok(Some(url))
}

fn limit() -> String {
    "Document exceeds the 4 MiB native preview transport limit; editing and saving remain available"
        .into()
}

// This is a writer for one fixed internal Matroska shape, not a general muxer.
// IDs are the official Matroska element IDs; sizes exclude the reserved all-ones
// value. Inputs are bounded above, so every size fits in at most four bytes.
fn element(id: u32, body: &[u8], out: &mut Vec<u8>) {
    let id = id.to_be_bytes();
    out.extend_from_slice(&id[id.iter().position(|&b| b != 0).unwrap_or(3)..]);
    let mut width = 1;
    while body.len() as u64 >= (1u64 << (7 * width)) - 1 {
        width += 1;
    }
    let size = ((body.len() as u64) | (1u64 << (7 * width))).to_be_bytes();
    out.extend_from_slice(&size[8 - width..]);
    out.extend_from_slice(body);
}
fn uint(id: u32, value: u64, out: &mut Vec<u8>) {
    let bytes = value.to_be_bytes();
    element(
        id,
        &bytes[bytes.iter().position(|&b| b != 0).unwrap_or(7)..],
        out,
    );
}

fn preview_text(text: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overrides_and_text_escapes_survive_but_plain_braces_remain_visible() {
        let tags = r"{\an8\c&H0000FF&\b1\t(0,800,\fs40)\move(10,20,30,40)}Hello\N{\r}World\h!";
        assert_eq!(preview_text(tags), tags);
        assert_eq!(
            preview_text(r"{\a6}SSA {ordinary} {\broken"),
            format!(r"{{\a6}}SSA \{{ordinary\}} \{{\{}broken", '\u{2060}')
        );
        assert_eq!(preview_text(r"{\N"), format!(r"\{{\{}N", '\u{2060}'));
        assert_eq!(preview_text("one\r\ntwo\n三"), r"one\Ntwo\N三");
        assert_eq!(preview_text(r"{\r}\{literal\}"), r"{\r}\{literal\}");
        assert!(!has_markup(r"C:\new\here \{\an8}"));
        assert_eq!(
            preview_text(r"C:\new\here"),
            format!("C:\\{}new\\{}here", '\u{2060}', '\u{2060}')
        );
    }

    #[test]
    fn html_styles_stay_balanced_and_malformed_html_remains_literal() {
        assert_eq!(
            preview_text(r"{\an8}<b>outer <b>inner</b> outer</b>"),
            r"{\an8}{\b1}outer inner outer{\b0}"
        );
        assert_eq!(
            preview_text(r"<B>bold <i>italic</i></B> <u>under</u>"),
            r"{\b1}bold {\i1}italic{\i0}{\b0} {\u1}under{\u0}"
        );
        for text in [
            r"{\an8}<b>x",
            r"<b><i>x</b></i>",
            r"<font color=red>x</font> &lt;",
        ] {
            assert_eq!(preview_text(text), text);
        }
    }

    #[test]
    fn transport_is_bounded_even_when_literal_escaping_expands() {
        use subtitle_core::{Cue, CueId, Milliseconds};
        let doc = Document::new(vec![
            Cue::new(
                CueId(1),
                Milliseconds::new(0).unwrap(),
                Milliseconds::new(1).unwrap(),
                "{".repeat(MAX_BYTES / 3),
            )
            .unwrap(),
        ])
        .unwrap();
        assert!(serialize(&doc).unwrap_err().contains("4 MiB"));
        assert!(
            serialize(&Document::new(vec![]).unwrap())
                .unwrap()
                .is_none()
        );
    }

    /// Independently demux the production bytes with the real native tool. Also
    /// retain reproducible inputs for the separate mpv composition check.
    #[test]
    #[ignore = "requires FFprobe; writes generated native evidence"]
    fn native_demux_preserves_milliseconds_overlaps_and_raw_override_packets() {
        let original = "7\n00:00:02,001 --> 00:00:03,007\n{\\an8\\1c&H0000FF&\\b1}Red top\n\n3\n00:00:01,001 --> 00:00:02,999\n{\\a6}<i>SSA</i>\\NLine\n\n9\n00:00:01,001 --> 00:00:02,007\n{\\move(30,140,350,140)\\fs32\\fad(100,100)}Moving\n\n";
        let parsed = subtitle_formats::srt::parse(original).unwrap();
        let url = serialize(&parsed.document).unwrap().unwrap();
        let bytes: Vec<u8> = url.as_bytes()[6..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
            .collect();
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/fixtures/generated/quality/ass-native");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("preview.mks"), &bytes).unwrap();
        std::fs::write(dir.join("preview.url"), &url).unwrap();
        let output = std::process::Command::new("ffprobe")
            .args(["-v", "error", "-show_packets", "-show_data", "-of", "json"])
            .arg(dir.join("preview.mks"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        std::fs::write(dir.join("ffprobe.json"), &output.stdout).unwrap();
        let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let packets = json["packets"].as_array().unwrap();
        let times: Vec<_> = packets
            .iter()
            .map(|p| (p["pts"].as_u64().unwrap(), p["duration"].as_u64().unwrap()))
            .collect();
        assert_eq!(times, [(1001, 1998), (1001, 1006), (2001, 1006)]);
        // FFprobe's ASCII column wraps at 16 bytes; decode its hex column before
        // asserting full packets, including every tag and original ReadOrder.
        let payloads: Vec<String> = packets
            .iter()
            .map(|p| {
                let hex: String = p["data"]
                    .as_str()
                    .unwrap()
                    .lines()
                    .filter_map(|line| line.split_once(": "))
                    .flat_map(|(_, data)| data.split("  ").next().unwrap().split_whitespace())
                    .collect();
                let bytes: Vec<_> = hex
                    .as_bytes()
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
                    .collect();
                String::from_utf8(bytes).unwrap()
            })
            .collect();
        assert_eq!(
            payloads,
            [
                r"1,0,Default,,0,0,0,,{\a6}{\i1}SSA{\i0}\NLine",
                r"2,0,Default,,0,0,0,,{\move(30,140,350,140)\fs32\fad(100,100)}Moving",
                r"0,0,Default,,0,0,0,,{\an8\1c&H0000FF&\b1}Red top",
            ]
        );
        assert_eq!(
            subtitle_formats::srt::serialize(&parsed.document, &parsed.source).unwrap(),
            original
        );
    }
}
