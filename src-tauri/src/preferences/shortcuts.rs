//! User bindings are data, validated before persistence or native publication.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Shortcuts {
    pub preset: Preset,
    pub bindings: Vec<Binding>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum Preset {
    Current,
    VsfDefault,
    VisualSubSyncClassic,
    Custom,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Binding {
    pub action: String,
    pub mode: Mode,
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mac_key: Option<String>,
}
impl Binding {
    pub fn platform_key(&self, mac: bool) -> &str {
        if mac {
            self.mac_key.as_deref().unwrap_or(&self.key)
        } else {
            &self.key
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    Global,
    Alternate,
    Workspace,
    Text,
}
impl Mode {
    fn overlaps(self, other: Self) -> bool {
        self == other
            || matches!(self, Self::Global | Self::Alternate)
            || matches!(other, Self::Global | Self::Alternate)
    }
}
impl Default for Shortcuts {
    fn default() -> Self {
        // Bundled source data, checked with every preset in the tests below.
        let mut presets: BTreeMap<String, Vec<Binding>> = serde_json::from_str(include_str!(
            "../../../src/lib/workspace/shortcut-presets.json"
        ))
        .expect("valid bundled shortcut presets");
        Self {
            preset: Preset::Current,
            bindings: presets
                .remove("current")
                .expect("bundled current shortcuts"),
        }
    }
}
impl Shortcuts {
    // Older bundled presets had empty zoom slots. Never overwrite a custom
    // binding or introduce a conflict while upgrading the bundled defaults.
    pub fn upgrade_zoom_defaults(&mut self) {
        if self.preset == Preset::Custom {
            return;
        }
        let mut next = self.clone();
        for (action, key) in [
            ("zoomIn", "CmdOrCtrl+="),
            ("zoomOut", "CmdOrCtrl+-"),
            ("fitWaveform", "CmdOrCtrl+0"),
        ] {
            if let Some(binding) = next
                .bindings
                .iter_mut()
                .find(|b| b.action == action && b.mode == Mode::Global && b.key.is_empty())
            {
                binding.key = key.into();
            }
        }
        if !next
            .bindings
            .iter()
            .any(|b| b.action == "zoomIn" && b.mode == Mode::Alternate)
        {
            next.bindings.push(Binding {
                action: "zoomIn".into(),
                mode: Mode::Alternate,
                key: "CmdOrCtrl+Shift+=".into(),
                mac_key: None,
            });
        }
        if next.validate().is_ok() {
            *self = next;
        }
    }

    // Add only missing bundled slots. Custom sets remain exactly as saved.
    pub fn upgrade_cue_text_defaults(&mut self) {
        if self.preset != Preset::Custom {
            for binding in Self::default()
                .bindings
                .into_iter()
                .filter(|b| b.mac_key.is_some())
            {
                if !self
                    .bindings
                    .iter()
                    .any(|b| b.action == binding.action && b.mode == binding.mode)
                {
                    self.bindings.push(binding);
                    // A previously valid imported set can already use this chord.
                    if self.validate().is_err() {
                        self.bindings.pop();
                    }
                }
            }
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        let actions: BTreeMap<String, serde_json::Value> = serde_json::from_str(include_str!(
            "../../../src/lib/workspace/action-descriptions.json"
        ))
        .map_err(|e| e.to_string())?;
        if self.bindings.len() > 512 {
            return Err("Too many shortcut bindings".into());
        }
        let mut slots = HashSet::new();
        for (index, binding) in self.bindings.iter().enumerate() {
            if !actions.contains_key(&binding.action)
                || !slots.insert((&binding.action, binding.mode))
            {
                return Err("Unknown action or duplicate shortcut mode".into());
            }
            for mac in [false, true] {
                let key = binding.platform_key(mac);
                if key.is_empty() {
                    continue;
                }
                validate_key(binding, key)?;
                if let Some(other) = self.bindings[..index].iter().find(|other| {
                    other.mode.overlaps(binding.mode)
                        && canonical(other.platform_key(mac)) == canonical(key)
                }) {
                    return Err(format!(
                        "{} conflicts with {} in an overlapping focus mode",
                        binding.action, other.action
                    ));
                }
            }
        }
        for (action, key) in [("quit", "CmdOrCtrl+Q"), ("focusWaveform", "Escape")] {
            if !self.bindings.iter().any(|b| {
                b.action == action
                    && b.mode == Mode::Global
                    && b.key == key
                    && b.platform_key(true) == key
            }) {
                return Err(format!("{action} keeps its reserved {key} binding"));
            }
        }
        Ok(())
    }
    pub fn key_for(&self, action: &str, mode: Mode, mac: bool) -> Option<&str> {
        self.bindings
            .iter()
            .find(|b| b.action == action && b.mode == mode)
            .map(|b| b.platform_key(mac))
            .filter(|key| !key.is_empty())
    }
}
fn canonical(key: &str) -> String {
    key.split('+')
        .map(|part| if part == "Enter" { "Return" } else { part })
        .collect::<Vec<_>>()
        .join("+")
}
fn validate_key(binding: &Binding, chord: &str) -> Result<(), String> {
    let parts: Vec<_> = chord.split('+').collect();
    let key = parts.last().copied().unwrap_or_default();
    let modifiers = &parts[..parts.len().saturating_sub(1)];
    let expected: Vec<_> = ["CmdOrCtrl", "Alt", "Shift"]
        .into_iter()
        .filter(|m| modifiers.contains(m))
        .collect();
    let function = key
        .strip_prefix('F')
        .and_then(|s| s.parse::<u8>().ok())
        .is_some_and(|n| (1..=24).contains(&n));
    let character = key.len() == 1
        && (key.as_bytes()[0].is_ascii_uppercase()
            || key.as_bytes()[0].is_ascii_digit()
            || ["[", "]", ",", ".", "/", "?", "-", "="].contains(&key));
    if chord.len() > 48
        || (key == "?" && modifiers.contains(&"Shift"))
        || modifiers != expected
        || !(function
            || character
            || [
                "Return",
                "Enter",
                "Escape",
                "Space",
                "Tab",
                "Left",
                "Right",
                "Up",
                "Down",
                "Backspace",
                "Delete",
            ]
            .contains(&key))
    {
        return Err("Use CmdOrCtrl, Alt, Shift and a supported key, in that order".into());
    }
    let command = modifiers.contains(&"CmdOrCtrl");
    let alt = modifiers.contains(&"Alt");
    if command && alt && character {
        return Err("Ctrl+Alt character keys are reserved for AltGr text input".into());
    }
    if matches!(binding.mode, Mode::Global | Mode::Alternate)
        && !command
        && !alt
        && !function
        && !(key == "Escape" && binding.action == "focusWaveform")
        && !(key == "?" && binding.action == "shortcutSheet")
    {
        return Err("Everywhere shortcuts need a command/Alt modifier or a function key".into());
    }
    if binding.mode == Mode::Text && !command && !alt && !function && key != "Tab" {
        return Err("Text-mode shortcuts must preserve ordinary typing".into());
    }
    let reserved = [
        "CmdOrCtrl+A",
        "CmdOrCtrl+B",
        "CmdOrCtrl+C",
        "CmdOrCtrl+I",
        "CmdOrCtrl+U",
        "CmdOrCtrl+V",
        "CmdOrCtrl+X",
        "CmdOrCtrl+H",
        "CmdOrCtrl+Space",
        "CmdOrCtrl+Tab",
        "Alt+Tab",
    ];
    if reserved.contains(&chord)
        || (chord == "Escape" && binding.action != "focusWaveform")
        || (chord == "CmdOrCtrl+Q" && binding.action != "quit")
    {
        return Err("This shortcut is reserved for text editing or the operating system".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bundled_presets_are_valid_and_focus_conflicts_are_rejected() {
        let presets: BTreeMap<String, Vec<Binding>> = serde_json::from_str(include_str!(
            "../../../src/lib/workspace/shortcut-presets.json"
        ))
        .unwrap();
        for bindings in presets.into_values() {
            Shortcuts {
                preset: Preset::Custom,
                bindings,
            }
            .validate()
            .unwrap();
        }
        let mut value = Shortcuts::default();
        value.bindings.push(Binding {
            action: "splitCue".into(),
            mode: Mode::Workspace,
            key: "Space".into(),
            mac_key: None,
        });
        assert!(value.validate().unwrap_err().contains("conflicts"));
        value.bindings.last_mut().unwrap().mode = Mode::Text;
        assert!(value.validate().unwrap_err().contains("typing"));
        value.bindings.last_mut().unwrap().key = "Shift+Tab".into();
        assert!(value.validate().is_ok());
        value.bindings.last_mut().unwrap().key = "CmdOrCtrl+S".into();
        assert!(value.validate().unwrap_err().contains("conflicts"));
    }
    #[test]
    fn old_bundled_sets_gain_only_missing_cue_text_slots_and_custom_sets_keep_their_bindings() {
        let current = Shortcuts::default();
        let mut old = current.clone();
        old.bindings.retain(|b| b.mac_key.is_none());
        let mut custom = old.clone();
        custom.preset = Preset::Custom;
        let saved = custom.clone();
        custom.upgrade_cue_text_defaults();
        assert_eq!(custom, saved);
        old.upgrade_cue_text_defaults();
        old.upgrade_cue_text_defaults();
        assert_eq!(old.bindings.len(), current.bindings.len());
        assert_eq!(
            old.key_for("previousCue", Mode::Text, true),
            Some("CmdOrCtrl+Alt+Up")
        );
        assert_eq!(old.key_for("previousCue", Mode::Text, false), None);
        // A macOS-only conflict must also be refused when imported elsewhere.
        old.bindings
            .iter_mut()
            .find(|b| b.action == "play" && b.mode == Mode::Alternate)
            .unwrap()
            .key = "CmdOrCtrl+Alt+Up".into();
        assert!(old.validate().unwrap_err().contains("conflicts"));
    }

    #[test]
    fn malformed_reserved_and_unknown_bindings_fail() {
        for key in [
            "ctrl+s",
            "Shift+CmdOrCtrl+S",
            "CmdOrCtrl+CmdOrCtrl+S",
            "CmdOrCtrl+C",
            "Escape",
            "Alt+Tab",
            "X+Y",
        ] {
            let mut value = Shortcuts::default();
            value.bindings.push(Binding {
                action: "splitCue".into(),
                mode: Mode::Text,
                key: key.into(),
                mac_key: None,
            });
            assert!(value.validate().is_err(), "{key}");
        }
        let mut value = Shortcuts::default();
        value.bindings[0].action = "notAnAction".into();
        assert!(value.validate().is_err());
    }
}

#[cfg(test)]
mod zoom_upgrade_tests {
    use super::*;
    #[test]
    fn bundled_zoom_defaults_upgrade_without_replacing_custom_choices() {
        let mut old = Shortcuts::default();
        old.bindings
            .retain(|b| !(b.action == "zoomIn" && b.mode == Mode::Alternate));
        for b in &mut old.bindings {
            if ["zoomIn", "zoomOut", "fitWaveform"].contains(&b.action.as_str()) {
                b.key.clear();
            }
        }
        let mut custom = old.clone();
        custom.preset = Preset::Custom;
        let before = custom.clone();
        custom.upgrade_zoom_defaults();
        assert_eq!(custom, before);
        old.upgrade_zoom_defaults();
        assert_eq!(
            old.key_for("zoomIn", Mode::Global, false),
            Some("CmdOrCtrl+=")
        );
        assert_eq!(
            old.key_for("zoomOut", Mode::Global, false),
            Some("CmdOrCtrl+-")
        );
        assert_eq!(
            old.key_for("fitWaveform", Mode::Global, false),
            Some("CmdOrCtrl+0")
        );
        old.validate().unwrap();
    }
}
