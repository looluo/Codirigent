//! Settings page state management.
//!
//! Contains the `SettingsPage` struct that holds the working copy of
//! user and project settings, tracks the active category, and manages
//! dirty detection and reset.

use crate::theme::{CodirigentTheme, Rgba};
use crate::theme_config::Theme;
use codirigent_core::config::{ProjectConfig, TerminalThemeOverrides, UserSettings};

/// Editable terminal style fields exposed in the settings UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalStyleField {
    /// Terminal background color.
    Background,
    /// Default terminal foreground/text color.
    Foreground,
    /// Terminal cursor color.
    Cursor,
    /// Terminal selection background color.
    SelectionBackground,
    /// Terminal selection foreground color.
    SelectionForeground,
    /// ANSI black palette slot.
    Black,
    /// ANSI red palette slot.
    Red,
    /// ANSI green palette slot.
    Green,
    /// ANSI yellow palette slot.
    Yellow,
    /// ANSI blue palette slot.
    Blue,
    /// ANSI magenta palette slot.
    Magenta,
    /// ANSI cyan palette slot.
    Cyan,
    /// ANSI white palette slot.
    White,
    /// ANSI bright black palette slot.
    BrightBlack,
    /// ANSI bright red palette slot.
    BrightRed,
    /// ANSI bright green palette slot.
    BrightGreen,
    /// ANSI bright yellow palette slot.
    BrightYellow,
    /// ANSI bright blue palette slot.
    BrightBlue,
    /// ANSI bright magenta palette slot.
    BrightMagenta,
    /// ANSI bright cyan palette slot.
    BrightCyan,
    /// ANSI bright white palette slot.
    BrightWhite,
}

impl TerminalStyleField {
    /// Primary terminal surface colors.
    pub const BASE: [Self; 5] = [
        Self::Background,
        Self::Foreground,
        Self::Cursor,
        Self::SelectionBackground,
        Self::SelectionForeground,
    ];

    /// ANSI 16-color palette overrides.
    pub const ANSI: [Self; 16] = [
        Self::Black,
        Self::Red,
        Self::Green,
        Self::Yellow,
        Self::Blue,
        Self::Magenta,
        Self::Cyan,
        Self::White,
        Self::BrightBlack,
        Self::BrightRed,
        Self::BrightGreen,
        Self::BrightYellow,
        Self::BrightBlue,
        Self::BrightMagenta,
        Self::BrightCyan,
        Self::BrightWhite,
    ];

    /// All terminal style fields in stable display order.
    pub const ALL: [Self; 21] = [
        Self::Background,
        Self::Foreground,
        Self::Cursor,
        Self::SelectionBackground,
        Self::SelectionForeground,
        Self::Black,
        Self::Red,
        Self::Green,
        Self::Yellow,
        Self::Blue,
        Self::Magenta,
        Self::Cyan,
        Self::White,
        Self::BrightBlack,
        Self::BrightRed,
        Self::BrightGreen,
        Self::BrightYellow,
        Self::BrightBlue,
        Self::BrightMagenta,
        Self::BrightCyan,
        Self::BrightWhite,
    ];

