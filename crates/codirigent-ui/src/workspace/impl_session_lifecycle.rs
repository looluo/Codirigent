//! Session lifecycle management for WorkspaceView.
//!
//! This module contains all methods related to:
//! - Session creation (create, create_at, create_in_slot)
//! - Session restoration from disk
//! - Session closure (close, close_focused)
//! - State persistence to disk

use super::gpui::WorkspaceView;
use super::types::SESSION_NAME_PREFIX;
use crate::terminal::Terminal;
use crate::terminal_header::TerminalHeader;
use crate::terminal_view::TerminalView;
use codirigent_core::config_service::ConfigService;
use codirigent_core::{
    CodirigentEvent, EventBus, ProcessMonitor, Session, SessionId, SessionManager, SessionStatus,
    SlotId,
};
use gpui::Context;
use std::path::PathBuf;
use tracing::{info, warn};

impl WorkspaceView {
    /// Create a new terminal session in the focused pane.
    pub fn create_session(&mut self, cx: &mut Context<Self>) {
        self.create_session_inner(None, cx);
    }

    /// Create a new session in a specific split tree slot.
    pub fn create_session_in_slot(&mut self, slot: SlotId, cx: &mut Context<Self>) {
        self.create_session_inner(Some(slot), cx);
    }

    /// Shared implementation for session creation.
    /// When `target_slot` is `None`, adds to the first available slot;
    /// when `Some(slot)`, adds to that specific slot.
    fn create_session_inner(&mut self, target_slot: Option<SlotId>, cx: &mut Context<Self>) {
        // Find the lowest available session number (reuse gaps from closed sessions)
        let existing_numbers: std::collections::HashSet<u64> = self
            .workspace
            .sessions()
            .iter()
            .filter_map(|s| {
                s.name
                    .strip_prefix(SESSION_NAME_PREFIX)
                    .and_then(|n| n.parse::<u64>().ok())
            })
            .collect();
        let mut num = 1u64;
        while existing_numbers.contains(&num) {
            num += 1;
        }
        let name = format!("{}{}", SESSION_NAME_PREFIX, num);
        self.next_session_id = num + 1;

        let working_dir = self
            .settings
            .config_service
            .as_ref()
            .and_then(|cs| cs.load_user_settings().ok())
            .and_then(|s| s.general.default_working_dir)
            .map(PathBuf::from)
            .filter(|p| p.is_dir())
            .or_else(|| self.project.project_root.clone())
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("/tmp"));

        // Determine shell from user settings (empty string = auto-detect)
        let shell = self
            .settings
            .config_service
            .as_ref()
            .and_then(|cs| cs.load_user_settings().ok())
            .map(|s| s.general.default_shell)
            .filter(|s| !s.is_empty());

        // Create session with real PTY via session manager
        let session_id = self.with_session_manager(|manager| {
            match manager.create_session(name.clone(), working_dir.clone(), shell) {
                Ok(id) => Some(id),
                Err(e) => {
                    warn!("Failed to create session: {}", e);
                    None
                }
            }
        });
        let session_id = match session_id {
            Some(id) => id,
            None => return,
        };

        // Get child PID for monitoring
        let child_pid = self.with_session_manager(|manager| manager.get_child_pid(session_id));

        // Start monitoring session status
        if let Some(pid) = child_pid {
            self.with_detector(|detector| {
                if let Err(e) = detector.start_monitoring(session_id, pid) {
                    warn!("Failed to start monitoring session {}: {}", session_id, e);
                }
            });
        }

        // Create terminal emulator for this session with PTY writer channel
        // so VTE can forward protocol responses (e.g. DSR cursor position) back
        let (pty_tx, pty_rx) = tokio::sync::mpsc::unbounded_channel();
        let terminal = Terminal::new(24, 80, session_id, pty_tx);
        let theme = self.workspace.theme();
        let terminal_view = TerminalView::new(terminal, theme.clone());
        self.terminals.insert(session_id, terminal_view);
        self.pty_write_receivers.insert(session_id, pty_rx);

        // Get session from manager (has git_info populated during creation)
        let session = self.with_session_manager(|manager| {
            manager
                .get_session(session_id)
                .unwrap_or_else(|| Session::new(session_id, name.clone(), working_dir))
        });

