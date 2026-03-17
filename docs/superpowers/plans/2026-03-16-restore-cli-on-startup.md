# Restore CLI on Startup Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a user settings toggle that controls whether CLI resume commands are sent during session restore, treating sessions as generic shells when disabled.

**Architecture:** Three focused changes — add the config field, gate four CLI behaviours in the restore path on that field, then wire a toggle in the settings UI. Each task is independent and can be committed separately.

**Tech Stack:** Rust, serde_json, GPUI 0.2, codirigent-core config types

---

## File Map

| File | Role |
|------|------|
| `crates/codirigent-core/src/config.rs` | Add `restore_cli_on_startup` field to `GeneralSettings` |
| `crates/codirigent-ui/src/workspace/impl_session_lifecycle.rs` | Gate CLI type, codex metadata, resume commands on the setting |
| `crates/codirigent-ui/src/workspace/settings_panels.rs` | Add toggle row in General → Startup section |

**Working directory for all commands:** Run this once before starting any task:

```bash
cd /Users/cyw/Desktop/github/Dirigent/.worktrees/feature/session-restore-prompt
```

---

## Task 1: Add `restore_cli_on_startup` to `GeneralSettings`

**Files:**
- Modify: `crates/codirigent-core/src/config.rs`

### Background

`GeneralSettings` lives in `codirigent-core/src/config.rs`. It uses a hand-written `impl Default` (not `#[derive(Default)]`). The `default_true()` free function already exists in this file and is used by `NotificationSettings` fields — reuse it. The `#[serde(default = "default_true")]` attribute is required on the new field because existing settings files may have a `general` key but lack this field; without it, deserialization would fail.

- [ ] **Step 1: Write the failing tests**

In the `#[cfg(test)]` module at the bottom of `config.rs`, find the `// UserSettings tests` comment block. Add a new `// GeneralSettings tests` comment block immediately before `// AppearanceSettings tests` (which follows the UserSettings tests):

```rust
#[test]
fn test_general_settings_restore_cli_defaults_true() {
    let settings = GeneralSettings::default();
    assert!(settings.restore_cli_on_startup);
}

#[test]
fn test_general_settings_restore_cli_serialization() {
    let settings = GeneralSettings {
        editor_command: "vim".to_string(),
        default_shell: String::new(),
        default_working_dir: None,
        show_splash: true,
        restore_cli_on_startup: false,
    };
    let json = serde_json::to_string(&settings).unwrap();
    let parsed: GeneralSettings = serde_json::from_str(&json).unwrap();
    assert!(!parsed.restore_cli_on_startup);
}

#[test]
fn test_general_settings_restore_cli_backward_compat() {
    // Existing settings files lack this field — must deserialize as true
    let json = r#"{"editor_command":"code","default_shell":"","show_splash":true}"#;
    let parsed: GeneralSettings = serde_json::from_str(json).unwrap();
    assert!(parsed.restore_cli_on_startup);
}
```

- [ ] **Step 2: Run tests to confirm they fail**

```bash
cargo test -p codirigent-core test_general_settings_restore_cli 2>&1 | tail -10
```

Expected: compile error — `restore_cli_on_startup` does not exist yet, or struct literal missing field.

- [ ] **Step 3: Add the field to `GeneralSettings`**

In `crates/codirigent-core/src/config.rs`, add after `show_splash`:

```rust
/// Resume CLI sessions (claude/codex/gemini) on startup restore.
/// When false, sessions open as generic shells with no CLI launched.
#[serde(default = "default_true")]
pub restore_cli_on_startup: bool,
```

- [ ] **Step 4: Add the field to `impl Default for GeneralSettings`**

```rust
impl Default for GeneralSettings {
    fn default() -> Self {
        Self {
            editor_command: "code".to_string(),
            default_shell: String::new(),
            default_working_dir: None,
            show_splash: true,
            restore_cli_on_startup: true,
        }
    }
}
```

- [ ] **Step 5: Run tests to confirm they pass**

```bash
cargo test -p codirigent-core test_general_settings_restore_cli 2>&1 | tail -10
```

Expected: `test result: ok. 3 passed`

- [ ] **Step 6: Run the full config test suite to check for regressions**

