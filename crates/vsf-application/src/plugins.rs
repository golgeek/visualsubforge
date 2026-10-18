//! Pure plugin captures and proposal validation. No engine, IO, Tauri or media.
mod package;
use crate::{
    subtitle_file,
    subtitle_markup::{MAX_CUE_BYTES, Projection, retiming_is_ambiguous},
};
pub use package::{Api, CAPABILITIES, Manifest, Package};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};
use subtitle_core::{Cue, CueId, CueUpdate, Document, Edit, Milliseconds};
use subtitle_formats::{Format, Source};

pub const MAX_CUES: usize = 100_000;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Key {
    pub generation: u64,
    pub revision: u64,
    pub context: u64,
}
impl Key {
    fn wire(self) -> Value {
        json!({"generation":self.generation.to_string(),"revision":self.revision.to_string(),"context":self.context.to_string()})
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    Check,
    Fix,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preferences {
    pub minimum_blank_ms: u64,
    pub minimum_duration_ms: u64,
    pub maximum_duration_ms: u64,
    pub cps_target: f64,
    pub rs_target_index: Option<u32>,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Scenes {
    pub cuts_ms: Vec<u64>,
    pub start_offset_ms: u64,
    pub stop_offset_ms: u64,
    pub filter_offset_ms: u64,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Context {
    pub selection: Vec<CueId>,
    pub cursor: Option<Milliseconds>,
    pub preferences: Preferences,
    /// Only completed, correctly mapped scene metadata may be supplied.
    pub scenes: Option<Scenes>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub key: Key,
    pub mode: Mode,
    pub scope: Vec<CueId>,
    pub context: Context,
    pub settings: BTreeMap<String, Value>,
}
pub struct Execution {
    pub script: String,
    pub input: Value,
}
pub struct Capture {
    document: Document,
    source: Source,
    package: Package,
    fingerprint: String,
    request: Request,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Finding {
    pub cue_id: String,
    pub message: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct WireEdit {
    cue_id: String,
    start_ms: u64,
    end_ms: u64,
    text: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Reply {
    version: u32,
    key: Value,
    fingerprint: String,
    name: String,
    can_fix: bool,
    settings: BTreeMap<String, Value>,
    findings: Vec<Finding>,
    edits: Vec<WireEdit>,
    logs: Vec<String>,
}
#[derive(Clone, Debug)]
pub struct Row {
    pub cue_index: usize,
    pub before: CueUpdate,
    pub after: CueUpdate,
    pub outside_scope: bool,
    pub warning: Option<String>,
}
/// Validated review. Callers can inspect rows but cannot replace validated edits.
pub struct Proposal {
    capture: Capture,
    name: String,
    can_fix: bool,
    settings: BTreeMap<String, Value>,
    findings: Vec<Finding>,
    rows: Vec<Row>,
    logs: Vec<String>,
}
fn update(cue: &Cue) -> CueUpdate {
    CueUpdate {
        id: cue.id(),
        start: cue.start(),
        end: cue.end(),
        text: cue.text().into(),
    }
}
fn validate_ids(ids: &[CueId], document: &Document) -> Result<(), String> {
    let requested: HashSet<_> = ids.iter().copied().collect();
    if requested.len() != ids.len()
        || ids.len() > document.cues().len()
        || document
            .cues()
            .iter()
            .filter(|c| requested.contains(&c.id()))
            .count()
            != ids.len()
    {
        return Err("Plugin scope/selection contains missing or duplicate cue identities".into());
    }
    Ok(())
}

impl Capture {
    pub fn new(
        document: Document,
        source: Source,
        package: Package,
        request: Request,
        mut checkpoint: impl FnMut() -> Result<(), String>,
    ) -> Result<Self, String> {
        checkpoint()?;
        let fingerprint = package.fingerprint()?;
        subtitle_file::validate_document(&document, &source)?;
        if document.cues().len() > MAX_CUES {
            return Err("Plugins support at most 100,000 captured cues".into());
        }
        validate_ids(&request.scope, &document)?;
        validate_ids(&request.context.selection, &document)?;
        package::validate_settings(&request.settings)?;
        let p = &request.context.preferences;
        if p.minimum_duration_ms == 0
            || p.maximum_duration_ms < p.minimum_duration_ms
            || p.maximum_duration_ms > Milliseconds::MAX
            || p.minimum_blank_ms > Milliseconds::MAX
            || !p.cps_target.is_finite()
            || p.cps_target <= 0.0
            || p.cps_target > 1000.0
        {
            return Err("Invalid effective plugin preferences".into());
        }
        if let Some(s) = &request.context.scenes {
            if s.cuts_ms.len() > MAX_CUES
                || s.cuts_ms.iter().any(|t| *t > Milliseconds::MAX)
                || s.cuts_ms.windows(2).any(|w| w[0] >= w[1])
                || [s.start_offset_ms, s.stop_offset_ms, s.filter_offset_ms]
                    .iter()
                    .any(|t| *t > Milliseconds::MAX)
            {
                return Err("Plugin scene metadata must be bounded, sorted and unique".into());
            }
        } else if package.manifest.requires.iter().any(|r| r == "scene-data") {
            return Err("This plugin requires completed scene metadata".into());
        }
        if package.manifest.api == Api::Vss1 {
            for cue in document.cues() {
                checkpoint()?;
                if cue.end().get() > i32::MAX as u64 {
                    return Err("VSS compatibility requires signed 32-bit millisecond times; use the modern API for this document".into());
                }
            }
            let limit = i32::MAX as u64;
            if [
                p.minimum_blank_ms,
                p.minimum_duration_ms,
                p.maximum_duration_ms,
            ]
            .iter()
            .any(|v| *v > limit)
                || p.rs_target_index.is_some_and(|v| v > i32::MAX as u32)
                || request.context.cursor.is_some_and(|t| t.get() > limit)
                || request.context.scenes.as_ref().is_some_and(|s| {
                    s.cuts_ms.last().is_some_and(|t| *t > limit)
                        || [s.start_offset_ms, s.stop_offset_ms, s.filter_offset_ms]
                            .iter()
                            .any(|t| *t > limit)
                })
            {
                return Err(
                    "VSS context requires signed 32-bit times and preference indices".into(),
                );
            }
        }
        checkpoint()?;
        Ok(Self {
            document,
            source,
            package,
            fingerprint,
            request,
        })
    }
    pub fn execution(
        &self,
        mut checkpoint: impl FnMut() -> Result<(), String>,
    ) -> Result<Execution, String> {
        checkpoint()?;
        let mut cues = Vec::with_capacity(self.document.cues().len());
        for (index, cue) in self.document.cues().iter().enumerate() {
            checkpoint()?;
            cues.push(json!({"id":cue.id().0.to_string(),"index":index+1,"startMs":cue.start().get(),"endMs":cue.end().get(),"text":cue.text()}));
        }
        let ctx = &self.request.context;
        let execution = Execution {
            script: format!(
                "{}\nreturn pluginAdapter(input);",
                include_str!("plugins/adapter.js")
            ),
            input: json!({"package":self.package,"fingerprint":self.fingerprint,"key":self.request.key.wire(),"mode":self.request.mode,"format":self.source.format().extension(),"cues":cues,"scope":self.request.scope.iter().map(|id|id.0.to_string()).collect::<Vec<_>>(),"settings":self.request.settings,
                "context":{"selection":ctx.selection.iter().map(|id|id.0.to_string()).collect::<Vec<_>>(),"cursorMs":ctx.cursor.map(Milliseconds::get),"preferences":ctx.preferences,"scenes":ctx.scenes}}),
        };
        checkpoint()?;
        Ok(execution)
    }
    pub fn validate(
        self,
        value: Value,
        mut checkpoint: impl FnMut() -> Result<(), String>,
    ) -> Result<Proposal, String> {
        checkpoint()?;
        let reply: Reply =
            serde_json::from_value(value).map_err(|e| format!("Invalid plugin result: {e}"))?;
        if reply.version != 1
            || reply.key != self.request.key.wire()
            || reply.fingerprint != self.fingerprint
        {
            return Err("Plugin result does not match its captured document/package".into());
        }
        if reply.name.trim().is_empty()
            || reply.name.len() > 256
            || reply.logs.len() > 100
            || reply.logs.iter().any(|s| s.len() > 4096)
            || reply.logs.iter().map(String::len).sum::<usize>() > 16384
        {
            return Err("Plugin result exceeds name/log limits".into());
        }
        package::validate_settings(&reply.settings)?;
        let scope: HashSet<_> = self
            .request
            .scope
            .iter()
            .map(|id| id.0.to_string())
            .collect();
        let mut found = HashSet::new();
        if reply.findings.len() > scope.len()
            || reply.edits.len() > self.document.cues().len()
            || (self.request.mode == Mode::Check && !reply.edits.is_empty())
        {
            return Err(
                "Plugin returned excess findings/edits or changed a read-only check".into(),
            );
        }
        for finding in &reply.findings {
            checkpoint()?;
            if !scope.contains(&finding.cue_id)
                || !found.insert(&finding.cue_id)
                || finding.message.trim().is_empty()
                || finding.message.len() > 4096
            {
                return Err("Plugin finding has an invalid cue identity or message".into());
            }
        }
        let mut edits = HashMap::new();
        for edit in reply.edits {
            checkpoint()?;
            if edits.insert(edit.cue_id.clone(), edit).is_some() {
                return Err("Plugin returned duplicate cue edits".into());
            }
        }
        let mut candidates = Vec::with_capacity(self.document.cues().len());
        let mut rows = Vec::new();
        for (index, before) in self.document.cues().iter().enumerate() {
            checkpoint()?;
            let id = before.id().0.to_string();
            let after = if let Some(edit) = edits.remove(&id) {
                let step = self.source.format().time_step_ms();
                if edit.start_ms % step != 0 || edit.end_ms % step != 0 {
                    return Err(format!(
                        "Cue {} plugin times must use {step}ms precision",
                        index + 1
                    ));
                }
                let text = self.source.prepare_text(edit.text.replace("\r\n", "\n"))?;
                let mut cue = Cue::new(
                    before.id(),
                    Milliseconds::new(edit.start_ms)?,
                    Milliseconds::new(edit.end_ms)?,
                    text,
                )?;
                if let Some(metadata) = before.metadata() {
                    cue = cue.with_metadata(metadata.clone());
                }
                let timing_changed = before.start() != cue.start() || before.end() != cue.end();
                let duration_changed = before.end().get() - before.start().get()
                    != cue.end().get() - cue.start().get();
                if (if self.source.format() == Format::WebVtt {
                    timing_changed
                } else {
                    duration_changed
                }) && (retiming_is_ambiguous(before, self.source.format())
                    || retiming_is_ambiguous(&cue, self.source.format()))
                {
                    return Err(format!(
                        "Cue {} plugin timing cannot safely preserve timed effects or ambiguous markup",
                        index + 1
                    ));
                }
                cue
            } else {
                before.clone()
            };
            if before != &after {
                rows.push(Row {
                    cue_index: index,
                    before: update(before),
                    after: update(&after),
                    outside_scope: !scope.contains(&id),
                    warning: markup_warning(before.text(), after.text(), self.source.format()),
                });
            }
            candidates.push(after);
        }
        if !edits.is_empty() {
            return Err("Plugin edited a missing or noncanonical cue identity".into());
        }
        let candidate = Document::new(candidates)?;
        if self
            .document
            .cues()
            .windows(2)
            .all(|w| w[0].start() <= w[1].start())
            && candidate
                .cues()
                .windows(2)
                .any(|w| w[0].start() > w[1].start())
        {
            return Err("Plugin timing would reverse chronological cue order".into());
        }
        subtitle_file::validate_document(&candidate, &self.source)?;
        checkpoint()?;
        Ok(Proposal {
            capture: self,
            name: reply.name,
            can_fix: reply.can_fix,
            settings: reply.settings,
            findings: reply.findings,
            rows,
            logs: reply.logs,
        })
    }
}
fn markup_warning(before: &str, after: &str, format: Format) -> Option<String> {
    if before == after {
        return None;
    }
    if before.len() > MAX_CUE_BYTES || after.len() > MAX_CUE_BYTES {
        return Some(
            "Markup comparison unavailable above 64 KiB; review the raw text carefully".into(),
        );
    }
    let a = Projection::for_format(before, format);
    let b = Projection::for_format(after, format);
    let old: Vec<_> = a.protected.iter().map(|r| &before[r.clone()]).collect();
    let new: Vec<_> = b.protected.iter().map(|r| &after[r.clone()]).collect();
    if old != new || a.ambiguous.is_some() || b.ambiguous.is_some() {
        Some("Plugin changes protected or ambiguous markup; review the raw text for formatting/effect loss".into())
    } else {
        None
    }
}
impl Proposal {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn can_fix(&self) -> bool {
        self.can_fix
    }
    pub fn settings(&self) -> &BTreeMap<String, Value> {
        &self.settings
    }
    pub fn findings(&self) -> &[Finding] {
        &self.findings
    }
    pub fn rows(&self) -> &[Row] {
        &self.rows
    }
    pub fn logs(&self) -> &[String] {
        &self.logs
    }
    /// The owner must supply its current capture key/context and also apply this
    /// edit with Editor's expected revision while holding its document lock.
    pub fn edit(
        &self,
        current: &Document,
        source: &Source,
        request: &Request,
    ) -> Result<Edit, String> {
        if current != &self.capture.document
            || source != &self.capture.source
            || request != &self.capture.request
        {
            return Err(
                "Document, plugin scope or context changed after review; run the plugin again"
                    .into(),
            );
        }
        let mut cues: Vec<_> = current.cues().iter().map(update).collect();
        for row in &self.rows {
            cues[row.cue_index] = row.after.clone();
        }
        Ok(Edit::UpdateBatch { cues })
    }
}
