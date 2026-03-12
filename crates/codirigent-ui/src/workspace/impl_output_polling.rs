//! Output polling and session status management for WorkspaceView.
//!
//! This module contains the main output polling loop that:
//! - Drains PTY output and feeds it to terminal emulators
//! - Detects CLI types from output banners
//! - Processes shell state markers (OSC 133) and working directory changes (OSC 7)
//! - Reads Claude Code hook signal files for instant, low-overhead status updates
//! - Polls Codex/Gemini JSONL logs (background thread, ~3s interval)
//! - Manages automatic task assignment and context compaction
//! - Handles clipboard preview auto-show/hide
//!
//! Phase A scaffolding note:
//! - The root keeps shared types, constants, and orchestration methods.
//! - Child modules under `workspace/impl_output_polling/` are created now so
//!   later phases can move responsibility-based helper clusters without
//!   changing `workspace/mod.rs`.

mod cli_pollers;
mod git_refresh;
mod hook_signals;
mod output_runtime;
mod status_reconcile;
mod terminal_input;

// Phase A keeps all behavior in this root file. The child modules above are
// destination files for Phase B moves only.

use super::gpui::WorkspaceView;
use crate::terminal_runtime::TerminalRenderSnapshot;
use codirigent_core::{
    AssignmentAction, CliType, CodirigentEvent, EventBus, Session, SessionId, SessionManager,
    SessionUpdate,
};
use codirigent_session::clipboard_service::{ClipboardService, DefaultClipboardService};
use codirigent_session::detect_cli_from_output;
use gpui::Context;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};
use tracing::{info, trace, warn};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct DetectorMaintenanceBatch {
    session_ids: Vec<SessionId>,
}

/// When `CODIRIGENT_LEGACY_PIPELINE=1` is set, the event-driven output
/// dispatcher and status reconciler are disabled and the legacy broad-scan
/// polling path runs exclusively. This is a temporary kill switch for the
/// pipeline transition — it will be removed once shadow-mode validation
/// confirms zero diffs.
///
/// Read once at first access and cached for the process lifetime.
/// Changing the env var after startup has no effect.
static LEGACY_PIPELINE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
/// When `CODIRIGENT_SHADOW_STATUS=1` is set, the reconciler logs full
/// input/output detail for every status change and the legacy fallback
/// logs sessions that the event-driven path missed. Used to validate
/// correctness during the pipeline transition.
///
/// Read once at first access and cached for the process lifetime.
/// Changing the env var after startup has no effect.
static SHADOW_STATUS: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

fn is_legacy_pipeline() -> bool {
    *LEGACY_PIPELINE.get_or_init(|| {
        std::env::var("CODIRIGENT_LEGACY_PIPELINE")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
    })
}

fn is_shadow_status() -> bool {
    *SHADOW_STATUS.get_or_init(|| {
        std::env::var("CODIRIGENT_SHADOW_STATUS")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
    })
}

pub(super) fn init_app_start_ts() {
    hook_signals::init_app_start_ts();
}

fn merge_detector_maintenance_session_ids(
    changed_ids: Vec<SessionId>,
    stale_candidates: Vec<SessionId>,
) -> Vec<SessionId> {
    let mut seen = HashSet::new();
    changed_ids
        .into_iter()
        .chain(stale_candidates)
        .filter(|session_id| seen.insert(*session_id))
        .collect()
}

fn collect_detector_maintenance_batch(
    detector: &std::sync::Arc<std::sync::Mutex<codirigent_detector::InputDetector>>,
    cli_readers: &std::sync::Arc<std::sync::Mutex<super::types::CliReaders>>,
) -> DetectorMaintenanceBatch {
    let changed_ids = detector
        .lock()
        .ok()
        .map(|mut detector| detector.tick())
        .unwrap_or_default();
    let stale_candidates = cli_readers
        .lock()
        .ok()
        .map(|readers| readers.cached_status.keys().copied().collect())
        .unwrap_or_default();

    DetectorMaintenanceBatch {
        session_ids: merge_detector_maintenance_session_ids(changed_ids, stale_candidates),
    }
}