```bash
cargo test -p codirigent-core 2>&1 | tail -5
```

Expected: all tests pass, 0 failed.

- [ ] **Step 7: Commit**

```bash
git add crates/codirigent-core/src/config.rs
git commit -m "feat: add restore_cli_on_startup to GeneralSettings"
```

---

## Task 2: Gate CLI restore behaviour in `finalize_restored_session_bootstrap`

**Files:**
- Modify: `crates/codirigent-ui/src/workspace/impl_session_lifecycle.rs`

### Background

`finalize_restored_session_bootstrap` is a `&mut self` method in `WorkspaceView`. It is the sole place where CLI resume commands, the CLI type badge, and codex session metadata are applied during restore. All four must be gated on `restore_cli_on_startup`.

Read the existing function body carefully before editing — the changes interleave with existing code. The current structure is (simplified):

1. `start_bootstrapped_session_monitoring(...)` — leave alone
2. `sync_manager_session_shell(...)` — leave alone
3. `record_effective_session_shell(...)` — leave alone
4. `record_restored_shell_warning(...)` — leave alone
5. `clipboard_service.set_session_cli_type(...)` ← **gate (a)**
6. `if plan.codex_execution_mode.is_some() || ...` block ← **gate (b)**
7. `let mut session = bootstrapped.session; session.codex_execution_mode = ...` ← **gate (c)**
8. `attach_bootstrapped_session(...)` — leave alone
9. `set_session_group(...)` — leave alone
10. `for command in restore_resume_commands(...)` ← **gate (d)**

### Tests

`finalize_restored_session_bootstrap` requires a full `WorkspaceView` and cannot be easily unit tested in isolation. The pure helper functions it calls are already tested. Add one focused unit test for `restore_plan_cli_type` confirming the `GenericShell` fallback is the correct value to use when gating, and one confirming `restore_resume_commands` returns empty for a plan with no CLI fields (documents the no-op path when all CLI fields are `None`):

- [ ] **Step 1: Write confirmatory tests (these document existing behaviour and will pass immediately)**

Add inside the existing `mod tests` block in `impl_session_lifecycle.rs`:

```rust
#[test]
fn restore_plan_cli_type_returns_generic_shell_for_empty_plan() {
    let plan = RestoreSessionPlan {
        original_session_id: SessionId(1),
        session_uuid: "uuid".to_string(),
        session_name: "Session 1".to_string(),
        working_dir: sample_working_dir(),
        shell: None,
        group: None,
        color: None,
        claude_resume: None,
        codex_resume: None,
        codex_execution_mode: None,
        codex_started_at: None,
        gemini_resume: None,
    };
    assert_eq!(restore_plan_cli_type(&plan), CliType::GenericShell);
}

#[test]
fn restore_resume_commands_empty_for_plan_with_no_cli_fields() {
    let plan = RestoreSessionPlan {
        original_session_id: SessionId(1),
        session_uuid: "uuid".to_string(),
        session_name: "Session 1".to_string(),
        working_dir: sample_working_dir(),
        shell: None,
        group: None,
        color: None,
        claude_resume: None,
        codex_resume: None,
        codex_execution_mode: None,
        codex_started_at: None,
        gemini_resume: None,
    };
    assert!(restore_resume_commands(&plan).is_empty());
}
```

- [ ] **Step 2: Run tests to confirm they pass (these document existing behaviour)**

```bash
cargo test -p codirigent-ui restore_plan_cli_type_returns_generic_shell 2>&1 | tail -5
cargo test -p codirigent-ui restore_resume_commands_empty_for_plan 2>&1 | tail -5
```

Expected: both pass.

- [ ] **Step 3: Apply the four gates to `finalize_restored_session_bootstrap`**

At the **top** of `finalize_restored_session_bootstrap`, immediately after the function opening brace, add:

```rust
let restore_cli = self.effective_user_settings().general.restore_cli_on_startup;
```

**Gate (a)** — replace the existing `set_session_cli_type` call:

