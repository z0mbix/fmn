use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyAction {
    Inspector,
    Themes,
    Details,
    Statistics,
    Settings,
    Scope,
    Actions,
    Help,
}

impl KeyAction {
    pub const ALL: [Self; 8] = [
        Self::Inspector,
        Self::Themes,
        Self::Details,
        Self::Statistics,
        Self::Settings,
        Self::Scope,
        Self::Actions,
        Self::Help,
    ];

    pub const fn config_name(self) -> &'static str {
        match self {
            Self::Inspector => "inspector",
            Self::Themes => "themes",
            Self::Details => "details",
            Self::Statistics => "statistics",
            Self::Settings => "settings",
            Self::Scope => "scope",
            Self::Actions => "actions",
            Self::Help => "help",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Inspector => "Toggle inspector",
            Self::Themes => "Choose theme",
            Self::Details => "Inspect occurrences",
            Self::Statistics => "History statistics",
            Self::Settings => "Open settings",
            Self::Scope => "Cycle search scope",
            Self::Actions => "Command actions",
            Self::Help => "Show help",
        }
    }

    pub const fn default_binding(self) -> KeyBinding {
        match self {
            Self::Inspector => KeyBinding::Ctrl('o'),
            Self::Themes => KeyBinding::Ctrl('t'),
            Self::Details => KeyBinding::Ctrl('d'),
            Self::Statistics => KeyBinding::Ctrl('g'),
            Self::Settings => KeyBinding::Ctrl('s'),
            Self::Scope => KeyBinding::Ctrl('i'), // Tab in traditional terminals.
            Self::Actions => KeyBinding::Ctrl('x'),
            Self::Help => KeyBinding::Question,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyBinding {
    Ctrl(char),
    Question,
}

impl KeyBinding {
    pub fn from_event(key: KeyEvent) -> Result<Self, String> {
        let binding = match (key.code, key.modifiers) {
            // Traditional terminals send the same byte for Tab and Ctrl-I.
            (KeyCode::Tab, modifiers) if modifiers.is_empty() => Self::Ctrl('i'),
            (KeyCode::Char('?'), modifiers)
                if modifiers.is_empty() || modifiers == KeyModifiers::SHIFT =>
            {
                Self::Question
            }
            (KeyCode::Char(character), KeyModifiers::CONTROL)
                if character.is_ascii_alphabetic() =>
            {
                Self::Ctrl(character.to_ascii_lowercase())
            }
            _ => return Err("Choose Ctrl plus a letter, Tab, or ? for help".into()),
        };
        binding.validate()?;
        Ok(binding)
    }

    fn validate(self) -> Result<(), String> {
        match self {
            Self::Question => Ok(()),
            Self::Ctrl(character) if !character.is_ascii_lowercase() => {
                Err("Use ctrl-a through ctrl-z".into())
            }
            Self::Ctrl('c' | 'h' | 'j' | 'm' | 'n' | 'p' | 'u') => {
                Err("That key is reserved for cancel, editing, or navigation".into())
            }
            Self::Ctrl(_) => Ok(()),
        }
    }

    pub fn matches(self, key: KeyEvent) -> bool {
        Self::from_event(key).ok() == Some(self)
    }

    pub fn short_label(self) -> String {
        match self {
            Self::Ctrl('i') => "Tab".into(),
            Self::Ctrl(character) => format!("^{}", character.to_ascii_uppercase()),
            Self::Question => "?".into(),
        }
    }
}

impl FromStr for KeyBinding {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let value = value.to_ascii_lowercase();
        let binding = if value == "?" {
            Self::Question
        } else if value == "tab" {
            Self::Ctrl('i')
        } else if let Some(letter) = value.strip_prefix("ctrl-") {
            let mut characters = letter.chars();
            let character = characters.next().ok_or("Use ctrl-a through ctrl-z")?;
            if characters.next().is_some() {
                return Err("Use ctrl-a through ctrl-z".into());
            }
            Self::Ctrl(character)
        } else {
            return Err("Use ctrl-a through ctrl-z, tab, or ? for help".into());
        };
        binding.validate()?;
        Ok(binding)
    }
}

impl fmt::Display for KeyBinding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ctrl('i') => write!(formatter, "Tab"),
            Self::Ctrl(character) => write!(formatter, "Ctrl-{}", character.to_ascii_uppercase()),
            Self::Question => write!(formatter, "?"),
        }
    }
}

impl Serialize for KeyBinding {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let value = match self {
            Self::Ctrl('i') => "tab".into(),
            Self::Ctrl(character) => format!("ctrl-{character}"),
            Self::Question => "?".into(),
        };
        serializer.serialize_str(&value)
    }
}

