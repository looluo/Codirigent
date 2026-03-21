//! Session-status reconciliation helpers.

use super::super::cli_helpers::clear_command;
use super::super::status_engine::reconcile;
use super::super::status_providers::{HintSource, StaleAction};
use super::super::types::CliStatusSource;
use super::WorkspaceView;
use codirigent_core::{
    CliType, CodirigentEvent, EventBus, ProcessMonitor, SessionId, SessionManager, SessionStatus,
    TaskStatus,
};
use codirigent_session::clipboard_service::ClipboardService;
use std::time::Instant;
use tracing::info;

impl WorkspaceView {
    /// Update session status from detector/cache state.
    ///
    /// Uses the status reconciler ([`super::super::status_engine::reconcile`]) to
    /// combine detector hints with cached CLI hints, then applies side effects
    /// (task transitions, compaction, auto-assign, notifications).
    ///
    /// Returns `true` if any UI-visible change was made that requires a repaint.
    pub(super) fn sync_session_status(&mut self, session_id: SessionId) -> bool {
        let mut any_dirty = false;

        // Gather inputs for the reconciler
        let (detector_status, idle_time) = self.with_detector(|detector| {
            (
                detector.get_status(session_id),
                detector.get_idle_time(session_id),
            )
        });

        // Gather cached CLI status in a single lock acquisition to ensure
        // consistency between status, source, and age.
        let (cached_status, cached_source, cache_age) = self
            .cli_readers
            .lock()
            .ok()
            .and_then(|mut readers| {
                let cached = readers.cached_status.get(&session_id)?;
                if cached.seen_at.elapsed() > cached.ttl {
                    // Hook-sourced resting states (Idle, ResponseReady) represent
                    // a stable "waiting for user input" condition that should
                    // persist until a new hook event arrives.  Evicting them
                    // causes the detector's Working status (from the shell's
                    // CommandExecuted state) to take over, producing a false
                    // yellow indicator on long-idle Claude Code sessions.
                    let is_resting_hook = cached.source == CliStatusSource::Hook
                        && matches!(
                            cached.status,
                            SessionStatus::Idle | SessionStatus::ResponseReady
                        );
                    if !is_resting_hook {
                        readers.cached_status.remove(&session_id);
                        return None;
                    }
                }
                let source = match cached.source {
                    CliStatusSource::Hook => HintSource::HookSignal,
                    CliStatusSource::Jsonl => HintSource::Jsonl,
                };
                let age = Some(cached.status_since.elapsed());
                Some((Some(cached.status), source, age))
            })
            .unwrap_or((None, HintSource::Detector, None));

        let previous_status = self.workspace.session(session_id).map(|s| s.status);

        let (reconciled, stale_action) = reconcile(
            session_id,
            detector_status,
            cached_status,
            cached_source,
            cache_age,
            previous_status,
        );

        if super::is_shadow_status() {
            if let Some(ref r) = reconciled {
                if r.changed {
                    info!(
                        ?session_id,
                        ?detector_status,
                        ?cached_status,
                        ?cached_source,
                        ?cache_age,
                        ?previous_status,
                        reconciled_status = ?r.status,
                        reconciled_source = ?r.source,
                        ?stale_action,
                        "shadow: reconciler status change"
                    );
                }
            }
        }

        match stale_action {
            StaleAction::ClearAndRevert {
                session_id: stale_id,
            } => {
                if let Ok(mut readers) = self.cli_readers.lock() {
                    readers.cached_status.remove(&stale_id);
                }
                self.clipboard
                    .clipboard_service
                    .set_session_cli_type(stale_id, CliType::GenericShell);
                info!(
                    ?stale_id,
                    "Cleared stale NeedsAttention, reverted to GenericShell"
                );
            }
            StaleAction::None => {}
        }

        if let Some(reconciled) = reconciled {
            let status = reconciled.status;
            if self.polling.idle_poll_count % Self::STATUS_LOG_INTERVAL == 0 {
                info!(?session_id, ?status, ?idle_time, "Session status poll");
            }
            let old_status = self.workspace.session(session_id).map(|s| s.status);
            let mut just_started_compaction = false;
            if self.workspace.update_session_status(session_id, status) {
                any_dirty = true;
                if let Some(old) = old_status {
                    let task_transitioned_to_review =
                        if let Ok(mut task_mgr) = self.task_manager.lock() {
                            let tid = task_mgr.on_session_status_changed(session_id, old, status);
                            if let Some(ref task_id) = tid {
                                task_mgr
                                    .get_task(task_id)
                                    .is_some_and(|t| t.status == TaskStatus::Review)
                            } else {
                                false
                            }
                        } else {
                            false
                        };

                    // When task auto-transitions to Review:
                    // 1. Clear current_task so auto-assign can work later
                    // 2. Send /clear to reset context for the next task.
                    if task_transitioned_to_review {
                        // Keep the previous JSONL status during transient parse/IO misses.
                        if let Ok(mgr) = self.session_manager.lock() {
                            mgr.with_session_state_mut(session_id, |state| {
                                state.session.current_task = None;
                            });
                        }
                        if let Some(session) = self.workspace.session_mut(session_id) {
                            session.current_task = None;
                        }
                        // Start context clear and reuse compaction infrastructure
                        let cli_type = self
                            .clipboard
                            .clipboard_service
                            .get_session_cli_type(session_id);
                        let clear_cmd = clear_command(cli_type);
                        if let Ok(mut svc) = self.persistence.compaction.lock() {
                            if svc.begin_compaction(session_id) {
                                if let Ok(mgr) = self.session_manager.lock() {
                                    let _ = mgr.send_input(session_id, clear_cmd.as_bytes());
                                }
                                self.polling
                                    .pending_enters
                                    .insert(session_id, (Instant::now(), false));
                                self.cache
                                    .compaction_start_times
                                    .insert(session_id, Instant::now());
                                just_started_compaction = true;
                            }
                        }
                    }
                }
                self.sync_task_derived_state();
            }

            // NeedsAttention is NOT treated as idle because session is blocked
            // Skip if we just started compaction and wait for /clear to finish
            // Skip if a deferred Enter is pending because text hasn't been submitted yet
            if matches!(status, SessionStatus::Idle)
                && !just_started_compaction
                && !self.polling.pending_enters.contains_key(&session_id)
            {
                let is_compacting = self
                    .persistence
                    .compaction
                    .lock()
                    .map(|svc| svc.is_compacting(session_id))
                    .unwrap_or(false);

                if is_compacting {
                    if let Ok(mut svc) = self.persistence.compaction.lock() {
                        svc.end_compaction(session_id);
                    }
                    self.cache.compaction_start_times.remove(&session_id);
                    self.event_bus
                        .publish(CodirigentEvent::CompactionCompleted {
                            session_id,
                            success: true,
                        });
                    info!(?session_id, "Compaction completed successfully");
                } else {
                    let has_task = self
                        .workspace
                        .session(session_id)
                        .is_some_and(|s| s.current_task.is_some());
                    if has_task && self.try_compact(session_id) {
                        return any_dirty;
                    }
                }

                self.try_auto_assign(session_id);
            }
        }
        any_dirty
    }
}
