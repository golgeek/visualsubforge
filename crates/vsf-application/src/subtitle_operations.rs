//! Conservative structural-edit policy, independent of desktop and renderer.
use crate::Result;
use crate::subtitle_markup::{MAX_CUE_BYTES, Projection};
use subtitle_core::Cue;

pub fn validate_split(cue: &Cue, format: subtitle_formats::Format) -> Result<()> {
    validate_split_text(cue, cue.text(), format)
}

fn validate_split_text(cue: &Cue, text: &str, format: subtitle_formats::Format) -> Result<()> {
    if format == subtitle_formats::Format::WebVtt
        && !subtitle_formats::webvtt::metadata(cue)?
            .identifier
            .is_empty()
    {
        return Err("Named WebVTT cues cannot be split or merged automatically: identifiers must stay unique and may be referenced by CSS. Edit the cue manually".into());
    }
    if cue
        .metadata()
        .and_then(|m| m.get("Effect"))
        .is_some_and(|value| !value.trim().is_empty())
    {
        return Err(
            "Split cannot preserve cue-relative event effects; edit these cues manually".into(),
        );
    }
    if !text.contains("{\\") && !text.contains('<') {
        return Ok(());
    }
    if text.len() > MAX_CUE_BYTES {
        return Err("Split requires manual review for text larger than 64 KiB".into());
    }
    let projection = Projection::for_format(text, format);
    if projection.timed || projection.ambiguous.is_some() {
        return Err(if format == subtitle_formats::Format::WebVtt {
            "Split cannot preserve absolute WebVTT timestamp markers or ambiguous markup; edit these cues manually"
        } else {
            "Split cannot preserve cue-relative effects or ambiguous markup; edit these cues manually"
        }.into());
    }
    Ok(())
}

pub fn validate_merge(first: &Cue, second: &Cue, format: subtitle_formats::Format) -> Result<()> {
    validate_merge_text(first, first.text(), second, format)
}

fn validate_merge_text(
    first: &Cue,
    first_text: &str,
    second: &Cue,
    format: subtitle_formats::Format,
) -> Result<()> {
    for (cue, text) in [(first, first_text), (second, second.text())] {
        validate_split_text(cue, text, format)?;
        if !text.contains("{\\") && !text.contains('<') {
            continue;
        }
        let projection = Projection::for_format(text, format);
        if projection.layout || (format == subtitle_formats::Format::Srt && text.contains("{\\")) {
            return Err("Merge cannot preserve independent positioning or mixed SRT/ASS style and escape contexts; edit these cues manually".into());
        }
    }
    if format == subtitle_formats::Format::WebVtt {
        let joined = format!("{}\n{}", first_text, second.text());
        if Projection::for_webvtt(&joined).ambiguous.is_some() {
            return Err("WebVTT merge would change markup scope; close optional voice tags before merging these cues".into());
        }
    }
    Ok(())
}

pub fn merge_separator(format: subtitle_formats::Format) -> &'static str {
    match format {
        subtitle_formats::Format::Ass | subtitle_formats::Format::Ssa => "\\N{\\r}",
        _ => "\n",
    }
}

fn dialogue_side(text: &str, format: subtitle_formats::Format) -> String {
    let projection = Projection::for_format(text, format);
    let mut result = String::with_capacity(text.len() + 2);
    let prefix_end = if projection.ass_document {
        projection
            .units
            .first()
            .map_or(text.len(), |unit| unit.source.start)
    } else {
        0
    };
    result.push_str(&text[..prefix_end]);
    if !projection.visible.trim_start().starts_with(['-', '–', '—']) {
        result.push_str("- ");
    }
    let mut copied = prefix_end;
    for unit in &projection.units {
        if &projection.visible[unit.visible.clone()] == "\n" {
            result.push_str(&text[copied..unit.source.start]);
            result.push(' ');
            copied = unit.source.end;
        }
    }
    result.push_str(&text[copied..]);
    result
}

