//! Serializable theme configuration.
//!
//! This module provides a file-friendly schema for themes and a conversion path
//! into the runtime [`crate::theme::CodirigentTheme`] model used by the UI and
//! terminal renderer.

mod builtins;
mod conversion;
mod schema;

const DEFAULT_UI_FONT_FAMILY: &str = "Inter";
const DEFAULT_BASE_FONT_SIZE: f32 = 13.0;
const DEFAULT_TERMINAL_FONT_SIZE: f32 = 13.0;
const DEFAULT_TERMINAL_LINE_HEIGHT: f32 = 1.0;
const DEFAULT_EXTRA_SMALL_SPACING: f32 = 2.0;
const DEFAULT_SMALL_SPACING: f32 = 4.0;
const DEFAULT_MEDIUM_SPACING: f32 = 8.0;
const DEFAULT_LARGE_SPACING: f32 = 16.0;
const DEFAULT_EXTRA_LARGE_SPACING: f32 = 24.0;
const DEFAULT_GRID_GAP: f32 = 4.0;
const DEFAULT_BORDER_RADIUS: f32 = 4.0;

pub use conversion::ThemeConversionError;
pub use schema::{
    HexColor, TerminalColors, TerminalPalette, Theme, ThemeAccentColors, ThemeBackgroundColors,
    ThemeBorderColors, ThemeColors, ThemeForegroundColors, ThemeInteractionColors,
    ThemePriorityColors, ThemeSpacing, ThemeStatusColors, ThemeTypography,
};

impl Theme {
    /// Parse a theme definition from JSON.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Serialize a theme definition to pretty JSON.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

impl Default for ThemeTypography {
    fn default() -> Self {
        Self {
            ui_font_family: DEFAULT_UI_FONT_FAMILY.to_string(),
            terminal_font_family: codirigent_core::config::default_terminal_font_family()
                .to_string(),
            base_font_size: DEFAULT_BASE_FONT_SIZE,
            terminal_font_size: DEFAULT_TERMINAL_FONT_SIZE,
            line_height: DEFAULT_TERMINAL_LINE_HEIGHT,
        }
    }
}

impl Default for ThemeSpacing {
    fn default() -> Self {
        Self {
            xs: DEFAULT_EXTRA_SMALL_SPACING,
            sm: DEFAULT_SMALL_SPACING,
            md: DEFAULT_MEDIUM_SPACING,
            lg: DEFAULT_LARGE_SPACING,
            xl: DEFAULT_EXTRA_LARGE_SPACING,
            grid_gap: DEFAULT_GRID_GAP,
            border_radius: DEFAULT_BORDER_RADIUS,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_theme_serializes_and_round_trips() {
        let theme = Theme::dark();
        let json = theme.to_json().expect("serialize");
        let loaded = Theme::from_json(&json).expect("deserialize");
        assert_eq!(theme, loaded);
    }

    #[test]
    fn light_theme_uses_runtime_terminal_font_family_default() {
        let theme = Theme::light();
        assert_eq!(
            theme.typography.terminal_font_family,
            codirigent_core::config::default_terminal_font_family()
        );
    }

    #[test]
    fn invalid_json_fails() {
        assert!(Theme::from_json("not json").is_err());
    }
}
