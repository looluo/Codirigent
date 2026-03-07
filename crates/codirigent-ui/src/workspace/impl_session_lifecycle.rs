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
use codirigent_core::{
    CodexExecutionMode, CodirigentEvent, EventBus, ProcessMonitor, Session, SessionId,
    SessionManager, SessionStatus, SlotId,
};
use gpui::Context;
use serde::Deserialize;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tracing::{info, warn};

#[derive(Debug)]
struct RestoreSessionPlan {
    session_name: String,
    working_dir: PathBuf,
    group: Option<String>,
    color: Option<String>,
    claude_resume: Option<String>,
    codex_resume: Option<String>,
    codex_execution_mode: Option<CodexExecutionMode>,
    codex_started_at: Option<chrono::DateTime<chrono::Utc>>,
    gemini_resume: Option<String>,
}

#[derive(Debug)]
struct RestorePlan {
    sessions: Vec<RestoreSessionPlan>,
}

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
    #[serde(default)]
    sandbox_policy: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct CodexTurnContext {
    #[serde(default)]
    approval_policy: Option<String>,
    #[serde(default)]
    sandbox_policy: Option<serde_json::Value>,
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

fn read_codex_turn_context(path: &Path) -> Option<CodexTurnContext> {
    let file = fs::File::open(path).ok()?;
    let reader = BufReader::new(file);

    // Turn context appears near the beginning of each turn, so scanning a
    // small prefix keeps restore cheap even when rollout logs are long.
    for line in reader.lines().take(64).flatten() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let Ok(entry) = serde_json::from_str::<CodexRolloutEntry>(trimmed) else {
            continue;
        };
        if entry.entry_type != "turn_context" {
            continue;
        }

        let Some(payload) = entry.payload else {
            continue;
        };
        let Ok(turn_context) = serde_json::from_value::<CodexTurnContext>(payload) else {
            continue;
        };
        if turn_context.approval_policy.is_some() || turn_context.sandbox_policy.is_some() {
            return Some(turn_context);
        }
    }

    None
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

fn codex_execution_mode_from_approval_and_sandbox(
    approval_policy: Option<&str>,
    sandbox_policy: Option<&serde_json::Value>,
) -> Option<CodexExecutionMode> {
    if !approval_policy.is_some_and(|value| value.eq_ignore_ascii_case("never")) {
        return None;
    }

    let sandbox_policy_type = match sandbox_policy? {
        serde_json::Value::String(value) => Some(value.as_str()),
        serde_json::Value::Object(map) => map.get("type").and_then(serde_json::Value::as_str),
        _ => None,
    }?;

    if sandbox_policy_type.eq_ignore_ascii_case("danger-full-access") {
        Some(CodexExecutionMode::Bypass)
    } else if sandbox_policy_type.eq_ignore_ascii_case("workspace-write")
        || sandbox_policy_type.eq_ignore_ascii_case("workspace_write")
    {
        Some(CodexExecutionMode::FullAuto)
    } else {
        None
    }
}

fn read_codex_execution_mode(path: &Path) -> Option<CodexExecutionMode> {
    let meta = read_codex_session_meta(path)?;

    meta.approval_mode
        .as_deref()
        .and_then(codex_execution_mode_from_str)
        .or_else(|| {
            codex_execution_mode_from_approval_and_sandbox(None, meta.sandbox_policy.as_ref())
        })
        .or_else(|| {
            let turn_context = read_codex_turn_context(path)?;
            codex_execution_mode_from_approval_and_sandbox(
                turn_context.approval_policy.as_deref(),
                turn_context.sandbox_policy.as_ref(),
            )
        })
}

fn read_saved_codex_execution_mode(
    working_directory: &Path,
    codex_session_id: &str,
) -> Option<CodexExecutionMode> {
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
            if let Some(mode) = read_codex_execution_mode(path) {
                return Some(mode);
            }
        }
    }

    let expected_cwd = working_directory.to_string_lossy();
    for (path, _) in &rollout_files {
        let Some(meta) = read_codex_session_meta(path) else {
            continue;
        };
        if meta.cwd.as_deref() == Some(expected_cwd.as_ref()) {
            return read_codex_execution_mode(path);
        }
    }

    None
}

