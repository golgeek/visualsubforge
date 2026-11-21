use super::*;
use std::collections::{BTreeMap, BTreeSet};
use vsf_application::review_package::{self as package, Anchor, Event, Message, Thread};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(deny_unknown_fields)]
pub struct Discussion {
    pub(super) threads: Vec<Thread>,
    pub(super) exported: BTreeSet<String>,
    pub(super) round: u32,
}
impl Discussion {
    pub(super) fn same_anchors(&self, other: &Self) -> bool {
        self.threads
            .iter()
            .map(|t| &t.id)
            .eq(other.threads.iter().map(|t| &t.id))
    }
    pub(super) fn empty(&self) -> bool {
        self.threads.is_empty() && self.round == 0 && self.exported.is_empty()
    }
    pub(super) fn validate(&self) -> Result<()> {
        package::validate_threads(&self.threads)?;
        let ids: BTreeSet<_> = self
            .threads
            .iter()
            .flat_map(|t| t.messages.iter().map(|m| &m.id))
            .collect();
        if self.exported.iter().any(|id| !ids.contains(id)) {
            return Err("Unknown exported message ID".into());
        }
        Ok(())
    }
}
#[derive(Deserialize, specta::Type)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum CommentEdit {
    Post {
        change_key: String,
        source_index: u32,
        text: String,
    },
    Reply {
        thread_id: String,
        event: Event,
        text: Option<String>,
    },
    Delete {
        thread_id: String,
        message_id: String,
    },
}
#[derive(Default, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DiscussionPage {
    pub rows: BTreeMap<String, Vec<Thread>>,
    pub detached: Vec<Thread>,
    pub exported: BTreeSet<String>,
    pub block_counts: BTreeMap<u32, u32>,
    pub open: u32,
}
pub(super) fn row_id(c: &engine::Change) -> String {
    format!(
        "{}/{}",
        c.key,
        c.original
            .first()
            .or(c.revised.first())
            .map_or(0, |q| q.source_index)
    )
}
impl Store {
    pub(super) fn comment(
        &mut self,
        key: ReviewKey,
        author: String,
        at_ms: u64,
        edit: CommentEdit,
    ) -> Result<()> {
        self.check_review_key(key)?;
        let mut discussion = self.loaded_mut()?.review.discussion.clone();
        match edit {
            CommentEdit::Post {
                change_key,
                source_index,
                text,
            } => {
                let (occurrence, c) = self
                    .comparison(key)?
                    .changes
                    .iter()
                    .filter(|c| c.key == change_key)
                    .enumerate()
                    .find(|(_, c)| {
                        c.original
                            .first()
                            .or(c.revised.first())
                            .is_some_and(|q| q.source_index == source_index)
                    })
                    .ok_or("The commented cue changed")?;
                let t = Thread::new(
                    Anchor::from_change(c, occurrence as u32),
                    author,
                    at_ms,
                    text,
                )?;
                if !discussion.threads.iter().any(|other| other.id == t.id) {
                    discussion.threads.push(t)
                }
            }
            CommentEdit::Reply {
                thread_id,
                event,
                text,
            } => {
                let t = discussion
                    .threads
                    .iter_mut()
                    .find(|t| t.id == thread_id)
                    .ok_or("Unknown thread")?;
                let m = Message::new(&thread_id, author, at_ms, event, text)?;
                if !t.messages.iter().any(|other| other.id == m.id) {
                    t.messages.push(m);
                    t.sort_messages()
                }
            }
            CommentEdit::Delete {
                thread_id,
                message_id,
            } => {
                if discussion.exported.contains(&message_id) {
                    return Err("Exported messages are immutable".into());
                }
                let t = discussion
                    .threads
                    .iter_mut()
                    .find(|t| t.id == thread_id)
                    .ok_or("Unknown thread")?;
                if message_id == t.first_message_id && t.messages.len() > 1 {
                    return Err("The first comment has replies".into());
                }
                let index = t
                    .messages
                    .iter()
                    .position(|m| m.id == message_id)
                    .ok_or("Unknown message")?;
                t.messages.remove(index);
                discussion.threads.retain(|t| !t.messages.is_empty());
            }
        }
        discussion.threads.sort_by(|a, b| a.id.cmp(&b.id));
        discussion.validate()?;
        let anchors_changed = !discussion.same_anchors(&self.loaded_mut()?.review.discussion);
        self.advance_discussion(key, anchors_changed)?;
        let context = &mut self.loaded_mut()?.review;
        context.discussion = discussion;
        context.reference.dirty = true;
        Ok(())
    }
}
/// The worker's indices are valid only for its matching revision and review epoch.
pub(super) struct ThreadIndex {
    attached: BTreeMap<usize, Vec<usize>>,
    pub(super) detached: Vec<usize>,
}
impl ThreadIndex {
    pub(super) fn new(positions: Vec<Option<usize>>) -> Self {
        let mut index = Self {
            attached: BTreeMap::new(),
            detached: Vec::new(),
        };
        for (t, p) in positions.into_iter().enumerate() {
            if let Some(p) = p {
                index.attached.entry(p).or_default().push(t)
            } else {
                index.detached.push(t)
            }
        }
        index
    }
    pub(super) fn row(&self, index: usize) -> &[usize] {
        self.attached.get(&index).map_or(&[], Vec::as_slice)
    }
    pub(super) fn project(
        &self,
        index: usize,
        c: &engine::Change,
        d: &Discussion,
        page: &mut DiscussionPage,
    ) {
        let ids = self.row(index);
        if !ids.is_empty() {
            let ts = ids
                .iter()
                .map(|&i| d.threads[i].clone())
                .collect::<Vec<_>>();
            for t in &ts {
                page.include_exported(t, d)
            }
            page.rows.insert(row_id(c), ts);
        }
    }
    pub(super) fn block_count(&self, b: &engine::ShiftBlock) -> u32 {
        self.attached
            .range(b.first as usize..b.end as usize)
            .map(|(_, ts)| ts.len() as u32)
            .sum()
    }
    pub(super) fn rows(&self, d: &Discussion, open: bool) -> Vec<(Option<usize>, Vec<usize>)> {
        let filter = |ids: &[usize]| {
            ids.iter()
                .copied()
                .filter(|&i| !open || !d.threads[i].resolved())
                .collect::<Vec<_>>()
        };
        let mut rows: Vec<_> = self
            .attached
            .iter()
            .filter_map(|(&i, ts)| {
                let ts = filter(ts);
                (!ts.is_empty()).then_some((Some(i), ts))
            })
            .collect();
        rows.extend(filter(&self.detached).into_iter().map(|i| (None, vec![i])));
        rows
    }
}
impl DiscussionPage {
    pub(super) fn include_exported(&mut self, t: &Thread, d: &Discussion) {
        self.exported.extend(
            t.messages
                .iter()
                .filter(|m| d.exported.contains(&m.id))
                .map(|m| m.id.clone()),
        )
    }
}
impl Store {
    pub(super) fn thread_page(
        &mut self,
        key: ReviewKey,
        open: bool,
        offset: u32,
        limit: u32,
        navigate: Option<(Option<String>, bool)>,
    ) -> Result<ReviewReply> {
        self.check_review_key(key)?;
        if !(1..=200).contains(&limit) {
            return Err("Discussion pages require 1–200 groups".into());
        }
        let mut reply = self.review_reply(None, 0, 0, None)?;
        let Some(job) = self.review.job.as_ref().filter(|j| j.key == key) else {
            return Ok(reply);
        };
        let (Some(result), Some(index), Some(page)) =
            (&job.result, &job.threads, &mut reply.discussion)
        else {
            return Ok(reply);
        };
        let d = &self
            .loaded
            .as_ref()
            .ok_or("Review has ended")?
            .review
            .discussion;
        let rows = index.rows(d, open);
        reply.total = rows.len() as u32;
        let mut start = offset as usize;
        if let Some((from, forward)) = navigate {
            let stops: Vec<_> = rows
                .iter()
                .enumerate()
                .flat_map(|(row, (_, ts))| ts.iter().map(move |&t| (row, t)))
                .collect();
            let pos = from.and_then(|id| stops.iter().position(|&(_, t)| d.threads[t].id == id));
            let next = match (pos, forward) {
                (None, true) => Some(0),
                (None, false) => stops.len().checked_sub(1),
                (Some(n), true) => Some(n + 1),
                (Some(n), false) => n.checked_sub(1),
            };
            let Some(&(row, t)) = next.and_then(|i| stops.get(i)) else {
                return Ok(reply);
            };
            reply.thread_id = Some(d.threads[t].id.clone());
            start = row;
            reply.position = Some(row as u32 + 1);
        }
        for (pos, ts) in rows.iter().skip(start).take(limit as usize) {
            if let Some(i) = pos {
                let c = &result.changes[*i];
                index.project(*i, c, d, page);
                reply.rows.push(c.clone())
            } else {
                for &t in ts {
                    page.include_exported(&d.threads[t], d);
                    page.detached.push(d.threads[t].clone())
                }
            }
        }
        Ok(reply)
    }
}
