//! Theme system for Dirigent.
//!
//! Provides color themes for the Dirigent UI, including dark and light modes.
//! Colors are inspired by Catppuccin for the dark theme.
//!
//! # Color Types
//!
//! This module provides two color representations:
//! - [`Hsla`] - Hue-Saturation-Lightness-Alpha for UI elements (GPUI compatible)
//! - [`Rgba`] - Red-Green-Blue-Alpha for terminal colors (alacritty_terminal compatible)

use dirigent_core::SessionStatus;

/// RGBA color representation.
///
/// Components are stored as u8 values (0-255). Used for terminal cell colors
/// which need to interface with alacritty_terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgba {
    /// Red component (0-255).
    pub r: u8,
    /// Green component (0-255).
    pub g: u8,
    /// Blue component (0-255).
    pub b: u8,
    /// Alpha component (0-255, 255 = fully opaque).
    pub a: u8,
}

impl Rgba {
    /// Create a new RGBA color.
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Create an opaque RGB color (alpha = 255).
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self::new(r, g, b, 255)
    }

    /// Convert to HSLA.
    pub fn to_hsla(&self) -> Hsla {
        let r = self.r as f32 / 255.0;
        let g = self.g as f32 / 255.0;
        let b = self.b as f32 / 255.0;
        let a = self.a as f32 / 255.0;

        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let l = (max + min) / 2.0;

        if (max - min).abs() < f32::EPSILON {
            return Hsla::new(0.0, 0.0, l, a);
        }

        let d = max - min;
        let s = if l > 0.5 {
            d / (2.0 - max - min)
        } else {
            d / (max + min)
        };

        let h = if (max - r).abs() < f32::EPSILON {
            ((g - b) / d + if g < b { 6.0 } else { 0.0 }) / 6.0
        } else if (max - g).abs() < f32::EPSILON {
            ((b - r) / d + 2.0) / 6.0
        } else {
            ((r - g) / d + 4.0) / 6.0
        };

        Hsla::new(h, s, l, a)
    }
}

impl Default for Rgba {
    fn default() -> Self {
        Self::rgb(0, 0, 0)
    }
}

/// Standard ANSI terminal colors (16 colors).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnsiColors {
    /// Colors 0-15 in order.
    pub colors: [Rgba; 16],
}

impl AnsiColors {
    /// Get color by index (0-15).
    pub fn get(&self, index: u8) -> Option<Rgba> {
        if index < 16 {
            Some(self.colors[index as usize])
        } else {
            None
        }
    }
}

impl Default for AnsiColors {
    fn default() -> Self {
        Self {
            colors: [
                Rgba::rgb(0, 0, 0),       // Black
                Rgba::rgb(204, 0, 0),     // Red
                Rgba::rgb(0, 204, 0),     // Green
                Rgba::rgb(204, 204, 0),   // Yellow
                Rgba::rgb(0, 0, 204),     // Blue
                Rgba::rgb(204, 0, 204),   // Magenta
                Rgba::rgb(0, 204, 204),   // Cyan
                Rgba::rgb(204, 204, 204), // White
                Rgba::rgb(128, 128, 128), // Bright Black
                Rgba::rgb(255, 0, 0),     // Bright Red
                Rgba::rgb(0, 255, 0),     // Bright Green
                Rgba::rgb(255, 255, 0),   // Bright Yellow
                Rgba::rgb(0, 0, 255),     // Bright Blue
                Rgba::rgb(255, 0, 255),   // Bright Magenta
                Rgba::rgb(0, 255, 255),   // Bright Cyan
                Rgba::rgb(255, 255, 255), // Bright White
            ],
        }
    }
}

/// HSLA color representation.
///
/// Hue-Saturation-Lightness-Alpha color model, compatible with GPUI's
/// color system when the `gpui` feature is enabled.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hsla {
    /// Hue (0.0 to 1.0).
    pub h: f32,
    /// Saturation (0.0 to 1.0).
    pub s: f32,
    /// Lightness (0.0 to 1.0).
    pub l: f32,
    /// Alpha (0.0 to 1.0).
    pub a: f32,
}

impl Hsla {
    /// Create a new HSLA color.
    pub const fn new(h: f32, s: f32, l: f32, a: f32) -> Self {
        Self { h, s, l, a }
    }
}

