//! Output dispatcher for the event-driven session pipeline.
//!
//! Replaces the broad `sessions_with_pending_output()` scan with a targeted
//! approach: the dispatcher is fed by [`SessionUpdate::OutputReady`] events
//! from the PTY reader mpsc channel and maintains a ready-session set.
//!
//! The dispatcher owns scheduling policy:
//! - Focused session gets first priority
//! - Fair round-robin among hot background sessions
//! - Per-poll chunk and byte budgets
//! - In-flight session tracking to prevent double-dispatch
//!
//! During the transition period, a low-frequency fallback poll (~1s) is
//! retained as a safety net to catch any events missed by the channel.

use codirigent_core::{SessionId, SessionUpdate};
use std::collections::HashSet;
use tracing::{trace, warn};

/// Maximum PTY chunks drained per session per poll cycle.
const DEFAULT_MAX_CHUNKS_PER_POLL: usize = 64;

/// Maximum bytes drained per session per poll cycle (256 KB).
const DEFAULT_MAX_BYTES_PER_POLL: usize = 256 * 1024;

/// Maximum events drained from the mpsc channel per poll cycle.
/// Prevents unbounded loop in pathological flooding scenarios (e.g., `yes`).
/// OutputReady events deduplicate into the HashSet, so this primarily caps
/// non-OutputReady events and loop iterations.
const MAX_EVENTS_PER_DRAIN: usize = 1024;

/// Scheduling policy for output dispatch.
///
/// Extracted from the inline logic in `schedule_output_preparation` to make
/// it independently testable and configurable.
pub(super) struct OutputDispatcher {
    /// Sessions with pending output (fed by `SessionUpdate::OutputReady`).
    ready_sessions: HashSet<SessionId>,
    /// Sessions currently being processed (background task in-flight).
    in_flight: HashSet<SessionId>,
    /// Maximum PTY chunks per session per poll cycle.
    #[allow(dead_code)] // Tested; production uses WorkspaceView constants directly for now
    pub max_chunks_per_poll: usize,
    /// Maximum bytes per session per poll cycle.
    #[allow(dead_code)] // Tested; production uses WorkspaceView constants directly for now
    pub max_bytes_per_poll: usize,
}

impl Default for OutputDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl OutputDispatcher {
    /// Create a new dispatcher with default budgets.
    pub fn new() -> Self {
        Self {
            ready_sessions: HashSet::new(),
            in_flight: HashSet::new(),
            max_chunks_per_poll: DEFAULT_MAX_CHUNKS_PER_POLL,
            max_bytes_per_poll: DEFAULT_MAX_BYTES_PER_POLL,
        }
    }