fn is_codex_full_auto_mode(mode: &str) -> bool {
    mode.eq_ignore_ascii_case("full-auto")
        || mode.eq_ignore_ascii_case("full_auto")
        || mode.eq_ignore_ascii_case("fullauto")
}

fn is_codex_bypass_mode(mode: &str) -> bool {
    mode.eq_ignore_ascii_case("yolo")
        || mode.eq_ignore_ascii_case("bypass")
        || mode.eq_ignore_ascii_case("dangerously-bypass-approvals-and-sandbox")
        || mode.eq_ignore_ascii_case("dangerously_bypass_approvals_and_sandbox")
}

fn codex_execution_mode_from_str(mode: &str) -> Option<CodexExecutionMode> {
    if is_codex_bypass_mode(mode) {
        Some(CodexExecutionMode::Bypass)
    } else if is_codex_full_auto_mode(mode) {
        Some(CodexExecutionMode::FullAuto)
    } else {
        None
    }
}

fn codex_resume_flag(mode: CodexExecutionMode) -> &'static str {
    match mode {
        CodexExecutionMode::FullAuto => "--full-auto",
        CodexExecutionMode::Bypass => "--dangerously-bypass-approvals-and-sandbox",
    }
}

fn resolve_saved_codex_execution_mode(
    saved_mode: Option<CodexExecutionMode>,
    working_dir: &Path,
    codex_session_id: &str,
) -> Option<CodexExecutionMode> {
    saved_mode.or_else(|| read_saved_codex_execution_mode(working_dir, codex_session_id))
}

fn build_codex_resume_command(codex_session_id: &str, mode: Option<CodexExecutionMode>) -> String {
    let mut cmd = format!("codex resume {}", codex_session_id);
    if let Some(mode) = mode {
        cmd.push(' ');
        cmd.push_str(codex_resume_flag(mode));
    }
    cmd.push('\r');
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn codex_resume_command_uses_resume_subcommand() {
        assert_eq!(
            build_codex_resume_command("session-123", None),
            "codex resume session-123\r"
        );
    }

    #[test]
    fn codex_resume_command_preserves_bypass_mode() {
        assert_eq!(
            build_codex_resume_command("session-123", Some(CodexExecutionMode::Bypass)),
            "codex resume session-123 --dangerously-bypass-approvals-and-sandbox\r"
        );
    }

    #[test]
    fn codex_resume_command_preserves_full_auto_mode() {
        assert_eq!(
            build_codex_resume_command("session-123", Some(CodexExecutionMode::FullAuto)),
            "codex resume session-123 --full-auto\r"
        );
    }

    #[test]
    fn codex_execution_mode_can_be_inferred_from_turn_context() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("rollout.jsonl");
        fs::write(
            &path,
            concat!(
                r#"{"type":"session_meta","payload":{"cwd":"/tmp"}}"#,
                "\n",
                r#"{"type":"turn_context","payload":{"approval_policy":"never","sandbox_policy":{"type":"danger-full-access"}}}"#
            ),
        )
        .unwrap();

        assert_eq!(
            read_codex_execution_mode(&path),
            Some(CodexExecutionMode::Bypass)
        );
    }
}

