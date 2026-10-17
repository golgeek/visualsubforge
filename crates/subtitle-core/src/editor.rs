use crate::{Cue, CueId, Document, Milliseconds, Result};

/// New timing for an existing cue. Text and document order cannot be changed by
/// a retiming operation. The application supplies one entry per accepted cue.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CueTiming {
    pub id: CueId,
    pub start: Milliseconds,
    pub end: Milliseconds,
}

/// Replacement text for one existing cue; timing is deliberately absent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CueText {
    pub id: CueId,
    pub text: String,
}

/// Domain edits use validated times and identities, independently of any IPC or format.
pub enum Edit {
    TextBatch {
        texts: Vec<CueText>,
    },
    Retime {
        timings: Vec<CueTiming>,
    },
    Text {
        id: CueId,
        start: Milliseconds,
        end: Milliseconds,
        text: String,
    },
    Create {
        after: Option<CueId>,
        start: Milliseconds,
        end: Milliseconds,
        text: String,
    },
    Delete {
        id: CueId,
    },
    Split {
        id: CueId,
        at: Milliseconds,
    },
    Merge {
        first: CueId,
        second: CueId,
    },
}

pub struct EditOutcome {
    pub changed: bool,
    pub created: Option<CueId>,
}

struct Change {
    index: usize,
    before: Vec<Cue>,
    after: Vec<Cue>,
    before_state: u64,
    after_state: u64,
}

