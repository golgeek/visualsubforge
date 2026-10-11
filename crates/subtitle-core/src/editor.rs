use crate::{Cue, CueId, Document, Milliseconds, Result};

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
    pub fn edit(
        &mut self,
        expected: u64,
        id: CueId,
        start: Milliseconds,
        end: Milliseconds,
        text: String,
    ) -> Result<bool> {
        self.expect_revision(expected)?;
        Cue::validate(start, end, &text)?;
        let index = self
            .document
            .cues
            .iter()
            .position(|cue| cue.id == id)
            .ok_or("Cue no longer exists")?;
        let before = self.document.cues[index].clone();
        let after = Cue {
            start,
            end,
            text,
            ..before.clone()
        };
        if before == after {
            return Ok(false);
        }
        self.replace(index, 1, vec![after]);
        Ok(true)
    }

    fn index(&self, id: CueId) -> Result<usize> {
        self.document
            .cues
            .iter()
            .position(|cue| cue.id == id)
            .ok_or_else(|| "Cue no longer exists".into())
    }

    fn allocate_id(&mut self) -> Result<CueId> {
        let id = self.next_cue.ok_or("Cue identities exhausted")?;
        // IPC represents identities as exact JavaScript integers.
        if id > 9_007_199_254_740_991 {
            return Err("Cue identities exhausted".into());
        }
        self.next_cue = id.checked_add(1);
        Ok(CueId(id))
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
        self.expect_revision(expected)?;
        Cue::validate(start, end, &text)?;
        let index = match after {
            Some(id) => self.index(id)? + 1,
            None => 0,
        };
        let id = self.allocate_id()?;
        self.replace(
            index,
            0,
            vec![Cue {
                id,
                start,
                end,
                text,
            }],
        );
        Ok(id)
    }

    pub fn delete(&mut self, expected: u64, id: CueId) -> Result<()> {
        self.expect_revision(expected)?;
        let index = self.index(id)?;
        self.replace(index, 1, Vec::new());
        Ok(())
    }

    /// Split an accepted cue into adjacent half-open intervals. Both retain the
    /// complete text; adjusting dialogue is a separate, explicit edit.
    pub fn split(&mut self, expected: u64, id: CueId, at: Milliseconds) -> Result<CueId> {
        self.expect_revision(expected)?;
        let index = self.index(id)?;
        let original = self.document.cues[index].clone();
        if at <= original.start || at >= original.end {
            return Err("Split time must be strictly inside the selected cue".into());
        }
        let new_id = self.allocate_id()?;
        let left = Cue {
            end: at,
            ..original.clone()
        };
        let right = Cue {
            id: new_id,
            start: at,
            ..original
        };
        self.replace(index, 1, vec![left, right]);
        Ok(new_id)
    }

    /// Merge two explicitly named adjacent cues in document order. Requiring
    /// both identities prevents a caller from accidentally merging a new neighbor.
    pub fn merge(&mut self, expected: u64, first: CueId, second: CueId) -> Result<()> {
        self.expect_revision(expected)?;
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
        let merged = Cue {
            id: first,
            start: left.start.min(right.start),
            end: left.end.max(right.end),
            text,
        };
        self.replace(index, 2, vec![merged]);
        Ok(())
    }

    // All fallible validation happens before this atomic history transaction.
    fn replace(&mut self, index: usize, count: usize, after: Vec<Cue>) {
        let before = self.document.cues[index..index + count].to_vec();
        let change = Change {
            index,
            before,
            after: after.clone(),
            before_state: self.state,
            after_state: self.next_state,
        };
        self.document.cues.splice(index..index + count, after);
        self.state = self.next_state;
        self.next_state += 1;
        self.revision += 1;
        self.undo.push(change);
        self.redo.clear();
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