fn prioritize_and_partition_output_sessions<F>(
    mut session_ids: Vec<SessionId>,
    focused_id: Option<SessionId>,
    mut can_schedule: F,
) -> (Vec<SessionId>, Vec<SessionId>)
where
    F: FnMut(SessionId) -> bool,
{
    if let Some(focused_id) = focused_id {
        if let Some(index) = session_ids.iter().position(|id| *id == focused_id) {
            session_ids.swap(0, index);
        }
    }

    let mut ready = Vec::with_capacity(session_ids.len());
    let mut deferred = Vec::new();
    for session_id in session_ids {
        if can_schedule(session_id) {
            ready.push(session_id);
        } else {
            deferred.push(session_id);
        }
    }

    (ready, deferred)
}

#[derive(Debug)]
struct PreparedSessionOutput {
    session_id: SessionId,
    bytes_drained: usize,
    has_more: bool,
    render_snapshot: Option<TerminalRenderSnapshot>,
    detected_cli_type: Option<CliType>,
    cwd_session: Option<Session>,
}

enum ClipboardPreviewUpdate {
    NoChange,
    ChangedToNonImage,
    Image {
        signature: u64,
        preview: crate::smart_clipboard::ThumbnailPreview,
    },
}

fn clipboard_image_signature(image_data: &codirigent_core::ImageData) -> u64 {
    let mut hasher = DefaultHasher::new();
    image_data.width.hash(&mut hasher);
    image_data.height.hash(&mut hasher);
    image_data.bytes.len().hash(&mut hasher);
    image_data.format.extension().hash(&mut hasher);
    image_data.bytes.hash(&mut hasher);
    hasher.finish()
}

fn should_show_clipboard_preview(image_data: &codirigent_core::ImageData) -> bool {
    // Windows apps often place a tiny DIB/bitmap icon or thumbnail on the
    // clipboard alongside other content. Those are not useful as "image in
    // clipboard" previews and are a common source of false positives.
    if image_data.format == codirigent_core::ImageFormat::Dib
        && image_data.width <= 64
        && image_data.height <= 64
        && image_data.bytes.len() <= 16 * 1024
    {
        return false;
    }

    true
}

impl WorkspaceView {
    const GENERIC_SHELL_JSONL_MAX_AGE: Duration = Duration::from_secs(600);
    /// TTL for Codex/Gemini cached JSONL status. Shorter than hook signals because
    /// JSONL polling is infrequent (3s) and a stale cache entry is less reliable.
    const GENERIC_SHELL_JSONL_CACHE_TTL: Duration = Duration::from_secs(120);
    /// Interval between background JSONL checks and git refreshes (seconds).
    const BACKGROUND_REFRESH_INTERVAL: Duration = Duration::from_secs(3);
    /// How often to log session status in the idle polling loop (every N ticks).
    const STATUS_LOG_INTERVAL: u32 = 120;
    /// Delay before sending a deferred Enter keypress after a task prompt (ms).
    const PENDING_ENTER_DELAY: Duration = Duration::from_millis(100);
    /// TTL for Claude Code hook signal cache. Matches the 600s stale-signal guard
    /// so a long-running task never loses its "working" status just because no
    /// hook fired recently.
    const HOOK_SIGNAL_CACHE_TTL: Duration = Duration::from_secs(600);
    /// Maximum output bytes to process from a session in one poll tick.
    const MAX_OUTPUT_BYTES_PER_POLL: usize = 256 * 1024;
    /// Maximum PTY chunks to process from a session in one poll tick.
    const MAX_OUTPUT_CHUNKS_PER_POLL: usize = 64;
    /// How often the legacy fallback drains `pending_output_sessions` as a
    /// safety net for sessions that bypass the mpsc channel.
    const LEGACY_FALLBACK_INTERVAL: Duration = Duration::from_secs(1);

    pub(super) fn poll_output(&mut self, cx: &mut Context<Self>) {
        self.process_deferred_enters();
        self.drain_vte_responses();

        let had_output_activity = self.schedule_output_preparation(cx);

        // Track output activity for adaptive polling
        //
        // Sessions that actually produced output are synchronized in
        // `apply_prepared_session_output()`. Detector-based status decay stays
        // on the slower maintenance cadence to avoid O(all sessions) work on
        // every active 16 ms poll.
        self.polling.last_poll_had_output = had_output_activity;
    }

