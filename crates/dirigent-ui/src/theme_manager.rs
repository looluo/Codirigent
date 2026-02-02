//! Theme manager for Dirigent.
//!
//! This module provides a manager for handling multiple themes, including:
//! - Built-in dark and light themes
//! - Loading custom themes from files
//! - Switching between themes at runtime
//!
//! # Example
//!
//! ```
//! use dirigent_ui::theme_manager::ThemeManager;
//!
//! let mut manager = ThemeManager::with_defaults();
//! assert_eq!(manager.active().id, "dark");
//!
//! manager.set_active("light");
//! assert_eq!(manager.active().id, "light");
//! ```

use crate::theme_config::Theme;
use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

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
    /// Create a new empty theme manager.
    ///
    /// Note: This creates a manager with no themes. Most users should
    /// use `with_defaults()` instead.
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
    /// Includes the default dark and light themes.
    ///
    /// # Example
    ///
    /// ```
    /// use dirigent_ui::theme_manager::ThemeManager;
    ///
    /// let manager = ThemeManager::with_defaults();
    /// assert_eq!(manager.list().len(), 2);
    /// ```
    pub fn with_defaults() -> Self {
        let mut themes = HashMap::new();
        themes.insert("dark".to_string(), Theme::dark());
        themes.insert("light".to_string(), Theme::light());

        Self {
            themes,
            active_theme: "dark".to_string(),
        }
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
    /// use dirigent_ui::theme_manager::ThemeManager;
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
        Ok(self.themes.get(&id).unwrap())
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
        Ok(self.themes.get(&id).unwrap())
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
                .get("dark")
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
    /// use dirigent_ui::theme_manager::ThemeManager;
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
    /// Built-in themes (dark, light) cannot be removed.
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
    /// use dirigent_ui::theme_manager::ThemeManager;
    ///
    /// let mut manager = ThemeManager::with_defaults();
    /// assert!(!manager.remove_theme("dark")); // Cannot remove built-in
    /// ```
    pub fn remove_theme(&mut self, id: &str) -> bool {
        // Cannot remove built-in themes
        if id == "dark" || id == "light" {
            return false;
        }
        let removed = self.themes.remove(id).is_some();

        // If we removed the active theme, fall back to dark
        if removed && self.active_theme == id {
            self.active_theme = "dark".to_string();
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
            self.set_active("dark");
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
}

impl Default for ThemeManager {
    fn default() -> Self {
        Self::with_defaults()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        assert_eq!(manager.len(), 2);
        assert!(manager.get("dark").is_some());
        assert!(manager.get("light").is_some());
    }

    #[test]
    fn test_default_trait() {
        let manager = ThemeManager::default();
        assert_eq!(manager.len(), 2);
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
        assert_eq!(themes.len(), 2);
    }

    #[test]
    fn test_list_ids() {
        let manager = ThemeManager::with_defaults();
        let ids = manager.list_ids();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"dark"));
        assert!(ids.contains(&"light"));
    }

    #[test]
    fn test_len_is_empty() {
        let manager = ThemeManager::with_defaults();
        assert!(!manager.is_empty());
        assert_eq!(manager.len(), 2);
    }

    #[test]
    fn test_add_theme() {
        let mut manager = ThemeManager::with_defaults();
        let mut custom = Theme::dark();
        custom.id = "custom".to_string();
        custom.name = "Custom Theme".to_string();
        manager.add_theme(custom);
        assert_eq!(manager.len(), 3);
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
        assert_eq!(dark_themes.len(), 1);
        assert!(dark_themes[0].is_dark);
    }

    #[test]
    fn test_light_themes() {
        let manager = ThemeManager::with_defaults();
        let light_themes = manager.light_themes();
        assert_eq!(light_themes.len(), 1);
        assert!(!light_themes[0].is_dark);
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
        assert_eq!(manager.len(), 3);
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
        assert_eq!(manager.len(), 2); // Only built-in themes
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
}
