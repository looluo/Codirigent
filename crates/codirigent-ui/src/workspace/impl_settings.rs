//! Settings management for WorkspaceView.

use super::gpui::WorkspaceView;
use super::types::{ShellPickerOption, ShellPickerSection, SHELL_PICKER_AUTO_DETECT_LABEL};
use crate::app::OpenSettings;
use crate::settings::SettingsPage;
use crate::theme::CodirigentTheme;
use crate::theme_manager::ThemeManager;
use codirigent_core::config_service::ConfigService;
use gpui::{Context, Window};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Duration;
use tracing::warn;

struct LoadedSettingsSnapshot {
    user_settings: codirigent_core::config::UserSettings,
    project_config: codirigent_core::config::ProjectConfig,
    project_dir: Option<PathBuf>,
    theme_manager: ThemeManager,
}

fn shell_picker_display_label(shell: &str) -> String {
    if shell.is_empty() {
        SHELL_PICKER_AUTO_DETECT_LABEL.to_string()
    } else {
        WorkspaceView::shell_display_label(Some(shell))
    }
}

fn is_common_shell_option(shell: &str) -> bool {
    matches!(
        WorkspaceView::shell_display_label(Some(shell))
            .to_ascii_lowercase()
            .as_str(),
        "zsh" | "bash" | "fish" | "sh" | "pwsh" | "powershell" | "cmd"
    )
}

fn build_shell_picker_options(shell_options: &[String]) -> Vec<ShellPickerOption> {
    let mut normalized_label_counts = HashMap::new();
    for raw_value in shell_options {
        *normalized_label_counts
            .entry(shell_picker_display_label(raw_value))
            .or_insert(0usize) += 1;
    }

    shell_options
        .iter()
        .enumerate()
        .map(|(source_index, raw_value)| {
            let base_label = shell_picker_display_label(raw_value);
            let label = if normalized_label_counts
                .get(&base_label)
                .copied()
                .unwrap_or_default()
                > 1
                && !raw_value.is_empty()
                && raw_value != &base_label
            {
                format!("{base_label} ({raw_value})")
            } else {
                base_label
            };

            ShellPickerOption {
                source_index,
                raw_value: raw_value.clone(),
                label,
            }
        })
        .collect()
}

fn build_shell_picker_sections(shell_options: &[String]) -> Vec<ShellPickerSection> {
    let mut primary = Vec::new();
    let mut more = Vec::new();

    for option in build_shell_picker_options(shell_options) {
        if option.raw_value.is_empty() || is_common_shell_option(&option.raw_value) {
            primary.push(option);
        } else {
            more.push(option);
        }
    }

    let mut sections = Vec::new();
    if !primary.is_empty() {
        sections.push(ShellPickerSection {
            title: None,
            options: primary,
        });
    }
    if !more.is_empty() {
        sections.push(ShellPickerSection {
            title: Some("More"),
            options: more,
        });
    }
    sections
}

fn shell_picker_option_order(shell_options: &[String]) -> Vec<usize> {
    build_shell_picker_sections(shell_options)
        .into_iter()
        .flat_map(|section| {
            section
                .options
                .into_iter()
                .map(|option| option.source_index)
        })
        .collect()
}

/// Convert our `KeyBinding` struct to the GPUI keystroke string format.
///
/// GPUI format: lowercase, dash-separated, e.g. `"secondary-shift-n"`.
/// Uses `"secondary-"` for the platform modifier (Cmd on macOS, Ctrl on
/// Windows/Linux), which is how GPUI's `KeyBinding::new` accepts it.
fn binding_to_gpui_string(binding: &crate::keybindings::KeyBinding) -> String {
    let mut owned: Vec<String> = Vec::new();
    if binding.modifiers.cmd {
        owned.push("secondary".to_string());
    }
    if binding.modifiers.ctrl {
        owned.push("ctrl".to_string());
    }
    if binding.modifiers.alt {
        owned.push("alt".to_string());
    }
    if binding.modifiers.shift {
        owned.push("shift".to_string());
    }
    owned.push(binding.key.to_lowercase());
    owned.join("-")
}

