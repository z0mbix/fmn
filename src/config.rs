use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::{Error, Result};

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub history: HistoryConfig,
    pub ui: UiConfig,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Theme {
    #[default]
    Mint,
    Rose,
    Iris,
    Terminal,
    NoColor,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct UiConfig {
    pub theme: Theme,
    pub show_inspector: bool,
    pub execute_on_select: bool,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: Theme::default(),
            show_inspector: true,
            execute_on_select: false,
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct HistoryConfig {
    pub ignore_leading_space: bool,
    pub ignore_patterns: Vec<String>,
}

impl Default for HistoryConfig {
    fn default() -> Self {
        Self {
            ignore_leading_space: true,
            ignore_patterns: Vec::new(),
        }
    }
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        match fs::read_to_string(path) {
            Ok(contents) => toml::from_str(&contents).map_err(|source| Error::InvalidConfig {
                path: path.to_path_buf(),
                source,
            }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Config;

    #[test]
    fn missing_config_uses_defaults() {
        let root = tempfile::tempdir().unwrap();
        let config = Config::load(&root.path().join("missing.toml")).unwrap();
        assert!(config.history.ignore_leading_space);
        assert!(config.history.ignore_patterns.is_empty());
    }

    #[test]
    fn loads_valid_config() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.toml");
        std::fs::write(
            &path,
            "[history]\nignore_leading_space = false\nignore_patterns = ['secret *']\n",
        )
        .unwrap();
        let config = Config::load(&path).unwrap();
        assert!(!config.history.ignore_leading_space);
        assert_eq!(config.history.ignore_patterns, ["secret *"]);
    }

    #[test]
    fn rejects_unknown_fields() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.toml");
        std::fs::write(&path, "[history]\nunknown = true\n").unwrap();
        assert!(Config::load(&path).is_err());
    }

    #[test]
    fn ui_defaults_and_theme_names_are_validated() {
        use super::Theme;

        let default: Config = toml::from_str("").unwrap();
        assert_eq!(default.ui.theme, Theme::Mint);
        assert!(default.ui.show_inspector);
        assert!(!default.ui.execute_on_select);
        let config: Config =
            toml::from_str("[ui]\ntheme = 'rose'\nshow_inspector = false").unwrap();
        assert_eq!(config.ui.theme, Theme::Rose);
        assert!(!config.ui.show_inspector);
        assert!(toml::from_str::<Config>("[ui]\ntheme = 'unknown'").is_err());
        assert!(toml::from_str::<Config>("[ui]\nunknown = true").is_err());
    }

    #[test]
    fn execution_requires_an_explicit_boolean_opt_in() {
        for contents in ["", "[ui]", "[ui]\nexecute_on_select = false"] {
            assert!(
                !toml::from_str::<Config>(contents)
                    .unwrap()
                    .ui
                    .execute_on_select
            );
        }
        assert!(
            toml::from_str::<Config>("[ui]\nexecute_on_select = true")
                .unwrap()
                .ui
                .execute_on_select
        );
        assert!(toml::from_str::<Config>("[ui]\nexecute_on_select = 'true'").is_err());
    }
}
