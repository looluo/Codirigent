//! Settings state management for WorkspaceView.

use crate::settings::SettingsPage;
use crate::theme_manager::ThemeManager;
use codirigent_core::config::{ProjectConfig, UserSettings};
use codirigent_core::config_service::DefaultConfigService;
use std::path::PathBuf;

/// Groups all settings-related state for the workspace.
pub(super) struct SettingsState {
    /// Currently active settings page (None if settings closed).
    pub(super) page: Option<SettingsPage>,
    /// Whether the settings panel is open.
    pub(super) open: bool,
    /// Configuration service for reading/writing settings.
    pub(super) config_service: Option<DefaultConfigService>,
    /// Background load task for startup/cache refresh.
    pub(super) load_task: Option<gpui::Task<()>>,
    /// Debounced background save task for settings persistence.
    pub(super) save_task: Option<gpui::Task<()>>,
    /// Cached user settings loaded in the background at startup.
    pub(super) cached_user_settings: UserSettings,
    /// Cached project config for the current working directory.
    pub(super) cached_project_config: ProjectConfig,
    /// Theme registry used to resolve theme IDs into runtime themes.
    pub(super) theme_manager: ThemeManager,
    /// Theme ID currently applied to the workspace runtime.
    pub(super) active_theme_id: String,
    /// Working directory used for project-scoped settings.
    pub(super) current_working_dir: Option<PathBuf>,
    /// Whether settings have been loaded from disk at least once.
    pub(super) loaded_once: bool,
    /// Whether session restore should run after the next successful settings load.
    pub(super) restore_after_load: bool,
}

impl SettingsState {
    pub(super) fn new() -> Self {
        let theme_manager = ThemeManager::with_defaults();
        let active_theme_id = theme_manager.active_id().to_string();
        Self {
            page: None,
            open: false,
            config_service: DefaultConfigService::new().ok(),
            load_task: None,
            save_task: None,
            cached_user_settings: UserSettings::default(),
            cached_project_config: ProjectConfig::default(),
            theme_manager,
            active_theme_id,
            current_working_dir: std::env::current_dir().ok(),
            loaded_once: false,
            restore_after_load: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme_manager::DEFAULT_THEME_ID;

    #[test]
    fn settings_state_starts_with_registry_default_theme_id() {
        let state = SettingsState::new();

        assert_eq!(state.active_theme_id, DEFAULT_THEME_ID);
        assert_eq!(state.theme_manager.active_id(), DEFAULT_THEME_ID);
    }
}
