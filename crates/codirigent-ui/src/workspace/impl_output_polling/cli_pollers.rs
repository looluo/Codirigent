//! Background JSONL and rollout polling helpers.

use super::super::cli_helpers::is_safe_cli_session_id;
use super::super::types::{CachedCliStatus, CliStatusSource};
use super::WorkspaceView;
use codirigent_core::{
    CliType, CodexExecutionMode, CodirigentEvent, EventBus, ProcessMonitor, SessionId,
    SessionManager, SessionStatus,
};
use codirigent_detector::NotificationType;
use codirigent_session::cli_detector::CliDetector;
use codirigent_session::clipboard_service::ClipboardService;
use codirigent_session::CliSessionStatus;
use gpui::Context;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant, UNIX_EPOCH};
use tracing::{info, trace, warn};

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

impl WorkspaceView {
    pub(super) fn spawn_background_jsonl_check(&mut self, cx: &mut Context<Self>) {
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
            .filter_map(|session| {
                let cli_type = self
                    .clipboard
                    .clipboard_service
                    .get_session_cli_type(session.id);
                // ClaudeCode uses hook signals exclusively; skip JSONL collection
                // to avoid unnecessary PID lookup and working dir copy.
                if cli_type == CliType::ClaudeCode {
                    return None;
                }
                let child_pid =
                    self.with_session_manager(|manager| manager.get_child_pid(session.id));
                let known_codex_session_id = session
                    .codex_session_id
                    .as_ref()
                    .filter(|id| *id != &session.id.0.to_string())
                    .cloned();
                Some(JsonlCheckInput {
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
                })
            })
            .collect();

        let no_id_codex_counts =
            count_codex_sessions_without_session_id_per_working_dir(&jsonl_inputs);

        let cli_readers = self.cli_readers.clone();
        let event_bus = self.event_bus.clone();
        let max_age = Self::GENERIC_SHELL_JSONL_MAX_AGE;

        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            // Background: perform JSONL reads (the expensive I/O)
            let results: (
                Vec<JsonlCheckOutput>,
                Vec<JsonlCheckInput>,
                Vec<(SessionId, CliType)>,
            ) = cx
                .background_executor()
                .spawn(async move {
                    let mut out: Vec<JsonlCheckOutput> = Vec::new();
                    let mut detected_types: Vec<(SessionId, CliType)> = Vec::new();
                    if let Ok(mut readers) = cli_readers.lock() {
                        for input in &jsonl_inputs {
                            // For GenericShell sessions, try process-tree detection.
                            // The detector walks the PTY's child processes looking
                            // for known CLI binaries (claude, gemini, codex).
                            // Note: don't use process-tree to REVERT ClaudeCode -> GenericShell
                            // because detection is unreliable (returns GenericShell even when
                            // Claude is running). Banner detection handles initial detection.
                            let effective_type = if input.cli_type == CliType::GenericShell {
                                if let Some(pid) = input.child_pid {
                                    let detected = readers.detector.detect_cli_type(pid);
                                    if detected != CliType::GenericShell {
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
                                CliType::ClaudeCode => {
                                    // Claude Code status is handled by hook signal files
                                    // (spawn_background_hook_signal_check); no JSONL reader needed here.
                                    (None, None, None)
                                }
                                CliType::CodexCli => {
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
                                CliType::GeminiCli => (
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
                                CliType::GenericShell => (None, None, None),
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
                        if let Some(readers) = cached_status.as_mut() {
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
                                    seen_at: cache_update_time,
                                    source: CliStatusSource::Jsonl,
                                    status_since,
                                    ttl: Self::GENERIC_SHELL_JSONL_CACHE_TTL,
                                },
                            );
                        }
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
                                this.notification_handle.send(
                                    notif_type,
                                    result.session_id,
                                    &session_name,
                                    detail,
                                );
                            }
                        }
                        status_sync_ids.insert(result.session_id);
                    } else {
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
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
