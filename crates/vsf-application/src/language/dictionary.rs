use super::Language;
use crate::quality::{Error, Result};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, sync::OnceLock};
use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};

pub const MAX_WORD_CHARS: usize = 32;
pub const MAX_WORD_BYTES: usize = 128;
const MAX_USER_WORDS: usize = 2048;

/// Serializable user data. Never mutate the shared bundled dictionaries.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WordLists {
    pub english_us: Vec<String>,
    pub french: Vec<String>,
}
impl WordLists {
    pub fn validate(&self) -> Result<()> {
        if self.english_us.len() + self.french.len() > MAX_USER_WORDS {
            return Err(Error::Limit(
                "Personal or ignored words exceed 2048 entries".into(),
            ));
        }
        for words in [&self.english_us, &self.french] {
            let mut seen = HashSet::new();
            for word in words {
                if !valid_word(word) || !seen.insert(key(word)) {
                    return Err(Error::Invalid("Words must be unique per language, contain letters, and use at most 32 characters / 128 UTF-8 bytes".into()));
                }
            }
        }
        Ok(())
    }
    pub fn words_mut(&mut self, language: Language) -> &mut Vec<String> {
        match language {
            Language::EnglishUs => &mut self.english_us,
            Language::French => &mut self.french,
        }
    }
    pub fn insert(&mut self, language: Language, word: &str) -> Result<()> {
        self.validate()?;
        bounded(word)?;
        let full = self.english_us.len() + self.french.len() == MAX_USER_WORDS;
        let words = self.words_mut(language);
        if !words.iter().any(|w| key(w) == key(word)) {
            if full {
                return Err(Error::Limit(
                    "Personal or ignored words exceed 2048 entries".into(),
                ));
            }
            words.push(word.into());
        }
        Ok(())
    }
    pub fn remove(&mut self, language: Language, word: &str) -> Result<()> {
        bounded(word)?;
        let wanted = key(word);
        self.words_mut(language).retain(|w| key(w) != wanted);
        Ok(())
    }
}

/// Immutable, normalized lookup sets captured with an evaluation. Construction is
/// bounded and happens once per capture, never once per token. Case/apostrophe
/// normalization applies to membership only; saved personal/source text is exact.
#[derive(Clone, Debug, Default)]
pub struct Lexicon {
    english_us: HashSet<String>,
    french: HashSet<String>,
}
impl Lexicon {
    pub fn is_empty(&self) -> bool {
        self.english_us.is_empty() && self.french.is_empty()
    }
    pub fn new(personal: &WordLists, ignored: &WordLists) -> Result<Self> {
        personal.validate()?;
        ignored.validate()?;
        Ok(Self {
            english_us: personal
                .english_us
                .iter()
                .chain(&ignored.english_us)
                .map(|w| key(w))
                .collect(),
            french: personal
                .french
                .iter()
                .chain(&ignored.french)
                .map(|w| key(w))
                .collect(),
        })
    }
    pub fn contains(&self, language: Language, word: &str) -> bool {
        let words = match language {
            Language::EnglishUs => &self.english_us,
            Language::French => &self.french,
        };
        !words.is_empty() && words.contains(&key(word))
    }
}
fn lookup(word: &str) -> String {
    word.replace('’', "'").nfc().collect()
}
fn key(word: &str) -> String {
    lookup(word).to_lowercase().nfc().collect()
}
pub(super) fn valid_word(word: &str) -> bool {
    !word.is_empty()
        && word.len() <= MAX_WORD_BYTES
        && word.chars().count() <= MAX_WORD_CHARS
        && word.chars().any(char::is_alphabetic)
        && word
            .chars()
            .all(|c| c.is_alphanumeric() || is_combining_mark(c) || matches!(c, '\'' | '’' | '-'))
        && word.chars().next().is_some_and(char::is_alphanumeric)
        && word
            .chars()
            .last()
            .is_some_and(|c| c.is_alphanumeric() || is_combining_mark(c))
}
fn bounded(word: &str) -> Result<()> {
    if !valid_word(word) {
        return Err(Error::Limit(
            "Spelling supports one word of at most 32 characters / 128 UTF-8 bytes".into(),
        ));
    }
    Ok(())
}

