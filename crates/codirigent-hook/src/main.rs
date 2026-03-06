//! Codirigent hook handler for Claude Code.
//!
//! Registered in `~/.claude/settings.json` as the command for
//! `UserPromptSubmit`, `Notification`, and `Stop` hooks.
//!
//! Claude Code provides a JSON payload on stdin:
//! ```json
//! {
//!   "session_id": "abc123",
//!   "hook_event_name": "UserPromptSubmit",
//!   "cwd": "/path/to/project",
//!   "notification_type": "permission_prompt"
//! }
//! ```
//!
//! This binary writes a tiny signal file to
//! `~/.config/codirigent/signals/<session_id>.json` that Codirigent
//! reads to determine session status.
//!
//! # Session matching
//!
//! When Codirigent spawns a Claude Code PTY it sets the environment variable
//! `CODIRIGENT_SESSION_ID=<id>`. This binary inherits that variable (Claude
//! Code passes its env to child processes) and writes it into the signal
//! file. Codirigent then matches signal files by `codirigent_session_id`
//! rather than by CWD, giving exact per-session matching even when multiple
//! Claude Code sessions share the same working directory.

use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Payload received from Claude Code on stdin.
#[derive(Deserialize)]
struct HookPayload {
    session_id: String,
    hook_event_name: String,
    notification_type: Option<String>,
}

/// Signal file written for Codirigent to read.
#[derive(Serialize)]
struct SignalFile {
    status: &'static str,
    /// Codirigent session ID injected via `CODIRIGENT_SESSION_ID` env var.
    /// `None` if Claude Code was started outside of Codirigent.
    codirigent_session_id: Option<String>,
    ts: u64,
}

fn main() {
    let mut input = String::new();
    if std::io::stdin().read_to_string(&mut input).is_err() {
        return;
    }

    let payload: HookPayload = match serde_json::from_str(&input) {
        Ok(p) => p,
        Err(_) => return,
    };

    // Only process hooks for sessions Codirigent owns.
    let codirigent_session_id = std::env::var("CODIRIGENT_SESSION_ID").ok();
    if codirigent_session_id.is_none() {
        return;
    }

    let status = map_status(
        &payload.hook_event_name,
        payload.notification_type.as_deref(),
    );
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let signal = SignalFile {
        status,
        codirigent_session_id,
        ts,
    };

    // Validate session_id before using it as a filename component.
    // Claude Code session IDs are UUID-like; reject anything with path separators.
    if !is_safe_filename(&payload.session_id) {
        return;
    }

    if let Some(signals_dir) = signals_dir() {
        let _ = std::fs::create_dir_all(&signals_dir);
        let path = signals_dir.join(format!("{}.json", payload.session_id));
        if let Ok(json) = serde_json::to_string(&signal) {
            let _ = std::fs::write(path, json);
        }
    }
}

/// Returns true if `name` is safe to use as a filename component.
///
/// Rejects anything containing path separators or other characters that could
/// cause the signal file to be written outside the signals directory.
fn is_safe_filename(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
}

fn map_status(event: &str, notification_type: Option<&str>) -> &'static str {
    match event {
        "UserPromptSubmit" => "working",
        "Stop" => "idle",
        "Notification" => match notification_type {
            Some("permission_prompt") => "needs_attention",
            _ => "idle",
        },
        _ => "idle",
    }
}

/// Returns the directory where this binary writes signal files.
///
/// Mirrors `codirigent_core::hook_signals_dir()` — kept here to avoid
/// depending on `codirigent-core` (which pulls in heavy GPUI transitive deps).
/// Keep both functions in sync when updating paths.
///
/// | Platform | Path |
/// |---|---|
/// | Windows  | `%APPDATA%\codirigent\signals` |
/// | Linux/macOS | `$XDG_CONFIG_HOME/codirigent/signals` (falls back to `~/.config/codirigent/signals`) |
fn signals_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var("APPDATA")
            .ok()
            .map(|p| PathBuf::from(p).join("codirigent").join("signals"))
    }
    #[cfg(not(target_os = "windows"))]
    {
        let config_home = std::env::var("XDG_CONFIG_HOME")
            .ok()
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var("HOME")
                    .ok()
                    .map(|h| PathBuf::from(h).join(".config"))
            })?;
        Some(config_home.join("codirigent").join("signals"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_status_user_prompt_submit_is_working() {
        assert_eq!(map_status("UserPromptSubmit", None), "working");
    }

    #[test]
    fn map_status_stop_is_idle() {
        assert_eq!(map_status("Stop", None), "idle");
    }

    #[test]
    fn map_status_notification_permission_prompt_is_needs_attention() {
        assert_eq!(
            map_status("Notification", Some("permission_prompt")),
            "needs_attention"
        );
    }

    #[test]
    fn map_status_notification_other_is_idle() {
        assert_eq!(map_status("Notification", Some("idle_prompt")), "idle");
        assert_eq!(map_status("Notification", None), "idle");
    }

    #[test]
    fn map_status_unknown_event_is_idle() {
        assert_eq!(map_status("UnknownEvent", None), "idle");
    }

    #[test]
    fn is_safe_filename_valid() {
        assert!(is_safe_filename("abc-123_DEF"));
        assert!(is_safe_filename("a"));
        assert!(is_safe_filename("some-uuid-1234"));
    }

    #[test]
    fn is_safe_filename_empty_rejected() {
        assert!(!is_safe_filename(""));
    }

    #[test]
    fn is_safe_filename_path_separators_rejected() {
        assert!(!is_safe_filename("../etc/passwd"));
        assert!(!is_safe_filename("foo/bar"));
        assert!(!is_safe_filename("foo\\bar"));
    }

    #[test]
    fn is_safe_filename_dot_rejected() {
        // Dots would allow .json extension injection.
        assert!(!is_safe_filename("foo.json"));
        assert!(!is_safe_filename(".hidden"));
    }
}