```rust
// Before:
self.clipboard
    .clipboard_service
    .set_session_cli_type(bootstrapped.session_id, restore_plan_cli_type(&plan));

// After:
let cli_type = if restore_cli {
    restore_plan_cli_type(&plan)
} else {
    CliType::GenericShell
};
self.clipboard
    .clipboard_service
    .set_session_cli_type(bootstrapped.session_id, cli_type);
```

**Gate (b)** — add `restore_cli &&` to the codex session manager block:

```rust
// Before:
if plan.codex_execution_mode.is_some() || plan.codex_started_at.is_some() {

// After:
if restore_cli && (plan.codex_execution_mode.is_some() || plan.codex_started_at.is_some()) {
```

**Gate (c)** — after `session.codex_started_at = plan.codex_started_at;` (line ~1431), add:

```rust
if !restore_cli {
    session.codex_execution_mode = None;
    session.codex_started_at = None;
}
```

**Gate (d)** — wrap the existing resume commands loop:

```rust
// Before:
for command in restore_resume_commands(&plan) {
    ...
}

// After:
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

- [ ] **Step 4: Confirm the crate compiles**

```bash
cargo build -p codirigent-ui 2>&1 | tail -5
```

Expected: `Finished` with no errors.

- [ ] **Step 5: Run the full test suite**

```bash
cargo test --all 2>&1 | tail -5
```

Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/codirigent-ui/src/workspace/impl_session_lifecycle.rs
git commit -m "feat: gate CLI restore on restore_cli_on_startup setting"
```

---

## Task 3: Add the settings toggle to the General panel

**Files:**
- Modify: `crates/codirigent-ui/src/workspace/settings_panels.rs`

### Background

`render_general_settings` builds the General settings page. Near the top of the function, several `let` bindings extract values from `page.user_settings` — this is required because `page` is an immutable borrow and the toggle callbacks need mutable access to `this`. All existing toggles follow the same pattern.

The Startup section already contains the `show_splash` toggle. The new row goes **after** `show_splash` and **before** the `.child(settings_section_header("Notifications", theme, false))` line.

`setting_row(label, description, theme, control)` is a free function defined in this file. `theme` is already extracted as a local near the top of `render_general_settings`.

### No automated test

Settings panel rendering requires the GPUI test harness and is not unit-tested in this codebase. Verify visually by running the app and opening Settings → General.

- [ ] **Step 1: Extract the local bool at the top of `render_general_settings`**

Find the block of `let` extractions at the top of `render_general_settings` (near `let show_splash = page.user_settings.general.show_splash;`) and add:

```rust
let restore_cli_on_startup = page.user_settings.general.restore_cli_on_startup;
```

- [ ] **Step 2: Add the toggle row after `show_splash` and before the Notifications header**

Find:
```rust
        .child(settings_section_header("Notifications", theme, false))
```

Insert immediately before it:

```rust
        .child(setting_row(
            "Restore AI sessions",
            "Resume previous Claude/Codex/Gemini sessions on startup",
            theme,
            self.render_toggle_control(
                "toggle-restore-cli",
                restore_cli_on_startup,
                cx,
                |this, _, cx| {
                    if let Some(page) = this.settings.page.as_mut() {
                        page.user_settings.general.restore_cli_on_startup =
                            !page.user_settings.general.restore_cli_on_startup;
                        page.user_save_pending = true;
                    }
                    cx.notify();
                },
            ),
        ))
```

- [ ] **Step 3: Confirm the crate compiles**

```bash
cargo build -p codirigent-ui 2>&1 | tail -5
```

Expected: `Finished` with no errors.

- [ ] **Step 4: Run the full test suite**

```bash
cargo test --all 2>&1 | tail -5
```

Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/codirigent-ui/src/workspace/settings_panels.rs
git commit -m "feat: add restore AI sessions toggle in General settings"
```

---

## Final Verification

- [ ] Run `cargo clippy --all -- -D warnings` and fix any warnings
- [ ] Run `cargo test --all` one final time — all tests pass
- [ ] Open the app, go to Settings → General → Startup, confirm the new toggle appears between "Show splash screen" and the Notifications section
- [ ] Toggle it off, quit, relaunch — confirm sessions restore as shells only (no CLI prompt)
- [ ] Toggle it back on, quit, relaunch — confirm sessions restore with CLI resume commands
