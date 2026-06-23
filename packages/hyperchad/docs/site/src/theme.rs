#![allow(clippy::module_name_repetitions)]

//! Theme values used by the default docs layout.

use hyperchad::color::Color;

/// Visual theme for a docs site.
#[derive(Clone)]
pub struct Theme {
    /// Page background color.
    pub background: Color,
    /// Surface/panel background color.
    pub surface: Color,
    /// Primary body text color.
    pub text_primary: Color,
    /// Secondary body text color.
    pub text_secondary: Color,
    /// Muted text color.
    pub text_muted: Color,
    /// Accent color for links and highlights.
    pub accent: Color,
    /// Border color.
    pub border: Color,
    /// Monospace font stack.
    pub mono_font: &'static str,
}

impl Theme {
    /// Return the default dark documentation theme.
    #[must_use]
    pub fn default_dark() -> Self {
        Self {
            background: Color::from_hex("#0d1117"),
            surface: Color::from_hex("#161b22"),
            text_primary: Color::from_hex("#f0f6fc"),
            text_secondary: Color::from_hex("#c9d1d9"),
            text_muted: Color::from_hex("#8b949e"),
            accent: Color::from_hex("#7ee787"),
            border: Color::from_hex("#21262d"),
            mono_font: "'SF Mono', 'Cascadia Code', 'Fira Code', Menlo, Consolas, monospace",
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::default_dark()
    }
}
