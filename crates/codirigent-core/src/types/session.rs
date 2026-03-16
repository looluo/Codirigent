//! Session metadata and state.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::git::GitRepoInfo;
use super::ids::{SessionId, TaskId};
use super::status::SessionStatus;

/// Generate a new stable UUID for a session.
pub fn generate_session_uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Effective Codex execution mode for a session.
///
/// This is persisted so restored sessions can reuse the same launch flags and
/// Codex status inference can avoid false approval prompts for bypassed runs.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum CodexExecutionMode {
    /// Equivalent to `codex --full-auto`.
    FullAuto,
    /// Equivalent to `codex --dangerously-bypass-approvals-and-sandbox`.
    Bypass,
}

/// Session metadata and state.
///
/// This is the persistent representation of a session,
/// stored in state.json and used throughout the application.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Session {
    /// Unique session identifier.
    pub id: SessionId,
    /// Immutable stable UUID for this session across renames and restores.
    #[serde(default = "generate_session_uuid")]
    pub session_uuid: String,
    /// Human-readable session name.
    pub name: String,
    /// Current session status.
    pub status: SessionStatus,
    /// Working directory for this session.
    pub working_directory: PathBuf,
    /// Requested shell for this session. `None` means Auto.
    #[serde(default)]
    pub shell: Option<String>,
    /// Currently assigned task, if any.
    pub current_task: Option<TaskId>,
    /// Context window usage (0.0 - 1.0), if available.
    pub context_usage: Option<f32>,
    /// When the session was created.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Session group name for visual grouping.
    pub group: Option<String>,
    /// Group color for visual identification.
    pub color: Option<String>,
    /// Git repository information (branch, dirty count, etc.).
    pub git_info: Option<GitRepoInfo>,
    /// Claude Code session ID (UUID) for this session, if Claude Code is running.
    /// Used to resume with `claude --resume <id>` on next app startup.
    pub claude_session_id: Option<String>,
    /// Codex session ID for this session, if Codex CLI is running.
    /// Used to resume with `codex resume <id>` on next app startup.
    pub codex_session_id: Option<String>,
    /// Effective Codex execution mode, if known.
    #[serde(default)]
    pub codex_execution_mode: Option<CodexExecutionMode>,
    /// When the current Codex CLI run was started in this session, if known.
    ///
    /// This is separate from `created_at` because a shell pane can live much
    /// longer than the Codex process it is currently hosting.
    #[serde(default)]
    pub codex_started_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Gemini CLI session ID (UUID) for this session, if Gemini CLI is running.
    pub gemini_session_id: Option<String>,
}

impl Session {
    /// Create a new session with default values.
    pub fn new(id: SessionId, name: String, working_directory: PathBuf) -> Self {
        Self {
            id,
            session_uuid: generate_session_uuid(),
            name,
            status: SessionStatus::default(),
            working_directory,
            shell: None,
            current_task: None,
            context_usage: None,
            created_at: chrono::Utc::now(),
            group: None,
            color: None,
            git_info: None,
            claude_session_id: None,
            codex_session_id: None,
            codex_execution_mode: None,
            codex_started_at: None,
            gemini_session_id: None,
        }
    }
}
