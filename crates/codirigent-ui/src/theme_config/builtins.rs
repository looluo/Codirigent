use super::schema::{
    HexColor, TerminalColors, TerminalPalette, Theme, ThemeAccentColors, ThemeBackgroundColors,
    ThemeBorderColors, ThemeColors, ThemeForegroundColors, ThemeInteractionColors,
    ThemePriorityColors, ThemeSpacing, ThemeStatusColors, ThemeTypography,
};
use crate::theme::{AnsiColors, CodirigentTheme, Hsla, Rgba};

const DEFAULT_UI_FONT_FAMILY: &str = "Inter";
const DEFAULT_BORDER_RADIUS: f32 = 4.0;
const DEFAULT_EXTRA_SMALL_SPACING: f32 = 2.0;
const EXTRA_LARGE_SPACING_MULTIPLIER: f32 = 1.5;
const BUILTIN_THEME_JSONS: &[(&str, &str)] = &[
    ("dark", include_str!("builtin_themes/dark.json")),
    ("light", include_str!("builtin_themes/light.json")),
    (
        "catppuccin-latte",
        include_str!("builtin_themes/catppuccin-latte.json"),
    ),
    (
        "github-light",
        include_str!("builtin_themes/github-light.json"),
    ),
    (
        "solarized-light",
        include_str!("builtin_themes/solarized-light.json"),
    ),
    (
        "catppuccin-mocha",
        include_str!("builtin_themes/catppuccin-mocha.json"),
    ),
    (
        "tokyo-night",
        include_str!("builtin_themes/tokyo-night.json"),
    ),
    ("one-dark", include_str!("builtin_themes/one-dark.json")),
    (
        "gruvbox-dark",
        include_str!("builtin_themes/gruvbox-dark.json"),
    ),
    (
        "solarized-dark",
        include_str!("builtin_themes/solarized-dark.json"),
    ),
];

pub(crate) fn builtin_themes() -> Vec<Theme> {
    BUILTIN_THEME_JSONS
        .iter()
        .map(|(_, json)| Theme::from_json(json).expect("builtin theme JSON must be valid"))
        .collect()
}

fn builtin_theme(id: &str) -> Theme {
    let json = BUILTIN_THEME_JSONS
        .iter()
        .find_map(|(theme_id, json)| (*theme_id == id).then_some(*json))
        .expect("builtin theme ID must exist");
    Theme::from_json(json).expect("builtin theme JSON must be valid")
}

impl Theme {
    /// Create the built-in dark theme definition.
    pub fn dark() -> Self {
        builtin_theme("dark")
    }

    /// Create the built-in light theme definition.
    pub fn light() -> Self {
        builtin_theme("light")
    }

    /// Build a serializable theme from the runtime theme model.
    pub fn from_runtime(id: &str, name: &str, is_dark: bool, theme: &CodirigentTheme) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            is_dark,
            colors: ThemeColors {
                background: ThemeBackgroundColors {
                    app: hsla_to_hex(theme.background),
                    panel: hsla_to_hex(theme.panel_background),
                    header: hsla_to_hex(theme.header_background),
                    sidebar: hsla_to_hex(theme.sidebar_background),
                    icon_rail: hsla_to_hex(theme.icon_rail_background),
                    drawer: hsla_to_hex(theme.drawer_background),
                },
                foreground: ThemeForegroundColors {
                    primary: hsla_to_hex(theme.foreground),
                    secondary: hsla_to_hex(theme.text_secondary),
                    muted: hsla_to_hex(theme.muted),
                },
                border: ThemeBorderColors {
                    default: hsla_to_hex(theme.border),
                    focused: hsla_to_hex(theme.selected_ring),
                },
                interaction: ThemeInteractionColors {
                    hover: hsla_to_hex(theme.hover),
                    active: hsla_to_hex(theme.active),
                    selection: hsla_to_hex(theme.selection),
                },
                accent: ThemeAccentColors {
                    primary: hsla_to_hex(theme.primary),
                    secondary: hsla_to_hex(theme.secondary),
                    purple: hsla_to_hex(theme.purple),
                    orange: hsla_to_hex(theme.orange),
                    selected_ring: hsla_to_hex(theme.selected_ring),
                    broadcast: hsla_to_hex(theme.broadcast_accent),
                    ai_summary_background: hsla_to_hex(theme.ai_summary_background),
                    ai_summary_text: hsla_to_hex(theme.ai_summary_text),
                    input_required_background: hsla_to_hex(theme.input_required_background),
                    input_required_accent: hsla_to_hex(theme.input_required_accent),
                },
                status: ThemeStatusColors {
                    idle: hsla_to_hex(theme.session_idle),
                    working: hsla_to_hex(theme.session_working),
                    needs_attention: hsla_to_hex(theme.session_needs_attention),
                    response_ready: hsla_to_hex(theme.session_response_ready),
                    error: hsla_to_hex(theme.session_error),
                },
                priority: ThemePriorityColors {
                    high: hsla_to_hex(theme.priority_high),
                    medium: hsla_to_hex(theme.priority_medium),
                    low: hsla_to_hex(theme.priority_low),
                },
                session_groups: theme
                    .session_colors
                    .iter()
                    .copied()
                    .map(hsla_to_hex)
                    .collect(),
                terminal: TerminalColors {
                    background: rgba_to_hex(theme.terminal_background),
                    foreground: rgba_to_hex(theme.terminal_foreground),
                    cursor: rgba_to_hex(theme.terminal_cursor),
                    selection_background: rgba_to_hex(theme.terminal_selection_bg),
                    selection_foreground: rgba_to_hex(theme.terminal_selection_fg),
                    palette: ansi_to_palette(theme.ansi),
                },
            },
            typography: ThemeTypography {
                ui_font_family: DEFAULT_UI_FONT_FAMILY.to_string(),
                terminal_font_family: String::new(),
                base_font_size: theme.font_size_base,
                terminal_font_size: theme.terminal_font_size,
                line_height: theme.terminal_line_height,
            },
            spacing: ThemeSpacing {
                xs: DEFAULT_EXTRA_SMALL_SPACING,
                sm: theme.spacing_small,
                md: theme.spacing_base,
                lg: theme.spacing_large,
                xl: theme.spacing_large * EXTRA_LARGE_SPACING_MULTIPLIER,
                grid_gap: theme.grid_gap,
                border_radius: DEFAULT_BORDER_RADIUS,
            },
        }
    }
}

