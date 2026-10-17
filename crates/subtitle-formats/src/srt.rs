//! Supported SRT: UTF-8 (optional BOM), numbered cues, comma-millisecond times,
//! plain or markup-bearing text preserved literally, LF or CRLF. Serialization
//! preserves cue labels, text, BOM/newline style; normalizes separator whitespace
//! and timestamp padding. Unsupported timing suffixes are rejected, never dropped.
use subtitle_core::{Cue, CueId, Document, Milliseconds, Result};

pub fn parse_time(text: &str) -> Result<Milliseconds> {
    let (clock, milliseconds) = text
        .split_once(',')
        .ok_or_else(|| format!("Invalid SRT timestamp: {text}; expected HH:MM:SS,mmm"))?;
    let mut parts: Vec<_> = clock.split(':').collect();
    parts.push(milliseconds);
    if parts.len() != 4
        || !(2..=4).contains(&parts[0].len())
        || parts[1].len() != 2
        || parts[2].len() != 2
        || parts[3].len() != 3
        || parts.iter().any(|p| !p.bytes().all(|b| b.is_ascii_digit()))
    {
        return Err(format!(
            "Invalid SRT timestamp: {text}; expected HH:MM:SS,mmm"
        ));
    }
    let value: Vec<u64> = parts
        .iter()
        .map(|p| {
            p.parse::<u64>()
                .map_err(|_| "Invalid timestamp number".to_string())
        })
        .collect::<Result<_>>()?;
    if value[1] >= 60 || value[2] >= 60 {
        return Err(format!(
            "Timestamp minutes and seconds must be below 60: {text}"
        ));
    }
    Milliseconds::new(value[0] * 3_600_000 + value[1] * 60_000 + value[2] * 1000 + value[3])
}
pub fn format_time(time: Milliseconds) -> String {
    let t = time.get();
    format!(
        "{:02}:{:02}:{:02},{:03}",
        t / 3_600_000,
        (t / 60_000) % 60,
        (t / 1000) % 60,
        t % 1000
    )
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    labels: Vec<(CueId, String)>,
    crlf: bool,
    bom: bool,
}
#[derive(Debug, PartialEq, Eq)]
pub struct Parsed {
    pub document: Document,
    pub source: Source,
}

pub fn validate_text(text: &str) -> Result<()> {
    if !text.is_empty() && text.split('\n').any(|line| line.trim().is_empty()) {
        return Err("SRT cue text cannot contain blank lines; use a separate cue".into());
    }
    Ok(())
}
pub fn parse(input: &str) -> Result<Parsed> {
    if input.contains('\0') {
        return Err("SRT contains NUL characters; UTF-8 text is required".into());
    }
    let bom = input.starts_with('\u{feff}');
    let input = input.strip_prefix('\u{feff}').unwrap_or(input);
    let crlf = input.contains("\r\n");
    let normalized = input.replace("\r\n", "\n");
    if normalized.contains('\r') {
        return Err("Unsupported lone carriage return in SRT".into());
    }
    // Choosing one output convention for mixed endings would silently change
    // source metadata. Ask for an explicitly normalized file instead.
    if crlf && input.replace("\r\n", "").contains('\n') {
        return Err("Mixed LF and CRLF line endings are unsupported".into());
    }
    let mut cues = Vec::new();
    let mut labels = Vec::new();
    let mut block = Vec::new();
    let mut block_line = 1;
    for (line_index, line) in normalized.lines().chain(std::iter::once("")).enumerate() {
        if line.trim().is_empty() {
            if !block.is_empty() {
                let parsed = parse_block(&block, CueId(cues.len() as u64 + 1))
                    .map_err(|e| format!("SRT line {block_line}: {e}"))?;
                labels.push((parsed.0.id(), parsed.1));
                cues.push(parsed.0);
                block.clear();
            }
        } else {
            if block.is_empty() {
                block_line = line_index + 1;
            }
            block.push(line);
        }
    }
    Ok(Parsed {
        document: Document::new(cues)?,
        source: Source { labels, crlf, bom },
    })
}
fn parse_block(lines: &[&str], id: CueId) -> Result<(Cue, String)> {
    if lines.len() < 2 {
        return Err("Cue needs a number and timing line".into());
    }
    let label = lines[0].trim();
    if label.is_empty() || !label.bytes().all(|b| b.is_ascii_digit()) {
        return Err("Expected numeric cue number".into());
    }
    let (start, end) = lines[1]
        .split_once("-->")
        .ok_or("Expected start --> end timing line")?;
    let start = parse_time(start.trim())?;
    let end = parse_time(end.trim())?;
    let text = lines[2..].join("\n");
    validate_text(&text)?;
    Ok((Cue::new(id, start, end, text)?, label.into()))
}
pub fn serialize(document: &Document, source: &Source) -> Result<String> {
    let mut text = String::new();
    write_srt(document, source, |part| text.push_str(part))?;
    Ok(text)
}

