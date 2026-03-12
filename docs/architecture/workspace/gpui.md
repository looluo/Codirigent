# GPUI And Rendering

This document explains the `WorkspaceView` side of the workspace module.

## What `gpui.rs` Owns

`crates/codirigent-ui/src/workspace/gpui.rs` is still the UI root even after
the split. It owns:

- the `WorkspaceView` type
- constructor wiring in `WorkspaceView::new`
- grouped UI state fields
- GPUI trait impls (`Render`, focus, IME/input handling)
- high-level render orchestration
- keyboard and IME behavior that still benefits from staying close to the root

The split was about moving lower-coupling helper clusters out of the root, not
about hiding the root.

## `WorkspaceView` State Layout

The struct is easiest to understand in four groups:

### Canonical services and shared backends

- `workspace`
- `event_bus`
- `session_manager`
- `detector`
- `task_manager`

These tie the UI to the core/session/detector layers.

### Rendered child components

- `top_bar`
- `icon_rail`
- `drawer`
- `task_board`
- `empty_cells`
- `terminal_headers`
- `terminals`

These are the long-lived UI components or terminal render surfaces.

### Grouped UI sub-state

- `project`
- `clipboard`
- `settings`
- `persistence`
- `modals`
- `selection`
- `polling`
- `cache`

These are intentionally split into dedicated state structs so the root does not
become a flat bag of dozens of unrelated fields.

### Output/status plumbing shared with polling

- `output_dispatcher`
- `update_rx`
- `update_tx`
- `cli_readers`
- `notification_manager`

These are UI-owned because the polling system ultimately mutates visible state.

## Child Modules Under `workspace/gpui/`

### `session_metadata.rs`

Small pure helpers:

- `session_project_name()`
- `resolved_task_title()`

This is the leaf module used by reducers when they need human-readable session
or task labels.

### `derived_state.rs`

Converts canonical session/task state into cached UI state:

- task board counts and snapshots
- terminal header state
- empty-grid-cell state

Key rule:

- derived state is refreshed from explicit mutation paths
- it should not be rebuilt as a silent render fallback

That rule keeps render cheap and prevents "render fixed my stale state" bugs.

### `ui_events.rs`

Drains component event queues and translates them into workspace mutations:

- task board events
- empty-cell create-session clicks
- top bar layout requests
- icon rail drawer/settings requests

This file should stay focused on translation, not business logic.

### `layout_sync.rs`

Owns the follow-up work after layout or selection changes:

- mark layout caches dirty
- focus-sensitive layout signatures
- selection helpers
- terminal cell metrics
- PTY resize throttling

This is the main place where UI layout changes meet terminal runtime behavior.

## Render Path

The high-level render flow is:

1. `Render::render()` in `gpui.rs`
2. drain pending UI component events
3. update cached layout signatures and render cell info
4. synchronize terminal dimensions and schedule PTY resizes
5. delegate UI composition into render-focused modules such as:
   - `render.rs`
   - `grid_render.rs`
   - `drawer_render.rs`
   - `task_board_render.rs`

Important design choice:

- the root keeps the render entry point so a reader can still find the UI
  lifecycle without jumping through many files first

## Mutation Path Rules

When a workspace mutation changes visible state, the usual follow-up pattern is:

1. mutate canonical workspace state
2. call `mark_layout_cache_dirty()` if structure/bounds changed
3. call `sync_layout_derived_state()` or `sync_task_derived_state()`
4. run any file-tree or session-selection follow-up
5. `cx.notify()` if the UI should repaint

Examples that follow this pattern:

- next layout
- toggle sidebar
- focus/select session
- top bar layout selection

## Where To Edit

If you need to change:

- terminal header fields:
  - `gpui/derived_state.rs`

- task board counters or task snapshot contents:
  - `gpui/derived_state.rs`

- top bar event behavior:
  - `gpui/ui_events.rs`
  - `top_bar_render.rs`

- icon rail event behavior:
  - `gpui/ui_events.rs`
  - `icon_rail_render.rs`

- empty-cell click behavior:
  - `gpui/ui_events.rs`

- session selection side effects:
  - `gpui/layout_sync.rs`

- resize throttling or collapsed-resize guards:
  - `gpui/layout_sync.rs`

- key handling or IME behavior:
  - `gpui.rs`
  - `impl_keyboard.rs`

## Cross-Platform Notes

The most platform-sensitive GPUI behavior in this area is terminal resize and
text input:

- collapsed intermediate layouts must not force PTYs to `1x1`
- IME and key handling must preserve correct behavior on macOS and Windows
- terminal font metrics are cached because font-system behavior differs by
  platform and is too expensive to recompute on every frame

When changing layout or input behavior, run the full gate and then manually
check both macOS and Windows UI behavior.
