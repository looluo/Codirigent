//! Theme manager for Codirigent.
//!
//! This module provides a manager for handling multiple themes, including:
//! - Built-in dark and light themes
//! - Loading custom themes from files
//! - Switching between themes at runtime
//!
//! # Example
//!
//! ```
//! use codirigent_ui::theme_manager::ThemeManager;
//!
//! let mut manager = ThemeManager::with_defaults();
//! assert_eq!(manager.active().id, "dark");
//!
//! manager.set_active("light");
//! assert_eq!(manager.active().id, "light");
//! ```

use crate::theme::CodirigentTheme;
use crate::theme_config::{builtin_themes, Theme};
use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::warn;

/// Built-in theme ID used as the final fallback.
pub const DEFAULT_THEME_ID: &str = "dark";
const BUILTIN_THEME_IDS: &[&str] = &[
    "dark",
    "light",
    "catppuccin-latte",
    "github-light",
    "solarized-light",
    "catppuccin-mocha",
    "tokyo-night",
    "one-dark",
    "gruvbox-dark",
    "solarized-dark",
];
/// Directory name under the user config root that stores custom theme files.
pub const CUSTOM_THEME_DIRECTORY_NAME: &str = "themes";

/// Result of resolving a requested theme into a runtime theme.
#[derive(Debug, Clone)]
pub struct RuntimeThemeResolution {
    /// Theme ID originally requested by the caller.
    pub requested_id: String,
    /// Theme ID that was actually resolved.
    pub resolved_id: String,
    /// Runtime theme used by the UI.
    pub theme: CodirigentTheme,
    /// Whether the resolution required a fallback.
    pub used_fallback: bool,
}

/// Manages available themes.
///
/// Provides access to built-in and custom themes, with the ability
/// to switch the active theme at runtime.
#[derive(Debug, Clone)]
pub struct ThemeManager {
    /// All available themes, keyed by ID.
    themes: HashMap<String, Theme>,
    /// Currently active theme ID.
    active_theme: String,
}

impl ThemeManager {
    /// Create a new theme manager with the dark theme as default.
    ///
    /// For a manager with both dark and light themes, use `with_defaults()`.
    pub fn new() -> Self {
        let dark = Theme::dark();
        let mut themes = HashMap::new();
        let active_id = dark.id.clone();
        themes.insert(dark.id.clone(), dark);

        Self {
            themes,
            active_theme: active_id,
        }
    }

    /// Create with built-in themes.
    ///
    /// Includes the default built-in themes.
    ///
    /// # Example
    ///
    /// ```
    /// use codirigent_ui::theme_manager::ThemeManager;
    ///
    /// let manager = ThemeManager::with_defaults();
    /// assert!(manager.list().len() >= 2);
    /// ```
    pub fn with_defaults() -> Self {
        let mut themes = HashMap::new();
        for theme in builtin_themes() {
            themes.insert(theme.id.clone(), theme);
        }

        Self {
            themes,
            active_theme: DEFAULT_THEME_ID.to_string(),
        }
    }

    /// Return the custom theme directory for a given user config root.
    pub fn custom_theme_dir(user_config_dir: &Path) -> PathBuf {
        user_config_dir.join(CUSTOM_THEME_DIRECTORY_NAME)
    }

    /// Create with built-in themes plus any user-installed custom themes.
    ///
    /// Invalid theme files are logged and ignored. An unreadable themes
    /// directory is also logged, but does not prevent the manager from
    /// returning built-in themes.
    pub fn with_user_themes(user_config_dir: &Path) -> Self {
        let mut manager = Self::with_defaults();
        let theme_dir = Self::custom_theme_dir(user_config_dir);
        if let Err(error) = manager.load_custom_themes(&theme_dir) {
            warn!(
                path = ?theme_dir,
                error = %error,
                "Failed to scan custom theme directory"
            );
        }
        manager
    }

