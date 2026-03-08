//! Status hint providers for the session status reconciler.
//!
//! Each provider contributes a status "hint" for a session from a different
//! source. The reconciler in [`super::status_engine`] combines hints using
//! explicit precedence rules.

use codirigent_core::{SessionId, SessionStatus, StatusHintSource};

/// Re-export the core hint source type for use throughout the workspace.
///
/// This avoids duplicate enum definitions between `codirigent_core` and the
/// UI layer. The reconciler uses only `Detector`, `HookSignal`, and `Jsonl`
/// variants; `Osc133` flows through the detector.
pub(super) type HintSource = StatusHintSource;

/// The result of reconciling multiple status hints for a single session.
#[derive(Debug)]
pub(super) struct ReconciledStatus {
    /// The final reconciled status.
    pub status: SessionStatus,
    /// Which source won.
    #[allow(dead_code)] // Validated in tests; used in shadow-mode diffing
    pub source: HintSource,
    /// Optional tool name from the winning hint.
    #[allow(dead_code)] // Used when tool-specific side effects are wired in
    pub tool_name: Option<String>,
    /// Whether the status changed from the previous value.
    #[allow(dead_code)] // Validated in tests; used in shadow-mode diffing
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
