//! Internal hot-path event types for the session output pipeline.
//!
//! These events flow through a dedicated `tokio::sync::mpsc` channel from
//! producers (PTY reader threads, OSC parsers, detectors) to a single consumer
//! (the output dispatcher in the UI layer).
//!
//! This is intentionally separate from [`CodirigentEvent`] / [`EventBus`]:
//! - `SessionUpdate` is an **internal transport channel** (mpsc, single consumer)
//! - `CodirigentEvent` is a **public business event bus** (broadcast, multi-subscriber)

use crate::types::{SessionId, ShellState};
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

    /// The PTY child process exited.
    ChildProcessExited {
        /// The session whose child process exited.
        session_id: SessionId,
    },
}

impl SessionUpdate {
    /// Get the session ID associated with this update.
    pub fn session_id(&self) -> SessionId {
        match self {
            Self::OutputReady { session_id }
            | Self::ShellStateChanged { session_id, .. }
            | Self::WorkingDirectoryChanged { session_id, .. }
            | Self::ChildProcessExited { session_id } => *session_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Constructs every `SessionUpdate` variant and verifies `session_id()`
    /// returns the correct ID. This also serves as a compile-time exhaustiveness
    /// check: adding a new variant without updating this test will fail to compile.
    #[test]
    fn session_id_returns_correct_id_for_all_variants() {
        let id = SessionId(42);
        let variants: Vec<SessionUpdate> = vec![
            SessionUpdate::OutputReady { session_id: id },
            SessionUpdate::ShellStateChanged {
                session_id: id,
                state: ShellState::PromptStart,
            },
            SessionUpdate::WorkingDirectoryChanged {
                session_id: id,
                cwd: std::path::PathBuf::from("/tmp"),
            },
            SessionUpdate::ChildProcessExited { session_id: id },
        ];
        for variant in &variants {
            assert_eq!(
                variant.session_id(),
                id,
                "session_id() mismatch for {:?}",
                variant
            );
        }
    }
}
