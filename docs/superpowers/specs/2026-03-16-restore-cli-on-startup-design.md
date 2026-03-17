# Design: Restore CLI on Startup Setting

**Date:** 2026-03-16
**Branch:** feature/session-restore-prompt

## Summary

Add a user-level settings toggle — **Restore AI sessions** — that controls whether CLI resume commands (`claude --resume`, `codex resume`, `gemini --resume`) are automatically sent to the shell when sessions are restored on startup. The shell, working directory, and layout always restore regardless of this setting.

## Background

Currently, session restore always sends CLI resume commands after bootstrapping each shell. There is no way for users to opt out of this behaviour without manually closing the CLI session each time. This setting gives users control over whether they want to resume their previous AI conversation context or start a fresh CLI session in the same directory.

## Design

### 1. Config (`codirigent-core/src/config.rs`)

Add `restore_cli_on_startup: bool` to `GeneralSettings`:

```rust
#[serde(default = "default_true")]
pub restore_cli_on_startup: bool,
```

- Default: `true` (preserves current behaviour, backward-compatible via serde default)
- Stored in: `~/.config/codirigent/settings.json`

### 2. Session restore gate (`codirigent-ui/src/workspace/impl_session_lifecycle.rs`)

In `finalize_restored_session_bootstrap`, wrap the existing resume command loop:

```rust
if self.effective_user_settings().general.restore_cli_on_startup {
    for command in restore_resume_commands(&plan) {
        // send_input ...
    }
}
```

No structural changes needed — `effective_user_settings()` is already accessible in this method.

### 3. Settings UI (`codirigent-ui/src/workspace/settings_panels.rs`)

In `render_general_settings`, add a toggle under the existing **Startup** section after `show_splash`:

- **Label:** Restore AI sessions
- **Description:** Resume previous Claude/Codex/Gemini sessions on startup
- **Toggle ID:** `toggle-restore-cli`

Callback follows the existing pattern:
```rust
page.user_settings.general.restore_cli_on_startup =
    !page.user_settings.general.restore_cli_on_startup;
page.user_save_pending = true;
```

Setting `user_save_pending = true` triggers the existing debounced save pipeline, persisting the value to `~/.config/codirigent/settings.json`.

## Data Flow

```
User toggles setting
  → page.user_settings.general.restore_cli_on_startup updated
  → page.user_save_pending = true
  → debounced save writes ~/.config/codirigent/settings.json

On next startup:
  settings loaded into cached_user_settings
  → spawn_restore_sessions_from_disk
  → apply_restore_plan (per session batch)
  → finalize_restored_session_bootstrap
  → if restore_cli_on_startup: send resume commands
```

## Files Changed

| File | Change |
|------|--------|
| `crates/codirigent-core/src/config.rs` | Add `restore_cli_on_startup` field + serde default + tests |
| `crates/codirigent-ui/src/workspace/settings_panels.rs` | Add toggle in Startup section |
| `crates/codirigent-ui/src/workspace/impl_session_lifecycle.rs` | Gate resume commands on setting |

## Non-Goals

- Does not affect shell/PTY restore
- Does not affect layout restore
- Does not affect working directory restore
- No startup prompt or dialog — this is a persistent preference only
