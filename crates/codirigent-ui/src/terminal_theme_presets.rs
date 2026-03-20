//! Built-in terminal color presets exposed in Settings.

use crate::theme::{AnsiColors, CodirigentTheme, Rgba};

/// Preset ID that keeps terminal colors tied to the active app theme.
pub(crate) const TERMINAL_THEME_DEFAULT_PRESET_ID: &str = "theme-default";

/// A named built-in terminal palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TerminalThemePreset {
    /// Stable persisted preset identifier.
    pub(crate) id: &'static str,
    /// User-facing name.
    pub(crate) name: &'static str,
    /// Short description shown in Settings.
    pub(crate) description: &'static str,
    /// Terminal background color.
    pub(crate) background: Rgba,
    /// Terminal foreground color.
    pub(crate) foreground: Rgba,
    /// Cursor color.
    pub(crate) cursor: Rgba,
    /// Selection background color.
    pub(crate) selection_background: Rgba,
    /// Selection foreground color.
    pub(crate) selection_foreground: Rgba,
    /// ANSI 16-color palette.
    pub(crate) ansi: AnsiColors,
}

const PRESETS: &[TerminalThemePreset] = &[
    TerminalThemePreset {
        id: "catppuccin-mocha",
        name: "Catppuccin Mocha",
        description: "Soft pastel dark theme with gentle contrast.",
        background: Rgba::rgb(0x1e, 0x1e, 0x2e),
        foreground: Rgba::rgb(0xcd, 0xd6, 0xf4),
        cursor: Rgba::rgb(0xf5, 0xe0, 0xdc),
        selection_background: Rgba::new(0x58, 0x5b, 0x70, 0xcc),
        selection_foreground: Rgba::rgb(0xcd, 0xd6, 0xf4),
        ansi: AnsiColors {
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
        },
    },
    TerminalThemePreset {
        id: "tokyo-night",
        name: "Tokyo Night",
        description: "Cool indigo dark theme with vivid blues.",
        background: Rgba::rgb(0x1a, 0x1b, 0x26),
        foreground: Rgba::rgb(0xc0, 0xca, 0xf5),
        cursor: Rgba::rgb(0xc0, 0xca, 0xf5),
        selection_background: Rgba::new(0x28, 0x34, 0x57, 0xcc),
        selection_foreground: Rgba::rgb(0xc0, 0xca, 0xf5),
        ansi: AnsiColors {
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
        },
    },
    TerminalThemePreset {
        id: "one-dark",
        name: "One Dark",
        description: "Balanced editor-style dark theme with clear accents.",
        background: Rgba::rgb(0x28, 0x2c, 0x34),
        foreground: Rgba::rgb(0xab, 0xb2, 0xbf),
        cursor: Rgba::rgb(0x52, 0x8b, 0xff),
        selection_background: Rgba::new(0x3e, 0x44, 0x51, 0xcc),
        selection_foreground: Rgba::rgb(0xab, 0xb2, 0xbf),
        ansi: AnsiColors {
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
        },
    },
    TerminalThemePreset {
        id: "gruvbox-dark",
        name: "Gruvbox Dark",
        description: "Warm earthy palette with strong ANSI contrast.",
        background: Rgba::rgb(0x28, 0x28, 0x28),
        foreground: Rgba::rgb(0xeb, 0xdb, 0xb2),
        cursor: Rgba::rgb(0xfa, 0xbd, 0x2f),
        selection_background: Rgba::new(0x3c, 0x38, 0x36, 0xcc),
        selection_foreground: Rgba::rgb(0xeb, 0xdb, 0xb2),
        ansi: AnsiColors {
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
        },
    },
    TerminalThemePreset {
        id: "solarized-dark",
        name: "Solarized Dark",
        description: "Low-contrast classic palette built for long sessions.",
        background: Rgba::rgb(0x00, 0x2b, 0x36),
        foreground: Rgba::rgb(0x83, 0x94, 0x96),
        cursor: Rgba::rgb(0x93, 0xa1, 0xa1),
        selection_background: Rgba::new(0x07, 0x36, 0x42, 0xcc),
        selection_foreground: Rgba::rgb(0x93, 0xa1, 0xa1),
        ansi: AnsiColors {
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
        },
    },
];

/// User-facing label for a persisted preset ID.
pub(crate) fn terminal_theme_preset_label(id: &str) -> &'static str {
    if id == TERMINAL_THEME_DEFAULT_PRESET_ID {
        "Follow app theme"
    } else {
        get_terminal_theme_preset(id)
            .map(|preset| preset.name)
            .unwrap_or("Follow app theme")
    }
}

/// Short description for a persisted preset ID.
pub(crate) fn terminal_theme_preset_description(id: &str) -> &'static str {
    if id == TERMINAL_THEME_DEFAULT_PRESET_ID {
        "Use the selected Codirigent theme for terminal colors."
    } else {
        get_terminal_theme_preset(id)
            .map(|preset| preset.description)
            .unwrap_or("Use the selected Codirigent theme for terminal colors.")
    }
}

/// All built-in terminal presets.
pub(crate) fn terminal_theme_presets() -> &'static [TerminalThemePreset] {
    PRESETS
}

/// Resolve a built-in preset by ID.
pub(crate) fn get_terminal_theme_preset(id: &str) -> Option<&'static TerminalThemePreset> {
    PRESETS.iter().find(|preset| preset.id == id)
}

/// Apply the preset colors onto a runtime theme.
pub(crate) fn apply_terminal_theme_preset(theme: &mut CodirigentTheme, preset_id: &str) {
    let Some(preset) = get_terminal_theme_preset(preset_id) else {
        return;
    };

    theme.terminal_background = preset.background;
    theme.terminal_foreground = preset.foreground;
    theme.terminal_cursor = preset.cursor;
    theme.cursor = preset.cursor.to_hsla();
    theme.terminal_selection_bg = preset.selection_background;
    theme.terminal_selection_fg = preset.selection_foreground;
    theme.ansi = preset.ansi;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_theme_preset_label_defaults_cleanly() {
        assert_eq!(
            terminal_theme_preset_label(TERMINAL_THEME_DEFAULT_PRESET_ID),
            "Follow app theme"
        );
        assert_eq!(terminal_theme_preset_label("missing"), "Follow app theme");
    }

    #[test]
    fn apply_terminal_theme_preset_updates_terminal_colors() {
        let mut theme = CodirigentTheme::dark();

        apply_terminal_theme_preset(&mut theme, "tokyo-night");

        assert_eq!(theme.terminal_background, Rgba::rgb(0x1a, 0x1b, 0x26));
        assert_eq!(theme.terminal_cursor, Rgba::rgb(0xc0, 0xca, 0xf5));
        assert_eq!(theme.ansi.colors[4], Rgba::rgb(0x7a, 0xa2, 0xf7));
    }
}