/// Build a GPUI `KeyBinding` list from the user settings keybindings map.
///
/// Each entry maps an action name (e.g. `"new_session"`) to a display
/// string (e.g. `"Ctrl+N"`). We parse the display string, convert it to
/// GPUI keystroke format, and produce a `gpui::KeyBinding`.
///
/// Entries with unknown action names or unparseable binding strings are
/// silently skipped.
fn keybindings_to_gpui_list(
    keybindings: &std::collections::HashMap<String, String>,
) -> Vec<gpui::KeyBinding> {
    use crate::app::{
        ClosePane, CloseSession, Copy, FocusSession1, FocusSession2, FocusSession3, FocusSession4,
        FocusSession5, FocusSession6, FocusSession7, FocusSession8, FocusSession9, NewSession,
        NextLayout, OpenSettings, Paste, QuickSwitch, Quit, SplitHorizontal, SplitVertical,
        ToggleSidebar, ToggleTaskBoard,
    };
    use crate::keybindings::KeybindingManager;

    keybindings
        .iter()
        .filter_map(|(action_name, binding_str)| {
            let km_binding = KeybindingManager::parse_binding(binding_str).ok()?;
            if km_binding.key.is_empty() {
                return None;
            }
            let gpui_str = binding_to_gpui_string(&km_binding);
            // Build the gpui::KeyBinding for each known action name.
            // switch_session_N and focus_session_N share the same numeric index.
            let kb: gpui::KeyBinding = match action_name.as_str() {
                "new_session" => gpui::KeyBinding::new(&gpui_str, NewSession, None),
                "close_session" => gpui::KeyBinding::new(&gpui_str, CloseSession, None),
                "toggle_layout" => gpui::KeyBinding::new(&gpui_str, NextLayout, None),
                "toggle_sidebar" => gpui::KeyBinding::new(&gpui_str, ToggleSidebar, None),
                "toggle_task_board" => gpui::KeyBinding::new(&gpui_str, ToggleTaskBoard, None),
                "open_settings" => gpui::KeyBinding::new(&gpui_str, OpenSettings, None),
                "quit" => gpui::KeyBinding::new(&gpui_str, Quit, None),
                "paste" => gpui::KeyBinding::new(&gpui_str, Paste, None),
                "copy" => gpui::KeyBinding::new(&gpui_str, Copy, None),
                "split_horizontal" => gpui::KeyBinding::new(&gpui_str, SplitHorizontal, None),
                "split_vertical" => gpui::KeyBinding::new(&gpui_str, SplitVertical, None),
                "close_pane" => gpui::KeyBinding::new(&gpui_str, ClosePane, None),
                "quick_switch" => gpui::KeyBinding::new(&gpui_str, QuickSwitch, None),
                "focus_session_1" | "switch_session_1" => {
                    gpui::KeyBinding::new(&gpui_str, FocusSession1, None)
                }
                "focus_session_2" | "switch_session_2" => {
                    gpui::KeyBinding::new(&gpui_str, FocusSession2, None)
                }
                "focus_session_3" | "switch_session_3" => {
                    gpui::KeyBinding::new(&gpui_str, FocusSession3, None)
                }
                "focus_session_4" | "switch_session_4" => {
                    gpui::KeyBinding::new(&gpui_str, FocusSession4, None)
                }
                "focus_session_5" | "switch_session_5" => {
                    gpui::KeyBinding::new(&gpui_str, FocusSession5, None)
                }
                "focus_session_6" | "switch_session_6" => {
                    gpui::KeyBinding::new(&gpui_str, FocusSession6, None)
                }
                "focus_session_7" | "switch_session_7" => {
                    gpui::KeyBinding::new(&gpui_str, FocusSession7, None)
                }
                "focus_session_8" | "switch_session_8" => {
                    gpui::KeyBinding::new(&gpui_str, FocusSession8, None)
                }
                "focus_session_9" | "switch_session_9" => {
                    gpui::KeyBinding::new(&gpui_str, FocusSession9, None)
                }
                _ => return None,
            };
            Some(kb)
        })
        .collect()
}

/// Normalize a keybinding string to the platform-correct display form.
///
/// Parses the string and re-formats it through `format_binding`, which
/// outputs "Ctrl" on Windows/Linux and "Cmd" on macOS for the platform
/// modifier. Returns the original string unchanged on parse failure.
fn normalize_keybinding_display(binding: &str) -> String {
    use crate::keybindings::KeybindingManager;
    KeybindingManager::parse_binding(binding)
        .map(|b| KeybindingManager::format_binding(&b))
        .unwrap_or_else(|_| binding.to_string())
}

