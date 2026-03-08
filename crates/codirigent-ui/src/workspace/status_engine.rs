//! Status reconciliation engine.
//!
//! Consolidates the status precedence rules that were previously spread
//! across `sync_session_status()` into a single, testable reconciler.
//!
//! ## Precedence Rules
//!
//! 1. Live `Working` from the detector beats any cached status (the session
//!    is actively producing output, so cached hook/JSONL data is stale).
//! 2. Stale `NeedsAttention` (cached for >30s while detector says `Idle`)
//!    means the CLI likely exited — clear cache and revert CLI type.
//! 3. Otherwise: cached JSONL/hook status overlays the detector status.
//! 4. Detector status is the fallback when no cached status exists.

use super::status_providers::{HintSource, ReconciledStatus, StaleAction};
use codirigent_core::{SessionId, SessionStatus};
use std::time::Duration;

/// Stale-NeedsAttention threshold — if a cached NeedsAttention status has
/// been unchanged for this long while the detector says Idle, it's stale.
const STALE_ATTENTION_THRESHOLD: Duration = Duration::from_secs(30);

/// Reconcile a detector hint with an optional cached CLI hint.
///
/// This implements the precedence rules documented above, returning both the
/// reconciled status and any side-effect actions (like clearing stale caches).
///
/// # Arguments
///
/// * `session_id` — The session being reconciled.
/// * `detector_status` — Status from the process-state / OSC 133 detector.
/// * `cached_status` — Optional cached status from hook signals or JSONL.
/// * `cached_tool_name` — Optional tool name from the cached status.
/// * `cached_source` — Source of the cached status.
/// * `cache_age` — How long the cached status has been unchanged.
/// * `previous_status` — The session's current status before reconciliation.
#[must_use]
pub(super) fn reconcile(
    session_id: SessionId,
    detector_status: Option<SessionStatus>,
    cached_status: Option<SessionStatus>,
    cached_tool_name: Option<String>,
    cached_source: HintSource,
    cache_age: Option<Duration>,
    previous_status: Option<SessionStatus>,
) -> (Option<ReconciledStatus>, StaleAction) {
    let Some(detector) = detector_status else {
        // No detector status — nothing to reconcile
        return (None, StaleAction::None);
    };

    // Helper: build a ReconciledStatus where the detector wins.
    let detector_wins = |prev: Option<SessionStatus>| ReconciledStatus {
        status: detector,
        source: HintSource::Detector,
        tool_name: None,
        changed: prev != Some(detector),
        previous_status: prev,
    };

    let Some(cached) = cached_status else {
        // No cached status — detector wins
        return (Some(detector_wins(previous_status)), StaleAction::None);
    };

    // Rule 1: Live Working from detector beats cached (session is active)
    if detector == SessionStatus::Working && cached != SessionStatus::Working {
        return (Some(detector_wins(previous_status)), StaleAction::None);
    }

    // Rule 2: Stale NeedsAttention — CLI likely exited
    if cached == SessionStatus::NeedsAttention
        && detector == SessionStatus::Idle
        && cache_age.is_some_and(|age| age > STALE_ATTENTION_THRESHOLD)
    {
        return (
            Some(detector_wins(previous_status)),
            StaleAction::ClearAndRevert { session_id },
        );
    }

    // Rule 3: Cached status wins (it's fresher or more specific)
    (
        Some(ReconciledStatus {
            status: cached,
            source: cached_source,
            tool_name: cached_tool_name,
            changed: previous_status != Some(cached),
            previous_status,
        }),
        StaleAction::None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detector_wins_when_no_cache() {
        let (result, stale) = reconcile(
            SessionId(1),
            Some(SessionStatus::Working),
            None,
            None,
            HintSource::Detector,
            None,
            None,
        );
        assert!(matches!(stale, StaleAction::None));
        let r = result.unwrap();
        assert_eq!(r.status, SessionStatus::Working);
        assert_eq!(r.source, HintSource::Detector);
        assert!(r.changed);
    }

    #[test]
    fn cached_overlay_beats_detector_idle() {
        let (result, stale) = reconcile(
            SessionId(1),
            Some(SessionStatus::Idle),
            Some(SessionStatus::ResponseReady),
            None,
            HintSource::HookSignal,
            Some(Duration::from_secs(5)),
            Some(SessionStatus::Working),
        );
        assert!(matches!(stale, StaleAction::None));
        let r = result.unwrap();
        assert_eq!(r.status, SessionStatus::ResponseReady);
        assert_eq!(r.source, HintSource::HookSignal);
        assert!(r.changed);
    }

    #[test]
    fn live_working_beats_cached_response_ready() {
        let (result, stale) = reconcile(
            SessionId(1),
            Some(SessionStatus::Working),
            Some(SessionStatus::ResponseReady),
            None,
            HintSource::HookSignal,
            Some(Duration::from_secs(5)),
            Some(SessionStatus::ResponseReady),
        );
        assert!(matches!(stale, StaleAction::None));
        let r = result.unwrap();
        assert_eq!(r.status, SessionStatus::Working);
        assert_eq!(r.source, HintSource::Detector);
    }

    #[test]
    fn stale_needs_attention_triggers_clear() {
        let (result, stale) = reconcile(
            SessionId(42),
            Some(SessionStatus::Idle),
            Some(SessionStatus::NeedsAttention),
            None,
            HintSource::HookSignal,
            Some(Duration::from_secs(60)),
            Some(SessionStatus::NeedsAttention),
        );
        assert!(matches!(
            stale,
            StaleAction::ClearAndRevert {
                session_id: SessionId(42)
            }
        ));
        let r = result.unwrap();
        assert_eq!(r.status, SessionStatus::Idle);
    }

    #[test]
    fn fresh_needs_attention_not_stale() {
        let (result, stale) = reconcile(
            SessionId(1),
            Some(SessionStatus::Idle),
            Some(SessionStatus::NeedsAttention),
            None,
            HintSource::HookSignal,
            Some(Duration::from_secs(10)),
            Some(SessionStatus::Working),
        );
        assert!(matches!(stale, StaleAction::None));
        let r = result.unwrap();
        assert_eq!(r.status, SessionStatus::NeedsAttention);
    }

    #[test]
    fn no_detector_status_returns_none() {
        let (result, stale) = reconcile(
            SessionId(1),
            None,
            Some(SessionStatus::Working),
            None,
            HintSource::Jsonl,
            None,
            None,
        );
        assert!(result.is_none());
        assert!(matches!(stale, StaleAction::None));
    }

    #[test]
    fn unchanged_status_not_marked_changed() {
        let (result, _) = reconcile(
            SessionId(1),
            Some(SessionStatus::Idle),
            None,
            None,
            HintSource::Detector,
            None,
            Some(SessionStatus::Idle),
        );
        let r = result.unwrap();
        assert!(!r.changed);
    }

    #[test]
    fn both_working_uses_cached_source() {
        // When both detector and cache say Working, the cache wins (Rule 1
        // only fires when cached != Working).
        let (result, stale) = reconcile(
            SessionId(1),
            Some(SessionStatus::Working),
            Some(SessionStatus::Working),
            Some("bash".to_string()),
            HintSource::Jsonl,
            Some(Duration::from_secs(2)),
            Some(SessionStatus::Idle),
        );
        assert!(matches!(stale, StaleAction::None));
        let r = result.unwrap();
        assert_eq!(r.status, SessionStatus::Working);
        assert_eq!(r.source, HintSource::Jsonl);
        assert!(r.changed); // Idle -> Working
    }

    #[test]
    fn needs_attention_with_no_cache_age_treated_as_fresh() {
        // When cache_age is None, is_some_and returns false, so the cached
        // NeedsAttention is treated as fresh (not stale).
        let (result, stale) = reconcile(
            SessionId(1),
            Some(SessionStatus::Idle),
            Some(SessionStatus::NeedsAttention),
            None,
            HintSource::HookSignal,
            None, // no age info
            Some(SessionStatus::Working),
        );
        assert!(matches!(stale, StaleAction::None));
        let r = result.unwrap();
        assert_eq!(r.status, SessionStatus::NeedsAttention);
    }

    #[test]
    fn needs_attention_at_exact_threshold_not_stale() {
        let (_, stale) = reconcile(
            SessionId(1),
            Some(SessionStatus::Idle),
            Some(SessionStatus::NeedsAttention),
            None,
            HintSource::HookSignal,
            Some(STALE_ATTENTION_THRESHOLD), // exactly 30s
            Some(SessionStatus::NeedsAttention),
        );
        // Strict `>` means exactly 30s is NOT stale
        assert!(matches!(stale, StaleAction::None));
    }

    #[test]
    fn detector_working_beats_cached_error() {
        let (result, stale) = reconcile(
            SessionId(1),
            Some(SessionStatus::Working),
            Some(SessionStatus::Error),
            None,
            HintSource::Jsonl,
            Some(Duration::from_secs(5)),
            Some(SessionStatus::Error),
        );
        assert!(matches!(stale, StaleAction::None));
        let r = result.unwrap();
        assert_eq!(r.status, SessionStatus::Working);
        assert_eq!(r.source, HintSource::Detector);
    }

    #[test]
    fn stale_error_not_auto_cleared() {
        // Unlike NeedsAttention, stale Error is NOT auto-cleared.
        // Error status staleness is handled by TTL eviction in the caller.
        let (result, stale) = reconcile(
            SessionId(1),
            Some(SessionStatus::Idle),
            Some(SessionStatus::Error),
            None,
            HintSource::HookSignal,
            Some(Duration::from_secs(120)),
            Some(SessionStatus::Error),
        );
        assert!(matches!(stale, StaleAction::None));
        let r = result.unwrap();
        assert_eq!(r.status, SessionStatus::Error);
    }
}