    /// Stable field identifier for focus state and DOM IDs.
    pub fn id(self) -> &'static str {
        match self {
            Self::Background => "background",
            Self::Foreground => "foreground",
            Self::Cursor => "cursor",
            Self::SelectionBackground => "selection_background",
            Self::SelectionForeground => "selection_foreground",
            Self::Black => "palette_black",
            Self::Red => "palette_red",
            Self::Green => "palette_green",
            Self::Yellow => "palette_yellow",
            Self::Blue => "palette_blue",
            Self::Magenta => "palette_magenta",
            Self::Cyan => "palette_cyan",
            Self::White => "palette_white",
            Self::BrightBlack => "palette_bright_black",
            Self::BrightRed => "palette_bright_red",
            Self::BrightGreen => "palette_bright_green",
            Self::BrightYellow => "palette_bright_yellow",
            Self::BrightBlue => "palette_bright_blue",
            Self::BrightMagenta => "palette_bright_magenta",
            Self::BrightCyan => "palette_bright_cyan",
            Self::BrightWhite => "palette_bright_white",
        }
    }

    pub(crate) fn get(self, overrides: &TerminalThemeOverrides) -> &str {
        match self {
            Self::Background => &overrides.background,
            Self::Foreground => &overrides.foreground,
            Self::Cursor => &overrides.cursor,
            Self::SelectionBackground => &overrides.selection_background,
            Self::SelectionForeground => &overrides.selection_foreground,
            Self::Black => &overrides.palette.black,
            Self::Red => &overrides.palette.red,
            Self::Green => &overrides.palette.green,
            Self::Yellow => &overrides.palette.yellow,
            Self::Blue => &overrides.palette.blue,
            Self::Magenta => &overrides.palette.magenta,
            Self::Cyan => &overrides.palette.cyan,
            Self::White => &overrides.palette.white,
            Self::BrightBlack => &overrides.palette.bright_black,
            Self::BrightRed => &overrides.palette.bright_red,
            Self::BrightGreen => &overrides.palette.bright_green,
            Self::BrightYellow => &overrides.palette.bright_yellow,
            Self::BrightBlue => &overrides.palette.bright_blue,
            Self::BrightMagenta => &overrides.palette.bright_magenta,
            Self::BrightCyan => &overrides.palette.bright_cyan,
            Self::BrightWhite => &overrides.palette.bright_white,
        }
    }

    pub(crate) fn get_mut(self, overrides: &mut TerminalThemeOverrides) -> &mut String {
        match self {
            Self::Background => &mut overrides.background,
            Self::Foreground => &mut overrides.foreground,
            Self::Cursor => &mut overrides.cursor,
            Self::SelectionBackground => &mut overrides.selection_background,
            Self::SelectionForeground => &mut overrides.selection_foreground,
            Self::Black => &mut overrides.palette.black,
            Self::Red => &mut overrides.palette.red,
            Self::Green => &mut overrides.palette.green,
            Self::Yellow => &mut overrides.palette.yellow,
            Self::Blue => &mut overrides.palette.blue,
            Self::Magenta => &mut overrides.palette.magenta,
            Self::Cyan => &mut overrides.palette.cyan,
            Self::White => &mut overrides.palette.white,
            Self::BrightBlack => &mut overrides.palette.bright_black,
            Self::BrightRed => &mut overrides.palette.bright_red,
            Self::BrightGreen => &mut overrides.palette.bright_green,
            Self::BrightYellow => &mut overrides.palette.bright_yellow,
            Self::BrightBlue => &mut overrides.palette.bright_blue,
            Self::BrightMagenta => &mut overrides.palette.bright_magenta,
            Self::BrightCyan => &mut overrides.palette.bright_cyan,
            Self::BrightWhite => &mut overrides.palette.bright_white,
        }
    }

    /// Read the effective runtime theme color for this field.
    pub fn theme_color(self, theme: &CodirigentTheme) -> Rgba {
        match self {
            Self::Background => theme.terminal_background,
            Self::Foreground => theme.terminal_foreground,
            Self::Cursor => theme.terminal_cursor,
            Self::SelectionBackground => theme.terminal_selection_bg,
            Self::SelectionForeground => theme.terminal_selection_fg,
            Self::Black => theme.ansi.colors[0],
            Self::Red => theme.ansi.colors[1],
            Self::Green => theme.ansi.colors[2],
            Self::Yellow => theme.ansi.colors[3],
            Self::Blue => theme.ansi.colors[4],
            Self::Magenta => theme.ansi.colors[5],
            Self::Cyan => theme.ansi.colors[6],
            Self::White => theme.ansi.colors[7],
            Self::BrightBlack => theme.ansi.colors[8],
            Self::BrightRed => theme.ansi.colors[9],
            Self::BrightGreen => theme.ansi.colors[10],
            Self::BrightYellow => theme.ansi.colors[11],
            Self::BrightBlue => theme.ansi.colors[12],
            Self::BrightMagenta => theme.ansi.colors[13],
            Self::BrightCyan => theme.ansi.colors[14],
            Self::BrightWhite => theme.ansi.colors[15],
        }
    }

    /// Apply this field to the serializable theme config.
    pub fn set_theme_config_value(self, theme: &mut Theme, value: String) {
        match self {
            Self::Background => theme.colors.terminal.background = value,
            Self::Foreground => theme.colors.terminal.foreground = value,
            Self::Cursor => theme.colors.terminal.cursor = value,
            Self::SelectionBackground => theme.colors.terminal.selection_background = value,
            Self::SelectionForeground => theme.colors.terminal.selection_foreground = value,
            Self::Black => theme.colors.terminal.palette.black = value,
            Self::Red => theme.colors.terminal.palette.red = value,
            Self::Green => theme.colors.terminal.palette.green = value,
            Self::Yellow => theme.colors.terminal.palette.yellow = value,
            Self::Blue => theme.colors.terminal.palette.blue = value,
            Self::Magenta => theme.colors.terminal.palette.magenta = value,
            Self::Cyan => theme.colors.terminal.palette.cyan = value,
            Self::White => theme.colors.terminal.palette.white = value,
            Self::BrightBlack => theme.colors.terminal.palette.bright_black = value,
            Self::BrightRed => theme.colors.terminal.palette.bright_red = value,
            Self::BrightGreen => theme.colors.terminal.palette.bright_green = value,
            Self::BrightYellow => theme.colors.terminal.palette.bright_yellow = value,
            Self::BrightBlue => theme.colors.terminal.palette.bright_blue = value,
            Self::BrightMagenta => theme.colors.terminal.palette.bright_magenta = value,
            Self::BrightCyan => theme.colors.terminal.palette.bright_cyan = value,
            Self::BrightWhite => theme.colors.terminal.palette.bright_white = value,
        }
    }
}