    pub(super) fn poll_maintenance(&mut self, cx: &mut Context<Self>) {
        self.spawn_background_hook_signal_check(cx);
        self.spawn_background_jsonl_check(cx);
        self.cleanup_compaction_timeouts();
        self.cleanup_stale_proposals();
        self.schedule_background_git_refresh(cx);

        if self.update_clipboard_preview(cx) {
            cx.notify();
        }
    }

    pub(super) fn spawn_background_detector_maintenance(&mut self, cx: &mut Context<Self>) {
        if self.terminals.is_empty() || self.polling.detector_maintenance_in_flight {
            return;
        }

        self.polling.detector_maintenance_in_flight = true;
        trace!("spawn_background_detector_maintenance");

        let detector = self.detector.clone();
        let cli_readers = self.cli_readers.clone();

        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let batch = cx
                .background_executor()
                .spawn(async move { collect_detector_maintenance_batch(&detector, &cli_readers) })
                .await;

            let _ = this.update(cx, |this, cx| {
                this.polling.detector_maintenance_in_flight = false;
                this.apply_detector_maintenance_batch(batch, cx);
            });
        })
        .detach();
    }

    fn apply_detector_maintenance_batch(
        &mut self,
        batch: DetectorMaintenanceBatch,
        cx: &mut Context<Self>,
    ) {
        if batch.session_ids.is_empty() {
            return;
        }

        trace!(
            changed = batch.session_ids.len(),
            "apply_detector_maintenance_batch"
        );

        let mut any_dirty = false;
        for session_id in batch.session_ids {
            if self.sync_session_status(session_id) {
                self.sync_session_header(session_id);
                any_dirty = true;
            }
        }

        if any_dirty {
            cx.notify();
        }
    }

    fn schedule_output_preparation(&mut self, cx: &mut Context<Self>) -> bool {
        if is_legacy_pipeline() {
            return self.schedule_output_preparation_legacy(cx);
        }

        // Phase 1: Drain the event-driven mpsc channel into the dispatcher.
        if let Some(ref mut rx) = self.update_rx {
            let other_events = self.output_dispatcher.drain_updates(rx);
            for event in other_events {
                match event {
                    SessionUpdate::ChildProcessExited { session_id } => {
                        // PTY child exited — mark session ready so it gets a
                        // final output drain and status re-evaluation.
                        trace!(
                            ?session_id,
                            "ChildProcessExited: marking ready for final drain"
                        );
                        self.output_dispatcher.mark_ready(session_id);
                    }
                    SessionUpdate::OutputReady { .. } => {
                        // Consumed by drain_updates into the ready set — should
                        // not appear here, but handle gracefully.
                    }
                    SessionUpdate::ShellStateChanged { session_id, .. }
                    | SessionUpdate::WorkingDirectoryChanged { session_id, .. } => {
                        // Phase-2: handled inline during output preparation
                        // (dual-path). Channel copies are informational only
                        // until phase-2 routing replaces the inline path.
                        trace!(?session_id, "phase-2 event received (not yet routed)");
                    }
                }
            }
        }

        // Phase 2: Low-frequency legacy safety net — drain the
        // pending_output_sessions set at ~1s intervals to catch any sessions
        // that bypass the mpsc channel (e.g., manual mark_output_pending
        // calls). This is NOT the hot path — the dispatcher handles that.
        if self.polling.last_legacy_fallback.elapsed() >= Self::LEGACY_FALLBACK_INTERVAL {
            self.polling.last_legacy_fallback = Instant::now();
            let legacy_ids =
                self.with_session_manager(|manager| manager.sessions_with_pending_output());
            if !legacy_ids.is_empty() {
                trace!(
                    count = legacy_ids.len(),
                    "legacy fallback drain (safety net)"
                );
                for id in &legacy_ids {
                    let was_new = self.output_dispatcher.mark_ready(*id);
                    // Shadow mode: log only genuinely missed events — sessions
                    // the mpsc channel didn't deliver to the dispatcher.
                    if is_shadow_status() && was_new {
                        info!(
                            ?id,
                            "shadow: legacy fallback discovered session not in dispatcher"
                        );
                    }
                }
            }
        }

        // Phase 3: Take ready sessions from the dispatcher (focused first).
        let session_ids = self
            .output_dispatcher
            .take_ready_sessions(self.workspace.focused_session_id());

        // Filter: only schedule sessions that have a terminal view.
        // Sessions without a terminal yet (gap between create_session and
        // terminals.insert) are re-queued so the next poll cycle picks them
        // up, avoiding a ~1s delay waiting for the legacy fallback.
        let mut schedulable = Vec::with_capacity(session_ids.len());
        for id in session_ids {
            if self.terminals.contains_key(&id) {
                schedulable.push(id);
            } else {
                self.output_dispatcher.mark_ready(id);
            }
        }

        let in_flight_count = self.output_dispatcher.in_flight_count();
        if !schedulable.is_empty() || in_flight_count > 0 {
            trace!(
                discovered_count = schedulable.len(),
                in_flight_count,
                "schedule_output_preparation"
            );
        }

        let had_output_activity = !schedulable.is_empty() || self.output_dispatcher.has_activity();

        for session_id in schedulable {
            self.schedule_session_output_preparation(session_id, cx);
        }

        had_output_activity
    }

    /// Legacy output preparation path — uses the broad
    /// `sessions_with_pending_output()` scan without the event-driven
    /// dispatcher. Activated by `CODIRIGENT_LEGACY_PIPELINE=1`.
    fn schedule_output_preparation_legacy(&mut self, cx: &mut Context<Self>) -> bool {
        let session_ids =
            self.with_session_manager(|manager| manager.sessions_with_pending_output());
        let (session_ids, deferred_ids) = prioritize_and_partition_output_sessions(
            session_ids,
            self.workspace.focused_session_id(),
            |id| {
                self.terminals.contains_key(&id)
                    && !self.polling.output_prepare_in_flight.contains(&id)
            },
        );

        if !deferred_ids.is_empty() {
            self.with_session_manager(|manager| {
                for session_id in deferred_ids {
                    manager.mark_output_pending(session_id);
                }
            });
        }

        let had_output_activity =
            !session_ids.is_empty() || !self.polling.output_prepare_in_flight.is_empty();

        for session_id in session_ids {
            self.schedule_session_output_preparation(session_id, cx);
        }

        had_output_activity
    }

    fn schedule_session_output_preparation(
        &mut self,
        session_id: SessionId,
        cx: &mut Context<Self>,
    ) {
        trace!(?session_id, "schedule_session_output_preparation");
        let Some(runtime) = self
            .terminals
            .get(&session_id)
            .map(|tv| tv.runtime_handle())
        else {
            trace!(
                ?session_id,
                "deferring output preparation until terminal runtime attaches"
            );
            self.output_dispatcher.mark_ready(session_id);
            self.with_session_manager(|manager| manager.mark_output_pending(session_id));
            return;
        };

        // Guard: prevent double-dispatch via the dispatcher's in-flight set.
        if !self.output_dispatcher.mark_in_flight(session_id) {
            return;
        }
        // TRANSITION: Legacy in-flight set kept in sync until
        // CODIRIGENT_LEGACY_PIPELINE and schedule_output_preparation_legacy
        // are removed. Both sets are always updated together.
        self.polling.output_prepare_in_flight.insert(session_id);
        debug_assert_eq!(
            self.polling.output_prepare_in_flight.len(),
            self.output_dispatcher.in_flight_count(),
            "dual in-flight sets desynchronized after marking session {} in-flight",
            session_id.0,
        );

        let session_manager = self.session_manager.clone();
        let detector = self.detector.clone();
        let update_tx = self.update_tx.clone();

        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let prepared = cx
                .background_executor()
                .spawn(async move {
                    let drained = {
                        let manager = session_manager.lock().ok()?;
                        manager.try_drain_output_bounded(
                            session_id,
                            Self::MAX_OUTPUT_CHUNKS_PER_POLL,
                            Self::MAX_OUTPUT_BYTES_PER_POLL,
                        )
                    }?;

                    let data = drained.data;
                    let bytes_drained = data.len();
                    let render_snapshot = runtime.apply_output(&data);
                    let detected_cli_type = detect_cli_from_output(&data);

                    {
                        let mut detector = detector.lock().ok()?;
                        detector.process_output(session_id, &data);
                        for event in codirigent_session::extract_osc133_events(&data) {
                            // DUAL-PATH: Emitted to channel for phase-2 event routing.
                            // Also applied directly below via set_shell_state() for correctness now.
                            if let Some(tx) = &update_tx {
                                if let Err(e) =
                                    tx.try_send(codirigent_core::SessionUpdate::ShellStateChanged {
                                        session_id,
                                        state: event.clone(),
                                    })
                                {
                                    trace!("ShellStateChanged try_send for {}: {e}", session_id.0);
                                }
                            }
                            detector.set_shell_state(session_id, event);
                        }
                    }

                    let cwd_session =
                        codirigent_session::extract_osc7_path(&data).and_then(|new_cwd| {
                            // DUAL-PATH: Emitted to channel for phase-2 event routing.
                            // Also applied directly below via update_working_directory() for correctness now.
                            if let Some(tx) = &update_tx {
                                if let Err(e) = tx.try_send(
                                    codirigent_core::SessionUpdate::WorkingDirectoryChanged {
                                        session_id,
                                        cwd: new_cwd.clone(),
                                    },
                                ) {
                                    trace!(
                                        "WorkingDirectoryChanged try_send for {}: {e}",
                                        session_id.0
                                    );
                                }
                            }
                            let manager = session_manager.lock().ok()?;
                            let changed = manager.update_working_directory(session_id, new_cwd);
                            if changed {
                                manager.get_session(session_id)
                            } else {
                                None
                            }
                        });

                    Some(PreparedSessionOutput {
                        session_id,
                        bytes_drained,
                        has_more: drained.has_more,
                        render_snapshot,
                        detected_cli_type,
                        cwd_session,
                    })
                })
                .await;

            let _ = this.update(cx, |this, cx| {
                this.polling.output_prepare_in_flight.remove(&session_id);
                this.output_dispatcher.complete_in_flight(session_id);
                debug_assert_eq!(
                    this.polling.output_prepare_in_flight.len(),
                    this.output_dispatcher.in_flight_count(),
                    "dual in-flight sets desynchronized after completing session {}",
                    session_id.0,
                );
                if let Some(prepared) = prepared {
                    this.apply_prepared_session_output(prepared, cx);
                } else {
                    // No output to drain (e.g. ChildProcessExited with no
                    // trailing bytes). Still run status reconciliation so
                    // OSC133-driven sessions don't stick in Working after
                    // the PTY exits.
                    if this.sync_session_status(session_id) {
                        this.sync_session_header(session_id);
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    fn apply_prepared_session_output(
        &mut self,
        prepared: PreparedSessionOutput,
        cx: &mut Context<Self>,
    ) {
        let PreparedSessionOutput {
            session_id,
            bytes_drained,
            has_more,
            render_snapshot,
            detected_cli_type,
            cwd_session,
        } = prepared;
        trace!(
            ?session_id,
            bytes_drained,
            has_more,
            "apply_prepared_session_output"
        );
        let mut any_dirty = false;

        if let Some(snapshot) = render_snapshot {
            if let Some(terminal_view) = self.terminals.get_mut(&session_id) {
                any_dirty |= terminal_view.apply_snapshot(snapshot);
            }
        }

        if let Some(cli_type) = detected_cli_type {
            let current = self
                .clipboard
                .clipboard_service
                .get_session_cli_type(session_id);
            if current == codirigent_core::CliType::GenericShell {
                self.clipboard
                    .clipboard_service
                    .set_session_cli_type(session_id, cli_type);
                info!(?session_id, ?cli_type, "Detected CLI type from output");
            }
        }

        if let Some(mgr_session) = cwd_session {
            if let Some(header) = self.terminal_headers.get_mut(&session_id) {
                header.git_branch = None;
                header.git_dirty_count = None;
            }

            if let Some(ws_session) = self.workspace.session_mut(session_id) {
                git_refresh::apply_cwd_session_update_from_manager(ws_session, &mgr_session);
            }

            if self.workspace.focused_session_id() == Some(session_id) {
                self.sync_file_tree_to_focused_session(cx);
            }

            self.spawn_session_git_refresh(session_id, mgr_session.working_directory.clone(), cx);
            any_dirty = true;
        }

        any_dirty |= self.sync_session_status(session_id);

        // Targeted delta: sync only this session's header instead of
        // dirtying the full UI sync path for every output poll.
        if any_dirty {
            self.sync_session_header(session_id);
            cx.notify();
        }
        if has_more {
            // Re-queue through the dispatcher so other sessions get fair
            // scheduling in the next poll cycle (16ms), instead of immediately
            // re-entering schedule_session_output_preparation which bypasses
            // the dispatcher's focused-first prioritization.
            self.output_dispatcher.mark_ready(session_id);
            // Also mark in the legacy pending set so the legacy path picks
            // it up when CODIRIGENT_LEGACY_PIPELINE=1 is active.
            self.with_session_manager(|manager| manager.mark_output_pending(session_id));
        }
    }

    /// Spawn a background JSONL status check for all sessions if the last check
    /// was more than 3 seconds ago and no check is currently in-flight.
    ///
    /// Reads JSONL files written by Claude Code, Codex, and Gemini CLIs and
    /// updates the cached session status on the UI thread.
    /// Reject pending task assignments whose target session became busy,
    /// and expire proposals older than 5 minutes.
    fn cleanup_stale_proposals(&mut self) {
        if let Ok(mut manager) = self.task_manager.lock() {
            let stale_task_ids: Vec<_> = manager
                .assignment()
                .pending_assignments()
                .iter()
                .filter(|p| {
                    self.workspace
                        .session(p.session_id)
                        .map_or(true, |s| s.current_task.is_some())
                })
                .map(|p| p.task_id.clone())
                .collect();
            for tid in stale_task_ids {
                manager.assignment_mut().reject_assignment(&tid);
            }
            manager.assignment_mut().clear_expired(300);
        }
    }

    /// Check the clipboard for new image content and start a background
    /// save/thumbnail if found. Auto-dismiss the preview after 4 seconds.
    ///
    /// All clipboard reads and image processing run off the UI thread. The
    /// platform clipboard providers handle any OS-specific main-thread rules.
    ///
    /// Returns `true` if `any_dirty` should be set (preview was auto-dismissed).
    fn update_clipboard_preview(&mut self, cx: &mut Context<Self>) -> bool {
        // Show new clipboard image if content changed
        if self.polling.last_clipboard_check.elapsed() >= Duration::from_secs(1)
            && !self.polling.clipboard_load_in_flight
        {
            self.polling.last_clipboard_check = Instant::now();
            self.polling.clipboard_load_in_flight = true;
            let clipboard = self.clipboard.smart_clipboard.clone();
            let temp_dir = self.clipboard.clipboard_service.temp_dir().to_path_buf();

            cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
                let update = cx
                    .background_executor()
                    .spawn(async move {
                        if !clipboard.has_changed() {
                            return ClipboardPreviewUpdate::NoChange;
                        }

                        // Many apps publish an auxiliary bitmap alongside text or
                        // file data. Only auto-show the image overlay for image-only
                        // clipboard changes.
                        if clipboard.has_text() || clipboard.has_files() {
                            return ClipboardPreviewUpdate::ChangedToNonImage;
                        }

                        match clipboard.read_content() {
                            Ok(codirigent_core::ClipboardContent::Image(image_data)) => {
                                if !should_show_clipboard_preview(&image_data) {
                                    return ClipboardPreviewUpdate::ChangedToNonImage;
                                }
                                let signature = clipboard_image_signature(&image_data);
                                let _ = std::fs::create_dir_all(&temp_dir);
                                let svc = DefaultClipboardService::new(
                                    temp_dir.parent().unwrap_or(&temp_dir),
                                );
                                let path = match svc.save_image(&image_data) {
                                    Ok(path) => path,
                                    Err(_) => return ClipboardPreviewUpdate::NoChange,
                                };
                                let file_size = image_data.bytes.len() as u64;
                                let preview =
                                    crate::clipboard_preview::ClipboardPreview::create_preview(
                                        &image_data,
                                        path,
                                        file_size,
                                    );
                                ClipboardPreviewUpdate::Image { signature, preview }
                            }
                            Ok(_) => ClipboardPreviewUpdate::ChangedToNonImage,
                            Err(_) => ClipboardPreviewUpdate::NoChange,
                        }
                    })
                    .await;

                let _ = this.update(cx, |this, cx| {
                    this.polling.clipboard_load_in_flight = false;
                    match update {
                        ClipboardPreviewUpdate::NoChange => {}
                        ClipboardPreviewUpdate::ChangedToNonImage => {
                            this.clipboard.last_preview_image_signature = None;
                        }
                        ClipboardPreviewUpdate::Image { signature, preview } => {
                            if this.clipboard.last_preview_image_signature != Some(signature) {
                                this.clipboard.last_preview_image_signature = Some(signature);
                                this.clipboard.clipboard_preview.show(preview);
                                this.clipboard.clipboard_preview_shown_at =
                                    Some(std::time::Instant::now());
                                cx.notify();
                            }
                        }
                    }
                });
            })
            .detach();
        }

        // Auto-dismiss after 4 seconds (checked every poll, not just the 1-second interval)
        if self.clipboard.clipboard_preview.is_visible() {
            if let Some(shown_at) = self.clipboard.clipboard_preview_shown_at {
                if shown_at.elapsed() > std::time::Duration::from_secs(4) {
                    self.clipboard.clipboard_preview.hide();
                    self.clipboard.clipboard_preview_shown_at = None;
                    return true;
                }
            }
        }

        false
    }

    /// Try to compact a session before verification.
    /// Returns true if compaction was started, false if skipped.
    fn try_compact(&mut self, session_id: SessionId) -> bool {
        let context_usage = self
            .workspace
            .session(session_id)
            .and_then(|s| s.context_usage);

        let command = {
            let mut svc = match self.persistence.compaction.lock() {
                Ok(s) => s,
                Err(_) => return false,
            };
            if !svc.should_compact(session_id, context_usage) {
                return false;
            }
            if !svc.begin_compaction(session_id) {
                return false;
            }
            svc.compact_command()
        };

        // Send /compact via PTY stdin
        if let Ok(mgr) = self.session_manager.lock() {
            if let Err(e) = mgr.send_input(session_id, command.as_bytes()) {
                warn!(?session_id, error = %e, "Failed to send /compact command");
                if let Ok(mut svc) = self.persistence.compaction.lock() {
                    svc.end_compaction(session_id);
                }
                return false;
            }
        }

        self.cache
            .compaction_start_times
            .insert(session_id, Instant::now());

        let focus = self
            .persistence
            .compaction
            .lock()
            .ok()
            .and_then(|svc| svc.config().focus_instructions.clone());
        self.event_bus
            .publish(CodirigentEvent::CompactionStarted { session_id, focus });

        info!(?session_id, "Compaction started");
        true
    }

    /// Try to auto-assign a queued task to a session that just became idle.
    ///
    /// Checks whether auto-assign is enabled and a task is available, then
    /// confirms the assignment, updates the session's `current_task`, and
    /// sends the generated prompt to the session's PTY.
    fn try_auto_assign(&mut self, session_id: SessionId) {
        let session = match self.workspace.session(session_id) {
            Some(s) => s.clone(),
            None => return,
        };

        // Skip if session already has a task assigned
        if session.current_task.is_some() {
            return;
        }

        // Never auto-assign to bare shell sessions before CLI is detected
        let cli_type = self
            .clipboard
            .clipboard_service
            .get_session_cli_type(session_id);
        if cli_type == codirigent_core::CliType::GenericShell {
            return;
        }

        // Block auto-assign until the user has manually assigned at least once.
        // A freshly-started CLI may need auth, config, or other user input first.
        if !self.cache.manually_assigned_sessions.contains(&session_id) {
            return;
        }

        let action = {
            let mut manager = match self.task_manager.lock() {
                Ok(m) => m,
                Err(_) => return,
            };
            manager.on_session_idle(&session)
        };

        match action {
            Some(AssignmentAction::AssignNow {
                task_id,
                session_id: target_id,
                prompt,
            }) => {
                // AssignNow already has the prompt; directly assign via queue
                {
                    let mut manager = match self.task_manager.lock() {
                        Ok(m) => m,
                        Err(_) => return,
                    };
                    if let Err(e) = manager.queue_mut().assign_task(&task_id, target_id) {
                        warn!("Failed to assign task in queue: {}", e);
                        return;
                    }
                }

                self.send_task_to_session(&task_id, target_id, &prompt);
                info!(?task_id, ?target_id, "Auto-assigned task to session");
            }
            Some(AssignmentAction::AwaitConfirmation {
                task_id,
                session_id: target_id,
            }) => {
                // Pending assignment is stored in AssignmentManager.pending;
                // the UI will render the confirmation banner on next frame.
                info!(
                    ?task_id,
                    ?target_id,
                    "Task proposed; awaiting user confirmation"
                );
            }
            Some(AssignmentAction::NoTask) | None => {}
        }
    }
}

#[cfg(test)]
mod tests;