fn load_settings_snapshot(
    config_service: &codirigent_core::config_service::DefaultConfigService,
    project_dir: Option<PathBuf>,
) -> LoadedSettingsSnapshot {
    let user_settings = config_service.load_user_settings().unwrap_or_default();
    let project_config = project_dir
        .as_ref()
        .and_then(|dir| config_service.load_project_config(dir).ok())
        .unwrap_or_default();
    let theme_manager = ThemeManager::with_user_themes(config_service.user_config_dir());

    LoadedSettingsSnapshot {
        user_settings,
        project_config,
        project_dir,
        theme_manager,
    }
}

impl WorkspaceView {
    pub(super) fn shell_picker_sections(
        &self,
        shell_options: &[String],
    ) -> Vec<ShellPickerSection> {
        build_shell_picker_sections(shell_options)
    }

    pub(super) fn shell_picker_display_label(shell: &str) -> String {
        shell_picker_display_label(shell)
    }

    pub(super) fn shell_picker_option_order(&self, shell_options: &[String]) -> Vec<usize> {
        shell_picker_option_order(shell_options)
    }

    pub(super) fn effective_user_settings(&self) -> &codirigent_core::config::UserSettings {
        self.settings
            .page
            .as_ref()
            .map(|page| &page.user_settings)
            .unwrap_or(&self.settings.cached_user_settings)
    }

    fn settings_project_dir(&self) -> Option<std::path::PathBuf> {
        self.project
            .project_root
            .clone()
            .or_else(|| self.settings.current_working_dir.clone())
            .or_else(|| std::env::current_dir().ok())
    }

    fn apply_theme_runtime_overrides(
        theme: &mut CodirigentTheme,
        user_settings: &codirigent_core::config::UserSettings,
    ) {
        theme.grid_gap = user_settings.appearance.grid_gap as f32;
        theme.font_size_base = user_settings.appearance.font_size;
        theme.font_size_small = (user_settings.appearance.font_size - Self::UI_FONT_VARIANT_DELTA)
            .max(Self::MIN_UI_SMALL_FONT_SIZE);
        theme.font_size_large = user_settings.appearance.font_size + Self::UI_FONT_VARIANT_DELTA;
        theme.terminal_font_size = user_settings.terminal.font_size;
        theme.terminal_line_height = user_settings.terminal.line_height;
        if !user_settings.terminal.font_family.is_empty() {
            theme.terminal_font_family = user_settings.terminal.font_family.clone();
        }
    }

    fn apply_runtime_theme(&mut self, theme: CodirigentTheme) {
        self.workspace.set_theme(theme.clone());
        self.clipboard.clipboard_preview.set_theme(theme.clone());
        for terminal_view in self.terminals_mut().values_mut() {
            terminal_view.set_theme(theme.clone());
        }
    }

    pub(super) fn resolve_and_apply_theme_id(
        &mut self,
        requested_id: &str,
        user_settings: &codirigent_core::config::UserSettings,
    ) -> String {
        let resolution = self
            .settings
            .theme_manager
            .resolve_runtime_theme(requested_id);
        if resolution.used_fallback {
            warn!(
                requested_theme_id = %resolution.requested_id,
                resolved_theme_id = %resolution.resolved_id,
                "Failed to resolve requested theme ID, using fallback theme"
            );
        }

        let mut theme = resolution.theme;
        Self::apply_theme_runtime_overrides(&mut theme, user_settings);

        self.settings.active_theme_id = resolution.resolved_id.clone();
        let _ = self
            .settings
            .theme_manager
            .set_active(&self.settings.active_theme_id);
        self.apply_runtime_theme(theme);
        self.settings.active_theme_id.clone()
    }

