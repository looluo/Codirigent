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

Also add to `impl Default for GeneralSettings`:

```rust
restore_cli_on_startup: true,
```

Both are required:
- `impl Default` — ensures `GeneralSettings::default()` compiles and returns the correct value.
- `#[serde(default = "default_true")]` — handles backward-compatible deserialization of existing settings files that contain a `general` key but lack this new field. `show_splash` has no such attribute because it was an original field and always exists in saved files; new fields added to an existing struct must carry field-level serde defaults.

- Default: `true` (preserves current behaviour)
- Stored via the existing settings service (platform-specific path resolved at runtime)
- Note: `default_true()` helper already exists in `config.rs` and can be reused directly.

### 2. Session restore gate (`codirigent-ui/src/workspace/impl_session_lifecycle.rs`)

Extract the flag once at the top of `finalize_restored_session_bootstrap`, then gate all CLI-specific state on it:

```rust
let restore_cli = self.effective_user_settings().general.restore_cli_on_startup;
```

**When `restore_cli = false`, the session is a generic shell.** Three things must be gated:

**a) CLI type badge** — pass `GenericShell` instead of the saved CLI type:
```rust
let cli_type = if restore_cli {
    restore_plan_cli_type(&plan)
} else {
    CliType::GenericShell
};
self.clipboard
    .clipboard_service
    .set_session_cli_type(bootstrapped.session_id, cli_type);
```

**b) Codex session manager state** — skip setting `codex_execution_mode` / `codex_started_at` on the session manager:
```rust
if restore_cli && (plan.codex_execution_mode.is_some() || plan.codex_started_at.is_some()) {
    // ... existing with_session_state_mut block unchanged
}
```

**c) Codex session struct fields** — clear them on the session before attaching:
```rust
if !restore_cli {
    session.codex_execution_mode = None;
    session.codex_started_at = None;
}
```

**d) Resume commands** — skip sending:
```rust
if restore_cli {
    for command in restore_resume_commands(&plan) {
        if let Ok(manager) = self.session_manager.lock() {
            if let Err(error) = manager.send_input(bootstrapped.session_id, command.as_bytes()) {
                warn!(?bootstrapped.session_id, %error, "Failed to send resume command");
            }
        }
    }
}
```

Extracting to a local `bool` is a clarity/defensive measure. `effective_user_settings(&self)` takes only `&self` — no `cx` parameter needed.

`finalize_restored_session_bootstrap` is the **sole call site** of `restore_resume_commands` and the sole place CLI type + codex metadata are applied during restore.

### 3. Settings UI (`codirigent-ui/src/workspace/settings_panels.rs`)

In `render_general_settings`, extract the value into a local `bool` (Copy) at the top of the function alongside the other locals (e.g. `show_splash`):

```rust
let restore_cli_on_startup = page.user_settings.general.restore_cli_on_startup;
```

Must be a `bool` copy (not a reference), to avoid a borrow conflict with the closure that later mutably borrows `this`.

Add a toggle under the existing **Startup** section, **after `show_splash` and before the Notifications section header**:

- **Label:** Restore AI sessions
- **Description:** Resume previous Claude/Codex/Gemini sessions on startup
- **Toggle ID:** `toggle-restore-cli`
- **Current value:** `restore_cli_on_startup` (the local extracted above)

Callback exactly matches the existing pattern (including guard and notify):
```rust
|this, _, cx| {
    if let Some(page) = this.settings.page.as_mut() {
        page.user_settings.general.restore_cli_on_startup =
            !page.user_settings.general.restore_cli_on_startup;
        page.user_save_pending = true;
    }
    cx.notify();
}
```

`user_save_pending = true` triggers the debounced save pipeline. `cx.notify()` triggers a UI re-render.

## Data Flow

```
User toggles setting
  → page.user_settings.general.restore_cli_on_startup updated
  → page.user_save_pending = true
  → debounced save persists via settings service

On next startup:
  settings loaded into cached_user_settings
  → spawn_restore_sessions_from_disk
  → apply_restore_plan (per session batch)
    → finalize_restored_session_bootstrap [sole CLI restore site]
      → reads restore_cli_on_startup into local bool
      → if true:  set CLI type badge, set codex metadata, send resume commands
      → if false: set CLI type = GenericShell, skip codex metadata, skip resume commands
```

## Files Changed

| File | Change |
|------|--------|
| `crates/codirigent-core/src/config.rs` | Add `restore_cli_on_startup` field + serde default + `impl Default` + tests |
| `crates/codirigent-ui/src/workspace/settings_panels.rs` | Add toggle in Startup section |
| `crates/codirigent-ui/src/workspace/impl_session_lifecycle.rs` | Gate CLI type, codex metadata, and resume commands on setting + tests |

## Non-Goals

- Does not affect shell/PTY restore
- Does not affect layout restore
- Does not affect working directory restore
- No startup prompt or dialog — this is a persistent preference only
