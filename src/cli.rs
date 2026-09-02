use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use clap::Parser;
use crossterm::tty::IsTty;

use crate::config::Config;
use crate::modes::{
    Mode, ModeName, DEFAULT_TIME_SECONDS, DEFAULT_WORD_COUNT, MAX_TIME_SECONDS, MAX_WORD_COUNT,
};
use crate::themes::Theme;

pub const MAX_CUSTOM_TEXT_BYTES: u64 = 1024 * 1024;

#[derive(Parser, Debug)]
#[command(
    name = "cmd-type",
    version,
    about = "A terminal-based typing test inspired by Monkeytype"
)]
pub struct Cli {
    /// Typing mode: timed, words, quote, custom, endless
    #[arg(long)]
    pub mode: Option<ModeName>,

    /// Word count for words mode (default 25, maximum 10000)
    #[arg(long, value_parser = parse_word_count)]
    pub words: Option<usize>,

    /// Time in seconds for timed mode (default 30, maximum 3600)
    #[arg(long, value_parser = parse_time)]
    pub time: Option<u32>,

    /// Theme name
    #[arg(long)]
    pub theme: Option<String>,

    /// Custom text file to type (maximum 1 MiB)
    #[arg(long, value_name = "PATH")]
    pub file: Option<PathBuf>,

    /// Custom word list for timed, words, or endless modes (maximum 1 MiB)
    #[arg(long, value_name = "PATH")]
    pub wordlist: Option<PathBuf>,

    /// Show test history and exit
    #[arg(long)]
    pub history: bool,
}

impl Cli {
    pub fn validate(&self) -> io::Result<()> {
        if self.history
            && (self.mode.is_some()
                || self.words.is_some()
                || self.time.is_some()
                || self.theme.is_some()
                || self.file.is_some()
                || self.wordlist.is_some())
        {
            return invalid("--history cannot be combined with test or theme options");
        }

        if let Some(path) = &self.file {
            if self.mode.is_some_and(|mode| mode != ModeName::Custom) {
                return invalid(format!(
                    "--file {} conflicts with a non-custom --mode",
                    path.display()
                ));
            }
            if self.wordlist.is_some() {
                return invalid("--file and --wordlist cannot be used together");
            }
        }

        if self.words.is_some() && self.mode != Some(ModeName::Words) {
            return invalid("--words requires --mode words");
        }
        if self.time.is_some() && self.mode != Some(ModeName::Timed) {
            return invalid("--time requires --mode timed");
        }
        if self.wordlist.is_some()
            && self
                .mode
                .is_some_and(|mode| matches!(mode, ModeName::Quote | ModeName::Custom))
        {
            return invalid("--wordlist is only valid with timed, words, or endless mode");
        }
        if let Some(theme) = &self.theme {
            if Theme::by_name(theme).is_none() {
                return invalid(format!(
                    "unknown theme {theme:?}; expected one of: {}",
                    Theme::names().join(", ")
                ));
            }
        }
        Ok(())
    }

    pub fn mode_from_cli(&self) -> Option<Mode> {
        self.mode.map(|mode| {
            mode.into_mode(
                self.time.unwrap_or(DEFAULT_TIME_SECONDS),
                self.words.unwrap_or(DEFAULT_WORD_COUNT),
            )
        })
    }

    /// Load custom text without modifying its leading or trailing whitespace.
    pub fn custom_text(&self) -> io::Result<Option<String>> {
        if let Some(path) = &self.file {
            return read_utf8_limited(path, MAX_CUSTOM_TEXT_BYTES)
                .and_then(nonempty)
                .map(Some);
        }

        if self.mode == Some(ModeName::Custom) && !io::stdin().is_tty() {
            let text = read_from_limited(io::stdin().lock(), MAX_CUSTOM_TEXT_BYTES, "stdin")?;
            return nonempty(text).map(Some);
        }
        Ok(None)
    }

    pub fn apply_overrides(&self, config: &mut Config) -> io::Result<()> {
        if let Some(theme) = &self.theme {
            config.theme.clone_from(theme);
        }
        if let Some(mode) = self.mode_from_cli() {
            config.default_mode = mode;
        } else if self.file.is_some() {
            config.default_mode = Mode::Custom;
        }
        config.validate()
    }

