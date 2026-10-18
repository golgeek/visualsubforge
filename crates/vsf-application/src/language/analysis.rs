use super::{Language, Lexicon, Settings, dictionary};
use crate::{
    quality::{Error, Finding, MAX_FINDINGS, Result, Rule, Severity},
    subtitle_markup::{MAX_CUE_BYTES, Projection},
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, ops::Range};
use subtitle_core::Cue;
use subtitle_formats::Format;
use unicode_segmentation::UnicodeSegmentation;

/// Offsets refer to original source UTF-16, including markup, like the editor.
/// An advisory may remain selectable even when a protected boundary prevents a fix.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Detail {
    pub start: u32,
    pub end: u32,
    pub word: String,
    pub replacement: Option<String>,
    pub manual_reason: Option<String>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Coverage {
    pub enabled: bool,
    pub checked_words: u32,
    pub skipped_words: u32,
    pub skipped_cues: u32,
    pub personalized: bool,
}

/// One immutable policy/lexicon per evaluation. Cache repeated words within this
/// evaluation only, with a fixed entry cap; never mutate bundled dictionaries.
pub(crate) struct Analysis<'a> {
    settings: &'a Settings,
    lexicon: &'a Lexicon,
    cache: HashMap<String, bool>,
    pub coverage: Coverage,
    sentence_start: bool,
}
impl<'a> Analysis<'a> {
    pub fn new(settings: &'a Settings, lexicon: &'a Lexicon) -> Self {
        Self {
            settings,
            lexicon,
            cache: HashMap::new(),
            sentence_start: true,
            coverage: Coverage {
                enabled: settings.enabled(),
                personalized: !lexicon.is_empty(),
                ..Coverage::default()
            },
        }
    }
    pub fn check(
        &mut self,
        cue: &Cue,
        index: usize,
        format: Format,
        out: &mut Vec<Finding>,
        mut checkpoint: impl FnMut() -> Result<()>,
    ) -> Result<()> {
        if !self.settings.enabled() {
            return Ok(());
        }
        checkpoint()?;
        let source = cue.text();
        let projection =
            (source.len() <= MAX_CUE_BYTES).then(|| Projection::for_format(source, format));
        let reason = match &projection {
            None => Some("Cue exceeds the 64 KiB language-tool limit; review manually."),
            Some(p) => p.ambiguous,
        };
        if let Some(reason) = reason {
            self.coverage.skipped_cues += 1;
            self.sentence_start = false;
            push(out, cue, index, Rule::LanguageCoverage, reason, None)?;
            return Ok(());
        }
        let p = projection.expect("bounded projection");
        let text = p.visible.as_str();
        let offsets = source_offsets(source);
        let mut emitter = Emitter {
            cue,
            index,
            projection: &p,
            offsets: &offsets,
            out,
        };
        if self.settings.spelling {
            // URLs/email addresses are explicitly excluded, without interpreting
            // punctuation in them as words or editorial typography.
            let excluded = addresses(text);
            let mut skipped = 0;
            for range in words(text) {
                checkpoint()?;
                let word = &text[range.clone()];
                if excluded_range(&excluded, &range) || !dictionary::valid_word(word) {
                    skipped += 1;
                    continue;
                }
                self.coverage.checked_words += 1;
                let correct = match self.cache.get(word) {
                    Some(correct) => *correct,
                    None => {
                        let correct =
                            dictionary::is_correct(self.settings.dictionary, word, self.lexicon)?;
                        if self.cache.len() < 50_000 {
                            self.cache.insert(word.into(), correct);
                        }
                        correct
                    }
                };
                if !correct {
                    emitter.emit(Rule::Spelling, range, None, "Word is absent from the selected dictionary and current personal/ignored words.")?;
                }
            }
            self.coverage.skipped_words += skipped;
            if skipped != 0 {
                push(
                    emitter.out,
                    cue,
                    index,
                    Rule::LanguageCoverage,
                    &format!(
                        "Spelling skipped {skipped} URL/email or unsupported/oversized word tokens; review manually."
                    ),
                    None,
                )?;
            }
        }
        let excluded = addresses(text);
        if self.settings.typography {
            typography(
                &mut emitter,
                self.settings.dictionary,
                &excluded,
                &mut checkpoint,
            )?;
        }
        if self.settings.dialogue_markers {
            dialogue(&mut emitter, self.settings.dictionary, &mut checkpoint)?;
        }
        if self.settings.capitalization {
            for (at, c) in text.char_indices() {
                checkpoint()?;
                if excluded_range(&excluded, &(at..at + 1)) {
                    self.sentence_start = false;
                    continue;
                }
                if matches!(c, '!' | '?') {
                    self.sentence_start = true;
                } else if c.is_alphabetic() {
                    if self.sentence_start && c.is_lowercase() {
                        emitter.emit(Rule::Capitalization, at..at + c.len_utf8(), Some(c.to_uppercase().collect()), "Consider a capital at the document start or after an explicit question/exclamation sentence ending.")?;
                    }
                    self.sentence_start = false;
                } else if c.is_numeric() {
                    self.sentence_start = false;
                }
            }
        }
        Ok(())
    }
}