fn ansi_to_palette(ansi: AnsiColors) -> TerminalPalette {
    TerminalPalette {
        black: rgba_to_hex(ansi.colors[0]),
        red: rgba_to_hex(ansi.colors[1]),
        green: rgba_to_hex(ansi.colors[2]),
        yellow: rgba_to_hex(ansi.colors[3]),
        blue: rgba_to_hex(ansi.colors[4]),
        magenta: rgba_to_hex(ansi.colors[5]),
        cyan: rgba_to_hex(ansi.colors[6]),
        white: rgba_to_hex(ansi.colors[7]),
        bright_black: rgba_to_hex(ansi.colors[8]),
        bright_red: rgba_to_hex(ansi.colors[9]),
        bright_green: rgba_to_hex(ansi.colors[10]),
        bright_yellow: rgba_to_hex(ansi.colors[11]),
        bright_blue: rgba_to_hex(ansi.colors[12]),
        bright_magenta: rgba_to_hex(ansi.colors[13]),
        bright_cyan: rgba_to_hex(ansi.colors[14]),
        bright_white: rgba_to_hex(ansi.colors[15]),
    }
}

fn hsla_to_hex(color: Hsla) -> HexColor {
    rgba_to_hex(hsla_to_rgba(color))
}

fn hsla_to_rgba(color: Hsla) -> Rgba {
    let (r, g, b) = hsl_to_rgb(color.h, color.s, color.l);
    Rgba::new(r, g, b, float_alpha_to_u8(color.a))
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    if s.abs() < f32::EPSILON {
        let gray = float_channel_to_u8(l);
        return (gray, gray, gray);
    }

    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;

    let r = hue_to_rgb(p, q, h + (1.0 / 3.0));
    let g = hue_to_rgb(p, q, h);
    let b = hue_to_rgb(p, q, h - (1.0 / 3.0));
    (
        float_channel_to_u8(r),
        float_channel_to_u8(g),
        float_channel_to_u8(b),
    )
}

fn hue_to_rgb(p: f32, q: f32, mut t: f32) -> f32 {
    if t < 0.0 {
        t += 1.0;
    }
    if t > 1.0 {
        t -= 1.0;
    }
    if t < (1.0 / 6.0) {
        return p + (q - p) * 6.0 * t;
    }
    if t < 0.5 {
        return q;
    }
    if t < (2.0 / 3.0) {
        return p + (q - p) * ((2.0 / 3.0) - t) * 6.0;
    }
    p
}

fn float_channel_to_u8(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn float_alpha_to_u8(value: f32) -> u8 {
    float_channel_to_u8(value)
}

fn rgba_to_hex(color: Rgba) -> HexColor {
    if color.a == u8::MAX {
        format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b)
    } else {
        format!(
            "#{:02x}{:02x}{:02x}{:02x}",
            color.r, color.g, color.b, color.a
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_round_trip_from_runtime_shape() {
        let dark = Theme::dark();
        assert_eq!(dark.id, "dark");
        assert_eq!(dark.colors.background.app, "#050505");
        assert_eq!(dark.colors.terminal.cursor, "#6366f1");
        assert_eq!(dark.colors.terminal.selection_background, "#6366f14d");

        let light = Theme::light();
        assert_eq!(light.id, "light");
        assert_eq!(light.colors.background.app, "#f5f5f7");
        assert_eq!(light.colors.terminal.selection_background, "#4f46e533");
    }

    #[test]
    fn hsla_to_rgba_handles_gray_without_saturation() {
        let rgba = hsla_to_rgba(Hsla::new(0.0, 0.0, 0.5, 1.0));
        assert_eq!(rgba, Rgba::rgb(128, 128, 128));
    }

    #[test]
    fn builtin_theme_registry_loads_all_checked_in_themes() {
        let ids: Vec<String> = builtin_themes().into_iter().map(|theme| theme.id).collect();
        assert!(ids.contains(&"dark".to_string()));
        assert!(ids.contains(&"light".to_string()));
        assert!(ids.contains(&"tokyo-night".to_string()));
        assert!(ids.contains(&"solarized-dark".to_string()));
    }
}
