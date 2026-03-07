//! Settings management for WorkspaceView.

use super::gpui::WorkspaceView;
use crate::app::OpenSettings;
use crate::settings::SettingsPage;
use codirigent_core::config_service::ConfigService;
use gpui::{Context, Window};
use std::time::Duration;
use tracing::warn;

impl WorkspaceView {
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

        let bg: gpui::Hsla = self.workspace.theme().background.into();
        user_settings.appearance.theme = if bg.l > 0.5 {
            "light".to_string()
        } else {
            "dark".to_string()
        };

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
                        this.notification_manager
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

            let _ = this.update(cx, |this, _cx| {
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
                        this.notification_manager
                            .update_settings(user_settings.notifications.clone());
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
                this.maybe_schedule_settings_save(_cx);
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
                .spawn(async move {
                    let user_settings = config_service.load_user_settings().unwrap_or_default();
                    let project_config = project_dir
                        .as_ref()
                        .and_then(|dir| config_service.load_project_config(dir).ok())
                        .unwrap_or_default();
                    (user_settings, project_config, project_dir)
                })
                .await;

            let _ = this.update(cx, |this, cx| {
                let restore_after_load = std::mem::take(&mut this.settings.restore_after_load);
                this.settings.load_task = None;
                this.settings.loaded_once = true;
                this.settings.cached_user_settings = loaded.0.clone();
                this.settings.cached_project_config = loaded.1.clone();
                this.settings.current_working_dir = loaded.2;
                this.notification_manager
                    .update_settings(loaded.0.notifications.clone());
                this.top_bar
                    .load_saved_profiles(loaded.0.saved_layouts.clone());

                if let Some(existing_page) = this.settings.page.as_ref() {
                    if !existing_page.user_save_pending && !existing_page.project_save_pending {
                        let active_category = existing_page.active_category();
                        let open_dropdown = existing_page.open_dropdown.clone();
                        let dropdown_click_pos = existing_page.dropdown_click_pos;
                        let recording_shortcut = existing_page.recording_shortcut.clone();

                        let mut page = this.build_settings_page();
                        page.set_category(active_category);
                        page.open_dropdown = open_dropdown;
                        page.dropdown_click_pos = dropdown_click_pos;
                        page.recording_shortcut = recording_shortcut;
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
