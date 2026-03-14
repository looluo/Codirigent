//! Hook-signal scanning and apply helpers for `impl_output_polling`.

use super::super::cli_helpers::is_safe_cli_session_id;
use super::super::types::{CachedCliStatus, CliStatusSource, ProcessedHookSignal};
use super::WorkspaceView;
use codirigent_core::{
    hook_signals_dir, CliType, CodexExecutionMode, CodirigentEvent, EventBus, SessionId,
    SessionStatus,
};
use codirigent_detector::NotificationType;
use codirigent_session::clipboard_service::ClipboardService;
use gpui::Context;
use serde::Deserialize;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tracing::{trace, warn};

const CLI_TYPE_CLAUDE: &str = "claude";
const CLI_TYPE_GEMINI: &str = "gemini";
const CLI_TYPE_CODEX: &str = "codex";

/// Unix timestamp (seconds) recorded at process startup, acting as a
/// per-process "run epoch". Hook signals written before this moment belong to
/// a previous Codirigent run and must be ignored, regardless of the 600-second
/// recency window, to prevent stale signals from routing to re-used session IDs.
static APP_START_TS: std::sync::OnceLock<u64> = std::sync::OnceLock::new();

fn app_start_ts() -> u64 {
    *APP_START_TS.get_or_init(|| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    })
}

/// Eagerly initialize the hook-signal run epoch.
///
/// Must be called early in startup (e.g., `WorkspaceView::new`) so that
/// hook signals emitted between app launch and the first scan are not
/// incorrectly filtered as belonging to a previous run.
pub(super) fn init_app_start_ts() {
    let _ = app_start_ts();
}

fn cli_type_from_hook_signal_name(cli_type_name: &str) -> Option<CliType> {
    match cli_type_name {
        CLI_TYPE_CLAUDE => Some(CliType::ClaudeCode),
        CLI_TYPE_GEMINI => Some(CliType::GeminiCli),
        CLI_TYPE_CODEX => Some(CliType::CodexCli),
        _ => None,
    }
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

        // Reject signals written before this process started. Session IDs
        // (1, 2, 3 ...) reset on every restart, so a signal from a previous run
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
    /// Read hook signal files on a background thread and apply them on the UI thread.
    pub(super) fn spawn_background_hook_signal_check(&mut self, cx: &mut Context<Self>) {
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
        let mut cli_type_changed = false;
        let cli_type_name = cli_type.as_deref().unwrap_or(CLI_TYPE_CLAUDE);
        if let Some(cli_type) = cli_type_from_hook_signal_name(cli_type_name) {
            let current_cli_type = self
                .clipboard
                .clipboard_service
                .get_session_cli_type(session_id);
            self.clipboard
                .clipboard_service
                .set_session_cli_type(session_id, cli_type);
            cli_type_changed = current_cli_type != cli_type;
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
        let is_focused = Some(session_id) == focused_id;
        let prev_status = self.workspace.session(session_id).map(|s| s.status);
        let new_status = match status.as_str() {
            "working" => SessionStatus::Working,
            "needs_attention" => SessionStatus::NeedsAttention,
            "response_ready" => {
                if is_focused {
                    SessionStatus::Idle
                } else {
                    SessionStatus::ResponseReady
                }
            }
            // "idle" signal from the CLI (e.g. idle_prompt notification).
            // If the session was previously ResponseReady and is unfocused,
            // keep ResponseReady; the user hasn't read the response yet.
            _ => {
                if !is_focused && prev_status == Some(SessionStatus::ResponseReady) {
                    SessionStatus::ResponseReady
                } else {
                    SessionStatus::Idle
                }
            }
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
                    seen_at: Instant::now(),
                    source: CliStatusSource::Hook,
                    status_since,
                    ttl: Self::HOOK_SIGNAL_CACHE_TTL,
                },
            );
        }

        let prev_status_for_notif = prev_status.unwrap_or(SessionStatus::Idle);

        if new_status == SessionStatus::NeedsAttention
            && prev_status_for_notif != SessionStatus::NeedsAttention
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

        if new_status == SessionStatus::ResponseReady
            && prev_status_for_notif == SessionStatus::Working
        {
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

        if self.sync_session_status(session_id) || cli_type_changed {
            self.sync_session_header(session_id);
            cx.notify();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            codex_execution_mode_from_approval_and_sandbox(Some("never"), Some("workspace-write")),
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
}
