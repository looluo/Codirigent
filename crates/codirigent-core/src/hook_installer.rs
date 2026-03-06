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

/// Substring that identifies a Codirigent hook command, used to detect and
/// upgrade legacy bare-name registrations.
const HOOK_MARKER: &str = "codirigent-hook";

/// Ensure Codirigent's hooks are present in `~/.claude/settings.json`.
///
/// `hook_binary` should be the full path to the `codirigent-hook` binary.
/// Using the full path avoids relying on PATH, which may not include the
/// binary's directory during Claude Code's hook execution.
///
/// Safe to call on every launch — the function is idempotent.
/// Returns `Ok(true)` if the file was modified, `Ok(false)` if already up to date.
pub fn ensure_hooks_installed(hook_binary: &Path) -> Result<bool> {
    let settings_path =
        claude_settings_path().context("Could not determine ~/.claude/settings.json path")?;

    let command = hook_binary.to_string_lossy().into_owned();
    let mut settings = read_settings(&settings_path)?;
    let modified = merge_hooks(&mut settings, &command)?;

    if modified {
        write_settings(&settings_path, &settings)?;
        info!("Codirigent hooks installed in {}", settings_path.display());
    } else {
        debug!("Codirigent hooks already present, no changes needed");
    }

    Ok(modified)
}

