//! Theme system for Codirigent.
//!
//! Provides color themes for the Codirigent UI, including dark and light modes.
//! The dark theme uses a custom color palette designed for the Dirigent dashboard.
//!
//! # Color Types
//!
//! This module provides two color representations:
//! - [`Hsla`] - Hue-Saturation-Lightness-Alpha for UI elements (GPUI compatible)
//! - [`Rgba`] - Red-Green-Blue-Alpha for terminal colors (alacritty_terminal compatible)

use codirigent_core::SessionStatus;

#[cfg(feature = "gpui-full")]
use gpui::Hsla as GpuiHsla;

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

/// Convert theme Hsla to GPUI Hsla.
#[cfg(feature = "gpui-full")]
impl From<Hsla> for GpuiHsla {
    fn from(color: Hsla) -> Self {
        GpuiHsla {
            h: color.h,
            s: color.s,
            l: color.l,
            a: color.a,
        }
    }
}

/// Convert GPUI Hsla to theme Hsla.
#[cfg(feature = "gpui-full")]
impl From<GpuiHsla> for Hsla {
    fn from(color: GpuiHsla) -> Self {
        Hsla {
            h: color.h,
            s: color.s,
            l: color.l,
            a: color.a,
        }
    }
}

/// Convert theme Rgba to GPUI Hsla.
#[cfg(feature = "gpui-full")]
impl From<Rgba> for GpuiHsla {
    fn from(color: Rgba) -> Self {
        let hsla = color.to_hsla();
        GpuiHsla {
            h: hsla.h,
            s: hsla.s,
            l: hsla.l,
            a: hsla.a,
        }
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

/// Convert a hex color string to HSLA.
///
/// Accepts formats: "#RGB", "#RRGGBB", "RGB", "RRGGBB"
///
/// # Arguments
///
/// * `hex` - Hex color string
///
/// # Returns
///
/// HSLA color or None if parsing fails
pub fn hex_to_hsla(hex: &str) -> Option<Hsla> {
    let hex = hex.trim_start_matches('#');
    let (r, g, b) = match hex.len() {
        3 => {
            let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 17;
            let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 17;
            let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 17;
            (r, g, b)
        }
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            (r, g, b)
        }
        _ => return None,
    };
    Some(Rgba::rgb(r, g, b).to_hsla())
}

/// Convert a hex color string to HSLA, panicking if invalid.
///
/// Use only with known-valid hex strings (e.g., compile-time constants).
fn hex(hex: &str) -> Hsla {
    hex_to_hsla(hex).expect("Invalid hex color")
}

/// Codirigent color theme.
///
/// Contains all colors used throughout the Codirigent UI. Each color is defined
/// as an HSLA value for flexibility in rendering.
#[derive(Clone, Debug, PartialEq)]
pub struct CodirigentTheme {
    // === Background Colors ===
    /// Main/deepest background color.
    pub background: Hsla,
    /// Panel background color (slightly lighter than background).
    pub panel_background: Hsla,
    /// Header/toolbar background color.
    pub header_background: Hsla,
    /// Sidebar background color.
    pub sidebar_background: Hsla,

    // === Border & Interaction Colors ===
    /// Border color for panels and dividers.
    pub border: Hsla,
    /// Hover state background color.
    pub hover: Hsla,
    /// Active/focused element background.
    pub active: Hsla,
    /// Selection highlight color.
    pub selection: Hsla,

    // === Text Colors ===
    /// Primary text color.
    pub foreground: Hsla,
    /// Secondary text color.
    pub text_secondary: Hsla,
    /// Muted/tertiary text color.
    pub muted: Hsla,

    // === Accent Colors ===
    /// Primary accent color (indigo).
    pub primary: Hsla,
    /// Secondary accent color (indigo-light).
    pub secondary: Hsla,
    /// Purple accent color.
    pub purple: Hsla,
    /// Orange accent color.
    pub orange: Hsla,

    // === Mockup-Specific Colors ===
    /// Icon rail background (narrow left bar).
    pub icon_rail_background: Hsla,
    /// Drawer panel background (expandable left panel).
    pub drawer_background: Hsla,
    /// Selected/focused session ring color.
    pub selected_ring: Hsla,
    /// Broadcast mode accent color (rose).
    pub broadcast_accent: Hsla,
    /// AI summary pill background (indigo tint).
    pub ai_summary_background: Hsla,
    /// AI summary pill text color.
    pub ai_summary_text: Hsla,
    /// Input required overlay background.
    pub input_required_background: Hsla,
    /// Input required accent color.
    pub input_required_accent: Hsla,

    // === Session Status Colors ===
    /// Color for idle sessions.
    pub session_idle: Hsla,
    /// Color for working/active sessions.
    pub session_working: Hsla,
    /// Color for sessions needing attention (input or permission).
    pub session_needs_attention: Hsla,
    /// Color for sessions where Claude just finished responding (not yet viewed).
    pub session_response_ready: Hsla,
    /// Color for sessions with errors.
    pub session_error: Hsla,

    // === Priority Colors ===
    /// High priority color.
    pub priority_high: Hsla,
    /// Medium priority color.
    pub priority_medium: Hsla,
    /// Low priority color.
    pub priority_low: Hsla,

    // === Session Group Colors ===
    /// Session group colors (for visual grouping).
    pub session_colors: [Hsla; 6],

    // === Terminal Colors ===
    /// Cursor color in terminals.
    pub cursor: Hsla,
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

    // === Layout ===
    /// Gap between grid cells in pixels.
    pub grid_gap: f32,

    // === Typography ===
    /// Base font size for UI text.
    pub font_size_base: f32,
    /// Small font size.
    pub font_size_small: f32,
    /// Large font size.
    pub font_size_large: f32,
    /// Terminal font size (separate from UI font size).
    pub terminal_font_size: f32,
    /// Terminal line height multiplier (1.0 = natural font height).
    pub terminal_line_height: f32,
    /// Terminal font family (separate from UI font family).
    pub terminal_font_family: String,

