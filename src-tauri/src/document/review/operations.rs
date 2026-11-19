use super::*;
use std::collections::{BTreeMap, HashSet};
pub(super) fn matches(change: &engine::Change, filter: Option<engine::Kind>) -> bool {
    change.kind != engine::Kind::Unchanged
        && filter.is_none_or(|f| match f {
            engine::Kind::Text => matches!(
                change.kind,
                engine::Kind::Text | engine::Kind::TextAndTiming
            ),
            engine::Kind::Timing => matches!(
                change.kind,
                engine::Kind::Timing | engine::Kind::TextAndTiming
            ),
            engine::Kind::Split | engine::Kind::Merged => {
                matches!(change.kind, engine::Kind::Split | engine::Kind::Merged)
            }
            _ => change.kind == f,
        })
}
pub(super) struct Stop<'a> {
    pub first: usize,
    end: usize,
    pub block: Option<&'a engine::ShiftBlock>,
}
// One sequence drives paging and navigation. Only its first visible row owns the block control.
pub(super) fn stops<'a>(
    comparison: &'a engine::Comparison,
    filter: Option<engine::Kind>,
    expanded: &[u32],
) -> Vec<Stop<'a>> {
    let expanded: HashSet<_> = expanded.iter().copied().collect();
    let mut blocks = comparison.blocks.iter().peekable();
    let mut result = Vec::new();
    let mut index = 0;
    while index < comparison.changes.len() {
        let block = blocks.next_if(|b| b.first as usize == index);
        let end = block
            .filter(|b| !expanded.contains(&b.first))
            .map_or(index + 1, |b| b.end as usize);
        if matches(&comparison.changes[index], filter) {
            result.push(Stop {
                first: index,
                end,
                block,
            });
        }
        index = end;
    }
    result
}
impl Store {
    fn comparison(&mut self, key: ReviewKey) -> Result<&engine::Comparison> {
        self.check_review_key(key)?;
        let job = self
            .review
            .job
            .as_mut()
            .filter(|j| j.key == key)
            .ok_or("Wait for the current comparison")?;
        job.refresh();
        job.result
            .as_ref()
            .ok_or("Wait for the current comparison".into())
    }
    pub(super) fn revert_review(
        &mut self,
        key: ReviewKey,
        selected: Option<&str>,
        source_index: Option<u32>,
        filter: Option<engine::Kind>,
        block: Option<u32>,
    ) -> Result<()> {
        let result = self.comparison(key)?;
        let scope = match block {
            Some(first) => {
                let b = result
                    .blocks
                    .iter()
                    .find(|b| b.first == first)
                    .ok_or("That shift block no longer exists")?;
                &result.changes[first as usize..b.end as usize]
            }
            None => result.changes.as_slice(),
        };
        let changes: Vec<_> = scope
            .iter()
            .filter(|c| {
                matches(c, filter)
                    && selected.is_none_or(|s| s == c.key)
                    && source_index.is_none_or(|index| {
                        c.original
                            .first()
                            .or(c.revised.first())
                            .is_some_and(|cue| cue.source_index == index)
                    })
            })
            .cloned()
            .collect();
        if changes.is_empty() {
            return Err("That change no longer exists".into());
        }
        if selected.is_some() && changes.len() > 1 {
            return Err(
                "Identical duplicate changes share this key; use Revert All in Filter".into(),
            );
        }
        let loaded = self.loaded_mut()?;
        let original = &loaded
            .review
            .reference
            .reference
            .as_ref()
            .ok_or("Review has ended")?
            .reference;
        let current = loaded.editor.document().cues();
        let mut removed = HashSet::new();
        let mut inserts: BTreeMap<usize, Vec<subtitle_core::CueReplacement>> = BTreeMap::new();
        for change in &changes {
            for cue in &change.revised {
                removed.insert(cue.cue_id);
            }
            let index = change
                .revised
                .iter()
                .map(|c| c.source_index as usize)
                .min()
                .unwrap_or_else(|| {
                    let start = change.original.first().map_or(0, |c| c.start_ms);
                    current
                        .iter()
                        .position(|c| c.start().get() >= start)
                        .unwrap_or(current.len())
                });
            for (i, cue) in change.original.iter().enumerate() {
                let source = &original.cues()[cue.source_index as usize];
                inserts
                    .entry(index)
                    .or_default()
                    .push(subtitle_core::CueReplacement {
                        id: change.revised.get(i).map(|c| CueId(c.cue_id)),
                        start: source.start(),
                        end: source.end(),
                        text: source.text().into(),
                        metadata: source.metadata().cloned(),
                    });
            }
        }
        let mut cues = Vec::new();
        for (index, cue) in current.iter().enumerate() {
            cues.extend(inserts.remove(&index).unwrap_or_default());
            if !removed.contains(&cue.id().0) {
                cues.push(subtitle_core::CueReplacement {
                    id: Some(cue.id()),
                    start: cue.start(),
                    end: cue.end(),
                    text: cue.text().into(),
                    metadata: cue.metadata().cloned(),
                });
            }
        }
        cues.extend(inserts.remove(&current.len()).unwrap_or_default());
        loaded.apply_edit(key.revision.get(), Edit::ReplaceCues { cues })?;
        Ok(())
    }
    pub(super) fn review_projection(
        &mut self,
        key: ReviewKey,
        ids: &[SafeU64],
        start: u64,
        end: u64,
    ) -> Result<ReviewReply> {
        if ids.len() > 200 || start > end {
            return Err("Invalid comparison projection window".into());
        }
        let ids: HashSet<_> = ids.iter().map(|id| id.get()).collect();
        let rows = self
            .comparison(key)?
            .changes
            .iter()
            .filter(|c| {
                c.kind != engine::Kind::Unchanged
                    && (c.revised.iter().any(|cue| ids.contains(&cue.cue_id))
                        || c.kind == engine::Kind::Removed
                            && c.original
                                .iter()
                                .any(|cue| cue.start_ms <= end && cue.end_ms >= start))
            })
            .take(200)
            .cloned()
            .collect();
        let mut reply = self.review_reply(None, 0, 1, None)?;
        reply.rows = rows;
        Ok(reply)
    }
    pub(super) fn navigate_review(
        &mut self,
        key: ReviewKey,
        from: Option<&str>,
        cue_id: Option<u64>,
        forward: bool,
        filter: Option<engine::Kind>,
        expanded: &[u32],
    ) -> Result<ReviewReply> {
        let cue_time = self
            .loaded
            .as_ref()
            .and_then(|l| {
                l.editor
                    .document()
                    .cues()
                    .iter()
                    .find(|c| Some(c.id().0) == cue_id)
            })
            .map(|c| c.start().get());
        let result = self.comparison(key)?;
        let stops = stops(result, filter, expanded);
        let rows: Vec<_> = stops.iter().map(|s| &result.changes[s.first]).collect();
        let current = cue_id
            .and_then(|id| {
                stops.iter().position(|s| {
                    result.changes[s.first..s.end]
                        .iter()
                        .any(|c| c.revised.iter().any(|cue| cue.cue_id == id))
                })
            })
            .or_else(|| {
                from.and_then(|k| {
                    rows.iter()
                        .position(|c| c.key == k && (cue_id.is_none() || c.revised.is_empty()))
                })
            });
        let next = match current {
            Some(i) => {
                if forward {
                    i.checked_add(1).filter(|i| *i < rows.len())
                } else {
                    i.checked_sub(1)
                }
            }
            None => {
                let time = |c: &&engine::Change| {
                    c.revised
                        .first()
                        .or(c.original.first())
                        .map_or(0, |c| c.start_ms)
                };
                if forward {
                    rows.iter()
                        .position(|c| cue_time.is_none_or(|t| time(c) >= t))
                } else {
                    rows.iter()
                        .rposition(|c| cue_time.is_none_or(|t| time(c) <= t))
                }
            }
        };
        let mut reply = self.review_page(
            None,
            next.unwrap_or(0) as u32,
            if next.is_some() { 1 } else { 0 },
            filter,
            expanded,
        )?;
        reply.position = next.map(|i| i as u32 + 1);
        Ok(reply)
    }
}
impl Documents {
    pub(super) fn export_review(
        &self,
        key: ReviewKey,
        path: &Path,
        include_unchanged: bool,
        date: &str,
    ) -> Result<ReviewReply> {
        let (comparison, original, revised) = {
            let mut store = self.0.lock().map_err(|_| "Document unavailable")?;
            let comparison = store.comparison(key)?.clone();
            let loaded = store.loaded.as_ref().ok_or("Open subtitles")?;
            loaded.translation.protect(path)?;
            loaded.review.reference.protect(path)?;
            let original = loaded
                .review
                .reference
                .reference
                .as_ref()
                .ok_or("Attach an original")?
                .origin
                .path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let revised = loaded
                .path
                .as_ref()
                .and_then(|p| p.file_name())
                .map_or_else(|| "Untitled".into(), |s| s.to_string_lossy().into_owned());
            (comparison, original, revised)
        };
        let html = engine::html(
            &comparison,
            &engine::ReportOptions {
                original_name: &original,
                revised_name: &revised,
                date,
                include_unchanged,
            },
            &engine::ExtraSections::default(),
        );
        let output = vsf_application::new_output::resolve(path).map_err(|e| e.to_string())?;
        let prepared = vsf_application::atomic_file::prepare(&output, html.as_bytes())?;
        let mut store = self.0.lock().map_err(|_| "Document unavailable")?;
        store.check_review_key(key)?;
        prepared.commit_new()?;
        store.review_reply(None, 0, 100, None)
    }
}
