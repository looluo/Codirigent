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

use super::cli_helpers::clear_command;
use super::gpui::WorkspaceView;
use super::types::CachedCliStatus;
use codirigent_core::{
    hook_signals_dir, AssignmentAction, CliType, CodirigentEvent, EventBus, ProcessMonitor,
    Session, SessionId, SessionManager, SessionStatus, TaskStatus,
};
use codirigent_detector::NotificationType;
use codirigent_session::cli_detector::CliDetector;
use codirigent_session::clipboard_service::{ClipboardService, DefaultClipboardService};
use codirigent_session::detect_cli_from_output;
use codirigent_session::CliSessionStatus;
use gpui::Context;
use serde::Deserialize;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tracing::{info, warn};

const CLI_TYPE_CLAUDE: &str = "claude";
const CLI_TYPE_GEMINI: &str = "gemini";
const CLI_TYPE_CODEX: &str = "codex";

/// Session status result from a background JSONL read: (status, optional detail string).
type JsonlStatusResult = Option<(SessionStatus, Option<String>)>;

#[derive(Debug)]
struct PreparedSessionOutput {
    session_id: SessionId,
    data: Vec<u8>,
    has_more: bool,
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

/// Signal file written by `codirigent-hook` for each hook event.
#[derive(Deserialize)]
struct HookSignal {
    status: String,
    cli_type: Option<String>,
    /// Codirigent session ID, present only when Claude Code was spawned by Codirigent
    /// (via the `CODIRIGENT_SESSION_ID` environment variable).
    codirigent_session_id: Option<String>,
    ts: u64,
}

#[derive(Debug)]
struct HookSignalUpdate {
    session_id: SessionId,
    cli_session_id: String,
    status: String,
    cli_type: Option<String>,
}

fn read_recent_hook_signal_updates() -> Vec<HookSignalUpdate> {
    let signals_dir = match hook_signals_dir() {
        Some(d) => d,
        None => return Vec::new(),
    };

    let entries = match std::fs::read_dir(&signals_dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    let now_ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let mut updates = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }

        let cli_session_id = match path.file_stem().and_then(|s| s.to_str()) {
            Some(s) => s.to_owned(),
            None => continue,
        };

        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let signal: HookSignal = match serde_json::from_str(&content) {
            Ok(s) => s,
            Err(_) => continue,
        };

        if now_ts.saturating_sub(signal.ts) > 600 {
            continue;
        }

        let session_id = match signal
            .codirigent_session_id
            .as_deref()
            .and_then(|id| id.parse::<u64>().ok())
        {
            Some(id) => SessionId(id),
            None => continue,
        };

        updates.push(HookSignalUpdate {
            session_id,
            cli_session_id,
            status: signal.status,
            cli_type: signal.cli_type,
        });
    }

    updates
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
    /// in check_hook_signals so a long-running task never loses its "working" status
    /// just because no hook fired recently.
    const HOOK_SIGNAL_CACHE_TTL: Duration = Duration::from_secs(600);
    /// Maximum output bytes to process from a session in one poll tick.
    const MAX_OUTPUT_BYTES_PER_POLL: usize = 256 * 1024;
    /// Maximum PTY chunks to process from a session in one poll tick.
    const MAX_OUTPUT_CHUNKS_PER_POLL: usize = 64;

    pub(super) fn poll_output(&mut self, cx: &mut Context<Self>) {
        self.process_deferred_enters();
        self.drain_vte_responses();

        let had_output_activity = self.schedule_output_preparation(cx);

        let mut session_ids: Vec<codirigent_core::SessionId> =
            self.terminals.keys().copied().collect();
        if let Some(focused_id) = self.workspace.focused_session_id() {
            if let Some(index) = session_ids.iter().position(|id| *id == focused_id) {
                session_ids.swap(0, index);
            }
        }
        let mut any_dirty = false;

        for session_id in session_ids {
            let session_dirty = self.sync_session_status(session_id);
            any_dirty |= session_dirty;
        }

        // Track output activity for adaptive polling
        self.polling.last_poll_had_output = had_output_activity;

        if any_dirty {
            cx.notify();
        }
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

    fn schedule_output_preparation(&mut self, cx: &mut Context<Self>) -> bool {
        let mut session_ids =
            self.with_session_manager(|manager| manager.sessions_with_pending_output());
        if let Some(focused_id) = self.workspace.focused_session_id() {
            if let Some(index) = session_ids.iter().position(|id| *id == focused_id) {
                session_ids.swap(0, index);
            }
        }
        session_ids.retain(|id| self.terminals.contains_key(id));

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
        if !self.polling.output_prepare_in_flight.insert(session_id) {
            return;
        }

        let session_manager = self.session_manager.clone();
        let detector = self.detector.clone();

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
                    let detected_cli_type = detect_cli_from_output(&data);

                    {
                        let mut detector = detector.lock().ok()?;
                        detector.process_output(session_id, &data);
                        for event in codirigent_session::extract_osc133_events(&data) {
                            detector.set_shell_state(session_id, event);
                        }
                    }

                    let cwd_session =
                        codirigent_session::extract_osc7_path(&data).and_then(|new_cwd| {
                            let manager = session_manager.lock().ok()?;
                            let changed = manager.update_working_directory(session_id, new_cwd);
                            if changed {
                                manager.invalidate_git_cache(session_id);
                                manager.get_session(session_id)
                            } else {
                                None
                            }
                        });

                    Some(PreparedSessionOutput {
                        session_id,
                        data,
                        has_more: drained.has_more,
                        detected_cli_type,
                        cwd_session,
                    })
                })
                .await;