/// Validates the same document-order pairs as the desktop two-cue operation.
/// Only the application knows which separators and override syntax survive serialization.
pub fn merge_run(
    document: &subtitle_core::Document,
    ids: Vec<subtitle_core::CueId>,
    format: subtitle_formats::Format,
    dialogue: bool,
) -> Result<subtitle_core::Edit> {
    if dialogue && ids.len() != 2 {
        return Err("Merge as dialog needs exactly two cues".into());
    }
    let cues = &document.cues()[document.merge_range(&ids)?];
    for pair in cues.windows(2) {
        validate_merge(&pair[0], &pair[1], format)?;
    }
    let separator = merge_separator(format);
    let text = if dialogue {
        cues.iter()
            .map(|c| dialogue_side(c.text(), format))
            .collect::<Vec<_>>()
            .join(separator)
    } else {
        let mut joined = String::new();
        let mut has_markup = false;
        for (index, cue) in cues.iter().enumerate() {
            // From the third operand on, the left cue in a repeated merge is
            // the accepted prefix. Its size and markup scope can differ from
            // the preceding source cue even when both adjacent pairs are safe.
            if index >= 2 && has_markup {
                validate_merge_text(&cues[0], &joined, cue, format)?;
            }
            if !cue.text().is_empty() {
                if !joined.is_empty() {
                    joined.push_str(separator);
                    has_markup |= separator.contains('{');
                }
                joined.push_str(cue.text());
                has_markup |= cue.text().contains(['<', '{', '&']);
            }
        }
        joined
    };
    Ok(subtitle_core::Edit::MergeRun { ids, text })
}

#[cfg(test)]
mod tests {
    use super::*;
    use subtitle_core::{CueId, Document, Editor, Milliseconds};
    use subtitle_formats::Format;

    fn document(texts: &[&str], format: Format) -> Document {
        let cue = |(i, text): (usize, &&str)| {
            let cue = Cue::new(
                CueId(i as u64 + 1),
                Milliseconds::new(1000).unwrap(),
                Milliseconds::new(6000).unwrap(),
                text.to_string(),
            )
            .unwrap();
            if format == Format::WebVtt {
                cue.with_metadata(subtitle_formats::webvtt::default_metadata().unwrap())
            } else {
                cue
            }
        };
        Document::new(texts.iter().enumerate().map(cue).collect()).unwrap()
    }

    #[test]
    fn dialogue_prefixes_each_side_after_overrides_and_flattens_internal_breaks() {
        for (format, left, right, expected) in [
            (
                Format::Ass,
                r"{\i1}Où vas-tu ?",
                "Au port.",
                r"{\i1}- Où vas-tu ?\N{\r}- Au port.",
            ),
            (
                Format::Ssa,
                r"{\i1}{\b1}– Où\Nvas-tu ?",
                r"{\i0}Au\nport.",
                r"{\i1}{\b1}– Où vas-tu ?\N{\r}{\i0}- Au port.",
            ),
            (
                Format::Srt,
                "— Où\nvas-tu ?",
                "Au port.",
                "— Où vas-tu ?\n- Au port.",
            ),
            (
                Format::WebVtt,
                "Hello",
                "- Already\nmarked",
                "- Hello\n- Already marked",
            ),
            (Format::Srt, "", "", "- \n- "),
        ] {
            let mut editor = Editor::new(document(&[left, right], format));
            let edit =
                merge_run(editor.document(), vec![CueId(1), CueId(2)], format, true).unwrap();
            editor.apply_edit(0, edit, |_| Ok(())).unwrap();
            assert_eq!(editor.document().cues()[0].text(), expected);
        }
    }

    #[test]
    fn run_checks_every_pair_and_omits_empty_plain_operands() {
        let ids = vec![CueId(1), CueId(2), CueId(3)];
        assert!(
            merge_run(
                &document(&["A", "B", r"{\k10}C"], Format::Ass),
                ids.clone(),
                Format::Ass,
                false
            )
            .is_err()
        );
        assert!(
            merge_run(
                &document(&["A", "B", "C"], Format::Srt),
                ids.clone(),
                Format::Srt,
                true
            )
            .is_err()
        );
        let large = "X".repeat(MAX_CUE_BYTES / 2);
        let marked = format!("<i>{large}</i>");
        assert!(
            merge_run(
                &document(&[&marked, &large, "C"], Format::Srt),
                ids.clone(),
                Format::Srt,
                false
            )
            .is_err()
        );
        let mut editor = Editor::new(document(&["A", "", "C"], Format::Srt));
        let edit = merge_run(editor.document(), ids, Format::Srt, false).unwrap();
        editor.apply_edit(0, edit, |_| Ok(())).unwrap();
        assert_eq!(editor.document().cues()[0].text(), "A\nC");
    }
}
