use std::fs;
use std::io::{self, Read};
use std::path::Path;
use std::path::PathBuf;

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::modes::Mode;
use crate::themes::Theme;

const MAX_CONFIG_BYTES: u64 = 64 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub schema_version: u32,
    pub theme: String,
    pub default_mode: Mode,
    pub sound: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            schema_version: 1,
            theme: "catppuccin_mocha".to_string(),
            default_mode: Mode::Timed(30),
            sound: false,
        }
    }
}

impl Config {
    pub fn config_dir() -> io::Result<PathBuf> {
        ProjectDirs::from("", "", "tui-type")
            .map(|dirs| dirs.config_dir().to_path_buf())
            .ok_or_else(|| io::Error::other("could not determine the platform configuration path"))
    }

    pub fn config_path() -> io::Result<PathBuf> {
        Ok(Self::config_dir()?.join("config.toml"))
    }

    pub fn history_path() -> io::Result<PathBuf> {
        Ok(Self::config_dir()?.join("history.json"))
    }

    pub fn load() -> io::Result<Self> {
        let path = Self::config_path()?;
        Self::load_from(&path)
    }

    pub fn load_from(path: &Path) -> io::Result<Self> {
        if path.exists() {
            let content = read_limited(path, MAX_CONFIG_BYTES)?;
            let config: Config = toml::from_str(&content).map_err(io::Error::other)?;
            config.validate()?;
            Ok(config)
        } else {
            let config = Config::default();
            config.save_to(path)?;
            Ok(config)
        }
    }

    pub fn save_to(&self, path: &Path) -> io::Result<()> {
        self.validate()?;
        let dir = path
            .parent()
            .ok_or_else(|| io::Error::other("configuration path has no parent directory"))?;
        fs::create_dir_all(dir)?;
        let content = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        fs::write(path, content)
    }

    pub fn validate(&self) -> io::Result<()> {
        if self.schema_version != 1 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "unsupported configuration schema version {} (expected 1)",
                    self.schema_version
                ),
            ));
        }
        if Theme::by_name(&self.theme).is_none() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "unknown theme {:?}; expected one of: {}",
                    self.theme,
                    Theme::names().join(", ")
                ),
            ));
        }
        self.default_mode
            .validate()
            .map_err(|message| io::Error::new(io::ErrorKind::InvalidData, message))
    }
}

fn read_limited(path: &Path, limit: u64) -> io::Result<String> {
    let file = fs::File::open(path)?;
    if file.metadata()?.len() > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{} exceeds the {} byte limit", path.display(), limit),
        ));
    }
    let mut content = String::new();
    file.take(limit + 1).read_to_string(&mut content)?;
    if content.len() as u64 > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{} exceeds the {} byte limit", path.display(), limit),
        ));
    }
    Ok(content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_fields_receive_schema_defaults() {
        let config: Config = toml::from_str("theme = \"dracula\"").unwrap();
        assert_eq!(config.schema_version, 1);
        assert_eq!(config.default_mode, Mode::Timed(30));
        assert!(!config.sound);
        config.validate().unwrap();
    }

    #[test]
    fn invalid_theme_is_reported() {
        let config = Config {
            theme: "not-a-theme".into(),
            ..Config::default()
        };
        assert!(config
            .validate()
            .unwrap_err()
            .to_string()
            .contains("unknown theme"));
    }
}
