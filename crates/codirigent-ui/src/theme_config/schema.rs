use serde::{Deserialize, Serialize};

/// Color value in hex format.
///
/// Supported forms for conversion into runtime colors are:
/// - `#RGB`
/// - `#RGBA`
/// - `#RRGGBB`
/// - `#RRGGBBAA`
pub type HexColor = String;

/// Complete serializable theme definition.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Theme {
    /// Theme identifier (unique).
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Whether this is a dark theme.
    pub is_dark: bool,
    /// Theme color palette.
    pub colors: ThemeColors,
    /// Typography settings.
    pub typography: ThemeTypography,
    /// Spacing settings.
    pub spacing: ThemeSpacing,
}

/// Runtime-oriented theme color schema.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ThemeColors {
    /// Background surfaces used across the application shell.
    pub background: ThemeBackgroundColors,
    /// Foreground/text colors.
    pub foreground: ThemeForegroundColors,
    /// Border colors.
    pub border: ThemeBorderColors,
    /// Hover, active, and selection state colors.
    pub interaction: ThemeInteractionColors,
    /// Accent colors and special-purpose highlights.
    pub accent: ThemeAccentColors,
    /// Session status colors.
    pub status: ThemeStatusColors,
    /// Task priority colors.
    pub priority: ThemePriorityColors,
    /// Session group palette.
    pub session_groups: Vec<HexColor>,
    /// Terminal appearance and ANSI palette.
    pub terminal: TerminalColors,
}

/// Background surfaces used across the workspace shell.
#[allow(missing_docs)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ThemeBackgroundColors {
    pub app: HexColor,
    pub panel: HexColor,
    pub header: HexColor,
    pub sidebar: HexColor,
    pub icon_rail: HexColor,
    pub drawer: HexColor,
}

/// Foreground/text colors.
#[allow(missing_docs)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ThemeForegroundColors {
    pub primary: HexColor,
    pub secondary: HexColor,
    pub muted: HexColor,
}

/// Border colors.
#[allow(missing_docs)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ThemeBorderColors {
    pub default: HexColor,
    pub focused: HexColor,
}

/// Interaction colors.
#[allow(missing_docs)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ThemeInteractionColors {
    pub hover: HexColor,
    pub active: HexColor,
    pub selection: HexColor,
}

/// Accent colors and highlight colors.
#[allow(missing_docs)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ThemeAccentColors {
    pub primary: HexColor,
    pub secondary: HexColor,
    pub purple: HexColor,
    pub orange: HexColor,
    pub selected_ring: HexColor,
    pub broadcast: HexColor,
    pub ai_summary_background: HexColor,
    pub ai_summary_text: HexColor,
    pub input_required_background: HexColor,
    pub input_required_accent: HexColor,
}

/// Session status colors.
#[allow(missing_docs)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ThemeStatusColors {
    pub idle: HexColor,
    pub working: HexColor,
    pub needs_attention: HexColor,
    pub response_ready: HexColor,
    pub error: HexColor,
}

/// Task priority colors.
#[allow(missing_docs)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ThemePriorityColors {
    pub high: HexColor,
    pub medium: HexColor,
    pub low: HexColor,
}

/// Terminal surfaces and ANSI palette.
#[allow(missing_docs)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TerminalColors {
    pub background: HexColor,
    pub foreground: HexColor,
    pub cursor: HexColor,
    pub selection_background: HexColor,
    pub selection_foreground: HexColor,
    pub palette: TerminalPalette,
}

/// ANSI 16-color palette.
#[allow(missing_docs)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TerminalPalette {
    pub black: HexColor,
    pub red: HexColor,
    pub green: HexColor,
    pub yellow: HexColor,
    pub blue: HexColor,
    pub magenta: HexColor,
    pub cyan: HexColor,
    pub white: HexColor,
    pub bright_black: HexColor,
    pub bright_red: HexColor,
    pub bright_green: HexColor,
    pub bright_yellow: HexColor,
    pub bright_blue: HexColor,
    pub bright_magenta: HexColor,
    pub bright_cyan: HexColor,
    pub bright_white: HexColor,
}

/// Typography settings for serialized themes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ThemeTypography {
    /// Main UI font family.
    pub ui_font_family: String,
    /// Terminal font family.
    pub terminal_font_family: String,
    /// Base UI font size in pixels.
    pub base_font_size: f32,
    /// Terminal font size in pixels.
    pub terminal_font_size: f32,
    /// Terminal line height multiplier.
    pub line_height: f32,
}

/// Spacing settings for serialized themes.
#[allow(missing_docs)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ThemeSpacing {
    pub xs: f32,
    pub sm: f32,
    pub md: f32,
    pub lg: f32,
    pub xl: f32,
    pub grid_gap: f32,
    pub border_radius: f32,
}
