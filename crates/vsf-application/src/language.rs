//! Language policies and bounded dictionary operations, independent of UI/IO.
mod analysis;
mod dictionary;
pub(crate) use analysis::Analysis;
pub use analysis::{Coverage, Detail};
pub use dictionary::{Lexicon, WordLists, is_correct, suggestions};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum Language {
    #[default]
    #[serde(rename = "en-US")]
    EnglishUs,
    #[serde(rename = "fr")]
    French,
}
impl Language {
    pub fn name(self) -> &'static str {
        match self {
            Self::EnglishUs => "English (United States), SCOWL 2020.12.07",
            Self::French => "French (Classique), Grammalecte 7.7",
        }
    }
}

/// Opt-in editorial aids. Existing profiles retain disabled defaults.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Settings {
    pub dictionary: Language,
    pub spelling: bool,
    pub typography: bool,
    pub capitalization: bool,
    pub dialogue_markers: bool,
}
impl Settings {
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }
    pub fn enabled(&self) -> bool {
        self.spelling || self.typography || self.capitalization || self.dialogue_markers
    }
}