        let added = match target_slot {
            Some(slot) => self.workspace.add_session_to_slot(session.clone(), slot),
            None => self.workspace.add_session(session.clone()),
        };

        if added {
            let mut header = TerminalHeader::new(&name, SessionStatus::Idle);

            // Populate git info on header if available from session manager
            if let Some(ref gi) = session.git_info {
                header = header.with_git_info(gi.branch.clone(), gi.dirty_count);
            }

            // Populate project name from git repo root or working directory
            let dir_name = session
                .git_info
                .as_ref()
                .and_then(|gi| gi.repo_root.file_name())
                .or_else(|| session.working_directory.file_name())
                .and_then(|n| n.to_str())
                .unwrap_or("unknown");
            header = header.with_project_name(dir_name);

            self.terminal_headers.insert(session_id, header);

            // Auto-select the newly created session for natural UX
            self.select_session_with_cx(session_id, cx);

            if let Some(slot) = target_slot {
                info!(%name, ?slot, "Created new session in slot with PTY");
            } else {
                info!(%name, "Created new session with PTY");
            }
            self.save_state_to_disk();
            cx.notify();
        }
    }

    /// Restore sessions from disk on startup.
    pub(super) fn restore_sessions_from_disk(&mut self, cx: &mut Context<Self>) {
        let state = match self.persistence.storage.load_state() {
            Ok(state) => state,
            Err(e) => {
                info!("No saved state to restore: {}", e);
                return;
            }
        };

        if state.sessions.is_empty() {
            return;
        }

        info!(count = state.sessions.len(), "Restoring sessions from disk");

        // Auto-expand grid layout if needed to accommodate all sessions
        let session_count = state.sessions.len();
        let current_max = self.workspace.layout_profile().max_sessions();

        if session_count > current_max {
            use crate::layout::LayoutProfile;
            let new_profile = if session_count <= 4 {
                LayoutProfile::Grid2x2
            } else if session_count <= 6 {
                LayoutProfile::Grid2x3
            } else if session_count <= 9 {
                LayoutProfile::Grid3x3
            } else {
                // For more than 9 sessions, create a custom grid
                let cols = 4;
                let rows = (session_count as u32 + cols - 1) / cols; // Ceiling division
                LayoutProfile::Custom { rows, cols }
            };

            info!(
                "Auto-expanding layout from {} to {} cells to fit {} sessions",
                current_max,
                new_profile.max_sessions(),
                session_count
            );
            self.workspace.set_layout(new_profile);
        }

        // Track used session names to ensure uniqueness during restoration
        let mut used_names = std::collections::HashSet::new();

        for saved in &state.sessions {
            let working_dir = if saved.working_directory.exists() {
                saved.working_directory.clone()
            } else {
                self.project
                    .project_root
                    .clone()
                    .or_else(|| std::env::current_dir().ok())
                    .unwrap_or_else(|| PathBuf::from("."))
            };

            // Determine shell from user settings (empty string = auto-detect)
            let shell = self
                .settings
                .config_service
                .as_ref()
                .and_then(|cs| cs.load_user_settings().ok())
                .map(|s| s.general.default_shell)
                .filter(|s| !s.is_empty());

            // Ensure session name is unique
            let mut session_name = saved.name.clone();
            if used_names.contains(&session_name) {
                // Find next available number suffix
                let mut counter = 2;
                loop {
                    let candidate = format!("{} ({})", saved.name, counter);
                    if !used_names.contains(&candidate) {
                        session_name = candidate;
                        break;
                    }
                    counter += 1;
                }
                warn!(
                    original = %saved.name,
                    renamed = %session_name,
                    "Renamed duplicate session name during restoration"
                );
            }
            used_names.insert(session_name.clone());

            let session_id = self.with_session_manager(|manager| {
                match manager.create_session(session_name.clone(), working_dir.clone(), shell) {
                    Ok(id) => Some(id),
                    Err(e) => {
                        warn!(name = %session_name, "Failed to restore session: {}", e);
                        None
                    }
                }
            });
            let session_id = match session_id {
                Some(id) => id,
                None => continue,
            };

            // Restore group/color
            if saved.group.is_some() || saved.color.is_some() {
                self.with_session_manager(|manager| {
                    let _ = manager.set_session_group(
                        session_id,
                        saved.group.clone(),
                        saved.color.clone(),
                    );
                });
            }

            // Start monitoring
            let child_pid = self.with_session_manager(|manager| manager.get_child_pid(session_id));
            if let Some(pid) = child_pid {
                self.with_detector(|detector| {
                    let _ = detector.start_monitoring(session_id, pid);
                });
            }

            // Create terminal view with PTY writer channel for VTE responses
            let (pty_tx, pty_rx) = tokio::sync::mpsc::unbounded_channel();
            let terminal = Terminal::new(24, 80, session_id, pty_tx);
            let theme = self.workspace.theme();
            let terminal_view = TerminalView::new(terminal, theme.clone());
            self.terminals.insert(session_id, terminal_view);
            self.pty_write_receivers.insert(session_id, pty_rx);

            // Get session from manager (has git_info)
            let session = self.with_session_manager(|manager| {
                manager
                    .get_session(session_id)
                    .unwrap_or_else(|| Session::new(session_id, session_name.clone(), working_dir))
            });

            if self.workspace.add_session(session.clone()) {
                let mut header = TerminalHeader::new(&session_name, SessionStatus::Idle);
                if let Some(ref gi) = session.git_info {
                    header = header.with_git_info(gi.branch.clone(), gi.dirty_count);
                }
                if let Some(ref group) = saved.group {
                    header.group_name = Some(group.clone());
                }
                if let Some(ref color) = saved.color {
                    header.session_color = crate::sidebar::Color::from_hex(color);
                }
                self.terminal_headers.insert(session_id, header);
            }
        }

        // Select the first restored session
        if let Some(first_id) = self.workspace.sessions().first().map(|s| s.id) {
            self.select_session_with_cx(first_id, cx);
        }

        info!("Session restoration complete");
        cx.notify();
    }

    /// Close the focused session.
    pub fn close_focused_session(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.workspace.focused_session_id() {
            // Stop monitoring (from main branch)
            self.with_detector(|detector| {
                detector.stop_monitoring(id);
            });

            // Remove terminal view and PTY writer (from main branch)
            self.terminals.remove(&id);
            self.pty_write_receivers.remove(&id);

            // Close PTY session (from main branch)
            self.with_session_manager(|manager| {
                if let Err(e) = manager.close_session(id) {
                    warn!("Failed to close session {}: {}", id, e);
                }
            });

            // Clean up compaction state
            if let Ok(mut svc) = self.persistence.compaction.lock() {
                svc.end_compaction(id);
            }
            self.cache.compaction_start_times.remove(&id);
            if let Ok(mut readers) = self.cli_readers.lock() {
                readers.cached_status.remove(&id);
            }

            // Remove the terminal header for this session (from feature branch)
            self.terminal_headers.remove(&id);

            // Remove from workspace UI
            self.workspace.remove_session(id);
            info!(?id, "Closed session");
            self.save_state_to_disk();
            cx.notify();
        }
    }

    /// Close a specific session by ID.
    pub fn close_session(&mut self, id: SessionId, cx: &mut Context<Self>) {
        // Stop monitoring
        self.with_detector(|detector| {
            detector.stop_monitoring(id);
        });

        // Remove terminal view and PTY writer
        self.terminals.remove(&id);
        self.pty_write_receivers.remove(&id);

        // Close PTY session
        self.with_session_manager(|manager| {
            if let Err(e) = manager.close_session(id) {
                warn!("Failed to close session {}: {}", id, e);
            }
        });

        // Clean up compaction state
        if let Ok(mut svc) = self.persistence.compaction.lock() {
            svc.end_compaction(id);
        }
        self.cache.compaction_start_times.remove(&id);
        if let Ok(mut readers) = self.cli_readers.lock() {
            readers.cached_status.remove(&id);
        }

        // Remove the terminal header for this session
        self.terminal_headers.remove(&id);

        // Remove from workspace
        self.workspace.remove_session(id);
        self.event_bus
            .publish(CodirigentEvent::SessionClosed { id });
        info!(?id, "Closed session");
        self.save_state_to_disk();
        cx.notify();
    }
}
