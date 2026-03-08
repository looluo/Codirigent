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
use super::cli_helpers::is_safe_cli_session_id;
use super::gpui::WorkspaceView;
use super::types::{CachedCliStatus, CliStatusSource, ProcessedHookSignal};
use codirigent_core::{
    hook_signals_dir, AssignmentAction, CliType, CodexExecutionMode, CodirigentEvent, EventBus,
    ProcessMonitor, Session, SessionId, SessionManager, SessionStatus, SessionUpdate, TaskStatus,
};
use codirigent_detector::NotificationType;
use codirigent_session::cli_detector::CliDetector;
use codirigent_session::clipboard_service::{ClipboardService, DefaultClipboardService};
use codirigent_session::detect_cli_from_output;
use codirigent_session::CliSessionStatus;
use gpui::Context;
use serde::Deserialize;
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tracing::{info, trace, warn};

const CLI_TYPE_CLAUDE: &str = "claude";
const CLI_TYPE_GEMINI: &str = "gemini";
const CLI_TYPE_CODEX: &str = "codex";

/// Unix timestamp (seconds) recorded the first time it is read, acting as a
/// per-process "run epoch".  Hook signals written before this moment belong to
/// a previous Codirigent run and must be ignored, regardless of the 600-second
/// recency window, to prevent stale signals from routing to re-used session IDs.
static APP_START_TS: std::sync::OnceLock<u64> = std::sync::OnceLock::new();

/// When `CODIRIGENT_LEGACY_PIPELINE=1` is set, the event-driven output
/// dispatcher and status reconciler are disabled and the legacy broad-scan
/// polling path runs exclusively. This is a temporary kill switch for the
/// pipeline transition — it will be removed once shadow-mode validation
/// confirms zero diffs.
static LEGACY_PIPELINE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

fn is_legacy_pipeline() -> bool {
    *LEGACY_PIPELINE.get_or_init(|| {
        std::env::var("CODIRIGENT_LEGACY_PIPELINE")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
    })
}

fn app_start_ts() -> u64 {
    *APP_START_TS.get_or_init(|| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    })
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

/// Session status result from a background JSONL read: (status, optional detail string).
type JsonlStatusResult = Option<(SessionStatus, Option<String>)>;

#[derive(Debug, Clone)]
struct JsonlCheckInput {
    session_id: SessionId,
    working_dir: std::path::PathBuf,
    child_pid: Option<u32>,
    cli_type: CliType,
    codex_session_id: Option<String>,
    codex_execution_mode: Option<CodexExecutionMode>,
    has_explicit_codex_started_at: bool,
    current_status: SessionStatus,
    created_at_millis: i64,
}

#[derive(Debug)]
struct JsonlCheckOutput {
    session_id: SessionId,
    status: JsonlStatusResult,
    codex_session_id: Option<String>,
    codex_execution_mode: Option<CodexExecutionMode>,
}

fn codex_execution_mode_from_rollout_mode(mode: &str) -> Option<CodexExecutionMode> {
    if mode.eq_ignore_ascii_case("yolo")
        || mode.eq_ignore_ascii_case("bypass")
        || mode.eq_ignore_ascii_case("dangerously-bypass-approvals-and-sandbox")
        || mode.eq_ignore_ascii_case("dangerously_bypass_approvals_and_sandbox")
    {
        Some(CodexExecutionMode::Bypass)
    } else if mode.eq_ignore_ascii_case("full-auto")
        || mode.eq_ignore_ascii_case("full_auto")
        || mode.eq_ignore_ascii_case("fullauto")
    {
        Some(CodexExecutionMode::FullAuto)
    } else {
        None
    }
}

fn count_codex_sessions_without_session_id_per_working_dir(
    inputs: &[JsonlCheckInput],
) -> HashMap<std::path::PathBuf, usize> {
    inputs
        .iter()
        .filter(|input| input.cli_type == CliType::CodexCli && input.codex_session_id.is_none())
        .fold(HashMap::new(), |mut counts, input| {
            *counts.entry(input.working_dir.clone()).or_default() += 1;
            counts
        })
}

fn should_defer_ambiguous_codex_probe(
    input: &JsonlCheckInput,
    no_id_codex_counts: &HashMap<std::path::PathBuf, usize>,
) -> bool {
    input.cli_type == CliType::CodexCli
        && input.codex_session_id.is_none()
        && !input.has_explicit_codex_started_at
        && no_id_codex_counts
            .get(&input.working_dir)
            .copied()
            .unwrap_or_default()
            > 1
}

