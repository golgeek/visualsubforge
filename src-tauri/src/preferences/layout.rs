//! User workspace placement only; never part of subtitle state or history.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum Preset {
    Timing,
    EditorAbove,
    Review,
    Translation,
    BigPicture,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PaneSizes {
    #[serde(with = "crate::ipc_number::finite")]
    #[specta(type = crate::ipc_number::finite::FiniteF64)]
    pub top_pct: f64,
    #[serde(with = "crate::ipc_number::finite")]
    #[specta(type = crate::ipc_number::finite::FiniteF64)]
    pub editor_pct: f64,
    #[serde(with = "crate::ipc_number::finite")]
    #[specta(type = crate::ipc_number::finite::FiniteF64)]
    pub wave_pct: f64,
}
impl PaneSizes {
    fn validate(&self) -> Result<(), String> {
        for (value, minimum, maximum) in [
            (self.top_pct, 10.0, 65.0),
            (self.editor_pct, 10.0, 45.0),
            (self.wave_pct, 20.0, 80.0),
        ] {
            if !value.is_finite() || !(minimum..=maximum).contains(&value) {
                return Err("Layout pane size is outside its supported range".into());
            }
        }
        Ok(())
    }
}
impl Default for PaneSizes {
    fn default() -> Self {
        Self {
            top_pct: 15.0,
            editor_pct: 16.0,
            wave_pct: 65.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LayoutPreferences {
    pub active: Preset,
    pub timing: PaneSizes,
    pub editor_above: PaneSizes,
    pub review: PaneSizes,
    pub translation: PaneSizes,
    pub big_picture: PaneSizes,
}
impl Default for LayoutPreferences {
    fn default() -> Self {
        Self {
            active: Preset::Timing,
            timing: Default::default(),
            editor_above: Default::default(),
            review: Default::default(),
            translation: Default::default(),
            big_picture: PaneSizes {
                top_pct: 40.0,
                wave_pct: 35.0,
                ..Default::default()
            },
        }
    }
}
impl LayoutPreferences {
    pub fn validate(&self) -> Result<(), String> {
        for sizes in [
            &self.timing,
            &self.editor_above,
            &self.review,
            &self.translation,
            &self.big_picture,
        ] {
            sizes.validate()?;
        }
        Ok(())
    }
}
