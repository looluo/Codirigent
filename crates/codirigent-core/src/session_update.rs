//! Internal hot-path event types for the session output pipeline.
//!
//! These events flow through a dedicated `tokio::sync::mpsc` channel from
//! producers (PTY reader threads, OSC parsers, detectors) to a single consumer
//! (the output dispatcher in the UI layer).
//!
//! This is intentionally separate from [`CodirigentEvent`] / [`EventBus`]:
//! - `SessionUpdate` is an **internal transport channel** (mpsc, single consumer)
//! - `CodirigentEvent` is a **public business event bus** (broadcast, multi-subscriber)

use crate::types::{GitRepoInfo, SessionId, SessionStatus, ShellState};
use std::path::PathBuf;

/// Source of a status hint for prioritization in the status reconciler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusHintSource {
    /// OSC 133 shell-state markers (high confidence, shell-reported).
    Osc133,
    /// Claude Code hook signal files.
    HookSignal,
    /// Codex/Gemini JSONL log files.
    Jsonl,
    /// Process-tree / heuristic detector.
    Detector,
}

/// Internal hot-path event emitted by session infrastructure.
///
/// These events notify the output dispatcher that something changed for a
/// specific session, avoiding the need for broad polling sweeps.
#[derive(Debug)]
pub enum SessionUpdate {
    /// PTY reader has enqueued new output bytes for this session.
    OutputReady {
        /// The session that has pending output.
        session_id: SessionId,
    },

    /// Output was drained and applied to the terminal emulator.
    #[allow(dead_code)] // Wired when budget enforcement tracks bytes drained
    OutputDrained {
        /// The session whose output was consumed.
        session_id: SessionId,
        /// Number of bytes drained in this batch.
        bytes: usize,
    },

    /// Shell state changed (via OSC 133 markers).
    ShellStateChanged {
        /// The session whose shell state changed.
        session_id: SessionId,
        /// New shell state.
        state: ShellState,
    },

    /// Working directory changed (via OSC 7).
    WorkingDirectoryChanged {
        /// The session whose CWD changed.
        session_id: SessionId,
        /// New working directory path.
        cwd: PathBuf,
    },

    /// A status hint arrived from a specific source.
    #[allow(dead_code)] // Wired when status providers emit via channel
    StatusHintChanged {
        /// The session whose status hint changed.
        session_id: SessionId,
        /// Where this hint came from.
        source: StatusHintSource,
        /// The hinted status.
        status: SessionStatus,
    },

    /// The PTY child process exited.
    ChildProcessExited {
        /// The session whose child process exited.
        session_id: SessionId,
    },

    /// Git repository info changed for this session.
    #[allow(dead_code)] // Wired when git refresh emits events directly
    GitInfoChanged {
        /// The session whose git info changed.
        session_id: SessionId,
        /// New git repository info, or `None` if no longer in a git repo.
        git_info: Option<Box<GitRepoInfo>>,
    },
}

impl SessionUpdate {
    /// Get the session ID associated with this update.
    pub fn session_id(&self) -> SessionId {
        match self {
            Self::OutputReady { session_id }
            | Self::OutputDrained { session_id, .. }
            | Self::ShellStateChanged { session_id, .. }
            | Self::WorkingDirectoryChanged { session_id, .. }
            | Self::StatusHintChanged { session_id, .. }
            | Self::ChildProcessExited { session_id }
            | Self::GitInfoChanged { session_id, .. } => *session_id,
        }
    }
}
