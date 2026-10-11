//! Internal preview encoding, independent of SRT persistence. WebVTT preserves
//! millisecond packet timestamps and gives the native decoder an explicit escape
//! syntax for literal angle brackets, braces and backslashes.
use subtitle_core::Document;

const MAX_PREVIEW_BYTES: usize = 4 * 1024 * 1024;

pub(crate) fn serialize(document: &Document) -> Result<Option<String>, String> {
    let mut cues: Vec<_> = document
        .cues()
        .iter()
        .enumerate()
        .filter(|(_, cue)| !cue.text().trim().is_empty())
        .collect();
    if cues.is_empty() {
        return Ok(None);
    }
    // Native collision layout sees a stable chronological order; equal starts
    // retain document order. This does not reorder the accepted document/list.
    cues.sort_by_key(|(index, cue)| (cue.start().get(), *index));
    let mut output = String::from("WEBVTT\n\n");
    for (_, cue) in cues {
        output.push_str(&timestamp(cue.start().get()));
        output.push_str(" --> ");
        output.push_str(&timestamp(cue.end().get()));
        output.push('\n');
        // Flattening redundant style tags can leave an empty payload line,
        // which terminates a WebVTT cue. Preserve that line with a space;
        // this presentation-only substitution never changes accepted text.
        for (index, line) in preview_text(cue.text()).split('\n').enumerate() {
            if index > 0 {
                output.push('\n');
            }
            output.push_str(if line.is_empty() { "\u{a0}" } else { line });
        }
        output.push_str("\n\n");
        if output.len() > MAX_PREVIEW_BYTES {
            return Err("Document exceeds the 4 MiB native preview limit; editing and saving remain available".into());
        }
    }
    Ok(Some(output))
}

fn timestamp(ms: u64) -> String {
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        ms / 3_600_000,
        ms / 60_000 % 60,
        ms / 1000 % 60,
        ms % 1000,
    )
}

fn literal(text: &str, output: &mut String) {
    for c in text.chars() {
        match c {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            _ => output.push(c),
        }
    }
}

fn preview_text(text: &str) -> String {
    let mut output = String::new();
    let mut remaining = text;
    let mut styles = Vec::new();
    let mut malformed_style = false;
    while let Some(start) = remaining.find('<') {
        literal(&remaining[..start], &mut output);
        remaining = &remaining[start..];
        let Some(end) = remaining.find('>') else {
            break;
        };
        let tag = remaining[..=end].to_ascii_lowercase();
        match tag.as_str() {
            "<b>" | "<i>" | "<u>" => {
                let style = tag.as_bytes()[1];
                // Native tags toggle a style; flatten nested copies of the same
                // style so closing an inner tag cannot disable its outer tag.
                if !styles.contains(&style) {
                    output.push_str(&tag);
                }
                styles.push(style);
            }
            "</b>" | "</i>" | "</u>" => {
                let style = tag.as_bytes()[2];
                malformed_style |= styles.pop() != Some(style);
                if !styles.contains(&style) {
                    output.push_str(&tag);
                }
            }
            _ => literal(&remaining[..=end], &mut output),
        }
        remaining = &remaining[end + 1..];
    }
    literal(remaining, &mut output);
    if malformed_style || !styles.is_empty() {
        // Do not silently repair mismatched/unclosed styling or hide its syntax.
        output.clear();
        literal(text, &mut output);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_styles_are_balanced_and_other_syntax_is_literal() {
        assert_eq!(
            preview_text("<b>outer <b>inner</b> outer</b>"),
            "<b>outer inner outer</b>"
        );
        assert_eq!(
            preview_text("<B>bold <i>italic</i></B> <u>under</u>"),
            "<b>bold <i>italic</i></b> <u>under</u>"
        );
        assert_eq!(
            preview_text("<font color=red>x</font> <00:01.000> &lt; {\\an8} \\N 日本語\nnext"),
            "&lt;font color=red&gt;x&lt;/font&gt; &lt;00:01.000&gt; &amp;lt; {\\an8} \\N 日本語\nnext"
        );
        for text in [
            "<b>x",
            "</i>x",
            "<b><i>x</b></i>",
            "1 < 2",
            "<unknown>x</unknown>",
        ] {
            let mut expected = String::new();
            literal(text, &mut expected);
            assert_eq!(preview_text(text), expected);
        }
    }

    #[test]
    fn preview_preserves_milliseconds_and_stably_orders_overlaps_without_mutation() {
        let original = "7\n00:00:02,001 --> 00:00:03,007\nLater\n\n3\n00:00:01,001 --> 00:00:02,999\nFirst\n\n9\n00:00:01,001 --> 00:00:02,007\nSame start\n\n";
        let parsed = subtitle_formats::srt::parse(original).unwrap();
        let preview = serialize(&parsed.document).unwrap().unwrap();
        assert_eq!(
            preview,
            "WEBVTT\n\n00:00:01.001 --> 00:00:02.999\nFirst\n\n00:00:01.001 --> 00:00:02.007\nSame start\n\n00:00:02.001 --> 00:00:03.007\nLater\n\n"
        );
        assert_eq!(
            subtitle_formats::srt::serialize(&parsed.document, &parsed.source).unwrap(),
            original
        );
    }

    #[test]
    fn flattened_style_only_lines_do_not_truncate_preview_text() {
        let original = "1\n00:00:01,000 --> 00:00:04,000\n<b>First\n<b></b>\nThird</b>\n\n";
        let parsed = subtitle_formats::srt::parse(original).unwrap();
        assert_eq!(
            serialize(&parsed.document).unwrap().unwrap(),
            "WEBVTT\n\n00:00:01.000 --> 00:00:04.000\n<b>First\n\u{a0}\nThird</b>\n\n"
        );
        assert_eq!(
            subtitle_formats::srt::serialize(&parsed.document, &parsed.source).unwrap(),
            original
        );
    }

    #[test]
    fn empty_document_clears_preview_and_expansion_respects_limit() {
        assert_eq!(serialize(&Document::new(vec![]).unwrap()).unwrap(), None);
        use subtitle_core::{Cue, CueId, Milliseconds};
        let doc = Document::new(vec![
            Cue::new(
                CueId(1),
                Milliseconds::new(0).unwrap(),
                Milliseconds::new(1).unwrap(),
                "<".repeat(MAX_PREVIEW_BYTES / 4),
            )
            .unwrap(),
        ])
        .unwrap();
        assert!(serialize(&doc).is_err());
    }
}
