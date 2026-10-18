//! Immutable attachment inventory and reviewed font changes. Payload parsing is
//! pure; the desktop reads user-selected files on its bounded review worker.
use super::fonts::{self, FontInfo};
use serde::Serialize;
use sha2::{Digest, Sha256};
use subtitle_core::{Document, Result};
use subtitle_formats::{Source, ass::AttachmentKind};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct AttachmentEntry {
    pub id: u32,
    pub name: String,
    pub kind: AttachmentType,
    pub encoded_bytes: u32,
    pub decoded_bytes: Option<u32>,
    pub sha256: Option<String>,
    pub font: Option<FontInfo>,
    pub problem: Option<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum AttachmentType {
    Font,
    Graphic,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct AttachmentInventory {
    pub entries: Vec<AttachmentEntry>,
    pub total: u32,
    pub complete: bool,
    /// Conservative decoded-size upper bound for the displayed font records.
    /// A partial inventory must never authorize a change.
    pub font_bytes_upper_bound: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum AttachmentAction {
    Add,
    Replace,
    Remove,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct AttachmentSummary {
    pub action: AttachmentAction,
    pub before: Option<AttachmentEntry>,
    pub after: Option<AttachmentEntry>,
    pub document_bytes: u32,
    pub preview_size_exceeded: bool,
}
pub enum FontEdit<'a> {
    Put {
        replace: Option<u32>,
        name: &'a str,
        bytes: &'a [u8],
    },
    Remove {
        id: u32,
    },
}
pub struct Proposal {
    document: Document,
    source: Source,
    pub summary: AttachmentSummary,
    pub changed: bool,
}
impl Proposal {
    pub fn document(&self) -> &Document {
        &self.document
    }
    pub fn source(&self) -> &Source {
        &self.source
    }
    pub fn into_parts(self) -> (Document, Source) {
        (self.document, self.source)
    }
}

pub fn inspect(
    source: &Source,
    mut checkpoint: impl FnMut() -> Result<()>,
) -> Result<AttachmentInventory> {
    checkpoint()?;
    let script = source
        .ass()
        .ok_or("Attachment inspection requires ASS/SSA subtitles")?;
    let inventory = script.attachments(200);
    let mut result = AttachmentInventory {
        complete: inventory.total == inventory.entries.len(),
        total: number(inventory.total)?,
        entries: Vec::with_capacity(inventory.entries.len()),
        font_bytes_upper_bound: 0,
    };
    for item in inventory.entries {
        checkpoint()?;
        let mut entry = AttachmentEntry {
            id: number(item.id)?,
            name: item.name,
            kind: match item.kind {
                AttachmentKind::Font => AttachmentType::Font,
                AttachmentKind::Graphic => AttachmentType::Graphic,
            },
            encoded_bytes: number(item.encoded_bytes)?,
            decoded_bytes: None,
            sha256: None,
            font: None,
            problem: item.problem.map(str::to_owned),
        };
        if item.kind == AttachmentKind::Font {
            let upper = item
                .encoded_bytes
                .checked_mul(3)
                .ok_or("Font size accounting overflow")?;
            result.font_bytes_upper_bound = result
                .font_bytes_upper_bound
                .checked_add(number(upper / 4)?)
                .ok_or("Font size accounting overflow")?;
            if entry.problem.is_none() {
                // Record interpretation errors remain visible; cancellation stays
                // an operation failure rather than becoming an unsupported row.
                let decoded = parse_with_checkpoint(&mut checkpoint, |check| {
                    fonts::decode(script, item.id, check)
                })?;
                match decoded {
                    Ok(bytes) => {
                        entry.decoded_bytes = Some(number(bytes.len())?);
                        entry.sha256 = Some(format!("{:x}", Sha256::digest(&bytes)));
                        let info = parse_with_checkpoint(&mut checkpoint, |check| {
                            fonts::inspect(&bytes, check)
                        })?;
                        match info {
                            Ok(info) => entry.font = Some(info),
                            Err(error) => entry.problem = Some(error),
                        }
                    }
                    Err(error) => entry.problem = Some(error),
                }
            }
        } else {
            entry
                .problem
                .get_or_insert("Graphic attachments are preserved read-only".into());
        }
        result.entries.push(entry);
    }
    checkpoint()?;
    Ok(result)
}

pub fn propose(
    document: &Document,
    source: &Source,
    edit: FontEdit<'_>,
    mut checkpoint: impl FnMut() -> Result<()>,
) -> Result<Proposal> {
    checkpoint()?;
    if document.cues().len() > 100_000 {
        return Err("Font authoring supports at most 100,000 cues".into());
    }
    let script = source
        .ass()
        .ok_or("Font authoring requires ASS/SSA subtitles")?;
    let inventory = inspect(source, &mut checkpoint)?;
    if !inventory.complete {
        return Err("This document has more than 200 attachment records; its complete inventory cannot be reviewed for font management".into());
    }
    let selected = |id| {
        inventory.entries.iter().find(|e| e.id == id).ok_or_else(|| "Font attachment no longer exists".to_owned()).and_then(|entry| {
        if entry.kind != AttachmentType::Font || entry.font.is_none() || entry.problem.is_some() { return Err("This attachment is unsupported or malformed and remains preserved read-only".into()); }
        Ok(entry.clone())
    })
    };
    let (action, before, next, changed) = match edit {
        FontEdit::Remove { id } => {
            let before = selected(id)?;
            (
                AttachmentAction::Remove,
                Some(before),
                script.without_font(id as usize)?,
                true,
            )
        }
        FontEdit::Put {
            replace,
            name,
            bytes,
        } => {
            if !name.rsplit_once('.').is_some_and(|(_, e)| {
                e.eq_ignore_ascii_case("ttf") || e.eq_ignore_ascii_case("otf")
            }) {
                return Err("Use a .ttf or .otf font attachment filename".into());
            }
            if inventory
                .entries
                .iter()
                .any(|entry| entry.kind == AttachmentType::Font && entry.problem.is_some())
            {
                return Err("Adding or replacing fonts requires a fully supported font inventory; unsupported font records remain preserved read-only".into());
            }
            fonts::inspect(bytes, &mut checkpoint)?;
            let before = replace.map(selected).transpose()?;
            let old_upper = before
                .as_ref()
                .map_or(0, |b| (b.encoded_bytes as usize) * 3 / 4);
            if (inventory.font_bytes_upper_bound as usize)
                .saturating_sub(old_upper)
                .checked_add(bytes.len())
                .is_none_or(|total| total > fonts::MAX_TOTAL_FONT_BYTES)
            {
                return Err("Font management limits aggregate decoded font data to 8 MiB".into());
            }
            let hash = format!("{:x}", Sha256::digest(bytes));
            let changed = before
                .as_ref()
                .is_none_or(|b| b.name != name || b.sha256.as_ref() != Some(&hash));
            let next = if changed {
                let encoded = fonts::encode(bytes, &mut checkpoint)?;
                script.with_font(replace.map(|id| id as usize), name, &encoded)?
            } else {
                script.clone()
            };
            (
                if replace.is_some() {
                    AttachmentAction::Replace
                } else {
                    AttachmentAction::Add
                },
                before,
                next,
                changed,
            )
        }
    };
    let next = Source::Ass(next);
    crate::subtitle_file::validate_document(document, &next)?;
    let after = if action == AttachmentAction::Remove {
        None
    } else {
        let inspected = inspect(&next, &mut checkpoint)?;
        let id = before.as_ref().map(|e| e.id);
        Some(if let Some(id) = id {
            inspected
                .entries
                .into_iter()
                .find(|e| e.id == id)
                .ok_or("Replacement font disappeared")?
        } else {
            inspected
                .entries
                .last()
                .cloned()
                .ok_or("Added font disappeared")?
        })
    };
    checkpoint()?;
    let length = next.serialized_len(document)?;
    Ok(Proposal {
        document: document.clone(),
        source: next,
        changed,
        summary: AttachmentSummary {
            action,
            before,
            after,
            document_bytes: number(length)?,
            preview_size_exceeded: length > 4 * 1024 * 1024,
        },
    })
}
fn number(value: usize) -> Result<u32> {
    value
        .try_into()
        .map_err(|_| "Attachment metadata exceeds supported limits".into())
}

// Font parse errors belong in the inventory; cancellation/deadline errors must
// abort the operation even if a caller supplies a nonpersistent checkpoint.
fn parse_with_checkpoint<T>(
    checkpoint: &mut impl FnMut() -> Result<()>,
    parse: impl FnOnce(&mut dyn FnMut() -> Result<()>) -> Result<T>,
) -> Result<Result<T>> {
    let mut interrupted = None;
    let parsed = parse(&mut || {
        let result = checkpoint();
        if let Err(error) = &result {
            interrupted = Some(error.clone());
        }
        result
    });
    match interrupted {
        Some(error) => Err(error),
        None => Ok(parsed),
    }
}
