use crate::Result;
use std::sync::Arc;

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

/// Accepted event properties travel with a cue through history. Field parsing
/// and format-specific meaning belong to the format/application boundaries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CueMetadata {
    // Cue clones in captures/history share immutable event fields. A style
    // replacement copies only that cue's fields; accepted snapshots cannot see
    // the new values until their own transaction is committed.
    fields: Arc<[(String, String)]>,
}
impl CueMetadata {
    pub fn new(fields: Vec<(String, String)>) -> Result<Self> {
        let mut names = std::collections::HashSet::new();
        for (name, value) in &fields {
            if name.is_empty()
                || name.trim() != name
                || name.contains('\r')
                || name.contains('\n')
                || name.contains('\0')
                || name.contains(',')
                || !names.insert(name.to_ascii_lowercase())
                || value.contains('\r')
                || value.contains('\n')
                || value.contains('\0')
            {
                return Err("Invalid or duplicate cue metadata field".into());
            }
        }
        Ok(Self {
            fields: fields.into(),
        })
    }
    pub fn fields(&self) -> &[(String, String)] {
        &self.fields
    }
    pub fn get(&self, name: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
    pub fn with_style(&self, style: String) -> Result<Self> {
        if style.trim().is_empty() || style.contains(['\r', '\n', '\0', ',']) {
            return Err("Style name must be nonempty and contain no commas or line breaks".into());
        }
        let mut metadata = self.clone();
        let (_, value) = Arc::make_mut(&mut metadata.fields)
            .iter_mut()
            .find(|(key, _)| key.eq_ignore_ascii_case("Style"))
            .ok_or("This cue has no assignable style")?;
        // Metadata retains original field bytes. Changing the logical style
        // name should not discard padding, including the event delimiter space
        // when Style happens to be the first source column.
        let left = value.len() - value.trim_start_matches([' ', '\t']).len();
        let right = value.trim_end_matches([' ', '\t']).len().max(left);
        *value = format!("{}{}{}", &value[..left], style, &value[right..]);
        Ok(metadata)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cue {
    pub(crate) id: CueId,
    pub(crate) start: Milliseconds,
    pub(crate) end: Milliseconds,
    pub(crate) text: String,
    pub(crate) metadata: Option<CueMetadata>,
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
    pub fn metadata(&self) -> Option<&CueMetadata> {
        self.metadata.as_ref()
    }
    pub fn with_metadata(mut self, metadata: CueMetadata) -> Self {
        self.metadata = Some(metadata);
        self
    }
    pub fn new(id: CueId, start: Milliseconds, end: Milliseconds, text: String) -> Result<Self> {
        Self::validate(start, end, &text)?;
        Ok(Self {
            id,
            start,
            end,
            text,
            metadata: None,
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