    fn build_settings_page(&self) -> SettingsPage {
        let mut user_settings = self.settings.cached_user_settings.clone();
        let project_config = self.settings.cached_project_config.clone();

        let default_keys = codirigent_core::config::UserSettings::default_keybindings();
        user_settings
            .keybindings
            .retain(|k, _| default_keys.contains_key(k));
        for (k, v) in &default_keys {
            user_settings
                .keybindings
                .entry(k.clone())
                .or_insert_with(|| v.clone());
        }
        // Normalize all displayed binding values to platform-correct labels.
        // This handles configs migrated from another platform (e.g. "Cmd+N" on Windows).
        for v in user_settings.keybindings.values_mut() {
            *v = normalize_keybinding_display(v);
        }

        user_settings.appearance.theme = self.settings.active_theme_id.clone();

        let theme = self.workspace.theme();
        user_settings.appearance.font_size = theme.font_size_base;
        user_settings.appearance.grid_gap = theme.grid_gap as u32;
        user_settings.terminal.font_size = theme.terminal_font_size;
        user_settings.terminal.line_height = theme.terminal_line_height;

        let mut detected = self
            .cache
            .detected_editors
            .clone()
            .unwrap_or_else(|| vec!["code".to_string()]);
        let current_editor = &user_settings.general.editor_command;
        if !current_editor.is_empty() && !detected.iter().any(|e| e == current_editor) {
            detected.insert(0, current_editor.clone());
        }
        if detected.is_empty() {
            detected.push("code".to_string());
        }

        let mut detected_shells = self.cache.detected_shells.clone().unwrap_or_default();
        let current_shell = &user_settings.general.default_shell;
        if !current_shell.is_empty() && !detected_shells.iter().any(|s| s == current_shell) {
            detected_shells.insert(0, current_shell.clone());
        }
        if !detected_shells.iter().any(|s| s.is_empty()) {
            detected_shells.insert(0, String::new());
        }
        let mut seen_shells = HashSet::new();
        detected_shells.retain(|shell| seen_shells.insert(shell.clone()));

        let mut detected_fonts = self.cache.monospace_fonts.clone().unwrap_or_default();
        let current_font = &user_settings.terminal.font_family;
        if !current_font.is_empty() && !detected_fonts.iter().any(|f| f == current_font) {
            detected_fonts.insert(0, current_font.clone());
        }

        SettingsPage::new(
            user_settings,
            project_config,
            detected,
            detected_shells,
            detected_fonts,
        )
    }

