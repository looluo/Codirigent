//! Codirigent hook handler for Claude and Codex CLI.
//!
//! Claude Code passes hook JSON on stdin via `~/.claude/settings.json`.
//! Codex executes the command from `~/.codex/config.toml` with JSON as the first
//! CLI argument.
//!
//! Both sources are normalized to the same signal file format:
//! `<codirigent-signals-dir>/<session_id>.json`.

use serde::{Deserialize, Serialize};
use std::env;
use std::io::Read;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const CLI_TYPE_CLAUDE: &str = "claude";
const CLI_TYPE_CODEX: &str = "codex";
const CLI_TYPE_GEMINI: &str = "gemini";

/// Payload from Claude Code hook stdin, Codex notify argument, or Gemini CLI hook.
#[derive(Deserialize)]
struct HookPayload {
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    hook_event_name: Option<String>,
    #[serde(default)]
    notification_type: Option<String>,
    #[serde(
        rename = "type",
        alias = "event",
        alias = "event_type",
        default
    )]
    event_type: Option<String>,
    #[serde(default)]
    cli_type: Option<String>,
}

/// Signal file format consumed by `check_hook_signals`.
#[derive(Serialize)]
struct SignalFile {
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    cli_type: Option<&'static str>,
    codirigent_session_id: Option<String>,
    ts: u64,
}

fn main() {
    let payload = match read_payload() {
        Ok(p) => p,
        Err(_) => return,
    };

    handle_payload(payload);
}

fn handle_payload(payload: HookPayload) {
    // Only process signals for sessions launched by Codirigent.
    let codirigent_session_id = env::var("CODIRIGENT_SESSION_ID").ok();
    if codirigent_session_id.is_none() {
        return;
    }

    let filename_session_id = payload
        .session_id
        .as_deref()
        .filter(|id| is_safe_filename(id))
        .or_else(|| codirigent_session_id.as_deref().filter(|id| is_safe_filename(id)))
        .unwrap_or_default();
    if filename_session_id.is_empty() {
        return;
    }

    let status = map_status(
        payload.hook_event_name.as_deref(),
        payload.notification_type.as_deref(),
        payload.event_type.as_deref(),
        payload.cli_type.as_deref(),
    );
    let cli_type = payload
        .cli_type
        .as_deref()
        .or_else(|| payload.hook_event_name.as_deref().map(|_| CLI_TYPE_CLAUDE))
        .or_else(|| payload.event_type.as_deref().map(|_| CLI_TYPE_CODEX))
        .unwrap_or(CLI_TYPE_GEMINI);

    let signal = SignalFile {
        status,
        cli_type: Some(cli_type),
        codirigent_session_id,
        ts: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
    };

    if let Some(signals_dir) = signals_dir() {
        let _ = std::fs::create_dir_all(&signals_dir);
        let path = signals_dir.join(format!("{}.json", filename_session_id));
        if let Ok(json) = serde_json::to_string(&signal) {
            let _ = std::fs::write(path, json);
        }
    }
}

fn read_payload() -> Result<HookPayload, serde_json::Error> {
    let mut input = String::new();
    if std::io::stdin().read_to_string(&mut input).is_ok() {
        let trimmed = input.trim();
        if !trimmed.is_empty() {
            if let Ok(payload) = serde_json::from_str::<HookPayload>(trimmed) {
                return Ok(payload);
            }
        }
    }

    let args: Vec<String> = env::args().collect();
    match args.get(1..).and_then(|parts| parts.last()) {
        Some(last) => serde_json::from_str(last),
        None => Err(serde_json::Error::custom("No hook payload")),
    }
}

fn map_status(
    hook_event: Option<&str>,
    notification_type: Option<&str>,
    event_type: Option<&str>,
    cli_type: Option<&str>,
) -> &'static str {
    if let Some(event) = hook_event {
        return match event {
            "UserPromptSubmit" => "working",
            "Stop" => "response_ready",
            "Notification" => match notification_type {
                Some("permission_prompt") => "needs_attention",
                _ => "idle",
            },
            _ => "idle",
        };
    }

    if let Some(cli) = cli_type {
        if cli == CLI_TYPE_GEMINI {
            return map_gemini_status(event_type);
        }
    }

    map_codex_status(event_type)
}

fn map_gemini_status(event_type: Option<&str>) -> &'static str {
    let event_type = event_type.unwrap_or("").to_ascii_lowercase();

    if event_type.contains("working") || event_type.contains("started") {
        return "working";
    }

    if event_type.contains("attention")
        || event_type.contains("prompt")
        || event_type.contains("input")
        || event_type.contains("ask")
    {
        return "needs_attention";
    }

    if event_type.contains("ready")
        || event_type.contains("stopped")
        || event_type.contains("finished")
        || event_type.contains("complete")
    {
        return "response_ready";
    }

    "idle"
}

fn map_codex_status(event_type: Option<&str>) -> &'static str {
    let event_type = event_type.unwrap_or("").to_ascii_lowercase();

    if event_type == "agent-turn-complete"
        || event_type == "response.completed"
        || event_type == "response.done"
        || event_type == "turn_complete"
    {
        return "response_ready";
    }

    if event_type.contains("permission")
        || event_type.contains("approval")
        || event_type.contains("question")
        || event_type.contains("ask")
    {
        return "needs_attention";
    }

    if event_type.contains("start")
        || event_type.contains("begin")
        || event_type.contains("turn-start")
        || event_type.contains("working")
    {
        return "working";
    }

    "idle"
}

/// Return true if a session id is safe as a filename stem.
fn is_safe_filename(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Returns the directory where this binary writes signal files.
///
/// Mirrors `codirigent_core::hook_signals_dir()` to avoid a heavy dependency
/// on `codirigent-core` from the hook binary crate.
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
            .or_else(|| std::env::var("HOME").ok().map(|h| PathBuf::from(h).join(".config")))?;
        Some(config_home.join("codirigent").join("signals"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_status_user_prompt_submit_is_working() {
        assert_eq!(map_status(Some("UserPromptSubmit"), None, None), "working");
    }

    #[test]
    fn map_status_stop_is_response_ready() {
        assert_eq!(map_status(Some("Stop"), None, None), "response_ready");
    }

    #[test]
    fn map_status_notification_permission_prompt_is_needs_attention() {
        assert_eq!(
            map_status(Some("Notification"), Some("permission_prompt"), None),
            "needs_attention"
        );
    }

    #[test]
    fn map_status_notification_other_is_idle() {
        assert_eq!(map_status(Some("Notification"), Some("idle_prompt"), None), "idle");
        assert_eq!(map_status(Some("Notification"), None, None), "idle");
    }

    #[test]
    fn map_status_unknown_event_is_idle() {
        assert_eq!(map_status(Some("UnknownEvent"), None, None), "idle");
    }

    #[test]
    fn map_codex_status_turn_complete_is_response_ready() {
        assert_eq!(map_codex_status(Some("agent-turn-complete")), "response_ready");
    }

    #[test]
    fn map_codex_status_start_events_are_working() {
        assert_eq!(map_codex_status(Some("agent-turn-start")), "working");
        assert_eq!(map_codex_status(Some("turn_start")), "working");
    }

    #[test]
    fn map_codex_status_permission_events_need_attention() {
        assert_eq!(map_codex_status(Some("permission_prompt")), "needs_attention");
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
        assert!(!is_safe_filename("foo.json"));
        assert!(!is_safe_filename(".hidden"));
    }
}