    // === Spacing ===
    /// Base spacing unit in pixels.
    pub spacing_base: f32,
    /// Small spacing.
    pub spacing_small: f32,
    /// Large spacing.
    pub spacing_large: f32,
}

impl CodirigentTheme {
    /// Create the dark theme.
    ///
    /// Uses the Dirigent dashboard color palette.
    pub fn dark() -> Self {
        Self {
            // === Background Colors (from mockup) ===
            background: hex("#050505"),         // Darkest background
            panel_background: hex("#0c0c0e"),   // Panel background
            header_background: hex("#09090b"),  // Header/toolbar background
            sidebar_background: hex("#0c0c0e"), // Same as panel

            // === Border & Interaction Colors ===
            border: hex("#1a1a1f"), // Border color
            hover: hex("#151518"),  // Hover state
            active: hex("#1a1a22"), // Active/focused state
            selection: Hsla {
                a: 0.3,
                ..hex("#6366f1")
            }, // Primary @ 30%

            // === Text Colors ===
            foreground: hex("#e0e0e0"),     // Primary text
            text_secondary: hex("#888888"), // Secondary text
            muted: hex("#555555"),          // Muted text

            // === Accent Colors ===
            primary: hex("#6366f1"),   // Indigo-500 (main accent)
            secondary: hex("#818cf8"), // Indigo-400
            purple: hex("#A78BFA"),    // Purple
            orange: hex("#F59E0B"),    // Orange

            // === Mockup-Specific Colors ===
            icon_rail_background: hex("#0c0c0e"),
            drawer_background: hex("#121214"),
            selected_ring: hex("#6366f1"),
            broadcast_accent: hex("#f43f5e"),
            ai_summary_background: Hsla {
                a: 0.05,
                ..hex("#6366f1")
            },
            ai_summary_text: Hsla {
                a: 0.8,
                ..hex("#c7d2fe")
            },
            input_required_background: Hsla {
                a: 0.2,
                ..hex("#4c0519")
            },
            input_required_accent: hex("#f43f5e"),

            // === Session Status Colors ===
            session_idle: hex("#52525b"),            // Zinc-600 for idle
            session_working: hex("#f59e0b"),         // Amber-500 for working
            session_needs_attention: hex("#f43f5e"), // Rose-500 for needs attention
            session_response_ready: hex("#22c55e"),  // Green-500 for response ready
            session_error: hex("#ef4444"),           // Red-500 for error

            // === Priority Colors ===
            priority_high: hex("#FF6B6B"),   // Red
            priority_medium: hex("#F59E0B"), // Orange
            priority_low: hex("#5B8DEF"),    // Blue

            // === Session Group Colors ===
            session_colors: [
                hex("#6366f1"), // Indigo
                hex("#818cf8"), // Indigo-400
                hex("#A78BFA"), // Purple
                hex("#F59E0B"), // Orange
                hex("#f43f5e"), // Rose
                hex("#10B981"), // Green
            ],

            // === Terminal Colors ===
            cursor: hex("#6366f1"), // Indigo cursor
            ansi: AnsiColors::default(),
            terminal_background: Rgba::rgb(5, 5, 5), // #050505
            terminal_foreground: Rgba::rgb(224, 224, 224), // #e0e0e0
            terminal_cursor: Rgba::rgb(99, 102, 241), // #6366f1
            terminal_selection_bg: Rgba::new(99, 102, 241, 77), // #6366f1 @ 30%
            terminal_selection_fg: Rgba::rgb(224, 224, 224), // #e0e0e0

            // === Layout ===
            grid_gap: 4.0,

            // === Typography ===
            font_size_base: 13.0,
            font_size_small: 11.0,
            font_size_large: 15.0,
            terminal_font_size: 13.0,
            terminal_line_height: 1.0,
            terminal_font_family: default_terminal_font_family().to_string(),

            // === Spacing ===
            spacing_base: 8.0,
            spacing_small: 4.0,
            spacing_large: 16.0,
        }
    }

    /// Create the light theme.
    ///
    /// Light theme variant with inverted colors for better readability
    /// in bright environments.
    pub fn light() -> Self {
        Self {
            // === Background Colors ===
            background: hex("#f5f5f7"),
            panel_background: hex("#ffffff"),
            header_background: hex("#e8e8ec"),
            sidebar_background: hex("#f0f0f4"),

            // === Border & Interaction Colors ===
            border: hex("#d0d0d8"),
            hover: hex("#e8e8ec"),
            active: hex("#d8d8e0"),
            selection: Hsla {
                a: 0.2,
                ..hex("#4f46e5")
            }, // Indigo-600 @ 20%

            // === Text Colors ===
            foreground: hex("#1a1a1c"),
            text_secondary: hex("#666666"),
            muted: hex("#999999"),

            // === Accent Colors (slightly darker for light bg) ===
            primary: hex("#4f46e5"),   // Indigo-600
            secondary: hex("#6366f1"), // Indigo-500
            purple: hex("#8B6FD9"),    // Darker purple
            orange: hex("#D98A0B"),    // Darker orange

            // === Mockup-Specific Colors ===
            icon_rail_background: hex("#f0f0f4"),
            drawer_background: hex("#ffffff"),
            selected_ring: hex("#4f46e5"),
            broadcast_accent: hex("#e11d48"),
            ai_summary_background: Hsla {
                a: 0.08,
                ..hex("#4f46e5")
            },
            ai_summary_text: hex("#3730a3"),
            input_required_background: Hsla {
                a: 0.1,
                ..hex("#e11d48")
            },
            input_required_accent: hex("#e11d48"),

            // === Session Status Colors ===
            session_idle: hex("#71717a"),            // Zinc-500
            session_working: hex("#d97706"),         // Amber-600
            session_needs_attention: hex("#e11d48"), // Rose-600
            session_response_ready: hex("#16a34a"),  // Green-600 for response ready
            session_error: hex("#dc2626"),           // Red-600

            // === Priority Colors ===
            priority_high: hex("#dc2626"),
            priority_medium: hex("#D98A0B"),
            priority_low: hex("#6366f1"),

            // === Session Group Colors ===
            session_colors: [
                hex("#4f46e5"), // Indigo-600
                hex("#6366f1"), // Indigo-500
                hex("#8B6FD9"), // Purple
                hex("#D98A0B"), // Orange
                hex("#e11d48"), // Rose
                hex("#059669"), // Emerald
            ],

            // === Terminal Colors ===
            cursor: hex("#4f46e5"),
            ansi: AnsiColors::default(),
            terminal_background: Rgba::rgb(245, 245, 247),
            terminal_foreground: Rgba::rgb(26, 26, 28),
            terminal_cursor: Rgba::rgb(79, 70, 229), // #4f46e5
            terminal_selection_bg: Rgba::new(79, 70, 229, 51), // #4f46e5 @ 20%
            terminal_selection_fg: Rgba::rgb(26, 26, 28),

            // === Layout ===
            grid_gap: 4.0,

            // === Typography ===
            font_size_base: 13.0,
            font_size_small: 11.0,
            font_size_large: 15.0,
            terminal_font_size: 13.0,
            terminal_line_height: 1.0,
            terminal_font_family: default_terminal_font_family().to_string(),

            // === Spacing ===
            spacing_base: 8.0,
            spacing_small: 4.0,
            spacing_large: 16.0,
        }
    }

