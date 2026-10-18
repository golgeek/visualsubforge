//! Versioned data-only policy. Serialization is shared by CLI, presets and desktop.
use super::{Error, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum TextAlgorithm {
    VssUtf16V1,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum SpeedBasis {
    OrdinaryV1,
    VssOverlapV1,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Quality,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Profile {
    pub version: u32,
    pub kind: Kind,
    pub id: String,
    pub name: String,
    pub revision: u32,
    pub publisher: String,
    pub source: String,
    pub language: String,
    pub text_algorithm: TextAlgorithm,
    pub speed_basis: SpeedBasis,
    pub timing: Timing,
    pub rules: Rules,
    /// Optional extension to v2. Omission preserves the original serialized
    /// policy and fingerprint; old profiles never enable language checks.
    #[serde(default, skip_serializing_if = "crate::language::Settings::is_default")]
    pub language_tools: crate::language::Settings,
    /// Unsupported source semantics. Nonempty means incomplete policy coverage,
    /// independently of whether supported rules found any problems.
    pub limitations: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Timing {
    pub cue_gap_ms: u32,
    pub scene_before_ms: u32,
    pub scene_after_ms: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Rules {
    pub duration: DurationRule,
    pub speed: SpeedRule,
    pub lines: LineRule,
    pub gaps: bool,
    pub empty: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DurationRule {
    pub enabled: bool,
    pub minimum_ms: u32,
    pub maximum_ms: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SpeedRule {
    pub enabled: bool,
    pub minimum_rs: Option<u32>,
    pub maximum_rs_exclusive: Option<u32>,
    pub minimum_cps: Option<u32>,
    pub maximum_cps: Option<u32>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LineRule {
    pub enabled: bool,
    pub maximum_lines: u32,
    pub single_line_preferred: u32,
    pub maximum_characters: u32,
}

impl Profile {
    pub fn builtin(id: &str) -> Option<Self> {
        match id {
            "vsf-default" => Some(Self::default()),
            "sw3-uk-vo-subset" => Some(Self::sw3()),
            _ => None,
        }
    }
    pub fn builtins() -> Vec<Self> {
        vec![Self::default(), Self::sw3()]
    }
    pub fn validate(&self) -> Result<()> {
        let invalid = |s: &str| Error::Invalid(s.into());
        if self.version != 2 {
            return Err(invalid("Unsupported quality preset version (expected 2)"));
        }
        if self.revision == 0 {
            return Err(invalid("Profile revision must be positive"));
        }
        for (s, max) in [
            (&self.id, 120),
            (&self.name, 120),
            (&self.publisher, 200),
            (&self.source, 1000),
            (&self.language, 40),
        ] {
            if s.trim().is_empty() || s.chars().count() > max || s.chars().any(char::is_control) {
                return Err(invalid(
                    "Profile identity/provenance must be bounded printable text",
                ));
            }
        }
        if !self
            .id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
        {
            return Err(invalid(
                "Profile ID accepts ASCII letters, digits, hyphens and underscores",
            ));
        }
        if self.limitations.len() > 32
            || self.limitations.iter().any(|s| {
                s.trim().is_empty() || s.chars().count() > 500 || s.chars().any(char::is_control)
            })
        {
            return Err(invalid(
                "Profile limitations must contain at most 32 bounded descriptions",
            ));
        }
        if [
            self.timing.cue_gap_ms,
            self.timing.scene_before_ms,
            self.timing.scene_after_ms,
        ]
        .iter()
        .any(|n| *n > 10_000)
        {
            return Err(invalid("Profile timing buffers must be 0–10000 ms"));
        }
        let d = &self.rules.duration;
        if d.minimum_ms == 0 || d.minimum_ms > d.maximum_ms || d.maximum_ms > 3_600_000 {
            return Err(invalid(
                "Duration bounds must be ordered within 1–3600000 ms",
            ));
        }
        let s = &self.rules.speed;
        if [
            s.minimum_rs,
            s.maximum_rs_exclusive,
            s.minimum_cps,
            s.maximum_cps,
        ]
        .into_iter()
        .flatten()
        .any(|n| n == 0 || n > 1000)
            || s.minimum_rs
                .zip(s.maximum_rs_exclusive)
                .is_some_and(|(a, b)| a >= b)
            || s.minimum_cps.zip(s.maximum_cps).is_some_and(|(a, b)| a > b)
        {
            return Err(invalid(
                "Speed bounds must be ordered within 1–1000; null disables a bound",
            ));
        }
        let l = &self.rules.lines;
        if !(1..=100).contains(&l.maximum_lines)
            || l.single_line_preferred == 0
            || l.single_line_preferred > l.maximum_characters
            || l.maximum_characters > 10_000
        {
            return Err(invalid("Line limits must be positive, ordered and bounded"));
        }
        if let Some(builtin) = Self::builtin(&self.id)
            && self != &builtin
        {
            return Err(invalid(
                "Built-in profiles are immutable; customize using a new ID and name",
            ));
        }
        Ok(())
    }
    pub fn fingerprint(&self) -> Result<String> {
        self.validate()?;
        let bytes = serde_json::to_vec(self).map_err(|e| Error::Invalid(e.to_string()))?;
        Ok(format!("{:x}", Sha256::digest(bytes)))
    }
    pub fn custom(&self, id: String, name: String) -> Self {
        Self {
            id,
            name,
            revision: 1,
            ..self.clone()
        }
    }
    fn sw3() -> Self {
        Self {
            id: "sw3-uk-vo-subset".into(), name: "SW3 UK VO — archived subset".into(),
            publisher: "VSF; derived from archived vss-plugins policy".into(),
            source: "vss-plugins Hg 48928de4cfa6f28c4b90b51a8c65f30803aab650, presets/SW3_UK_VO.ini and jsplugin/common/common.js; VSS host 71778b9115e57c03e9be85234318c2004ac427b3".into(),
            language: "en".into(), speed_basis: SpeedBasis::VssOverlapV1,
            timing: Timing { cue_gap_ms: 160, scene_before_ms: 120, scene_after_ms: 120 },
            rules: Rules {
                duration: DurationRule { enabled: true, minimum_ms: 800, maximum_ms: 5000 },
                speed: SpeedRule { enabled: true, minimum_rs: Some(5), maximum_rs_exclusive: Some(35), minimum_cps: None, maximum_cps: None },
                lines: LineRule { enabled: true, maximum_lines: 2, single_line_preferred: 37, maximum_characters: 40 },
                gaps: true, empty: true,
            },
            limitations: vec![
                "Strict duration only: interactive minimum, neighbor/scene exemptions and historical tolerance are not implemented.".into(),
                "Scene/filter/sticky-blank and consecutive-fast/scene-sequence checks are not implemented.".into(),
                "Typography, capitalization, dialogue, tag validity, linguistic splitting and pixel-width checks are not implemented.".into(),
                "Independent source-derived metrics; no exhaustive executed VSS compatibility claim or full SW compliance.".into(),
            ],
            ..Self::default()
        }
    }
}
impl Default for Profile {
    fn default() -> Self {
        Self {
            version: 2,
            kind: Kind::Quality,
            id: "vsf-default".into(),
            name: "VSF authoring defaults".into(),
            revision: 1,
            publisher: "VisualSubForge".into(),
            source: "VSF quality contract v1; authoring aid, not a broadcaster standard".into(),
            language: "und".into(),
            text_algorithm: TextAlgorithm::VssUtf16V1,
            speed_basis: SpeedBasis::OrdinaryV1,
            timing: Timing {
                cue_gap_ms: 80,
                scene_before_ms: 130,
                scene_after_ms: 130,
            },
            rules: Rules {
                duration: DurationRule {
                    enabled: true,
                    minimum_ms: 800,
                    maximum_ms: 7000,
                },
                speed: SpeedRule {
                    enabled: true,
                    minimum_rs: Some(5),
                    maximum_rs_exclusive: Some(35),
                    minimum_cps: None,
                    maximum_cps: Some(20),
                },
                lines: LineRule {
                    enabled: true,
                    maximum_lines: 2,
                    single_line_preferred: 40,
                    maximum_characters: 42,
                },
                gaps: true,
                empty: true,
            },
            limitations: Vec::new(),
            language_tools: crate::language::Settings::default(),
        }
    }
}