/// Settings category for the sidebar navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsCategory {
    /// General preferences (editor, shell, startup).
    General,
    /// Appearance (theme, UI font size, grid gap).
    Appearance,
    /// Terminal (font, cursor, line height).
    Terminal,
    /// Keyboard shortcuts table.
    KeyboardShortcuts,
    /// Session limits and defaults (project-level).
    Sessions,
    /// Advanced (scheduler, verification, git) (project-level).
    Advanced,
}

impl SettingsCategory {
    /// Visible categories in display order.
    pub const ALL: [SettingsCategory; 4] = [
        SettingsCategory::General,
        SettingsCategory::Appearance,
        SettingsCategory::Terminal,
        SettingsCategory::KeyboardShortcuts,
    ];

    /// Returns whether this category is currently exposed in the UI.
    pub fn is_visible(self) -> bool {
        matches!(
            self,
            SettingsCategory::General
                | SettingsCategory::Appearance
                | SettingsCategory::Terminal
                | SettingsCategory::KeyboardShortcuts
        )
    }

    /// Human-readable label for the category.
    pub fn label(&self) -> &'static str {
        match self {
            SettingsCategory::General => "General",
            SettingsCategory::Appearance => "Appearance",
            SettingsCategory::Terminal => "Terminal",
            SettingsCategory::KeyboardShortcuts => "Keyboard Shortcuts",
            SettingsCategory::Sessions => "Sessions",
            SettingsCategory::Advanced => "Advanced",
        }
    }
}

/// Settings page state.
///
/// Holds working copies of user and project settings that are edited
/// in-place. Changes are applied immediately to the running app and
/// persisted via debounced save.
pub struct SettingsPage {
    /// Currently selected category.
    active_category: SettingsCategory,
    /// Working copy of user settings (edited live).
    pub user_settings: UserSettings,
    /// Working copy of project config (edited live).
    pub project_config: ProjectConfig,
    /// Original user settings snapshot for dirty detection and reset.
    original_user: UserSettings,
    /// Original project config snapshot for dirty detection and reset.
    original_project: ProjectConfig,
    /// Which keybinding is currently being recorded (action name).
    pub recording_shortcut: Option<String>,
    /// Which shortcut row currently has keyboard focus (action name).
    pub focused_shortcut_row: Option<String>,
    /// Which terminal style field currently has keyboard focus.
    pub focused_terminal_style_field: Option<TerminalStyleField>,
    /// Which dropdown is currently open (by ID string).
    pub open_dropdown: Option<String>,
    /// Click position (window coordinates) where the dropdown was triggered.
    pub dropdown_click_pos: (f32, f32),
    /// Whether a debounced save for user settings is pending.
    pub user_save_pending: bool,
    /// Whether a debounced save for project config is pending.
    pub project_save_pending: bool,
    /// Editors detected on the system PATH.
    pub detected_editors: Vec<String>,
    /// Shells detected on the system.
    pub detected_shells: Vec<String>,
    /// Monospace fonts detected on the system.
    pub detected_fonts: Vec<String>,
    /// Theme colors captured when the terminal editor was opened/reset.
    pub terminal_style_base: TerminalThemeOverrides,
    /// In-progress terminal style editor draft values.
    pub terminal_style_draft: TerminalThemeOverrides,
    /// Pre-sorted list of keybinding action names for the Keyboard Shortcuts panel.
    /// Rebuilt whenever `user_settings.keybindings` changes.
    pub sorted_shortcut_keys: Vec<String>,
}

