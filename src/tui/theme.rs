use ratatui::style::{Color, Modifier, Style};

use crate::config::Theme;

pub const THEMES: [(Theme, &str); 5] = [
    (Theme::Mint, "Mint"),
    (Theme::Rose, "Rose"),
    (Theme::Iris, "Iris"),
    (Theme::Terminal, "Terminal"),
    (Theme::NoColor, "No colour"),
];

#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub background: Color,
    pub panel: Color,
    pub text: Color,
    pub muted: Color,
    pub border: Color,
    pub accent: Color,
    pub selected: Color,
    pub success: Color,
    pub error: Color,
    pub matched: Color,
}

impl Palette {
    pub fn new(theme: Theme) -> Self {
        use Color::{Reset, Rgb};

        match theme {
            Theme::Mint => Self {
                background: Rgb(22, 26, 35),
                panel: Rgb(27, 32, 44),
                text: Rgb(213, 220, 232),
                muted: Rgb(146, 157, 179),
                border: Rgb(50, 59, 78),
                accent: Rgb(121, 216, 202),
                selected: Rgb(36, 61, 67),
                success: Rgb(154, 203, 138),
                error: Rgb(241, 139, 157),
                matched: Rgb(168, 220, 232),
            },
            Theme::Rose => Self {
                background: Rgb(25, 24, 29),
                panel: Rgb(34, 32, 39),
                text: Rgb(228, 220, 226),
                muted: Rgb(167, 153, 168),
                border: Rgb(68, 56, 68),
                accent: Rgb(234, 178, 205),
                selected: Rgb(68, 48, 62),
                success: Rgb(154, 203, 138),
                error: Rgb(241, 139, 157),
                matched: Rgb(241, 209, 226),
            },
            Theme::Iris => Self {
                background: Rgb(23, 27, 40),
                panel: Rgb(32, 38, 56),
                text: Rgb(212, 221, 246),
                muted: Rgb(147, 159, 190),
                border: Rgb(53, 64, 93),
                accent: Rgb(177, 173, 250),
                selected: Rgb(49, 57, 85),
                success: Rgb(154, 203, 138),
                error: Rgb(241, 139, 157),
                matched: Rgb(198, 195, 255),
            },
            Theme::Terminal | Theme::NoColor => Self {
                background: Reset,
                panel: Reset,
                text: Reset,
                muted: Reset,
                border: Reset,
                accent: if theme == Theme::Terminal {
                    Color::Cyan
                } else {
                    Reset
                },
                selected: Reset,
                success: if theme == Theme::Terminal {
                    Color::Green
                } else {
                    Reset
                },
                error: if theme == Theme::Terminal {
                    Color::Red
                } else {
                    Reset
                },
                matched: Reset,
            },
        }
    }

    pub fn base(self) -> Style {
        Style::default().fg(self.text).bg(self.background)
    }

    pub fn selection(self) -> Style {
        let style = Style::default().fg(self.text).bg(self.selected);
        if self.selected == Color::Reset {
            style.add_modifier(Modifier::REVERSED)
        } else {
            style
        }
    }
}
