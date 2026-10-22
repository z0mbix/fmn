use std::fs;
use std::io::{self, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::keybindings::{KeyAction, KeyBinding, KeyBindings};
use crate::scope::SearchScope;
use crate::{Error, Result};

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Keymap {
    Function,
    #[default]
    Control,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiPreference {
    Theme(Theme),
    ShowInspector(bool),
    DefaultScope(SearchScope),
    ExecuteOnSelect(bool),
    Keybinding(KeyAction, KeyBinding),
    ResetKeybindings,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub history: HistoryConfig,
    pub ui: UiConfig,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Theme {
    #[default]
    Mint,
    Rose,
    Iris,
    Terminal,
    NoColor,
    AccessibleDark,
    AccessibleLight,
    Nord,
    RosePine,
    RosePineMoon,
    RosePineDawn,
    CatppuccinMocha,
    CatppuccinMacchiato,
    CatppuccinFrappe,
    CatppuccinLatte,
    Dracula,
    Material,
    Palenight,
    Darcula,
    Monokai,
    Ubuntu,
    Lemonade,
    PinkLemonade,
    GruvboxDark,
    GruvboxLight,
    TokyoNight,
    TokyoNightStorm,
    TokyoNightLight,
    OneDark,
    SolarizedDark,
    SolarizedLight,
    AyuDark,
    AyuMirage,
    AyuLight,
    EverforestDarkHard,
    EverforestDarkMedium,
    EverforestDarkSoft,
    EverforestLightHard,
    EverforestLightMedium,
    EverforestLightSoft,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct UiConfig {
    pub theme: Theme,
    pub show_inspector: bool,
    pub execute_on_select: bool,
    pub default_scope: SearchScope,
    // Accepted for config compatibility; individual bindings are authoritative.
    pub keymap: Keymap,
    pub keybindings: KeyBindings,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: Theme::default(),
            show_inspector: true,
            execute_on_select: false,
            default_scope: SearchScope::Global,
            keymap: Keymap::Control,
            keybindings: KeyBindings::default(),
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

    pub fn save_theme(path: &Path, theme: Theme) -> Result<()> {
        Self::save_preference(path, UiPreference::Theme(theme))
    }

    pub fn save_preference(path: &Path, preference: UiPreference) -> Result<()> {
        let save_error = |source| Error::SaveConfig {
            path: path.to_path_buf(),
            source,
        };
        // Follow existing dotfile symlinks rather than replacing the link.
        let target = match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.is_symlink() => fs::canonicalize(path).map_err(save_error)?,
            Ok(_) => path.to_path_buf(),
            Err(error) if error.kind() == io::ErrorKind::NotFound => path.to_path_buf(),
            Err(error) => return Err(save_error(error)),
        };
        let contents = match fs::read_to_string(&target) {
            Ok(contents) => contents,
            Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
            Err(error) => return Err(save_error(error)),
        };
        // Re-read and validate at confirmation time so edits made while the
        // picker was open survive, and invalid configuration is never replaced.
        toml::from_str::<Self>(&contents).map_err(|source| Error::InvalidConfig {
            path: path.to_path_buf(),
            source,
        })?;
        let mut document = contents
            .parse::<toml_edit::DocumentMut>()
            .map_err(|error| save_error(io::Error::new(io::ErrorKind::InvalidData, error)))?;
        let serialize = toml_edit::ser::ValueSerializer::new;
        let updates = match preference {
            UiPreference::Theme(value) => vec![(None, "theme", value.serialize(serialize()))],
            UiPreference::ShowInspector(value) => {
                vec![(None, "show_inspector", value.serialize(serialize()))]
            }
            UiPreference::DefaultScope(value) => {
                vec![(None, "default_scope", value.serialize(serialize()))]
            }
            UiPreference::ExecuteOnSelect(value) => {
                vec![(None, "execute_on_select", value.serialize(serialize()))]
            }
            UiPreference::Keybinding(action, value) => vec![(
                Some("keybindings"),
                action.config_name(),
                value.serialize(serialize()),
            )],
            UiPreference::ResetKeybindings => KeyAction::ALL
                .into_iter()
                .map(|action| {
                    (
                        Some("keybindings"),
                        action.config_name(),
                        action.default_binding().serialize(serialize()),
                    )
                })
                .collect(),
        };
        if !document.contains_key("ui") {
            document["ui"] = toml_edit::Item::Table(toml_edit::Table::new());
        }
        for (group, key, value) in updates {
            let mut value = value
                .map_err(|error| save_error(io::Error::new(io::ErrorKind::InvalidData, error)))?;
            let mut table = &mut document["ui"];
            if let Some(group) = group {
                if table
                    .as_table_like()
                    .is_none_or(|table| !table.contains_key(group))
                {
                    let child = if table.as_inline_table().is_some() {
                        toml_edit::Item::Value(toml_edit::Value::InlineTable(
                            toml_edit::InlineTable::new(),
                        ))
                    } else {
                        toml_edit::Item::Table(toml_edit::Table::new())
                    };
                    table[group] = child;
                }
                table = &mut table[group];
            }
            let item = &mut table[key];
            if let Some(previous) = item.as_value() {
                *value.decor_mut() = previous.decor().clone();
            }
            *item = toml_edit::Item::Value(value);
        }
        let updated = document.to_string();
        // A concurrent edit may have reassigned another action since the UI
        // opened. Never persist a map with duplicate or reserved bindings.
        toml::from_str::<Self>(&updated).map_err(|source| Error::InvalidConfig {
            path: path.to_path_buf(),
            source,
        })?;
        if updated == contents {
            return Ok(());
        }

        let permissions = match fs::metadata(&target) {
            Ok(metadata) => {
                let permissions = metadata.permissions();
                if permissions.readonly() {
                    return Err(save_error(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "configuration file is read-only",
                    )));
                }
                Some(permissions)
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(save_error(error)),
        };
        let parent = target
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let mut directory = fs::DirBuilder::new();
        directory.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            directory.mode(0o700);
        }
        directory.create(parent).map_err(save_error)?;
        let mut temporary = tempfile::Builder::new()
            .prefix(".fmn-config-")
            .tempfile_in(parent)
            .map_err(save_error)?;
        temporary
            .write_all(updated.as_bytes())
            .map_err(save_error)?;
        if let Some(permissions) = permissions {
            temporary
                .as_file()
                .set_permissions(permissions)
                .map_err(save_error)?;
        }
        temporary.as_file().sync_all().map_err(save_error)?;
        temporary
            .persist(&target)
            .map_err(|error| save_error(error.error))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{Config, Theme};
    use crate::keybindings::{KeyAction, KeyBinding};

    #[test]
    fn saving_theme_creates_only_the_preference_and_private_parent_directories() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("nested/fmn/config.toml");
        Config::save_theme(&path, Theme::CatppuccinMocha).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "[ui]\ntheme = \"catppuccin-mocha\"\n"
        );
        let config = Config::load(&path).unwrap();
        assert_eq!(config.ui.theme, Theme::CatppuccinMocha);
        assert!(!config.ui.execute_on_select);
        assert!(config.history.ignore_leading_space);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
            assert_eq!(
                std::fs::metadata(path.parent().unwrap())
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
        }
    }

    #[test]
    fn saving_theme_preserves_comments_settings_and_supported_table_layouts() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.toml");
        for contents in [
            "# my settings\n[history]\nignore_leading_space = false\nignore_patterns = ['secret *'] # privacy\n\n[ui] # appearance\nshow_inspector = false\ntheme = 'rose' # keep this comment\nexecute_on_select = true\n",
            "# inline table\nui = { theme = 'rose', show_inspector = false, execute_on_select = true } # keep\n",
            "# dotted settings\nui.theme = 'rose' # keep\nui.show_inspector = false\nui.execute_on_select = true\n",
        ] {
            std::fs::write(&path, contents).unwrap();
            Config::save_theme(&path, Theme::Nord).unwrap();
            let updated = std::fs::read_to_string(&path).unwrap();
            assert_eq!(updated, contents.replace("'rose'", "\"nord\""));
            let config = Config::load(&path).unwrap();
            assert_eq!(config.ui.theme, Theme::Nord);
            assert!(!config.ui.show_inspector);
            assert!(config.ui.execute_on_select);
            Config::save_theme(&path, Theme::Nord).unwrap();
            assert_eq!(std::fs::read_to_string(&path).unwrap(), updated);
        }
    }

    #[test]
    fn saving_theme_adds_missing_ui_keys_and_leaves_invalid_config_intact() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.toml");
        for contents in [
            "# retained\n",
            "[ui]\nshow_inspector = false\n",
            "ui = { show_inspector = false }\n",
        ] {
            std::fs::write(&path, contents).unwrap();
            Config::save_theme(&path, Theme::AccessibleLight).unwrap();
            assert_eq!(
                Config::load(&path).unwrap().ui.theme,
                Theme::AccessibleLight
            );
        }
        for contents in ["[ui", "[ui]\nunknown = true\n", "[ui]\ntheme = 'unknown'\n"] {
            std::fs::write(&path, contents).unwrap();
            assert!(matches!(
                Config::save_theme(&path, Theme::Mint),
                Err(crate::Error::InvalidConfig { .. })
            ));
            assert_eq!(std::fs::read_to_string(&path).unwrap(), contents);
        }
    }

    #[cfg(unix)]
    #[test]
    fn saving_theme_preserves_symlinks_and_permissions_and_reports_read_only_files() {
        use std::os::unix::fs::{PermissionsExt, symlink};

        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("dotfile.toml");
        let path = root.path().join("config.toml");
        std::fs::write(&target, "[ui]\ntheme = 'rose'\n").unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o640)).unwrap();
        symlink("dotfile.toml", &path).unwrap();
        Config::save_theme(&path, Theme::Nord).unwrap();
        assert!(std::fs::symlink_metadata(&path).unwrap().is_symlink());
        assert_eq!(Config::load(&target).unwrap().ui.theme, Theme::Nord);
        assert_eq!(
            std::fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o640
        );
        let contents = std::fs::read_to_string(&target).unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o400)).unwrap();
        let error = Config::save_theme(&path, Theme::Mint).unwrap_err();
        assert!(error.to_string().contains(path.to_str().unwrap()));
        assert_eq!(std::fs::read_to_string(&target).unwrap(), contents);
    }

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

    #[test]
    fn saving_settings_changes_only_the_chosen_key_and_retains_live_edits() {
        use super::{Keymap, UiPreference};
        use crate::scope::SearchScope;

        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.toml");
        let original = "# preferences\n[history]\nignore_patterns = ['secret *']\n[ui]\ntheme = 'nord' # appearance\nexecute_on_select = false # opt in\n";
        std::fs::write(&path, original).unwrap();
        Config::save_preference(&path, UiPreference::DefaultScope(SearchScope::Repository))
            .unwrap();
        Config::save_preference(
            &path,
            UiPreference::Keybinding(KeyAction::Inspector, KeyBinding::Ctrl('o')),
        )
        .unwrap();
        Config::save_preference(&path, UiPreference::ShowInspector(false)).unwrap();
        let contents = std::fs::read_to_string(&path)
            .unwrap()
            .replace("['secret *']", "['private *']");
        std::fs::write(&path, contents).unwrap();
        Config::save_preference(&path, UiPreference::ExecuteOnSelect(true)).unwrap();
        let updated = std::fs::read_to_string(&path).unwrap();
        assert!(updated.starts_with("# preferences\n"));
        assert!(updated.contains("theme = 'nord' # appearance"));
        assert!(updated.contains("execute_on_select = true # opt in"));
        let loaded = Config::load(&path).unwrap();
        assert_eq!(loaded.ui.default_scope, SearchScope::Repository);
        assert_eq!(loaded.ui.keymap, Keymap::Control);
        assert!(!loaded.ui.show_inspector);
        assert!(loaded.ui.execute_on_select);
        assert_eq!(loaded.history.ignore_patterns, ["private *"]);
        assert!(toml::from_str::<Config>("[ui]\ndefault_scope = 'typo'").is_err());
        assert!(toml::from_str::<Config>("[ui]\nkeymap = 'typo'").is_err());
        for (contents, expected) in [
            ("", Keymap::Control),
            ("ui = { keymap = 'control' }", Keymap::Control),
        ] {
            assert_eq!(
                toml::from_str::<Config>(contents).unwrap().ui.keymap,
                expected
            );
        }
    }

    #[test]
    fn binding_saves_preserve_nested_inline_and_dotted_config_and_reset_only_bindings() {
        use super::UiPreference;
        use crate::keybindings::KeyBindings;

        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.toml");
        for original in [
            "[ui]\ntheme = 'nord' # keep\n[ui.keybindings]\ninspector = 'ctrl-o' # shortcut\nstatistics = 'ctrl-b'\n",
            "ui = { theme = 'nord', keybindings = { inspector = 'ctrl-o', statistics = 'ctrl-b' } } # keep\n",
            "ui.theme = 'nord' # keep\nui.keybindings.inspector = 'ctrl-o' # shortcut\nui.keybindings.statistics = 'ctrl-b'\n",
        ] {
            std::fs::write(&path, original).unwrap();
            Config::save_preference(
                &path,
                UiPreference::Keybinding(KeyAction::Inspector, KeyBinding::Ctrl('a')),
            )
            .unwrap();
            let saved = std::fs::read_to_string(&path).unwrap();
            assert_eq!(saved, original.replace("'ctrl-o'", "\"ctrl-a\""));
            let config = Config::load(&path).unwrap();
            assert_eq!(
                config.ui.keybindings.get(KeyAction::Statistics),
                KeyBinding::Ctrl('b')
            );
            Config::save_preference(&path, UiPreference::ResetKeybindings).unwrap();
            let reset = std::fs::read_to_string(&path).unwrap();
            assert!(reset.contains("# keep"));
            assert_eq!(
                Config::load(&path).unwrap().ui.keybindings,
                KeyBindings::default()
            );
            assert_eq!(Config::load(&path).unwrap().ui.theme, Theme::Nord);
        }
        for original in ["", "ui = { theme = 'nord' }\n", "[ui]\ntheme = 'nord'\n"] {
            std::fs::write(&path, original).unwrap();
            Config::save_preference(
                &path,
                UiPreference::Keybinding(KeyAction::Inspector, KeyBinding::Ctrl('a')),
            )
            .unwrap();
            assert_eq!(
                Config::load(&path)
                    .unwrap()
                    .ui
                    .keybindings
                    .get(KeyAction::Inspector),
                KeyBinding::Ctrl('a')
            );
            Config::save_preference(
                &path,
                UiPreference::Keybinding(KeyAction::Scope, KeyBinding::Ctrl('f')),
            )
            .unwrap();
            Config::save_preference(
                &path,
                UiPreference::Keybinding(KeyAction::Scope, KeyBinding::Ctrl('i')),
            )
            .unwrap();
            assert!(std::fs::read_to_string(&path).unwrap().contains("\"tab\""));
            assert_eq!(
                Config::load(&path)
                    .unwrap()
                    .ui
                    .keybindings
                    .get(KeyAction::Scope),
                KeyBinding::Ctrl('i')
            );
        }
        // Another process can reassign a key after the editor opened. Validate
        // the patched map again and leave that external edit untouched.
        let external = "[ui.keybindings]\nthemes = 'ctrl-a'\n";
        std::fs::write(&path, external).unwrap();
        assert!(
            Config::save_preference(
                &path,
                UiPreference::Keybinding(KeyAction::Inspector, KeyBinding::Ctrl('a'))
            )
            .is_err()
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), external);
    }
}
