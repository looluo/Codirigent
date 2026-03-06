//! Auto-installs Codirigent hooks into `~/.claude/settings.json`.
//!
//! On first launch (and on updates), Codirigent merges its required hooks into
//! Claude Code's settings file so the user never has to run a manual setup
//! command. The merge is additive and idempotent — existing hooks from other
//! plugins are preserved.
//!
//! # Hooks registered
//!
//! | Event | Matcher | Purpose |
//! |---|---|---|
//! | `UserPromptSubmit` | (all) | Mark session as "working" |
//! | `Notification` | `idle_prompt\|permission_prompt` | Mark as "idle" or "needs_attention" |
//! | `Stop` | (all) | Mark session as "idle" |
//!
//! # Signal files
//!
//! Each hook invocation writes a tiny JSON signal file to
//! `~/.config/codirigent/signals/<session_id>.json` (Windows:
//! `%APPDATA%\codirigent\signals\`). Codirigent polls this directory to
//! update session status without reading multi-megabyte JSONL logs.

use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tracing::{debug, info};

/// Name embedded in each hook command so we can detect existing installations.
const HOOK_MARKER: &str = "codirigent-hook";

/// Ensure Codirigent's hooks are present in `~/.claude/settings.json`.
///
/// Safe to call on every launch — the function is idempotent.
/// Returns `Ok(true)` if the file was modified, `Ok(false)` if already up to date.
pub fn ensure_hooks_installed() -> Result<bool> {
    let settings_path =
        claude_settings_path().context("Could not determine ~/.claude/settings.json path")?;

    let mut settings = read_settings(&settings_path)?;
    let modified = merge_hooks(&mut settings)?;

    if modified {
        write_settings(&settings_path, &settings)?;
        info!("Codirigent hooks installed in {}", settings_path.display());
    } else {
        debug!("Codirigent hooks already present, no changes needed");
    }

    Ok(modified)
}

fn claude_settings_path() -> Option<PathBuf> {
    let home = home_dir()?;
    Some(home.join(".claude").join("settings.json"))
}

/// Returns the directory where `codirigent-hook` writes signal files.
///
/// | Platform | Path |
/// |---|---|
/// | Windows | `%APPDATA%\codirigent\signals` |
/// | Linux/macOS | `$XDG_CONFIG_HOME/codirigent/signals` (falls back to `~/.config/codirigent/signals`) |
///
/// Note: `%APPDATA%` (roaming app data) is distinct from `%USERPROFILE%` used for
/// `~/.claude`. This is intentional — signal files are transient runtime data that
/// belongs in the per-machine config location, not in the user's profile root.
pub fn hook_signals_dir() -> Option<PathBuf> {
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
            .or_else(|| home_dir().map(|h| h.join(".config")))?;
        Some(config_home.join("codirigent").join("signals"))
    }
}

fn home_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var("USERPROFILE").ok().map(PathBuf::from)
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("HOME").ok().map(PathBuf::from)
    }
}

fn read_settings(path: &Path) -> Result<Value> {
    if !path.exists() {
        return Ok(json!({}));
    }
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    serde_json::from_str(&content).with_context(|| format!("Failed to parse {}", path.display()))
}

fn write_settings(path: &Path, settings: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(settings).context("Failed to serialize settings")?;

    // Atomic write: write to a PID-scoped temp file then rename.
    // Using the process ID prevents two concurrent Codirigent instances from
    // clobbering each other's temp file during simultaneous startup.
    let tmp = path.with_file_name(format!(".settings-{}.tmp", std::process::id()));
    std::fs::write(&tmp, &json).with_context(|| format!("Failed to write {}", tmp.display()))?;
    std::fs::rename(&tmp, path)
        .with_context(|| format!("Failed to rename {} to {}", tmp.display(), path.display()))?;

    Ok(())
}

/// Merge Codirigent's hooks into the settings value. Returns true if modified.
fn merge_hooks(settings: &mut Value) -> Result<bool> {
    let hooks = settings
        .as_object_mut()
        .context("settings.json root must be an object")?
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .context("hooks must be an object")?;

    let mut modified = false;

    for (event, matcher, description) in hook_definitions() {
        let event_hooks = hooks
            .entry(event.to_string())
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .with_context(|| format!("{event} hooks must be an array"))?;

        if already_installed(event_hooks) {
            continue;
        }

        debug!("Adding Codirigent hook for {event} ({description})");
        event_hooks.push(json!({
            "matcher": matcher,
            "hooks": [{ "type": "command", "command": HOOK_MARKER }]
        }));
        modified = true;
    }

    Ok(modified)
}

fn hook_definitions() -> &'static [(&'static str, &'static str, &'static str)] {
    &[
        ("UserPromptSubmit", "", "mark session as working"),
        (
            "Notification",
            "idle_prompt|permission_prompt",
            "mark session as idle or needs_attention",
        ),
        ("Stop", "", "mark session as idle on exit"),
    ]
}

fn already_installed(event_hooks: &[Value]) -> bool {
    event_hooks.iter().any(|hook| {
        hook.get("hooks")
            .and_then(|h| h.as_array())
            .map(|cmds| {
                cmds.iter().any(|cmd| {
                    cmd.get("command")
                        .and_then(|c| c.as_str())
                        .map(|s| s.contains(HOOK_MARKER))
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_install_adds_three_hooks() {
        let mut settings = json!({});
        let modified = merge_hooks(&mut settings).unwrap();
        assert!(modified);

        let hooks = settings["hooks"].as_object().unwrap();
        assert!(hooks.contains_key("UserPromptSubmit"));
        assert!(hooks.contains_key("Notification"));
        assert!(hooks.contains_key("Stop"));
    }

    #[test]
    fn idempotent_second_call() {
        let mut settings = json!({});
        merge_hooks(&mut settings).unwrap();
        let modified = merge_hooks(&mut settings).unwrap();
        assert!(!modified, "second call should not modify");
    }

    #[test]
    fn preserves_existing_hooks() {
        let mut settings = json!({
            "hooks": {
                "UserPromptSubmit": [
                    {"matcher": "", "hooks": [{"type": "command", "command": "other-tool"}]}
                ]
            }
        });
        merge_hooks(&mut settings).unwrap();

        let arr = settings["hooks"]["UserPromptSubmit"].as_array().unwrap();
        assert_eq!(arr.len(), 2, "existing hook must be preserved");
    }

    #[test]
    fn does_not_duplicate_if_already_present() {
        // Pre-seed all three events that hook_definitions() produces.
        let mut settings = json!({
            "hooks": {
                "UserPromptSubmit": [
                    {"matcher": "", "hooks": [{"type": "command", "command": "codirigent-hook"}]}
                ],
                "Notification": [
                    {"matcher": "idle_prompt|permission_prompt", "hooks": [{"type": "command", "command": "codirigent-hook"}]}
                ],
                "Stop": [
                    {"matcher": "", "hooks": [{"type": "command", "command": "codirigent-hook"}]}
                ]
            }
        });
        let modified = merge_hooks(&mut settings).unwrap();
        assert!(!modified);

        for event in &["UserPromptSubmit", "Notification", "Stop"] {
            let arr = settings["hooks"][event].as_array().unwrap();
            assert_eq!(arr.len(), 1, "{event} must not be duplicated");
        }
    }
}
