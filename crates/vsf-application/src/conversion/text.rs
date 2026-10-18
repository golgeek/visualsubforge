use super::{ConversionFormat, ConversionReport, ConversionSeverity};
use subtitle_core::Cue;
use subtitle_formats::Format;

pub(super) fn convert(
    cue: &Cue,
    source: Format,
    target: ConversionFormat,
    report: &mut ConversionReport,
) -> String {
    if source == Format::Srt {
        let text = crate::subtitle_text::srt_to_ass(cue.text());
        if text.contains('<') {
            report.issue("literal_html", ConversionSeverity::Loss,
                "Angle-bracket text outside supported balanced b/i/u formatting remains literal in ASS/SSA. Other SRT players may have interpreted it as markup.",
                1, Some(cue), "Unsupported font/color tags, entities and malformed HTML are not interpreted.");
        }
        if text != cue.text() {
            report.issue("text_encoding", ConversionSeverity::Notice,
                "SRT text is encoded as ASS: balanced b/i/u markup becomes overrides, line breaks become \\N, and literal braces/backslashes are protected.",
                1, Some(cue), "Unbalanced or unsupported HTML stays literal, following VSF preview. Literal backslashes can include an invisible U+2060 word joiner.");
        }
        if crate::subtitle_text::has_markup(cue.text()) {
            report.issue("srt_overrides", ConversionSeverity::Notice,
                "Fansub ASS/SSA overrides are retained. Coordinates and named resets now use the exported default style and canvas.",
                1, Some(cue), "No tags are stripped or retimed.");
        }
        if target == ConversionFormat::Ssa && crate::subtitle_text::has_markup(&text) {
            report.issue("ssa_extensions", ConversionSeverity::Notice,
                "SSA output retains ASS extensions and converted b/i/u tags. Their rendering requires a compatible player.",
                1, Some(cue), "The converter does not emulate unsupported tags for older SSA players.");
        }
        return text;
    }
    if target != ConversionFormat::Srt {
        if crate::subtitle_text::has_markup(cue.text()) {
            report.issue("variant_overrides", ConversionSeverity::Notice,
                "Override bytes are retained. ASS extensions in SSA require a compatible renderer; conversion does not emulate unsupported tags.",
                1, Some(cue), "Review playback in the destination player.");
        }
        return cue.text().to_owned();
    }
    // SRT deliberately retains fansub ASS syntax rather than flattening drawings,
    // transforms or karaoke into apparent dialogue. Opt in even for escape-only
    // cues. No split on \\N, which could otherwise create invalid blank SRT lines.
    let mut text = String::with_capacity(cue.text().len() + 4);
    let mut rest = cue.text();
    let mut comments = false;
    let mut html = false;
    while !rest.is_empty() {
        if rest.starts_with("\\{") || rest.starts_with("\\}") {
            text.push_str(&rest[..2]);
            rest = &rest[2..];
            continue;
        }
        if rest.starts_with('{') {
            let Some(end) = rest.find('}') else {
                report.issue(
                    "incomplete_override",
                    ConversionSeverity::Blocked,
                    "An incomplete ASS block cannot be safely reinterpreted as SRT.",
                    1,
                    Some(cue),
                    rest,
                );
                return cue.text().to_owned();
            };
            if rest[1..end].contains('{') {
                report.issue(
                    "nested_override",
                    ConversionSeverity::Blocked,
                    "Nested ASS braces need manual review before SRT conversion.",
                    1,
                    Some(cue),
                    &rest[..=end],
                );
            }
            if rest.starts_with("{\\") {
                text.push_str(&rest[..=end]);
            } else {
                comments = true;
                // Braced comments can also contain a later override introducer.
                // Preserve that suffix as an ordinary complete override block.
                if let Some(at) = rest[..end].find('\\') {
                    text.push('{');
                    text.push_str(&rest[at..=end]);
                }
            }
            rest = &rest[end + 1..];
            continue;
        }
        let c = rest.chars().next().expect("nonempty UTF-8");
        rest = &rest[c.len_utf8()..];
        text.push(c);
        if c == '<' {
            // ASS treats angle markup as text. Prevent SRT/HTML interpreters
            // turning it into formatting, without inventing an entity policy.
            text.push('\u{2060}');
            html = true;
        }
    }
    if comments {
        report.issue("ass_comments", ConversionSeverity::Loss,
            "Non-rendered braced comments are removed from SRT text; contained overrides are retained.",
            1, Some(cue), "The original comments remain in the open source document.");
    }
    if html {
        report.issue("literal_angles", ConversionSeverity::Notice,
            "An invisible U+2060 word joiner follows literal '<' characters so SRT players do not reinterpret ASS text as HTML.",
            1, Some(cue), "Exported text bytes and text-search behavior change.");
    }
    if text.contains('\\') && !crate::subtitle_text::has_markup(&text) {
        text.insert_str(0, "{\\r}");
    }
    if crate::subtitle_text::has_markup(&text) {
        report.issue("fansub_srt", ConversionSeverity::Loss,
            "This SRT retains nonstandard ASS/SSA tags and escapes. Players without fansub-tag support may show raw syntax; styles, canvas and attachments are unavailable.",
            1, Some(cue), "Named resets, positioning, drawings and other style-dependent effects can render differently.");
    }
    text
}
