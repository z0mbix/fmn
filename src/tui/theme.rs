use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use ratatui::style::{Color, Modifier, Style};

use crate::config::Theme;

// kqt palette sources and bundled notices: see THEMES.md.
pub const THEMES: [(Theme, &str); 40] = [
    (Theme::Mint, "Mint"),
    (Theme::Rose, "Rose"),
    (Theme::Iris, "Iris"),
    (Theme::Terminal, "Terminal"),
    (Theme::NoColor, "No colour"),
    (Theme::AccessibleDark, "Accessible Dark"),
    (Theme::AccessibleLight, "Accessible Light"),
    (Theme::Nord, "Nord"),
    (Theme::RosePine, "Rosé Pine"),
    (Theme::RosePineMoon, "Rosé Pine Moon"),
    (Theme::RosePineDawn, "Rosé Pine Dawn"),
    (Theme::CatppuccinMocha, "Catppuccin Mocha"),
    (Theme::CatppuccinMacchiato, "Catppuccin Macchiato"),
    (Theme::CatppuccinFrappe, "Catppuccin Frappe"),
    (Theme::CatppuccinLatte, "Catppuccin Latte"),
    (Theme::Dracula, "Dracula"),
    (Theme::Material, "Material"),
    (Theme::Palenight, "Palenight"),
    (Theme::Darcula, "Darcula"),
    (Theme::Monokai, "Monokai"),
    (Theme::Ubuntu, "Ubuntu"),
    (Theme::Lemonade, "Lemonade"),
    (Theme::PinkLemonade, "Pink Lemonade"),
    (Theme::GruvboxDark, "Gruvbox Dark"),
    (Theme::GruvboxLight, "Gruvbox Light"),
    (Theme::TokyoNight, "Tokyo Night"),
    (Theme::TokyoNightStorm, "Tokyo Night Storm"),
    (Theme::TokyoNightLight, "Tokyo Night Light"),
    (Theme::OneDark, "One Dark"),
    (Theme::SolarizedDark, "Solarized Dark"),
    (Theme::SolarizedLight, "Solarized Light"),
    (Theme::AyuDark, "Ayu Dark"),
    (Theme::AyuMirage, "Ayu Mirage"),
    (Theme::AyuLight, "Ayu Light"),
    (Theme::EverforestDarkHard, "Everforest Dark Hard"),
    (Theme::EverforestDarkMedium, "Everforest Dark Medium"),
    (Theme::EverforestDarkSoft, "Everforest Dark Soft"),
    (Theme::EverforestLightHard, "Everforest Light Hard"),
    (Theme::EverforestLightMedium, "Everforest Light Medium"),
    (Theme::EverforestLightSoft, "Everforest Light Soft"),
];

#[derive(Debug)]
pub(super) struct ThemeMatch {
    pub index: usize,
    pub indices: Vec<u32>,
    score: u32,
}

#[derive(Debug)]
pub(super) struct ThemePicker {
    pub query: String,
    pub matches: Vec<ThemeMatch>,
    matcher: Matcher,
    utf32_buf: Vec<char>,
}

impl Default for ThemePicker {
    fn default() -> Self {
        let mut picker = Self {
            query: String::new(),
            matches: Vec::new(),
            matcher: Matcher::new(Config::DEFAULT),
            utf32_buf: Vec::new(),
        };
        picker.refilter();
        picker
    }
}

