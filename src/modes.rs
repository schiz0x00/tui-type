use clap::ValueEnum;
use serde::{Deserialize, Serialize};

pub const DEFAULT_TIME_SECONDS: u32 = 30;
pub const DEFAULT_WORD_COUNT: usize = 25;
pub const MAX_TIME_SECONDS: u32 = 3_600;
pub const MAX_WORD_COUNT: usize = 10_000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Mode {
    Timed(u32),
    Words(usize),
    Quote,
    Custom,
    Endless,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ModeName {
    Timed,
    Words,
    Quote,
    Custom,
    Endless,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModeConfig {
    Timed(u32),
    Words(usize),
    Quote,
    Custom,
    Endless,
}

impl Mode {
    pub fn all() -> Vec<Mode> {
        vec![
            Mode::Timed(15),
            Mode::Timed(30),
            Mode::Timed(60),
            Mode::Timed(120),
            Mode::Words(10),
            Mode::Words(25),
            Mode::Words(50),
            Mode::Words(100),
            Mode::Quote,
            Mode::Custom,
            Mode::Endless,
        ]
    }

    pub fn label(&self) -> String {
        match self {
            Mode::Timed(s) => format!("time {}", s),
            Mode::Words(n) => format!("words {}", n),
            Mode::Quote => "quote".to_string(),
            Mode::Custom => "custom".to_string(),
            Mode::Endless => "endless".to_string(),
        }
    }

    pub fn short_label(&self) -> String {
        match self {
            Mode::Timed(s) => format!("{}s", s),
            Mode::Words(n) => format!("{}w", n),
            Mode::Quote => "quote".to_string(),
            Mode::Custom => "custom".to_string(),
            Mode::Endless => "∞".to_string(),
        }
    }

    pub fn to_config(&self) -> ModeConfig {
        match self {
            Mode::Timed(s) => ModeConfig::Timed(*s),
            Mode::Words(n) => ModeConfig::Words(*n),
            Mode::Quote => ModeConfig::Quote,
            Mode::Custom => ModeConfig::Custom,
            Mode::Endless => ModeConfig::Endless,
        }
    }

    pub fn uses_generated_words(&self) -> bool {
        matches!(self, Mode::Timed(_) | Mode::Words(_) | Mode::Endless)
    }

    pub fn validate(&self) -> Result<(), String> {
        match self {
            Mode::Timed(seconds) if !(1..=MAX_TIME_SECONDS).contains(seconds) => Err(format!(
                "timed duration must be between 1 and {MAX_TIME_SECONDS} seconds"
            )),
            Mode::Words(words) if !(1..=MAX_WORD_COUNT).contains(words) => {
                Err(format!("word count must be between 1 and {MAX_WORD_COUNT}"))
            }
            _ => Ok(()),
        }
    }
}

impl ModeName {
    pub fn into_mode(self, time: u32, words: usize) -> Mode {
        match self {
            Self::Timed => Mode::Timed(time),
            Self::Words => Mode::Words(words),
            Self::Quote => Mode::Quote,
            Self::Custom => Mode::Custom,
            Self::Endless => Mode::Endless,
        }
    }
}

impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}
