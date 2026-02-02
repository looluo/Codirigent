//! Color conversion utilities for terminal rendering.
//!
//! This module provides functions to convert alacritty_terminal colors
//! to the Codirigent theme color format.

use crate::theme::{CodirigentTheme, Rgba};
use alacritty_terminal::vte::ansi::{Color as TermColor, NamedColor};

/// Convert alacritty terminal color to theme Rgba.
///
/// Handles three color types:
/// - Named colors (16 ANSI colors)
/// - Indexed colors (256-color palette)
/// - Spec colors (24-bit RGB)
pub fn convert_color(color: TermColor, theme: &CodirigentTheme, _is_foreground: bool) -> Rgba {
    match color {
        TermColor::Named(named) => named_color_to_rgba(named, theme),
        TermColor::Spec(rgb) => Rgba::rgb(rgb.r, rgb.g, rgb.b),
        TermColor::Indexed(idx) => theme.get_indexed_color(idx),
    }
}

/// Convert a named ANSI color to Rgba using the theme.
pub fn named_color_to_rgba(color: NamedColor, theme: &CodirigentTheme) -> Rgba {
    match color {
        NamedColor::Black => theme.ansi.colors[0],
        NamedColor::Red => theme.ansi.colors[1],
        NamedColor::Green => theme.ansi.colors[2],
        NamedColor::Yellow => theme.ansi.colors[3],
        NamedColor::Blue => theme.ansi.colors[4],
        NamedColor::Magenta => theme.ansi.colors[5],
        NamedColor::Cyan => theme.ansi.colors[6],
        NamedColor::White => theme.ansi.colors[7],
        NamedColor::BrightBlack => theme.ansi.colors[8],
        NamedColor::BrightRed => theme.ansi.colors[9],
        NamedColor::BrightGreen => theme.ansi.colors[10],
        NamedColor::BrightYellow => theme.ansi.colors[11],
        NamedColor::BrightBlue => theme.ansi.colors[12],
        NamedColor::BrightMagenta => theme.ansi.colors[13],
        NamedColor::BrightCyan => theme.ansi.colors[14],
        NamedColor::BrightWhite => theme.ansi.colors[15],
        NamedColor::Foreground => theme.terminal_foreground,
        NamedColor::Background => theme.terminal_background,
        NamedColor::Cursor => theme.terminal_cursor,
        NamedColor::DimBlack => dim_color(theme.ansi.colors[0]),
        NamedColor::DimRed => dim_color(theme.ansi.colors[1]),
        NamedColor::DimGreen => dim_color(theme.ansi.colors[2]),
        NamedColor::DimYellow => dim_color(theme.ansi.colors[3]),
        NamedColor::DimBlue => dim_color(theme.ansi.colors[4]),
        NamedColor::DimMagenta => dim_color(theme.ansi.colors[5]),
        NamedColor::DimCyan => dim_color(theme.ansi.colors[6]),
        NamedColor::DimWhite => dim_color(theme.ansi.colors[7]),
        NamedColor::BrightForeground => brighten_color(theme.terminal_foreground),
        NamedColor::DimForeground => dim_color(theme.terminal_foreground),
    }
}

/// Create a dimmed version of a color (reduce brightness by 30%).
pub fn dim_color(color: Rgba) -> Rgba {
    Rgba::new(
        (color.r as f32 * 0.7) as u8,
        (color.g as f32 * 0.7) as u8,
        (color.b as f32 * 0.7) as u8,
        color.a,
    )
}

/// Create a brightened version of a color (increase brightness by 20%).
pub fn brighten_color(color: Rgba) -> Rgba {
    Rgba::new(
        ((color.r as f32 * 1.2).min(255.0)) as u8,
        ((color.g as f32 * 1.2).min(255.0)) as u8,
        ((color.b as f32 * 1.2).min(255.0)) as u8,
        color.a,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dim_color() {
        let original = Rgba::rgb(100, 100, 100);
        let dimmed = dim_color(original);
        assert_eq!(dimmed.r, 70);
        assert_eq!(dimmed.g, 70);
        assert_eq!(dimmed.b, 70);
    }

    #[test]
    fn test_brighten_color() {
        let original = Rgba::rgb(100, 100, 100);
        let brightened = brighten_color(original);
        assert_eq!(brightened.r, 120);
        assert_eq!(brightened.g, 120);
        assert_eq!(brightened.b, 120);
    }

    #[test]
    fn test_brighten_color_clamps() {
        let original = Rgba::rgb(250, 250, 250);
        let brightened = brighten_color(original);
        assert_eq!(brightened.r, 255);
        assert_eq!(brightened.g, 255);
        assert_eq!(brightened.b, 255);
    }

    #[test]
    fn test_convert_named_color() {
        let theme = CodirigentTheme::dark();
        let color = convert_color(TermColor::Named(NamedColor::Red), &theme, true);
        assert_eq!(color, theme.ansi.colors[1]);
    }

    #[test]
    fn test_convert_spec_color() {
        let theme = CodirigentTheme::dark();
        let rgb = alacritty_terminal::vte::ansi::Rgb {
            r: 128,
            g: 64,
            b: 32,
        };
        let color = convert_color(TermColor::Spec(rgb), &theme, true);
        assert_eq!(color.r, 128);
        assert_eq!(color.g, 64);
        assert_eq!(color.b, 32);
    }

    #[test]
    fn test_convert_indexed_color() {
        let theme = CodirigentTheme::dark();
        let color = convert_color(TermColor::Indexed(1), &theme, true);
        assert_eq!(color, theme.ansi.colors[1]);
    }
}