            let _ = this.update(cx, |this, cx| {
                this.polling.output_prepare_in_flight.remove(&session_id);
                if let Some(prepared) = prepared {
                    this.apply_prepared_session_output(prepared, cx);
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
            data,
            has_more,
            detected_cli_type,
            cwd_session,
        } = prepared;
        let mut any_dirty = false;

        if let Some(terminal_view) = self.terminals.get_mut(&session_id) {
            terminal_view.terminal_mut().process_output(&data);
            any_dirty = true;
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
                ws_session.working_directory = mgr_session.working_directory.clone();
                ws_session.group = None;
                ws_session.git_info = None;
            }

            if self.workspace.focused_session_id() == Some(session_id) {
                self.sync_file_tree_to_focused_session(cx);
            }

            self.spawn_session_git_refresh(session_id, mgr_session.working_directory.clone(), cx);
            self.mark_ui_sync_dirty();
            any_dirty = true;
        }

        any_dirty |= self.sync_session_status(session_id);
        if any_dirty {
            cx.notify();
        }
        if has_more {
            self.schedule_session_output_preparation(session_id, cx);
        }
    }

    /// Update session status from detector/cache state.
    ///
    /// Returns `true` if any UI-visible change was made that requires a repaint.
    fn sync_session_status(&mut self, session_id: codirigent_core::SessionId) -> bool {
        let mut any_dirty = false;

        // Update session status from detector
        let (mut status, idle_time) = self.with_detector(|detector| {
            (
                detector.get_status(session_id),
                detector.get_idle_time(session_id),
            )
        });

        // Overlay cached JSONL status (background task updates the cache).
        // But don't overlay stale NeedsAttention when the detector says Idle —
        // this means Claude exited and the shell prompt is showing.
        if let Some((cached_status, _tool_name)) = self.get_recent_cached_cli_status(session_id) {
            let is_stale_attention = cached_status == SessionStatus::NeedsAttention
                && matches!(status, Some(SessionStatus::Idle))
                && self.is_cli_status_stale(session_id, Duration::from_secs(30));
            if is_stale_attention {
                // Claude likely exited — clear JSONL cache AND revert CLI type
                // so the JSONL reader stops being consulted on subsequent polls.
                if let Ok(mut readers) = self.cli_readers.lock() {
                    readers.cached_status.remove(&session_id);
                }
                self.clipboard
                    .clipboard_service
                    .set_session_cli_type(session_id, codirigent_core::CliType::GenericShell);
                info!(
                    ?session_id,
                    "Cleared stale NeedsAttention, reverted to GenericShell"
                );
            } else {
                status = Some(cached_status);
            }
        }

        if let Some(status) = status {
            if self.polling.idle_poll_count % Self::STATUS_LOG_INTERVAL == 0 {
                info!(?session_id, ?status, ?idle_time, "Session status poll");
            }
            let old_status = self.workspace.session(session_id).map(|s| s.status);
            let mut just_started_compaction = false;
            if self.workspace.update_session_status(session_id, status) {
                self.mark_ui_sync_dirty();
                any_dirty = true;
                // Sync task board with the canonical (JSONL-corrected) status
                if let Some(old) = old_status {
                    // Check if task transitioned to Review
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
                        self.mark_ui_sync_dirty();
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
                    // Compaction just finished and session returned to Idle
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
                    // Fall through to try_auto_assign
                } else {
                    // Not compacting; check if we should compact before proceeding
                    let has_task = self
                        .workspace
                        .session(session_id)
                        .is_some_and(|s| s.current_task.is_some());
                    if has_task && self.try_compact(session_id) {
                        // Compaction started; skip auto-assign this cycle
                        return any_dirty;
                    }
                }

                self.try_auto_assign(session_id);
            }
        }
        any_dirty
    }