impl<'de> Deserialize<'de> for KeyBinding {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyBindings([KeyBinding; 8]);

impl Default for KeyBindings {
    fn default() -> Self {
        Self(KeyAction::ALL.map(KeyAction::default_binding))
    }
}

impl KeyBindings {
    pub fn get(self, action: KeyAction) -> KeyBinding {
        self.0[action as usize]
    }

    pub fn rebind(&mut self, action: KeyAction, binding: KeyBinding) -> Result<(), String> {
        binding.validate()?;
        if binding == KeyBinding::Question && action != KeyAction::Help {
            return Err("? is available only for help".into());
        }
        if let Some(other) = KeyAction::ALL
            .into_iter()
            .find(|other| *other != action && self.get(*other) == binding)
        {
            return Err(format!(
                "{binding} is already used for {}",
                other.label().to_lowercase()
            ));
        }
        self.0[action as usize] = binding;
        Ok(())
    }

    pub fn action(self, key: KeyEvent) -> Option<KeyAction> {
        KeyAction::ALL
            .into_iter()
            .find(|action| self.get(*action).matches(key))
    }
}

impl<'de> Deserialize<'de> for KeyBindings {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let overrides = BTreeMap::<KeyAction, KeyBinding>::deserialize(deserializer)?;
        let mut bindings = Self::default();
        // Validate the final map, allowing a config file to swap two bindings.
        for (action, binding) in overrides {
            bindings.0[action as usize] = binding;
        }
        for action in KeyAction::ALL {
            bindings
                .rebind(action, bindings.get(action))
                .map_err(serde::de::Error::custom)?;
        }
        Ok(bindings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn ctrl_i_and_tab_are_the_same_binding_and_defaults_have_no_function_keys() {
        let bindings = KeyBindings::default();
        assert_eq!(
            bindings.action(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
            Some(KeyAction::Scope)
        );
        assert_eq!(
            bindings.action(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::CONTROL)),
            Some(KeyAction::Scope)
        );
        assert_eq!(
            bindings.action(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL)),
            Some(KeyAction::Inspector)
        );
        let scope = bindings.get(KeyAction::Scope);
        assert_eq!(scope.to_string(), "Tab");
        assert_eq!(scope.short_label(), "Tab");
        assert_eq!(
            toml_edit::ser::to_string(&BTreeMap::from([("scope", scope)])).unwrap(),
            "scope = \"tab\"\n"
        );
        for number in 1..=12 {
            assert!(bindings.action(KeyCode::F(number).into()).is_none());
        }
        for action in KeyAction::ALL {
            let binding = action.default_binding();
            let encoded =
                toml_edit::ser::to_string(&BTreeMap::from([("binding", binding)])).unwrap();
            let decoded: BTreeMap<String, KeyBinding> = toml::from_str(&encoded).unwrap();
            assert_eq!(decoded["binding"], binding);
        }
        assert_eq!("tab".parse::<KeyBinding>().unwrap(), KeyBinding::Ctrl('i'));
        assert_eq!(
            "CTRL-I".parse::<KeyBinding>().unwrap(),
            KeyBinding::Ctrl('i')
        );
        assert_eq!(
            bindings.action(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::SHIFT)),
            Some(KeyAction::Help)
        );
    }

    #[test]
    fn invalid_reserved_and_conflicting_config_bindings_are_rejected() {
        for value in [
            "ctrl-c", "ctrl-h", "ctrl-j", "ctrl-m", "ctrl-n", "ctrl-p", "ctrl-u", "ctrl-",
            "ctrl-ab", "ctrl-中", "F3", "a", "alt-t",
        ] {
            assert!(value.parse::<KeyBinding>().is_err(), "{value}");
        }
        for config in [
            "[ui.keybindings]\nthemes = 'tab'",
            "[ui.keybindings]\nthemes = 'ctrl-i'",
            "[ui.keybindings]\ninspector = 'ctrl-t'",
            "[ui.keybindings]\nsettings = '?'\nhelp = 'ctrl-a'",
            "[ui.keybindings]\nunknown = 'ctrl-a'",
        ] {
            assert!(toml::from_str::<Config>(config).is_err(), "{config}");
        }
        // Overrides merge with defaults, and a deliberate swap is valid.
        let config: Config =
            toml::from_str("[ui.keybindings]\ninspector = 'ctrl-t'\nthemes = 'ctrl-o'").unwrap();
        assert_eq!(
            config.ui.keybindings.get(KeyAction::Inspector),
            KeyBinding::Ctrl('t')
        );
        assert_eq!(
            config.ui.keybindings.get(KeyAction::Settings),
            KeyBinding::Ctrl('s')
        );
        for legacy in ["function", "control"] {
            let config: Config = toml::from_str(&format!("[ui]\nkeymap = '{legacy}'")).unwrap();
            assert_eq!(config.ui.keybindings, KeyBindings::default());
        }
    }
}