    /// Process a batch of `SessionUpdate` events from the mpsc channel.
    ///
    /// Drains the receiver without blocking (returns when empty or channel
    /// closed). Only `OutputReady` events are consumed here; other event
    /// types are returned for the caller to dispatch.
    pub fn drain_updates(
        &mut self,
        rx: &mut tokio::sync::mpsc::Receiver<SessionUpdate>,
    ) -> Vec<SessionUpdate> {
        let mut other_events = Vec::new();
        let mut drained = 0usize;
        loop {
            if drained >= MAX_EVENTS_PER_DRAIN {
                trace!(drained, "drain_updates: hit event cap");
                break;
            }
            match rx.try_recv() {
                Ok(event) => {
                    drained += 1;
                    match event {
                        SessionUpdate::OutputReady { session_id } => {
                            self.ready_sessions.insert(session_id);
                        }
                        other => {
                            other_events.push(other);
                        }
                    }
                }
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                    warn!("SessionUpdate channel disconnected — all senders dropped");
                    break;
                }
            }
        }
        other_events
    }

    /// Mark a session as ready for output processing.
    ///
    /// Used by the legacy fallback poll path and when `has_more` is true
    /// after a bounded drain.
    pub fn mark_ready(&mut self, session_id: SessionId) {
        self.ready_sessions.insert(session_id);
    }

    /// Mark a session as in-flight (background task started).
    ///
    /// Returns `false` if the session is already in-flight (caller should
    /// re-mark it as ready instead of double-dispatching).
    pub fn mark_in_flight(&mut self, session_id: SessionId) -> bool {
        if self.in_flight.contains(&session_id) {
            // Already processing — re-queue so it's not lost.
            // The next poll cycle will dispatch it after the current
            // in-flight task completes.
            self.ready_sessions.insert(session_id);
            false
        } else {
            self.ready_sessions.remove(&session_id);
            self.in_flight.insert(session_id);
            true
        }
    }

    /// Mark a session as no longer in-flight (background task completed).
    pub fn complete_in_flight(&mut self, session_id: SessionId) {
        self.in_flight.remove(&session_id);
    }

    /// Take the set of sessions ready for dispatch, prioritized.
    ///
    /// The focused session (if ready) is placed first. All returned sessions
    /// are removed from the ready set but NOT yet marked in-flight — the
    /// caller must call [`mark_in_flight`] for each one it actually dispatches.
    ///
    /// Sessions that are already in-flight are excluded and left in the
    /// ready set for the next cycle.
    pub fn take_ready_sessions(&mut self, focused_session_id: Option<SessionId>) -> Vec<SessionId> {
        if self.ready_sessions.is_empty() {
            return Vec::new();
        }

        let mut result = Vec::with_capacity(self.ready_sessions.len());
        let mut deferred = Vec::new();

        // Priority: focused session is dispatched first to minimize
        // perceived latency for the pane the user is looking at.
        if let Some(focused) = focused_session_id {
            if self.ready_sessions.remove(&focused) {
                if self.in_flight.contains(&focused) {
                    deferred.push(focused);
                } else {
                    result.push(focused);
                }
            }
        }

        // Remaining sessions in arbitrary order (HashSet iteration).
        // Fair round-robin is approximated by the fact that in-flight
        // sessions are deferred, preventing any single session from
        // monopolizing dispatch slots.
        let remaining: Vec<SessionId> = self.ready_sessions.drain().collect();
        for id in remaining {
            if self.in_flight.contains(&id) {
                deferred.push(id);
            } else {
                result.push(id);
            }
        }

        // Re-insert deferred sessions so they aren't lost — they'll be
        // picked up on the next poll cycle after their in-flight task
        // completes.
        for id in deferred {
            self.ready_sessions.insert(id);
        }

        if !result.is_empty() {
            trace!(
                count = result.len(),
                deferred = self.ready_sessions.len(),
                "dispatcher: take_ready_sessions"
            );
        }

        result
    }

    /// Whether there is any active work (ready or in-flight sessions).
    pub fn has_activity(&self) -> bool {
        !self.ready_sessions.is_empty() || !self.in_flight.is_empty()
    }

    /// Number of sessions currently in-flight.
    pub fn in_flight_count(&self) -> usize {
        self.in_flight.len()
    }

    /// Number of sessions in the ready set.
    #[allow(dead_code)] // Used in tests; exposed for diagnostics
    pub fn ready_count(&self) -> usize {
        self.ready_sessions.len()
    }

    /// Remove a session from all tracking (e.g., when session is closed).
    pub fn remove_session(&mut self, session_id: SessionId) {
        self.ready_sessions.remove(&session_id);
        self.in_flight.remove(&session_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focused_session_is_prioritized() {
        let mut dispatcher = OutputDispatcher::new();
        let s1 = SessionId(1);
        let s2 = SessionId(2);
        let s3 = SessionId(3);

        dispatcher.mark_ready(s1);
        dispatcher.mark_ready(s2);
        dispatcher.mark_ready(s3);

        let ready = dispatcher.take_ready_sessions(Some(s2));
        assert_eq!(ready[0], s2);
        assert_eq!(ready.len(), 3);
    }

    #[test]
    fn in_flight_sessions_are_deferred() {
        let mut dispatcher = OutputDispatcher::new();
        let s1 = SessionId(1);
        let s2 = SessionId(2);

        dispatcher.mark_ready(s1);
        dispatcher.mark_ready(s2);

        // Mark s1 as in-flight
        assert!(dispatcher.mark_in_flight(s1));
        // Now s1 is in-flight, s2 is still ready
        dispatcher.mark_ready(s1); // new output arrived while in-flight

        let ready = dispatcher.take_ready_sessions(None);
        // s1 should be deferred (it's in-flight), only s2 returned
        assert_eq!(ready, vec![s2]);
        // s1 should still be in the ready set
        assert_eq!(dispatcher.ready_count(), 1);
    }

    #[test]
    fn mark_in_flight_prevents_double_dispatch() {
        let mut dispatcher = OutputDispatcher::new();
        let s1 = SessionId(1);

        dispatcher.mark_ready(s1);
        assert!(dispatcher.mark_in_flight(s1));
        // Second attempt should fail and re-queue
        assert!(!dispatcher.mark_in_flight(s1));
        assert_eq!(dispatcher.ready_count(), 1);
    }

    #[test]
    fn complete_in_flight_clears_tracking() {
        let mut dispatcher = OutputDispatcher::new();
        let s1 = SessionId(1);

        dispatcher.mark_ready(s1);
        dispatcher.mark_in_flight(s1);
        assert_eq!(dispatcher.in_flight_count(), 1);

        dispatcher.complete_in_flight(s1);
        assert_eq!(dispatcher.in_flight_count(), 0);
    }

    #[test]
    fn has_activity_reflects_state() {
        let mut dispatcher = OutputDispatcher::new();
        assert!(!dispatcher.has_activity());

        let s1 = SessionId(1);
        dispatcher.mark_ready(s1);
        assert!(dispatcher.has_activity());

        dispatcher.mark_in_flight(s1);
        assert!(dispatcher.has_activity());

        dispatcher.complete_in_flight(s1);
        assert!(!dispatcher.has_activity());
    }

    #[test]
    fn remove_session_clears_all_state() {
        let mut dispatcher = OutputDispatcher::new();
        let s1 = SessionId(1);

        dispatcher.mark_ready(s1);
        dispatcher.mark_in_flight(s1);
        dispatcher.mark_ready(s1); // re-queued while in-flight

        dispatcher.remove_session(s1);
        assert!(!dispatcher.has_activity());
    }

    #[test]
    fn drain_updates_separates_output_ready() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(16);
        let mut dispatcher = OutputDispatcher::new();

        let s1 = SessionId(1);
        let s2 = SessionId(2);

        // Send mixed events
        tx.try_send(SessionUpdate::OutputReady { session_id: s1 })
            .unwrap();
        tx.try_send(SessionUpdate::ChildProcessExited { session_id: s2 })
            .unwrap();
        tx.try_send(SessionUpdate::OutputReady { session_id: s2 })
            .unwrap();

        let others = dispatcher.drain_updates(&mut rx);

        // OutputReady events consumed into ready set
        assert_eq!(dispatcher.ready_count(), 2);
        // Non-OutputReady events returned
        assert_eq!(others.len(), 1);
        assert_eq!(others[0].session_id(), s2);
    }

    #[test]
    fn default_budgets_match_constants() {
        let dispatcher = OutputDispatcher::new();
        assert_eq!(dispatcher.max_chunks_per_poll, DEFAULT_MAX_CHUNKS_PER_POLL);
        assert_eq!(dispatcher.max_bytes_per_poll, DEFAULT_MAX_BYTES_PER_POLL);
    }

    #[test]
    fn drain_updates_handles_disconnected_channel() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(16);
        let mut dispatcher = OutputDispatcher::new();

        let s1 = SessionId(1);

        // Send one event then drop the sender to disconnect
        tx.try_send(SessionUpdate::OutputReady { session_id: s1 })
            .unwrap();
        drop(tx);

        // drain_updates should consume the buffered event and stop
        // gracefully when it hits the disconnected state
        let others = dispatcher.drain_updates(&mut rx);

        assert_eq!(dispatcher.ready_count(), 1);
        assert!(others.is_empty());
    }

    #[test]
    fn drain_updates_deduplicates_output_ready() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(16);
        let mut dispatcher = OutputDispatcher::new();

        let s1 = SessionId(1);

        // Send the same OutputReady event 5 times
        for _ in 0..5 {
            tx.try_send(SessionUpdate::OutputReady { session_id: s1 })
                .unwrap();
        }

        let others = dispatcher.drain_updates(&mut rx);

        // HashSet deduplicates — only 1 session in ready set
        assert_eq!(dispatcher.ready_count(), 1);
        assert!(others.is_empty());
    }

    #[test]
    fn drain_updates_respects_event_cap() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(2048);
        let mut dispatcher = OutputDispatcher::new();

        // Send more events than MAX_EVENTS_PER_DRAIN (1024)
        for i in 0..1200 {
            tx.try_send(SessionUpdate::OutputReady {
                session_id: SessionId(i as u64),
            })
            .unwrap();
        }

        let _others = dispatcher.drain_updates(&mut rx);

        // Should have drained at most MAX_EVENTS_PER_DRAIN events
        assert!(dispatcher.ready_count() <= MAX_EVENTS_PER_DRAIN);
        // Remaining events should still be in the channel
        assert!(rx.try_recv().is_ok());
    }
}
