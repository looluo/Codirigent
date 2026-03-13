# Workspace Module Map

This file maps the workspace source tree to responsibilities. Use it when you
need to find the right module quickly.

## Entry Points

### `workspace/mod.rs`

Owns module declarations and the public surface:

- `pub use core::{CellInfo, Workspace}`
- `pub use gpui::WorkspaceView` behind `gpui-full`

This file is intentionally boring. If behavior changes require touching
`workspace/mod.rs`, the change is probably architectural rather than local.

### `core.rs`

Canonical workspace model:

- session placement and removal
- grid/split-tree layout state
- focus movement
- cell bounds and visible session calculation

`core.rs` should stay free of GPUI rendering details.

### `gpui.rs`

Primary UI root:

- defines `WorkspaceView`
- owns constructor wiring and grouped UI state
- keeps GPUI trait impls easy to find
- coordinates render-time orchestration

Helper clusters that extend the root now live under `workspace/gpui/`.

### `impl_output_polling.rs`

Primary runtime/polling root:

- shared polling constants and helper types
- adaptive maintenance cadence
- detector maintenance orchestration
- clipboard preview updates
- kill-switch / shadow-mode toggles for the output pipeline transition

Helper clusters that extend the root now live under
`workspace/impl_output_polling/`.

## `workspace/gpui/` Submodules

### `session_metadata.rs`

Leaf helpers used by higher-level reducers:

- derive project names from session metadata
- resolve task titles from cached or fallback inputs

This module should remain dependency-light.

### `derived_state.rs`

Mutation-driven UI reducers:

- task board snapshot/count refresh
- terminal header synchronization
- empty-cell synchronization
- explicit derived-state refresh entry points

This module converts canonical session/task state into UI-facing cached state.

### `ui_events.rs`

Component event translation:

- task board and empty-cell event draining
- top bar events -> workspace mutations
- icon rail events -> drawer/settings actions

This is where component-local UI events become `WorkspaceView` actions.

### `layout_sync.rs`

Layout and focus follow-up:

- layout cache invalidation
- focus-sensitive layout refresh
- session selection helpers
- terminal cell metrics and PTY resize coordination

This module is the main bridge between UI layout changes and terminal runtime
effects.

## `workspace/impl_output_polling/` Submodules

### `output_runtime.rs`

Hot path for terminal output:

- event-driven output scheduling
- focused-session prioritization
- background output preparation
- prepared output application back on the UI thread

If output appears late or the wrong session gets priority, start here.

### `status_reconcile.rs`

Status application and side effects:

- reconciles detector and cached CLI hints via `status_engine`
- clears stale cached status
- applies task-state side effects
- triggers compaction completion and auto-assign follow-up

If status changes are correct but the UI/task side effects are wrong, start
here.

### `cli_pollers.rs`

Background polling for log-backed CLIs:

- Codex/Gemini JSONL reads
- rollout / execution-mode inference
- CLI-type detection fallback
- cache updates and notifications from JSONL snapshots

### `hook_signals.rs`

Background polling for hook signal files:

- signal-file scanning
- stale signal guards
- session-id resolution
- hook-derived status and CLI metadata updates

### `git_refresh.rs`

Background git refresh coordination:

- bulk git refresh scheduling
- apply refreshed git info to headers and session snapshots

### `terminal_input.rs`

Terminal follow-up helpers:

- deferred Enter handling
- VTE DSR/DA response forwarding
- compaction timeout cleanup

## Other Important Workspace Modules

### Operational `impl_*` files

These extend `WorkspaceView` outside the two split roots:

- `impl_session_lifecycle.rs`
  - create, restore, close, bootstrap, resume

- `impl_keyboard.rs`
  - keyboard shortcuts and keybinding-driven actions

- `impl_task_board.rs`
  - task board mutations and modal/task operations

- `impl_modals.rs`
  - modal state transitions

- `impl_file_tree.rs`
  - file tree / focused-session path sync

- `impl_clipboard.rs`
  - clipboard actions and session clipboard integration

- `impl_settings.rs`
  - settings page behavior

- `impl_action_handlers.rs`
  - GPUI action callbacks that delegate into higher-level helpers

- `impl_ui_operations.rs`
  - broader UI helpers that do not fit cleanly into render or polling roots

### Rendering files

These mostly build UI elements rather than owning long-lived behavior:

- `render.rs`
  - top-level composition for the workspace body

- `grid_render.rs`
  - grid-layout composition
  - split-vs-grid render dispatch
  - shared session-cell rendering

- `drawer_render.rs`
  - drawer panels and left-side content

- `split_render.rs`
  - split-tree recursion
  - divider rendering and drag hit areas
  - empty split-slot rendering

- `pane_header_render.rs`
  - pane-header tabs
  - header badges and title rows
  - pane-local `+` session creation affordance

- `task_board_render.rs`
  - task board UI and task cards

- `top_bar_render.rs`
  - top bar layout/profile UI

- `icon_rail_render.rs`
  - icon rail chrome and click surfaces

- `modal_render.rs`
  - modal composition

- `terminal_render.rs`
  - terminal-specific render helpers

### Pointer interaction helpers

- `impl_pointer_interactions.rs`
  - split-resize drag reducers
  - session-drag move/finalize reducers
  - workspace-global gesture completion/cancellation

### State containers

These group related state to keep `WorkspaceView` readable:

- `clipboard_state.rs`
- `project_state.rs`
- `settings_state.rs`
- `persistence_state.rs`
- `types.rs`

## Dependency Rules

The current structure tries to preserve these rules:

- `core.rs` remains the canonical layout/session model.
- `gpui.rs` stays readable as the main UI root.
- `impl_output_polling.rs` stays readable as the main polling root.
- child helper modules extend a root through `impl WorkspaceView` rather than
  introducing new public entry points.
- sibling helper modules coordinate through root-owned methods on
  `WorkspaceView`, not by importing one another's private helpers.

## Where To Start Reading

For common tasks:

- "How is the whole workspace assembled?"
  - `workspace/mod.rs`
  - `gpui.rs`
  - `render.rs`

- "Why did a layout or selection change affect terminal sizing?"
  - `gpui/layout_sync.rs`
  - `grid_render.rs`
  - `split_render.rs`

- "Why did a header drag or divider drag behave strangely?"
  - `impl_pointer_interactions.rs`
  - `pane_header_render.rs`
  - `split_render.rs`

- "Where do pane tabs or pane-header badges come from?"
  - `pane_header_render.rs`
  - `gpui/derived_state.rs`

- "Why is a session badge wrong?"
  - `impl_output_polling/status_reconcile.rs`
  - `status_engine.rs`
  - `gpui/derived_state.rs`

- "Why is output delayed or missing?"
  - `impl_output_polling/output_runtime.rs`
  - `output_dispatcher.rs`

- "Why didn't a task board/header update happen?"
  - `gpui/derived_state.rs`
  - `impl_task_board.rs`