/// Exact UTF-8 output size without allocating a second copy of cue text.
/// Uses the same writer as serialization, including labels, BOM and line endings.
pub fn serialized_len(document: &Document, source: &Source) -> Result<usize> {
    let mut length = 0_usize;
    write_srt(document, source, |part| {
        length = length.saturating_add(part.len())
    })?;
    Ok(length)
}

fn write_srt(document: &Document, source: &Source, mut write: impl FnMut(&str)) -> Result<()> {
    // Structural edits renumber labels; Undo restores imported identity/order.
    let original_order = document.cues().len() == source.labels.len()
        && document
            .cues()
            .iter()
            .zip(&source.labels)
            .all(|(cue, (id, _))| cue.id() == *id);
    let newline = if source.crlf { "\r\n" } else { "\n" };
    if source.bom {
        write("\u{feff}");
    }
    for (index, cue) in document.cues().iter().enumerate() {
        if cue.metadata().is_some() {
            return Err(
                "SRT cannot preserve event metadata; explicit format conversion is required".into(),
            );
        }
        validate_text(cue.text())?;
        if original_order {
            write(&source.labels[index].1);
        } else {
            write(&(index + 1).to_string());
        }
        write(newline);
        write(&format_time(cue.start()));
        write(" --> ");
        write(&format_time(cue.end()));
        write(newline);
        for (line, text) in cue.text().split('\n').enumerate() {
            if line > 0 {
                write(newline);
            }
            write(text);
        }
        write(newline);
        write(newline);
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unicode_markup_labels_and_format_round_trip() {
        let input =
            "\u{feff}007\r\n00:00:01,250 --> 00:00:02,999\r\n <i>Héllo</i> \r\n世界\r\n\r\n";
        let doc = parse(input).unwrap();
        assert_eq!(serialize(&doc.document, &doc.source).unwrap(), input);
        assert_eq!(
            serialized_len(&doc.document, &doc.source).unwrap(),
            input.len()
        );
        assert_eq!(
            parse(&serialize(&doc.document, &doc.source).unwrap()).unwrap(),
            doc
        );
        let dup =
            parse("1\n00:00:01,000 --> 00:00:03,000\nA\n\n1\n00:00:02,000 --> 00:00:04,000\nB\n")
                .unwrap();
        assert_ne!(dup.document.cues()[0].id(), dup.document.cues()[1].id()); // overlaps and repeated source labels are allowed
    }
    #[test]
    fn structural_edits_renumber_and_undo_restores_source_labels() {
        use subtitle_core::Editor;
        let input = "\u{feff}007\r\n00:00:01,000 --> 00:00:03,000\r\n 日本語 \r\n\r\n002\r\n00:00:04,000 --> 00:00:05,000\r\nB\r\n\r\n";
        let parsed = parse(input).unwrap();
        let mut editor = Editor::new(parsed.document);
        let right = editor
            .split(0, CueId(1), Milliseconds::new(2000).unwrap())
            .unwrap();
        let output = serialize(editor.document(), &parsed.source).unwrap();
        assert_eq!(
            serialized_len(editor.document(), &parsed.source).unwrap(),
            output.len()
        );
        assert!(output.starts_with("\u{feff}1\r\n"));
        assert!(output.contains("\r\n\r\n2\r\n00:00:02,000"));
        assert!(output.contains("\r\n\r\n3\r\n00:00:04,000"));
        let reopened = parse(&output).unwrap();
        for (actual, expected) in reopened
            .document
            .cues()
            .iter()
            .zip(editor.document().cues())
        {
            assert_eq!(
                (actual.start(), actual.end(), actual.text()),
                (expected.start(), expected.end(), expected.text())
            );
        }
        editor.undo(1).unwrap();
        assert_eq!(serialize(editor.document(), &parsed.source).unwrap(), input);
        assert_eq!(
            serialized_len(editor.document(), &parsed.source).unwrap(),
            input.len()
        );
        editor.redo(2).unwrap();
        assert_eq!(
            serialize(editor.document(), &parsed.source).unwrap(),
            output
        );
        editor.merge(3, CueId(1), right).unwrap();
        // Original order/IDs survive merge-back, so imported labels apply again.
        assert!(
            serialize(editor.document(), &parsed.source)
                .unwrap()
                .starts_with("\u{feff}007\r\n")
        );
        editor.delete(4, CueId(1)).unwrap();
        editor.delete(5, CueId(2)).unwrap();
        assert_eq!(
            serialize(editor.document(), &parsed.source).unwrap(),
            "\u{feff}"
        );
        assert!(parse("\u{feff}").unwrap().document.cues().is_empty());
    }

    #[test]
    fn malformed_or_unsupported_content_is_not_silently_dropped() {
        for input in [
            "text",
            "1\n00:00:02,000 --> 00:00:01,000\nA",
            "1\n00:00:01,000 --> 00:00:02,000 X1:0\nA",
            "1\r\n00:00:01,000 --> 00:00:02,000\nA",
        ] {
            assert!(parse(input).is_err(), "{input}");
        }
        for time in [
            "-1:00:00,000",
            "00:60:00,000",
            "00:00:60,000",
            "00:00:01.000",
            "00,00:01:000",
            "00:00,01,000",
            "00:00:01,00",
            "10000:00:00,000",
        ] {
            assert!(parse_time(time).is_err(), "{time}");
        }
    }
    #[test]
    fn time_boundaries_and_empty_cues_round_trip() {
        for value in [
            0,
            1,
            999,
            1000,
            59_999,
            60_000,
            3_600_000,
            Milliseconds::MAX,
        ] {
            let time = Milliseconds::new(value).unwrap();
            assert_eq!(parse_time(&format_time(time)).unwrap(), time);
        }
        assert!(Milliseconds::new(Milliseconds::MAX + 1).is_err());
        let doc = parse("1\n00:00:00,000 --> 00:00:01,000\n\n").unwrap();
        assert_eq!(
            parse(&serialize(&doc.document, &doc.source).unwrap()).unwrap(),
            doc
        );
    }
    #[test]
    fn editing_fixture_preserves_numeric_dialogue_and_has_explicit_import_limits() {
        let text = include_str!("../../../tests/fixtures/editing.srt");
        let parsed = parse(text).unwrap();
        assert_eq!(parsed.document.cues().len(), 4);
        assert!(parsed.document.cues()[2].text().ends_with("\n123"));
        assert_eq!(serialize(&parsed.document, &parsed.source).unwrap(), text);
        for unsupported in [
            "00:00:01,000 --> 00:00:02,000\nUnnumbered",
            "1\n00:00:01.000 --> 00:00:02.000\nDot times",
            "1\n00:00:01,000 --> 00:00:02,000\nText\n\n2",
        ] {
            assert!(parse(unsupported).is_err());
        }
    }
}