fn cli_type_from_hook_signal_name(cli_type_name: &str) -> Option<CliType> {
    match cli_type_name {
        CLI_TYPE_CLAUDE => Some(CliType::ClaudeCode),
        CLI_TYPE_GEMINI => Some(CliType::GeminiCli),
        CLI_TYPE_CODEX => Some(CliType::CodexCli),
        _ => None,
    }
}

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
    #[serde(default)]
    cli_session_id: Option<String>,
    #[serde(default)]
    approval_policy: Option<String>,
    #[serde(default)]
    sandbox_policy_type: Option<String>,
    /// Codirigent session ID, present only when Claude Code was spawned by Codirigent
    /// (via the `CODIRIGENT_SESSION_ID` environment variable).
    codirigent_session_id: Option<String>,
    ts: u64,
}

#[derive(Debug)]
struct HookSignalUpdate {
    session_id: SessionId,
    signal_file_id: String,
    cli_session_id: Option<String>,
    codex_execution_mode: Option<CodexExecutionMode>,
    status: String,
    cli_type: Option<String>,
    ts: u64,
}

fn codex_execution_mode_fingerprint(mode: Option<CodexExecutionMode>) -> Option<&'static str> {
    match mode {
        Some(CodexExecutionMode::FullAuto) => Some("full-auto"),
        Some(CodexExecutionMode::Bypass) => Some("bypass"),
        None => None,
    }
}

fn hook_signal_fingerprint(
    status: &str,
    cli_type: Option<&str>,
    cli_session_id: Option<&str>,
    codex_execution_mode: Option<CodexExecutionMode>,
) -> u64 {
    let mut hasher = DefaultHasher::new();
    status.hash(&mut hasher);
    cli_type.hash(&mut hasher);
    cli_session_id.hash(&mut hasher);
    codex_execution_mode_fingerprint(codex_execution_mode).hash(&mut hasher);
    hasher.finish()
}

fn should_apply_hook_signal(
    last_seen: Option<ProcessedHookSignal>,
    signal_ts: u64,
    signal_fingerprint: u64,
) -> bool {
    match last_seen {
        Some(last_seen) if signal_ts < last_seen.ts => false,
        Some(last_seen)
            if signal_ts == last_seen.ts && signal_fingerprint == last_seen.fingerprint =>
        {
            false
        }
        _ => true,
    }
}

fn resolve_hook_cli_session_id(
    signal_file_id: &str,
    explicit_cli_session_id: Option<&str>,
    session_id: SessionId,
) -> Option<String> {
    if let Some(explicit_id) = explicit_cli_session_id
        .map(str::trim)
        .filter(|id| !id.is_empty())
    {
        if is_safe_cli_session_id(explicit_id) {
            return Some(explicit_id.to_owned());
        }
        warn!(
            session_id = session_id.0,
            signal_file_id,
            cli_session_id = %explicit_id,
            "Ignoring unsafe CLI session ID from hook signal"
        );
        return None;
    }

    let fallback = signal_file_id.trim();
    if fallback.is_empty() || fallback == session_id.0.to_string() {
        return None;
    }
    if !is_safe_cli_session_id(fallback) {
        warn!(
            session_id = session_id.0,
            signal_file_id = fallback,
            "Ignoring unsafe fallback CLI session ID from hook signal filename"
        );
        return None;
    }

    Some(fallback.to_owned())
}

