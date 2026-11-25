//! Cue-local language work from successful accepted quality evaluations.
use super::{Analysis, Detail, Lexicon, analysis::push};
use crate::quality::{Finding, Result, Rule};
use std::collections::HashMap;
use subtitle_core::Cue;
use subtitle_formats::Format;

const MAX_ENTRIES: usize = 16_384;
const MAX_BYTES: usize = 8 * 1024 * 1024;

struct Advisory {
    rule: Rule,
    explanation: String,
    detail: Option<Detail>,
}

struct Entry {
    findings: Vec<Advisory>,
    checked_words: u32,
    skipped_words: u32,
    skipped_cues: u32,
    sentence_start: bool,
    bytes: usize,
    visited: bool,
}

/// Owned by one evaluator at a time. The two maps distinguish the entering
/// sentence state and allow looking up source text without allocating a key.
#[derive(Default)]
pub struct LanguageCache {
    scope: Option<(String, Format, Lexicon)>,
    entries: [HashMap<Box<str>, Entry>; 2],
    bytes: usize,
}

impl LanguageCache {
    pub(crate) fn prepare(&mut self, fingerprint: &str, format: Format, lexicon: &Lexicon) {
        if self
            .scope
            .as_ref()
            .is_some_and(|(hash, f, words)| hash == fingerprint && *f == format && words == lexicon)
        {
            for entry in self.entries.iter_mut().flat_map(|map| map.values_mut()) {
                entry.visited = false;
            }
            return;
        }
        *self = Self {
            scope: Some((fingerprint.into(), format, lexicon.clone())),
            ..Self::default()
        };
    }

    fn len(&self) -> usize {
        self.entries[0].len() + self.entries[1].len()
    }

    pub(crate) fn finish_scan(&mut self) {
        for map in &mut self.entries {
            map.retain(|_, entry| {
                if !entry.visited {
                    self.bytes -= entry.bytes;
                }
                entry.visited
            });
        }
    }

    pub(crate) fn check(
        &mut self,
        analysis: &mut Analysis<'_>,
        cue: &Cue,
        index: usize,
        format: Format,
        out: &mut Vec<Finding>,
        mut checkpoint: impl FnMut() -> Result<()>,
    ) -> Result<()> {
        let entering = usize::from(analysis.sentence_start);
        if let Some(entry) = self.entries[entering].get_mut(cue.text()) {
            entry.visited = true;
            for f in &entry.findings {
                checkpoint()?;
                push(out, cue, index, f.rule, &f.explanation, f.detail.clone())?;
            }
            analysis.coverage.checked_words += entry.checked_words;
            analysis.coverage.skipped_words += entry.skipped_words;
            analysis.coverage.skipped_cues += entry.skipped_cues;
            analysis.sentence_start = entry.sentence_start;
            return Ok(());
        }
        let before = analysis.coverage.clone();
        let first = out.len();
        analysis.check(cue, index, format, out, checkpoint)?;
        // A scan over capacity must not evict entries it can reuse on its next pass.
        if self.len() >= MAX_ENTRIES {
            return Ok(());
        }
        let findings = &out[first..];
        let bytes = cue.text().len()
            + size_of::<Entry>()
            + findings
                .iter()
                .map(|f| {
                    size_of::<Advisory>()
                        + f.explanation.len()
                        + f.language.as_ref().map_or(0, |d| {
                            d.word.len()
                                + d.manual_reason.as_ref().map_or(0, String::len)
                                + d.replacement.as_ref().map_or(0, String::len)
                        })
                })
                .sum::<usize>();
        if self.bytes + bytes > MAX_BYTES {
            return Ok(());
        }
        self.entries[entering].insert(
            cue.text().into(),
            Entry {
                findings: findings
                    .iter()
                    .map(|f| Advisory {
                        rule: f.rule,
                        explanation: f.explanation.clone(),
                        detail: f.language.clone(),
                    })
                    .collect(),
                checked_words: analysis.coverage.checked_words - before.checked_words,
                skipped_words: analysis.coverage.skipped_words - before.skipped_words,
                skipped_cues: analysis.coverage.skipped_cues - before.skipped_cues,
                sentence_start: analysis.sentence_start,
                bytes,
                visited: true,
            },
        );
        self.bytes += bytes;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language::Settings;
    use subtitle_core::{CueId, Milliseconds};

    #[test]
    fn completed_scans_sweep_unused_entries_and_respect_both_caps() {
        let mut cache = LanguageCache::default();
        let words = Lexicon::default();
        let settings = Settings {
            typography: true,
            ..Default::default()
        };
        let mut analysis = Analysis::new(&settings, &words);
        let start = Milliseconds::new(0).unwrap();
        let end = Milliseconds::new(1000).unwrap();
        let mut insert = |cache: &mut LanguageCache, text| {
            let cue = Cue::new(CueId(1), start, end, text).unwrap();
            cache
                .check(&mut analysis, &cue, 0, Format::Srt, &mut Vec::new(), || {
                    Ok(())
                })
                .unwrap();
        };
        cache.prepare("profile", Format::Srt, &words);
        for i in 0..=MAX_ENTRIES {
            insert(&mut cache, i.to_string());
        }
        assert_eq!(cache.len(), MAX_ENTRIES);
        assert!(cache.entries[1].contains_key("0"));
        cache.finish_scan();
        cache.prepare("profile", Format::Srt, &words);
        insert(&mut cache, "0".into());
        cache.finish_scan();
        assert_eq!(cache.len(), 1);
        // Oversized-for-analysis cues still contribute a cached coverage finding.
        for i in 0..9 {
            insert(&mut cache, format!("{i}{}", "x".repeat(1024 * 1024)));
        }
        assert!(cache.bytes <= MAX_BYTES);
        assert!(cache.len() < 9);
        cache.prepare("profile", Format::Ass, &words);
        assert_eq!(cache.len(), 0);
        insert(&mut cache, "Hello".into());
        cache.prepare("another profile", Format::Ass, &words);
        assert_eq!(cache.len(), 0);
    }
}
