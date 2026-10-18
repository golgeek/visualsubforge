//! Uniform cue shifts. No text, metadata, order or duration changes; no IO.
use crate::quality::{Error, Result};
use std::collections::HashSet;
use subtitle_core::{CueId, CueTiming, Document, Edit, Milliseconds};
use subtitle_formats::Format;

#[derive(Clone, Debug)]
pub enum Scope {
    Selected(Vec<CueId>),
    All,
    Following(CueId),        // inclusive file order, independent of timestamp order
    AtOrAfter(Milliseconds), // cue starts at/after the captured playhead
}
#[derive(Clone, Copy, Debug)]
pub enum Change {
    Shift(i64),
    Align(Milliseconds), // earliest scoped start; nearest format tick, ties later
}
#[derive(Clone, Debug)]
pub struct Request {
    pub scope: Scope,
    pub change: Change,
}
#[derive(Clone, Debug)]
pub struct Row {
    pub cue_index: usize,
    pub before: CueTiming,
    pub after: CueTiming,
}
pub struct Proposal {
    document: Document,
    pub offset_ms: i64,
    pub aligned_ms: Option<Milliseconds>,
    pub rows: Vec<Row>,
}
impl Proposal {
    pub fn edit(&self, current: &Document) -> Result<Edit> {
        if current != &self.document {
            return Err(Error::Invalid(
                "Subtitles changed after batch preview; preview again".into(),
            ));
        }
        let mut timings: Vec<_> = current.cues().iter().map(timing).collect();
        for row in &self.rows {
            timings[row.cue_index] = row.after.clone();
        }
        Ok(Edit::Retime { timings })
    }
}
fn timing(cue: &subtitle_core::Cue) -> CueTiming {
    CueTiming {
        id: cue.id(),
        start: cue.start(),
        end: cue.end(),
    }
}
pub fn propose(
    document: Document,
    format: Format,
    request: Request,
    mut checkpoint: impl FnMut() -> Result<()>,
) -> Result<Proposal> {
    checkpoint()?;
    let cues = document.cues();
    let ids: HashSet<_> = match &request.scope {
        Scope::Selected(ids) => {
            if ids.len() > cues.len() {
                return Err(Error::Invalid(
                    "Selection contains too many cue identities".into(),
                ));
            }
            let selected: HashSet<_> = ids.iter().copied().collect();
            if selected.len() != ids.len() {
                return Err(Error::Invalid(
                    "Selection contains duplicate cue identities".into(),
                ));
            }
            selected
        }
        _ => HashSet::new(),
    };
    let from = if let Scope::Following(id) = request.scope {
        cues.iter()
            .position(|c| c.id() == id)
            .ok_or_else(|| Error::Invalid("Current cue no longer exists".into()))?
    } else {
        0
    };
    let mut indices = Vec::new();
    for (i, cue) in cues.iter().enumerate() {
        checkpoint()?;
        let selected = match request.scope {
            Scope::Selected(_) => ids.contains(&cue.id()),
            Scope::All => true,
            Scope::Following(_) => i >= from,
            Scope::AtOrAfter(position) => cue.start() >= position,
        };
        if selected {
            indices.push(i);
        }
    }
    if matches!(request.scope, Scope::Selected(_)) && indices.len() != ids.len() {
        return Err(Error::Invalid("Selected cues no longer exist".into()));
    }
    let earliest = indices
        .iter()
        .map(|&i| cues[i].start().get())
        .min()
        .ok_or_else(|| Error::Invalid("No cues in this timing scope".into()))?;
    let step = format.time_step_ms();
    let (offset_ms, aligned_ms) = match request.change {
        Change::Shift(offset) => (offset, None),
        Change::Align(position) => {
            let target = (position.get() + step / 2) / step * step;
            let target = Milliseconds::new(target).map_err(Error::Invalid)?;
            (target.get() as i64 - earliest as i64, Some(target))
        }
    };
    if offset_ms == 0 {
        return Err(Error::Invalid(
            "This batch would not change any timings".into(),
        ));
    }
    if offset_ms.unsigned_abs() > Milliseconds::MAX || offset_ms % step as i64 != 0 {
        return Err(Error::Invalid(format!(
            "Offset must be within the supported time range and an exact multiple of {step} ms"
        )));
    }
    let shift = |time: Milliseconds| {
        time.get().checked_add_signed(offset_ms)
            .and_then(|ms| Milliseconds::new(ms).ok())
            .ok_or_else(|| Error::Invalid("A shifted endpoint would be negative or exceed 9999:59:59,999; no cues were changed".into()))
    };
    let mut rows = Vec::with_capacity(indices.len());
    for i in indices {
        checkpoint()?;
        if format == Format::WebVtt
            && crate::subtitle_markup::retiming_is_ambiguous(&cues[i], format)
        {
            return Err(Error::Invalid("WebVTT timestamp markers use absolute media time; timed or ambiguous cues require manual timing review. No cues were changed".into()));
        }
        let before = timing(&cues[i]);
        let after = CueTiming {
            id: before.id,
            start: shift(before.start)?,
            end: shift(before.end)?,
        };
        if !after.start.get().is_multiple_of(step) || !after.end.get().is_multiple_of(step) {
            return Err(Error::Invalid(
                "Source endpoints cannot be represented in this format".into(),
            ));
        }
        rows.push(Row {
            cue_index: i,
            before,
            after,
        });
    }
    if format == Format::WebVtt {
        let mut changed = rows.iter().peekable();
        let mut previous = None;
        for (i, cue) in cues.iter().enumerate() {
            checkpoint()?;
            let start = if changed.peek().is_some_and(|row| row.cue_index == i) {
                changed.next().expect("peeked row").after.start
            } else {
                cue.start()
            };
            if previous.is_some_and(|before| before > start) {
                return Err(Error::Invalid("This shift would put WebVTT cues out of start-time order; select a wider range or a smaller offset".into()));
            }
            previous = Some(start);
        }
    }
    checkpoint()?;
    Ok(Proposal {
        document,
        offset_ms,
        aligned_ms,
        rows,
    })
}

#[cfg(test)]
mod tests;
