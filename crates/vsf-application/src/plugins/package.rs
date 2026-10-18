use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};

pub const MAX_FILES: usize = 64;
pub const MAX_PACKAGE_BYTES: usize = 1024 * 1024;
pub const CAPABILITIES: &[&str] = &[
    "rules",
    "fixes",
    "core-read",
    "scene-data",
    "package-scripts",
    "stripped-text-write",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Api {
    #[serde(rename = "vsf-1")]
    Vsf1,
    #[serde(rename = "vss-1")]
    Vss1,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Manifest {
    pub version: u32,
    pub id: String,
    pub name: String,
    pub api: Api,
    pub entry: String,
    #[serde(default)]
    pub requires: Vec<String>,
    /// Modern parameter defaults; VSS values explicitly override script defaults.
    #[serde(default)]
    pub settings: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Package {
    pub manifest: Manifest,
    /// Already captured file contents. Loading filesystem packages is an IO owner concern.
    pub files: BTreeMap<String, String>,
}

impl Package {
    pub fn validate(&self) -> Result<(), String> {
        let m = &self.manifest;
        if m.version != 1
            || m.id.is_empty()
            || m.id.len() > 64
            || !m
                .id
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, b'-' | b'.'))
            || m.name.trim().is_empty()
            || m.name.len() > 256
        {
            return Err("Unsupported plugin manifest version or invalid identity/name".into());
        }
        if self.files.is_empty()
            || self.files.len() > MAX_FILES
            || !self.files.contains_key(&m.entry)
        {
            return Err("Plugin requires an entry script and at most 64 captured files".into());
        }
        let mut names = HashSet::new();
        let mut size = 0usize;
        for (name, source) in &self.files {
            if name.len() > 255
                || !name.ends_with(".js")
                || name.contains(['\\', ':', '\0', '*', '?'])
                || name
                    .split('/')
                    .any(|p| p.is_empty() || p == "." || p == "..")
                || !names.insert(name.to_lowercase())
            {
                return Err(format!(
                    "Invalid or ambiguous package-local JavaScript path: {name}"
                ));
            }
            size = size.saturating_add(source.len());
        }
        if size > MAX_PACKAGE_BYTES {
            return Err("Plugin package exceeds 1 MiB of JavaScript".into());
        }
        let mut required = HashSet::new();
        for capability in &m.requires {
            if !CAPABILITIES.contains(&capability.as_str()) || !required.insert(capability) {
                return Err(format!(
                    "Unsupported or repeated plugin requirement: {capability}"
                ));
            }
        }
        validate_settings(&m.settings)
    }
    pub fn fingerprint(&self) -> Result<String, String> {
        self.validate()?;
        let bytes = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        Ok(format!("{:x}", Sha256::digest(bytes)))
    }
}

pub(super) fn validate_settings(settings: &BTreeMap<String, Value>) -> Result<(), String> {
    if settings.len() > 64 {
        return Err("Plugin has more than 64 settings".into());
    }
    for (name, value) in settings {
        if name.is_empty()
            || name.len() > 128
            || !name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
            || matches!(name.as_str(), "__proto__" | "constructor" | "prototype")
        {
            return Err("Invalid plugin setting name".into());
        }
        match value {
            Value::Bool(_) => {}
            Value::String(s) if s.len() <= 4096 => {}
            Value::Number(n)
                if n.as_f64()
                    .is_some_and(|v| v.is_finite() && v.abs() <= 9_007_199_254_740_991.0) => {}
            _ => {
                return Err(format!(
                    "Setting {name} must be a bounded string, boolean or finite number"
                ));
            }
        }
    }
    Ok(())
}
