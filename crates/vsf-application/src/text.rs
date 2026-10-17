//! Pure, bounded text proposals. Accepted source is never replaced with stripped
//! quality text. An immutable baseline plus source offsets protects tags/history.
mod formatting;
mod reflow;
use crate::quality::{self, Error, Profile, Result};
use crate::subtitle_markup::Projection;
pub use formatting::{Format, Formatted, format_selection};
use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};
use subtitle_core::{Cue, CueText, Document, Edit};

pub const MAX_CUE_BYTES: usize = crate::subtitle_markup::MAX_CUE_BYTES;
pub const MAX_MATCHES: usize = 100_000;
pub const MAX_INPUT_BYTES: usize = 4096;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Operation {
    Find {
        query: String,
        case_sensitive: bool,
    },
    Replace {
        query: String,
        replacement: String,
        case_sensitive: bool,
    },
    JoinLines,
    Reflow,
}
#[derive(Clone, Debug)]
pub struct Match {
    /// Source UTF-16 offsets for textarea selection, not bytes or visible offsets.
    pub start: u32,
    pub end: u32,
    pub skipped: Option<&'static str>,
    source: std::ops::Range<usize>,
}
#[derive(Clone, Debug)]
pub struct Row {
    pub cue_index: usize,
    pub matches: Vec<Match>,
    pub after: Option<String>,
    pub skipped: Option<&'static str>,
}
pub struct Proposal {
    document: Document,
    rows: Vec<Row>,
    before_findings: usize,
    after_findings: usize,
    matches: usize,
    replacement: Option<String>,
}
impl Proposal {
    pub fn document(&self) -> &Document {
        &self.document
    }
    pub fn rows(&self) -> &[Row] {
        &self.rows
    }
    pub fn matches(&self) -> usize {
        self.matches
    }
    pub fn changed(&self) -> usize {
        self.rows.iter().filter(|r| r.after.is_some()).count()
    }
    pub fn before_findings(&self) -> usize {
        self.before_findings
    }
    pub fn after_findings(&self) -> usize {
        self.after_findings
    }
    pub fn edit(&self, current: &Document, only: Option<(usize, usize)>) -> Result<Edit> {
        if current != &self.document {
            return Err(Error::Invalid("Subtitles changed; preview again.".into()));
        }
        let mut texts: Vec<_> = current
            .cues()
            .iter()
            .map(|c| CueText {
                id: c.id(),
                text: c.text().into(),
            })
            .collect();
        let mut changed = false;
        if let Some((cue, index)) = only {
            let row = self
                .rows
                .iter()
                .find(|r| r.cue_index == cue)
                .ok_or_else(|| Error::Invalid("Match no longer exists.".into()))?;
            let found = row
                .matches
                .get(index)
                .filter(|m| m.skipped.is_none())
                .ok_or_else(|| Error::Invalid("This match cannot be replaced safely.".into()))?;
            let replacement = self
                .replacement
                .as_deref()
                .ok_or_else(|| Error::Invalid("Create a replacement preview first.".into()))?;
            let source = current.cues()[cue].text();
            let after = format!(
                "{}{}{}",
                &source[..found.source.start],
                replacement,
                &source[found.source.end..]
            );
            if after.len() > MAX_CUE_BYTES || !same_markup(source, &after) {
                return Err(Error::Invalid(
                    "This replacement would change markup or exceed the cue limit.".into(),
                ));
            }
            changed = after != source;
            texts[cue].text = after;
        } else {
            for row in &self.rows {
                if let Some(text) = &row.after {
                    texts[row.cue_index].text = text.clone();
                    changed = true;
                }
            }
        }
        if !changed {
            return Err(Error::Invalid(
                "There are no text changes to accept.".into(),
            ));
        }
        Ok(Edit::TextBatch { texts })
    }
}