impl WorkspaceView {
    fn build_restore_plan(
        state: codirigent_core::AppState,
        fallback_dir: PathBuf,
    ) -> Option<RestorePlan> {
        if state.sessions.is_empty() {
            return None;
        }

        let mut used_names = std::collections::HashSet::new();
        let mut used_claude_ids: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        let mut used_codex_ids: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        let mut used_gemini_ids: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        let mut sessions = Vec::with_capacity(state.sessions.len());

        for saved in state.sessions {
            let working_dir = if saved.working_directory.exists() {
                saved.working_directory.clone()
            } else {
                fallback_dir.clone()
            };

            let mut session_name = saved.name.clone();
            if used_names.contains(&session_name) {
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

            let claude_resume = saved
                .claude_session_id
                .as_ref()
                .filter(|id| used_claude_ids.insert((*id).clone()))
                .map(|claude_id| {
                    let permission_mode =
                        read_claude_permission_mode(claude_id).unwrap_or_default();
                    let mut cmd = format!("claude --resume {}", claude_id);
                    if permission_mode == "bypassPermissions" {
                        cmd.push_str(" --dangerously-skip-permissions");
                    }
                    cmd.push('\r');
                    cmd
                });

            let codex_resume = saved
                .codex_session_id
                .as_ref()
                .filter(|id| used_codex_ids.insert((*id).clone()))
                .map(|codex_id| {
                    let mode = resolve_saved_codex_execution_mode(
                        saved.codex_execution_mode,
                        &working_dir,
                        codex_id,
                    );
                    build_codex_resume_command(codex_id, mode)
                });

            let gemini_resume = saved
                .gemini_session_id
                .as_ref()
                .filter(|id| used_gemini_ids.insert((*id).clone()))
                .map(|gemini_id| format!("gemini --resume {}\r", gemini_id));

            sessions.push(RestoreSessionPlan {
                session_name,
                working_dir,
                group: saved.group,
                color: saved.color,
                claude_resume,
                codex_resume,
                codex_execution_mode: saved.codex_execution_mode,
                codex_started_at: saved.codex_started_at,
                gemini_resume,
            });
        }

        Some(RestorePlan { sessions })
    }

    fn apply_restore_plan(
        &mut self,
        plan: RestorePlan,
        shell: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if plan.sessions.is_empty() {
            return;
        }

        info!(count = plan.sessions.len(), "Restoring sessions from disk");

        let session_count = plan.sessions.len();
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
            self.mark_layout_cache_dirty();
        }

        let restore_sessions = plan.sessions;
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let mut remaining = restore_sessions.into_iter().peekable();
            while remaining.peek().is_some() {
                let batch = remaining.by_ref().take(2).collect::<Vec<_>>();
                let is_last_batch = remaining.peek().is_none();
                let shell = shell.clone();

                let _ = this.update(cx, |this, cx| {
                    for plan in batch {
                        this.restore_session_from_plan(plan, shell.clone());
                    }

                    if is_last_batch {
                        if let Some(first_id) = this.workspace.sessions().first().map(|s| s.id) {
                            this.select_session_with_cx(first_id, cx);
                        }
                        info!("Session restoration complete");
                    }

                    this.mark_ui_sync_dirty();
                    cx.notify();
                });

                if !is_last_batch {
                    cx.background_executor()
                        .timer(Duration::from_millis(1))
                        .await;
                }
            }
        })
        .detach();
    }

    fn restore_session_from_plan(&mut self, plan: RestoreSessionPlan, shell: Option<String>) {
        let session_id = self.with_session_manager(|manager| {
            match manager.create_session(
                plan.session_name.clone(),
                plan.working_dir.clone(),
                shell.clone(),
            ) {
                Ok(id) => Some(id),
                Err(e) => {
                    warn!(name = %plan.session_name, "Failed to restore session: {}", e);
                    None
                }
            }
        });
        let session_id = match session_id {
            Some(id) => id,
            None => return,
        };

        if plan.codex_execution_mode.is_some() || plan.codex_started_at.is_some() {
            let codex_execution_mode = plan.codex_execution_mode;
            let codex_started_at = plan.codex_started_at;
            if let Ok(mgr) = self.session_manager.lock() {
                mgr.with_session_state_mut(session_id, |state| {
                    state.session.codex_execution_mode = codex_execution_mode;
                    state.session.codex_started_at = codex_started_at;
                });
            }
        }

        for cmd in [
            plan.claude_resume.as_deref(),
            plan.codex_resume.as_deref(),
            plan.gemini_resume.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            if let Ok(mgr) = self.session_manager.lock() {
                if let Err(e) = mgr.send_input(session_id, cmd.as_bytes()) {
                    warn!(?session_id, error = %e, "Failed to send resume command");
                }
            }
        }

        if plan.group.is_some() || plan.color.is_some() {
            self.with_session_manager(|manager| {
                let _ =
                    manager.set_session_group(session_id, plan.group.clone(), plan.color.clone());
            });
        }

        let child_pid = self.with_session_manager(|manager| manager.get_child_pid(session_id));
        if let Some(pid) = child_pid {
            self.with_detector(|detector| {
                let _ = detector.start_monitoring(session_id, pid);
            });
        }

        let (pty_tx, pty_rx) = tokio::sync::mpsc::unbounded_channel();
        let terminal = Terminal::new(24, 80, session_id, pty_tx);
        let theme = self.workspace.theme();
        let terminal_view = TerminalView::new(terminal, theme.clone());
        self.terminals.insert(session_id, terminal_view);
        self.pty_write_receivers.insert(session_id, pty_rx);

        let session = self.with_session_manager(|manager| {
            manager.get_session(session_id).unwrap_or_else(|| {
                Session::new(
                    session_id,
                    plan.session_name.clone(),
                    plan.working_dir.clone(),
                )
            })
        });

        if self.workspace.add_session(session.clone()) {
            self.mark_layout_cache_dirty();
            let mut header = TerminalHeader::new(&plan.session_name, SessionStatus::Idle);
            if let Some(ref gi) = session.git_info {
                header = header.with_git_info(gi.branch.clone(), gi.dirty_count);
            }
            if let Some(ref group) = plan.group {
                header.group_name = Some(group.clone());
            }
            if let Some(ref color) = plan.color {
                header.session_color = crate::sidebar::Color::from_hex(color);
            }
            self.terminal_headers.insert(session_id, header);
        }

        if let Some(ws_session) = self.workspace.session_mut(session_id) {
            ws_session.codex_execution_mode = plan.codex_execution_mode;
            ws_session.codex_started_at = plan.codex_started_at;
        }
    }

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
            .effective_user_settings()
            .general
            .default_working_dir
            .clone()
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
            self.mark_layout_cache_dirty();
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
            self.mark_ui_sync_dirty();
            self.save_state_to_disk(cx);
            cx.notify();
        }
    }

    /// Restore sessions from disk on startup without blocking the UI thread.
    pub(super) fn spawn_restore_sessions_from_disk(&mut self, cx: &mut Context<Self>) {
        if self.polling.restore_in_flight {
            return;
        }

        self.polling.restore_in_flight = true;
        let storage = self.persistence.storage.clone();
        let shell = self.configured_shell();
        let fallback_dir = self
            .project
            .project_root
            .clone()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."));

        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let restore_plan = cx
                .background_executor()
                .spawn(async move {
                    let state = match storage.load_state() {
                        Ok(state) => state,
                        Err(e) => {
                            info!("No saved state to restore: {}", e);
                            return None;
                        }
                    };
                    WorkspaceView::build_restore_plan(state, fallback_dir)
                })
                .await;

            let _ = this.update(cx, |this, cx| {
                this.polling.restore_in_flight = false;
                if let Some(plan) = restore_plan {
                    this.apply_restore_plan(plan, shell.clone(), cx);
                }
            });
        })
        .detach();
    }

    /// Return the configured shell, or `None` to use the system default.
    fn configured_shell(&self) -> Option<String> {
        let shell = self.effective_user_settings().general.default_shell.clone();
        if shell.is_empty() {
            None
        } else {
            Some(shell)
        }
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
        self.polling.shell_input_buffers.remove(&id);

        // Remove the terminal header for this session
        self.terminal_headers.remove(&id);

        // Remove from workspace
        self.workspace.remove_session(id);
        self.mark_layout_cache_dirty();
        self.event_bus
            .publish(CodirigentEvent::SessionClosed { id });
        info!(?id, "Closed session");
        self.mark_ui_sync_dirty();
        self.save_state_to_disk(cx);
        cx.notify();
    }
}
