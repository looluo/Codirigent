//! Status hint providers for the session status reconciler.
//!
//! Each provider contributes a status "hint" for a session from a different
//! source. The reconciler in [`super::status_engine`] combines hints using
//! explicit precedence rules.

use codirigent_core::{SessionId, SessionStatus};
use std::time::{Duration, Instant};

/// A status hint from a specific provider.
#[derive(Debug, Clone)]
pub(super) struct StatusHint {
    /// The hinted status.
    #[allow(dead_code)] // Used when provider-level reconciliation is wired in
    pub status: SessionStatus,
    /// When this hint was last updated.
    #[allow(dead_code)] // Used when provider-level freshness checking is wired in
    pub updated_at: Instant,
    /// Optional tool name (e.g., for JSONL providers).
    #[allow(dead_code)] // Used when provider-level freshness checking is wired in
    pub tool_name: Option<String>,
    /// Where this hint came from.
    #[allow(dead_code)] // Used when provider-level reconciliation is wired in
    pub source: HintSource,
}

/// Source of a status hint, used for reconciler precedence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum HintSource {
    /// Process-tree / heuristic detector (OSC 133 + idle timeout).
    Detector,
    /// Claude Code hook signal files (TTL: 600s).
    HookSignal,
    /// Codex/Gemini JSONL log files (TTL: 120s).
    Jsonl,
}

impl StatusHint {
    /// Whether this hint is still within its time-to-live.
    #[allow(dead_code)] // Used when provider-level freshness checking is wired in
    pub fn is_fresh(&self, ttl: Duration) -> bool {
        self.updated_at.elapsed() < ttl
    }
}

/// The result of reconciling multiple status hints for a single session.
#[derive(Debug)]
pub(super) struct ReconciledStatus {
    /// The final reconciled status.
    pub status: SessionStatus,
    /// Which source won.
    #[allow(dead_code)] // Logged in shadow-mode diffing (Task 5)
    pub source: HintSource,
    /// Optional tool name from the winning hint.
    #[allow(dead_code)] // Used when tool-specific side effects are wired in
    pub tool_name: Option<String>,
    /// Whether the status changed from the previous value.
    #[allow(dead_code)] // Used in shadow-mode diffing (Task 5)
    pub changed: bool,
    /// Previous status (for side-effect logic).
    #[allow(dead_code)] // Used when side-effect routing is consolidated
    pub previous_status: Option<SessionStatus>,
}

/// Result of provider-level stale NeedsAttention detection.
///
/// When a cached CLI status shows `NeedsAttention` but the detector
/// says `Idle` and the cache is stale, this signals the CLI exited
/// and the session should revert to generic shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StaleAction {
    /// No staleness detected — use the cached status normally.
    None,
    /// Cache is stale — clear cached status and revert CLI type.
    ClearAndRevert {
        /// The session whose cache should be cleared.
        session_id: SessionId,
    },
}
