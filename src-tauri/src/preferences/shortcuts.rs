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
            if binding.key.is_empty() {
                continue;
            }
            validate_key(binding)?;
            if let Some(other) = self.bindings[..index].iter().find(|other| {
                other.mode.overlaps(binding.mode)
                    && canonical(&other.key) == canonical(&binding.key)
            }) {
                return Err(format!(
                    "{} conflicts with {} in an overlapping focus mode",
                    binding.action, other.action
                ));
            }
        }
        // OS Quit and editor escape/formatting retain their established owners.
        for (action, key) in [("quit", "CmdOrCtrl+Q"), ("focusWaveform", "Escape")] {
            if !self
                .bindings
                .iter()
                .any(|b| b.action == action && b.mode == Mode::Global && b.key == key)
            {
                return Err(format!("{action} keeps its reserved {key} binding"));
            }
        }
        Ok(())
    }
    pub fn native_key(&self, action: &str) -> Option<&str> {
        self.bindings
            .iter()
            .find(|b| b.action == action && b.mode == Mode::Global && !b.key.is_empty())
            .map(|b| b.key.as_str())
    }
}
fn canonical(key: &str) -> String {
    key.split('+')
        .map(|part| if part == "Enter" { "Return" } else { part })
        .collect::<Vec<_>>()
        .join("+")
}
fn validate_key(binding: &Binding) -> Result<(), String> {
    let parts: Vec<_> = binding.key.split('+').collect();
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
    if binding.key.len() > 48
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
    if reserved.contains(&binding.key.as_str())
        || (binding.key == "Escape" && binding.action != "focusWaveform")
        || (binding.key == "CmdOrCtrl+Q" && binding.action != "quit")
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
            });
            assert!(value.validate().is_err(), "{key}");
        }
        let mut value = Shortcuts::default();
        value.bindings[0].action = "notAnAction".into();
        assert!(value.validate().is_err());
    }
}