    /// Load custom themes from a directory.
    ///
    /// Scans the directory for `.json` files and attempts to load each
    /// as a theme. Invalid files are logged but don't cause errors.
    ///
    /// # Arguments
    ///
    /// * `dir` - Directory containing theme JSON files
    ///
    /// # Returns
    ///
    /// Ok if the directory was scanned (even if some files failed to load).
    ///
    /// # Example
    ///
    /// ```ignore
    /// use codirigent_ui::theme_manager::ThemeManager;
    /// use std::path::Path;
    ///
    /// let mut manager = ThemeManager::with_defaults();
    /// manager.load_custom_themes(Path::new("/path/to/themes")).ok();
    /// ```
    pub fn load_custom_themes(&mut self, dir: &Path) -> Result<usize> {
        if !dir.exists() {
            return Ok(0);
        }

        let mut loaded = 0;
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "json") {
                match std::fs::read_to_string(&path) {
                    Ok(content) => match Theme::from_json(&content) {
                        Ok(theme) => {
                            tracing::info!(id = %theme.id, "Loaded custom theme");
                            self.themes.insert(theme.id.clone(), theme);
                            loaded += 1;
                        }
                        Err(e) => {
                            tracing::warn!(path = ?path, error = %e, "Failed to parse theme");
                        }
                    },
                    Err(e) => {
                        tracing::warn!(path = ?path, error = %e, "Failed to read theme file");
                    }
                }
            }
        }
        Ok(loaded)
    }

    /// Load a single theme from a file.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the theme JSON file
    ///
    /// # Returns
    ///
    /// The loaded theme, or an error if loading failed.
    pub fn load_theme_file(&mut self, path: &Path) -> Result<&Theme> {
        let content = std::fs::read_to_string(path)?;
        let theme = Theme::from_json(&content)?;
        let id = theme.id.clone();
        self.themes.insert(id.clone(), theme);
        self.themes
            .get(&id)
            .ok_or_else(|| anyhow::anyhow!("Theme '{}' not found after insertion", id))
    }

    /// Load a theme from a JSON string.
    ///
    /// # Arguments
    ///
    /// * `json` - JSON string containing theme definition
    ///
    /// # Returns
    ///
    /// The loaded theme, or an error if parsing failed.
    pub fn load_theme_json(&mut self, json: &str) -> Result<&Theme> {
        let theme = Theme::from_json(json)?;
        let id = theme.id.clone();
        self.themes.insert(id.clone(), theme);
        self.themes
            .get(&id)
            .ok_or_else(|| anyhow::anyhow!("Theme '{}' not found after insertion", id))
    }

    /// Get the active theme.
    ///
    /// # Returns
    ///
    /// Reference to the currently active theme. Falls back to dark theme
    /// if the active theme ID is invalid.
    pub fn active(&self) -> &Theme {
        self.themes.get(&self.active_theme).unwrap_or_else(|| {
            self.themes
                .get(DEFAULT_THEME_ID)
                .expect("Dark theme must always exist")
        })
    }

    /// Get the ID of the active theme.
    pub fn active_id(&self) -> &str {
        &self.active_theme
    }

    /// Set the active theme by ID.
    ///
    /// # Arguments
    ///
    /// * `id` - Theme ID to activate
    ///
    /// # Returns
    ///
    /// `true` if the theme exists and was activated.
    ///
    /// # Example
    ///
    /// ```
    /// use codirigent_ui::theme_manager::ThemeManager;
    ///
    /// let mut manager = ThemeManager::with_defaults();
    /// assert!(manager.set_active("light"));
    /// assert_eq!(manager.active().id, "light");
    /// ```
    pub fn set_active(&mut self, id: &str) -> bool {
        if self.themes.contains_key(id) {
            self.active_theme = id.to_string();
            true
        } else {
            false
        }
    }

    /// Get a theme by ID.
    ///
    /// # Arguments
    ///
    /// * `id` - Theme ID to look up
    ///
    /// # Returns
    ///
    /// Reference to the theme, or None if not found.
    pub fn get(&self, id: &str) -> Option<&Theme> {
        self.themes.get(id)
    }

    /// List all available themes.
    ///
    /// # Returns
    ///
    /// Vector of references to all themes.
    pub fn list(&self) -> Vec<&Theme> {
        self.themes.values().collect()
    }

    /// List all theme IDs.
    ///
    /// # Returns
    ///
    /// Vector of theme ID strings.
    pub fn list_ids(&self) -> Vec<&str> {
        self.themes.keys().map(|s| s.as_str()).collect()
    }

    /// Get the number of available themes.
    pub fn len(&self) -> usize {
        self.themes.len()
    }

    /// Check if there are no themes.
    pub fn is_empty(&self) -> bool {
        self.themes.is_empty()
    }

    /// Add or update a theme.
    ///
    /// # Arguments
    ///
    /// * `theme` - Theme to add or update
    pub fn add_theme(&mut self, theme: Theme) {
        self.themes.insert(theme.id.clone(), theme);
    }

    /// Remove a custom theme.
    ///
    /// Built-in themes cannot be removed.
    ///
    /// # Arguments
    ///
    /// * `id` - Theme ID to remove
    ///
    /// # Returns
    ///
    /// `true` if the theme was removed. Returns `false` for built-in themes
    /// or non-existent themes.
    ///
    /// # Example
    ///
    /// ```
    /// use codirigent_ui::theme_manager::ThemeManager;
    ///
    /// let mut manager = ThemeManager::with_defaults();
    /// assert!(!manager.remove_theme("dark")); // Cannot remove built-in
    /// ```
    pub fn remove_theme(&mut self, id: &str) -> bool {
        // Cannot remove built-in themes
        if BUILTIN_THEME_IDS.contains(&id) {
            return false;
        }
        let removed = self.themes.remove(id).is_some();

        // If we removed the active theme, fall back to dark
        if removed && self.active_theme == id {
            self.active_theme = DEFAULT_THEME_ID.to_string();
        }

        removed
    }

    /// Toggle between dark and light themes.
    ///
    /// If current theme is dark, switches to light. Otherwise switches to dark.
    pub fn toggle_dark_light(&mut self) {
        if self.active().is_dark {
            self.set_active("light");
        } else {
            self.set_active(DEFAULT_THEME_ID);
        }
    }

    /// Check if the active theme is dark.
    pub fn is_dark(&self) -> bool {
        self.active().is_dark
    }

    /// Get all dark themes.
    pub fn dark_themes(&self) -> Vec<&Theme> {
        self.themes.values().filter(|t| t.is_dark).collect()
    }

    /// Get all light themes.
    pub fn light_themes(&self) -> Vec<&Theme> {
        self.themes.values().filter(|t| !t.is_dark).collect()
    }

    /// Returns whether a theme ID refers to a built-in theme.
    pub fn is_builtin(&self, id: &str) -> bool {
        BUILTIN_THEME_IDS.contains(&id)
    }

    /// Convert a registered theme into the runtime theme model.
    pub fn runtime_theme(&self, id: &str) -> Result<CodirigentTheme> {
        let theme = self
            .get(id)
            .ok_or_else(|| anyhow::anyhow!("Theme '{id}' not found"))?;
        CodirigentTheme::try_from(theme).map_err(anyhow::Error::from)
    }

    /// Resolve a requested theme ID into a runtime theme, falling back to the
    /// built-in dark theme when the requested theme is missing or invalid.
    pub fn resolve_runtime_theme(&self, requested_id: &str) -> RuntimeThemeResolution {
        if let Ok(theme) = self.runtime_theme(requested_id) {
            return RuntimeThemeResolution {
                requested_id: requested_id.to_string(),
                resolved_id: requested_id.to_string(),
                theme,
                used_fallback: false,
            };
        }

        let fallback_theme = self
            .runtime_theme(DEFAULT_THEME_ID)
            .unwrap_or_else(|_| CodirigentTheme::dark());

        RuntimeThemeResolution {
            requested_id: requested_id.to_string(),
            resolved_id: DEFAULT_THEME_ID.to_string(),
            theme: fallback_theme,
            used_fallback: requested_id != DEFAULT_THEME_ID,
        }
    }
}