impl SettingsPage {
    /// Create a new settings page with the given configuration.
    pub fn new(
        user_settings: UserSettings,
        project_config: ProjectConfig,
        detected_editors: Vec<String>,
        detected_shells: Vec<String>,
        detected_fonts: Vec<String>,
        terminal_style_values: TerminalThemeOverrides,
    ) -> Self {
        let mut sorted_shortcut_keys: Vec<String> =
            user_settings.keybindings.keys().cloned().collect();
        sorted_shortcut_keys.sort();
        let terminal_style_base = terminal_style_values.clone();
        let terminal_style_draft = terminal_style_values;
        Self {
            active_category: SettingsCategory::General,
            original_user: user_settings.clone(),
            original_project: project_config.clone(),
            user_settings,
            project_config,
            recording_shortcut: None,
            focused_shortcut_row: None,
            focused_terminal_style_field: None,
            open_dropdown: None,
            dropdown_click_pos: (0.0, 0.0),
            user_save_pending: false,
            project_save_pending: false,
            detected_editors,
            detected_shells,
            detected_fonts,
            terminal_style_base,
            terminal_style_draft,
            sorted_shortcut_keys,
        }
    }

    /// Rebuild the sorted shortcut key cache after `user_settings.keybindings` changes.
    pub fn refresh_sorted_shortcut_keys(&mut self) {
        self.sorted_shortcut_keys = self.user_settings.keybindings.keys().cloned().collect();
        self.sorted_shortcut_keys.sort();
    }

    /// Get the active category.
    pub fn active_category(&self) -> SettingsCategory {
        if self.active_category.is_visible() {
            self.active_category
        } else {
            SettingsCategory::General
        }
    }

    /// Set the active category.
    pub fn set_category(&mut self, category: SettingsCategory) {
        self.active_category = category;
    }

    /// Check if user settings have been modified.
    pub fn is_user_dirty(&self) -> bool {
        self.user_settings != self.original_user
    }

    /// Check if project config has been modified.
    pub fn is_project_dirty(&self) -> bool {
        self.project_config != self.original_project
    }

    /// Reset user settings for the current category to defaults.
    pub fn reset_user_category(&mut self) {
        let defaults = UserSettings::default();
        match self.active_category {
            SettingsCategory::General => {
                self.user_settings.general = defaults.general;
                self.user_settings.notifications = defaults.notifications;
            }
            SettingsCategory::Appearance => {
                self.user_settings.appearance = defaults.appearance;
            }
            SettingsCategory::Terminal => {
                self.user_settings.terminal = defaults.terminal;
                self.terminal_style_draft = self.terminal_style_base.clone();
                self.focused_terminal_style_field = None;
            }
            SettingsCategory::KeyboardShortcuts => {
                self.user_settings.keybindings = defaults.keybindings;
                self.refresh_sorted_shortcut_keys();
            }
            _ => {}
        }
        self.user_save_pending = true;
    }

    /// Reset project config for the current category to defaults.
    pub fn reset_project_category(&mut self) {
        let defaults = ProjectConfig::default();
        match self.active_category {
            SettingsCategory::Sessions => {
                self.project_config.sessions = defaults.sessions;
            }
            SettingsCategory::Advanced => {
                self.project_config.scheduler = defaults.scheduler;
                self.project_config.verification = defaults.verification;
                self.project_config.git = defaults.git;
            }
            _ => {}
        }
        self.project_save_pending = true;
    }

    /// Mark user settings as saved (snapshot updated).
    pub fn mark_user_saved(&mut self) {
        self.original_user = self.user_settings.clone();
        self.user_save_pending = false;
    }

    /// Mark project config as saved (snapshot updated).
    pub fn mark_project_saved(&mut self) {
        self.original_project = self.project_config.clone();
        self.project_save_pending = false;
    }