/// Helper function to create an HSLA color.
///
/// # Arguments
///
/// * `h` - Hue (0.0 to 1.0)
/// * `s` - Saturation (0.0 to 1.0)
/// * `l` - Lightness (0.0 to 1.0)
/// * `a` - Alpha (0.0 to 1.0)
pub const fn hsla(h: f32, s: f32, l: f32, a: f32) -> Hsla {
    Hsla::new(h, s, l, a)
}

/// Dirigent color theme.
///
/// Contains all colors used throughout the Dirigent UI. Each color is defined
/// as an HSLA value for flexibility in rendering.
#[derive(Clone, Debug, PartialEq)]
pub struct DirigentTheme {
    /// Main background color.
    pub background: Hsla,
    /// Primary text color.
    pub foreground: Hsla,
    /// Border color for panels and dividers.
    pub border: Hsla,
    /// Selection highlight color.
    pub selection: Hsla,
    /// Cursor color in terminals.
    pub cursor: Hsla,
    /// Color for idle sessions.
    pub session_idle: Hsla,
    /// Color for working/active sessions.
    pub session_working: Hsla,
    /// Color for sessions waiting for input.
    pub session_waiting: Hsla,
    /// Color for completed sessions.
    pub session_done: Hsla,
    /// Color for sessions with errors.
    pub session_error: Hsla,
    /// Sidebar background color.
    pub sidebar_background: Hsla,
    /// Panel/pane background color.
    pub panel_background: Hsla,
    /// Active/focused element highlight.
    pub active: Hsla,
    /// Muted/secondary text color.
    pub muted: Hsla,
    /// Terminal ANSI colors.
    pub ansi: AnsiColors,
    /// Terminal background as RGBA.
    pub terminal_background: Rgba,
    /// Terminal foreground as RGBA.
    pub terminal_foreground: Rgba,
    /// Terminal cursor as RGBA.
    pub terminal_cursor: Rgba,
    /// Terminal selection background as RGBA.
    pub terminal_selection_bg: Rgba,
    /// Terminal selection foreground as RGBA.
    pub terminal_selection_fg: Rgba,
    /// Gap between grid cells in pixels.
    pub grid_gap: f32,
}

impl DirigentTheme {
    /// Create the dark theme.
    ///
    /// Uses colors inspired by Catppuccin Mocha palette.
    pub fn dark() -> Self {
        Self {
            // Base colors (Catppuccin Mocha)
            background: hsla(240.0 / 360.0, 0.21, 0.12, 1.0), // #1e1e2e
            foreground: hsla(226.0 / 360.0, 0.64, 0.88, 1.0), // #cdd6f4
            border: hsla(237.0 / 360.0, 0.16, 0.23, 1.0),     // #313244
            selection: hsla(267.0 / 360.0, 0.84, 0.81, 0.3),  // #cba6f7 @ 30%
            cursor: hsla(267.0 / 360.0, 0.84, 0.81, 1.0),     // #cba6f7

            // Session status colors
            session_idle: hsla(231.0 / 360.0, 0.11, 0.47, 1.0), // #6c7086
            session_working: hsla(217.0 / 360.0, 0.92, 0.76, 1.0), // #89b4fa (blue)
            session_waiting: hsla(39.0 / 360.0, 0.67, 0.69, 1.0), // #f9e2af (yellow)
            session_done: hsla(115.0 / 360.0, 0.54, 0.76, 1.0), // #a6e3a1 (green)
            session_error: hsla(343.0 / 360.0, 0.81, 0.75, 1.0), // #f38ba8 (red)

            // Panel colors
            sidebar_background: hsla(240.0 / 360.0, 0.21, 0.10, 1.0),
            panel_background: hsla(240.0 / 360.0, 0.21, 0.14, 1.0),

            // UI states
            active: hsla(267.0 / 360.0, 0.84, 0.81, 1.0), // #cba6f7
            muted: hsla(231.0 / 360.0, 0.11, 0.47, 1.0),  // #6c7086

            // Terminal colors
            ansi: AnsiColors::default(),
            terminal_background: Rgba::rgb(30, 30, 46), // #1e1e2e
            terminal_foreground: Rgba::rgb(205, 214, 244), // #cdd6f4
            terminal_cursor: Rgba::rgb(203, 166, 247),  // #cba6f7
            terminal_selection_bg: Rgba::new(203, 166, 247, 77), // #cba6f7 @ 30%
            terminal_selection_fg: Rgba::rgb(205, 214, 244), // #cdd6f4

            // Layout
            grid_gap: 4.0,
        }
    }

