use std::collections::HashSet;
use std::io;
use std::path::Path;

use rand::seq::SliceRandom;
use rand::thread_rng;

use crate::cli::{read_utf8_limited, MAX_CUSTOM_TEXT_BYTES};

const EMBEDDED_WORDS: &str = include_str!("../assets/words.txt");
const EMBEDDED_QUOTES: &str = include_str!("../assets/quotes.json");
const MAX_QUOTE_BYTES: usize = 16 * 1024;

#[derive(Debug)]
pub struct TextGenerator {
    words: Vec<String>,
    quotes: Vec<String>,
}

impl TextGenerator {
    pub fn new(wordlist: Option<&Path>) -> io::Result<Self> {
        let words = match wordlist {
            Some(path) => {
                let source = read_utf8_limited(path, MAX_CUSTOM_TEXT_BYTES)?;
                Self::parse_words(&source)
            }
            None => Self::parse_words(EMBEDDED_WORDS),
        };
        let words = validate_words(words)?;
        let quotes = parse_quotes(EMBEDDED_QUOTES)?;
        Ok(Self { words, quotes })
    }

    /// Split on Unicode whitespace, remove duplicates, and preserve source order.
    pub fn parse_words(source: &str) -> Vec<String> {
        let mut seen = HashSet::new();
        source
            .split_whitespace()
            .filter(|word| seen.insert((*word).to_owned()))
            .map(str::to_owned)
            .collect()
    }

    pub fn random_words(&self, count: usize) -> String {
        let mut rng = thread_rng();
        (0..count)
            .filter_map(|_| self.words.choose(&mut rng).map(String::as_str))
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub fn random_quote(&self) -> String {
        self.quotes
            .choose(&mut thread_rng())
            .expect("validated embedded quote list is non-empty")
            .clone()
    }
}

fn validate_words(words: Vec<String>) -> io::Result<Vec<String>> {
    if words.is_empty() {
        return invalid("word list contains no words");
    }
    if let Some(word) = words.iter().find(|word| word.chars().any(char::is_control)) {
        return invalid(format!(
            "word list entry {word:?} contains a control character"
        ));
    }
    Ok(words)
}

fn parse_quotes(source: &str) -> io::Result<Vec<String>> {
    let quotes: Vec<String> = serde_json::from_str(source).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("embedded quote corpus is malformed: {error}"),
        )
    })?;
    if quotes.is_empty() {
        return invalid("embedded quote corpus contains no quotes");
    }
    for quote in &quotes {
        if quote.trim().is_empty() {
            return invalid("embedded quote corpus contains an empty quote");
        }
        if quote.len() > MAX_QUOTE_BYTES {
            return invalid("embedded quote corpus contains an oversized quote");
        }
        if quote.chars().any(char::is_control) {
            return invalid(format!("quote {quote:?} contains a control character"));
        }
    }
    Ok(quotes)
}

fn invalid<T>(message: impl Into<String>) -> io::Result<T> {
    Err(io::Error::new(io::ErrorKind::InvalidData, message.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_assets_are_valid_and_hermetic() {
        let generator = TextGenerator::new(None).unwrap();
        assert!(generator.words.len() > 100);
        assert!(generator.quotes.len() >= 20);
    }

    #[test]
    fn random_words_returns_requested_count() {
        let text = TextGenerator::new(None).unwrap().random_words(25);
        assert_eq!(text.split_whitespace().count(), 25);
    }

    #[test]
    fn paragraphs_are_valid_deduplicated_wordlists() {
        assert_eq!(
            TextGenerator::parse_words("one two\n\nthree\ttwo four"),
            ["one", "two", "three", "four"]
        );
    }

    #[test]
    fn a_missing_explicit_wordlist_is_an_error() {
        let directory = tempfile::tempdir().unwrap();
        let error = TextGenerator::new(Some(&directory.path().join("missing.txt"))).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn malformed_and_invalid_quotes_are_rejected() {
        assert!(parse_quotes("not json").is_err());
        assert!(parse_quotes("[]").is_err());
        assert!(parse_quotes("[\"ok\", \"bad\\nquote\"]").is_err());
    }
}
