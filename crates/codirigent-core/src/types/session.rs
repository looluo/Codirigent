//! Session metadata and state.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::git::GitRepoInfo;
use super::ids::{SessionId, TaskId};
use super::status::SessionStatus;

/// Session metadata and state.
///
/// This is the persistent representation of a session,
/// stored in state.json and used throughout the application.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Session {
    /// Unique session identifier.
    pub id: SessionId,
    /// Human-readable session name.
    pub name: String,
    /// Current session status.
    pub status: SessionStatus,
    /// Working directory for this session.
    pub working_directory: PathBuf,
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
}

impl Session {
    /// Create a new session with default values.
    pub fn new(id: SessionId, name: String, working_directory: PathBuf) -> Self {
        Self {
            id,
            name,
            status: SessionStatus::default(),
            working_directory,
            current_task: None,
            context_usage: None,
            created_at: chrono::Utc::now(),
            group: None,
            color: None,
            git_info: None,
            claude_session_id: None,
        }
    }
}