    /// Spawn a background JSONL status check for all sessions if the last check
    /// was more than 3 seconds ago and no check is currently in-flight.
    ///
    /// Reads JSONL files written by Claude Code, Codex, and Gemini CLIs and
    /// updates the cached session status on the UI thread.
    fn spawn_background_jsonl_check(&mut self, cx: &mut Context<Self>) {
        let has_any_reader = self
            .cli_readers
            .lock()
            .map(|r| r.codex.is_some() || r.gemini.is_some())
            .unwrap_or(false);
        if !has_any_reader
            || self.polling.last_jsonl_check.elapsed() < Self::BACKGROUND_REFRESH_INTERVAL
            || self.polling.jsonl_check_in_flight
        {
            return;
        }
        self.polling.last_jsonl_check = Instant::now();
        self.polling.jsonl_check_in_flight = true;

        // Collect inputs for background JSONL check
        let jsonl_inputs: Vec<(
            SessionId,
            std::path::PathBuf,
            Option<u32>,
            codirigent_core::CliType,
            SessionStatus, // current status for transition detection
            i64,           // session created_at (unix timestamp for JSONL file filtering)
        )> = self
            .workspace
            .sessions()
            .iter()
            .map(|s| {
                let cli_type = self.clipboard.clipboard_service.get_session_cli_type(s.id);
                let child_pid = self.with_session_manager(|manager| manager.get_child_pid(s.id));
                (
                    s.id,
                    s.working_directory.clone(),
                    child_pid,
                    cli_type,
                    s.status,
                    s.created_at.timestamp(),
                )
            })
            .collect();

        let cli_readers = self.cli_readers.clone();
        let event_bus = self.event_bus.clone();
        let max_age = Self::GENERIC_SHELL_JSONL_MAX_AGE;

        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            // Background: perform JSONL reads (the expensive I/O)
            let results = cx
                .background_executor()
                .spawn(async move {
                    let mut out: Vec<(SessionId, JsonlStatusResult)> = Vec::new();
                    let mut detected_types: Vec<(SessionId, codirigent_core::CliType)> = Vec::new();
                    if let Ok(mut readers) = cli_readers.lock() {
                        for (
                            session_id,
                            working_dir,
                            child_pid,
                            cli_type,
                            _current_status,
                            _created_at,
                        ) in &jsonl_inputs
                        {
                            // For GenericShell sessions, try process-tree detection.
                            // The detector walks the PTY's child processes looking
                            // for known CLI binaries (claude, gemini, codex).
                            // Note: don't use process-tree to REVERT ClaudeCode → GenericShell
                            // because detection is unreliable (returns GenericShell even when
                            // Claude is running). Banner detection handles initial detection.
                            let effective_type =
                                if *cli_type == codirigent_core::CliType::GenericShell {
                                    if let Some(pid) = child_pid {
                                        let detected = readers.detector.detect_cli_type(*pid);
                                        if detected != codirigent_core::CliType::GenericShell {
                                            info!(
                                                ?session_id,
                                                ?detected,
                                                "Process-tree detected CLI type"
                                            );
                                            detected_types.push((*session_id, detected));
                                            detected
                                        } else {
                                            *cli_type
                                        }
                                    } else {
                                        *cli_type
                                    }
                                } else {
                                    *cli_type
                                };

                            let cli_status: Option<CliSessionStatus> = match effective_type {
                                codirigent_core::CliType::ClaudeCode => {
                                    // Claude Code status is handled by hook signal files
                                    // (check_hook_signals) — no JSONL reader needed here.
                                    None
                                }
                                codirigent_core::CliType::CodexCli => {
                                    readers.codex.as_mut().and_then(|r| {
                                        r.get_status_if_recent(working_dir, *child_pid, max_age)
                                    })
                                }
                                codirigent_core::CliType::GeminiCli => {
                                    readers.gemini.as_mut().and_then(|r| {
                                        r.get_status_if_recent(working_dir, *child_pid, max_age)
                                    })
                                }
                                codirigent_core::CliType::GenericShell => None,
                            };
                            let resolved = cli_status.and_then(|s| s.to_session_status());
                            out.push((*session_id, resolved));
                        }
                    }
                    (out, jsonl_inputs, detected_types)
                })
                .await;

            // Marshal results back to UI thread
            let _ = this.update(cx, |this, cx| {
                this.polling.jsonl_check_in_flight = false;
                let mut changed = false;
                let (results, inputs, detected_types) = results;
                let input_statuses: std::collections::HashMap<SessionId, SessionStatus> = inputs
                    .iter()
                    .map(|(id, _, _, _, status, _)| (*id, *status))
                    .collect();
                let cache_update_time = Instant::now();
                let mut cached_status = this.cli_readers.lock().ok();

                // Apply process-tree CLI type detections on UI thread
                for (session_id, detected_type) in &detected_types {
                    this.clipboard
                        .clipboard_service
                        .set_session_cli_type(*session_id, *detected_type);
                    info!(
                        ?session_id,
                        ?detected_type,
                        "Applied process-tree CLI type detection"
                    );
                }
                for (session_id, cli_status) in &results {
                    if let Some((new_status, tool_name)) = cli_status {
                        // Cache the JSONL result
                        if let Some(readers) = cached_status.as_mut() {
                            // Preserve status_since if status hasn't changed
                            let status_since = readers
                                .cached_status
                                .get(session_id)
                                .filter(|c| c.status == *new_status)
                                .map(|c| c.status_since)
                                .unwrap_or(cache_update_time);
                            readers.cached_status.insert(
                                *session_id,
                                CachedCliStatus {
                                    status: *new_status,
                                    tool_name: tool_name.clone(),
                                    seen_at: cache_update_time,
                                    status_since,
                                    ttl: Self::GENERIC_SHELL_JSONL_CACHE_TTL,
                                },
                            );
                        }
                        // Fire AttentionRequired on transition
                        if *new_status == SessionStatus::NeedsAttention {
                            let current_status = input_statuses.get(session_id).copied();
                            if current_status != Some(SessionStatus::NeedsAttention) {
                                event_bus.publish(CodirigentEvent::AttentionRequired {
                                    session_id: *session_id,
                                    detail: tool_name.clone(),
                                });
                                let session_name = this
                                    .workspace
                                    .session(*session_id)
                                    .map(|s| s.name.clone())
                                    .unwrap_or_else(|| format!("Session {}", session_id.0));
                                let (notif_type, detail) = match tool_name.as_deref() {
                                    Some("question") | None => {
                                        (NotificationType::InputRequired, None)
                                    }
                                    Some(tool) => (NotificationType::PermissionPrompt, Some(tool)),
                                };
                                this.notification_manager.notify(
                                    notif_type,
                                    *session_id,
                                    &session_name,
                                    detail,
                                );
                            }
                        }
                        changed = true;
                    } else {
                        // No JSONL result — check if detector says idle and clear stale cache
                        let detector_idle = this.with_detector(|detector| {
                            matches!(
                                detector.get_status(*session_id),
                                Some(SessionStatus::Idle) | None
                            )
                        });
                        if detector_idle {
                            if let Some(readers) = cached_status.as_mut() {
                                let is_stale = readers
                                    .cached_status
                                    .get(session_id)
                                    .map(|c| c.seen_at.elapsed() > c.ttl)
                                    .unwrap_or(false);
                                if is_stale {
                                    readers.cached_status.remove(session_id);
                                    changed = true;
                                }
                            }
                        }
                    }
                }
                if changed {
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Send deferred Enter keypresses and clean up phase-2 grace periods.
    ///
    /// Task input is split into two PTY writes (the prompt text, then `\r`) so
    /// that the CLI treats them as separate stdin events. This helper runs the
    /// two-phase timing logic:
    ///
    /// - Phase 1: send `\r` once 100 ms have elapsed since the text was sent.
    /// - Phase 2: remove the entry after a 500 ms grace period so auto-assign
    ///   does not consider the session available while the CLI processes the command.
    fn process_deferred_enters(&mut self) {
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
    fn drain_vte_responses(&mut self) {
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
    fn cleanup_compaction_timeouts(&mut self) {
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

    /// Spawn a background git-status refresh for all sessions if the last
    /// refresh was more than 3 seconds ago and no refresh is in-flight.
    fn schedule_background_git_refresh(&mut self, cx: &mut Context<Self>) {
        if self.polling.last_git_refresh.elapsed() < Self::BACKGROUND_REFRESH_INTERVAL
            || self.polling.git_refresh_in_flight
        {
            return;
        }
        self.polling.last_git_refresh = Instant::now();
        self.polling.git_refresh_in_flight = true;
        let session_ids: Vec<SessionId> = self.workspace.sessions().iter().map(|s| s.id).collect();
        let session_manager = self.session_manager.clone();

        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let git_infos = cx
                .background_executor()
                .spawn(async move {
                    let mgr = match session_manager.lock() {
                        Ok(m) => m,
                        Err(_) => return Vec::new(),
                    };
                    session_ids
                        .iter()
                        .map(|id| (*id, mgr.refresh_git_status(*id)))
                        .collect::<Vec<_>>()
                })
                .await;

            let _ = this.update(cx, |this, cx| {
                this.polling.git_refresh_in_flight = false;
                let mut git_changed = false;
                for (id, git_info) in &git_infos {
                    if let Some(header) = this.terminal_headers.get_mut(id) {
                        let branch = git_info.as_ref().map(|info| info.branch.clone());
                        let dirty_count = git_info.as_ref().map(|info| info.dirty_count);
                        if header.git_branch != branch || header.git_dirty_count != dirty_count {
                            header.git_branch = branch;
                            header.git_dirty_count = dirty_count;
                            git_changed = true;
                        }
                    }
                    if let Some(session) = this.workspace.session_mut(*id) {
                        let next_group = git_info.as_ref().map(|info| info.branch.clone());
                        if session.git_info != *git_info || session.group != next_group {
                            session.git_info = git_info.clone();
                            session.group = next_group;
                            git_changed = true;
                        }
                    }
                }
                if git_changed {
                    this.mark_ui_sync_dirty();
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn spawn_session_git_refresh(
        &mut self,
        session_id: SessionId,
        expected_cwd: std::path::PathBuf,
        cx: &mut Context<Self>,
    ) {
        let session_manager = self.session_manager.clone();

        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let expected_cwd_for_bg = expected_cwd.clone();
            let git_info = cx
                .background_executor()
                .spawn(async move {
                    let mgr = session_manager.lock().ok()?;
                    let session = mgr.get_session(session_id)?;
                    if session.working_directory != expected_cwd_for_bg {
                        return None;
                    }
                    Some((
                        session_id,
                        expected_cwd_for_bg,
                        mgr.refresh_git_status(session_id),
                    ))
                })
                .await;

            let _ = this.update(cx, |this, cx| {
                let Some((session_id, expected_cwd, git_info)) = git_info else {
                    return;
                };
                if !this
                    .workspace
                    .session(session_id)
                    .is_some_and(|session| session.working_directory == expected_cwd)
                {
                    return;
                }

                let branch = git_info.as_ref().map(|info| info.branch.clone());
                let dirty_count = git_info.as_ref().map(|info| info.dirty_count);
                let next_group = git_info.as_ref().map(|info| info.branch.clone());
                let mut changed = false;
                if let Some(header) = this.terminal_headers.get_mut(&session_id) {
                    if header.git_branch != branch || header.git_dirty_count != dirty_count {
                        header.git_branch = branch.clone();
                        header.git_dirty_count = dirty_count;
                        changed = true;
                    }
                }
                if let Some(session) = this.workspace.session_mut(session_id) {
                    if session.git_info != git_info || session.group != next_group {
                        session.git_info = git_info.clone();
                        session.group = next_group;
                        changed = true;
                    }
                }
                if changed {
                    this.mark_ui_sync_dirty();
                    cx.notify();
                }
            });
        })
        .detach();
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

    /// Check if the cached JSONL status hasn't changed for longer than `threshold`.
    fn is_cli_status_stale(&self, session_id: SessionId, threshold: Duration) -> bool {
        self.cli_readers
            .lock()
            .ok()
            .and_then(|r| r.cached_status.get(&session_id).map(|c| c.status_since))
            .is_some_and(|since| since.elapsed() > threshold)
    }

    fn get_recent_cached_cli_status(
        &mut self,
        session_id: SessionId,
    ) -> Option<(SessionStatus, Option<String>)> {
        let mut readers = self.cli_readers.lock().ok()?;
        let cached_status = readers.cached_status.get(&session_id)?;

        // Use the per-entry TTL: hook-based entries (Claude Code) stay valid for
        // HOOK_SIGNAL_CACHE_TTL (600s); JSONL-based entries (Codex/Gemini) expire
        // after GENERIC_SHELL_JSONL_CACHE_TTL (120s).
        if cached_status.seen_at.elapsed() > cached_status.ttl {
            readers.cached_status.remove(&session_id);
            return None;
        }

        Some((cached_status.status, cached_status.tool_name.clone()))
    }

    /// Read hook signal files on a background thread and apply them on the UI thread.
    fn spawn_background_hook_signal_check(&mut self, cx: &mut Context<Self>) {
        if self.polling.last_hook_signal_check.elapsed() < Duration::from_secs(1)
            || self.polling.hook_signal_check_in_flight
        {
            return;
        }

        self.polling.last_hook_signal_check = Instant::now();
        self.polling.hook_signal_check_in_flight = true;

        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let updates = cx
                .background_executor()
                .spawn(async move { read_recent_hook_signal_updates() })
                .await;

            let _ = this.update(cx, |this, cx| {
                this.polling.hook_signal_check_in_flight = false;
                for update in updates {
                    this.apply_hook_signal_update(update, cx);
                }
            });
        })
        .detach();
    }

    fn apply_hook_signal_update(&mut self, update: HookSignalUpdate, cx: &mut Context<Self>) {
        let HookSignalUpdate {
            session_id,
            cli_session_id,
            status,
            cli_type,
        } = update;

        let mut id_changed = false;
        let cli_type_name = cli_type.as_deref().unwrap_or(CLI_TYPE_CLAUDE);
        match cli_type_name {
            CLI_TYPE_CLAUDE => {
                id_changed = self
                    .session_manager
                    .lock()
                    .ok()
                    .and_then(|mgr| {
                        mgr.with_session_state_mut(session_id, |state| {
                            let changed =
                                state.session.claude_session_id.as_deref() != Some(&cli_session_id);
                            state.session.claude_session_id = Some(cli_session_id.clone());
                            changed
                        })
                    })
                    .unwrap_or(false);
            }
            CLI_TYPE_GEMINI => {
                id_changed = self
                    .session_manager
                    .lock()
                    .ok()
                    .and_then(|mgr| {
                        mgr.with_session_state_mut(session_id, |state| {
                            let changed =
                                state.session.gemini_session_id.as_deref() != Some(&cli_session_id);
                            state.session.gemini_session_id = Some(cli_session_id.clone());
                            changed
                        })
                    })
                    .unwrap_or(false);
            }
            CLI_TYPE_CODEX => {
                id_changed = self
                    .session_manager
                    .lock()
                    .ok()
                    .and_then(|mgr| {
                        mgr.with_session_state_mut(session_id, |state| {
                            let changed =
                                state.session.codex_session_id.as_deref() != Some(&cli_session_id);
                            state.session.codex_session_id = Some(cli_session_id.clone());
                            changed
                        })
                    })
                    .unwrap_or(false);
            }
            _ => {}
        }

        if id_changed {
            self.save_state_to_disk(cx);
        }

        let focused_id = self.workspace.focused_session_id();
        let new_status = match status.as_str() {
            "working" => SessionStatus::Working,
            "needs_attention" => SessionStatus::NeedsAttention,
            "response_ready" => {
                if Some(session_id) == focused_id {
                    SessionStatus::Idle
                } else {
                    SessionStatus::ResponseReady
                }
            }
            _ => SessionStatus::Idle,
        };

        if let Ok(mut readers) = self.cli_readers.lock() {
            let status_since = readers
                .cached_status
                .get(&session_id)
                .filter(|c| c.status == new_status)
                .map(|c| c.status_since)
                .unwrap_or_else(Instant::now);
            readers.cached_status.insert(
                session_id,
                CachedCliStatus {
                    status: new_status,
                    tool_name: cli_type.clone(),
                    seen_at: Instant::now(),
                    status_since,
                    ttl: Self::HOOK_SIGNAL_CACHE_TTL,
                },
            );
        }

        let prev_status = self
            .workspace
            .session(session_id)
            .map(|s| s.status)
            .unwrap_or(SessionStatus::Idle);

        if new_status == SessionStatus::NeedsAttention
            && prev_status != SessionStatus::NeedsAttention
        {
            self.event_bus.publish(CodirigentEvent::AttentionRequired {
                session_id,
                detail: None,
            });
            let name = self
                .workspace
                .session(session_id)
                .map(|s| s.name.clone())
                .unwrap_or_else(|| format!("Session {}", session_id.0));
            self.notification_manager.notify(
                NotificationType::InputRequired,
                session_id,
                &name,
                None,
            );
        }

        if new_status == SessionStatus::ResponseReady && prev_status == SessionStatus::Working {
            let name = self
                .workspace
                .session(session_id)
                .map(|s| s.name.clone())
                .unwrap_or_else(|| format!("Session {}", session_id.0));
            self.notification_manager.notify(
                NotificationType::ResponseReady,
                session_id,
                &name,
                None,
            );
        }
    }

    /// Check Claude Code hook signal files and update session status.
    ///
    /// Signal files are written by `codirigent-hook` for Claude and Codex sessions.
    /// Each file is tiny
    /// (<100 bytes) so this runs synchronously on the UI thread without a
    /// background task.
    ///
    /// Matching is exact: each signal file contains `codirigent_session_id`
    /// which is injected via the `CODIRIGENT_SESSION_ID` environment variable
    /// when Codirigent spawns the Claude Code process. Signal files without
    /// this field are ignored (Claude Code started outside Codirigent).
    #[allow(dead_code)]
    fn check_hook_signals(&mut self) {
        let signals_dir = match hook_signals_dir() {
            Some(d) => d,
            None => return,
        };

        let entries = match std::fs::read_dir(&signals_dir) {
            Ok(e) => e,
            Err(_) => return,
        };

        let now_ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            // The filename stem is the Claude Code session_id.
            let claude_session_id = match path.file_stem().and_then(|s| s.to_str()) {
                Some(s) => s.to_owned(),
                None => continue,
            };
            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let signal: HookSignal = match serde_json::from_str(&content) {
                Ok(s) => s,
                Err(_) => continue,
            };
            // Ignore stale signals (older than 10 minutes).
            if now_ts.saturating_sub(signal.ts) > 600 {
                continue;
            }
            // Ignore signals from Claude Code sessions not spawned by Codirigent.
            let codirigent_id_str = match &signal.codirigent_session_id {
                Some(id) => id,
                None => continue,
            };
            let session_id = match codirigent_id_str.parse::<u64>() {
                Ok(n) => SessionId(n),
                Err(_) => continue,
            };

            // Store the Claude/Gemini/Codex session_id on the Session for resume on next startup.
            // Persist to disk whenever it changes (first assignment or new session
            // started in the same terminal) so a clean app quit never loses the ID.
            let mut id_changed = false;

            let cli_type = signal.cli_type.as_deref().unwrap_or(CLI_TYPE_CLAUDE);
            match cli_type {
                CLI_TYPE_CLAUDE => {
                    id_changed = self
                        .session_manager
                        .lock()
                        .ok()
                        .and_then(|mgr| {
                            mgr.with_session_state_mut(session_id, |state| {
                                let changed = state.session.claude_session_id.as_deref()
                                    != Some(&claude_session_id);
                                state.session.claude_session_id = Some(claude_session_id.clone());
                                changed
                            })
                        })
                        .unwrap_or(false);
                }
                CLI_TYPE_GEMINI => {
                    id_changed = self
                        .session_manager
                        .lock()
                        .ok()
                        .and_then(|mgr| {
                            mgr.with_session_state_mut(session_id, |state| {
                                let changed = state.session.gemini_session_id.as_deref()
                                    != Some(&claude_session_id);
                                state.session.gemini_session_id = Some(claude_session_id.clone());
                                changed
                            })
                        })
                        .unwrap_or(false);
                }
                CLI_TYPE_CODEX => {
                    id_changed = self
                        .session_manager
                        .lock()
                        .ok()
                        .and_then(|mgr| {
                            mgr.with_session_state_mut(session_id, |state| {
                                let changed = state.session.codex_session_id.as_deref()
                                    != Some(&claude_session_id);
                                state.session.codex_session_id = Some(claude_session_id.clone());
                                changed
                            })
                        })
                        .unwrap_or(false);
                }
                _ => {}
            }

            let _ = id_changed;

            let focused_id = self.workspace.focused_session_id();
            let new_status = match signal.status.as_str() {
                "working" => SessionStatus::Working,
                "needs_attention" => SessionStatus::NeedsAttention,
                // response_ready: use ResponseReady only for non-focused sessions.
                // If the session is already in view, the user is seeing the response,
                // so treat it as plain Idle to avoid a spurious badge.
                "response_ready" => {
                    if Some(session_id) == focused_id {
                        SessionStatus::Idle
                    } else {
                        SessionStatus::ResponseReady
                    }
                }
                _ => SessionStatus::Idle,
            };

            if let Ok(mut readers) = self.cli_readers.lock() {
                let status_since = readers
                    .cached_status
                    .get(&session_id)
                    .filter(|c| c.status == new_status)
                    .map(|c| c.status_since)
                    .unwrap_or_else(Instant::now);
                readers.cached_status.insert(
                    session_id,
                    CachedCliStatus {
                        status: new_status,
                        tool_name: signal.cli_type.clone(),
                        seen_at: Instant::now(),
                        status_since,
                        ttl: Self::HOOK_SIGNAL_CACHE_TTL,
                    },
                );
            }

            let prev_status = self
                .workspace
                .session(session_id)
                .map(|s| s.status)
                .unwrap_or(SessionStatus::Idle);

            // Fire AttentionRequired on transition to NeedsAttention.
            if new_status == SessionStatus::NeedsAttention
                && prev_status != SessionStatus::NeedsAttention
            {
                self.event_bus.publish(CodirigentEvent::AttentionRequired {
                    session_id,
                    detail: None,
                });
                let name = self
                    .workspace
                    .session(session_id)
                    .map(|s| s.name.clone())
                    .unwrap_or_else(|| format!("Session {}", session_id.0));
                // Hook signals carry no tool detail — treat as InputRequired.
                // If the hook protocol is later extended with a tool name, this
                // can be dispatched as PermissionPrompt with detail.
                self.notification_manager.notify(
                    NotificationType::InputRequired,
                    session_id,
                    &name,
                    None,
                );
            }

            // Fire ResponseReady notification on transition from Working → ResponseReady.
            if new_status == SessionStatus::ResponseReady && prev_status == SessionStatus::Working {
                let name = self
                    .workspace
                    .session(session_id)
                    .map(|s| s.name.clone())
                    .unwrap_or_else(|| format!("Session {}", session_id.0));
                self.notification_manager.notify(
                    NotificationType::ResponseReady,
                    session_id,
                    &name,
                    None,
                );
            }
        }
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
mod tests {
    use super::*;
    use codirigent_core::{ImageData, ImageFormat};

    fn sig(status: &str, codirigent_session_id: Option<&str>, ts: u64) -> HookSignal {
        HookSignal {
            status: status.to_owned(),
            cli_type: None,
            codirigent_session_id: codirigent_session_id.map(str::to_owned),
            ts,
        }
    }

    #[test]
    fn hook_signal_without_codirigent_id_is_ignored() {
        // Signals without codirigent_session_id come from Claude Code started
        // outside Codirigent and should be silently discarded.
        let signal = sig("working", None, 100);
        assert!(signal.codirigent_session_id.is_none());
    }

    #[test]
    fn hook_signal_with_codirigent_id_is_valid() {
        let signal = sig("working", Some("42"), 100);
        assert_eq!(signal.codirigent_session_id.as_deref(), Some("42"));
        assert_eq!(signal.status, "working");
    }

    #[test]
    fn hook_signal_codirigent_id_parses_to_session_id() {
        let signal = sig("needs_attention", Some("7"), 100);
        let id: u64 = signal
            .codirigent_session_id
            .unwrap()
            .parse()
            .expect("should parse");
        assert_eq!(id, 7);
    }

    #[test]
    fn hook_signal_invalid_codirigent_id_not_parseable() {
        // Non-numeric IDs are rejected at parse time in check_hook_signals.
        let bad_id = "not-a-number".to_owned();
        assert!(bad_id.parse::<u64>().is_err());
    }

    #[test]
    fn hook_signal_deserializes_from_json() {
        let json = r#"{"status":"working","codirigent_session_id":"3","ts":1234567890}"#;
        let signal: HookSignal = serde_json::from_str(json).unwrap();
        assert_eq!(signal.status, "working");
        assert_eq!(signal.codirigent_session_id.as_deref(), Some("3"));
        assert_eq!(signal.ts, 1234567890);
    }

    #[test]
    fn hook_signal_deserializes_without_codirigent_id() {
        // Backwards-compatible: old signal files without the field deserialize fine.
        let json = r#"{"status":"idle","ts":100}"#;
        let signal: HookSignal = serde_json::from_str(json).unwrap();
        assert!(signal.codirigent_session_id.is_none());
    }

    #[test]
    fn tiny_dib_preview_is_suppressed() {
        let image = ImageData {
            bytes: vec![0; 8 * 1024],
            width: 32,
            height: 32,
            format: ImageFormat::Dib,
        };

        assert!(!should_show_clipboard_preview(&image));
    }

    #[test]
    fn larger_dib_preview_is_allowed() {
        let image = ImageData {
            bytes: vec![0; 40 * 1024],
            width: 320,
            height: 240,
            format: ImageFormat::Dib,
        };

        assert!(should_show_clipboard_preview(&image));
    }
}