/// Returns the full path to the `codirigent-hook` binary that lives next to
/// the currently-running executable.
///
/// Falls back to the bare binary name `codirigent-hook` (relying on PATH)
/// if the current executable path cannot be determined.
pub fn hook_binary_path() -> PathBuf {
    let hook_name = if cfg!(windows) {
        "codirigent-hook.exe"
    } else {
        "codirigent-hook"
    };
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|dir| dir.join(hook_name)))
        .unwrap_or_else(|| PathBuf::from(hook_name))
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
///
/// If a hook entry already exists with the correct `command`, it is left
/// unchanged. If a legacy bare-name entry (containing `HOOK_MARKER` but not
/// matching `command`) is found, it is upgraded in place to `command`.
fn merge_hooks(settings: &mut Value, command: &str) -> Result<bool> {
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

        if already_installed(event_hooks, command) {
            continue;
        }

        // Upgrade a legacy bare-name entry rather than adding a duplicate.
        if upgrade_hook(event_hooks, command) {
            debug!("Upgraded Codirigent hook for {event} to full binary path");
            modified = true;
            continue;
        }

        debug!("Adding Codirigent hook for {event} ({description})");
        event_hooks.push(json!({
            "matcher": matcher,
            "hooks": [{ "type": "command", "command": command }]
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

/// Returns true if any hook entry already uses exactly `command`.
fn already_installed(event_hooks: &[Value], command: &str) -> bool {
    event_hooks.iter().any(|hook| {
        hook.get("hooks")
            .and_then(|h| h.as_array())
            .map(|cmds| {
                cmds.iter().any(|cmd| {
                    cmd.get("command")
                        .and_then(|c| c.as_str())
                        .map(|s| s == command)
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false)
    })
}

/// Upgrades a legacy bare-name hook (containing `HOOK_MARKER`) to `command`.
/// Returns true if any entry was updated.
fn upgrade_hook(event_hooks: &mut [Value], command: &str) -> bool {
    let mut upgraded = false;
    for hook in event_hooks.iter_mut() {
        if let Some(cmds) = hook.get_mut("hooks").and_then(|h| h.as_array_mut()) {
            for cmd in cmds.iter_mut() {
                let current = cmd
                    .get("command")
                    .and_then(|c| c.as_str())
                    .map(|s| s.to_owned());
                if let Some(c) = current {
                    if c.contains(HOOK_MARKER) && c != command {
                        cmd["command"] = json!(command);
                        upgraded = true;
                    }
                }
            }
        }
    }
    upgraded
}

#[cfg(test)]
mod tests {
    use super::*;

    const CMD: &str = "/usr/local/bin/codirigent-hook";
    const CMD2: &str = "/opt/codirigent/codirigent-hook";

    #[test]
    fn fresh_install_adds_three_hooks() {
        let mut settings = json!({});
        let modified = merge_hooks(&mut settings, CMD).unwrap();
        assert!(modified);

        let hooks = settings["hooks"].as_object().unwrap();
        assert!(hooks.contains_key("UserPromptSubmit"));
        assert!(hooks.contains_key("Notification"));
        assert!(hooks.contains_key("Stop"));
    }

    #[test]
    fn fresh_install_uses_provided_command() {
        let mut settings = json!({});
        merge_hooks(&mut settings, CMD).unwrap();
        let cmd = settings["hooks"]["UserPromptSubmit"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap();
        assert_eq!(cmd, CMD);
    }

    #[test]
    fn idempotent_second_call() {
        let mut settings = json!({});
        merge_hooks(&mut settings, CMD).unwrap();
        let modified = merge_hooks(&mut settings, CMD).unwrap();
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
        merge_hooks(&mut settings, CMD).unwrap();

        let arr = settings["hooks"]["UserPromptSubmit"].as_array().unwrap();
        assert_eq!(arr.len(), 2, "existing hook must be preserved");
    }

    #[test]
    fn does_not_duplicate_if_already_present() {
        let mut settings = json!({
            "hooks": {
                "UserPromptSubmit": [
                    {"matcher": "", "hooks": [{"type": "command", "command": CMD}]}
                ],
                "Notification": [
                    {"matcher": "idle_prompt|permission_prompt", "hooks": [{"type": "command", "command": CMD}]}
                ],
                "Stop": [
                    {"matcher": "", "hooks": [{"type": "command", "command": CMD}]}
                ]
            }
        });
        let modified = merge_hooks(&mut settings, CMD).unwrap();
        assert!(!modified);

        for event in &["UserPromptSubmit", "Notification", "Stop"] {
            let arr = settings["hooks"][event].as_array().unwrap();
            assert_eq!(arr.len(), 1, "{event} must not be duplicated");
        }
    }

    #[test]
    fn upgrades_bare_name_to_full_path() {
        // Simulate a legacy install with just "codirigent-hook" (no path).
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
        let modified = merge_hooks(&mut settings, CMD).unwrap();
        assert!(modified, "legacy entry must be upgraded");

        for event in &["UserPromptSubmit", "Notification", "Stop"] {
            let arr = settings["hooks"][event].as_array().unwrap();
            // No duplicate added — the existing entry was upgraded in place.
            assert_eq!(arr.len(), 1, "{event} must not grow");
            let cmd = arr[0]["hooks"][0]["command"].as_str().unwrap();
            assert_eq!(cmd, CMD, "{event} command must be updated to full path");
        }
    }

    #[test]
    fn idempotent_after_upgrade() {
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
        merge_hooks(&mut settings, CMD).unwrap();
        // Second call with same command must be a no-op.
        let modified = merge_hooks(&mut settings, CMD).unwrap();
        assert!(!modified, "second call after upgrade must be idempotent");
    }

    #[test]
    fn upgrades_when_binary_path_changes() {
        // Simulate a previous install with CMD, now re-installed to CMD2.
        let mut settings = json!({
            "hooks": {
                "UserPromptSubmit": [
                    {"matcher": "", "hooks": [{"type": "command", "command": CMD}]}
                ],
                "Notification": [
                    {"matcher": "idle_prompt|permission_prompt", "hooks": [{"type": "command", "command": CMD}]}
                ],
                "Stop": [
                    {"matcher": "", "hooks": [{"type": "command", "command": CMD}]}
                ]
            }
        });
        let modified = merge_hooks(&mut settings, CMD2).unwrap();
        assert!(modified, "changed path must trigger upgrade");

        for event in &["UserPromptSubmit", "Notification", "Stop"] {
            let arr = settings["hooks"][event].as_array().unwrap();
            assert_eq!(arr.len(), 1, "{event} must not grow");
            let cmd = arr[0]["hooks"][0]["command"].as_str().unwrap();
            assert_eq!(cmd, CMD2);
        }
    }
}
