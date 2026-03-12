# Workspace Module Split Plan

## Status

Draft only. Local planning document for a follow-up maintainability refactor after the UI-thread offload work. This document is intentionally scoped as a no-behavior-change module split and dependency cleanup. It is not committed.

## Purpose

The UI-thread offload refactor is functionally complete, but the workspace layer now has several oversized files that are difficult to review and risky to extend. The next step is to split those files into smaller modules without changing behavior.

This plan exists to keep that work separate from the completed architectural refactor. The goals here are maintainability, reviewability, and dependency hygiene, not new product behavior.

## Primary Hotspots

Current line counts in `crates/codirigent-ui/src/workspace`:

- `impl_output_polling.rs`: 2647 lines
- `gpui.rs`: 2500 lines
- `impl_session_lifecycle.rs`: 1388 lines
- `settings_panels.rs`: 1448 lines
- `task_board_render.rs`: 1360 lines
- `drawer_render.rs`: 1234 lines

This plan focuses first on:

1. `crates/codirigent-ui/src/workspace/impl_output_polling.rs`
2. `crates/codirigent-ui/src/workspace/gpui.rs`

These two files are the highest-value split targets because they mix too many responsibilities and sit on the hottest integration boundaries.

## Why This Is Separate From The Offload Plan

The offload plan changed ownership boundaries and thread responsibilities. That work is complete enough to review as a functional unit.

This plan is different:

- It must be no-behavior-change.
- It will mostly move code, not redesign logic.
- It should preserve current public module paths where possible.
- It should be easy to review commit by commit.

Mixing this work into the earlier plan would blur architectural changes with structural ones and make rollback harder.

## Objectives

1. Reduce file size and responsibility sprawl in the workspace layer.
2. Make it obvious where output flow, status reconciliation, UI reducers, event handling, and render-adjacent logic live.
3. Keep module dependencies directional and predictable.
4. Preserve current behavior, current tests, and current feature gates.
5. Avoid introducing new cross-platform assumptions, new `unwrap()` usage, or new warnings.

## Non-Goals

This refactor must not:

- change session status behavior
- change output polling cadence
- change render behavior
- change task assignment behavior
- change file-tree behavior
- change startup/restore behavior
- move code across crates
- redesign the terminal runtime

If a change affects behavior, it belongs in a different plan.

## Constraints

1. Keep `workspace/mod.rs` stable if possible.
2. Prefer internal submodules under existing module roots before renaming public modules.
3. Keep `gpui-full` feature gating correct for every new module.
4. Keep test discovery and test names stable where feasible.
5. Preserve branch hygiene:
   - no new production `unwrap()`
   - no new warnings
   - no Unix-only path assumptions in touched production code

## Target Shape

### `impl_output_polling.rs`

Keep `workspace/mod.rs` unchanged with `mod impl_output_polling;`.

Use `impl_output_polling.rs` as a thin root module that owns shared types and re-exports internal helpers from submodules in `crates/codirigent-ui/src/workspace/impl_output_polling/`.

Proposed internal split:

- `output_runtime.rs`
  - `poll_output()`
  - dispatch scheduling
  - prepared output apply
  - terminal runtime handoff
  - OSC 7 / OSC 133 extraction

- `status_reconcile.rs`
  - `sync_session_status()`
  - cached-status reconciliation
  - session-status side effects
  - notifications and event-bus transitions tied to status changes

- `cli_pollers.rs`
  - JSONL readers
  - rollout readers
  - CLI metadata update application
  - background polling entry points

- `hook_signals.rs`
  - hook-signal scan
  - hook-signal apply
  - run-epoch helpers

- `git_refresh.rs`
  - background git refresh scheduling
  - apply helpers
  - cwd/git cache sync helpers

- `terminal_input.rs`
  - deferred enter handling
  - VTE response forwarding
  - compaction input follow-up helpers

- `tests.rs`
  - optional follow-up if test density keeps the root too large

Rules:

- Shared helper functions should stay close to the submodule that owns them.
- The root module should only keep cross-cutting types/constants that are genuinely shared by multiple submodules.
- Do not move business logic into one new giant replacement module.

### `gpui.rs`

Keep `workspace/mod.rs` unchanged with `pub mod gpui;`.

Keep `gpui.rs` as the root module that owns:

- `WorkspaceView`
- constructor wiring
- core trait impls (`Render`, `Focusable`, IME-related impls)
- any shared root-level constants that are used widely enough to justify staying at the top

Move implementation clusters into `crates/codirigent-ui/src/workspace/gpui/` submodules.

Proposed internal split:

- `derived_state.rs`
  - task-board reducer helpers
  - header sync helpers
  - empty-cell sync helpers
  - mutation-driven derived-state refresh entry points

- `ui_events.rs`
  - `process_ui_events()`
  - `process_top_bar_events()`
  - `process_icon_rail_events()`

- `layout_sync.rs`
  - layout switching helpers
  - session focus helpers
  - drag/swap follow-up helpers
  - terminal dimension / resize coordination

- `session_metadata.rs`
  - lightweight session metadata helpers such as project-name/task-title derivation

- `tests.rs`
  - optional only if root test module becomes noisy

Rules:

- `Render::render()` should remain easy to scan and mostly orchestration-only.
- Do not bury trait impls deep enough that `WorkspaceView` becomes hard to understand.
- Avoid circular helper dependencies between `derived_state`, `layout_sync`, and `ui_events`.

## Secondary Candidates

These are not phase-one split targets, but they should be reviewed after the primary split:

- `impl_session_lifecycle.rs`
- `settings_panels.rs`
- `task_board_render.rs`
- `drawer_render.rs`

They should not be pulled into the first branch unless the primary split exposes an obvious dependency problem that requires them to move.

## Dependency Rules

The split should make dependencies clearer, not more tangled.

Allowed direction:

- `gpui` root -> `gpui::*` helpers
- `impl_output_polling` root -> `impl_output_polling::*` helpers
- narrow helper modules -> `types`, `status_engine`, `output_dispatcher`, `project_state`, existing workspace utilities

Avoid:

- helper modules calling back into sibling modules in both directions
- shared “misc” modules
- moving state ownership into helper modules
- duplicating logic just to avoid imports

If two submodules need the same logic, either:

1. keep it in the root module, or
2. extract a clearly named shared helper

## Size Targets

Soft targets after the split:

- no primary workspace implementation file over 900 lines
- target most new implementation modules to land between 250 and 700 lines
- the root `gpui.rs` and `impl_output_polling.rs` files should become orchestration layers, not logic dumps

These are maintainability targets, not hard rules.

## Delivery Strategy

### Phase A: Scaffolding

Goals:

- create target submodule directories
- move only imports, helper declarations, and `mod` wiring where needed
- keep behavior identical

Checks:

- compile with no logic changes
- no public module path changes

### Phase B: Split `impl_output_polling.rs`

Recommended order:

1. `git_refresh.rs`
2. `terminal_input.rs`
3. `hook_signals.rs`
4. `cli_pollers.rs`
5. `status_reconcile.rs`
6. `output_runtime.rs`

Reason:

- start with the least risky chunks
- leave the highest-coupling output/runtime code for last after the module pattern is proven

Phase exit criteria:

- root `impl_output_polling.rs` is substantially smaller and mostly orchestration-only
- no behavior changes in output/status flow

### Phase C: Split `gpui.rs`

Recommended order:

1. `session_metadata.rs`
2. `derived_state.rs`
3. `ui_events.rs`
4. `layout_sync.rs`

Reason:

- begin with pure helpers
- move reducer logic before moving event orchestration
- leave render-adjacent layout coordination until the end

Phase exit criteria:

- root `gpui.rs` remains readable as the high-level workspace view entry point
- trait impls are still easy to locate

### Phase D: Cleanup And Naming Pass

Goals:

- normalize module names
- remove dead helpers/imports
- consolidate any duplicated private helper logic created during the move
- confirm file sizes and dependency directions are improved

Phase exit criteria:

- no oversized root modules remain in the targeted area
- module names match actual responsibilities

## Test Plan

This refactor must prove behavior did not change.

### Automated checks for every phase

- existing unit tests stay green
- existing integration tests stay green
- no new warnings
- no new `unwrap()` in production paths

### Focused regression tests

Before and after the split, ensure coverage still exercises:

- output dispatch prioritization
- output preparation when no terminal is attached
- hook-signal ingestion
- JSONL status ingestion
- detector maintenance apply path
- task-board reducer behavior
- layout/focus-derived header updates

If code motion breaks test clarity, move tests with the code they validate rather than centralizing more into giant files.

## Manual Validation

Even though this is a no-behavior-change refactor, do the following after the final phase:

- open the app in focus mode and verify the current offload behavior still works
- create, restore, rename, group, and close sessions
- exercise task creation, assignment, review, and completion
- verify hook-capable sessions still update status
- verify generic shell sessions still decay back to idle

## Required Verification Gate

Run the same gate used for the offload phases:

```bash
cargo clean
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features
cargo build --workspace --all-features
cargo test --all --all-targets --all-features
cargo clippy --all --all-targets --all-features -- -D warnings
cargo check -p codirigent-ui --features gpui-full
```

Also run:

```bash
git diff --check
```

## Review Strategy

Use small commits with clear boundaries.

Recommended commit shape:

1. scaffolding only
2. `impl_output_polling` submodule moves
3. `gpui` submodule moves
4. cleanup and naming pass
5. doc updates if needed

Each commit should remain reviewable without mentally reconstructing the entire workspace layer.

## Risks

1. Import churn can hide behavior changes.
2. Private helper moves can accidentally widen visibility.
3. Test motion can make diffs look larger than the logic change.
4. Over-splitting can create a module maze.

Mitigations:

- keep root modules as orchestration entry points
- prefer a few responsibility-based modules over many tiny files
- move code with minimal rewriting
- review diffs with behavior preservation as the first question

## Success Criteria

This plan is successful when:

1. `impl_output_polling.rs` and `gpui.rs` are no longer oversized monoliths.
2. Reviewers can find output-flow logic, status logic, reducer logic, and UI event logic quickly.
3. The verification gate is green with `gpui-full`.
4. No behavior regressions are found in the manual validation pass.

## Follow-On Work

If this split succeeds cleanly, the same pattern can be applied later to:

- `impl_session_lifecycle.rs`
- `settings_panels.rs`
- `task_board_render.rs`
- `drawer_render.rs`

That follow-on work should be planned separately after the primary split lands.
