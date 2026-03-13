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
//! Split note:
//! - The root keeps shared types, constants, and maintenance orchestration.
//! - Responsibility-based polling clusters now live under
//!   `workspace/impl_output_polling/` without changing `workspace/mod.rs`.

mod cli_pollers;
mod git_refresh;
mod hook_signals;
mod output_runtime;
mod status_reconcile;
mod terminal_input;

// The root still owns shared polling state, detector maintenance, clipboard
// preview, and task/compaction orchestration. Hot-path polling helpers now
// live in the child modules above.

use super::gpui::WorkspaceView;
use codirigent_core::{AssignmentAction, CodirigentEvent, EventBus, SessionId, SessionManager};
use codirigent_session::clipboard_service::{ClipboardService, DefaultClipboardService};
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
