# Workspace Architecture

This directory documents the current `codirigent-ui::workspace` module
structure. The goal is to let a future developer or coding agent answer
"where does this behavior live?" without opening every file in
`crates/codirigent-ui/src/workspace/`.

## Read This First

- [Module Map](module-map.md)
  - Top-level ownership boundaries.
  - Which files are roots, helpers, renderers, or state containers.

- [GPUI And Rendering](gpui.md)
  - `WorkspaceView`, render orchestration, pointer interactions, UI event
    translation, layout sync.

- [Output Polling And Status](output-polling.md)
  - PTY output flow, status reconciliation, JSONL polling, hook signals.

## Workspace In One Screen

`workspace/mod.rs` exposes two conceptual roots:

- `core.rs`
  - Canonical workspace state and layout logic.
  - No GPUI-specific rendering concerns.

- `gpui.rs`
  - `WorkspaceView` and the GPUI-facing shell around `Workspace`.
  - Renders the UI, owns UI-scoped state, and coordinates helper modules.

The second major internal root is:

- `impl_output_polling.rs`
  - Output polling, status refresh, detector maintenance, background checks,
    and compaction/task follow-up.

Everything else in `workspace/` either:

- extends `WorkspaceView` with a focused behavior cluster
- renders a specific UI region
- stores grouped sub-state used by the roots above

The key render and interaction helpers added by the recent refactors are:

- `grid_render.rs`
  - Grid-layout composition and shared session-cell rendering.

- `split_render.rs`
  - Recursive split-tree rendering, divider setup, and empty split slots.

- `pane_header_render.rs`
  - Pane-header tabs, badges, and pane-local session creation controls.

- `impl_pointer_interactions.rs`
  - Workspace-global drag/resize reducers used by the GPUI root.

## Quick Lookup

If you need to change:

- layout switching, focus movement, terminal resize:
  - [gpui.md](gpui.md)
  - `gpui/layout_sync.rs`

- split-tree rendering or divider behavior:
  - [gpui.md](gpui.md)
  - `split_render.rs`
  - `impl_pointer_interactions.rs`

- pane tabs, header badges, pane `+` behavior:
  - [gpui.md](gpui.md)
  - `pane_header_render.rs`

- grid cells and split/grid render dispatch:
  - [gpui.md](gpui.md)
  - `grid_render.rs`

- task board counts, header badges, empty cell sync:
  - [gpui.md](gpui.md)
  - `gpui/derived_state.rs`

- top bar, icon rail, empty-cell event translation:
  - [gpui.md](gpui.md)
  - `gpui/ui_events.rs`

- PTY output draining, terminal runtime application, output scheduling:
  - [output-polling.md](output-polling.md)
  - `impl_output_polling/output_runtime.rs`

- session status decisions, stale cache clearing, auto-assign/compaction follow-up:
  - [output-polling.md](output-polling.md)
  - `impl_output_polling/status_reconcile.rs`

- JSONL-based Codex/Gemini status ingestion:
  - [output-polling.md](output-polling.md)
  - `impl_output_polling/cli_pollers.rs`

- hook-signal ingestion:
  - [output-polling.md](output-polling.md)
  - `impl_output_polling/hook_signals.rs`

## Related Docs

- [../overview.md](../overview.md)
  - Crate-level architecture and high-level system view.

- [../../hook-and-status-system.md](../../hook-and-status-system.md)
  - Lower-level hook file format and end-to-end status semantics.

- [../../session-resume.md](../../session-resume.md)
  - Session resume behavior and restore-oriented details.
