use std::sync::OnceLock;

use ratatui::style::Color;

/// Small semantic theme seam expected by the copied Grok view modules.
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub accent_user: Color,
    pub bg_base: Color,
    pub bg_highlight: Color,
    pub bg_hover: Color,
    pub bg_light: Color,
    pub bg_visual: Color,
    pub gray: Color,
    pub gray_bright: Color,
    pub gray_dim: Color,
    pub fuzzy_accent: Color,
    pub text_secondary: Color,
    pub text_primary: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            accent_user: Color::Rgb(120, 180, 255),
            bg_base: Color::Rgb(17, 19, 24),
            bg_highlight: Color::Rgb(30, 34, 43),
            bg_hover: Color::Rgb(38, 43, 54),
            bg_light: Color::Rgb(27, 31, 40),
            bg_visual: Color::Rgb(45, 52, 68),
            gray: Color::Rgb(160, 168, 182),
            gray_bright: Color::Rgb(218, 224, 235),
            gray_dim: Color::Rgb(93, 101, 116),
            fuzzy_accent: Color::Rgb(120, 190, 255),
            text_secondary: Color::Rgb(223, 229, 240),
            text_primary: Color::Rgb(235, 239, 247),
        }
    }
}

impl Theme {
    pub fn current() -> &'static Self {
        static THEME: OnceLock<Theme> = OnceLock::new();
        THEME.get_or_init(Self::default)
    }
}