pub fn propose(
    document: Document,
    profile: &Profile,
    operation: &Operation,
    mut checkpoint: impl FnMut() -> Result<()>,
) -> Result<Proposal> {
    checkpoint()?;
    let (search, replacement) = search(operation)?;
    let before_findings = quality::evaluate(&document, profile, |_, _| checkpoint())?
        .findings
        .len();
    let mut rows = vec![];
    let mut matches = 0;
    let mut output_bytes = 0;
    for (cue_index, cue) in document.cues().iter().enumerate() {
        checkpoint()?;
        let source = cue.text();
        if source.len() > MAX_CUE_BYTES {
            rows.push(Row {
                cue_index,
                matches: vec![],
                after: None,
                skipped: Some("Cue exceeds the 64 KiB text-tool limit; edit manually."),
            });
            output_bytes += source.len();
            continue;
        }
        let projection = Projection::new(source);
        let row = if let Some(regex) = &search {
            replace_row(
                cue_index,
                source,
                &projection,
                regex,
                replacement,
                &mut matches,
                &mut checkpoint,
            )?
        } else {
            reflow::row(
                cue_index,
                source,
                &projection,
                profile,
                matches!(operation, Operation::JoinLines),
            )
        };
        output_bytes += row.after.as_ref().map_or(source.len(), String::len);
        if output_bytes > quality::MAX_TEXT_BYTES {
            return Err(Error::Limit(
                "Proposed text exceeds 16 MiB; narrow the operation.".into(),
            ));
        }
        if row.after.is_some() || row.skipped.is_some() || !row.matches.is_empty() {
            rows.push(row);
        }
    }
    checkpoint()?;
    let changed = rows.iter().any(|r| r.after.is_some());
    let after_findings = if changed {
        let mut cursor = rows.iter().peekable();
        let mut cues = Vec::with_capacity(document.cues().len());
        for (i, cue) in document.cues().iter().enumerate() {
            checkpoint()?;
            let row = if cursor.peek().is_some_and(|r| r.cue_index == i) {
                cursor.next()
            } else {
                None
            };
            let text = row.and_then(|r| r.after.as_deref()).unwrap_or(cue.text());
            cues.push(
                Cue::new(cue.id(), cue.start(), cue.end(), text.into()).map_err(Error::Invalid)?,
            );
        }
        let candidate = Document::new(cues).map_err(Error::Invalid)?;
        quality::evaluate(&candidate, profile, |_, _| checkpoint())?
            .findings
            .len()
    } else {
        before_findings
    };
    checkpoint()?;
    Ok(Proposal {
        document,
        rows,
        before_findings,
        after_findings,
        matches,
        replacement: replacement.map(str::to_owned),
    })
}
fn search(operation: &Operation) -> Result<(Option<Regex>, Option<&str>)> {
    let (query, sensitive, replacement) = match operation {
        Operation::Find {
            query,
            case_sensitive,
        } => (query, case_sensitive, None),
        Operation::Replace {
            query,
            replacement,
            case_sensitive,
        } => {
            if replacement.len() > MAX_INPUT_BYTES
                || replacement.contains(['\r', '\0', '<', '>', '{', '}', '\\'])
            {
                return Err(Error::Invalid("Replacement must be at most 4096 UTF-8 bytes of plain text, without markup delimiters, backslashes, CR or NUL.".into()));
            }
            (query, case_sensitive, Some(replacement.as_str()))
        }
        _ => return Ok((None, None)),
    };
    if query.is_empty() || query.len() > MAX_INPUT_BYTES || query.contains(['\r', '\0']) {
        return Err(Error::Invalid(
            "Find text must contain 1–4096 UTF-8 bytes without CR or NUL.".into(),
        ));
    }
    let regex = RegexBuilder::new(&regex::escape(query))
        .case_insensitive(!sensitive)
        .size_limit(4 * 1024 * 1024)
        .build()
        .map_err(|e| Error::Invalid(format!("Cannot search: {e}")))?;
    Ok((Some(regex), replacement))
}
fn replace_row(
    cue_index: usize,
    source: &str,
    projection: &Projection,
    regex: &Regex,
    replacement: Option<&str>,
    total: &mut usize,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<Row> {
    let mut row = Row {
        cue_index,
        matches: vec![],
        after: None,
        skipped: None,
    };
    let mut patches = vec![];
    // Build source UTF-16 offsets once; repeated matches must not rescan prefixes.
    let mut utf16 = vec![0u32; source.len() + 1];
    let mut units = 0;
    for (at, c) in source.char_indices() {
        utf16[at] = units;
        units += c.len_utf16() as u32;
    }
    utf16[source.len()] = units;
    for found in regex.find_iter(&projection.visible) {
        checkpoint()?;
        *total += 1;
        if *total > MAX_MATCHES {
            return Err(Error::Limit(
                "More than 100,000 matches; narrow the search.".into(),
            ));
        }
        let range = projection.source_range(found.range());
        let skipped = projection.ambiguous.or_else(|| {
            (!projection.safe_range(source, range.clone())).then_some(
                "Match crosses a tag or ASS escape; replace manually to choose its formatting.",
            )
        });
        row.matches.push(Match {
            start: utf16[range.start],
            end: utf16[range.end],
            skipped,
            source: range.clone(),
        });
        if skipped.is_none()
            && let Some(replacement) = replacement
        {
            patches.push((range, replacement));
        }
    }
    if !patches.is_empty() {
        // Apply in a single forward pass. Never quadratic insertions into a large cue.
        let extra = patches.iter().map(|(_, s)| s.len()).sum::<usize>();
        if extra > quality::MAX_TEXT_BYTES {
            return Err(Error::Limit("Replacement expansion exceeds 16 MiB.".into()));
        }
        let removed = patches.iter().map(|(r, _)| r.len()).sum::<usize>();
        if source.len() - removed + extra > MAX_CUE_BYTES {
            row.skipped = Some("Replacement would exceed the 64 KiB text-tool cue limit.");
            for found in &mut row.matches {
                found.skipped = row.skipped;
            }
            return Ok(row);
        }
        let mut after = String::with_capacity(source.len() + extra);
        let mut at = 0;
        for (range, text) in patches {
            after.push_str(&source[at..range.start]);
            after.push_str(text);
            at = range.end;
        }
        after.push_str(&source[at..]);
        if after != source {
            if !same_markup(source, &after) {
                for found in &mut row.matches {
                    found.skipped =
                        Some("Replacement would create or change markup; edit manually.");
                }
            } else {
                row.after = Some(after);
            }
        }
    }
    Ok(row)
}

fn same_markup(before: &str, after: &str) -> bool {
    let old = Projection::new(before);
    let new = Projection::new(after);
    old.ambiguous == new.ambiguous
        && old
            .protected
            .iter()
            .map(|r| &before[r.clone()])
            .eq(new.protected.iter().map(|r| &after[r.clone()]))
}

#[cfg(test)]
mod tests;
