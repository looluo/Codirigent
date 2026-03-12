# Workspace Architecture

This directory documents the `codirigent-ui::workspace` module after the
module split. The goal is to let a future developer or coding agent answer
"where does this behavior live?" without opening every file in
`crates/codirigent-ui/src/workspace/`.

## Read This First

- [Module Map](module-map.md)
  - Top-level ownership boundaries.
  - Which files are roots, helpers, renderers, or state containers.

- [GPUI And Rendering](gpui.md)
  - `WorkspaceView`, render orchestration, UI event translation, layout sync.

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

## Quick Lookup

If you need to change:

- layout switching, focus movement, terminal resize:
  - [gpui.md](gpui.md)
  - `gpui/layout_sync.rs`

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
