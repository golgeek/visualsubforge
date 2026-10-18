//! Candidate-engine evidence. Product language integration remains a separate task.
use std::time::Instant;
use unicode_normalization::UnicodeNormalization;
use unicode_segmentation::UnicodeSegmentation;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    for (language, aff, dic, correct, typo, expected) in [
        (
            "en-US",
            include_str!("../assets/dictionaries/en-US.aff"),
            include_str!("../assets/dictionaries/en-US.dic"),
            vec!["Hello", "world", "can't", "doesn't", "subtitle", "color"],
            "helo",
            "hello",
        ),
        (
            "fr",
            include_str!("../assets/dictionaries/fr.aff"),
            include_str!("../assets/dictionaries/fr.dic"),
            vec![
                "bonjour",
                "élèves",
                "mangeaient",
                "l'amour",
                "cœur",
                "e\u{301}le\u{300}ves",
            ],
            "bonjor",
            "bonjour",
        ),
    ] {
        let started = Instant::now();
        let dictionary = spellbook::Dictionary::new(aff, dic)
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        let cold_ms = started.elapsed().as_secs_f64() * 1000.;
        assert!(
            cold_ms < 2000.,
            "Cold dictionary initialization exceeded 2s"
        );
        for word in correct {
            let nfc: String = word.nfc().collect();
            assert!(dictionary.check(&nfc), "{language}: {word}");
        }
        assert!(!dictionary.check(typo));
        let mut suggestions = vec![];
        let started = Instant::now();
        dictionary
            .suggester()
            .with_ngram_suggestions(false)
            .suggest(typo, &mut suggestions);
        let suggestion_ms = started.elapsed().as_secs_f64() * 1000.;
        assert!(suggestion_ms < 250., "Suggestion exceeded 250ms");
        assert!(
            suggestions.iter().any(|s| s.eq_ignore_ascii_case(expected)),
            "{language}: {suggestions:?}"
        );
        let mut worst_ms: f64 = 0.;
        for word in [
            "x".repeat(32),
            "a".repeat(32),
            "qwertyuiopasdfghjklzxcvbnm".into(),
            "é".repeat(32),
        ] {
            let started = Instant::now();
            dictionary
                .suggester()
                .with_ngram_suggestions(false)
                .suggest(&word, &mut suggestions);
            worst_ms = worst_ms.max(started.elapsed().as_secs_f64() * 1000.);
        }
        assert!(
            worst_ms < 250.,
            "Bounded difficult suggestion exceeded 250ms: {language} {worst_ms}"
        );
        let text = if language == "fr" {
            "Les élèves mangeaient. Bonjour, l'amour !"
        } else {
            "Hello, the world doesn't need another subtitle."
        };
        let started = Instant::now();
        let mut words = 0;
        for _ in 0..10_000 {
            for (_, word) in text.unicode_word_indices() {
                let nfc: String = word.nfc().collect();
                assert!(dictionary.check(&nfc), "{language}: {word}");
                words += 1;
            }
        }
        let check_ms = started.elapsed().as_secs_f64() * 1000.;
        println!(
            "{}",
            serde_json::json!({"language":language,"cold_ms":cold_ms,"suggestion_ms":suggestion_ms,"difficult_suggestion_max_ms":worst_ms,"checked_words":words,"check_10000_cues_ms":check_ms})
        );
    }
    Ok(())
}
