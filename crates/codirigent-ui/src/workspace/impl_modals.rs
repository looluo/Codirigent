//! Modal dialog handlers for WorkspaceView.
//!
//! This module contains all methods related to:
//! - Task creation modal (open, close, apply, edit)
//! - Session action modal (rename, assign group)
//! - Modal keyboard input handling

use super::gpui::WorkspaceView;
use super::types::{SessionActionKind, SessionActionModal, TaskCreationModal, GROUP_COLOR_PALETTE};
use codirigent_core::{SessionId, SessionManager, Task, TaskId};
use gpui::{Context, KeyDownEvent};
use std::path::Path;
use tracing::{info, warn};

impl WorkspaceView {
    pub(super) fn open_session_action_modal(
        &mut self,
        session_id: SessionId,
        kind: SessionActionKind,
    ) {
        let input = match kind {
            SessionActionKind::Rename => self
                .workspace
                .session(session_id)
                .map(|session| session.name.clone())
                .unwrap_or_default(),
            SessionActionKind::AssignGroup => self
                .workspace
                .session(session_id)
                .and_then(|session| session.group.clone())
                .unwrap_or_default(),
        };

        self.modals.session_action = Some(SessionActionModal {
            session_id,
            kind,
            input,
            error: None,
        });
    }

    pub(super) fn close_session_action_modal(&mut self) {
        self.modals.session_action = None;
    }

    /// Pick the next unused group color from the palette.
    pub(super) fn next_group_color(&self) -> String {
        let used_colors: std::collections::HashSet<&str> = self
            .workspace
            .sessions()
            .iter()
            .filter_map(|s| s.color.as_deref())
            .collect();
        GROUP_COLOR_PALETTE
            .iter()
            .find(|c| !used_colors.contains(**c))
            .unwrap_or(&GROUP_COLOR_PALETTE[0])
            .to_string()
    }

    pub(super) fn open_task_creation_modal(&mut self) {
        let project_dir = self.workspace.focused_session().and_then(|s| {
            s.git_info
                .as_ref()
                .map(|g| g.repo_root.clone())
                .or_else(|| Some(s.working_directory.clone()))
        });

        self.modals.task_creation = Some(TaskCreationModal {
            title: String::new(),
            description: String::new(),
            priority: codirigent_core::TaskPriority::Medium,
            focused_field: 0,
            cursor_positions: [0, 0, 0],
            error: None,
            project_dir,
            plan_file: String::new(),
            editing_task_id: None,
        });
    }

    /// Open the task creation modal pre-filled with a file's name and path.
    pub(super) fn open_task_creation_modal_for_file(&mut self, path: &Path) {
        let project_dir = self
            .project
            .file_tree_model
            .as_ref()
            .map(|t| t.root().to_path_buf());

        let relative_path = project_dir
            .as_ref()
            .and_then(|root| path.strip_prefix(root).ok())
            .unwrap_or(path);

        let filename = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        self.modals.task_creation = Some(TaskCreationModal {
            title: filename,
            description: String::new(),
            priority: codirigent_core::TaskPriority::Medium,
            focused_field: 0,
            cursor_positions: [
                path.file_name()
                    .map(|n| n.to_string_lossy().chars().count())
                    .unwrap_or(0),
                0,
                relative_path.to_string_lossy().chars().count(),
            ],
            error: None,
            project_dir,
            plan_file: relative_path.to_string_lossy().to_string(),
            editing_task_id: None,
        });
    }

    /// Open the task modal pre-filled with an existing task's data for editing.
    pub(super) fn open_task_edit_modal(&mut self, task_id: &TaskId) {
        let task = match self.task_manager.lock() {
            Ok(mgr) => mgr.get_task(task_id).cloned(),
            Err(_) => None,
        };

        let Some(task) = task else {
            warn!("Cannot edit task {}: not found", task_id);
            return;
        };

        self.modals.task_creation = Some(TaskCreationModal {
            title: task.title.clone(),
            description: task.description.clone(),
            priority: task.priority,
            focused_field: 0,
            cursor_positions: [
                task.title.chars().count(),
                task.description.chars().count(),
                task.plan_file
                    .as_ref()
                    .map(|s| s.chars().count())
                    .unwrap_or(0),
            ],
            error: None,
            project_dir: task.project_dir.clone(),
            plan_file: task.plan_file.clone().unwrap_or_default(),
            editing_task_id: Some(task_id.clone()),
        });
    }

