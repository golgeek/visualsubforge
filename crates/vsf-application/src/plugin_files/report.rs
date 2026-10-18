use super::{Error, FileIdentity, Options, Result, input::digest};
use crate::plugins::{Api, Mode, Preferences, Proposal, Scenes};
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    io::{self, Write},
};

pub const MAX_REPORT_BYTES: usize = 64 * 1024 * 1024;
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub schema_version: u32,
    pub review_id: String,
    pub mode: Mode,
    pub source: FileIdentity,
    pub source_format: String,
    pub package: PackageInfo,
    pub context: ContextInfo,
    pub name: String,
    pub can_fix: bool,
    pub settings: BTreeMap<String, Value>,
    pub findings: Vec<Finding>,
    pub changes: Vec<Change>,
    pub logs: Vec<String>,
    pub output_sha256: Option<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageInfo {
    pub id: String,
    pub api: Api,
    pub fingerprint: String,
    pub files: Vec<FileIdentity>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextInfo {
    pub scope: Vec<u32>,
    pub selection: Vec<u32>,
    pub cursor_ms: Option<u64>,
    pub preferences: Preferences,
    pub scenes: Option<Scenes>,
    pub settings: BTreeMap<String, Value>,
    pub input: Option<FileIdentity>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub cue_id: String,
    pub message: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    pub cue: u32,
    pub cue_id: String,
    pub before: Content,
    pub after: Content,
    pub outside_scope: bool,
    pub warning: Option<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Content {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}
impl From<&subtitle_core::CueUpdate> for Content {
    fn from(c: &subtitle_core::CueUpdate) -> Self {
        Self {
            start_ms: c.start.get(),
            end_ms: c.end.get(),
            text: c.text.clone(),
        }
    }
}
impl Report {
    pub(super) fn changes(p: &Proposal) -> Vec<Change> {
        p.rows()
            .iter()
            .map(|r| Change {
                cue: r.cue_index as u32 + 1,
                cue_id: r.after.id.0.to_string(),
                before: (&r.before).into(),
                after: (&r.after).into(),
                outside_scope: r.outside_scope,
                warning: r.warning.clone(),
            })
            .collect()
    }
    pub(super) fn seal(&mut self) -> Result<Vec<u8>> {
        // The ID covers every review field with its own field empty. Applying
        // reruns the operation and compares this exact ID before publication.
        self.review_id.clear();
        self.review_id = digest(&encode(self)?);
        encode(self)
    }
}
pub(super) fn context(
    options: &Options,
    scope: Vec<u32>,
    preferences: Preferences,
    scenes: Option<Scenes>,
    cursor_ms: Option<u64>,
    input: Option<FileIdentity>,
) -> ContextInfo {
    ContextInfo {
        scope,
        selection: options.selection.clone(),
        cursor_ms,
        preferences,
        scenes,
        settings: options.settings.clone(),
        input,
    }
}
fn encode(value: &impl Serialize) -> Result<Vec<u8>> {
    struct Bounded(Vec<u8>);
    impl Write for Bounded {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            if self.0.len().saturating_add(b.len()) > MAX_REPORT_BYTES {
                return Err(io::Error::other("Plugin report exceeds 64 MiB"));
            }
            self.0.extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut writer = Bounded(Vec::new());
    serde_json::to_writer(&mut writer, value).map_err(|e| Error::Invalid(e.to_string()))?;
    Ok(writer.0)
}
