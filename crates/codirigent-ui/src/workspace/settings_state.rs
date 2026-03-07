//! Settings state management for WorkspaceView.

use crate::settings::SettingsPage;
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
    /// Working directory used for project-scoped settings.
    pub(super) current_working_dir: Option<PathBuf>,
}

impl SettingsState {
    pub(super) fn new() -> Self {
        Self {
            page: None,
            open: false,
            config_service: DefaultConfigService::new().ok(),
            load_task: None,
            save_task: None,
            cached_user_settings: UserSettings::default(),
            cached_project_config: ProjectConfig::default(),
            current_working_dir: std::env::current_dir().ok(),
        }
    }
}