    pub(super) fn close_task_creation_modal(&mut self) {
        self.modals.task_creation = None;
    }

    pub(super) fn apply_task_creation_modal(&mut self, cx: &mut Context<Self>) {
        let Some(modal) = self.modals.task_creation.clone() else {
            return;
        };

        let title = modal.title.trim().to_string();
        let description = modal.description.trim().to_string();

        // Validate title is not empty
        if title.is_empty() {
            if let Some(ref mut active) = self.modals.task_creation {
                active.error = Some("Title is required".to_string());
            }
            cx.notify();
            return;
        }

        let plan_file = if modal.plan_file.trim().is_empty() {
            None
        } else {
            Some(modal.plan_file.trim().to_string())
        };

        if let Some(existing_id) = &modal.editing_task_id {
            // Update existing task
            if let Ok(mut manager) = self.task_manager.lock() {
                if let Err(e) = manager.update_task(
                    existing_id,
                    title,
                    description,
                    modal.priority,
                    plan_file,
                    modal.project_dir.clone(),
                ) {
                    if let Some(ref mut active) = self.modals.task_creation {
                        active.error = Some(format!("Failed to update task: {}", e));
                    }
                    cx.notify();
                    return;
                }
                info!(%existing_id, "Task updated successfully from modal");
            } else {
                if let Some(ref mut active) = self.modals.task_creation {
                    active.error = Some("Failed to access task manager".to_string());
                }
                cx.notify();
                return;
            }
        } else {
            // Create new task
            let task_id = TaskId::from(format!("task-{}", self.next_session_id));
            self.next_session_id += 1;

            let mut task = Task::new(task_id.clone(), title, description);
            task.priority = modal.priority;
            task.project_dir = modal.project_dir.clone();
            task.plan_file = plan_file;

            if let Ok(mut manager) = self.task_manager.lock() {
                if let Err(e) = manager.create_task(task) {
                    if let Some(ref mut active) = self.modals.task_creation {
                        active.error = Some(format!("Failed to create task: {}", e));
                    }
                    cx.notify();
                    return;
                }
                info!(%task_id, "Task created successfully from modal");
            } else {
                if let Some(ref mut active) = self.modals.task_creation {
                    active.error = Some("Failed to access task manager".to_string());
                }
                cx.notify();
                return;
            }
        }

        self.mark_ui_sync_dirty();
        self.close_task_creation_modal();
        cx.notify();
    }

    pub(super) fn apply_session_action_modal(&mut self, cx: &mut Context<Self>) {
        let Some(modal) = self.modals.session_action.clone() else {
            return;
        };

        let value = modal.input.trim().to_string();
        if value.is_empty() {
            if let Some(ref mut active) = self.modals.session_action {
                active.error = Some("Value is required".to_string());
            }
            cx.notify();
            return;
        }

        match modal.kind {
            SessionActionKind::Rename => {
                if let Ok(manager) = self.session_manager.lock() {
                    if let Err(e) = manager.rename_session(modal.session_id, value) {
                        warn!("Failed to rename session: {}", e);
                    }
                }
            }
            SessionActionKind::AssignGroup => {
                let color = self.next_group_color();
                if let Ok(manager) = self.session_manager.lock() {
                    if let Err(e) =
                        manager.set_session_group(modal.session_id, Some(value), Some(color))
                    {
                        warn!("Failed to set session group: {}", e);
                    }
                }
            }
        }

        // Sync workspace cache immediately so the UI reflects the change
        if let Ok(manager) = self.session_manager.lock() {
            self.workspace
                .sync_sessions_from_manager(&manager.list_sessions());
        }
        self.mark_ui_sync_dirty();
        self.save_state_to_disk(cx);
        self.close_session_action_modal();
        cx.notify();
    }