    /// Create the Catppuccin Latte theme.
    pub fn catppuccin_latte() -> Self {
        let mut theme = Self::light();
        theme.background = hex("#eff1f5");
        theme.panel_background = hex("#ffffff");
        theme.header_background = hex("#dce0e8");
        theme.sidebar_background = hex("#e6e9ef");
        theme.icon_rail_background = hex("#e6e9ef");
        theme.drawer_background = hex("#ffffff");
        theme.border = hex("#ccd0da");
        theme.hover = hex("#dce0e8");
        theme.active = hex("#ccd0da");
        theme.selection = Hsla {
            a: 0.18,
            ..hex("#1e66f5")
        };
        theme.foreground = hex("#4c4f69");
        theme.text_secondary = hex("#5c5f77");
        theme.muted = hex("#8c8fa1");
        theme.primary = hex("#1e66f5");
        theme.secondary = hex("#179299");
        theme.purple = hex("#8839ef");
        theme.orange = hex("#fe640b");
        theme.selected_ring = hex("#1e66f5");
        theme.broadcast_accent = hex("#d20f39");
        theme.ai_summary_background = Hsla {
            a: 0.08,
            ..hex("#1e66f5")
        };
        theme.ai_summary_text = hex("#1e66f5");
        theme.input_required_background = Hsla {
            a: 0.12,
            ..hex("#d20f39")
        };
        theme.input_required_accent = hex("#d20f39");
        theme.session_idle = hex("#8c8fa1");
        theme.session_working = hex("#df8e1d");
        theme.session_needs_attention = hex("#d20f39");
        theme.session_response_ready = hex("#40a02b");
        theme.session_error = hex("#d20f39");
        theme.priority_high = hex("#d20f39");
        theme.priority_medium = hex("#df8e1d");
        theme.priority_low = hex("#1e66f5");
        theme.session_colors = [
            hex("#1e66f5"),
            hex("#179299"),
            hex("#8839ef"),
            hex("#fe640b"),
            hex("#d20f39"),
            hex("#40a02b"),
        ];
        theme.cursor = hex("#1e66f5");
        theme.ansi = AnsiColors {
            colors: [
                Rgba::rgb(0x5c, 0x5f, 0x77),
                Rgba::rgb(0xd2, 0x0f, 0x39),
                Rgba::rgb(0x40, 0xa0, 0x2b),
                Rgba::rgb(0xdf, 0x8e, 0x1d),
                Rgba::rgb(0x1e, 0x66, 0xf5),
                Rgba::rgb(0xea, 0x76, 0xcb),
                Rgba::rgb(0x17, 0x92, 0x99),
                Rgba::rgb(0xac, 0xb0, 0xbe),
                Rgba::rgb(0x6c, 0x6f, 0x85),
                Rgba::rgb(0xd2, 0x0f, 0x39),
                Rgba::rgb(0x40, 0xa0, 0x2b),
                Rgba::rgb(0xdf, 0x8e, 0x1d),
                Rgba::rgb(0x1e, 0x66, 0xf5),
                Rgba::rgb(0xea, 0x76, 0xcb),
                Rgba::rgb(0x17, 0x92, 0x99),
                Rgba::rgb(0x4c, 0x4f, 0x69),
            ],
        };
        theme.terminal_background = Rgba::rgb(0xef, 0xf1, 0xf5);
        theme.terminal_foreground = Rgba::rgb(0x4c, 0x4f, 0x69);
        theme.terminal_cursor = Rgba::rgb(0x1e, 0x66, 0xf5);
        theme.terminal_selection_bg = Rgba::new(0xcc, 0xd0, 0xda, 0xcc);
        theme.terminal_selection_fg = Rgba::rgb(0x4c, 0x4f, 0x69);
        theme
    }

    /// Create the GitHub Light theme.
    pub fn github_light() -> Self {
        let mut theme = Self::light();
        theme.background = hex("#f6f8fa");
        theme.panel_background = hex("#ffffff");
        theme.header_background = hex("#f3f4f6");
        theme.sidebar_background = hex("#f6f8fa");
        theme.icon_rail_background = hex("#f6f8fa");
        theme.drawer_background = hex("#ffffff");
        theme.border = hex("#d0d7de");
        theme.hover = hex("#eef2f6");
        theme.active = hex("#d8dee4");
        theme.selection = Hsla {
            a: 0.16,
            ..hex("#0969da")
        };
        theme.foreground = hex("#24292f");
        theme.text_secondary = hex("#57606a");
        theme.muted = hex("#6e7781");
        theme.primary = hex("#0969da");
        theme.secondary = hex("#1f883d");
        theme.purple = hex("#8250df");
        theme.orange = hex("#bc4c00");
        theme.selected_ring = hex("#0969da");
        theme.broadcast_accent = hex("#cf222e");
        theme.ai_summary_background = Hsla {
            a: 0.08,
            ..hex("#0969da")
        };
        theme.ai_summary_text = hex("#0550ae");
        theme.input_required_background = Hsla {
            a: 0.12,
            ..hex("#cf222e")
        };
        theme.input_required_accent = hex("#cf222e");
        theme.session_idle = hex("#6e7781");
        theme.session_working = hex("#bc4c00");
        theme.session_needs_attention = hex("#cf222e");
        theme.session_response_ready = hex("#1a7f37");
        theme.session_error = hex("#cf222e");
        theme.priority_high = hex("#cf222e");
        theme.priority_medium = hex("#bc4c00");
        theme.priority_low = hex("#0969da");
        theme.session_colors = [
            hex("#0969da"),
            hex("#1f883d"),
            hex("#8250df"),
            hex("#bc4c00"),
            hex("#cf222e"),
            hex("#1a7f37"),
        ];
        theme.cursor = hex("#0969da");
        theme.ansi = AnsiColors {
            colors: [
                Rgba::rgb(0x24, 0x29, 0x2f),
                Rgba::rgb(0xcf, 0x22, 0x2e),
                Rgba::rgb(0x1a, 0x7f, 0x37),
                Rgba::rgb(0x9a, 0x67, 0x00),
                Rgba::rgb(0x09, 0x69, 0xda),
                Rgba::rgb(0x82, 0x50, 0xdf),
                Rgba::rgb(0x05, 0x50, 0xae),
                Rgba::rgb(0x57, 0x60, 0x6a),
                Rgba::rgb(0x6e, 0x77, 0x81),
                Rgba::rgb(0xa4, 0x0e, 0x26),
                Rgba::rgb(0x1a, 0x7f, 0x37),
                Rgba::rgb(0xbf, 0x87, 0x00),
                Rgba::rgb(0x21, 0x8b, 0xff),
                Rgba::rgb(0xa4, 0x75, 0xf9),
                Rgba::rgb(0x09, 0x69, 0xda),
                Rgba::rgb(0x24, 0x29, 0x2f),
            ],
        };
        theme.terminal_background = Rgba::rgb(0xff, 0xff, 0xff);
        theme.terminal_foreground = Rgba::rgb(0x24, 0x29, 0x2f);
        theme.terminal_cursor = Rgba::rgb(0x09, 0x69, 0xda);
        theme.terminal_selection_bg = Rgba::new(0xd8, 0xde, 0xe4, 0xcc);
        theme.terminal_selection_fg = Rgba::rgb(0x24, 0x29, 0x2f);
        theme
    }