impl Default for ThemeManager {
    fn default() -> Self {
        Self::with_defaults()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme_config::TerminalPalette;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_new() {
        let manager = ThemeManager::new();
        assert_eq!(manager.len(), 1);
        assert!(manager.get("dark").is_some());
    }

    #[test]
    fn test_with_defaults() {
        let manager = ThemeManager::with_defaults();
        assert_eq!(manager.len(), 10);
        assert!(manager.get("dark").is_some());
        assert!(manager.get("light").is_some());
        assert!(manager.get("catppuccin-latte").is_some());
        assert!(manager.get("github-light").is_some());
        assert!(manager.get("solarized-light").is_some());
        assert!(manager.get("catppuccin-mocha").is_some());
        assert!(manager.get("tokyo-night").is_some());
    }

    #[test]
    fn test_default_trait() {
        let manager = ThemeManager::default();
        assert_eq!(manager.len(), 10);
    }

    #[test]
    fn test_custom_theme_dir_appends_themes_directory() {
        let config_dir = Path::new("/tmp/codirigent");

        assert_eq!(
            ThemeManager::custom_theme_dir(config_dir),
            config_dir.join(CUSTOM_THEME_DIRECTORY_NAME)
        );
    }

    #[test]
    fn test_active() {
        let manager = ThemeManager::with_defaults();
        assert_eq!(manager.active().id, "dark");
    }

    #[test]
    fn test_active_id() {
        let manager = ThemeManager::with_defaults();
        assert_eq!(manager.active_id(), "dark");
    }

    #[test]
    fn test_set_active() {
        let mut manager = ThemeManager::with_defaults();
        assert!(manager.set_active("light"));
        assert_eq!(manager.active().id, "light");
    }

    #[test]
    fn test_set_active_nonexistent() {
        let mut manager = ThemeManager::with_defaults();
        assert!(!manager.set_active("nonexistent"));
        assert_eq!(manager.active().id, "dark"); // Unchanged
    }

    #[test]
    fn test_get() {
        let manager = ThemeManager::with_defaults();
        let dark = manager.get("dark").unwrap();
        assert_eq!(dark.id, "dark");
        assert!(dark.is_dark);
    }

    #[test]
    fn test_get_nonexistent() {
        let manager = ThemeManager::with_defaults();
        assert!(manager.get("nonexistent").is_none());
    }

    #[test]
    fn test_list() {
        let manager = ThemeManager::with_defaults();
        let themes = manager.list();
        assert_eq!(themes.len(), 10);
    }

    #[test]
    fn test_list_ids() {
        let manager = ThemeManager::with_defaults();
        let ids = manager.list_ids();
        assert_eq!(ids.len(), 10);
        assert!(ids.contains(&"dark"));
        assert!(ids.contains(&"light"));
        assert!(ids.contains(&"catppuccin-latte"));
        assert!(ids.contains(&"github-light"));
        assert!(ids.contains(&"solarized-light"));
        assert!(ids.contains(&"catppuccin-mocha"));
        assert!(ids.contains(&"tokyo-night"));
    }

    #[test]
    fn test_len_is_empty() {
        let manager = ThemeManager::with_defaults();
        assert!(!manager.is_empty());
        assert_eq!(manager.len(), 10);
    }

    #[test]
    fn test_add_theme() {
        let mut manager = ThemeManager::with_defaults();
        let mut custom = Theme::dark();
        custom.id = "custom".to_string();
        custom.name = "Custom Theme".to_string();
        manager.add_theme(custom);
        assert_eq!(manager.len(), 11);
        assert!(manager.get("custom").is_some());
    }

    #[test]
    fn test_remove_theme() {
        let mut manager = ThemeManager::with_defaults();
        let mut custom = Theme::dark();
        custom.id = "custom".to_string();
        manager.add_theme(custom);
        assert!(manager.remove_theme("custom"));
        assert!(manager.get("custom").is_none());
    }

    #[test]
    fn test_cannot_remove_builtin_dark() {
        let mut manager = ThemeManager::with_defaults();
        assert!(!manager.remove_theme("dark"));
        assert!(manager.get("dark").is_some());
    }

    #[test]
    fn test_cannot_remove_builtin_light() {
        let mut manager = ThemeManager::with_defaults();
        assert!(!manager.remove_theme("light"));
        assert!(manager.get("light").is_some());
    }

    #[test]
    fn test_cannot_remove_builtin_tokyo_night() {
        let mut manager = ThemeManager::with_defaults();
        assert!(!manager.remove_theme("tokyo-night"));
        assert!(manager.get("tokyo-night").is_some());
    }

    #[test]
    fn test_remove_active_theme_fallback() {
        let mut manager = ThemeManager::with_defaults();
        let mut custom = Theme::dark();
        custom.id = "custom".to_string();
        manager.add_theme(custom);
        manager.set_active("custom");
        assert_eq!(manager.active_id(), "custom");

        manager.remove_theme("custom");
        assert_eq!(manager.active_id(), "dark"); // Fallback
    }

    #[test]
    fn test_toggle_dark_light() {
        let mut manager = ThemeManager::with_defaults();
        assert!(manager.is_dark());

        manager.toggle_dark_light();
        assert!(!manager.is_dark());
        assert_eq!(manager.active_id(), "light");

        manager.toggle_dark_light();
        assert!(manager.is_dark());
        assert_eq!(manager.active_id(), "dark");
    }

    #[test]
    fn test_is_dark() {
        let manager = ThemeManager::with_defaults();
        assert!(manager.is_dark());
    }

    #[test]
    fn test_dark_themes() {
        let manager = ThemeManager::with_defaults();
        let dark_themes = manager.dark_themes();
        assert_eq!(dark_themes.len(), 6);
        assert!(dark_themes.iter().all(|theme| theme.is_dark));
    }

    #[test]
    fn test_light_themes() {
        let manager = ThemeManager::with_defaults();
        let light_themes = manager.light_themes();
        assert_eq!(light_themes.len(), 4);
        assert!(light_themes.iter().all(|theme| !theme.is_dark));
    }

    #[test]
    fn test_load_theme_json() {
        let mut manager = ThemeManager::with_defaults();
        let mut custom = Theme::dark();
        custom.id = "json_test".to_string();
        custom.name = "JSON Test".to_string();
        let json = serde_json::to_string(&custom).unwrap();

        let loaded = manager.load_theme_json(&json).unwrap();
        assert_eq!(loaded.id, "json_test");
        assert_eq!(manager.len(), 11);
    }

    #[test]
    fn test_load_theme_json_invalid() {
        let mut manager = ThemeManager::with_defaults();
        let result = manager.load_theme_json("invalid json");
        assert!(result.is_err());
    }

    #[test]
    fn test_load_custom_themes_empty_dir() {
        let dir = tempdir().unwrap();
        let mut manager = ThemeManager::with_defaults();
        let loaded = manager.load_custom_themes(dir.path()).unwrap();
        assert_eq!(loaded, 0);
    }

    #[test]
    fn test_load_custom_themes_nonexistent_dir() {
        let mut manager = ThemeManager::with_defaults();
        let loaded = manager
            .load_custom_themes(Path::new("/nonexistent/path"))
            .unwrap();
        assert_eq!(loaded, 0);
    }

    #[test]
    fn test_load_custom_themes_with_files() {
        let dir = tempdir().unwrap();

        // Create a valid theme file
        let mut custom = Theme::dark();
        custom.id = "file_test".to_string();
        custom.name = "File Test".to_string();
        let json = serde_json::to_string(&custom).unwrap();

        let theme_path = dir.path().join("custom.json");
        let mut file = std::fs::File::create(&theme_path).unwrap();
        file.write_all(json.as_bytes()).unwrap();

        let mut manager = ThemeManager::with_defaults();
        let loaded = manager.load_custom_themes(dir.path()).unwrap();
        assert_eq!(loaded, 1);
        assert!(manager.get("file_test").is_some());
    }

    #[test]
    fn test_load_custom_themes_ignores_invalid() {
        let dir = tempdir().unwrap();

        // Create an invalid theme file
        let theme_path = dir.path().join("invalid.json");
        let mut file = std::fs::File::create(&theme_path).unwrap();
        file.write_all(b"not valid json").unwrap();

        let mut manager = ThemeManager::with_defaults();
        let loaded = manager.load_custom_themes(dir.path()).unwrap();
        assert_eq!(loaded, 0);
        assert_eq!(manager.len(), 10); // Only built-in themes
    }

    #[test]
    fn test_with_user_themes_loads_from_themes_subdirectory() {
        let dir = tempdir().unwrap();
        let themes_dir = ThemeManager::custom_theme_dir(dir.path());
        std::fs::create_dir_all(&themes_dir).unwrap();

        let custom = Theme::from_runtime("aurora", "Aurora", false, &CodirigentTheme::light());
        let json = serde_json::to_string(&custom).unwrap();
        let theme_path = themes_dir.join("aurora.json");
        let mut file = std::fs::File::create(&theme_path).unwrap();
        file.write_all(json.as_bytes()).unwrap();

        let manager = ThemeManager::with_user_themes(dir.path());

        assert_eq!(manager.len(), 11);
        assert!(manager.get("aurora").is_some());
    }

    #[test]
    fn test_load_theme_file() {
        let dir = tempdir().unwrap();

        let mut custom = Theme::light();
        custom.id = "loaded_file".to_string();
        custom.name = "Loaded File".to_string();
        let json = serde_json::to_string(&custom).unwrap();

        let theme_path = dir.path().join("theme.json");
        let mut file = std::fs::File::create(&theme_path).unwrap();
        file.write_all(json.as_bytes()).unwrap();

        let mut manager = ThemeManager::with_defaults();
        let theme = manager.load_theme_file(&theme_path).unwrap();
        assert_eq!(theme.id, "loaded_file");
    }

    #[test]
    fn test_load_theme_file_not_found() {
        let mut manager = ThemeManager::with_defaults();
        let result = manager.load_theme_file(Path::new("/nonexistent/theme.json"));
        assert!(result.is_err());
    }

    #[test]
    fn test_manager_clone() {
        let manager = ThemeManager::with_defaults();
        let cloned = manager.clone();
        assert_eq!(manager.len(), cloned.len());
        assert_eq!(manager.active_id(), cloned.active_id());
    }

    #[test]
    fn test_manager_debug() {
        let manager = ThemeManager::with_defaults();
        let debug = format!("{:?}", manager);
        assert!(debug.contains("ThemeManager"));
    }

    #[test]
    fn test_active_fallback() {
        let mut manager = ThemeManager::with_defaults();
        manager.active_theme = "nonexistent".to_string();
        // Should fall back to dark theme
        assert_eq!(manager.active().id, "dark");
    }

    #[test]
    fn test_runtime_theme_converts_registered_theme() {
        let manager = ThemeManager::with_defaults();
        let runtime = manager
            .runtime_theme(DEFAULT_THEME_ID)
            .expect("runtime theme");

        assert_eq!(runtime.background, CodirigentTheme::dark().background);
        assert_eq!(
            runtime.terminal_cursor,
            CodirigentTheme::dark().terminal_cursor
        );
    }

    #[test]
    fn test_resolve_runtime_theme_falls_back_for_missing_id() {
        let manager = ThemeManager::with_defaults();
        let resolved = manager.resolve_runtime_theme("missing-theme");

        assert_eq!(resolved.requested_id, "missing-theme");
        assert_eq!(resolved.resolved_id, DEFAULT_THEME_ID);
        assert!(resolved.used_fallback);
        assert_eq!(
            resolved.theme.background,
            CodirigentTheme::dark().background
        );
    }

    #[test]
    fn test_resolve_runtime_theme_falls_back_for_invalid_theme_payload() {
        let mut manager = ThemeManager::with_defaults();
        let mut broken = Theme::dark();
        broken.id = "broken".to_string();
        broken.colors.terminal.palette = TerminalPalette {
            red: "#zz0000".to_string(),
            ..broken.colors.terminal.palette.clone()
        };
        manager.add_theme(broken);

        let resolved = manager.resolve_runtime_theme("broken");
        assert_eq!(resolved.resolved_id, DEFAULT_THEME_ID);
        assert!(resolved.used_fallback);
    }
}