    /// Create the light theme.
    ///
    /// Uses colors inspired by Catppuccin Latte palette.
    pub fn light() -> Self {
        Self {
            // Base colors (Catppuccin Latte)
            background: hsla(220.0 / 360.0, 0.23, 0.95, 1.0), // #eff1f5
            foreground: hsla(234.0 / 360.0, 0.16, 0.35, 1.0), // #4c4f69
            border: hsla(220.0 / 360.0, 0.13, 0.85, 1.0),     // #ccd0da
            selection: hsla(267.0 / 360.0, 0.84, 0.70, 0.3),  // Mauve @ 30%
            cursor: hsla(267.0 / 360.0, 0.84, 0.50, 1.0),     // Mauve

            // Session status colors (darker for visibility on light bg)
            session_idle: hsla(231.0 / 360.0, 0.10, 0.55, 1.0), // Gray
            session_working: hsla(217.0 / 360.0, 0.92, 0.45, 1.0), // Blue
            session_waiting: hsla(39.0 / 360.0, 0.80, 0.45, 1.0), // Yellow
            session_done: hsla(115.0 / 360.0, 0.54, 0.40, 1.0), // Green
            session_error: hsla(343.0 / 360.0, 0.81, 0.50, 1.0), // Red

            // Panel colors
            sidebar_background: hsla(220.0 / 360.0, 0.23, 0.92, 1.0),
            panel_background: hsla(220.0 / 360.0, 0.23, 0.97, 1.0),

            // UI states
            active: hsla(267.0 / 360.0, 0.84, 0.50, 1.0),
            muted: hsla(231.0 / 360.0, 0.10, 0.55, 1.0),

            // Terminal colors
            ansi: AnsiColors::default(),
            terminal_background: Rgba::rgb(239, 241, 245), // #eff1f5
            terminal_foreground: Rgba::rgb(76, 79, 105),   // #4c4f69
            terminal_cursor: Rgba::rgb(136, 57, 239),      // Mauve
            terminal_selection_bg: Rgba::new(136, 57, 239, 77), // Mauve @ 30%
            terminal_selection_fg: Rgba::rgb(76, 79, 105), // #4c4f69

            // Layout
            grid_gap: 4.0,
        }
    }

    /// Get the color for a given session status.
    ///
    /// Returns the appropriate status indicator color based on the session's
    /// current state.
    ///
    /// # Arguments
    ///
    /// * `status` - The session status to get the color for
    ///
    /// # Returns
    ///
    /// The HSLA color corresponding to the status
    pub fn status_color(&self, status: SessionStatus) -> Hsla {
        match status {
            SessionStatus::Idle => self.session_idle,
            SessionStatus::Working => self.session_working,
            SessionStatus::WaitingForInput => self.session_waiting,
            SessionStatus::Done => self.session_done,
            SessionStatus::Error => self.session_error,
        }
    }

    /// Get the display name for a session status.
    ///
    /// Returns a human-readable string for the status.
    ///
    /// # Arguments
    ///
    /// * `status` - The session status
    ///
    /// # Returns
    ///
    /// A string slice with the status name
    pub fn status_name(status: SessionStatus) -> &'static str {
        match status {
            SessionStatus::Idle => "Idle",
            SessionStatus::Working => "Working",
            SessionStatus::WaitingForInput => "Waiting",
            SessionStatus::Done => "Done",
            SessionStatus::Error => "Error",
        }
    }
}

