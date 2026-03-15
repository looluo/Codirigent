use super::schema::{TerminalPalette, Theme};
use crate::theme::{AnsiColors, CodirigentTheme, Hsla, Rgba};
use std::convert::TryFrom;

const OPAQUE_ALPHA: u8 = u8::MAX;
const RGB_SHORT_LENGTH: usize = 3;
const RGBA_SHORT_LENGTH: usize = 4;
const RGB_LONG_LENGTH: usize = 6;
const RGBA_LONG_LENGTH: usize = 8;
const MIN_SESSION_GROUP_COLORS: usize = 6;
const UI_FONT_SIZE_DELTA: f32 = 2.0;
const MIN_SMALL_FONT_SIZE: f32 = 8.0;

/// Error returned when a serialized theme cannot be converted into a runtime
/// [`CodirigentTheme`].
#[allow(missing_docs)]
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ThemeConversionError {
    /// A color string used an unsupported hex format.
    #[error("invalid hex color for {field}: {value}")]
    InvalidHex { field: &'static str, value: String },
    /// The serialized session group palette was shorter than runtime expects.
    #[error("theme requires at least {required} session group colors, found {actual}")]
    NotEnoughSessionGroupColors { required: usize, actual: usize },
}

impl TryFrom<&Theme> for CodirigentTheme {
    type Error = ThemeConversionError;

    fn try_from(theme: &Theme) -> Result<Self, Self::Error> {
        Ok(Self {
            background: parse_hsla(&theme.colors.background.app, "colors.background.app")?,
            panel_background: parse_hsla(
                &theme.colors.background.panel,
                "colors.background.panel",
            )?,
            header_background: parse_hsla(
                &theme.colors.background.header,
                "colors.background.header",
            )?,
            sidebar_background: parse_hsla(
                &theme.colors.background.sidebar,
                "colors.background.sidebar",
            )?,
            border: parse_hsla(&theme.colors.border.default, "colors.border.default")?,
            hover: parse_hsla(&theme.colors.interaction.hover, "colors.interaction.hover")?,
            active: parse_hsla(
                &theme.colors.interaction.active,
                "colors.interaction.active",
            )?,
            selection: parse_hsla(
                &theme.colors.interaction.selection,
                "colors.interaction.selection",
            )?,
            foreground: parse_hsla(
                &theme.colors.foreground.primary,
                "colors.foreground.primary",
            )?,
            text_secondary: parse_hsla(
                &theme.colors.foreground.secondary,
                "colors.foreground.secondary",
            )?,
            muted: parse_hsla(&theme.colors.foreground.muted, "colors.foreground.muted")?,
            primary: parse_hsla(&theme.colors.accent.primary, "colors.accent.primary")?,
            secondary: parse_hsla(&theme.colors.accent.secondary, "colors.accent.secondary")?,
            purple: parse_hsla(&theme.colors.accent.purple, "colors.accent.purple")?,
            orange: parse_hsla(&theme.colors.accent.orange, "colors.accent.orange")?,
            icon_rail_background: parse_hsla(
                &theme.colors.background.icon_rail,
                "colors.background.icon_rail",
            )?,
            drawer_background: parse_hsla(
                &theme.colors.background.drawer,
                "colors.background.drawer",
            )?,
            selected_ring: parse_hsla(
                &theme.colors.accent.selected_ring,
                "colors.accent.selected_ring",
            )?,
            broadcast_accent: parse_hsla(
                &theme.colors.accent.broadcast,
                "colors.accent.broadcast",
            )?,
            ai_summary_background: parse_hsla(
                &theme.colors.accent.ai_summary_background,
                "colors.accent.ai_summary_background",
            )?,
            ai_summary_text: parse_hsla(
                &theme.colors.accent.ai_summary_text,
                "colors.accent.ai_summary_text",
            )?,
            input_required_background: parse_hsla(
                &theme.colors.accent.input_required_background,
                "colors.accent.input_required_background",
            )?,
            input_required_accent: parse_hsla(
                &theme.colors.accent.input_required_accent,
                "colors.accent.input_required_accent",
            )?,
            session_idle: parse_hsla(&theme.colors.status.idle, "colors.status.idle")?,
            session_working: parse_hsla(&theme.colors.status.working, "colors.status.working")?,
            session_needs_attention: parse_hsla(
                &theme.colors.status.needs_attention,
                "colors.status.needs_attention",
            )?,
            session_response_ready: parse_hsla(
                &theme.colors.status.response_ready,
                "colors.status.response_ready",
            )?,
            session_error: parse_hsla(&theme.colors.status.error, "colors.status.error")?,
            priority_high: parse_hsla(&theme.colors.priority.high, "colors.priority.high")?,
            priority_medium: parse_hsla(&theme.colors.priority.medium, "colors.priority.medium")?,
            priority_low: parse_hsla(&theme.colors.priority.low, "colors.priority.low")?,
            session_colors: parse_session_colors(theme)?,
            cursor: parse_rgba(&theme.colors.terminal.cursor, "colors.terminal.cursor")?.to_hsla(),
            ansi: parse_ansi_colors(&theme.colors.terminal.palette)?,
            terminal_background: parse_rgba(
                &theme.colors.terminal.background,
                "colors.terminal.background",
            )?,
            terminal_foreground: parse_rgba(
                &theme.colors.terminal.foreground,
                "colors.terminal.foreground",
            )?,
            terminal_cursor: parse_rgba(&theme.colors.terminal.cursor, "colors.terminal.cursor")?,
            terminal_selection_bg: parse_rgba(
                &theme.colors.terminal.selection_background,
                "colors.terminal.selection_background",
            )?,
            terminal_selection_fg: parse_rgba(
                &theme.colors.terminal.selection_foreground,
                "colors.terminal.selection_foreground",
            )?,
            grid_gap: theme.spacing.grid_gap,
            font_size_base: theme.typography.base_font_size,
            font_size_small: (theme.typography.base_font_size - UI_FONT_SIZE_DELTA)
                .max(MIN_SMALL_FONT_SIZE),
            font_size_large: theme.typography.base_font_size + UI_FONT_SIZE_DELTA,
            terminal_font_size: theme.typography.terminal_font_size,
            terminal_line_height: theme.typography.line_height,
            terminal_font_family: theme.typography.terminal_font_family.clone(),
            spacing_base: theme.spacing.md,
            spacing_small: theme.spacing.sm,
            spacing_large: theme.spacing.lg,
        })
    }
}

fn parse_session_colors(theme: &Theme) -> Result<[Hsla; 6], ThemeConversionError> {
    let actual = theme.colors.session_groups.len();
    if actual < MIN_SESSION_GROUP_COLORS {
        return Err(ThemeConversionError::NotEnoughSessionGroupColors {
            required: MIN_SESSION_GROUP_COLORS,
            actual,
        });
    }

    let mut colors = [Hsla::new(0.0, 0.0, 0.0, 1.0); MIN_SESSION_GROUP_COLORS];
    for (target, source) in colors.iter_mut().zip(
        theme
            .colors
            .session_groups
            .iter()
            .take(MIN_SESSION_GROUP_COLORS),
    ) {
        *target = parse_hsla(source, "colors.session_groups")?;
    }
    Ok(colors)
}

fn parse_ansi_colors(palette: &TerminalPalette) -> Result<AnsiColors, ThemeConversionError> {
    Ok(AnsiColors {
        colors: [
            parse_rgba(&palette.black, "colors.terminal.palette.black")?,
            parse_rgba(&palette.red, "colors.terminal.palette.red")?,
            parse_rgba(&palette.green, "colors.terminal.palette.green")?,
            parse_rgba(&palette.yellow, "colors.terminal.palette.yellow")?,
            parse_rgba(&palette.blue, "colors.terminal.palette.blue")?,
            parse_rgba(&palette.magenta, "colors.terminal.palette.magenta")?,
            parse_rgba(&palette.cyan, "colors.terminal.palette.cyan")?,
            parse_rgba(&palette.white, "colors.terminal.palette.white")?,
            parse_rgba(
                &palette.bright_black,
                "colors.terminal.palette.bright_black",
            )?,
            parse_rgba(&palette.bright_red, "colors.terminal.palette.bright_red")?,
            parse_rgba(
                &palette.bright_green,
                "colors.terminal.palette.bright_green",
            )?,
            parse_rgba(
                &palette.bright_yellow,
                "colors.terminal.palette.bright_yellow",
            )?,
            parse_rgba(&palette.bright_blue, "colors.terminal.palette.bright_blue")?,
            parse_rgba(
                &palette.bright_magenta,
                "colors.terminal.palette.bright_magenta",
            )?,
            parse_rgba(&palette.bright_cyan, "colors.terminal.palette.bright_cyan")?,
            parse_rgba(
                &palette.bright_white,
                "colors.terminal.palette.bright_white",
            )?,
        ],
    })
}

fn parse_hsla(value: &str, field: &'static str) -> Result<Hsla, ThemeConversionError> {
    Ok(parse_rgba(value, field)?.to_hsla())
}

fn parse_rgba(value: &str, field: &'static str) -> Result<Rgba, ThemeConversionError> {
    let hex = value.trim_start_matches('#');
    let (r, g, b, a) = match hex.len() {
        RGB_SHORT_LENGTH => (
            expand_nibble(&hex[0..1], value, field)?,
            expand_nibble(&hex[1..2], value, field)?,
            expand_nibble(&hex[2..3], value, field)?,
            OPAQUE_ALPHA,
        ),
        RGBA_SHORT_LENGTH => (
            expand_nibble(&hex[0..1], value, field)?,
            expand_nibble(&hex[1..2], value, field)?,
            expand_nibble(&hex[2..3], value, field)?,
            expand_nibble(&hex[3..4], value, field)?,
        ),
        RGB_LONG_LENGTH => (
            parse_byte(&hex[0..2], value, field)?,
            parse_byte(&hex[2..4], value, field)?,
            parse_byte(&hex[4..6], value, field)?,
            OPAQUE_ALPHA,
        ),
        RGBA_LONG_LENGTH => (
            parse_byte(&hex[0..2], value, field)?,
            parse_byte(&hex[2..4], value, field)?,
            parse_byte(&hex[4..6], value, field)?,
            parse_byte(&hex[6..8], value, field)?,
        ),
        _ => return Err(invalid_hex(field, value)),
    };

    Ok(Rgba::new(r, g, b, a))
}

fn expand_nibble(
    nibble: &str,
    original: &str,
    field: &'static str,
) -> Result<u8, ThemeConversionError> {
    parse_byte(&format!("{nibble}{nibble}"), original, field)
}

fn parse_byte(byte: &str, original: &str, field: &'static str) -> Result<u8, ThemeConversionError> {
    u8::from_str_radix(byte, 16).map_err(|_| invalid_hex(field, original))
}

fn invalid_hex(field: &'static str, value: &str) -> ThemeConversionError {
    ThemeConversionError::InvalidHex {
        field,
        value: value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme_config::TerminalColors;

    #[test]
    fn convert_builtin_dark_theme_into_runtime_theme() {
        let config = Theme::dark();
        let runtime = CodirigentTheme::try_from(&config).expect("convert dark theme");

        assert_eq!(runtime.background, CodirigentTheme::dark().background);
        assert_eq!(
            runtime.terminal_selection_bg,
            CodirigentTheme::dark().terminal_selection_bg
        );
    }

    #[test]
    fn parse_rgba_supports_alpha_hex() {
        let rgba = parse_rgba("#11223344", "test").expect("rgba");
        assert_eq!(rgba, Rgba::new(0x11, 0x22, 0x33, 0x44));
    }

    #[test]
    fn reject_short_session_group_palette() {
        let mut theme = Theme::dark();
        theme.colors.session_groups.truncate(2);

        let err = CodirigentTheme::try_from(&theme).expect_err("must fail");
        assert_eq!(
            err,
            ThemeConversionError::NotEnoughSessionGroupColors {
                required: MIN_SESSION_GROUP_COLORS,
                actual: 2,
            }
        );
    }

    #[test]
    fn reject_invalid_hex_values() {
        let mut theme = Theme::dark();
        theme.colors.terminal.cursor = "#12zz00".to_string();

        let err = CodirigentTheme::try_from(&theme).expect_err("must fail");
        assert_eq!(
            err,
            ThemeConversionError::InvalidHex {
                field: "colors.terminal.cursor",
                value: "#12zz00".to_string(),
            }
        );
    }

    #[test]
    fn parse_ansi_palette_uses_terminal_palette_entries() {
        let theme = Theme::dark();
        let ansi = parse_ansi_colors(&theme.colors.terminal.palette).expect("ansi");
        assert_eq!(ansi.colors[1], Rgba::rgb(204, 0, 0));
    }

    #[test]
    fn terminal_colors_include_full_surface_definition() {
        let colors = TerminalColors {
            background: "#000000".to_string(),
            foreground: "#ffffff".to_string(),
            cursor: "#abcdef".to_string(),
            selection_background: "#11223344".to_string(),
            selection_foreground: "#eeeeee".to_string(),
            palette: Theme::dark().colors.terminal.palette,
        };
        let cursor = parse_rgba(&colors.cursor, "cursor").expect("cursor");
        assert_eq!(cursor, Rgba::rgb(0xab, 0xcd, 0xef));
    }
}
