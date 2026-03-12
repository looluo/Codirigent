//! Session lifecycle management for WorkspaceView.
//!
//! This module contains all methods related to:
//! - Session creation (create, create_at, create_in_slot)
//! - Session restoration from disk
//! - Session closure (close, close_focused)
//! - State persistence to disk

use super::cli_helpers::is_safe_cli_session_id;
use super::gpui::WorkspaceView;
use super::types::SESSION_NAME_PREFIX;
use crate::terminal::Terminal;
use crate::terminal_header::TerminalHeader;
use crate::terminal_view::TerminalView;
use codirigent_core::{
    CodexExecutionMode, CodirigentEvent, EventBus, GridPosition, LayoutMode, ProcessMonitor,
    Session, SessionId, SessionManager, SessionStatus, SlotId,
};
use codirigent_session::DefaultSessionManager;
use gpui::Context;
use serde::Deserialize;
use std::collections::HashSet;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
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
    layout: LayoutMode,
    sessions: Vec<RestoreSessionPlan>,
}

#[derive(Debug)]
struct SessionBootstrapRequest {
    session_name: String,
    working_dir: PathBuf,
    shell: Option<String>,
}

#[derive(Debug)]
struct SessionBootstrapResult {
    request: SessionBootstrapRequest,
    session_id: SessionId,
    session: Session,
    child_pid: Option<u32>,
}

#[derive(Debug)]
struct CompletedRestoreBootstrap {
    plan: RestoreSessionPlan,
    result: Result<SessionBootstrapResult, String>,
}

fn next_available_session_number(
    existing_sessions: &[Session],
    reserved_numbers: &HashSet<u64>,
) -> u64 {
    let existing_numbers: HashSet<u64> = existing_sessions
        .iter()
        .filter_map(|session| {
            session
                .name
                .strip_prefix(SESSION_NAME_PREFIX)
                .and_then(|number| number.parse::<u64>().ok())
        })
        .collect();

    let mut num = 1u64;
    while existing_numbers.contains(&num) || reserved_numbers.contains(&num) {
        num += 1;
    }
    num
}

