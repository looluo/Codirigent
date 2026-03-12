//! Deferred terminal input and VTE response helpers.

use super::WorkspaceView;
use codirigent_core::{CodirigentEvent, EventBus, SessionId, SessionManager};
use std::time::{Duration, Instant};
use tracing::warn;

impl WorkspaceView {
    /// Send the deferred Enter keystrokes used for compaction and other
    /// command-submission follow-up so the session only returns to the
    /// available pool after the CLI has had a brief chance to process input.
    pub(super) fn process_deferred_enters(&mut self) {
        // Collect both phases in one pass to avoid iterating pending_enters twice.
        let mut need_enter: Vec<SessionId> = Vec::new();
        let mut expired: Vec<SessionId> = Vec::new();
        for (&session_id, &(when, sent)) in &self.polling.pending_enters {
            if !sent && when.elapsed() >= Self::PENDING_ENTER_DELAY {
                need_enter.push(session_id);
            } else if sent && when.elapsed() >= Duration::from_millis(500) {
                expired.push(session_id);
            }
        }
        for session_id in need_enter {
            if let Ok(mgr) = self.session_manager.lock() {
                let _ = mgr.send_input(session_id, b"\r");
            }
            // Flip to phase 2: keep entry for a grace period so the CLI can
            // process the command before auto-assign considers this session.
            self.polling
                .pending_enters
                .insert(session_id, (Instant::now(), true));
        }
        for session_id in expired {
            self.polling.pending_enters.remove(&session_id);
        }
    }

    /// Drain VTE PtyWrite responses (DSR, DA1, etc.) and forward them to each PTY.
    ///
    /// This is critical: PowerShell blocks on DSR (`\x1b[6n]`) until it gets a
    /// response. Failing to forward these makes PowerShell hang at its prompt.
    pub(super) fn drain_vte_responses(&mut self) {
        for (sid, rx) in &mut self.pty_write_receivers {
            let mut buf = Vec::with_capacity(64);
            while let Ok(bytes) = rx.try_recv() {
                buf.extend_from_slice(&bytes);
            }
            if !buf.is_empty() {
                if let Ok(mgr) = self.session_manager.lock() {
                    if let Err(e) = mgr.send_input(*sid, &buf) {
                        warn!(?sid, error = %e, "Failed to forward VTE PtyWrite response");
                    }
                }
            }
        }
    }

    /// End compaction for sessions that have exceeded the configured timeout.
    pub(super) fn cleanup_compaction_timeouts(&mut self) {
        let timeout_secs = self
            .persistence
            .compaction
            .lock()
            .map(|svc| svc.timeout_secs())
            .unwrap_or(120);
        let timed_out: Vec<SessionId> = self
            .cache
            .compaction_start_times
            .iter()
            .filter(|(_, start)| start.elapsed() > Duration::from_secs(timeout_secs))
            .map(|(id, _)| *id)
            .collect();
        for session_id in timed_out {
            if let Ok(mut svc) = self.persistence.compaction.lock() {
                svc.end_compaction(session_id);
            }
            self.cache.compaction_start_times.remove(&session_id);
            self.event_bus
                .publish(CodirigentEvent::CompactionCompleted {
                    session_id,
                    success: false,
                });
            warn!(?session_id, "Compaction timed out");
        }
    }
}