    /// Create the Solarized Light theme.
    pub fn solarized_light() -> Self {
        let mut theme = Self::light();
        theme.background = hex("#fdf6e3");
        theme.panel_background = hex("#fffdf7");
        theme.header_background = hex("#eee8d5");
        theme.sidebar_background = hex("#f5efdc");
        theme.icon_rail_background = hex("#f5efdc");
        theme.drawer_background = hex("#fffdf7");
        theme.border = hex("#d8cfb1");
        theme.hover = hex("#eee8d5");
        theme.active = hex("#e3dcc6");
        theme.selection = Hsla {
            a: 0.18,
            ..hex("#268bd2")
        };
        theme.foreground = hex("#586e75");
        theme.text_secondary = hex("#657b83");
        theme.muted = hex("#93a1a1");
        theme.primary = hex("#268bd2");
        theme.secondary = hex("#2aa198");
        theme.purple = hex("#6c71c4");
        theme.orange = hex("#cb4b16");
        theme.selected_ring = hex("#268bd2");
        theme.broadcast_accent = hex("#dc322f");
        theme.ai_summary_background = Hsla {
            a: 0.08,
            ..hex("#268bd2")
        };
        theme.ai_summary_text = hex("#005f87");
        theme.input_required_background = Hsla {
            a: 0.12,
            ..hex("#dc322f")
        };
        theme.input_required_accent = hex("#dc322f");
        theme.session_idle = hex("#93a1a1");
        theme.session_working = hex("#b58900");
        theme.session_needs_attention = hex("#dc322f");
        theme.session_response_ready = hex("#859900");
        theme.session_error = hex("#dc322f");
        theme.priority_high = hex("#dc322f");
        theme.priority_medium = hex("#cb4b16");
        theme.priority_low = hex("#268bd2");
        theme.session_colors = [
            hex("#268bd2"),
            hex("#2aa198"),
            hex("#6c71c4"),
            hex("#cb4b16"),
            hex("#dc322f"),
            hex("#859900"),
        ];
        theme.cursor = hex("#268bd2");
        theme.ansi = AnsiColors {
            colors: [
                Rgba::rgb(0x07, 0x36, 0x42),
                Rgba::rgb(0xdc, 0x32, 0x2f),
                Rgba::rgb(0x85, 0x99, 0x00),
                Rgba::rgb(0xb5, 0x89, 0x00),
                Rgba::rgb(0x26, 0x8b, 0xd2),
                Rgba::rgb(0xd3, 0x36, 0x82),
                Rgba::rgb(0x2a, 0xa1, 0x98),
                Rgba::rgb(0xee, 0xe8, 0xd5),
                Rgba::rgb(0x65, 0x7b, 0x83),
                Rgba::rgb(0xcb, 0x4b, 0x16),
                Rgba::rgb(0x93, 0xa1, 0xa1),
                Rgba::rgb(0x65, 0x7b, 0x83),
                Rgba::rgb(0x83, 0x94, 0x96),
                Rgba::rgb(0x6c, 0x71, 0xc4),
                Rgba::rgb(0x58, 0x6e, 0x75),
                Rgba::rgb(0x00, 0x2b, 0x36),
            ],
        };
        theme.terminal_background = Rgba::rgb(0xfd, 0xf6, 0xe3);
        theme.terminal_foreground = Rgba::rgb(0x58, 0x6e, 0x75);
        theme.terminal_cursor = Rgba::rgb(0x26, 0x8b, 0xd2);
        theme.terminal_selection_bg = Rgba::new(0xee, 0xe8, 0xd5, 0xcc);
        theme.terminal_selection_fg = Rgba::rgb(0x58, 0x6e, 0x75);
        theme
    }