/// History stores replaced cue ranges, not copies of the whole document. State IDs
/// identify saved content across undo/redo; revision advances on every accepted
/// operation so an old frontend draft cannot overwrite a newer accepted edit.
pub struct Editor {
    document: Document,
    undo: Vec<Change>,
    redo: Vec<Change>,
    revision: u64,
    state: u64,
    next_state: u64,
    saved_state: u64,
    // Never rewound by Undo: discarded history must not alias a later cue.
    next_cue: Option<u64>,
}
impl Editor {
    pub fn new(document: Document) -> Self {
        let next_cue = document
            .cues
            .iter()
            .map(|cue| cue.id.0)
            .max()
            .unwrap_or(0)
            .checked_add(1);
        Self {
            document,
            undo: Vec::new(),
            redo: Vec::new(),
            revision: 0,
            state: 0,
            next_state: 1,
            saved_state: 0,
            next_cue,
        }
    }
    pub fn document(&self) -> &Document {
        &self.document
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn dirty(&self) -> bool {
        self.state != self.saved_state
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    /// Called only after a successful save of this exact accepted revision.
    pub fn mark_saved(&mut self, revision: u64) -> Result<()> {
        self.expect_revision(revision)?;
        self.saved_state = self.state;
        Ok(())
    }
    fn expect_revision(&self, expected: u64) -> Result<()> {
        if expected != self.revision {
            Err("Subtitle document changed; refresh before committing this edit".into())
        } else {
            Ok(())
        }
    }
    /// Validate a proposed document before publishing history or allocating IDs.
    /// The validator borrows the proposal synchronously and cannot mutate this editor.
    /// A rejection restores the replaced range, including redo and saved-state identity.
    pub fn apply_edit(
        &mut self,
        expected: u64,
        edit: Edit,
        validate: impl FnOnce(&Document) -> Result<()>,
    ) -> Result<EditOutcome> {
        self.expect_revision(expected)?;
        let mut created = None;
        let (index, count, after) = match edit {
            Edit::TextBatch { texts } => {
                if texts.len() != self.document.cues.len() {
                    return Err("Text batch must include every cue once in document order".into());
                }
                let mut after = Vec::with_capacity(texts.len());
                for (original, replacement) in self.document.cues.iter().zip(texts) {
                    if original.id != replacement.id {
                        return Err(
                            "Text batch must include every cue once in document order".into()
                        );
                    }
                    Cue::validate(original.start, original.end, &replacement.text)?;
                    after.push(Cue {
                        text: replacement.text,
                        ..original.clone()
                    });
                }
                (0, self.document.cues.len(), after)
            }
            Edit::Retime { timings } => {
                if timings.len() != self.document.cues.len() {
                    return Err("Retiming must include every cue once in document order".into());
                }
                let mut after = Vec::with_capacity(timings.len());
                for (original, timing) in self.document.cues.iter().zip(timings) {
                    if original.id != timing.id {
                        return Err("Retiming must include every cue once in document order".into());
                    }
                    Cue::validate(timing.start, timing.end, &original.text)?;
                    after.push(Cue {
                        start: timing.start,
                        end: timing.end,
                        ..original.clone()
                    });
                }
                (0, self.document.cues.len(), after)
            }
            Edit::Text {
                id,
                start,
                end,
                text,
            } => {
                Cue::validate(start, end, &text)?;
                let index = self.index(id)?;
                (
                    index,
                    1,
                    vec![Cue {
                        id,
                        start,
                        end,
                        text,
                    }],
                )
            }
            Edit::Create {
                after,
                start,
                end,
                text,
            } => {
                Cue::validate(start, end, &text)?;
                let index = match after {
                    Some(id) => self.index(id)? + 1,
                    None => 0,
                };
                let id = self.available_id()?;
                created = Some(id);
                (
                    index,
                    0,
                    vec![Cue {
                        id,
                        start,
                        end,
                        text,
                    }],
                )
            }
            Edit::Delete { id } => (self.index(id)?, 1, Vec::new()),
            Edit::Split { id, at } => {
                let index = self.index(id)?;
                let original = &self.document.cues[index];
                if at <= original.start || at >= original.end {
                    return Err("Split time must be strictly inside the selected cue".into());
                }
                let new_id = self.available_id()?;
                created = Some(new_id);
                (
                    index,
                    1,
                    vec![
                        Cue {
                            end: at,
                            ..original.clone()
                        },
                        Cue {
                            id: new_id,
                            start: at,
                            ..original.clone()
                        },
                    ],
                )
            }
            Edit::Merge { first, second } => {
                let index = self.index(first)?;
                let left = &self.document.cues[index];
                let right = self
                    .document
                    .cues
                    .get(index + 1)
                    .filter(|cue| cue.id == second)
                    .ok_or("Merge requires two adjacent cues in document order")?;
                let text = match (left.text.is_empty(), right.text.is_empty()) {
                    (true, _) => right.text.clone(),
                    (_, true) => left.text.clone(),
                    _ => format!("{}\n{}", left.text, right.text),
                };
                (
                    index,
                    2,
                    vec![Cue {
                        id: first,
                        start: left.start.min(right.start),
                        end: left.end.max(right.end),
                        text,
                    }],
                )
            }
        };
        if self.document.cues[index..index + count] == after {
            return Ok(EditOutcome {
                changed: false,
                created: None,
            });
        }
        let before = self.document.cues[index..index + count].to_vec();
        self.document
            .cues
            .splice(index..index + count, after.clone());
        if let Err(error) = validate(&self.document) {
            self.document
                .cues
                .splice(index..index + after.len(), before);
            return Err(error);
        }
        self.undo.push(Change {
            index,
            before,
            after,
            before_state: self.state,
            after_state: self.next_state,
        });
        self.state = self.next_state;
        self.next_state += 1;
        self.revision += 1;
        self.redo.clear();
        if let Some(id) = created {
            self.next_cue = id.0.checked_add(1);
        }
        Ok(EditOutcome {
            changed: true,
            created,
        })
    }

    fn index(&self, id: CueId) -> Result<usize> {
        self.document
            .cues
            .iter()
            .position(|cue| cue.id == id)
            .ok_or_else(|| "Cue no longer exists".into())
    }
    fn available_id(&self) -> Result<CueId> {
        let id = self.next_cue.ok_or("Cue identities exhausted")?;
        // IPC represents identities as exact JavaScript integers.
        if id > 9_007_199_254_740_991 {
            return Err("Cue identities exhausted".into());
        }
        Ok(CueId(id))
    }
    pub fn edit(
        &mut self,
        expected: u64,
        id: CueId,
        start: Milliseconds,
        end: Milliseconds,
        text: String,
    ) -> Result<bool> {
        self.apply_edit(
            expected,
            Edit::Text {
                id,
                start,
                end,
                text,
            },
            |_| Ok(()),
        )
        .map(|outcome| outcome.changed)
    }
    /// Insert after an explicit cue, or at the start of an empty/unselected list.
    /// Existing document order is never silently sorted by timing.
    pub fn create(
        &mut self,
        expected: u64,
        after: Option<CueId>,
        start: Milliseconds,
        end: Milliseconds,
        text: String,
    ) -> Result<CueId> {
        self.apply_edit(
            expected,
            Edit::Create {
                after,
                start,
                end,
                text,
            },
            |_| Ok(()),
        )?
        .created
        .ok_or_else(|| "Create did not allocate a cue".into())
    }
    pub fn delete(&mut self, expected: u64, id: CueId) -> Result<()> {
        self.apply_edit(expected, Edit::Delete { id }, |_| Ok(()))
            .map(|_| ())
    }
    /// Both halves retain the complete text; dialogue changes are separate edits.
    pub fn split(&mut self, expected: u64, id: CueId, at: Milliseconds) -> Result<CueId> {
        self.apply_edit(expected, Edit::Split { id, at }, |_| Ok(()))?
            .created
            .ok_or_else(|| "Split did not allocate a cue".into())
    }
    /// Both identities are required so a stale caller cannot merge a new neighbor.
    pub fn merge(&mut self, expected: u64, first: CueId, second: CueId) -> Result<()> {
        self.apply_edit(expected, Edit::Merge { first, second }, |_| Ok(()))
            .map(|_| ())
    }
    pub fn undo(&mut self, expected: u64) -> Result<bool> {
        self.expect_revision(expected)?;
        let Some(change) = self.undo.pop() else {
            return Ok(false);
        };
        self.document.cues.splice(
            change.index..change.index + change.after.len(),
            change.before.clone(),
        );
        self.state = change.before_state;
        self.revision += 1;
        self.redo.push(change);
        Ok(true)
    }
    pub fn redo(&mut self, expected: u64) -> Result<bool> {
        self.expect_revision(expected)?;
        let Some(change) = self.redo.pop() else {
            return Ok(false);
        };
        self.document.cues.splice(
            change.index..change.index + change.before.len(),
            change.after.clone(),
        );
        self.state = change.after_state;
        self.revision += 1;
        self.undo.push(change);
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejected_policy_preserves_document_redo_saved_state_revision_and_next_identity() {
        let mut editor = editor();
        let at = Milliseconds::new(1500).unwrap();
        assert_eq!(editor.split(0, CueId(1), at).unwrap(), CueId(2));
        editor.undo(1).unwrap();
        let original = editor.document().clone();
        let result = editor.apply_edit(2, Edit::Split { id: CueId(1), at }, |proposal| {
            assert_eq!(proposal.cues().len(), 2);
            Err("Application policy rejected this proposal".into())
        });
        assert!(result.is_err());
        assert_eq!(editor.document(), &original);
        assert_eq!(editor.revision(), 2);
        assert!(!editor.dirty());
        assert!(!editor.can_undo());
        assert!(editor.can_redo());
        editor.redo(2).unwrap();
        assert_eq!(editor.document().cues()[1].id(), CueId(2));
        editor.undo(3).unwrap();
        // The rejected split did not consume ID 3, and branching still clears redo.
        assert_eq!(editor.split(4, CueId(1), at).unwrap(), CueId(3));
        assert!(!editor.can_redo());
    }

    fn editor() -> Editor {
        Editor::new(
            Document::new(vec![
                Cue::new(
                    CueId(1),
                    Milliseconds::new(1000).unwrap(),
                    Milliseconds::new(2000).unwrap(),
                    "Hello".into(),
                )
                .unwrap(),
            ])
            .unwrap(),
        )
    }
    #[test]
    fn undo_restores_content_identity_and_saved_state() {
        let mut e = editor();
        let before = e.document.clone();
        e.edit(
            0,
            CueId(1),
            Milliseconds::new(1100).unwrap(),
            Milliseconds::new(2300).unwrap(),
            "Changed\ntext".into(),
        )
        .unwrap();
        let after = e.document.clone();
        assert!(e.dirty());
        assert_eq!(e.document.cues[0].id(), CueId(1));
        e.undo(1).unwrap();
        assert_eq!(e.document, before);
        assert!(!e.dirty());
        e.redo(2).unwrap();
        assert_eq!(e.document, after);
        assert!(e.dirty());
        e.mark_saved(3).unwrap();
        assert!(!e.dirty());
        e.undo(3).unwrap();
        assert!(e.dirty());
        e.redo(4).unwrap();
        assert!(!e.dirty());
    }
    #[test]
    fn rejected_stale_and_invalid_edits_preserve_history() {
        let mut e = editor();
        let original = e.document.clone();
        let a = Milliseconds::new(1000).unwrap();
        let b = Milliseconds::new(2000).unwrap();
        assert!(e.edit(2, CueId(1), a, b, "Stale".into()).is_err());
        assert!(e.edit(0, CueId(1), b, a, "Invalid".into()).is_err());
        assert!(e.edit(0, CueId(999), a, b, "Missing".into()).is_err());
        assert!(!e.edit(0, CueId(1), a, b, "Hello".into()).unwrap());
        assert_eq!(e.document, original);
        assert_eq!(e.revision(), 0);
        assert!(!e.can_undo());
        e.edit(0, CueId(1), a, b, "A".into()).unwrap();
        e.mark_saved(1).unwrap();
        e.undo(1).unwrap();
        e.edit(2, CueId(1), a, b, "B".into()).unwrap();
        assert!(!e.can_redo());
        assert!(e.dirty());
        assert!(e.mark_saved(1).is_err());
    }
}