struct Emitter<'a> {
    cue: &'a Cue,
    index: usize,
    projection: &'a Projection,
    offsets: &'a [u32],
    out: &'a mut Vec<Finding>,
}
impl Emitter<'_> {
    fn emit(
        &mut self,
        rule: Rule,
        visible: Range<usize>,
        replacement: Option<String>,
        explanation: &str,
    ) -> Result<()> {
        let source = self.projection.source_range(visible.clone());
        let safe = self.projection.safe_range(self.cue.text(), source.clone());
        push(
            self.out,
            self.cue,
            self.index,
            rule,
            explanation,
            Some(Detail {
                start: self.offsets[source.start],
                end: self.offsets[source.end],
                word: self.projection.visible[visible].into(),
                replacement,
                manual_reason: (!safe)
                    .then(|| "The text crosses a protected tag or escape; edit manually.".into()),
            }),
        )
    }
}
fn push(
    out: &mut Vec<Finding>,
    cue: &Cue,
    index: usize,
    rule: Rule,
    explanation: &str,
    language: Option<Detail>,
) -> Result<()> {
    if out.len() >= MAX_FINDINGS {
        return Err(Error::Limit(
            "Quality findings exceed 100000; narrow the enabled rules".into(),
        ));
    }
    out.push(Finding {
        rule,
        cue_id: cue.id().0.to_string(),
        cue_index: index as u32,
        related_cue_id: None,
        severity: Severity::Advisory,
        measured: None,
        expected: "Review the selected language policy".into(),
        explanation: explanation.into(),
        language,
    });
    Ok(())
}
fn source_offsets(source: &str) -> Vec<u32> {
    let mut offsets = vec![0; source.len() + 1];
    let mut units = 0;
    for (at, c) in source.char_indices() {
        offsets[at] = units;
        units += c.len_utf16() as u32;
    }
    offsets[source.len()] = units;
    offsets
}
fn words(text: &str) -> Vec<Range<usize>> {
    let mut result: Vec<Range<usize>> = vec![];
    for (at, word) in text.unicode_word_indices() {
        if let Some(last) = result.last_mut()
            && matches!(&text[last.end..at], "-" | "'" | "’")
        {
            last.end = at + word.len();
        } else {
            result.push(at..at + word.len());
        }
    }
    result.retain(|r| text[r.clone()].chars().any(char::is_alphabetic));
    result
}
fn addresses(text: &str) -> Vec<Range<usize>> {
    let mut ranges = vec![];
    let mut offset = 0;
    for part in text.split_inclusive(char::is_whitespace) {
        if part.contains("://") || part.contains('@') || part.starts_with("www.") {
            ranges.push(offset..offset + part.trim_end().len());
        }
        offset += part.len();
    }
    ranges
}
fn excluded_range(excluded: &[Range<usize>], range: &Range<usize>) -> bool {
    let at = excluded.partition_point(|r| r.end <= range.start);
    excluded.get(at).is_some_and(|r| r.start < range.end)
}
fn typography(
    emitter: &mut Emitter<'_>,
    language: Language,
    excluded: &[Range<usize>],
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<()> {
    let text = emitter.projection.visible.as_str();
    let chars: Vec<_> = text.char_indices().collect();
    for (i, &(at, c)) in chars.iter().enumerate() {
        checkpoint()?;
        if excluded_range(excluded, &(at..at + 1)) {
            continue;
        }
        let previous = i.checked_sub(1).map(|i| chars[i].1);
        let next = chars.get(i + 1).map(|p| p.1);
        if c == '\''
            && previous.is_some_and(char::is_alphabetic)
            && next.is_some_and(char::is_alphabetic)
        {
            emitter.emit(
                Rule::Typography,
                at..at + 1,
                Some("’".into()),
                "Consider a typographic apostrophe inside this word.",
            )?;
        }
        if text[at..].starts_with("...")
            && previous != Some('.')
            && !text[at + 3..].starts_with('.')
        {
            emitter.emit(
                Rule::Typography,
                at..at + 3,
                Some("…".into()),
                "Consider one ellipsis character for exactly three dots.",
            )?;
        }
        if matches!(c, ':' | ';' | '?' | '!') && language == Language::French {
            if previous == Some(' ') && (i < 2 || !chars[i - 2].1.is_whitespace()) {
                emitter.emit(
                    Rule::Typography,
                    at - 1..at,
                    Some("\u{a0}".into()),
                    "French punctuation convention: use a nonbreaking space before : ; ? !.",
                )?;
            } else if previous.is_some_and(|p| p.is_alphabetic()) {
                // Replace the punctuation with space+punctuation; no empty range.
                emitter.emit(
                    Rule::Typography,
                    at..at + c.len_utf8(),
                    Some(format!("\u{a0}{c}")),
                    "French punctuation convention: insert a nonbreaking space before : ; ? !.",
                )?;
            }
        }
        if matches!(c, ',' | '.' | ';' | ':' | '?' | '!')
            && language == Language::EnglishUs
            && previous == Some(' ')
        {
            let mut start = at - 1;
            while start > 0 && text.as_bytes()[start - 1] == b' ' {
                start -= 1;
            }
            if start > 0 && !text[..start].ends_with('\n') {
                emitter.emit(
                    Rule::Typography,
                    start..at + c.len_utf8(),
                    Some(c.to_string()),
                    "English punctuation convention: remove spaces before punctuation.",
                )?;
            }
        }
    }
    Ok(())
}
fn dialogue(
    emitter: &mut Emitter<'_>,
    language: Language,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<()> {
    let text = emitter.projection.visible.as_str();
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        checkpoint()?;
        let trimmed = line.trim_start_matches([' ', '\t']);
        let at = offset + line.len() - trimmed.len();
        if let Some(mark) = trimmed
            .chars()
            .next()
            .filter(|c| matches!(c, '-' | '–' | '—'))
        {
            let following = &trimmed[mark.len_utf8()..];
            let body = following.trim_start_matches([' ', '\t', '\u{a0}']);
            if body.chars().next().is_some_and(char::is_alphabetic) {
                let end = at + trimmed.len() - body.len();
                let wanted = if language == Language::French {
                    "— "
                } else {
                    "- "
                };
                if &text[at..end] != wanted {
                    emitter.emit(Rule::DialogueMarker, at..end, Some(wanted.into()), "Normalize an existing dialogue marker and its following space; no speaker is inferred.")?;
                }
            }
        }
        offset += line.len();
    }
    Ok(())
}