type Dictionary = std::result::Result<spellbook::Dictionary, String>;
static ENGLISH: OnceLock<Dictionary> = OnceLock::new();
static FRENCH: OnceLock<Dictionary> = OnceLock::new();
fn dictionary(language: Language) -> Result<&'static spellbook::Dictionary> {
    let (cache, aff, dic) = match language {
        Language::EnglishUs => (
            &ENGLISH,
            include_str!("../../assets/dictionaries/en-US.aff"),
            include_str!("../../assets/dictionaries/en-US.dic"),
        ),
        Language::French => (
            &FRENCH,
            include_str!("../../assets/dictionaries/fr.aff"),
            include_str!("../../assets/dictionaries/fr.dic"),
        ),
    };
    cache
        .get_or_init(|| spellbook::Dictionary::new(aff, dic).map_err(|e| e.to_string()))
        .as_ref()
        .map_err(|e| {
            Error::Invalid(format!(
                "Bundled {} dictionary could not load: {e}",
                language.name()
            ))
        })
}

/// Call off the UI thread. Fixed verified data bounds initialization; the engine
/// does not expose an interruption point inside one bounded lookup/suggestion.
pub fn is_correct(language: Language, word: &str, lexicon: &Lexicon) -> Result<bool> {
    bounded(word)?;
    if lexicon.contains(language, word) {
        return Ok(true);
    }
    Ok(dictionary(language)?.check(&lookup(word)))
}
pub fn suggestions(
    language: Language,
    word: &str,
    mut checkpoint: impl FnMut() -> Result<()>,
) -> Result<Vec<String>> {
    checkpoint()?;
    bounded(word)?;
    let dict = dictionary(language)?;
    checkpoint()?;
    let mut result = vec![];
    dict.suggester()
        .with_ngram_suggestions(false)
        .suggest(&lookup(word), &mut result);
    checkpoint()?;
    let mut seen = HashSet::new();
    result.retain(|s| {
        s.len() <= MAX_WORD_BYTES && !s.chars().any(char::is_control) && seen.insert(s.clone())
    });
    result.truncate(8);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bundled_lookup_is_unicode_aware_without_mutating_personal_sources() {
        let personal = WordLists {
            english_us: vec!["Shinjix".into()],
            french: vec!["Élodiex".into()],
        };
        let ignored = WordLists {
            english_us: vec!["Fanword".into()],
            ..WordLists::default()
        };
        let lexicon = Lexicon::new(&personal, &ignored).unwrap();
        for (lang, word) in [
            (Language::EnglishUs, "can't"),
            (Language::EnglishUs, "doesn’t"),
            (Language::EnglishUs, "SHINJIX"),
            (Language::EnglishUs, "fanword"),
            (Language::French, "e\u{301}le\u{300}ves"),
            (Language::French, "l’amour"),
            (Language::French, "E\u{301}LODIEX"),
        ] {
            assert!(is_correct(lang, word, &lexicon).unwrap(), "{lang:?} {word}");
        }
        assert!(!is_correct(Language::French, "Shinjix", &lexicon).unwrap());
        assert!(!is_correct(Language::EnglishUs, "Shinjix", &Lexicon::default()).unwrap());
        assert_eq!(personal.french, ["Élodiex"]);
    }
    #[test]
    fn suggestions_and_word_limits_are_bounded_and_cancellable() {
        for (lang, word, expected) in [
            (Language::EnglishUs, "helo", "hello"),
            (Language::French, "bonjor", "bonjour"),
        ] {
            let result = suggestions(lang, word, || Ok(())).unwrap();
            assert!(result.len() <= 8 && result.iter().any(|s| s == expected));
            assert_eq!(
                suggestions(lang, word, || Err(Error::Cancelled)),
                Err(Error::Cancelled)
            );
            let mut calls = 0;
            assert_eq!(
                suggestions(lang, word, || {
                    calls += 1;
                    if calls == 3 {
                        Err(Error::Cancelled)
                    } else {
                        Ok(())
                    }
                }),
                Err(Error::Cancelled)
            );
        }
        for word in [
            "",
            "word\n",
            "two words",
            "<i>hello",
            "--word",
            &"x".repeat(33),
        ] {
            assert!(suggestions(Language::EnglishUs, word, || Ok(())).is_err());
        }
        assert!(
            WordLists {
                english_us: vec!["Name".into(), "NAME".into()],
                ..WordLists::default()
            }
            .validate()
            .is_err()
        );
        assert!(
            WordLists {
                french: vec!["word".into(); 2049],
                ..WordLists::default()
            }
            .validate()
            .is_err()
        );
    }
}