    pub fn starts_immediately(&self) -> bool {
        self.mode.is_some() || self.file.is_some()
    }

    pub fn applicable_wordlist(&self, mode: &Mode) -> Option<&Path> {
        let interactive_selection = self.mode.is_none() && self.file.is_none();
        (interactive_selection || mode.uses_generated_words())
            .then_some(self.wordlist.as_deref())
            .flatten()
    }
}

pub fn read_utf8_limited(path: &Path, limit: u64) -> io::Result<String> {
    let file = File::open(path)?;
    let label = path.display().to_string();
    read_from_limited(file, limit, &label)
}

fn read_from_limited<R: Read>(reader: R, limit: u64, label: &str) -> io::Result<String> {
    let mut bytes = Vec::new();
    reader.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{label} exceeds the {limit} byte limit"),
        ));
    }
    String::from_utf8(bytes).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{label} is not valid UTF-8: {error}"),
        )
    })
}

fn nonempty(text: String) -> io::Result<String> {
    if text.is_empty() {
        invalid("custom text is empty")
    } else {
        let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
        if let Some(character) = normalized
            .chars()
            .find(|character| character.is_control() && !matches!(character, '\n' | '\t'))
        {
            return invalid(format!(
                "custom text contains untypeable control character U+{:04X}",
                character as u32
            ));
        }
        Ok(normalized)
    }
}

fn parse_word_count(value: &str) -> Result<usize, String> {
    let value = value
        .parse::<usize>()
        .map_err(|_| "word count must be an integer".to_string())?;
    if (1..=MAX_WORD_COUNT).contains(&value) {
        Ok(value)
    } else {
        Err(format!("word count must be between 1 and {MAX_WORD_COUNT}"))
    }
}

fn parse_time(value: &str) -> Result<u32, String> {
    let value = value
        .parse::<u32>()
        .map_err(|_| "time must be an integer".to_string())?;
    if (1..=MAX_TIME_SECONDS).contains(&value) {
        Ok(value)
    } else {
        Err(format!(
            "time must be between 1 and {MAX_TIME_SECONDS} seconds"
        ))
    }
}

fn invalid<T>(message: impl Into<String>) -> io::Result<T> {
    Err(io::Error::new(io::ErrorKind::InvalidInput, message.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cli() -> Cli {
        Cli {
            mode: None,
            words: None,
            time: None,
            theme: None,
            file: None,
            wordlist: None,
            history: false,
        }
    }

    #[test]
    fn mode_defaults_are_applied_only_to_relevant_modes() {
        let mut value = cli();
        value.mode = Some(ModeName::Timed);
        assert_eq!(value.mode_from_cli(), Some(Mode::Timed(30)));
        value.mode = Some(ModeName::Words);
        assert_eq!(value.mode_from_cli(), Some(Mode::Words(25)));
    }

    #[test]
    fn custom_text_preserves_outer_and_internal_whitespace() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("custom.txt");
        std::fs::write(&path, "  first\r\n\tsecond  ").unwrap();
        let mut value = cli();
        value.file = Some(path);
        assert_eq!(value.custom_text().unwrap().unwrap(), "  first\n\tsecond  ");
    }

    #[test]
    fn custom_text_rejects_untypeable_control_characters() {
        assert!(nonempty("hello\u{7f}".to_string()).is_err());
        assert!(nonempty("hello\n\tworld".to_string()).is_ok());
    }

    #[test]
    fn conflicting_options_are_rejected() {
        let mut value = cli();
        value.mode = Some(ModeName::Quote);
        value.wordlist = Some("words.txt".into());
        assert!(value.validate().is_err());

        let mut value = cli();
        value.words = Some(10);
        assert!(value.validate().is_err());
    }

    #[test]
    fn numeric_boundaries_are_validated_by_clap() {
        assert!(Cli::try_parse_from(["cmd-type", "--mode", "words", "--words", "0"]).is_err());
        assert!(Cli::try_parse_from(["cmd-type", "--mode", "timed", "--time", "3601"]).is_err());
    }

    #[test]
    fn wordlist_alone_does_not_start_a_test() {
        let mut value = cli();
        value.wordlist = Some("words.txt".into());
        assert!(!value.starts_immediately());
    }
}