    /// Return the current draft value for a terminal style field.
    pub fn terminal_style_draft_value(&self, field: TerminalStyleField) -> &str {
        field.get(&self.terminal_style_draft)
    }

    /// Return the base theme value for a terminal style field.
    pub fn terminal_style_base_value(&self, field: TerminalStyleField) -> &str {
        field.get(&self.terminal_style_base)
    }

    /// Update the draft value for a terminal style field.
    pub fn set_terminal_style_draft_value(&mut self, field: TerminalStyleField, value: String) {
        *field.get_mut(&mut self.terminal_style_draft) = value;
    }

    /// Reset one field back to the base theme value captured for the editor.
    pub fn reset_terminal_style_field_to_base(&mut self, field: TerminalStyleField) {
        let value = field.get(&self.terminal_style_base).to_string();
        *field.get_mut(&mut self.terminal_style_draft) = value;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settings_category_all() {
        assert_eq!(SettingsCategory::ALL.len(), 4);
        assert_eq!(SettingsCategory::ALL[0], SettingsCategory::General);
        assert_eq!(
            SettingsCategory::ALL[3],
            SettingsCategory::KeyboardShortcuts
        );
    }

    #[test]
    fn test_settings_category_label() {
        assert_eq!(SettingsCategory::General.label(), "General");
        assert_eq!(SettingsCategory::Appearance.label(), "Appearance");
        assert_eq!(SettingsCategory::Terminal.label(), "Terminal");
        assert_eq!(
            SettingsCategory::KeyboardShortcuts.label(),
            "Keyboard Shortcuts"
        );
        assert_eq!(SettingsCategory::Sessions.label(), "Sessions");
        assert_eq!(SettingsCategory::Advanced.label(), "Advanced");
    }

    #[test]
    fn test_settings_page_focused_shortcut_row_default_none() {
        let page = SettingsPage::new(
            UserSettings::default(),
            ProjectConfig::default(),
            vec![],
            vec![],
            vec![],
            TerminalThemeOverrides::default(),
        );
        assert!(page.focused_shortcut_row.is_none());
        assert!(page.focused_terminal_style_field.is_none());
    }

    #[test]
    fn test_settings_page_new() {
        let page = SettingsPage::new(
            UserSettings::default(),
            ProjectConfig::default(),
            vec!["code".to_string()],
            vec!["bash".to_string(), "zsh".to_string()],
            vec!["Menlo".to_string(), "Courier New".to_string()],
            TerminalThemeOverrides::default(),
        );
        assert_eq!(page.active_category(), SettingsCategory::General);
        assert!(!page.is_user_dirty());
        assert!(!page.is_project_dirty());
        assert!(page.recording_shortcut.is_none());
        assert!(page.terminal_style_draft.background.is_empty());
    }

    #[test]
    fn test_terminal_style_draft_tracks_editor_value() {
        let mut page = SettingsPage::new(
            UserSettings::default(),
            ProjectConfig::default(),
            vec![],
            vec![],
            vec![],
            TerminalThemeOverrides::default(),
        );

        page.set_terminal_style_draft_value(TerminalStyleField::Background, "#123456".to_string());

        assert_eq!(
            page.terminal_style_draft_value(TerminalStyleField::Background),
            "#123456"
        );
    }

    #[test]
    fn test_reset_terminal_style_field_to_base() {
        let base = TerminalThemeOverrides {
            background: "#111111".to_string(),
            ..TerminalThemeOverrides::default()
        };
        let mut page = SettingsPage::new(
            UserSettings::default(),
            ProjectConfig::default(),
            vec![],
            vec![],
            vec![],
            base,
        );

        page.set_terminal_style_draft_value(TerminalStyleField::Background, "#123456".to_string());
        page.reset_terminal_style_field_to_base(TerminalStyleField::Background);

        assert_eq!(
            page.terminal_style_draft_value(TerminalStyleField::Background),
            "#111111"
        );
    }

    #[test]
    fn test_set_category() {
        let mut page = SettingsPage::new(
            UserSettings::default(),
            ProjectConfig::default(),
            vec!["code".to_string()],
            vec!["bash".to_string(), "zsh".to_string()],
            vec!["Menlo".to_string(), "Courier New".to_string()],
            TerminalThemeOverrides::default(),
        );
        page.set_category(SettingsCategory::Terminal);
        assert_eq!(page.active_category(), SettingsCategory::Terminal);
    }

    #[test]
    fn test_hidden_category_falls_back_to_general() {
        let mut page = SettingsPage::new(
            UserSettings::default(),
            ProjectConfig::default(),
            vec!["code".to_string()],
            vec!["bash".to_string(), "zsh".to_string()],
            vec!["Menlo".to_string(), "Courier New".to_string()],
            TerminalThemeOverrides::default(),
        );
        page.set_category(SettingsCategory::Sessions);
        assert_eq!(page.active_category(), SettingsCategory::General);

        page.set_category(SettingsCategory::Advanced);
        assert_eq!(page.active_category(), SettingsCategory::General);
    }

    #[test]
    fn test_dirty_detection() {
        let mut page = SettingsPage::new(
            UserSettings::default(),
            ProjectConfig::default(),
            vec!["code".to_string()],
            vec!["bash".to_string(), "zsh".to_string()],
            vec!["Menlo".to_string(), "Courier New".to_string()],
            TerminalThemeOverrides::default(),
        );
        assert!(!page.is_user_dirty());
        page.user_settings.general.editor_command = "vim".to_string();
        assert!(page.is_user_dirty());
    }

    #[test]
    fn test_project_dirty_detection() {
        let mut page = SettingsPage::new(
            UserSettings::default(),
            ProjectConfig::default(),
            vec!["code".to_string()],
            vec!["bash".to_string(), "zsh".to_string()],
            vec!["Menlo".to_string(), "Courier New".to_string()],
            TerminalThemeOverrides::default(),
        );
        assert!(!page.is_project_dirty());
        page.project_config.sessions.max_concurrent = 16;
        assert!(page.is_project_dirty());
    }

    #[test]
    fn test_reset_user_category() {
        let mut page = SettingsPage::new(
            UserSettings::default(),
            ProjectConfig::default(),
            vec!["code".to_string()],
            vec!["bash".to_string(), "zsh".to_string()],
            vec!["Menlo".to_string(), "Courier New".to_string()],
            TerminalThemeOverrides::default(),
        );
        page.user_settings.general.editor_command = "vim".to_string();
        assert!(page.is_user_dirty());
        page.set_category(SettingsCategory::General);
        page.reset_user_category();
        assert_eq!(page.user_settings.general.editor_command, "code");
        assert!(page.user_save_pending);
    }

    #[test]
    fn test_reset_project_category() {
        let mut page = SettingsPage::new(
            UserSettings::default(),
            ProjectConfig::default(),
            vec!["code".to_string()],
            vec!["bash".to_string(), "zsh".to_string()],
            vec!["Menlo".to_string(), "Courier New".to_string()],
            TerminalThemeOverrides::default(),
        );
        page.project_config.sessions.max_concurrent = 16;
        page.set_category(SettingsCategory::Sessions);
        page.reset_project_category();
        assert_eq!(page.project_config.sessions.max_concurrent, 9);
        assert!(page.project_save_pending);
    }

    #[test]
    fn test_mark_saved() {
        let mut page = SettingsPage::new(
            UserSettings::default(),
            ProjectConfig::default(),
            vec!["code".to_string()],
            vec!["bash".to_string(), "zsh".to_string()],
            vec!["Menlo".to_string(), "Courier New".to_string()],
            TerminalThemeOverrides::default(),
        );
        page.user_settings.general.editor_command = "vim".to_string();
        page.user_save_pending = true;
        page.mark_user_saved();
        assert!(!page.is_user_dirty());
        assert!(!page.user_save_pending);
    }

    #[test]
    fn test_reset_general_also_resets_notifications() {
        let mut page = SettingsPage::new(
            UserSettings::default(),
            ProjectConfig::default(),
            vec![],
            vec![],
            vec![],
            TerminalThemeOverrides::default(),
        );
        // Mutate notification fields
        page.user_settings.notifications.desktop = false;
        page.user_settings.notifications.cooldown_seconds = 120;
        assert!(page.is_user_dirty());

        page.set_category(SettingsCategory::General);
        page.reset_user_category();

        assert!(page.user_settings.notifications.desktop);
        assert_eq!(page.user_settings.notifications.cooldown_seconds, 30);
        assert!(page.user_save_pending);
    }
}