fn restore_resume_commands(plan: &RestoreSessionPlan) -> Vec<&str> {
    [
        plan.claude_resume.as_deref(),
        plan.codex_resume.as_deref(),
        plan.gemini_resume.as_deref(),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn bootstrap_session(
    session_manager: Arc<Mutex<DefaultSessionManager>>,
    request: SessionBootstrapRequest,
) -> Result<SessionBootstrapResult, String> {
    let manager = session_manager
        .lock()
        .map_err(|_| "session manager mutex poisoned".to_string())?;

    let session_id = manager
        .create_session(
            request.session_name.clone(),
            request.working_dir.clone(),
            request.shell.clone(),
        )
        .map_err(|error| error.to_string())?;

    let child_pid = manager.get_child_pid(session_id);
    let session = manager.get_session(session_id).unwrap_or_else(|| {
        Session::new(
            session_id,
            request.session_name.clone(),
            request.working_dir.clone(),
        )
    });

    Ok(SessionBootstrapResult {
        request,
        session_id,
        session,
        child_pid,
    })
}

fn layout_profile_for_restore(layout: &LayoutMode) -> Option<crate::layout::LayoutProfile> {
    if let Some(profile) = crate::layout::LayoutProfile::from_mode(layout) {
        return Some(profile);
    }

    match layout {
        LayoutMode::Custom { positions } => custom_positions_layout_profile(positions),
        _ => None,
    }
}

fn custom_positions_layout_profile(
    positions: &[(SessionId, GridPosition)],
) -> Option<crate::layout::LayoutProfile> {
    let max_row = positions.iter().map(|(_, position)| position.row).max()?;
    let max_col = positions.iter().map(|(_, position)| position.col).max()?;
    crate::layout::LayoutProfile::custom(max_row + 1, max_col + 1)
}

fn restore_layout_capacity(layout: &LayoutMode) -> usize {
    match layout {
        LayoutMode::Grid { rows, cols } => (*rows as usize) * (*cols as usize),
        LayoutMode::Single => 1,
        LayoutMode::Custom { positions } => layout_profile_for_restore(layout)
            .map(|profile| profile.max_sessions())
            .unwrap_or_else(|| positions.len()),
        LayoutMode::SplitTree { root } => root.leaf_count(),
    }
}

fn expanded_restore_layout(session_count: usize) -> LayoutMode {
    if session_count <= 1 {
        LayoutMode::Single
    } else if session_count <= 4 {
        LayoutMode::Grid { rows: 2, cols: 2 }
    } else if session_count <= 6 {
        LayoutMode::Grid { rows: 2, cols: 3 }
    } else if session_count <= 9 {
        LayoutMode::Grid { rows: 3, cols: 3 }
    } else {
        let cols = 4u32;
        let rows = (session_count as u32).div_ceil(cols);
        LayoutMode::Grid { rows, cols }
    }
}

fn staging_layout_for_restore(saved_layout: &LayoutMode, session_count: usize) -> LayoutMode {
    if restore_layout_capacity(saved_layout) >= session_count {
        saved_layout.clone()
    } else {
        expanded_restore_layout(session_count)
    }
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

fn build_codex_resume_command(
    codex_session_id: &str,
    mode: Option<CodexExecutionMode>,
) -> Option<String> {
    if !is_safe_cli_session_id(codex_session_id) {
        warn!(
            session_id = %codex_session_id,
            "Ignoring unsafe persisted Codex session ID during restore"
        );
        return None;
    }

    let mut cmd = format!("codex resume {}", codex_session_id);
    if let Some(mode) = mode {
        cmd.push(' ');
        cmd.push_str(codex_resume_flag(mode));
    }
    cmd.push('\r');
    Some(cmd)
}

fn build_resume_command(program: &str, session_id: &str, extra_args: &[&str]) -> Option<String> {
    if !is_safe_cli_session_id(session_id) {
        warn!(
            program,
            session_id = %session_id,
            "Ignoring unsafe persisted CLI session ID during restore"
        );
        return None;
    }

    let mut cmd = format!("{} --resume {}", program, session_id);
    for arg in extra_args {
        cmd.push(' ');
        cmd.push_str(arg);
    }
    cmd.push('\r');
    Some(cmd)
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;
    use codirigent_core::{
        AppState, DefaultEventBus, LayoutNode, Session, SessionId, SessionManager, SplitDirection,
    };
    use codirigent_session::{normalize_path, DefaultSessionManager};
    use std::collections::HashSet;
    use std::sync::{Arc, Mutex};
    use tempfile::TempDir;

    fn create_test_session_manager() -> Arc<Mutex<DefaultSessionManager>> {
        Arc::new(Mutex::new(DefaultSessionManager::new(Arc::new(
            DefaultEventBus::new(16),
        ))))
    }

    fn sample_working_dir() -> PathBuf {
        std::env::temp_dir()
    }

    #[test]
    fn next_available_session_number_skips_existing_and_reserved_values() {
        let sessions = vec![
            Session::new(SessionId(1), "Session 1".to_string(), sample_working_dir()),
            Session::new(SessionId(2), "Session 3".to_string(), sample_working_dir()),
        ];
        let reserved = HashSet::from([2u64, 4u64]);

        assert_eq!(next_available_session_number(&sessions, &reserved), 5);
    }

    #[test]
    fn restore_resume_commands_preserve_cli_order() {
        let plan = RestoreSessionPlan {
            session_name: "Session 1".to_string(),
            working_dir: sample_working_dir(),
            group: None,
            color: None,
            claude_resume: Some("claude --resume abc\r".to_string()),
            codex_resume: Some("codex resume def\r".to_string()),
            codex_execution_mode: None,
            codex_started_at: None,
            gemini_resume: Some("gemini --resume ghi\r".to_string()),
        };

        assert_eq!(
            restore_resume_commands(&plan),
            vec![
                "claude --resume abc\r",
                "codex resume def\r",
                "gemini --resume ghi\r",
            ]
        );
    }

    #[test]
    fn bootstrap_session_returns_session_metadata() {
        let session_manager = create_test_session_manager();
        let temp = TempDir::new().unwrap();
        let request = SessionBootstrapRequest {
            session_name: "Session 1".to_string(),
            working_dir: temp.path().to_path_buf(),
            shell: None,
        };

        let result = bootstrap_session(session_manager.clone(), request).unwrap();

        assert_eq!(result.session.name, "Session 1");
        assert_eq!(
            result.session.working_directory,
            normalize_path(temp.path())
        );
        assert_eq!(result.request.session_name, "Session 1");
        assert!(
            result.child_pid.is_some(),
            "bootstrap should capture a PTY child pid"
        );
        let manager = session_manager.lock().unwrap_or_else(|p| p.into_inner());
        assert!(manager.get_session(result.session_id).is_some());
    }

    #[test]
    fn bootstrap_session_invalid_working_directory_returns_error_without_creating_session() {
        let session_manager = create_test_session_manager();
        let temp = TempDir::new().unwrap();
        let request = SessionBootstrapRequest {
            session_name: "Session 1".to_string(),
            working_dir: temp.path().join("missing-session-bootstrap"),
            shell: None,
        };

        let result = bootstrap_session(session_manager.clone(), request);
        assert!(result.is_err());

        let manager = session_manager.lock().unwrap_or_else(|p| p.into_inner());
        assert_eq!(manager.session_count(), 0);
    }

    #[test]
    fn codex_resume_command_uses_resume_subcommand() {
        assert_eq!(
            build_codex_resume_command("session-123", None),
            Some("codex resume session-123\r".to_string())
        );
    }

    #[test]
    fn codex_resume_command_preserves_bypass_mode() {
        assert_eq!(
            build_codex_resume_command("session-123", Some(CodexExecutionMode::Bypass)),
            Some(
                "codex resume session-123 --dangerously-bypass-approvals-and-sandbox\r".to_string()
            )
        );
    }

    #[test]
    fn codex_resume_command_preserves_full_auto_mode() {
        assert_eq!(
            build_codex_resume_command("session-123", Some(CodexExecutionMode::FullAuto)),
            Some("codex resume session-123 --full-auto\r".to_string())
        );
    }

    #[test]
    fn codex_resume_command_rejects_unsafe_session_id() {
        assert_eq!(build_codex_resume_command("session-123;rm", None), None);
    }

    #[test]
    fn codex_execution_mode_can_be_inferred_from_turn_context() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("rollout.jsonl");
        let cwd = tmp.path().display().to_string();
        let session_meta = serde_json::json!({
            "type": "session_meta",
            "payload": { "cwd": cwd },
        });
        let turn_context = serde_json::json!({
            "type": "turn_context",
            "payload": {
                "approval_policy": "never",
                "sandbox_policy": { "type": "danger-full-access" },
            },
        });
        fs::write(&path, format!("{session_meta}\n{turn_context}")).unwrap();

        assert_eq!(
            read_codex_execution_mode(&path),
            Some(CodexExecutionMode::Bypass)
        );
    }

    #[test]
    fn build_restore_plan_preserves_saved_custom_grid_layout() {
        let fallback_dir = sample_working_dir();
        let state = AppState {
            sessions: vec![Session::new(
                SessionId(1),
                "Session 1".to_string(),
                fallback_dir.clone(),
            )],
            layout: LayoutMode::Grid { rows: 1, cols: 4 },
            updated_at: None,
            window_bounds: None,
        };

        let plan = WorkspaceView::build_restore_plan(state, fallback_dir).unwrap();
        assert_eq!(plan.layout, LayoutMode::Grid { rows: 1, cols: 4 });
    }

    #[test]
    fn build_restore_plan_preserves_saved_split_tree_layout() {
        let fallback_dir = sample_working_dir();
        let split_tree = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf { slot: SlotId(0) }),
            second: Box::new(LayoutNode::Leaf { slot: SlotId(1) }),
        };
        let state = AppState {
            sessions: vec![Session::new(
                SessionId(1),
                "Session 1".to_string(),
                fallback_dir.clone(),
            )],
            layout: LayoutMode::SplitTree {
                root: split_tree.clone(),
            },
            updated_at: None,
            window_bounds: None,
        };

        let plan = WorkspaceView::build_restore_plan(state, fallback_dir).unwrap();
        assert_eq!(
            plan.layout,
            LayoutMode::SplitTree {
                root: split_tree.clone(),
            }
        );
        assert_eq!(
            restore_layout_capacity(&plan.layout),
            split_tree.leaf_count()
        );
    }

    #[test]
    fn staging_layout_for_restore_expands_when_saved_layout_is_too_small() {
        assert_eq!(
            staging_layout_for_restore(&LayoutMode::Single, 3),
            LayoutMode::Grid { rows: 2, cols: 2 }
        );
    }

    #[test]
    fn layout_profile_for_restore_supports_legacy_custom_positions() {
        let profile = layout_profile_for_restore(&LayoutMode::Custom {
            positions: vec![
                (SessionId(1), GridPosition { row: 0, col: 0 }),
                (SessionId(2), GridPosition { row: 1, col: 2 }),
            ],
        });

        assert_eq!(
            profile,
            Some(crate::layout::LayoutProfile::Custom { rows: 2, cols: 3 })
        );
    }
}