impl ThemePicker {
    pub fn refilter(&mut self) {
        // Config-style hyphens and display-name spaces are equivalent here.
        // Theme names are case insensitive, unlike shell command text.
        let query = self.query.replace('-', " ");
        let pattern = Pattern::parse(&query, CaseMatching::Ignore, Normalization::Smart);
        self.matches.clear();
        for (index, (_, name)) in THEMES.iter().enumerate() {
            let mut indices = Vec::new();
            if let Some(score) = pattern.indices(
                Utf32Str::new(name, &mut self.utf32_buf),
                &mut self.matcher,
                &mut indices,
            ) {
                indices.sort_unstable();
                indices.dedup();
                self.matches.push(ThemeMatch {
                    index,
                    indices,
                    score,
                });
            }
        }
        self.matches.sort_by(|left, right| {
            right
                .score
                .cmp(&left.score)
                .then_with(|| left.index.cmp(&right.index))
        });
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub background: Color,
    pub panel: Color,
    pub text: Color,
    pub muted: Color,
    pub border: Color,
    pub accent: Color,
    pub selected: Color,
    pub selection_text: Color,
    pub success: Color,
    pub error: Color,
    pub matched: Color,
}

impl Palette {
    pub fn new(theme: Theme) -> Self {
        use Color::{Indexed, Reset};

        // RGB columns: background, panel, text, muted, border, accent,
        // selection background, success, error. Matches use the accent.
        match theme {
            Theme::Mint => Self {
                matched: rgb(0xa8dce8),
                ..rgb_palette([
                    0x161a23, 0x1b202c, 0xd5dce8, 0x929db3, 0x323b4e, 0x79d8ca, 0x243d43, 0x9acb8a,
                    0xf18b9d,
                ])
            },
            Theme::Rose => Self {
                matched: rgb(0xf1d1e2),
                ..rgb_palette([
                    0x19181d, 0x222027, 0xe4dce2, 0xa799a8, 0x443844, 0xeab2cd, 0x44303e, 0x9acb8a,
                    0xf18b9d,
                ])
            },
            Theme::Iris => Self {
                matched: rgb(0xc6c3ff),
                ..rgb_palette([
                    0x171b28, 0x202638, 0xd4ddf6, 0x939fbe, 0x35405d, 0xb1adfa, 0x313955, 0x9acb8a,
                    0xf18b9d,
                ])
            },
            Theme::Terminal => Self {
                background: Reset,
                panel: Reset,
                text: Reset,
                muted: Indexed(7),
                border: Indexed(8),
                accent: Indexed(6),
                selected: Indexed(4),
                selection_text: Reset,
                success: Indexed(2),
                error: Indexed(1),
                matched: Indexed(6),
            },
            Theme::NoColor => Self {
                background: Reset,
                panel: Reset,
                text: Reset,
                muted: Reset,
                border: Reset,
                accent: Reset,
                selected: Reset,
                selection_text: Reset,
                success: Reset,
                error: Reset,
                matched: Reset,
            },
            Theme::AccessibleDark => Self {
                selection_text: rgb(0x121212),
                ..rgb_palette([
                    0x121212, 0x1c1c1c, 0xf0f0f0, 0xbcbcbc, 0x949494, 0x56b4e9, 0xe4e4e4, 0x56b4e9,
                    0xcc79a7,
                ])
            },
            Theme::AccessibleLight => Self {
                selection_text: rgb(0xffffff),
                ..rgb_palette([
                    0xffffff, 0xf4f4f4, 0x1c1c1c, 0x585858, 0x767676, 0x005f87, 0x303030, 0x005f87,
                    0x882255,
                ])
            },
            Theme::Nord => rgb_palette([
                0x2e3440, 0x3b4252, 0xeceff4, 0xd8dee9, 0x4c566a, 0x88c0d0, 0x434c5e, 0xa3be8c,
                0xbf616a,
            ]),
            Theme::RosePine => rgb_palette([
                0x191724, 0x1f1d2e, 0xe0def4, 0x908caa, 0x524f67, 0xc4a7e7, 0x403d52, 0x9ccfd8,
                0xeb6f92,
            ]),
            Theme::RosePineMoon => rgb_palette([
                0x232136, 0x2a273f, 0xe0def4, 0x908caa, 0x56526e, 0xc4a7e7, 0x44415a, 0x9ccfd8,
                0xeb6f92,
            ]),
            Theme::RosePineDawn => rgb_palette([
                0xfaf4ed, 0xfffaf3, 0x575279, 0x797593, 0x9893a5, 0x907aa9, 0xdfdad9, 0x286983,
                0xb4637a,
            ]),
            Theme::CatppuccinMocha => rgb_palette([
                0x1e1e2e, 0x181825, 0xcdd6f4, 0xa6adc8, 0x585b70, 0xcba6f7, 0x313244, 0xa6e3a1,
                0xf38ba8,
            ]),
            Theme::CatppuccinMacchiato => rgb_palette([
                0x24273a, 0x1e2030, 0xcad3f5, 0xa5adcb, 0x5b6078, 0xc6a0f6, 0x363a4f, 0xa6da95,
                0xed8796,
            ]),
            Theme::CatppuccinFrappe => rgb_palette([
                0x303446, 0x292c3c, 0xc6d0f5, 0xa5adce, 0x626880, 0xca9ee6, 0x414559, 0xa6d189,
                0xe78284,
            ]),
            Theme::CatppuccinLatte => rgb_palette([
                0xeff1f5, 0xe6e9ef, 0x4c4f69, 0x5c5f77, 0x9ca0b0, 0x8839ef, 0xccd0da, 0x287d19,
                0xd20f39,
            ]),
            Theme::Dracula => rgb_palette([
                0x282a36, 0x21222c, 0xf8f8f2, 0xb6b8c8, 0x6272a4, 0xbd93f9, 0x44475a, 0x50fa7b,
                0xff5555,
            ]),
            Theme::Material => rgb_palette([
                0x263238, 0x1a2327, 0xeeffff, 0xa0afb7, 0x546e7a, 0x89ddff, 0x2c3b41, 0xc3e88d,
                0xff627d,
            ]),
            Theme::Palenight => rgb_palette([
                0x292d3e, 0x232635, 0xbfc7d5, 0x929ac9, 0x676e95, 0xc792ea, 0x414764, 0xc3e88d,
                0xff5c78,
            ]),
            Theme::Darcula => rgb_palette([
                0x242424, 0x303030, 0xcccccc, 0xa0a0a0, 0x707070, 0xcf884b, 0x213b6f, 0x8fa876,
                0xff6b68,
            ]),
            Theme::Monokai => rgb_palette([
                0x272822, 0x1e1f1c, 0xf8f8f2, 0x90908a, 0x75715e, 0xa6e22e, 0x414339, 0xa6e22e,
                0xff4d87,
            ]),
            Theme::Ubuntu => rgb_palette([
                0x300a24, 0x2a091f, 0xf1f1e7, 0xa5a59e, 0x841c63, 0xfa8d3e, 0x562329, 0x4cbf99,
                0xf07171,
            ]),
            Theme::Lemonade => rgb_palette([
                0x2a2d34, 0x1e2024, 0xe8e8e8, 0x969696, 0x60646f, 0x4ecdc4, 0x3a3d44, 0x95e1d3,
                0xff6b6b,
            ]),
            Theme::PinkLemonade => rgb_palette([
                0xffbdc8, 0xfcb5c1, 0x535353, 0x5a505d, 0xb16b7d, 0xa40b43, 0xf7cad0, 0x405a2e,
                0xa5152c,
            ]),
            Theme::GruvboxDark => rgb_palette([
                0x282828, 0x32302f, 0xebdbb2, 0xbdae93, 0x7c6f64, 0xfabd2f, 0x504945, 0xb8bb26,
                0xfb4934,
            ]),
            Theme::GruvboxLight => rgb_palette([
                0xfbf1c7, 0xf2e5bc, 0x3c3836, 0x665c54, 0x928374, 0x076678, 0xd5c4a1, 0x79740e,
                0x9d0006,
            ]),
            Theme::TokyoNight => rgb_palette([
                0x1a1b26, 0x16161e, 0xc0caf5, 0xa9b1d6, 0x565f89, 0x7aa2f7, 0x292e42, 0x9ece6a,
                0xf7768e,
            ]),
            Theme::TokyoNightStorm => rgb_palette([
                0x24283b, 0x1f2335, 0xc0caf5, 0xa9b1d6, 0x565f89, 0x7aa2f7, 0x364a82, 0x9ece6a,
                0xf7768e,
            ]),
            Theme::TokyoNightLight => rgb_palette([
                0xd5d6db, 0xe9e9ec, 0x343b58, 0x565f89, 0x9699a3, 0x34548a, 0xb4b5b9, 0x33635c,
                0x8c4351,
            ]),
            Theme::OneDark => rgb_palette([
                0x282c34, 0x21252b, 0xabb2bf, 0x9da5b4, 0x5c6370, 0x61afef, 0x3e4451, 0x98c379,
                0xe06c75,
            ]),
            Theme::SolarizedDark => rgb_palette([
                0x002b36, 0x073642, 0x93a1a1, 0x839496, 0x586e75, 0x2aa198, 0x073642, 0x859900,
                0xdc322f,
            ]),
            Theme::SolarizedLight => rgb_palette([
                0xfdf6e3, 0xeee8d5, 0x586e75, 0x657b83, 0x93a1a1, 0x006c99, 0xeee8d5, 0x586e00,
                0xc12d2a,
            ]),
            Theme::AyuDark => rgb_palette([
                0x10141c, 0x0d1017, 0xbfbdb6, 0x8a91a1, 0x414b5e, 0xe6b450, 0x193155, 0xaad94c,
                0xd95757,
            ]),
            Theme::AyuMirage => rgb_palette([
                0x242936, 0x1f2430, 0xcccac2, 0x949cab, 0x49566e, 0xffcc66, 0x2b4768, 0xd5ff80,
                0xff6666,
            ]),
            Theme::AyuLight => rgb_palette([
                0xfcfcfc, 0xf8f9fa, 0x5c6166, 0x677485, 0xabb4be, 0xa15b00, 0xd7e4f6, 0x587700,
                0xc83f3f,
            ]),
            Theme::EverforestDarkHard => rgb_palette([
                0x272e33, 0x1e2326, 0xd3c6aa, 0x9da9a0, 0x7a8478, 0xa7c080, 0x374145, 0x83c092,
                0xe67e80,
            ]),
            Theme::EverforestDarkMedium => rgb_palette([
                0x2d353b, 0x232a2e, 0xd3c6aa, 0x9da9a0, 0x7a8478, 0xa7c080, 0x3d484d, 0x83c092,
                0xe67e80,
            ]),
            Theme::EverforestDarkSoft => rgb_palette([
                0x333c43, 0x293136, 0xd3c6aa, 0x9da9a0, 0x7a8478, 0xa7c080, 0x434f55, 0x83c092,
                0xed8e8f,
            ]),
            Theme::EverforestLightHard => rgb_palette([
                0xfffbef, 0xf2efdf, 0x536169, 0x5e6b5c, 0x939f91, 0x596900, 0xf2efdf, 0x216b4f,
                0xb73735,
            ]),
            Theme::EverforestLightMedium => rgb_palette([
                0xfdf6e3, 0xefebd4, 0x536169, 0x5e6b5c, 0x939f91, 0x596900, 0xefebd4, 0x216b4f,
                0xb73735,
            ]),
            Theme::EverforestLightSoft => rgb_palette([
                0xf3ead3, 0xe5dfc5, 0x536169, 0x576354, 0x939f91, 0x596900, 0xe5dfc5, 0x216b4f,
                0xa92f2d,
            ]),
        }
    }

