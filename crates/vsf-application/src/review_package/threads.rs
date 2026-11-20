//! Immutable discussion events. Their wire representation also defines identity.
use super::*;
use crate::comparison::{Change, CueSnapshot, Kind};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_THREADS: usize = 10_000;
pub const MAX_MESSAGES: usize = 100_000;
pub const MAX_TEXT_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct SnapshotCue {
    #[cfg_attr(feature="specta", specta(type=f64))]
    pub start_ms: u64,
    #[cfg_attr(feature="specta", specta(type=f64))]
    pub end_ms: u64,
    pub text: String,
}
impl From<&CueSnapshot> for SnapshotCue {
    fn from(c: &CueSnapshot) -> Self {
        Self {
            start_ms: c.start_ms,
            end_ms: c.end_ms,
            text: c.text.clone(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Anchor {
    Change {
        change_kind: Kind,
        change_key: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        occurrence: Option<u32>,
        original: Vec<SnapshotCue>,
        revised: Vec<SnapshotCue>,
    },
    Cue {
        revised: [SnapshotCue; 1],
    },
}
impl Anchor {
    pub fn from_change(c: &Change, occurrence: u32) -> Self {
        if c.kind == Kind::Unchanged {
            return Self::Cue {
                revised: [(&c.revised[0]).into()],
            };
        }
        Self::Change {
            change_kind: c.kind,
            change_key: c.key.clone(),
            occurrence: (occurrence != 0).then_some(occurrence),
            original: c.original.iter().map(Into::into).collect(),
            revised: c.revised.iter().map(Into::into).collect(),
        }
    }
    pub fn sides(&self) -> (&[SnapshotCue], &[SnapshotCue]) {
        match self {
            Self::Change {
                original, revised, ..
            } => (original, revised),
            Self::Cue { revised } => (&[], revised),
        }
    }
    fn validate(&self) -> Result<usize> {
        let (a, b) = self.sides();
        if let Self::Change {
            change_kind,
            change_key,
            ..
        } = self
        {
            let valid = match change_kind {
                Kind::Unchanged => false,
                Kind::Removed => (a.len(), b.len()) == (1, 0),
                Kind::Added => (a.len(), b.len()) == (0, 1),
                Kind::Split => a.len() == 1 && (2..=3).contains(&b.len()),
                Kind::Merged => b.len() == 1 && (2..=3).contains(&a.len()),
                _ => a.len() == 1 && b.len() == 1,
            };
            if !valid {
                return Err("Invalid change anchor shape".into());
            }
            fn side(s: &[SnapshotCue]) -> Vec<(u64, u64, &str)> {
                s.iter()
                    .map(|c| (c.start_ms, c.end_ms, c.text.as_str()))
                    .collect()
            }
            if *change_key != crate::comparison::content_key(*change_kind, &side(a), &side(b)) {
                return Err("Anchor change key mismatch".into());
            }
        }
        let mut size = 0;
        for cue in a.iter().chain(b) {
            let time = |ms| subtitle_core::Milliseconds::new(ms).map_err(|e| e.to_string());
            subtitle_core::Cue::validate(time(cue.start_ms)?, time(cue.end_ms)?, &cue.text)
                .map_err(|e| e.to_string())?;
            size += cue.text.len();
        }
        Ok(size)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum Event {
    Comment,
    Resolve,
    Reopen,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct Message {
    pub id: String,
    pub author: String,
    #[cfg_attr(feature="specta",specta(type=f64))]
    pub at_ms: u64,
    #[serde(rename = "type")]
    pub event: Event,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}
fn hash_json(value: &impl Serialize) -> String {
    digest(&serde_json::to_vec(value).expect("scalar identity fields serialize"))
}
fn text_controls(text: &str) -> Result<()> {
    if text
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\r' | '\n' | '\t'))
    {
        return Err("Control characters in review text".into());
    }
    Ok(())
}
pub fn validate_name(name: &str) -> Result<()> {
    if name.trim().is_empty() || name.chars().count() > 100 || name.chars().any(char::is_control) {
        return Err("Review names require 1–100 characters without controls".into());
    }
    Ok(())
}
impl Message {
    pub fn new(
        thread: &str,
        author: String,
        at_ms: u64,
        event: Event,
        text: Option<String>,
    ) -> Result<Self> {
        validate_name(&author)?;
        validate_time(at_ms)?;
        match (&text, event) {
            (Some(s), Event::Comment) if !s.trim().is_empty() && s.chars().count() <= 10_000 => {
                text_controls(s)?
            }
            (None, Event::Resolve | Event::Reopen) => (),
            _ => {
                return Err(
                    "A comment needs 1–10,000 characters; status events have no text".into(),
                );
            }
        }
        Ok(Self {
            id: hash_json(&(thread, &author, at_ms, event, &text)),
            author,
            at_ms,
            event,
            text,
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct Thread {
    pub id: String,
    pub anchor: Anchor,
    pub first_message_id: String,
    pub messages: Vec<Message>,
}
impl Thread {
    pub fn new(anchor: Anchor, author: String, at_ms: u64, text: String) -> Result<Self> {
        let id = hash_json(&(&anchor, &author, at_ms, &text));
        let first = Message::new(&id, author, at_ms, Event::Comment, Some(text))?;
        Ok(Self {
            id,
            anchor,
            first_message_id: first.id.clone(),
            messages: vec![first],
        })
    }
    pub fn resolved(&self) -> bool {
        self.messages
            .iter()
            .filter(|m| m.event != Event::Comment)
            .max_by_key(|m| (m.at_ms, &m.id))
            .is_some_and(|m| m.event == Event::Resolve)
    }
    pub fn sort_messages(&mut self) {
        self.messages
            .sort_by(|a, b| (a.at_ms, &a.id).cmp(&(b.at_ms, &b.id)))
    }
}
pub fn validate_threads(threads: &[Thread]) -> Result<()> {
    if threads.len() > MAX_THREADS {
        return Err("Review exceeds 10,000 threads".into());
    }
    let mut count = 0;
    let mut bytes = 0;
    let mut ids = BTreeSet::new();
    for t in threads {
        count += t.messages.len();
        if count > MAX_MESSAGES {
            return Err("Review exceeds 100,000 messages".into());
        }
        bytes += t.anchor.validate()?;
        for m in &t.messages {
            bytes += m.author.len() + m.text.as_ref().map_or(0, String::len)
        }
        if bytes > MAX_TEXT_BYTES {
            return Err("Review exceeds 16 MiB of thread text".into());
        }
        if !ids.insert(&t.id) {
            return Err("Duplicate thread ID".into());
        }
        let root = t
            .messages
            .iter()
            .find(|m| m.id == t.first_message_id && m.event == Event::Comment)
            .ok_or("Missing first comment")?;
        if hash_json(&(&t.anchor, &root.author, root.at_ms, &root.text)) != t.id {
            return Err("Thread ID mismatch".into());
        }
        for m in &t.messages {
            let expected = Message::new(&t.id, m.author.clone(), m.at_ms, m.event, m.text.clone())?;
            if m.id != expected.id || !ids.insert(&m.id) {
                return Err("Duplicate or mismatched message ID".into());
            }
        }
    }
    Ok(())
}
/// Inputs are validated captures. Canonical order is independent of arrival.
pub fn merge(a: &[Thread], b: &[Thread]) -> Result<Vec<Thread>> {
    let mut groups = BTreeMap::new();
    for t in a.iter().chain(b) {
        let (_, events) = groups.entry(&t.id).or_insert_with(|| (t, BTreeMap::new()));
        for m in &t.messages {
            events.insert(&m.id, m);
        }
    }
    let result: Vec<_> = groups
        .into_values()
        .map(|(t, events)| {
            let mut merged = Thread {
                id: t.id.clone(),
                anchor: t.anchor.clone(),
                first_message_id: t.first_message_id.clone(),
                messages: events.into_values().cloned().collect(),
            };
            merged.sort_messages();
            merged
        })
        .collect();
    validate_threads(&result)?;
    Ok(result)
}
