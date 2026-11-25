//! Cue-local language work from successful accepted quality evaluations.
use super::{Analysis, Detail, Lexicon, analysis::push};
use crate::quality::{Finding, Result, Rule};
use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};
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
}

/// Owned by one evaluator at a time. The two maps distinguish the entering
/// sentence state and allow looking up source text without allocating a key.
#[derive(Default)]
pub struct LanguageCache {
    scope: Option<(String, Format, Lexicon)>,
    entries: [HashMap<Arc<str>, Entry>; 2],
    insertion_order: VecDeque<(usize, Arc<str>)>,
    bytes: usize,
}

impl LanguageCache {
    pub(crate) fn prepare(&mut self, fingerprint: &str, format: Format, lexicon: &Lexicon) {
        if self
            .scope
            .as_ref()
            .is_some_and(|(hash, f, words)| hash == fingerprint && *f == format && words == lexicon)
        {
            return;
        }
        *self = Self {
            scope: Some((fingerprint.into(), format, lexicon.clone())),
            ..Self::default()
        };
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
        if let Some(entry) = self.entries[entering].get(cue.text()) {
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
        if bytes > MAX_BYTES {
            return Ok(());
        }
        while self.insertion_order.len() >= MAX_ENTRIES || self.bytes + bytes > MAX_BYTES {
            let (state, text) = self.insertion_order.pop_front().expect("cache is nonempty");
            let evicted = self.entries[state]
                .remove(&text)
                .expect("queued cache entry");
            self.bytes -= evicted.bytes;
        }
        let text: Arc<str> = cue.text().into();
        self.insertion_order.push_back((entering, text.clone()));
        self.entries[entering].insert(
            text,
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
    fn fifo_bounds_and_scope_changes_evict_entries() {
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
        assert_eq!(cache.insertion_order.len(), MAX_ENTRIES);
        assert!(!cache.entries[1].contains_key("0"));
        // Oversized-for-analysis cues still contribute a cached coverage finding.
        for i in 0..9 {
            insert(&mut cache, format!("{i}{}", "x".repeat(1024 * 1024)));
        }
        assert!(cache.bytes <= MAX_BYTES);
        assert!(cache.insertion_order.len() < 9);
        cache.prepare("profile", Format::Ass, &words);
        assert!(cache.insertion_order.is_empty());
        insert(&mut cache, "Hello".into());
        cache.prepare("another profile", Format::Ass, &words);
        assert!(cache.insertion_order.is_empty());
    }
}
