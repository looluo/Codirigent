//! Task board event handlers for WorkspaceView.
//!
//! This module contains all methods related to:
//! - Task board event handling (add, start, review, complete, delete, assign)
//! - Task assignment to sessions
//! - Assignment confirmation and rejection

use super::cli_helpers::format_task_input;
use super::gpui::WorkspaceView;
use codirigent_core::{SessionId, SessionManager, TaskId};
use codirigent_session::clipboard_service::ClipboardService;
use gpui::Context;
use std::time::Instant;
use tracing::{info, warn};

impl WorkspaceView {
    pub(super) fn handle_task_board_event(
        &mut self,
        event: crate::task_board::TaskBoardEvent,
        cx: &mut Context<Self>,
    ) {
        use crate::task_board::TaskAction;

        match event {
            crate::task_board::TaskBoardEvent::TabSelected(tab) => {
                info!(?tab, "Task board tab selected");
            }
            crate::task_board::TaskBoardEvent::AutoAssignModeChanged(mode) => {
                info!(?mode, "Auto-assign mode changed");
                let (auto, confirm) = mode.to_config();
                if let Ok(mut manager) = self.task_manager.lock() {
                    manager.assignment_mut().set_auto_assign(auto);
                    manager.assignment_mut().set_confirm_before_assign(confirm);
                }
            }
            crate::task_board::TaskBoardEvent::AddTaskClicked => {
                info!("Add task clicked");
                self.open_task_creation_modal();
            }
            crate::task_board::TaskBoardEvent::TaskSelected(id) => {
                info!(%id, "Task selected");
            }
            crate::task_board::TaskBoardEvent::TaskAction { task_id, action } => {
                info!(%task_id, ?action, "Task action triggered");

                let task_id = TaskId::from(task_id);

                // Handle Edit outside the lock — it only needs to open a modal
                if matches!(action, TaskAction::Edit) {
                    info!("Edit action triggered for task {}", task_id);
                    self.open_task_edit_modal(&task_id);
                    cx.notify();
                    return;
                }

                let Ok(mut manager) = self.task_manager.lock() else {
                    warn!("Failed to lock task manager");
                    return;
                };
                let result = match action {
                    TaskAction::Start => {
                        info!("Starting task {}", task_id);
                        manager.start_task(&task_id)
                    }
                    TaskAction::Review => {
                        // Move to Review status and release from session
                        info!("Moving task {} to review", task_id);
                        let r = manager.move_to_review(&task_id);
                        let sid = r
                            .is_ok()
                            .then(|| self.session_with_task(&task_id))
                            .flatten();
                        drop(manager);
                        if let Some(sid) = sid {
                            self.clear_task_from_session(sid, cx);
                            return;
                        }
                        r
                    }
                    TaskAction::Complete => {
                        // Approve and complete the task, releasing it from its session
                        info!("Approving task {}", task_id);
                        let r = manager.approve_task(&task_id);
                        let sid = r
                            .is_ok()
                            .then(|| self.session_with_task(&task_id))
                            .flatten();
                        drop(manager);
                        if let Some(sid) = sid {
                            self.clear_task_from_session(sid, cx);
                            return;
                        }
                        r
                    }
                    TaskAction::Delete => {
                        info!("Deleting task {}", task_id);
                        let r = manager.delete_task(&task_id);
                        let sid = r
                            .is_ok()
                            .then(|| self.session_with_task(&task_id))
                            .flatten();
                        drop(manager);
                        if let Some(sid) = sid {
                            self.clear_task_from_session(sid, cx);
                            return;
                        }
                        r
                    }
                    TaskAction::Assign => {
                        info!("Assign action triggered for task {}", task_id);
                        let task = manager.get_task(&task_id).cloned();
                        let target_id = task
                            .as_ref()
                            .and_then(|t| self.find_assignable_session_for_task(t));

                        if let Some(sid) = target_id {
                            match manager.direct_assign(&task_id, sid) {
                                Ok(prompt) => {
                                    drop(manager);
                                    self.send_task_to_session(&task_id, sid, &prompt);
                                    self.cache.manually_assigned_sessions.insert(sid);
                                    info!("Manually assigned task {} to session {}", task_id, sid);
                                    self.sync_task_derived_state();
                                    cx.notify();
                                    return;
                                }
                                Err(e) => {
                                    warn!("Failed to assign task: {}", e);
                                    Err(e)
                                }
                            }
                        } else {
                            warn!("No matching session available for assignment (check directory matching)");
                            Ok(())
                        }
                    }
                    TaskAction::Edit => {
                        // Handled above, before the lock
                        unreachable!()
                    }
                };

                if let Err(e) = result {
                    warn!("Task action failed: {}", e);
                }
            }
            crate::task_board::TaskBoardEvent::ConfirmAssignment { task_id } => {
                info!(%task_id, "Confirming pending assignment");
                let task_id = TaskId::from(task_id);

                // Confirm the pending assignment and get the prompt
                let (prompt, session_id) = {
                    let mut manager = match self.task_manager.lock() {
                        Ok(m) => m,
                        Err(_) => {
                            cx.notify();
                            return;
                        }
                    };
                    match manager.assignment_mut().confirm_assignment(&task_id) {
                        Ok(assignment) => {
                            // Also assign the task in the queue
                            if let Err(e) = manager
                                .queue_mut()
                                .assign_task(&task_id, assignment.session_id)
                            {
                                warn!("Failed to assign task in queue: {}", e);
                                cx.notify();
                                return;
                            }
                            (assignment.prompt, assignment.session_id)
                        }
                        Err(e) => {
                            warn!("Failed to confirm assignment: {}", e);
                            cx.notify();
                            return;
                        }
                    }
                };

                self.send_task_to_session(&task_id, session_id, &prompt);
                info!(?task_id, ?session_id, "Confirmed and sent task to session");
            }
            crate::task_board::TaskBoardEvent::RejectAssignment { task_id } => {
                info!(%task_id, "Rejecting pending assignment");
                let task_id = TaskId::from(task_id);
                if let Ok(mut manager) = self.task_manager.lock() {
                    manager.assignment_mut().reject_assignment(&task_id);
                }
                info!(?task_id, "Rejected assignment — task remains queued");
            }
        }
        self.sync_task_derived_state();
        cx.notify();
    }