    /// Create the Catppuccin Mocha theme.
    pub fn catppuccin_mocha() -> Self {
        let mut theme = Self::dark();
        theme.background = hex("#11111b");
        theme.panel_background = hex("#181825");
        theme.header_background = hex("#181825");
        theme.sidebar_background = hex("#181825");
        theme.icon_rail_background = hex("#181825");
        theme.drawer_background = hex("#1e1e2e");
        theme.border = hex("#313244");
        theme.hover = hex("#1e1e2e");
        theme.active = hex("#313244");
        theme.selection = Hsla {
            a: 0.24,
            ..hex("#89b4fa")
        };
        theme.foreground = hex("#cdd6f4");
        theme.text_secondary = hex("#a6adc8");
        theme.muted = hex("#6c7086");
        theme.primary = hex("#89b4fa");
        theme.secondary = hex("#74c7ec");
        theme.purple = hex("#cba6f7");
        theme.orange = hex("#fab387");
        theme.selected_ring = hex("#89b4fa");
        theme.broadcast_accent = hex("#f38ba8");
        theme.ai_summary_background = Hsla {
            a: 0.08,
            ..hex("#89b4fa")
        };
        theme.ai_summary_text = hex("#b4befe");
        theme.input_required_background = Hsla {
            a: 0.16,
            ..hex("#f38ba8")
        };
        theme.input_required_accent = hex("#f38ba8");
        theme.session_idle = hex("#6c7086");
        theme.session_working = hex("#fab387");
        theme.session_needs_attention = hex("#f38ba8");
        theme.session_response_ready = hex("#a6e3a1");
        theme.session_error = hex("#f38ba8");
        theme.priority_high = hex("#f38ba8");
        theme.priority_medium = hex("#fab387");
        theme.priority_low = hex("#89b4fa");
        theme.session_colors = [
            hex("#89b4fa"),
            hex("#74c7ec"),
            hex("#cba6f7"),
            hex("#fab387"),
            hex("#f38ba8"),
            hex("#a6e3a1"),
        ];
        theme.cursor = hex("#f5e0dc");
        theme.ansi = AnsiColors {
            colors: [
                Rgba::rgb(0x45, 0x47, 0x5a),
                Rgba::rgb(0xf3, 0x8b, 0xa8),
                Rgba::rgb(0xa6, 0xe3, 0xa1),
                Rgba::rgb(0xf9, 0xe2, 0xaf),
                Rgba::rgb(0x89, 0xb4, 0xfa),
                Rgba::rgb(0xf5, 0xc2, 0xe7),
                Rgba::rgb(0x94, 0xe2, 0xd5),
                Rgba::rgb(0xba, 0xc2, 0xde),
                Rgba::rgb(0x58, 0x5b, 0x70),
                Rgba::rgb(0xf3, 0x8b, 0xa8),
                Rgba::rgb(0xa6, 0xe3, 0xa1),
                Rgba::rgb(0xf9, 0xe2, 0xaf),
                Rgba::rgb(0x89, 0xb4, 0xfa),
                Rgba::rgb(0xf5, 0xc2, 0xe7),
                Rgba::rgb(0x94, 0xe2, 0xd5),
                Rgba::rgb(0xa6, 0xad, 0xc8),
            ],
        };
        theme.terminal_background = Rgba::rgb(0x1e, 0x1e, 0x2e);
        theme.terminal_foreground = Rgba::rgb(0xcd, 0xd6, 0xf4);
        theme.terminal_cursor = Rgba::rgb(0xf5, 0xe0, 0xdc);
        theme.terminal_selection_bg = Rgba::new(0x58, 0x5b, 0x70, 0xcc);
        theme.terminal_selection_fg = Rgba::rgb(0xcd, 0xd6, 0xf4);
        theme
    }

    /// Create the Tokyo Night theme.
    pub fn tokyo_night() -> Self {
        let mut theme = Self::dark();
        theme.background = hex("#16161e");
        theme.panel_background = hex("#1a1b26");
        theme.header_background = hex("#1f2335");
        theme.sidebar_background = hex("#1a1b26");
        theme.icon_rail_background = hex("#1a1b26");
        theme.drawer_background = hex("#1f2335");
        theme.border = hex("#292e42");
        theme.hover = hex("#24283b");
        theme.active = hex("#2f3549");
        theme.selection = Hsla {
            a: 0.24,
            ..hex("#7aa2f7")
        };
        theme.foreground = hex("#c0caf5");
        theme.text_secondary = hex("#9aa5ce");
        theme.muted = hex("#565f89");
        theme.primary = hex("#7aa2f7");
        theme.secondary = hex("#7dcfff");
        theme.purple = hex("#bb9af7");
        theme.orange = hex("#ff9e64");
        theme.selected_ring = hex("#7aa2f7");
        theme.broadcast_accent = hex("#f7768e");
        theme.ai_summary_background = Hsla {
            a: 0.08,
            ..hex("#7aa2f7")
        };
        theme.ai_summary_text = hex("#c0caf5");
        theme.input_required_background = Hsla {
            a: 0.14,
            ..hex("#f7768e")
        };
        theme.input_required_accent = hex("#f7768e");
        theme.session_idle = hex("#565f89");
        theme.session_working = hex("#ff9e64");
        theme.session_needs_attention = hex("#f7768e");
        theme.session_response_ready = hex("#9ece6a");
        theme.session_error = hex("#f7768e");
        theme.priority_high = hex("#f7768e");
        theme.priority_medium = hex("#ff9e64");
        theme.priority_low = hex("#7aa2f7");
        theme.session_colors = [
            hex("#7aa2f7"),
            hex("#7dcfff"),
            hex("#bb9af7"),
            hex("#ff9e64"),
            hex("#f7768e"),
            hex("#9ece6a"),
        ];
        theme.cursor = hex("#c0caf5");
        theme.ansi = AnsiColors {
            colors: [
                Rgba::rgb(0x15, 0x16, 0x1e),
                Rgba::rgb(0xf7, 0x76, 0x8e),
                Rgba::rgb(0x9e, 0xce, 0x6a),
                Rgba::rgb(0xe0, 0xaf, 0x68),
                Rgba::rgb(0x7a, 0xa2, 0xf7),
                Rgba::rgb(0xbb, 0x9a, 0xf7),
                Rgba::rgb(0x7d, 0xcf, 0xff),
                Rgba::rgb(0xa9, 0xb1, 0xd6),
                Rgba::rgb(0x41, 0x48, 0x68),
                Rgba::rgb(0xf7, 0x76, 0x8e),
                Rgba::rgb(0x9e, 0xce, 0x6a),
                Rgba::rgb(0xe0, 0xaf, 0x68),
                Rgba::rgb(0x7a, 0xa2, 0xf7),
                Rgba::rgb(0xbb, 0x9a, 0xf7),
                Rgba::rgb(0x7d, 0xcf, 0xff),
                Rgba::rgb(0xc0, 0xca, 0xf5),
            ],
        };
        theme.terminal_background = Rgba::rgb(0x1a, 0x1b, 0x26);
        theme.terminal_foreground = Rgba::rgb(0xc0, 0xca, 0xf5);
        theme.terminal_cursor = Rgba::rgb(0xc0, 0xca, 0xf5);
        theme.terminal_selection_bg = Rgba::new(0x28, 0x34, 0x57, 0xcc);
        theme.terminal_selection_fg = Rgba::rgb(0xc0, 0xca, 0xf5);
        theme
    }