impl Default for DirigentTheme {
    fn default() -> Self {
        Self::dark()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dark_theme_creation() {
        let theme = DirigentTheme::dark();
        // Dark theme should have low lightness background
        assert!(theme.background.l < 0.2);
        // Dark theme should have high lightness foreground
        assert!(theme.foreground.l > 0.5);
    }

    #[test]
    fn test_light_theme_creation() {
        let theme = DirigentTheme::light();
        // Light theme should have high lightness background
        assert!(theme.background.l > 0.8);
        // Light theme should have low lightness foreground
        assert!(theme.foreground.l < 0.5);
    }

    #[test]
    fn test_default_is_dark() {
        let default = DirigentTheme::default();
        let dark = DirigentTheme::dark();
        assert_eq!(default.background, dark.background);
    }

    #[test]
    fn test_status_colors_all_variants() {
        let theme = DirigentTheme::dark();

        // Test all status variants return valid colors
        let statuses = [
            SessionStatus::Idle,
            SessionStatus::Working,
            SessionStatus::WaitingForInput,
            SessionStatus::Done,
            SessionStatus::Error,
        ];

        for status in statuses {
            let color = theme.status_color(status);
            // All colors should be fully opaque
            assert!(
                color.a >= 0.9,
                "Status {:?} should have alpha >= 0.9",
                status
            );
        }
    }

    #[test]
    fn test_status_colors_distinct() {
        let theme = DirigentTheme::dark();

        // Status colors should be distinct from each other
        let working = theme.status_color(SessionStatus::Working);
        let waiting = theme.status_color(SessionStatus::WaitingForInput);
        let done = theme.status_color(SessionStatus::Done);
        let error = theme.status_color(SessionStatus::Error);

        assert_ne!(working, waiting, "Working and Waiting should be different");
        assert_ne!(waiting, done, "Waiting and Done should be different");
        assert_ne!(done, error, "Done and Error should be different");
    }

    #[test]
    fn test_status_names() {
        assert_eq!(DirigentTheme::status_name(SessionStatus::Idle), "Idle");
        assert_eq!(
            DirigentTheme::status_name(SessionStatus::Working),
            "Working"
        );
        assert_eq!(
            DirigentTheme::status_name(SessionStatus::WaitingForInput),
            "Waiting"
        );
        assert_eq!(DirigentTheme::status_name(SessionStatus::Done), "Done");
        assert_eq!(DirigentTheme::status_name(SessionStatus::Error), "Error");
    }

    #[test]
    fn test_theme_clone() {
        let theme = DirigentTheme::dark();
        let cloned = theme.clone();
        assert_eq!(theme, cloned);
    }

    #[test]
    fn test_theme_debug() {
        let theme = DirigentTheme::dark();
        let debug_str = format!("{:?}", theme);
        assert!(debug_str.contains("DirigentTheme"));
    }

    #[test]
    fn test_hsla_new() {
        let color = Hsla::new(0.5, 0.5, 0.5, 1.0);
        assert_eq!(color.h, 0.5);
        assert_eq!(color.s, 0.5);
        assert_eq!(color.l, 0.5);
        assert_eq!(color.a, 1.0);
    }

    #[test]
    fn test_hsla_helper() {
        let color = hsla(0.25, 0.75, 0.5, 0.8);
        assert_eq!(color.h, 0.25);
        assert_eq!(color.s, 0.75);
        assert_eq!(color.l, 0.5);
        assert_eq!(color.a, 0.8);
    }

    #[test]
    fn test_grid_gap() {
        let theme = DirigentTheme::dark();
        assert_eq!(theme.grid_gap, 4.0);
    }

    #[test]
    fn test_rgba_new() {
        let color = Rgba::new(255, 128, 64, 255);
        assert_eq!(color.r, 255);
        assert_eq!(color.g, 128);
        assert_eq!(color.b, 64);
        assert_eq!(color.a, 255);
    }

    #[test]
    fn test_rgba_rgb() {
        let color = Rgba::rgb(255, 128, 64);
        assert_eq!(color.a, 255);
    }

    #[test]
    fn test_rgba_default() {
        let color = Rgba::default();
        assert_eq!(color.r, 0);
        assert_eq!(color.g, 0);
        assert_eq!(color.b, 0);
        assert_eq!(color.a, 255);
    }

    #[test]
    fn test_rgba_to_hsla() {
        let color = Rgba::rgb(255, 0, 0); // Pure red
        let hsla = color.to_hsla();
        assert!((hsla.h - 0.0).abs() < 0.01); // Hue should be 0 (red)
        assert!((hsla.s - 1.0).abs() < 0.01); // Full saturation
        assert!((hsla.l - 0.5).abs() < 0.01); // Middle lightness
    }

    #[test]
    fn test_ansi_colors_default() {
        let ansi = AnsiColors::default();
        assert_eq!(ansi.colors.len(), 16);
    }

    #[test]
    fn test_ansi_colors_get() {
        let ansi = AnsiColors::default();
        assert!(ansi.get(0).is_some());
        assert!(ansi.get(15).is_some());
        assert!(ansi.get(16).is_none());
    }

    #[test]
    fn test_terminal_colors() {
        let theme = DirigentTheme::dark();
        assert_eq!(theme.terminal_background.r, 30);
        assert_eq!(theme.terminal_background.g, 30);
        assert_eq!(theme.terminal_background.b, 46);
    }
}