    /// Send a task prompt to a session: updates `current_task` in both the
    /// session manager and the workspace cache, formats and sends the PTY input,
    /// and inserts a deferred Enter keypress.
    ///
    /// Must be called **after** any task-manager lock has been dropped.
    pub(super) fn send_task_to_session(
        &mut self,
        task_id: &TaskId,
        session_id: codirigent_core::SessionId,
        prompt: &str,
    ) {
        let cli_type = self
            .clipboard
            .clipboard_service
            .get_session_cli_type(session_id);
        let input = format_task_input(prompt, cli_type);
        if let Ok(mgr) = self.session_manager.lock() {
            mgr.with_session_state_mut(session_id, |state| {
                state.session.current_task = Some(task_id.clone());
            });
            if let Err(e) = mgr.send_input(session_id, input.as_bytes()) {
                warn!(
                    "Failed to send task prompt to session {}: {}",
                    session_id, e
                );
            }
        }
        if let Some(ws_session) = self.workspace.session_mut(session_id) {
            ws_session.current_task = Some(task_id.clone());
        }
        self.polling
            .pending_enters
            .insert(session_id, (Instant::now(), false));
        self.sync_task_derived_state();
    }

    /// Find the best assignable session for a task, returning only its ID (no clone).
    fn find_assignable_session_for_task(&self, task: &codirigent_core::Task) -> Option<SessionId> {
        let focused_id = self.workspace.focused_session_id();

        let mut best: Option<(SessionId, f32)> = None;
        for s in self.workspace.sessions().iter().filter(|s| {
            s.status == codirigent_core::SessionStatus::Idle
                && s.current_task.is_none()
                && self.clipboard.clipboard_service.get_session_cli_type(s.id)
                    != codirigent_core::CliType::GenericShell
                && task.project_dir.as_ref().map_or(true, |pd| {
                    codirigent_core::session_matches_project(&s.working_directory, pd)
                })
        }) {
            // Focused session wins immediately
            if Some(s.id) == focused_id {
                return Some(s.id);
            }
            let usage = s.context_usage.unwrap_or(0.0);
            if best
                .as_ref()
                .map_or(true, |&(_, best_usage)| usage < best_usage)
            {
                best = Some((s.id, usage));
            }
        }
        best.map(|(id, _)| id)
    }

    /// Clear `current_task` from a session after a task action completes.
    ///
    /// Updates both the in-memory workspace view and the persisted session state
    /// managed by the session manager. Must be called **after** dropping the
    /// task manager lock to avoid lock-order issues.
    fn clear_task_from_session(&mut self, sid: codirigent_core::SessionId, cx: &mut Context<Self>) {
        if let Ok(mgr) = self.session_manager.lock() {
            mgr.with_session_state_mut(sid, |state| {
                state.session.current_task = None;
            });
        }
        if let Some(session) = self.workspace.session_mut(sid) {
            session.current_task = None;
        }
        self.sync_task_derived_state();
        cx.notify();
    }

    /// Find the ID of the session currently running the given task.
    ///
    /// Used by task action handlers to locate which session to release after
    /// a Review, Complete, or Delete action succeeds.
    fn session_with_task(&self, task_id: &TaskId) -> Option<codirigent_core::SessionId> {
        self.workspace
            .sessions()
            .iter()
            .find(|s| s.current_task.as_ref() == Some(task_id))
            .map(|s| s.id)
    }
}