impl WorkspaceView {
    fn release_session_create_reservation(
        &mut self,
        reserved_number: u64,
        target_slot: Option<SlotId>,
    ) {
        self.polling
            .pending_session_bootstrap_numbers
            .remove(&reserved_number);
        if let Some(slot) = target_slot {
            self.polling.pending_session_bootstrap_slots.remove(&slot);
        }
    }

    fn build_terminal_header(
        &self,
        session: &Session,
        session_name: &str,
        group: Option<&String>,
        color: Option<&String>,
    ) -> TerminalHeader {
        let mut header = TerminalHeader::new(session_name, SessionStatus::Idle);
        if let Some(ref git_info) = session.git_info {
            header = header.with_git_info(git_info.branch.clone(), git_info.dirty_count);
        }

        let dir_name = session
            .git_info
            .as_ref()
            .and_then(|git_info| git_info.repo_root.file_name())
            .or_else(|| session.working_directory.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("unknown");
        header = header.with_project_name(dir_name);

        if let Some(group) = group {
            header.group_name = Some(group.clone());
        }
        if let Some(color) = color {
            header.session_color = crate::sidebar::Color::from_hex(color);
        }

        header
    }

    fn create_terminal_view_for_session(&mut self, session_id: SessionId) {
        let (pty_tx, pty_rx) = tokio::sync::mpsc::unbounded_channel();
        let terminal = Terminal::new(24, 80, session_id, pty_tx);
        let theme = self.workspace.theme();
        let terminal_view = TerminalView::new(terminal, theme.clone());
        self.terminals.insert(session_id, terminal_view);
        self.pty_write_receivers.insert(session_id, pty_rx);
    }

    fn discard_bootstrapped_session(&mut self, session_id: SessionId) {
        self.terminals.remove(&session_id);
        self.pty_write_receivers.remove(&session_id);
        self.terminal_headers.remove(&session_id);
        self.output_dispatcher.remove_session(session_id);
        self.polling.output_prepare_in_flight.remove(&session_id);
        self.with_detector(|detector| detector.stop_monitoring(session_id));
        self.with_session_manager(|manager| {
            if let Err(error) = manager.close_session(session_id) {
                warn!(
                    ?session_id,
                    %error,
                    "Failed to discard unattached bootstrapped session"
                );
            }
        });
    }

    fn attach_bootstrapped_session(
        &mut self,
        session: Session,
        session_name: &str,
        target_slot: Option<SlotId>,
        group: Option<&String>,
        color: Option<&String>,
    ) -> bool {
        let session_id = session.id;
        self.create_terminal_view_for_session(session_id);

        let added = match target_slot {
            Some(slot) => {
                if self.workspace.add_session_to_slot(session.clone(), slot) {
                    true
                } else {
                    warn!(
                        ?session_id,
                        ?slot,
                        "Reserved slot unavailable when session bootstrap completed; falling back"
                    );
                    self.workspace.add_session(session.clone())
                }
            }
            None => self.workspace.add_session(session.clone()),
        };

        if !added {
            self.discard_bootstrapped_session(session_id);
            warn!(
                ?session_id,
                "Discarded bootstrapped session because the workspace could not attach it"
            );
            return false;
        }

        self.mark_layout_cache_dirty();
        let header = self.build_terminal_header(&session, session_name, group, color);
        self.terminal_headers.insert(session_id, header);
        self.output_dispatcher.mark_ready(session_id);
        self.with_session_manager(|manager| manager.mark_output_pending(session_id));
        true
    }

    fn start_bootstrapped_session_monitoring(
        &mut self,
        session_id: SessionId,
        child_pid: Option<u32>,
    ) {
        if let Some(pid) = child_pid {
            self.with_detector(|detector| {
                if let Err(error) = detector.start_monitoring(session_id, pid) {
                    warn!(
                        ?session_id,
                        %error,
                        "Failed to start monitoring bootstrapped session"
                    );
                }
            });
        }
    }

    fn finalize_created_session_bootstrap(
        &mut self,
        bootstrapped: SessionBootstrapResult,
        target_slot: Option<SlotId>,
        cx: &mut Context<Self>,
    ) {
        self.start_bootstrapped_session_monitoring(bootstrapped.session_id, bootstrapped.child_pid);

        if !self.attach_bootstrapped_session(
            bootstrapped.session.clone(),
            &bootstrapped.request.session_name,
            target_slot,
            None,
            None,
        ) {
            return;
        }

        self.select_session_with_cx(bootstrapped.session_id, cx);

        if let Some(slot) = target_slot {
            info!(
                name = %bootstrapped.request.session_name,
                ?slot,
                "Created new session in slot via background bootstrap"
            );
        } else {
            info!(
                name = %bootstrapped.request.session_name,
                "Created new session via background bootstrap"
            );
        }

        self.refresh_derived_ui_state();
        self.save_state_to_disk(cx);
        cx.notify();
    }

    fn finalize_restored_session_bootstrap(
        &mut self,
        bootstrapped: SessionBootstrapResult,
        plan: RestoreSessionPlan,
    ) {
        self.start_bootstrapped_session_monitoring(bootstrapped.session_id, bootstrapped.child_pid);

        if plan.codex_execution_mode.is_some() || plan.codex_started_at.is_some() {
            let codex_execution_mode = plan.codex_execution_mode;
            let codex_started_at = plan.codex_started_at;
            if let Ok(manager) = self.session_manager.lock() {
                manager.with_session_state_mut(bootstrapped.session_id, |state| {
                    state.session.codex_execution_mode = codex_execution_mode;
                    state.session.codex_started_at = codex_started_at;
                });
            }
        }

        let mut session = bootstrapped.session;
        session.group = plan.group.clone();
        session.color = plan.color.clone();
        session.codex_execution_mode = plan.codex_execution_mode;
        session.codex_started_at = plan.codex_started_at;

        if !self.attach_bootstrapped_session(
            session,
            &plan.session_name,
            None,
            plan.group.as_ref(),
            plan.color.as_ref(),
        ) {
            return;
        }

        if plan.group.is_some() || plan.color.is_some() {
            self.with_session_manager(|manager| {
                let _ = manager.set_session_group(
                    bootstrapped.session_id,
                    plan.group.clone(),
                    plan.color.clone(),
                );
            });
        }

        for command in restore_resume_commands(&plan) {
            if let Ok(manager) = self.session_manager.lock() {
                if let Err(error) = manager.send_input(bootstrapped.session_id, command.as_bytes())
                {
                    warn!(
                        ?bootstrapped.session_id,
                        %error,
                        "Failed to send resume command"
                    );
                }
            }
        }
    }

    fn spawn_create_session_bootstrap(
        &mut self,
        request: SessionBootstrapRequest,
        reserved_number: u64,
        target_slot: Option<SlotId>,
        cx: &mut Context<Self>,
    ) {
        let session_manager = self.session_manager.clone();
        let session_name = request.session_name.clone();

        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { bootstrap_session(session_manager, request) })
                .await;

            let _ = this.update(cx, |this, cx| {
                this.release_session_create_reservation(reserved_number, target_slot);
                match result {
                    Ok(bootstrapped) => {
                        this.finalize_created_session_bootstrap(bootstrapped, target_slot, cx);
                    }
                    Err(error) => {
                        warn!(
                            name = %session_name,
                            %error,
                            "Failed to create session via background bootstrap"
                        );
                    }
                }
            });
        })
        .detach();
    }

    fn build_restore_plan(
        state: codirigent_core::AppState,
        fallback_dir: PathBuf,
    ) -> Option<RestorePlan> {
        let codirigent_core::AppState {
            sessions: saved_sessions,
            layout,
            ..
        } = state;

        if saved_sessions.is_empty() {
            return None;
        }

        let mut used_names = std::collections::HashSet::new();
        let mut used_claude_ids: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        let mut used_codex_ids: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        let mut used_gemini_ids: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        let mut sessions = Vec::with_capacity(saved_sessions.len());

        for saved in saved_sessions {
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
                .and_then(|claude_id| {
                    let permission_mode = if is_safe_cli_session_id(claude_id) {
                        read_claude_permission_mode(claude_id).unwrap_or_default()
                    } else {
                        String::new()
                    };
                    let extra_args = if permission_mode == "bypassPermissions" {
                        vec!["--dangerously-skip-permissions"]
                    } else {
                        Vec::new()
                    };
                    build_resume_command("claude", claude_id, &extra_args)
                });

            let codex_resume = saved
                .codex_session_id
                .as_ref()
                .filter(|id| used_codex_ids.insert((*id).clone()))
                .and_then(|codex_id| {
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
                .and_then(|gemini_id| build_resume_command("gemini", gemini_id, &[]));

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

        Some(RestorePlan { layout, sessions })
    }

    fn apply_restore_plan(
        &mut self,
        plan: RestorePlan,
        shell: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if plan.sessions.is_empty() {
            self.polling.restore_in_flight = false;
            return;
        }

        info!(count = plan.sessions.len(), "Restoring sessions from disk");

        let session_count = plan.sessions.len();
        let desired_layout = plan.layout.clone();
        let staging_layout = staging_layout_for_restore(&desired_layout, session_count);
        let reapply_saved_layout = staging_layout != desired_layout;
        self.apply_restored_layout_mode(&staging_layout);
        self.polling.restore_in_flight = true;
        let restore_sessions = plan.sessions;
        let session_manager = self.session_manager.clone();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let mut remaining = restore_sessions.into_iter().peekable();
            while remaining.peek().is_some() {
                let batch = remaining.by_ref().take(2).collect::<Vec<_>>();
                let is_last_batch = remaining.peek().is_none();
                let shell = shell.clone();
                let desired_layout = desired_layout.clone();
                let session_manager = session_manager.clone();

                let completions = cx
                    .background_executor()
                    .spawn(async move {
                        batch
                            .into_iter()
                            .map(|plan| {
                                let request = SessionBootstrapRequest {
                                    session_name: plan.session_name.clone(),
                                    working_dir: plan.working_dir.clone(),
                                    shell: shell.clone(),
                                };
                                CompletedRestoreBootstrap {
                                    plan,
                                    result: bootstrap_session(session_manager.clone(), request),
                                }
                            })
                            .collect::<Vec<_>>()
                    })
                    .await;

                let _ = this.update(cx, |this, cx| {
                    for completion in completions {
                        match completion.result {
                            Ok(bootstrapped) => {
                                this.finalize_restored_session_bootstrap(
                                    bootstrapped,
                                    completion.plan,
                                );
                            }
                            Err(error) => {
                                warn!(
                                    name = %completion.plan.session_name,
                                    %error,
                                    "Failed to restore session via background bootstrap"
                                );
                            }
                        }
                    }

                    if is_last_batch {
                        if reapply_saved_layout {
                            this.apply_restored_layout_mode(&desired_layout);
                        }
                        if let Some(first_id) = this.workspace.sessions().first().map(|s| s.id) {
                            this.select_session_with_cx(first_id, cx);
                        }
                        this.polling.restore_in_flight = false;
                        info!("Session restoration complete");
                    }

                    this.refresh_derived_ui_state();
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

    fn apply_restored_layout_mode(&mut self, layout: &LayoutMode) {
        match layout {
            LayoutMode::SplitTree { root } => self.workspace.set_split_tree(root.clone()),
            _ => {
                if let Some(profile) = layout_profile_for_restore(layout) {
                    self.workspace.set_layout(profile);
                }
            }
        }

        if let Some(profile) = layout_profile_for_restore(layout) {
            self.top_bar.set_active_layout(profile);
        } else if let Some(profile_id) = self
            .top_bar
            .profile_manager
            .list_profiles()
            .iter()
            .find(|profile| profile.layout == *layout)
            .map(|profile| profile.id.clone())
        {
            self.top_bar.set_active_profile_id(&profile_id);
        } else {
            self.top_bar.set_active_profile_id("__none__");
        }

        self.mark_layout_cache_dirty();
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
        if let Some(slot) = target_slot {
            if self.polling.pending_session_bootstrap_slots.contains(&slot) {
                warn!(?slot, "Session creation already pending for slot");
                return;
            }
            self.polling.pending_session_bootstrap_slots.insert(slot);
        }
        let num = next_available_session_number(
            self.workspace.sessions(),
            &self.polling.pending_session_bootstrap_numbers,
        );
        self.polling.pending_session_bootstrap_numbers.insert(num);
        let name = format!("{}{}", SESSION_NAME_PREFIX, num);
        self.next_session_id = self.next_session_id.max(num + 1);

        let working_dir = self
            .effective_user_settings()
            .general
            .default_working_dir
            .clone()
            .map(PathBuf::from)
            .filter(|p| p.is_dir())
            .or_else(|| self.project.project_root.clone())
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(std::env::temp_dir);

        let shell = self.configured_shell();
        let request = SessionBootstrapRequest {
            session_name: name,
            working_dir,
            shell,
        };
        self.spawn_create_session_bootstrap(request, num, target_slot, cx);
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
                if let Some(plan) = restore_plan {
                    this.apply_restore_plan(plan, shell.clone(), cx);
                } else {
                    this.polling.restore_in_flight = false;
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

        // Remove from output dispatcher tracking (ready/in-flight sets)
        self.output_dispatcher.remove_session(id);

        // Remove the terminal header for this session
        self.terminal_headers.remove(&id);

        // Remove from workspace
        self.workspace.remove_session(id);
        self.mark_layout_cache_dirty();
        self.event_bus
            .publish(CodirigentEvent::SessionClosed { id });
        info!(?id, "Closed session");
        self.refresh_derived_ui_state();
        self.save_state_to_disk(cx);
        cx.notify();
    }
}