    pub fn base(self) -> Style {
        Style::default().fg(self.text).bg(self.background)
    }

    pub fn selection(self) -> Style {
        let style = Style::default().fg(self.selection_text).bg(self.selected);
        if self.selected == Color::Reset {
            style.add_modifier(Modifier::REVERSED)
        } else {
            style
        }
    }

    // The accessible presets use an inverted selection fill. Keep matches,
    // metadata and status symbols readable on that fill as well as plain text.
    pub fn selected_content(self) -> Self {
        if self.selection_text == self.text {
            self
        } else {
            Self {
                text: self.selection_text,
                muted: self.selection_text,
                accent: self.selection_text,
                success: self.selection_text,
                error: self.selection_text,
                matched: self.selection_text,
                ..self
            }
        }
    }
}

const fn rgb(value: u32) -> Color {
    Color::Rgb((value >> 16) as u8, (value >> 8) as u8, value as u8)
}

fn rgb_palette(
    [
        background,
        panel,
        text,
        muted,
        border,
        accent,
        selected,
        success,
        error,
    ]: [u32; 9],
) -> Palette {
    Palette {
        background: rgb(background),
        panel: rgb(panel),
        text: rgb(text),
        muted: rgb(muted),
        border: rgb(border),
        accent: rgb(accent),
        selected: rgb(selected),
        selection_text: rgb(text),
        success: rgb(success),
        error: rgb(error),
        matched: rgb(accent),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn fuzzy_theme_search_handles_abbreviations_config_names_case_and_accents() {
        let mut picker = ThemePicker::default();
        for (query, expected) in [
            ("RP-D", Theme::RosePineDawn),
            ("tokyo-night-storm", Theme::TokyoNightStorm),
            ("rose-pine", Theme::RosePine),
            ("CAT mocha", Theme::CatppuccinMocha),
            ("rose", Theme::Rose),
        ] {
            picker.query = query.into();
            picker.refilter();
            assert_eq!(THEMES[picker.matches[0].index].0, expected, "{query}");
            assert!(!picker.matches[0].indices.is_empty());
        }
        picker.query = "RP-D".into();
        picker.refilter();
        assert_eq!(picker.matches.len(), 1);
        let matched = &picker.matches[0];
        let name: Vec<_> = THEMES[matched.index].1.chars().collect();
        let highlighted: String = matched.indices.iter().map(|&i| name[i as usize]).collect();
        assert_eq!(highlighted, "RPD");
    }

    #[test]
    fn clearing_theme_search_restores_the_full_catalogue_in_original_order() {
        let mut picker = ThemePicker::default();
        for query in ["", "zzzzzzz", "", "   "] {
            picker.query = query.into();
            picker.refilter();
            if query == "zzzzzzz" {
                assert!(picker.matches.is_empty());
            } else {
                assert_eq!(
                    picker
                        .matches
                        .iter()
                        .map(|matched| matched.index)
                        .collect::<Vec<_>>(),
                    (0..THEMES.len()).collect::<Vec<_>>()
                );
                assert!(
                    picker
                        .matches
                        .iter()
                        .all(|matched| matched.indices.is_empty())
                );
            }
        }
    }

    #[test]
    fn every_kqt_config_name_is_selectable_and_originals_are_retained() {
        // Canonical names from kqt, independent of the picker labels.
        let shared = "terminal no-color accessible-dark accessible-light nord
            rose-pine rose-pine-moon rose-pine-dawn catppuccin-mocha
            catppuccin-macchiato catppuccin-frappe catppuccin-latte dracula
            material palenight darcula monokai ubuntu lemonade pink-lemonade
            gruvbox-dark gruvbox-light tokyo-night tokyo-night-storm
            tokyo-night-light one-dark solarized-dark solarized-light ayu-dark
            ayu-mirage ayu-light everforest-dark-hard everforest-dark-medium
            everforest-dark-soft everforest-light-hard everforest-light-medium
            everforest-light-soft";
        assert_eq!(shared.split_whitespace().count(), 37);
        let mut seen = Vec::new();
        for name in shared.split_whitespace().chain(["mint", "rose", "iris"]) {
            let config: Config = toml::from_str(&format!("[ui]\ntheme = '{name}'")).unwrap();
            assert_eq!(
                serde_json::to_value(config.ui.theme).unwrap().as_str(),
                Some(name)
            );
            assert!(THEMES.iter().any(|(theme, _)| *theme == config.ui.theme));
            assert!(!seen.contains(&config.ui.theme), "duplicate theme: {name}");
            seen.push(config.ui.theme);
        }
        assert_eq!(seen.len(), THEMES.len());
        assert_eq!(Config::default().ui.theme, Theme::Mint);
    }

    fn contrast(first: Color, second: Color) -> f64 {
        fn luminance(color: Color) -> f64 {
            let Color::Rgb(red, green, blue) = color else {
                panic!("accessible preset must use explicit RGB colours");
            };
            let linear = |channel: u8| {
                let value = f64::from(channel) / 255.0;
                if value <= 0.04045 {
                    value / 12.92
                } else {
                    ((value + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * linear(red) + 0.7152 * linear(green) + 0.0722 * linear(blue)
        }
        let first = luminance(first);
        let second = luminance(second);
        (first.max(second) + 0.05) / (first.min(second) + 0.05)
    }

    #[test]
    fn accessible_presets_keep_text_and_selection_contrast() {
        for theme in [Theme::AccessibleDark, Theme::AccessibleLight] {
            let palette = Palette::new(theme);
            for background in [palette.background, palette.panel] {
                for text in [
                    palette.text,
                    palette.muted,
                    palette.accent,
                    palette.success,
                    palette.error,
                    palette.matched,
                ] {
                    assert!(contrast(text, background) >= 4.5, "{theme:?}");
                }
                assert!(contrast(palette.border, background) >= 3.0);
                assert!(contrast(palette.selected, background) >= 3.0);
            }
            let selected = palette.selected_content();
            for text in [
                selected.text,
                selected.muted,
                selected.accent,
                selected.success,
                selected.error,
                selected.matched,
            ] {
                assert!(contrast(text, palette.selected) >= 4.5, "{theme:?}");
            }
        }
    }
}
