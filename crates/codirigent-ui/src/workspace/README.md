# Workspace Module

The workspace module owns the main application window: session layout, GPUI view state, rendering, polling, and workspace-scoped UI interactions.

## Current Structure

- `core.rs`
  - Canonical workspace state and layout logic.
  - Session placement, focus, bounds, and layout transitions.

- `gpui.rs`
  - Root `WorkspaceView` type.
  - Constructor wiring, trait impls, render entry point, keyboard/IME handling, and high-level orchestration.
  - Lower-coupling helper clusters live under `workspace/gpui/`:
    - `session_metadata.rs`
    - `derived_state.rs`
    - `ui_events.rs`
    - `layout_sync.rs`

- `impl_output_polling.rs`
  - Root polling coordinator for output/status maintenance.
  - Lower-coupling helper clusters live under `workspace/impl_output_polling/`:
    - `output_runtime.rs`
    - `status_reconcile.rs`
    - `cli_pollers.rs`
    - `hook_signals.rs`
    - `git_refresh.rs`
    - `terminal_input.rs`

- Rendering modules
  - `render.rs`, `grid_render.rs`, `drawer_render.rs`, `task_board_render.rs`, `top_bar_render.rs`, `icon_rail_render.rs`, `modal_render.rs`
  - These keep UI composition close to the components they render while relying on root-owned `WorkspaceView` state.

## Dependency Shape

- `Workspace` in `core.rs` stays free of GPUI concerns.
- `WorkspaceView` in `gpui.rs` is the GPUI-facing root and remains the main place to start reading the UI layer.
- `workspace/gpui/*.rs` helpers extend `WorkspaceView` without changing public module paths.
- `workspace/impl_output_polling/*.rs` helpers extend the polling root without changing public module paths.
- Sibling modules coordinate through `WorkspaceView` methods rather than importing one another's private helpers.

## Key Responsibilities

- Layout and focus:
  - `core.rs`
  - `workspace/gpui/layout_sync.rs`

- Derived UI state:
  - `workspace/gpui/derived_state.rs`

- UI event translation:
  - `workspace/gpui/ui_events.rs`

- Session metadata helpers:
  - `workspace/gpui/session_metadata.rs`

- Output polling and runtime preparation:
  - `impl_output_polling.rs`
  - `workspace/impl_output_polling/output_runtime.rs`
  - `workspace/impl_output_polling/cli_pollers.rs`
  - `workspace/impl_output_polling/status_reconcile.rs`
  - `workspace/impl_output_polling/hook_signals.rs`
  - `workspace/impl_output_polling/git_refresh.rs`
  - `workspace/impl_output_polling/terminal_input.rs`

## Testing

The workspace layer is verified with:

- core unit tests in `workspace/tests.rs`
- module-local tests moved next to extracted helpers where practical
- full workspace and UI crate test runs via:

```bash
cargo test -p codirigent-ui --lib workspace::
```

For refactor verification, use the full workspace gate documented in `docs/architecture/workspace-module-split-plan.md`.