    fn schedule_user_settings_snapshot_save(
        &mut self,
        user_settings: codirigent_core::config::UserSettings,
        delay: Duration,
        cx: &mut Context<Self>,
    ) {
        let Some(config_service) = self.settings.config_service.clone() else {
            return;
        };

        self.settings.save_task = None;
        self.settings.save_task = Some(cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let snapshot = user_settings.clone();
            if !delay.is_zero() {
                cx.background_executor().timer(delay).await;
            }

            let result = cx
                .background_executor()
                .spawn(async move { config_service.save_user_settings(&snapshot) })
                .await;

            let _ = this.update(cx, |this, _cx| {
                this.settings.save_task = None;
                match result {
                    Ok(()) => {
                        this.settings.cached_user_settings = user_settings.clone();
                        this.notification_handle
                            .update_settings(user_settings.notifications.clone());
                    }
                    Err(e) => warn!("Failed to save user settings: {}", e),
                }
            });
        }));
    }

    pub(super) fn schedule_settings_save(&mut self, delay: Duration, cx: &mut Context<Self>) {
        let Some(config_service) = self.settings.config_service.clone() else {
            return;
        };
        let Some(page) = self.settings.page.as_ref() else {
            return;
        };

        let user_save_pending = page.user_save_pending;
        let project_save_pending = page.project_save_pending;
        if !user_save_pending && !project_save_pending {
            return;
        }

        let user_settings = page.user_settings.clone();
        let project_config = page.project_config.clone();
        let project_dir = self.settings_project_dir();
        self.settings.current_working_dir = project_dir.clone();

        self.settings.save_task = None;
        self.settings.save_task = Some(cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            if !delay.is_zero() {
                cx.background_executor().timer(delay).await;
            }

            let result = cx
                .background_executor()
                .spawn(async move {
                    let user_error = if user_save_pending {
                        config_service.save_user_settings(&user_settings).err()
                    } else {
                        None
                    };
                    let project_error = if project_save_pending {
                        if let Some(ref dir) = project_dir {
                            config_service
                                .save_project_config(dir, &project_config)
                                .err()
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    (
                        user_settings,
                        project_config,
                        project_dir,
                        user_save_pending,
                        project_save_pending,
                        user_error,
                        project_error,
                    )
                })
                .await;

            let _ = this.update(cx, |this, cx| {
                this.settings.save_task = None;
                let (
                    user_settings,
                    project_config,
                    project_dir,
                    user_save_pending,
                    project_save_pending,
                    user_error,
                    project_error,
                ) = result;

                if user_save_pending {
                    if let Some(err) = user_error {
                        warn!("Failed to save user settings: {}", err);
                    } else {
                        this.settings.cached_user_settings = user_settings.clone();
                        this.notification_handle
                            .update_settings(user_settings.notifications.clone());
                        // Re-register keybindings with GPUI so user changes take
                        // effect immediately without requiring a restart.
                        // Clear the existing keymap first (GPUI bind_keys appends — without
                        // clearing, the list grows by 44 entries on every save and dispatch
                        // becomes O(saves)).
                        cx.clear_key_bindings();
                        let mut merged = crate::app::default_gpui_keybindings();
                        merged.extend(keybindings_to_gpui_list(&user_settings.keybindings));
                        cx.bind_keys(merged);
                        if let Some(page) = this.settings.page.as_mut() {
                            if page.user_settings == user_settings {
                                page.mark_user_saved();
                            } else {
                                page.user_save_pending = true;
                            }
                        }
                    }
                }

                if project_save_pending {
                    if let Some(err) = project_error {
                        warn!("Failed to save project config: {}", err);
                    } else {
                        this.settings.cached_project_config = project_config.clone();
                        this.settings.current_working_dir = project_dir;
                        if let Some(page) = this.settings.page.as_mut() {
                            if page.project_config == project_config {
                                page.mark_project_saved();
                            } else {
                                page.project_save_pending = true;
                            }
                        }
                    }
                }
                this.maybe_schedule_settings_save(cx);
            });
        }));
    }

    pub(super) fn maybe_schedule_settings_save(&mut self, cx: &mut Context<Self>) {
        let should_flush = self
            .settings
            .page
            .as_ref()
            .map(|page| page.user_save_pending || page.project_save_pending)
            .unwrap_or(false);
        if should_flush {
            self.schedule_settings_save(Duration::from_millis(500), cx);
        }
    }

    pub(super) fn start_settings_background_load(
        &mut self,
        restore_after_load: bool,
        cx: &mut Context<Self>,
    ) {
        self.settings.restore_after_load |= restore_after_load;
        let Some(config_service) = self.settings.config_service.clone() else {
            if self.settings.restore_after_load {
                self.settings.restore_after_load = false;
                self.spawn_restore_sessions_from_disk(cx);
            }
            return;
        };

        let project_dir = self.settings_project_dir();
        if self.settings.current_working_dir == project_dir {
            if self.settings.load_task.is_some() {
                return;
            }
            if self.settings.loaded_once {
                if self.settings.restore_after_load
                    && self.workspace.sessions().is_empty()
                    && !self.polling.restore_in_flight
                {
                    self.settings.restore_after_load = false;
                    self.spawn_restore_sessions_from_disk(cx);
                }
                return;
            }
        }
        self.settings.current_working_dir = project_dir.clone();
        self.settings.load_task = None;
        self.settings.load_task = Some(cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let loaded = cx
                .background_executor()
                .spawn(async move { load_settings_snapshot(&config_service, project_dir) })
                .await;

            let _ = this.update(cx, |this, cx| {
                let restore_after_load = std::mem::take(&mut this.settings.restore_after_load);
                this.settings.load_task = None;
                this.settings.loaded_once = true;
                this.settings.theme_manager = loaded.theme_manager;
                let mut user_settings = loaded.user_settings.clone();
                let resolved_theme_id = this
                    .resolve_and_apply_theme_id(&user_settings.appearance.theme, &user_settings);
                user_settings.appearance.theme = resolved_theme_id;
                this.settings.cached_user_settings = user_settings.clone();
                this.settings.cached_project_config = loaded.project_config.clone();
                this.settings.current_working_dir = loaded.project_dir;
                this.notification_handle
                    .update_settings(user_settings.notifications.clone());
                this.top_bar
                    .load_saved_profiles(user_settings.saved_layouts.clone());

                if let Some(existing_page) = this.settings.page.as_ref() {
                    if !existing_page.user_save_pending && !existing_page.project_save_pending {
                        let active_category = existing_page.active_category();
                        let open_dropdown = existing_page.open_dropdown.clone();
                        let dropdown_click_pos = existing_page.dropdown_click_pos;
                        let recording_shortcut = existing_page.recording_shortcut.clone();
                        let focused_shortcut_row = existing_page.focused_shortcut_row.clone();

                        let mut page = this.build_settings_page();
                        page.set_category(active_category);
                        page.open_dropdown = open_dropdown;
                        page.dropdown_click_pos = dropdown_click_pos;
                        page.recording_shortcut = recording_shortcut;
                        page.focused_shortcut_row = focused_shortcut_row;
                        // Validate the preserved recording state is still pointing to a known
                        // action.  If the action was removed (e.g. by a downgrade), drop the
                        // stale state.
                        if let Some(ref name) = page.recording_shortcut {
                            if !page.user_settings.keybindings.contains_key(name) {
                                page.recording_shortcut = None;
                                page.focused_shortcut_row = None;
                            }
                        }
                        this.settings.page = Some(page);
                    }
                } else if this.settings.open {
                    this.settings.page = Some(this.build_settings_page());
                }

                if restore_after_load
                    && this.workspace.sessions().is_empty()
                    && !this.polling.restore_in_flight
                {
                    this.spawn_restore_sessions_from_disk(cx);
                }
            });
        }));
    }

    pub(super) fn open_settings(&mut self) {
        if self.settings.page.is_none()
            && (self.settings.loaded_once || self.settings.load_task.is_none())
        {
            self.settings.page = Some(self.build_settings_page());
        }
        self.settings.open = true;
    }

    pub(super) fn close_settings(&mut self, cx: &mut Context<Self>) {
        self.settings.save_task = None;
        self.schedule_settings_save(Duration::ZERO, cx);
        if let Some(ref mut page) = self.settings.page {
            page.open_dropdown = None;
        }
        self.settings.open = false;
    }

    pub(super) fn handle_open_settings(
        &mut self,
        _action: &OpenSettings,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_settings();
        cx.notify();
    }

    pub(super) fn persist_layout_profiles_to_settings(&mut self, cx: &mut Context<Self>) {
        let saved_layouts = self.top_bar.export_user_profiles();
        self.settings.cached_user_settings.saved_layouts = saved_layouts.clone();
        if let Some(page) = self.settings.page.as_mut() {
            page.user_settings.saved_layouts = saved_layouts;
            page.user_save_pending = true;
            self.schedule_settings_save(Duration::from_millis(200), cx);
        } else {
            self.schedule_user_settings_snapshot_save(
                self.settings.cached_user_settings.clone(),
                Duration::from_millis(200),
                cx,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::CodirigentTheme;
    use crate::theme_config::Theme;
    use crate::theme_manager::CUSTOM_THEME_DIRECTORY_NAME;
    use codirigent_core::config_service::DefaultConfigService;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_binding_to_gpui_string_ctrl_n() {
        use crate::keybindings::{KeyBinding, Modifiers};
        let binding = KeyBinding::new(
            "N",
            Modifiers {
                cmd: true,
                ..Default::default()
            },
        );
        let result = binding_to_gpui_string(&binding);
        assert_eq!(result, "secondary-n");
    }

    #[test]
    fn test_binding_to_gpui_string_shift() {
        use crate::keybindings::{KeyBinding, Modifiers};
        let binding = KeyBinding::new(
            "D",
            Modifiers {
                cmd: true,
                shift: true,
                ..Default::default()
            },
        );
        let result = binding_to_gpui_string(&binding);
        assert_eq!(result, "secondary-shift-d");
    }

    #[test]
    fn test_keybindings_to_gpui_list_new_session() {
        let mut map = std::collections::HashMap::new();
        map.insert("new_session".to_string(), "Ctrl+N".to_string());
        let list = keybindings_to_gpui_list(&map);
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn test_keybindings_to_gpui_list_skips_unknown_action() {
        let mut map = std::collections::HashMap::new();
        map.insert("unknown_action_xyz".to_string(), "Ctrl+N".to_string());
        let list = keybindings_to_gpui_list(&map);
        assert_eq!(list.len(), 0);
    }

    #[test]
    fn test_keybindings_to_gpui_list_includes_toggle_task_board() {
        let mut map = std::collections::HashMap::new();
        map.insert("toggle_task_board".to_string(), "Ctrl+B".to_string());
        let list = keybindings_to_gpui_list(&map);
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn test_keybindings_to_gpui_list_includes_quick_switch() {
        // quick_switch is a live-reload binding that must appear in the list.
        let mut map = std::collections::HashMap::new();
        map.insert("quick_switch".to_string(), "Ctrl+K".to_string());
        let list = keybindings_to_gpui_list(&map);
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn test_keybindings_to_gpui_list_skips_empty_key() {
        // A binding that parses but has an empty key should be silently dropped
        // rather than causing KeyBinding::new to panic on an invalid keystroke.
        let mut map = std::collections::HashMap::new();
        // "Ctrl+" parses as a modifier-only binding with empty key — verify safe skip.
        // Since parse_binding may reject this, also test via a direct call.
        // If parse_binding rejects it, the entry is already dropped before our guard;
        // the test verifies the overall list is still empty / doesn't panic.
        map.insert("new_session".to_string(), "Ctrl+".to_string());
        let list = keybindings_to_gpui_list(&map);
        // Either parse_binding rejects "Ctrl+" (returning 0) or our guard catches
        // the empty key (also returning 0). Either way no panic.
        assert_eq!(list.len(), 0);
    }

    #[test]
    fn test_keybindings_to_gpui_list_includes_new_actions() {
        let mut map = std::collections::HashMap::new();
        map.insert("open_settings".to_string(), "Ctrl+,".to_string());
        map.insert("quit".to_string(), "Ctrl+Q".to_string());
        map.insert("paste".to_string(), "Ctrl+V".to_string());
        map.insert("copy".to_string(), "Ctrl+C".to_string());
        map.insert("split_horizontal".to_string(), "Ctrl+D".to_string());
        map.insert("split_vertical".to_string(), "Ctrl+Shift+D".to_string());
        map.insert("close_pane".to_string(), "Ctrl+Shift+W".to_string());
        let list = keybindings_to_gpui_list(&map);
        assert_eq!(list.len(), 7);
    }

    #[test]
    fn test_normalize_keybinding_display_cmd_to_ctrl_on_non_macos() {
        // "Cmd+N" → platform modifier display: "Cmd+N" on macOS, "Ctrl+N" elsewhere.
        #[cfg(not(target_os = "macos"))]
        assert_eq!(normalize_keybinding_display("Cmd+N"), "Ctrl+N");
        #[cfg(target_os = "macos")]
        assert_eq!(normalize_keybinding_display("Cmd+N"), "Cmd+N");
    }

    #[test]
    fn test_normalize_keybinding_display_preserves_valid_platform_string() {
        #[cfg(not(target_os = "macos"))]
        assert_eq!(normalize_keybinding_display("Ctrl+N"), "Ctrl+N");
        #[cfg(target_os = "macos")]
        assert_eq!(normalize_keybinding_display("Cmd+N"), "Cmd+N");
    }

    #[test]
    fn test_normalize_keybinding_display_falls_back_on_invalid() {
        // Unparseable strings should be returned unchanged.
        assert_eq!(
            normalize_keybinding_display("not-a-binding"),
            "not-a-binding"
        );
    }

    #[test]
    fn shell_picker_sections_group_common_shells_before_more() {
        let sections = build_shell_picker_sections(&[
            String::new(),
            "nu".to_string(),
            "zsh".to_string(),
            "bash".to_string(),
            "xonsh".to_string(),
        ]);

        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].title, None);
        assert_eq!(
            sections[0]
                .options
                .iter()
                .map(|option| option.label.as_str())
                .collect::<Vec<_>>(),
            vec![SHELL_PICKER_AUTO_DETECT_LABEL, "zsh", "bash"]
        );
        assert_eq!(sections[1].title, Some("More"));
        assert_eq!(
            sections[1]
                .options
                .iter()
                .map(|option| option.label.as_str())
                .collect::<Vec<_>>(),
            vec!["nu", "xonsh"]
        );
    }

    #[test]
    fn shell_picker_sections_treat_normalized_common_shells_as_primary() {
        let sections = build_shell_picker_sections(&[
            r"C:\Windows\System32\WindowsPowerShell\v1.0\POWERSHELL.EXE".to_string(),
            "/bin/zsh".to_string(),
        ]);

        assert_eq!(sections.len(), 1);
        assert_eq!(
            sections[0]
                .options
                .iter()
                .map(|option| option.label.as_str())
                .collect::<Vec<_>>(),
            vec!["POWERSHELL", "zsh"]
        );
    }

    #[test]
    fn shell_picker_sections_disambiguate_duplicate_normalized_labels() {
        let sections = build_shell_picker_sections(&[
            "zsh".to_string(),
            "/bin/zsh".to_string(),
            r"C:\Windows\System32\cmd.exe".to_string(),
            "cmd".to_string(),
        ]);

        assert_eq!(sections.len(), 1);
        assert_eq!(
            sections[0]
                .options
                .iter()
                .map(|option| option.label.as_str())
                .collect::<Vec<_>>(),
            vec![
                "zsh",
                "zsh (/bin/zsh)",
                r"cmd (C:\Windows\System32\cmd.exe)",
                "cmd",
            ]
        );
    }

    #[test]
    fn shell_picker_option_order_matches_visual_section_order() {
        let order = shell_picker_option_order(&[
            String::new(),
            "nu".to_string(),
            "zsh".to_string(),
            "bash".to_string(),
        ]);

        assert_eq!(order, vec![0, 2, 3, 1]);
    }

    #[test]
    fn test_default_binding_tables_are_in_sync() {
        use crate::app::default_gpui_keybindings;
        use codirigent_core::config::UserSettings;

        let gpui_count = default_gpui_keybindings().len();
        let settings_bindings = UserSettings::default_keybindings();
        let live_reload_count = keybindings_to_gpui_list(&settings_bindings).len();

        // Every action registered at startup must also be rebindable via settings.
        // UserSettings::default_keybindings uses "switch_session_N" action names while
        // default_gpui_keybindings uses "focus_session_N" action names — keybindings_to_gpui_list
        // recognises both aliases, so counts should be equal.
        // If this fails, a new action was added to one table but not the other.
        assert_eq!(
            live_reload_count, gpui_count,
            "default_gpui_keybindings ({gpui_count} entries) and \
             UserSettings::default_keybindings live-reload list ({live_reload_count} entries) \
             are out of sync — add the missing action to both tables"
        );
    }

    #[test]
    fn apply_theme_runtime_overrides_preserves_user_preferences() {
        let mut theme = CodirigentTheme::dark();
        let mut user_settings = codirigent_core::config::UserSettings::default();
        user_settings.appearance.font_size = 17.0;
        user_settings.appearance.grid_gap = 7;
        user_settings.terminal.font_size = 15.0;
        user_settings.terminal.line_height = 1.3;
        user_settings.terminal.font_family = "FiraCode Nerd Font".to_string();

        WorkspaceView::apply_theme_runtime_overrides(&mut theme, &user_settings);

        assert_eq!(theme.font_size_base, 17.0);
        assert_eq!(theme.font_size_small, 15.0);
        assert_eq!(theme.font_size_large, 19.0);
        assert_eq!(theme.grid_gap, 7.0);
        assert_eq!(theme.terminal_font_size, 15.0);
        assert_eq!(theme.terminal_line_height, 1.3);
        assert_eq!(theme.terminal_font_family, "FiraCode Nerd Font");
    }

    #[test]
    fn apply_theme_runtime_overrides_keeps_theme_terminal_font_when_unset() {
        let mut theme = CodirigentTheme::dark();
        let original_font_family = theme.terminal_font_family.clone();
        let mut user_settings = codirigent_core::config::UserSettings::default();
        user_settings.terminal.font_family.clear();

        WorkspaceView::apply_theme_runtime_overrides(&mut theme, &user_settings);

        assert_eq!(theme.terminal_font_family, original_font_family);
    }

    #[test]
    fn load_settings_snapshot_loads_custom_themes_from_user_config_dir() {
        let dir = tempdir().unwrap();
        let config_service = DefaultConfigService::with_config_dir(dir.path().to_path_buf());
        let themes_dir = dir.path().join(CUSTOM_THEME_DIRECTORY_NAME);
        fs::create_dir_all(&themes_dir).unwrap();

        let mut settings = codirigent_core::config::UserSettings::default();
        settings.appearance.theme = "aurora".to_string();
        config_service.save_user_settings(&settings).unwrap();

        let custom_theme =
            Theme::from_runtime("aurora", "Aurora", false, &CodirigentTheme::light());
        fs::write(
            themes_dir.join("aurora.json"),
            custom_theme.to_json().unwrap(),
        )
        .unwrap();

        let snapshot = load_settings_snapshot(&config_service, None);

        assert_eq!(snapshot.user_settings.appearance.theme, "aurora");
        assert!(snapshot.theme_manager.get("aurora").is_some());
        assert_eq!(snapshot.theme_manager.len(), 3);
    }
}