    /// Create the One Dark theme.
    pub fn one_dark() -> Self {
        let mut theme = Self::dark();
        theme.background = hex("#21252b");
        theme.panel_background = hex("#282c34");
        theme.header_background = hex("#1f2329");
        theme.sidebar_background = hex("#21252b");
        theme.icon_rail_background = hex("#21252b");
        theme.drawer_background = hex("#282c34");
        theme.border = hex("#3b4048");
        theme.hover = hex("#2c313a");
        theme.active = hex("#333842");
        theme.selection = Hsla {
            a: 0.22,
            ..hex("#61afef")
        };
        theme.foreground = hex("#abb2bf");
        theme.text_secondary = hex("#828997");
        theme.muted = hex("#5c6370");
        theme.primary = hex("#61afef");
        theme.secondary = hex("#56b6c2");
        theme.purple = hex("#c678dd");
        theme.orange = hex("#d19a66");
        theme.selected_ring = hex("#61afef");
        theme.broadcast_accent = hex("#e06c75");
        theme.ai_summary_background = Hsla {
            a: 0.08,
            ..hex("#61afef")
        };
        theme.ai_summary_text = hex("#cdd6f4");
        theme.input_required_background = Hsla {
            a: 0.14,
            ..hex("#e06c75")
        };
        theme.input_required_accent = hex("#e06c75");
        theme.session_idle = hex("#5c6370");
        theme.session_working = hex("#d19a66");
        theme.session_needs_attention = hex("#e06c75");
        theme.session_response_ready = hex("#98c379");
        theme.session_error = hex("#e06c75");
        theme.priority_high = hex("#e06c75");
        theme.priority_medium = hex("#d19a66");
        theme.priority_low = hex("#61afef");
        theme.session_colors = [
            hex("#61afef"),
            hex("#56b6c2"),
            hex("#c678dd"),
            hex("#d19a66"),
            hex("#e06c75"),
            hex("#98c379"),
        ];
        theme.cursor = hex("#61afef");
        theme.ansi = AnsiColors {
            colors: [
                Rgba::rgb(0x28, 0x2c, 0x34),
                Rgba::rgb(0xe0, 0x6c, 0x75),
                Rgba::rgb(0x98, 0xc3, 0x79),
                Rgba::rgb(0xe5, 0xc0, 0x7b),
                Rgba::rgb(0x61, 0xaf, 0xef),
                Rgba::rgb(0xc6, 0x78, 0xdd),
                Rgba::rgb(0x56, 0xb6, 0xc2),
                Rgba::rgb(0xdc, 0xdf, 0xe4),
                Rgba::rgb(0x5c, 0x63, 0x70),
                Rgba::rgb(0xe0, 0x6c, 0x75),
                Rgba::rgb(0x98, 0xc3, 0x79),
                Rgba::rgb(0xe5, 0xc0, 0x7b),
                Rgba::rgb(0x61, 0xaf, 0xef),
                Rgba::rgb(0xc6, 0x78, 0xdd),
                Rgba::rgb(0x56, 0xb6, 0xc2),
                Rgba::rgb(0xff, 0xff, 0xff),
            ],
        };
        theme.terminal_background = Rgba::rgb(0x28, 0x2c, 0x34);
        theme.terminal_foreground = Rgba::rgb(0xab, 0xb2, 0xbf);
        theme.terminal_cursor = Rgba::rgb(0x52, 0x8b, 0xff);
        theme.terminal_selection_bg = Rgba::new(0x3e, 0x44, 0x51, 0xcc);
        theme.terminal_selection_fg = Rgba::rgb(0xab, 0xb2, 0xbf);
        theme
    }

    /// Create the Gruvbox Dark theme.
    pub fn gruvbox_dark() -> Self {
        let mut theme = Self::dark();
        theme.background = hex("#1d2021");
        theme.panel_background = hex("#282828");
        theme.header_background = hex("#32302f");
        theme.sidebar_background = hex("#282828");
        theme.icon_rail_background = hex("#282828");
        theme.drawer_background = hex("#32302f");
        theme.border = hex("#504945");
        theme.hover = hex("#3c3836");
        theme.active = hex("#504945");
        theme.selection = Hsla {
            a: 0.22,
            ..hex("#83a598")
        };
        theme.foreground = hex("#ebdbb2");
        theme.text_secondary = hex("#d5c4a1");
        theme.muted = hex("#928374");
        theme.primary = hex("#83a598");
        theme.secondary = hex("#8ec07c");
        theme.purple = hex("#d3869b");
        theme.orange = hex("#fe8019");
        theme.selected_ring = hex("#83a598");
        theme.broadcast_accent = hex("#fb4934");
        theme.ai_summary_background = Hsla {
            a: 0.08,
            ..hex("#83a598")
        };
        theme.ai_summary_text = hex("#ebdbb2");
        theme.input_required_background = Hsla {
            a: 0.14,
            ..hex("#fb4934")
        };
        theme.input_required_accent = hex("#fb4934");
        theme.session_idle = hex("#928374");
        theme.session_working = hex("#fabd2f");
        theme.session_needs_attention = hex("#fb4934");
        theme.session_response_ready = hex("#b8bb26");
        theme.session_error = hex("#fb4934");
        theme.priority_high = hex("#fb4934");
        theme.priority_medium = hex("#fe8019");
        theme.priority_low = hex("#83a598");
        theme.session_colors = [
            hex("#83a598"),
            hex("#8ec07c"),
            hex("#d3869b"),
            hex("#fe8019"),
            hex("#fb4934"),
            hex("#b8bb26"),
        ];
        theme.cursor = hex("#fabd2f");
        theme.ansi = AnsiColors {
            colors: [
                Rgba::rgb(0x28, 0x28, 0x28),
                Rgba::rgb(0xcc, 0x24, 0x1d),
                Rgba::rgb(0x98, 0x97, 0x1a),
                Rgba::rgb(0xd7, 0x99, 0x21),
                Rgba::rgb(0x45, 0x85, 0x88),
                Rgba::rgb(0xb1, 0x62, 0x86),
                Rgba::rgb(0x68, 0x9d, 0x6a),
                Rgba::rgb(0xa8, 0x99, 0x84),
                Rgba::rgb(0x92, 0x83, 0x74),
                Rgba::rgb(0xfb, 0x49, 0x34),
                Rgba::rgb(0xb8, 0xbb, 0x26),
                Rgba::rgb(0xfa, 0xbd, 0x2f),
                Rgba::rgb(0x83, 0xa5, 0x98),
                Rgba::rgb(0xd3, 0x86, 0x9b),
                Rgba::rgb(0x8e, 0xc0, 0x7c),
                Rgba::rgb(0xeb, 0xdb, 0xb2),
            ],
        };
        theme.terminal_background = Rgba::rgb(0x28, 0x28, 0x28);
        theme.terminal_foreground = Rgba::rgb(0xeb, 0xdb, 0xb2);
        theme.terminal_cursor = Rgba::rgb(0xfa, 0xbd, 0x2f);
        theme.terminal_selection_bg = Rgba::new(0x50, 0x49, 0x45, 0xcc);
        theme.terminal_selection_fg = Rgba::rgb(0xeb, 0xdb, 0xb2);
        theme
    }

