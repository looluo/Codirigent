# Workspace Module

The workspace module owns the main application window: session layout, GPUI
view state, rendering, polling, and workspace-scoped UI interactions.

For the longer-form architecture reference, see
[`docs/architecture/workspace/`](../../../../docs/architecture/workspace/).

## Current Structure

- `core.rs`
  - Canonical workspace state and layout logic.
  - Session placement, focus, bounds, pane stacks, and layout transitions.

- `gpui.rs`
  - Root `WorkspaceView` type.
  - Constructor wiring, trait impls, render entry point, keyboard/IME handling,
    and root event wiring.
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

## Render-Facing Modules

- `render.rs`
  - Main workspace composition.

- `grid_render.rs`
  - Grid-layout composition, split/grid dispatch, and shared session-cell
    rendering.

- `split_render.rs`
  - Recursive split-tree rendering, divider setup, and empty split-slot
    rendering.

- `pane_header_render.rs`
  - Pane tabs, header badges, and pane-local session creation affordances.

- `impl_pointer_interactions.rs`
  - Workspace-global drag/resize reducers used by the GPUI root.

- `drawer_render.rs`, `task_board_render.rs`, `top_bar_render.rs`,
  `icon_rail_render.rs`, `modal_render.rs`, `terminal_render.rs`
  - Focused render helpers for their respective UI regions.

## Dependency Shape

- `Workspace` in `core.rs` stays free of GPUI concerns.
- `WorkspaceView` in `gpui.rs` is the GPUI-facing root and remains the main
  place to start reading the UI layer.
- `workspace/gpui/*.rs` helpers extend `WorkspaceView` without changing public
  module paths.
- `workspace/impl_output_polling/*.rs` helpers extend the polling root without
  changing public module paths.
- Sibling modules coordinate through `WorkspaceView` methods rather than
  importing one another's private helpers.

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

- Split rendering and divider behavior:
  - `split_render.rs`
  - `impl_pointer_interactions.rs`

- Pane tabs, badges, and pane `+` behavior:
  - `pane_header_render.rs`

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
