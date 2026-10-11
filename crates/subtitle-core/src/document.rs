use crate::Result;

/// Millisecond precision matches the first supported format, SRT. UI/media
/// floating-point seconds must be rounded explicitly before entering the domain.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Milliseconds(u64);
impl Milliseconds {
    // Four-digit hours keep serialization, UI arithmetic and JS numbers exact.
    pub const MAX: u64 = 9_999 * 3_600_000 + 3_599_999;
    pub fn new(value: u64) -> Result<Self> {
        if value > Self::MAX {
            return Err("Time exceeds 9999:59:59,999".into());
        }
        Ok(Self(value))
    }
    pub fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CueId(pub u64);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cue {
    pub(crate) id: CueId,
    pub(crate) start: Milliseconds,
    pub(crate) end: Milliseconds,
    pub(crate) text: String,
}
impl Cue {
    pub fn id(&self) -> CueId {
        self.id
    }
    pub fn start(&self) -> Milliseconds {
        self.start
    }
    pub fn end(&self) -> Milliseconds {
        self.end
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn new(id: CueId, start: Milliseconds, end: Milliseconds, text: String) -> Result<Self> {
        Self::validate(start, end, &text)?;
        Ok(Self {
            id,
            start,
            end,
            text,
        })
    }
    pub(crate) fn validate(start: Milliseconds, end: Milliseconds, text: &str) -> Result<()> {
        if end <= start {
            return Err("Cue end must be later than its start".into());
        }
        if text.contains('\r') || text.contains('\0') {
            return Err("Cue text cannot contain carriage returns or NUL characters".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Document {
    pub(crate) cues: Vec<Cue>,
}
impl Document {
    pub fn new(cues: Vec<Cue>) -> Result<Self> {
        let mut ids = std::collections::HashSet::new();
        if cues.iter().any(|cue| !ids.insert(cue.id)) {
            return Err("Cue identities must be unique".into());
        }
        Ok(Self { cues })
    }
    pub fn cues(&self) -> &[Cue] {
        &self.cues
    }
}
