# Session Shell Selection Plan

## Purpose

Define the full implementation scope for GitHub issue `#12`: allow users to choose which shell environment a session opens with, such as `bash`, `zsh`, `pwsh`, `powershell`, or `cmd`, and preserve that choice across session persistence and restore.

## Problem

Users may work across multiple shell environments depending on project or platform needs. The application already supports a global default shell, but that is not sufficient when users want different sessions to run in different shells at the same time.

The product requirement is per-session shell choice at creation time, with persistence and restore behavior that keeps sessions consistent across restarts.

## Goals

- Let users choose a shell when creating a session.
- Support all session creation entry points consistently.
- Persist the chosen shell as part of session state.
- Restore sessions using their original shell choice.
- Show the selected shell in the UI.
- Handle missing shells on restore in a predictable way.

## Non-Goals

- No live in-place shell mutation for an already running PTY.
- No freeform shell command text entry in the first implementation.
- No separate duplicate-session or clone-session behavior.

## Creation Entry Points

Shell selection should be available anywhere a new session can be created:

- clicking an empty pane
- clicking the pane-level `+` button in a tabbed pane

Both entry points should use the same create-session flow so behavior stays consistent.

## Shell Selection UX

The create-session flow should include a shell selector populated from detected available shells.

Expected options:

- `Auto`
- detected installed shells for the current platform

Examples:

- macOS/Linux: `bash`, `zsh`, `sh`
- Windows: `pwsh`, `powershell`, `cmd`

The selector should use friendly labels where possible, while still storing the exact shell identifier needed by the session manager.

## Behavior Model

### Auto

- `Auto` means the session should use the application's existing default-shell behavior.
- This should continue to respect the global default shell setting, or platform default behavior if the global setting is unset.

### Explicit Shell

- If the user selects a shell explicitly, that choice applies only to the new session being created.
- It overrides `Auto` for that session.

## Persistence

The selected shell must be stored as part of persistent session state.

This should be represented on:

- `Session`
- `PersistentSession`

It should not live only in transient UI state or be inferred only from the launch path.

## Restore Behavior

On restore:

- if the saved shell is still available, restore the session with that shell
- if the saved shell is unavailable, restore the session using `Auto`

The application should surface a clear warning that the originally requested shell was unavailable and that `Auto` was used instead.

Restore should not silently swap shells without feedback, and it should not drop the session entirely because the requested shell is missing.

## UI Visibility

The selected shell should be visible in the session UI so users can verify environment at a glance.

Recommended places:

- session details or session menu
- compact session/pane header indicator where space permits

The shell display should distinguish between:

- `Auto`
- explicit shell selections such as `bash` or `pwsh`

## Changing Shell After Creation

Shell choice should be treated as a session launch property, not a live mutable terminal property.

That means:

- changing shell for an existing session should not attempt to mutate the running PTY in place
- if the product later exposes a shell-change action, it should be modeled as reopening or recreating the session with a different shell

This issue does not require implementing that action now, but the underlying data model should not imply live shell mutation is supported.

## Implementation Outline

### Session Model

Extend the session domain model so a session can carry its shell choice explicitly.

Expected responsibilities:

- represent `Auto` versus explicit shell choice
- persist the chosen value
- restore it faithfully

### Create Flow

Update the create-session UI flow used by empty-pane creation and pane `+` creation so it includes shell selection and passes the chosen shell through to session bootstrap.

### Restore Flow

Update restore planning and bootstrap so restored sessions use their stored shell value, with fallback-to-`Auto` if the shell is missing.

### Availability Detection

Use the existing shell detection mechanism as the source of available shell choices and restore validation.

### Warning Surface

When restore falls back to `Auto`, surface a warning in a user-visible way so the mismatch is not silent.

## Testing Plan

Add or update tests for:

- creating a session with `Auto`
- creating a session with an explicit shell
- persisting the selected shell into saved state
- restoring a session with the same shell when available
- restoring a session with fallback to `Auto` when the saved shell is unavailable
- warning generation for unavailable saved shells
- consistent shell-selection behavior across both creation entry points

## Rollout Notes

This feature should be implemented as a full per-session shell-selection workflow, not only as a one-time creation override.

The expected finished behavior is:

- user chooses shell at creation time
- shell is stored with the session
- restore uses the same shell when possible
- restore falls back to `Auto` with clear warning when necessary
- the UI shows what shell the session is using