    pub(super) fn handle_session_action_key_down(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(modal) = self.modals.session_action.as_mut() else {
            return false;
        };

        let key = event.keystroke.key.to_lowercase();
        match key.as_str() {
            "escape" => {
                self.close_session_action_modal();
                cx.notify();
                return true;
            }
            "enter" => {
                self.apply_session_action_modal(cx);
                return true;
            }
            "backspace" => {
                modal.input.pop();
                cx.notify();
                return true;
            }
            "space" => {
                // GPUI on Windows reports space as key="space" with key_char=None
                modal.input.push(' ');
                cx.notify();
                return true;
            }
            _ => {}
        }

        // Ctrl+A selects all (clears input for easy replacement)
        if (event.keystroke.modifiers.control || event.keystroke.modifiers.platform) && key == "a" {
            modal.input.clear();
            cx.notify();
            return true;
        }

        // Ignore other modifier-based shortcuts inside the modal.
        if event.keystroke.modifiers.control
            || event.keystroke.modifiers.alt
            || event.keystroke.modifiers.platform
        {
            return true;
        }

        if let Some(ref key_char) = event.keystroke.key_char {
            if let Some(ch) = key_char.chars().next() {
                if ch.is_ascii_graphic() || ch == ' ' {
                    modal.input.push(ch);
                    cx.notify();
                }
            }
        }

        true
    }

    fn char_count(text: &str) -> usize {
        text.chars().count()
    }

    fn byte_index_for_char(text: &str, char_index: usize) -> usize {
        if char_index == 0 {
            return 0;
        }
        text.char_indices()
            .nth(char_index)
            .map(|(i, _)| i)
            .unwrap_or(text.len())
    }

    fn focused_field_and_cursor_mut(
        modal: &mut TaskCreationModal,
    ) -> Option<(&mut String, &mut usize)> {
        match modal.focused_field {
            0 => Some((&mut modal.title, &mut modal.cursor_positions[0])),
            1 => Some((&mut modal.description, &mut modal.cursor_positions[1])),
            2 => Some((&mut modal.plan_file, &mut modal.cursor_positions[2])),
            _ => None,
        }
    }

    fn clamp_task_modal_cursor(modal: &mut TaskCreationModal) {
        let title_len = Self::char_count(&modal.title);
        let desc_len = Self::char_count(&modal.description);
        let plan_len = Self::char_count(&modal.plan_file);
        modal.cursor_positions[0] = modal.cursor_positions[0].min(title_len);
        modal.cursor_positions[1] = modal.cursor_positions[1].min(desc_len);
        modal.cursor_positions[2] = modal.cursor_positions[2].min(plan_len);
    }

    fn insert_at_cursor(field: &mut String, cursor: &mut usize, text: &str) {
        let cursor_byte = Self::byte_index_for_char(field, *cursor);
        field.insert_str(cursor_byte, text);
        *cursor += text.chars().count();
    }

    fn backspace_at_cursor(field: &mut String, cursor: &mut usize) {
        if *cursor == 0 {
            return;
        }
        let end = Self::byte_index_for_char(field, *cursor);
        let start = Self::byte_index_for_char(field, *cursor - 1);
        field.replace_range(start..end, "");
        *cursor -= 1;
    }

    fn delete_at_cursor(field: &mut String, cursor: &mut usize) {
        let len = Self::char_count(field);
        if *cursor >= len {
            return;
        }
        let start = Self::byte_index_for_char(field, *cursor);
        let end = Self::byte_index_for_char(field, *cursor + 1);
        field.replace_range(start..end, "");
    }

    fn move_cursor_left(field: &str, cursor: &mut usize) {
        let len = Self::char_count(field);
        *cursor = (*cursor).min(len);
        if *cursor > 0 {
            *cursor -= 1;
        }
    }

    fn move_cursor_right(field: &str, cursor: &mut usize) {
        let len = Self::char_count(field);
        if *cursor < len {
            *cursor += 1;
        }
    }

    fn move_cursor_home(cursor: &mut usize) {
        *cursor = 0;
    }