    /// Create the Solarized Dark theme.
    pub fn solarized_dark() -> Self {
        let mut theme = Self::dark();
        theme.background = hex("#002b36");
        theme.panel_background = hex("#073642");
        theme.header_background = hex("#00212b");
        theme.sidebar_background = hex("#073642");
        theme.icon_rail_background = hex("#073642");
        theme.drawer_background = hex("#0b3c49");
        theme.border = hex("#0f4b59");
        theme.hover = hex("#0b3c49");
        theme.active = hex("#135564");
        theme.selection = Hsla {
            a: 0.2,
            ..hex("#268bd2")
        };
        theme.foreground = hex("#839496");
        theme.text_secondary = hex("#93a1a1");
        theme.muted = hex("#586e75");
        theme.primary = hex("#268bd2");
        theme.secondary = hex("#2aa198");
        theme.purple = hex("#6c71c4");
        theme.orange = hex("#cb4b16");
        theme.selected_ring = hex("#268bd2");
        theme.broadcast_accent = hex("#dc322f");
        theme.ai_summary_background = Hsla {
            a: 0.08,
            ..hex("#268bd2")
        };
        theme.ai_summary_text = hex("#93a1a1");
        theme.input_required_background = Hsla {
            a: 0.14,
            ..hex("#dc322f")
        };
        theme.input_required_accent = hex("#dc322f");
        theme.session_idle = hex("#586e75");
        theme.session_working = hex("#b58900");
        theme.session_needs_attention = hex("#dc322f");
        theme.session_response_ready = hex("#859900");
        theme.session_error = hex("#dc322f");
        theme.priority_high = hex("#dc322f");
        theme.priority_medium = hex("#cb4b16");
        theme.priority_low = hex("#268bd2");
        theme.session_colors = [
            hex("#268bd2"),
            hex("#2aa198"),
            hex("#6c71c4"),
            hex("#cb4b16"),
            hex("#dc322f"),
            hex("#859900"),
        ];
        theme.cursor = hex("#93a1a1");
        theme.ansi = AnsiColors {
            colors: [
                Rgba::rgb(0x07, 0x36, 0x42),
                Rgba::rgb(0xdc, 0x32, 0x2f),
                Rgba::rgb(0x85, 0x99, 0x00),
                Rgba::rgb(0xb5, 0x89, 0x00),
                Rgba::rgb(0x26, 0x8b, 0xd2),
                Rgba::rgb(0xd3, 0x36, 0x82),
                Rgba::rgb(0x2a, 0xa1, 0x98),
                Rgba::rgb(0xee, 0xe8, 0xd5),
                Rgba::rgb(0x00, 0x2b, 0x36),
                Rgba::rgb(0xcb, 0x4b, 0x16),
                Rgba::rgb(0x58, 0x6e, 0x75),
                Rgba::rgb(0x65, 0x7b, 0x83),
                Rgba::rgb(0x83, 0x94, 0x96),
                Rgba::rgb(0x6c, 0x71, 0xc4),
                Rgba::rgb(0x93, 0xa1, 0xa1),
                Rgba::rgb(0xfd, 0xf6, 0xe3),
            ],
        };
        theme.terminal_background = Rgba::rgb(0x00, 0x2b, 0x36);
        theme.terminal_foreground = Rgba::rgb(0x83, 0x94, 0x96);
        theme.terminal_cursor = Rgba::rgb(0x93, 0xa1, 0xa1);
        theme.terminal_selection_bg = Rgba::new(0x07, 0x36, 0x42, 0xcc);
        theme.terminal_selection_fg = Rgba::rgb(0x93, 0xa1, 0xa1);
        theme
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
            SessionStatus::NeedsAttention => self.session_needs_attention,
            SessionStatus::ResponseReady => self.session_response_ready,
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
            SessionStatus::NeedsAttention => "Attention",
            SessionStatus::ResponseReady => "Ready",
            SessionStatus::Error => "Error",
        }
    }

    /// Get a color from the 256-color indexed palette.
    ///
    /// The 256-color palette is organized as:
    /// - 0-15: Standard ANSI colors
    /// - 16-231: 6x6x6 color cube
    /// - 232-255: Grayscale ramp
    ///
    /// # Arguments
    ///
    /// * `index` - Color index (0-255)
    ///
    /// # Returns
    ///
    /// The RGBA color for the given index
    pub fn get_indexed_color(&self, index: u8) -> Rgba {
        match index {
            // Standard ANSI colors (0-15)
            0..=15 => self.ansi.colors[index as usize],
            // 6x6x6 color cube (16-231)
            16..=231 => {
                let idx = index - 16;
                let r = (idx / 36) % 6;
                let g = (idx / 6) % 6;
                let b = idx % 6;
                // Convert 0-5 to 0-255 (0, 95, 135, 175, 215, 255)
                let to_component = |v: u8| -> u8 {
                    if v == 0 {
                        0
                    } else {
                        55 + v * 40
                    }
                };
                Rgba::rgb(to_component(r), to_component(g), to_component(b))
            }
            // Grayscale ramp (232-255)
            232..=255 => {
                let gray = 8 + (index - 232) * 10;
                Rgba::rgb(gray, gray, gray)
            }
        }
    }
}

impl Default for CodirigentTheme {
    fn default() -> Self {
        Self::dark()
    }
}

