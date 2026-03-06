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
use serde::Deserialize;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tracing::{info, warn};

/// Read the `permissionMode` value from the last complete JSON line of a
/// Claude Code JSONL session file.
///
/// Returns `None` if the file cannot be found or does not contain the field.
fn read_claude_permission_mode(claude_session_id: &str) -> Option<String> {
    let home = {
        #[cfg(target_os = "windows")]
        {
            std::env::var("USERPROFILE").ok().map(PathBuf::from)?
        }
        #[cfg(not(target_os = "windows"))]
        {
            std::env::var("HOME").ok().map(PathBuf::from)?
        }
    };
    let projects_dir = home.join(".claude").join("projects");
    // Search all project dirs for <claude_session_id>.jsonl
    let target = format!("{}.jsonl", claude_session_id);
    let entries = std::fs::read_dir(&projects_dir).ok()?;
    for project_entry in entries.flatten() {
        let jsonl_path = project_entry.path().join(&target);
        if !jsonl_path.exists() {
            continue;
        }
        // Scan lines in reverse to find the most recent entry that carries
        // permissionMode — not every line has this field.
        let content = std::fs::read_to_string(&jsonl_path).ok()?;
        for line in content.lines().rev() {
            if line.trim().is_empty() || !line.contains("permissionMode") {
                continue;
            }
            if let Ok(obj) = serde_json::from_str::<serde_json::Value>(line) {
                if let Some(mode) = obj.get("permissionMode").and_then(|v| v.as_str()) {
                    return Some(mode.to_owned());
                }
            }
        }
    }
    None
}

#[derive(Debug, Deserialize)]
struct CodexRolloutEntry {
    #[serde(rename = "type", default)]
    entry_type: String,
    #[serde(default)]
    payload: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct CodexSessionMeta {
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    approval_mode: Option<String>,
}

fn resolve_codex_home() -> Option<PathBuf> {
    if let Ok(home) = std::env::var("CODEX_HOME") {
        return Some(PathBuf::from(home));
    }
    dirs::home_dir().map(|home| home.join(".codex"))
}

fn read_first_line(path: &Path) -> Option<String> {
    let file = fs::File::open(path).ok()?;
    let mut reader = BufReader::new(file);
    let mut first = String::new();
    if reader.read_line(&mut first).ok()? == 0 {
        None
    } else {
        Some(first)
    }
}

fn read_codex_session_meta(path: &Path) -> Option<CodexSessionMeta> {
    let first_line = read_first_line(path)?;
    let entry: CodexRolloutEntry = serde_json::from_str(first_line.trim()).ok()?;
    if entry.entry_type != "session_meta" {
        return None;
    }
    let payload = entry.payload?;
    serde_json::from_value::<CodexSessionMeta>(payload).ok()
}

fn collect_codex_rollout_files(base: &Path, out: &mut Vec<(PathBuf, SystemTime)>) {
    let entries = match fs::read_dir(base) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_codex_rollout_files(&path, out);
            continue;
        }

        let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !file_name.starts_with("rollout-") || !file_name.ends_with(".jsonl") {
            continue;
        }

        let Ok(metadata) = path.metadata() else {
            continue;
        };
        let Ok(modified) = metadata.modified() else {
            continue;
        };
        out.push((path, modified));
    }
}

fn read_codex_permission_mode(working_directory: &Path, codex_session_id: &str) -> Option<String> {
    let codex_home = resolve_codex_home()?;
    let sessions_dir = codex_home.join("sessions");
    if !sessions_dir.is_dir() {
        return None;
    }

    let mut rollout_files = Vec::new();
    collect_codex_rollout_files(&sessions_dir, &mut rollout_files);
    if rollout_files.is_empty() {
        return None;
    }

    rollout_files.sort_unstable_by(|a, b| b.1.cmp(&a.1));

    if !codex_session_id.is_empty() {
        let session_suffix = format!("-{}.jsonl", codex_session_id);
        for (path, _) in &rollout_files {
            let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if !file_name.ends_with(&session_suffix) {
                continue;
            }
            if let Some(meta) = read_codex_session_meta(path) {
                if let Some(approval_mode) = meta.approval_mode {
                    return Some(approval_mode);
                }
            }
        }
    }

    let expected_cwd = working_directory.to_string_lossy();
    for (path, _) in &rollout_files {
        let Some(meta) = read_codex_session_meta(path) else {
            continue;
        };
        if meta.cwd.as_deref() == Some(expected_cwd.as_ref()) {
            return meta.approval_mode;
        }
    }

    None
}

fn is_codex_full_auto_mode(mode: &str) -> bool {
    mode.eq_ignore_ascii_case("full-auto")
        || mode.eq_ignore_ascii_case("full_auto")
        || mode.eq_ignore_ascii_case("fullauto")
        || mode.eq_ignore_ascii_case("yolo")
}

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

        let shell = self.configured_shell();

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
                let rows = (session_count as u32).div_ceil(cols);
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

            let shell = self.configured_shell();

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

            // Resume Claude Code session if we have a stored session ID.
            if let Some(ref claude_id) = saved.claude_session_id {
                let permission_mode = read_claude_permission_mode(claude_id).unwrap_or_default();
                let mut cmd = format!("claude --resume {}", claude_id);
                if permission_mode == "bypassPermissions" {
                    cmd.push_str(" --dangerously-skip-permissions");
                }
                cmd.push('\r');
                if let Ok(mgr) = self.session_manager.lock() {
                    if let Err(e) = mgr.send_input(session_id, cmd.as_bytes()) {
                        warn!(?session_id, error = %e, "Failed to send claude --resume command");
                    }
                }
            }

            // Resume Codex CLI session if we have a stored session ID.
            if let Some(ref codex_id) = saved.codex_session_id {
                let permission_mode = read_codex_permission_mode(&working_dir, codex_id);
                let mut cmd = format!("codex --session {}", codex_id);
                if permission_mode
                    .as_deref()
                    .is_some_and(|mode| is_codex_full_auto_mode(mode))
                {
                    cmd.push_str(" --full-auto");
                }
                cmd.push('\r');
                if let Ok(mgr) = self.session_manager.lock() {
                    if let Err(e) = mgr.send_input(session_id, cmd.as_bytes()) {
                        warn!(?session_id, error = %e, "Failed to send codex --session command");
                    }
                }
            }

            // Resume Gemini CLI session if we have a stored session ID.
            if let Some(ref gemini_id) = saved.gemini_session_id {
                let cmd = format!("gemini --resume {}\r", gemini_id);
                if let Ok(mgr) = self.session_manager.lock() {
                    if let Err(e) = mgr.send_input(session_id, cmd.as_bytes()) {
                        warn!(?session_id, error = %e, "Failed to send gemini --resume command");
                    }
                }
            }

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

    /// Return the configured shell, or `None` to use the system default.
    fn configured_shell(&self) -> Option<String> {
        self.settings
            .config_service
            .as_ref()
            .and_then(|cs| cs.load_user_settings().ok())
            .map(|s| s.general.default_shell)
            .filter(|s| !s.is_empty())
    }

    /// Close the focused session.
    pub fn close_focused_session(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.workspace.focused_session_id() {
            self.close_session(id, cx);
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