    fn move_cursor_end(field: &str, cursor: &mut usize) {
        *cursor = Self::char_count(field);
    }

    pub(super) fn handle_task_creation_key_down(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(modal) = self.modals.task_creation.as_mut() else {
            return false;
        };
        Self::clamp_task_modal_cursor(modal);

        let key = event.keystroke.key.to_lowercase();
        match key.as_str() {
            "escape" => {
                self.close_task_creation_modal();
                cx.notify();
                return true;
            }
            "enter" => {
                // Submit from title or plan_file field, newline in description
                if modal.focused_field == 0 || modal.focused_field == 2 {
                    self.apply_task_creation_modal(cx);
                } else {
                    if let Some((field, cursor)) = Self::focused_field_and_cursor_mut(modal) {
                        Self::insert_at_cursor(field, cursor, "\n");
                    }
                    cx.notify();
                }
                return true;
            }
            "tab" => {
                // Cycle: title(0) -> description(1) -> plan_file(2) -> title(0)
                modal.focused_field = (modal.focused_field + 1) % 3;
                cx.notify();
                return true;
            }
            "backspace" => {
                if let Some((field, cursor)) = Self::focused_field_and_cursor_mut(modal) {
                    Self::backspace_at_cursor(field, cursor);
                }
                modal.error = None;
                cx.notify();
                return true;
            }
            "delete" => {
                if let Some((field, cursor)) = Self::focused_field_and_cursor_mut(modal) {
                    Self::delete_at_cursor(field, cursor);
                }
                modal.error = None;
                cx.notify();
                return true;
            }
            "left" | "arrowleft" => {
                if let Some((field, cursor)) = Self::focused_field_and_cursor_mut(modal) {
                    Self::move_cursor_left(field, cursor);
                }
                cx.notify();
                return true;
            }
            "right" | "arrowright" => {
                if let Some((field, cursor)) = Self::focused_field_and_cursor_mut(modal) {
                    Self::move_cursor_right(field, cursor);
                }
                cx.notify();
                return true;
            }
            "home" => {
                if let Some((_, cursor)) = Self::focused_field_and_cursor_mut(modal) {
                    Self::move_cursor_home(cursor);
                }
                cx.notify();
                return true;
            }
            "end" => {
                if let Some((field, cursor)) = Self::focused_field_and_cursor_mut(modal) {
                    Self::move_cursor_end(field, cursor);
                }
                cx.notify();
                return true;
            }
            "space" => {
                if let Some((field, cursor)) = Self::focused_field_and_cursor_mut(modal) {
                    Self::insert_at_cursor(field, cursor, " ");
                }
                modal.error = None;
                cx.notify();
                return true;
            }
            _ => {}
        }

        // Ctrl+A selects all (clears focused field for easy replacement)
        if (event.keystroke.modifiers.control || event.keystroke.modifiers.platform) && key == "a" {
            if let Some((field, cursor)) = Self::focused_field_and_cursor_mut(modal) {
                field.clear();
                *cursor = 0;
            }
            modal.error = None;
            cx.notify();
            return true;
        }

        // Ctrl+V / Cmd+V — paste from system clipboard
        if (event.keystroke.modifiers.control || event.keystroke.modifiers.platform) && key == "v" {
            if let Ok(codirigent_core::ClipboardContent::Text(text)) =
                self.clipboard.smart_clipboard.read_content()
            {
                if let Some((field, cursor)) = Self::focused_field_and_cursor_mut(modal) {
                    Self::insert_at_cursor(field, cursor, &text);
                }
                modal.error = None;
                cx.notify();
            }
            return true;
        }

        // Ignore other modifier-based shortcuts inside the modal.
        if event.keystroke.modifiers.control
            || event.keystroke.modifiers.alt
            || event.keystroke.modifiers.platform
        {
            return true;
        }

        if let Some(ref key_char) = event.keystroke.key_char {
            if !key_char.is_empty() {
                if let Some((field, cursor)) = Self::focused_field_and_cursor_mut(modal) {
                    Self::insert_at_cursor(field, cursor, key_char);
                    modal.error = None;
                    cx.notify();
                }
            }
        }

        true
    }
}