/// Default monospace font family for terminals per platform.
pub fn default_terminal_font_family() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "Consolas"
    }
    #[cfg(target_os = "macos")]
    {
        "Menlo"
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        "DejaVu Sans Mono"
    }
    #[cfg(not(any(windows, unix)))]
    {
        "monospace"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dark_theme_creation() {
        let theme = CodirigentTheme::dark();
        // Dark theme should have low lightness background
        assert!(
            theme.background.l < 0.1,
            "Dark bg lightness: {}",
            theme.background.l
        );
        // Dark theme should have high lightness foreground
        assert!(
            theme.foreground.l > 0.5,
            "Dark fg lightness: {}",
            theme.foreground.l
        );
    }

    #[test]
    fn test_light_theme_creation() {
        let theme = CodirigentTheme::light();
        // Light theme should have high lightness background
        assert!(
            theme.background.l > 0.9,
            "Light bg lightness: {}",
            theme.background.l
        );
        // Light theme should have low lightness foreground
        assert!(
            theme.foreground.l < 0.2,
            "Light fg lightness: {}",
            theme.foreground.l
        );
    }

    #[test]
    fn test_default_is_dark() {
        let default = CodirigentTheme::default();
        let dark = CodirigentTheme::dark();
        assert_eq!(default.background, dark.background);
    }

    #[test]
    fn test_status_colors_all_variants() {
        let theme = CodirigentTheme::dark();

        // Test all status variants return valid colors
        let statuses = [
            SessionStatus::Idle,
            SessionStatus::Working,
            SessionStatus::NeedsAttention,
            SessionStatus::ResponseReady,
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
        let theme = CodirigentTheme::dark();

        // Status colors should be distinct from each other
        let working = theme.status_color(SessionStatus::Working);
        let attention = theme.status_color(SessionStatus::NeedsAttention);
        let idle = theme.status_color(SessionStatus::Idle);

        assert_ne!(
            working, attention,
            "Working and NeedsAttention should be different"
        );
        assert_ne!(idle, working, "Idle and Working should be different");
    }

    #[test]
    fn test_status_names() {
        assert_eq!(CodirigentTheme::status_name(SessionStatus::Idle), "Idle");
        assert_eq!(
            CodirigentTheme::status_name(SessionStatus::Working),
            "Working"
        );
        assert_eq!(
            CodirigentTheme::status_name(SessionStatus::NeedsAttention),
            "Attention"
        );
        assert_eq!(
            CodirigentTheme::status_name(SessionStatus::ResponseReady),
            "Ready"
        );
        assert_eq!(CodirigentTheme::status_name(SessionStatus::Error), "Error");
    }

    #[test]
    fn test_theme_clone() {
        let theme = CodirigentTheme::dark();
        let cloned = theme.clone();
        assert_eq!(theme, cloned);
    }

    #[test]
    fn test_theme_debug() {
        let theme = CodirigentTheme::dark();
        let debug_str = format!("{:?}", theme);
        assert!(debug_str.contains("CodirigentTheme"));
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
        let theme = CodirigentTheme::dark();
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
        let theme = CodirigentTheme::dark();
        // #050505
        assert_eq!(theme.terminal_background.r, 5);
        assert_eq!(theme.terminal_background.g, 5);
        assert_eq!(theme.terminal_background.b, 5);
    }

    #[test]
    fn test_hex_to_hsla_valid() {
        // Test 6-digit hex
        let color = hex_to_hsla("#FF0000").unwrap();
        assert!((color.h - 0.0).abs() < 0.01); // Red
        assert!((color.s - 1.0).abs() < 0.01);
        assert!((color.l - 0.5).abs() < 0.01);

        // Test without hash
        let color2 = hex_to_hsla("00FF00").unwrap();
        assert!((color2.h - 0.333).abs() < 0.01); // Green

        // Test 3-digit hex
        let color3 = hex_to_hsla("#F00").unwrap();
        assert!((color3.h - 0.0).abs() < 0.01); // Red
    }

    #[test]
    fn test_hex_to_hsla_invalid() {
        assert!(hex_to_hsla("invalid").is_none());
        assert!(hex_to_hsla("#GGG").is_none());
        assert!(hex_to_hsla("#12345").is_none()); // Wrong length
    }

    #[test]
    fn test_accent_colors() {
        let theme = CodirigentTheme::dark();
        // Primary should be teal-ish (around 174 degrees hue, or ~0.48 normalized)
        assert!(theme.primary.s > 0.4, "Primary should be saturated");
        // Secondary should be blue-ish
        assert!(theme.secondary.s > 0.4, "Secondary should be saturated");
    }

    #[test]
    fn test_session_colors() {
        let theme = CodirigentTheme::dark();
        assert_eq!(theme.session_colors.len(), 6);
        // All session colors should be opaque
        for color in theme.session_colors {
            assert_eq!(color.a, 1.0);
        }
    }

    #[test]
    fn test_typography_values() {
        let theme = CodirigentTheme::dark();
        assert_eq!(theme.font_size_base, 13.0);
        assert!(theme.font_size_small < theme.font_size_base);
        assert!(theme.font_size_large > theme.font_size_base);
    }

    #[test]
    fn test_spacing_values() {
        let theme = CodirigentTheme::dark();
        assert_eq!(theme.spacing_base, 8.0);
        assert!(theme.spacing_small < theme.spacing_base);
        assert!(theme.spacing_large > theme.spacing_base);
    }

    #[test]
    fn test_priority_colors() {
        let theme = CodirigentTheme::dark();
        // Priority colors should all be distinct
        assert_ne!(theme.priority_high, theme.priority_medium);
        assert_ne!(theme.priority_medium, theme.priority_low);
        assert_ne!(theme.priority_high, theme.priority_low);
    }

    #[test]
    fn test_mockup_specific_colors() {
        let dark = CodirigentTheme::dark();
        // All new fields should be opaque or have intentional alpha
        assert!(dark.icon_rail_background.a >= 0.9);
        assert!(dark.drawer_background.a >= 0.9);
        assert!(dark.selected_ring.a >= 0.9);
        assert!(dark.broadcast_accent.a >= 0.9);
        assert!(dark.input_required_accent.a >= 0.9);
        // Semi-transparent fields
        assert!(dark.ai_summary_background.a < 0.5);
        assert!(dark.input_required_background.a < 0.5);
    }

    #[test]
    fn test_light_theme_mockup_colors() {
        let light = CodirigentTheme::light();
        assert!(light.icon_rail_background.a >= 0.9);
        assert!(light.broadcast_accent.a >= 0.9);
        assert!(light.selected_ring.a >= 0.9);
    }
}