fn codex_execution_mode_from_approval_and_sandbox(
    approval_policy: Option<&str>,
    sandbox_policy_type: Option<&str>,
) -> Option<CodexExecutionMode> {
    if !approval_policy.is_some_and(|value| value.eq_ignore_ascii_case("never")) {
        return None;
    }

    match sandbox_policy_type {
        Some(value) if value.eq_ignore_ascii_case("danger-full-access") => {
            Some(CodexExecutionMode::Bypass)
        }
        Some(value)
            if value.eq_ignore_ascii_case("workspace-write")
                || value.eq_ignore_ascii_case("workspace_write") =>
        {
            Some(CodexExecutionMode::FullAuto)
        }
        _ => None,
    }
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

        let signal_file_id = match path.file_stem().and_then(|s| s.to_str()) {
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

        // Reject signals written before this process started.  Session IDs
        // (1, 2, 3 …) reset on every restart, so a signal from a previous run
        // that shares an ID with a newly-created session would route to the
        // wrong session and corrupt its claude_session_id.
        if signal.ts < app_start_ts() {
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
            signal_file_id,
            cli_session_id: signal.cli_session_id,
            codex_execution_mode: codex_execution_mode_from_approval_and_sandbox(
                signal.approval_policy.as_deref(),
                signal.sandbox_policy_type.as_deref(),
            ),
            status: signal.status,
            cli_type: signal.cli_type,
            ts: signal.ts,
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
        let any_status_dirty = self.tick_detector_statuses();
        self.spawn_background_hook_signal_check(cx);
        self.spawn_background_jsonl_check(cx);
        self.cleanup_compaction_timeouts();
        self.cleanup_stale_proposals();
        self.schedule_background_git_refresh(cx);

        if any_status_dirty || self.update_clipboard_preview(cx) {
            cx.notify();
        }
    }

    /// Advance process-state detection on a maintenance cadence, then sync any
    /// resulting status changes into the workspace cache.
    ///
    /// This is required on shells without OSC 133 integration (notably the
    /// Windows PTY path), where `process_output()` can move a session into
    /// `Working` but only `tick()` can later decay it back to `Idle`.
    fn tick_detector_statuses(&mut self) -> bool {
        let session_ids: Vec<SessionId> = self.terminals.keys().copied().collect();
        if session_ids.is_empty() {
            return false;
        }

        let session_count = session_ids.len();
        trace!(session_count, "tick_detector_statuses");
        self.with_detector(|detector| detector.tick());

        session_ids.into_iter().fold(false, |dirty, session_id| {
            dirty | self.sync_session_status(session_id)
        })
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
                    _ => {
                        // Phase-2: ShellStateChanged, WorkingDirectoryChanged,
                        // etc. are handled inline during output preparation
                        // (dual-path). Channel copies are informational only
                        // until phase-2 routing replaces the inline path.
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
                    self.output_dispatcher.mark_ready(*id);
                }
            }
        }

        // Phase 3: Take ready sessions from the dispatcher (focused first).
        let session_ids = self
            .output_dispatcher
            .take_ready_sessions(self.workspace.focused_session_id());

        // Filter: only schedule sessions that have a terminal view.
        // Sessions without a terminal are NOT re-queued to avoid hot-loop
        // spinning; the 1s legacy fallback will re-discover them if needed.
        let schedulable: Vec<_> = session_ids
            .into_iter()
            .filter(|id| self.terminals.contains_key(id))
            .collect();

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
        // Guard: prevent double-dispatch via the dispatcher's in-flight set.
        if !self.output_dispatcher.mark_in_flight(session_id) {
            return;
        }
        // TRANSITION: Legacy in-flight set kept in sync until
        // CODIRIGENT_LEGACY_PIPELINE and schedule_output_preparation_legacy
        // are removed. Both sets are always updated together.
        self.polling.output_prepare_in_flight.insert(session_id);

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
                        data,
                        has_more: drained.has_more,
                        detected_cli_type,
                        cwd_session,
                    })
                })
                .await;

            let _ = this.update(cx, |this, cx| {
                this.polling.output_prepare_in_flight.remove(&session_id);
                this.output_dispatcher.complete_in_flight(session_id);
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
        let bytes_drained = data.len();
        trace!(
            ?session_id,
            bytes_drained,
            has_more,
            "apply_prepared_session_output"
        );
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

        // Targeted delta: sync only this session's header instead of
        // dirtying the full UI sync path for every output poll.
        if any_dirty {
            self.sync_session_header(session_id);
            cx.notify();
        }
        if has_more {
            self.schedule_session_output_preparation(session_id, cx);
        }
    }

    /// Update session status from detector/cache state.
    ///
    /// Uses the status reconciler ([`super::status_engine::reconcile`]) to
    /// combine detector hints with cached CLI hints, then applies side effects
    /// (task transitions, compaction, auto-assign, notifications).
    ///
    /// Returns `true` if any UI-visible change was made that requires a repaint.
    fn sync_session_status(&mut self, session_id: codirigent_core::SessionId) -> bool {
        use super::status_engine::reconcile;
        use super::status_providers::{HintSource, StaleAction};

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
        let (cached_status, cached_tool_name, cached_source, cache_age) = self
            .cli_readers
            .lock()
            .ok()
            .and_then(|mut readers| {
                let cached = readers.cached_status.get(&session_id)?;
                if cached.seen_at.elapsed() > cached.ttl {
                    readers.cached_status.remove(&session_id);
                    return None;
                }
                let source = match cached.source {
                    CliStatusSource::Hook => HintSource::HookSignal,
                    CliStatusSource::Jsonl => HintSource::Jsonl,
                };
                let age = Some(cached.status_since.elapsed());
                // tool_name not yet consumed by reconciler — skip clone
                Some((Some(cached.status), None, source, age))
            })
            .unwrap_or((None, None, HintSource::Detector, None));

        let previous_status = self.workspace.session(session_id).map(|s| s.status);

        // Run the reconciler
        let (reconciled, stale_action) = reconcile(
            session_id,
            detector_status,
            cached_status,
            cached_tool_name,
            cached_source,
            cache_age,
            previous_status,
        );

        // Handle stale cache action
        match stale_action {
            StaleAction::ClearAndRevert {
                session_id: stale_id,
            } => {
                if let Ok(mut readers) = self.cli_readers.lock() {
                    readers.cached_status.remove(&stale_id);
                }
                self.clipboard
                    .clipboard_service
                    .set_session_cli_type(stale_id, codirigent_core::CliType::GenericShell);
                info!(
                    ?stale_id,
                    "Cleared stale NeedsAttention, reverted to GenericShell"
                );
            }
            StaleAction::None => {}
        }

        // Apply reconciled status and side effects
        if let Some(reconciled) = reconciled {
            let status = reconciled.status;
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
        trace!("spawn_background_jsonl_check");

        // Collect inputs for background JSONL check from the authoritative
        // SessionManager snapshot so hook-updated Codex ids/modes are visible
        // immediately to the JSONL matcher.
        let manager_sessions = self.with_session_manager(|manager| manager.list_sessions());
        let jsonl_inputs: Vec<JsonlCheckInput> = manager_sessions
            .into_iter()
            .map(|session| {
                let cli_type = self
                    .clipboard
                    .clipboard_service
                    .get_session_cli_type(session.id);
                let child_pid =
                    self.with_session_manager(|manager| manager.get_child_pid(session.id));
                let known_codex_session_id = session
                    .codex_session_id
                    .as_ref()
                    .filter(|id| *id != &session.id.0.to_string())
                    .cloned();
                JsonlCheckInput {
                    session_id: session.id,
                    working_dir: session.working_directory,
                    child_pid,
                    cli_type,
                    codex_session_id: known_codex_session_id,
                    codex_execution_mode: session.codex_execution_mode,
                    has_explicit_codex_started_at: session.codex_started_at.is_some(),
                    current_status: self
                        .workspace
                        .session(session.id)
                        .map(|s| s.status)
                        .unwrap_or(session.status),
                    created_at_millis: session
                        .codex_started_at
                        .unwrap_or(session.created_at)
                        .timestamp_millis(),
                }
            })
            .collect();

        let no_id_codex_counts =
            count_codex_sessions_without_session_id_per_working_dir(&jsonl_inputs);

        let cli_readers = self.cli_readers.clone();
        let event_bus = self.event_bus.clone();
        let max_age = Self::GENERIC_SHELL_JSONL_MAX_AGE;

        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            // Background: perform JSONL reads (the expensive I/O)
            let results = cx
                .background_executor()
                .spawn(async move {
                    let mut out: Vec<JsonlCheckOutput> = Vec::new();
                    let mut detected_types: Vec<(SessionId, codirigent_core::CliType)> = Vec::new();
                    if let Ok(mut readers) = cli_readers.lock() {
                        for input in &jsonl_inputs {
                            // For GenericShell sessions, try process-tree detection.
                            // The detector walks the PTY's child processes looking
                            // for known CLI binaries (claude, gemini, codex).
                            // Note: don't use process-tree to REVERT ClaudeCode → GenericShell
                            // because detection is unreliable (returns GenericShell even when
                            // Claude is running). Banner detection handles initial detection.
                            let effective_type =
                                if input.cli_type == codirigent_core::CliType::GenericShell {
                                    if let Some(pid) = input.child_pid {
                                        let detected = readers.detector.detect_cli_type(pid);
                                        if detected != codirigent_core::CliType::GenericShell {
                                            info!(
                                                session_id = ?input.session_id,
                                                ?detected,
                                                "Process-tree detected CLI type"
                                            );
                                            detected_types.push((input.session_id, detected));
                                            detected
                                        } else {
                                            input.cli_type
                                        }
                                    } else {
                                        input.cli_type
                                    }
                                } else {
                                    input.cli_type
                                };

                            let ambiguous_codex_probe = effective_type == CliType::CodexCli
                                && should_defer_ambiguous_codex_probe(input, &no_id_codex_counts);

                            let (
                                cli_status,
                                detected_codex_session_id,
                                detected_codex_execution_mode,
                            ): (
                                Option<CliSessionStatus>,
                                Option<String>,
                                Option<CodexExecutionMode>,
                            ) = match effective_type {
                                codirigent_core::CliType::ClaudeCode => {
                                    // Claude Code status is handled by hook signal files
                                    // (spawn_background_hook_signal_check) — no JSONL reader needed here.
                                    (None, None, None)
                                }
                                codirigent_core::CliType::CodexCli => {
                                    if ambiguous_codex_probe {
                                        (None, None, None)
                                    } else {
                                        readers
                                            .codex
                                            .as_mut()
                                            .and_then(|r| {
                                                let created_after = (input.created_at_millis >= 0)
                                                    .then_some(
                                                        UNIX_EPOCH
                                                            + Duration::from_millis(
                                                                input.created_at_millis as u64,
                                                            ),
                                                    );
                                                r.get_status_snapshot_if_recent(
                                                    &input.working_dir,
                                                    input.codex_session_id.as_deref(),
                                                    input.child_pid,
                                                    max_age,
                                                    created_after,
                                                    input.codex_execution_mode,
                                                )
                                            })
                                            .map(|snapshot| {
                                                (
                                                    Some(snapshot.status),
                                                    snapshot.session_id,
                                                    snapshot.execution_mode.or_else(|| {
                                                        snapshot.approval_mode.as_deref().and_then(
                                                            codex_execution_mode_from_rollout_mode,
                                                        )
                                                    }),
                                                )
                                            })
                                            .unwrap_or((None, None, None))
                                    }
                                }
                                codirigent_core::CliType::GeminiCli => (
                                    readers.gemini.as_mut().and_then(|r| {
                                        r.get_status_if_recent(
                                            &input.working_dir,
                                            input.child_pid,
                                            max_age,
                                        )
                                    }),
                                    None,
                                    None,
                                ),
                                codirigent_core::CliType::GenericShell => (None, None, None),
                            };
                            let resolved = cli_status.and_then(|s| s.to_session_status());
                            out.push(JsonlCheckOutput {
                                session_id: input.session_id,
                                status: resolved,
                                codex_session_id: detected_codex_session_id,
                                codex_execution_mode: input
                                    .codex_execution_mode
                                    .or(detected_codex_execution_mode),
                            });
                        }
                    }
                    (out, jsonl_inputs, detected_types)
                })
                .await;

            // Marshal results back to UI thread
            let _ = this.update(cx, |this, cx| {
                this.polling.jsonl_check_in_flight = false;
                let mut any_dirty = false;
                let (results, inputs, detected_types) = results;
                let input_statuses: HashMap<SessionId, SessionStatus> = inputs
                    .iter()
                    .map(|input| (input.session_id, input.current_status))
                    .collect();
                let input_modes: HashMap<SessionId, Option<CodexExecutionMode>> = inputs
                    .iter()
                    .map(|input| (input.session_id, input.codex_execution_mode))
                    .collect();
                let cache_update_time = Instant::now();
                let mut status_sync_ids = HashSet::new();
                let mut cached_status = this.cli_readers.lock().ok();
                let mut should_save_state = false;
                let mut pending_mode_updates = Vec::new();

                // Apply process-tree CLI type detections on UI thread
                for (session_id, detected_type) in &detected_types {
                    this.clipboard
                        .clipboard_service
                        .set_session_cli_type(*session_id, *detected_type);
                    if *detected_type == CliType::CodexCli {
                        let started_at = chrono::Utc::now();
                        let manager_changed = this
                            .session_manager
                            .lock()
                            .ok()
                            .and_then(|mgr| {
                                mgr.with_session_state_mut(*session_id, |state| {
                                    if state.session.codex_started_at.is_none() {
                                        state.session.codex_started_at = Some(started_at);
                                        true
                                    } else {
                                        false
                                    }
                                })
                            })
                            .unwrap_or(false);
                        let workspace_changed = this
                            .workspace
                            .session_mut(*session_id)
                            .map(|session| {
                                if session.codex_started_at.is_none() {
                                    session.codex_started_at = Some(started_at);
                                    true
                                } else {
                                    false
                                }
                            })
                            .unwrap_or(false);
                        should_save_state |= manager_changed || workspace_changed;
                    }
                    info!(
                        ?session_id,
                        ?detected_type,
                        "Applied process-tree CLI type detection"
                    );
                }
                for result in &results {
                    let session_id = result.session_id;
                    if let Some(codex_session_id) = result.codex_session_id.as_deref() {
                        if !is_safe_cli_session_id(codex_session_id) {
                            warn!(
                                ?session_id,
                                codex_session_id,
                                "Ignoring unsafe Codex session ID from rollout polling"
                            );
                            continue;
                        }
                        let mut updated = false;
                        if let Ok(mgr) = this.session_manager.lock() {
                            updated |= mgr
                                .with_session_state_mut(session_id, |state| {
                                    if state.session.codex_session_id.as_deref()
                                        != Some(codex_session_id)
                                    {
                                        state.session.codex_session_id =
                                            Some(codex_session_id.to_owned());
                                        true
                                    } else {
                                        false
                                    }
                                })
                                .unwrap_or(false);
                        }
                        if let Some(session) = this.workspace.session_mut(session_id) {
                            if session.codex_session_id.as_deref() != Some(codex_session_id) {
                                session.codex_session_id = Some(codex_session_id.to_owned());
                                updated = true;
                            }
                        }
                        should_save_state |= updated;
                    }

                    if let Some(mode) = result.codex_execution_mode {
                        let current_mode = input_modes
                            .get(&result.session_id)
                            .copied()
                            .flatten()
                            .or_else(|| {
                                this.workspace
                                    .session(result.session_id)
                                    .and_then(|session| session.codex_execution_mode)
                            });
                        if current_mode != Some(mode) {
                            pending_mode_updates.push((result.session_id, mode));
                        }
                    }

                    if let Some((new_status, tool_name)) = &result.status {
                        // Cache the JSONL result
                        if let Some(readers) = cached_status.as_mut() {
                            // Preserve status_since if status hasn't changed
                            let status_since = readers
                                .cached_status
                                .get(&result.session_id)
                                .filter(|c| c.status == *new_status)
                                .map(|c| c.status_since)
                                .unwrap_or(cache_update_time);
                            readers.cached_status.insert(
                                result.session_id,
                                CachedCliStatus {
                                    status: *new_status,
                                    tool_name: tool_name.clone(),
                                    seen_at: cache_update_time,
                                    source: CliStatusSource::Jsonl,
                                    status_since,
                                    ttl: Self::GENERIC_SHELL_JSONL_CACHE_TTL,
                                },
                            );
                        }
                        // Fire AttentionRequired on transition
                        if *new_status == SessionStatus::NeedsAttention {
                            let current_status = input_statuses.get(&result.session_id).copied();
                            if current_status != Some(SessionStatus::NeedsAttention) {
                                event_bus.publish(CodirigentEvent::AttentionRequired {
                                    session_id: result.session_id,
                                    detail: tool_name.clone(),
                                });
                                let session_name = this
                                    .workspace
                                    .session(result.session_id)
                                    .map(|s| s.name.clone())
                                    .unwrap_or_else(|| format!("Session {}", result.session_id.0));
                                let (notif_type, detail) = match tool_name.as_deref() {
                                    Some("question") | None => {
                                        (NotificationType::InputRequired, None)
                                    }
                                    Some(tool) => (NotificationType::PermissionPrompt, Some(tool)),
                                };
                                this.notification_manager.notify(
                                    notif_type,
                                    result.session_id,
                                    &session_name,
                                    detail,
                                );
                            }
                        }
                        status_sync_ids.insert(result.session_id);
                    } else {
                        // No JSONL result — check if detector says idle and clear stale cache
                        let detector_idle = this.with_detector(|detector| {
                            matches!(
                                detector.get_status(result.session_id),
                                Some(SessionStatus::Idle) | None
                            )
                        });
                        if detector_idle {
                            if let Some(readers) = cached_status.as_mut() {
                                let is_stale = readers
                                    .cached_status
                                    .get(&result.session_id)
                                    .map(|c| {
                                        c.source == CliStatusSource::Jsonl
                                            && c.seen_at.elapsed() > c.ttl
                                    })
                                    .unwrap_or(false);
                                if is_stale {
                                    readers.cached_status.remove(&result.session_id);
                                    status_sync_ids.insert(result.session_id);
                                }
                            }
                        }
                    }
                }
                drop(cached_status);
                for (session_id, mode) in pending_mode_updates {
                    this.set_session_codex_execution_mode(session_id, Some(mode), cx);
                }
                if should_save_state {
                    this.save_state_to_disk(cx);
                }
                for session_id in status_sync_ids {
                    any_dirty |= this.sync_session_status(session_id);
                }
                if any_dirty {
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
                        mgr.refresh_git_status_fresh(session_id),
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

    /// Read hook signal files on a background thread and apply them on the UI thread.
    fn spawn_background_hook_signal_check(&mut self, cx: &mut Context<Self>) {
        if self.polling.last_hook_signal_check.elapsed() < Duration::from_secs(1)
            || self.polling.hook_signal_check_in_flight
        {
            return;
        }

        trace!("spawn_background_hook_signal_check");
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
            signal_file_id,
            cli_session_id,
            codex_execution_mode,
            status,
            cli_type,
            ts,
        } = update;

        let signal_fingerprint = hook_signal_fingerprint(
            &status,
            cli_type.as_deref(),
            cli_session_id.as_deref(),
            codex_execution_mode,
        );
        let last_seen = self
            .polling
            .last_processed_hook_signal_ts
            .get(&signal_file_id)
            .copied();
        if !should_apply_hook_signal(last_seen, ts, signal_fingerprint) {
            return;
        }
        self.polling.last_processed_hook_signal_ts.insert(
            signal_file_id.clone(),
            ProcessedHookSignal {
                ts,
                fingerprint: signal_fingerprint,
            },
        );

        let mut id_changed = false;
        let cli_type_name = cli_type.as_deref().unwrap_or(CLI_TYPE_CLAUDE);
        if let Some(cli_type) = cli_type_from_hook_signal_name(cli_type_name) {
            self.clipboard
                .clipboard_service
                .set_session_cli_type(session_id, cli_type);
        }
        let resolved_cli_session_id =
            resolve_hook_cli_session_id(&signal_file_id, cli_session_id.as_deref(), session_id);
        if let Some(cli_session_id) = resolved_cli_session_id.as_deref() {
            match cli_type_name {
                CLI_TYPE_CLAUDE => {
                    id_changed = self
                        .session_manager
                        .lock()
                        .ok()
                        .and_then(|mgr| {
                            mgr.with_session_state_mut(session_id, |state| {
                                let changed = state.session.claude_session_id.as_deref()
                                    != Some(cli_session_id);
                                state.session.claude_session_id = Some(cli_session_id.to_owned());
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
                                    != Some(cli_session_id);
                                state.session.gemini_session_id = Some(cli_session_id.to_owned());
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
                                    != Some(cli_session_id);
                                state.session.codex_session_id = Some(cli_session_id.to_owned());
                                changed
                            })
                        })
                        .unwrap_or(false);
                    if let Some(session) = self.workspace.session_mut(session_id) {
                        if session.codex_session_id.as_deref() != Some(cli_session_id) {
                            session.codex_session_id = Some(cli_session_id.to_owned());
                            id_changed = true;
                        }
                        if session.codex_started_at.is_none() {
                            session.codex_started_at = Some(chrono::Utc::now());
                            id_changed = true;
                        }
                    }
                }
                _ => {}
            }
        }

        if cli_type_name == CLI_TYPE_CODEX {
            if let Some(mode) = codex_execution_mode {
                self.set_session_codex_execution_mode(session_id, Some(mode), cx);
            }
            let started_at = chrono::Utc::now();
            let manager_changed = self
                .session_manager
                .lock()
                .ok()
                .and_then(|mgr| {
                    mgr.with_session_state_mut(session_id, |state| {
                        if state.session.codex_started_at.is_none() {
                            state.session.codex_started_at = Some(started_at);
                            true
                        } else {
                            false
                        }
                    })
                })
                .unwrap_or(false);
            let workspace_changed = self
                .workspace
                .session_mut(session_id)
                .map(|session| {
                    if session.codex_started_at.is_none() {
                        session.codex_started_at = Some(started_at);
                        true
                    } else {
                        false
                    }
                })
                .unwrap_or(false);
            id_changed |= manager_changed || workspace_changed;
        }

        if id_changed {
            self.save_state_to_disk(cx);
        }

        let focused_id = self.workspace.focused_session_id();
        let raw_status = match status.as_str() {
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
        let new_status = raw_status;

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
                    source: CliStatusSource::Hook,
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

        if self.sync_session_status(session_id) {
            cx.notify();
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

    fn codex_input(
        session_id: u64,
        working_dir: &str,
        has_explicit_codex_started_at: bool,
    ) -> JsonlCheckInput {
        JsonlCheckInput {
            session_id: SessionId(session_id),
            working_dir: std::path::PathBuf::from(working_dir),
            child_pid: None,
            cli_type: CliType::CodexCli,
            codex_session_id: None,
            codex_execution_mode: None,
            has_explicit_codex_started_at,
            current_status: SessionStatus::Idle,
            created_at_millis: 0,
        }
    }

    fn sig(status: &str, codirigent_session_id: Option<&str>, ts: u64) -> HookSignal {
        HookSignal {
            status: status.to_owned(),
            cli_type: None,
            cli_session_id: None,
            approval_policy: None,
            sandbox_policy_type: None,
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
        // Non-numeric IDs are rejected at parse time in hook signal processing.
        let bad_id = "not-a-number".to_owned();
        assert!(bad_id.parse::<u64>().is_err());
    }

    #[test]
    fn hook_signal_deserializes_from_json() {
        let json = r#"{"status":"working","cli_session_id":"codex-session","codirigent_session_id":"3","ts":1234567890}"#;
        let signal: HookSignal = serde_json::from_str(json).unwrap();
        assert_eq!(signal.status, "working");
        assert_eq!(signal.cli_session_id.as_deref(), Some("codex-session"));
        assert_eq!(signal.codirigent_session_id.as_deref(), Some("3"));
        assert_eq!(signal.ts, 1234567890);
    }

    #[test]
    fn hook_signal_deserializes_without_codirigent_id() {
        // Backwards-compatible: old signal files without the field deserialize fine.
        let json = r#"{"status":"idle","ts":100}"#;
        let signal: HookSignal = serde_json::from_str(json).unwrap();
        assert!(signal.cli_session_id.is_none());
        assert!(signal.codirigent_session_id.is_none());
    }

    #[test]
    fn hook_signal_context_infers_bypass_mode() {
        assert_eq!(
            codex_execution_mode_from_approval_and_sandbox(
                Some("never"),
                Some("danger-full-access"),
            ),
            Some(CodexExecutionMode::Bypass)
        );
    }

    #[test]
    fn hook_signal_context_infers_full_auto_mode() {
        assert_eq!(
            codex_execution_mode_from_approval_and_sandbox(Some("never"), Some("workspace-write"),),
            Some(CodexExecutionMode::FullAuto)
        );
    }

    #[test]
    fn hook_signal_is_applied_when_timestamp_advances() {
        let fp = hook_signal_fingerprint("working", Some(CLI_TYPE_CLAUDE), None, None);
        assert!(should_apply_hook_signal(None, 100, fp));
        assert!(should_apply_hook_signal(
            Some(ProcessedHookSignal {
                ts: 99,
                fingerprint: fp,
            }),
            100,
            fp,
        ));
    }

    #[test]
    fn identical_hook_signal_is_ignored_when_timestamp_does_not_advance() {
        let fp = hook_signal_fingerprint("working", Some(CLI_TYPE_CLAUDE), None, None);
        assert!(!should_apply_hook_signal(
            Some(ProcessedHookSignal {
                ts: 100,
                fingerprint: fp,
            }),
            100,
            fp,
        ));
        assert!(!should_apply_hook_signal(
            Some(ProcessedHookSignal {
                ts: 101,
                fingerprint: fp,
            }),
            100,
            fp,
        ));
    }

    #[test]
    fn changed_hook_signal_with_same_timestamp_is_still_applied() {
        let old_fp = hook_signal_fingerprint("working", Some(CLI_TYPE_CLAUDE), None, None);
        let new_fp = hook_signal_fingerprint("response_ready", Some(CLI_TYPE_CLAUDE), None, None);

        assert!(should_apply_hook_signal(
            Some(ProcessedHookSignal {
                ts: 100,
                fingerprint: old_fp,
            }),
            100,
            new_fp,
        ));
    }

    #[test]
    fn numeric_signal_file_id_is_not_treated_as_codex_session_id() {
        assert_eq!(resolve_hook_cli_session_id("3", None, SessionId(3)), None);
    }

    #[test]
    fn non_numeric_signal_file_id_can_backfill_cli_session_id() {
        assert_eq!(
            resolve_hook_cli_session_id("codex-uuid", None, SessionId(3)),
            Some("codex-uuid".to_string())
        );
    }

    #[test]
    fn explicit_cli_session_id_wins_over_signal_file_id() {
        assert_eq!(
            resolve_hook_cli_session_id("3", Some("real-codex-id"), SessionId(3)),
            Some("real-codex-id".to_string())
        );
    }

    #[test]
    fn unsafe_hook_cli_session_id_is_rejected() {
        assert_eq!(
            resolve_hook_cli_session_id("3", Some("bad;id"), SessionId(3)),
            None
        );
        assert_eq!(
            resolve_hook_cli_session_id("bad;id", None, SessionId(3)),
            None
        );
    }

    #[test]
    fn ambiguous_codex_probe_is_deferred_without_explicit_start_time() {
        let inputs = vec![
            codex_input(1, "C:/repo", false),
            codex_input(2, "C:/repo", false),
        ];
        let counts = count_codex_sessions_without_session_id_per_working_dir(&inputs);

        assert!(should_defer_ambiguous_codex_probe(&inputs[0], &counts));
        assert!(should_defer_ambiguous_codex_probe(&inputs[1], &counts));
    }

    #[test]
    fn ambiguous_codex_probe_uses_timestamp_when_start_time_is_known() {
        let inputs = vec![
            codex_input(1, "C:/repo", true),
            codex_input(2, "C:/repo", true),
        ];
        let counts = count_codex_sessions_without_session_id_per_working_dir(&inputs);

        assert!(!should_defer_ambiguous_codex_probe(&inputs[0], &counts));
        assert!(!should_defer_ambiguous_codex_probe(&inputs[1], &counts));
    }

    #[test]
    fn ambiguous_codex_probe_only_defers_session_missing_start_time() {
        let inputs = vec![
            codex_input(1, "C:/repo", true),
            codex_input(2, "C:/repo", false),
        ];
        let counts = count_codex_sessions_without_session_id_per_working_dir(&inputs);

        assert!(!should_defer_ambiguous_codex_probe(&inputs[0], &counts));
        assert!(should_defer_ambiguous_codex_probe(&inputs[1], &counts));
    }

    #[test]
    fn hook_signal_cli_type_maps_to_codex() {
        assert_eq!(
            cli_type_from_hook_signal_name(CLI_TYPE_CODEX),
            Some(CliType::CodexCli)
        );
    }

    #[test]
    fn hook_signal_cli_type_maps_to_claude_and_gemini() {
        assert_eq!(
            cli_type_from_hook_signal_name(CLI_TYPE_CLAUDE),
            Some(CliType::ClaudeCode)
        );
        assert_eq!(
            cli_type_from_hook_signal_name(CLI_TYPE_GEMINI),
            Some(CliType::GeminiCli)
        );
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

    #[test]
    fn focused_schedulable_output_is_prioritized() {
        let session_ids = vec![SessionId(1), SessionId(2), SessionId(3)];
        let schedulable = HashSet::from([SessionId(2), SessionId(3)]);

        let (ready, deferred) =
            prioritize_and_partition_output_sessions(session_ids, Some(SessionId(2)), |id| {
                schedulable.contains(&id)
            });

        assert_eq!(ready, vec![SessionId(2), SessionId(3)]);
        assert_eq!(deferred, vec![SessionId(1)]);
    }

    #[test]
    fn unschedulable_output_sessions_are_deferred_instead_of_dropped() {
        let session_ids = vec![SessionId(1), SessionId(2), SessionId(3)];
        let schedulable = HashSet::from([SessionId(3)]);

        let (ready, deferred) =
            prioritize_and_partition_output_sessions(session_ids, Some(SessionId(2)), |id| {
                schedulable.contains(&id)
            });

        assert_eq!(ready, vec![SessionId(3)]);
        assert_eq!(deferred, vec![SessionId(2), SessionId(1)]);
    }
}
