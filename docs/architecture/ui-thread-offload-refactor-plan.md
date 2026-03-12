# UI-Thread Offload Refactor Plan

## Status

In progress. This document defines the planned refactor for the remaining UI-thread-bound session workflow in Codirigent, and now reflects implemented progress through Phase 1.

## Progress Snapshot

| Phase | Status | Notes |
| --- | --- | --- |
| Phase 0: Instrumentation And Baseline | Pending | Planned first in the roadmap, but not yet implemented. |
| Phase 1: PTY Command Queue | Complete | Queue-backed PTY writes/resizes landed with a dedicated worker, tests, and full verification gate. |
| Phase 2: Async Session Bootstrap | Pending | Not started. |
| Phase 3: Terminal Runtime Offload | Pending | Not started. |
| Phase 4: Detector Worker | Pending | Not started. |
| Phase 5: Derived UI State Cleanup | Pending | Not started. |

### Phase 1 Completion Notes

Implemented:

- Added a dedicated PTY I/O worker in `crates/codirigent-session/src/session_io.rs`.
- Moved per-session write and resize operations behind a queue-backed handle in `SessionState`.
- Updated `DefaultSessionManager::send_input()` and `DefaultSessionManager::resize()` to enqueue commands instead of performing synchronous PTY I/O.
- Added worker-level tests for write ordering, contiguous resize coalescing, and shutdown behavior.
- Added a manager-level test that verifies ordered command delivery through the real PTY path.

Additional fixes made while validating the phase:

- Initialized the cached cursor position earlier in `TerminalView` so the all-features UI test suite remains green.
- Fixed terminal-editor detection for Windows-style paths in `editor_detection.rs`.

Verification completed successfully with:

```bash
cargo clean
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features
cargo build --workspace --all-features
cargo test --all --all-targets --all-features
cargo clippy --all --all-targets --all-features -- -D warnings
cargo check -p codirigent-ui --features gpui-full
```

## Problem Statement

Focus mode with a single visible session exposes the current architecture's weakest path:

1. PTY output is drained on a background task.
2. The prepared output is handed back to `WorkspaceView`.
3. `WorkspaceView` applies terminal output on the UI thread.
4. Rendering then rebuilds or reshapes terminal rows on the same thread.
5. The same thread is also responsible for click handling, keyboard handling, layout, and paint submission.

Under sustained output, the UI thread becomes both the terminal state mutator and the renderer. That is the core architectural issue. Poll frequency tuning can change how often the problem appears, but it does not change ownership of the hot path.

## Goals

- Move the remaining terminal/session workflow off the UI thread where practical.
- Make the UI thread responsible only for event dispatch, state application, layout, and paint.
- Preserve visible behavior during the migration.
- Reduce worst-case frame time under sustained PTY output.
- Make focused single-session rendering no worse than multi-session rendering in terms of responsiveness.
- Replace render-time recomputation with mutation-driven derived UI state.

## Non-Goals

- Redesigning the visual terminal renderer.
- Replacing GPUI.
- Rewriting the detector heuristics from scratch.
- Extracting the terminal engine into a new crate in this first pass.
- Perfectly eliminating all background work from `WorkspaceView`; the target is to remove heavy and blocking work from the UI thread, not every mutation.

## Scope

This plan covers the five remaining migration items:

1. Move terminal output application and terminal damage generation off the UI thread.
2. Move PTY writes and PTY resizes off the UI thread.
3. Move session creation and restore PTY spawn flow off the UI thread.
4. Move detector ticking and process-state polling off the UI thread.
5. Remove full `sync_ui_state()` recomputation from the render path.

## Current Hotspots

### 1. Terminal output apply on the UI thread

- `crates/codirigent-ui/src/workspace/impl_output_polling.rs`
  - `apply_prepared_session_output()`
- `crates/codirigent-ui/src/terminal.rs`
  - `Terminal::process_output()`

Current behavior:

- PTY bytes are drained in the background.
- The bytes are sent back into `WorkspaceView::update(...)`.
- `Terminal::process_output()` is called synchronously on the UI thread.
- Status reconciliation and header updates then run on the same thread.

### 2. Terminal row cache and shaping work in the render path

- `crates/codirigent-ui/src/workspace/terminal_render.rs`
  - `render_terminal_content()`
- `crates/codirigent-ui/src/terminal_view.rs`
  - `render_rows()`
  - `shaped_rows()`

Current behavior:

- Rendering can trigger row-cache rebuilds.
- Dirty rows can trigger shape rebuilds.
- In worst-case output, render cost grows with viewport size and damage size.

### 3. PTY writes and resizes called synchronously from UI callbacks

- `crates/codirigent-session/src/manager.rs`
  - `send_input()`
  - `resize()`
- Representative call sites:
  - `crates/codirigent-ui/src/workspace/gpui.rs`
  - `crates/codirigent-ui/src/workspace/impl_output_polling.rs`
  - `crates/codirigent-ui/src/workspace/impl_task_board.rs`

Current behavior:

- Keyboard handlers, deferred-enter processing, VTE response forwarding, task assignment, and layout resize propagation can all call into PTY I/O synchronously.

### 4. Session creation and restore still do PTY spawn on the UI thread

- `crates/codirigent-ui/src/workspace/impl_session_lifecycle.rs`
  - `create_session_inner()`
  - `restore_session_from_plan()`
- `crates/codirigent-session/src/manager.rs`
  - `create_session()`

Current behavior:

- Working directory validation, shell resolution, PTY spawn, session registration, and some restore replay happen synchronously from UI mutation paths.

### 5. Detector maintenance work still runs from the UI polling loop

- `crates/codirigent-ui/src/workspace/impl_output_polling.rs`
  - `poll_maintenance()`
  - `tick_detector_statuses()`
- `crates/codirigent-detector/src/detector.rs`
  - `tick()`

Current behavior:

- Detector ticking, stale cache sweep, and related status reconciliation are still initiated from the UI thread on the maintenance cadence.

### 6. Full UI model recomputation still happens from `render()`

- `crates/codirigent-ui/src/workspace/gpui.rs`
  - `sync_ui_state()`
  - `render()`

Current behavior:

- `render()` still has a fallback path that rebuilds task board snapshots and other derived UI metadata.
- This violates a clean render contract and can turn missed invalidations into visible hitches.

## Existing Background Work To Preserve

The following areas are already backgrounded and should stay that way:

- File tree construction and worktree enumeration.
- App state load/save.
- Settings load/save.
- JSONL readers.
- Hook signal scanning.
- Git refresh work.
- Clipboard preview image processing.

This refactor should not regress those paths by reintroducing UI-thread blocking.

## Target Architecture

The target model is event-driven, snapshot-based, and actor-owned.

### High-Level Ownership

- `SessionManager`
  - Owns session metadata registration.
  - Owns PTY bootstrap.
  - Owns PTY reader and PTY command channel lifecycle.

- `SessionIoWorker`
  - Owns PTY write and resize commands after creation.
  - Serializes PTY mutations.
  - Coalesces resizes.

- `TerminalRuntime`
  - Owns terminal parser state for each live session.
  - Consumes drained PTY bytes off the UI thread.
  - Produces immutable render snapshots or deltas.
  - Emits metadata updates derived from output:
    - shell state
    - cwd changes
    - CLI detection hints
    - output activity markers

- `DetectorWorker`
  - Owns detector ticks and process-state polling.
  - Produces status deltas instead of requiring the UI thread to poll detector state.

- `UiStateReducer`
  - Runs on the UI thread.
  - Applies already-prepared mutations.
  - Maintains derived UI state incrementally.
  - Never performs O(all tasks) or O(all sessions) recomputation inside `render()`.

### Required Data Contracts

The refactor depends on explicit message boundaries.

#### PTY command path

```rust
enum SessionIoCommand {
    Write { bytes: Vec<u8> },
    Resize { rows: u16, cols: u16 },
    Shutdown,
}
```

Requirements:

- `Write` must preserve ordering.
- `Resize` must be latest-wins when multiple resizes queue up during drag.
- Commands must be fire-and-forget from the UI thread.

#### Terminal runtime path

```rust
struct SessionRenderSnapshot {
    session_id: SessionId,
    generation: u64,
    rows: Arc<[RenderRow]>,
    dirty_rows: Arc<[usize]>,
    cursor: Option<CursorSnapshot>,
    viewport: ViewportSnapshot,
}

struct SessionMetadataDelta {
    session_id: SessionId,
    cwd: Option<PathBuf>,
    detected_cli_type: Option<CliType>,
    shell_state: Option<ShellState>,
    has_more_output: bool,
}
```

Requirements:

- Snapshots must be immutable once published.
- The UI should always be able to swap to the latest snapshot without reparsing PTY bytes.
- Snapshot publication must not require holding UI state locks.

#### Detector path

```rust
struct SessionStatusDelta {
    session_id: SessionId,
    detector_status: Option<SessionStatus>,
    idle_time: Option<Duration>,
    stale_cache_candidates: bool,
}
```

Requirements:

- Detector work should return changed sessions only.
- The UI thread should apply targeted status deltas, not scan all sessions each maintenance tick.

## Module-By-Module Change Map

This section maps the planned refactor onto the current code layout so the implementation can be split into reviewable PRs.

| Area | Current modules | Planned changes | Notes |
| --- | --- | --- | --- |
| PTY command queue | `crates/codirigent-session/src/manager.rs` | Add a per-session command sender, worker bootstrap, queue-backed `send_input()`, queue-backed `resize()` | First low-risk slice. |
| Session bootstrap | `crates/codirigent-ui/src/workspace/impl_session_lifecycle.rs`, `crates/codirigent-session/src/manager.rs` | Extract blocking creation into background bootstrap job and completion messages | Needed before full terminal runtime offload. |
| Terminal runtime | `crates/codirigent-ui/src/workspace/impl_output_polling.rs`, `crates/codirigent-ui/src/terminal.rs`, `crates/codirigent-ui/src/terminal_view.rs` | Introduce background terminal runtime ownership and immutable snapshots | Main performance workstream. |
| Render path | `crates/codirigent-ui/src/workspace/terminal_render.rs`, `crates/codirigent-ui/src/terminal_view.rs` | Stop render-triggered state mutation; render committed snapshots only | Final text shaping may remain UI-owned. |
| Detector maintenance | `crates/codirigent-ui/src/workspace/impl_output_polling.rs`, `crates/codirigent-detector/src/detector.rs` | Move detector tick and stale cache sweep into a worker | Can be staged after terminal runtime work. |
| Derived UI state | `crates/codirigent-ui/src/workspace/gpui.rs` | Split `sync_ui_state()` into reducers and remove full recompute from `render()` | Final cleanup phase. |

### Suggested New Modules

- `crates/codirigent-ui/src/workspace/terminal_runtime.rs`
  - background terminal parser ownership
  - snapshot publication
  - generation tracking

- `crates/codirigent-session/src/session_io.rs`
  - PTY command worker
  - write ordering
  - resize coalescing

- `crates/codirigent-ui/src/workspace/session_bootstrap.rs`
  - async session creation and restore bootstrap jobs

- `crates/codirigent-ui/src/workspace/status_worker.rs`
  - detector cadence and status-delta publication

- `crates/codirigent-ui/src/workspace/ui_reducer.rs`
  - derived task-board and session-summary reducers

These names are recommendations, not requirements. The important part is separating worker-owned logic from UI-owned mutation and render logic.

## Thread Ownership After Refactor

### Must stay on the UI thread

- GPUI event handling.
- Focus changes.
- Input routing decisions.
- Layout calculation.
- Final element tree creation.
- Final text shaping if GPUI text APIs remain UI-thread-bound.
- Paint submission.
- Applying already-prepared background deltas to UI-visible state.

### Must move off the UI thread

- PTY spawn and restore bootstrapping.
- PTY writes.
- PTY resizes.
- Terminal output parsing and terminal-state mutation.
- Terminal damage generation.
- Detector ticking and stale-status polling.
- Any session-wide or task-wide derived-state recomputation that does not require GPUI APIs.

### Split ownership

- Terminal rendering data preparation
  - Move terminal-state mutation, damage extraction, and row materialization off-thread.
  - Keep only the final GPUI text shaping step on the UI thread unless GPUI exposes a safe background shaping path.

This split is important. We should not block this refactor on moving text shaping off-thread if the framework does not support it.

## Workstream 1: Move Terminal Output Apply Off The UI Thread

### Current Entry Points

- `WorkspaceView::apply_prepared_session_output()`
- `Terminal::process_output()`
- `TerminalView::render_rows()`

### Target End State

- PTY bytes are parsed by a background `TerminalRuntime`.
- `WorkspaceView` receives ready-to-apply snapshots and metadata deltas.
- The UI thread no longer calls `Terminal::process_output()`.
- `TerminalView` becomes a lightweight snapshot holder plus view-specific caches.

### Design

Introduce one `TerminalRuntimeHandle` per session.

Responsibilities:

- Consume PTY bytes from the current prepared-output path.
- Apply `Terminal::process_output()` off-thread.
- Track terminal damage.
- Materialize render rows or row deltas.
- Emit `SessionRenderSnapshot` plus `SessionMetadataDelta`.

Initial implementation should reuse the current output preparation pipeline rather than rewriting session output delivery and terminal rendering in one step. The migration should change the owner of `process_output()`, not the entire output transport in phase one.

### Detailed Steps

1. Add a new runtime module in `codirigent-ui` for terminal background work.
2. Move the mutable terminal parser state out of `TerminalView`.
3. Convert `TerminalView` into:
   - latest committed snapshot
   - font/theme settings
   - UI-only caches
   - cursor/IME view data
4. Replace `apply_prepared_session_output()` so it publishes drained bytes to the session runtime instead of mutating terminal state directly.
5. Add a UI-facing channel for `SessionRenderSnapshot` and `SessionMetadataDelta`.
6. Apply snapshots inside `WorkspaceView::update(...)` with no parsing work.
7. Remove `Terminal::process_output()` from all UI-thread code paths.

### Damage Strategy

- Use damage ranges or dirty row lists from the terminal runtime.
- Publish full snapshots only as a fallback.
- Prefer latest-snapshot-wins semantics for rendering.
- Intermediate paint requests may be dropped; terminal state must never be dropped.

### Acceptance Criteria

- No call to `Terminal::process_output()` from any `WorkspaceView` UI update path.
- Focused single-session mode remains interactive under sustained PTY output.
- Snapshot application on the UI thread is bounded and does not parse bytes.
- Existing session status behavior still updates correctly.

### Risks

- Snapshot payloads can become too large if they clone entire viewports too often.
- TerminalView may still do too much UI-side shaping work if snapshots are too raw.
- Output-derived metadata must remain ordered relative to rendered content.

### Risk Mitigations

- Start with dirty-row snapshots.
- Use generation counters to discard stale updates.
- Benchmark snapshot size and clone frequency before widening the contract.

## Workstream 2: Move PTY Writes And Resizes Off The UI Thread

### Current Entry Points

- `DefaultSessionManager::send_input()`
- `DefaultSessionManager::resize()`
- UI call sites in keyboard handlers, deferred-enter handling, task assignment, VTE response forwarding, and resize sync.

### Target End State

- The UI thread enqueues PTY commands and returns immediately.
- A per-session `SessionIoWorker` owns PTY writes and resizes.
- PTY command ordering is explicit.

### Design

At session creation time, split PTY responsibilities:

- PTY output reader remains background-owned.
- PTY write/resize path moves behind a `SessionIoCommand` sender.

`SessionState` should retain:

- session metadata
- child pid
- command sender
- output reader handle metadata

It should no longer require a UI-thread caller to lock the manager and directly touch the PTY for every input event.

### Detailed Steps

1. Add a per-session command channel when the PTY is created.
2. Spawn a blocking worker that owns the PTY write/resize operations.
3. Change `send_input()` to enqueue `SessionIoCommand::Write`.
4. Change `resize()` to enqueue `SessionIoCommand::Resize`.
5. Add resize coalescing in the worker:
   - collapse multiple queued resizes into the last seen size
   - avoid replaying obsolete geometry during window drags
6. Audit all synchronous PTY call sites and convert them to the queue-based API.
7. Add observability:
   - command queue depth
   - resize collapse count
   - write failures

### Acceptance Criteria

- No UI handler directly performs PTY I/O.
- Window drag/resize does not trigger synchronous PTY calls on the UI thread.
- Keyboard and VTE response paths remain ordered and correct.

### Risks

- Write ordering bugs can corrupt terminal interaction.
- Unbounded queues can hide overload until memory grows.

### Risk Mitigations

- Preserve per-session ordering with one queue per session.
- Consider bounded queues with explicit logging if pressure appears.
- Keep resize latest-wins while preserving write ordering.

## Workstream 3: Move Session Creation And Restore Off The UI Thread

### Current Entry Points

- `WorkspaceView::create_session_inner()`
- `WorkspaceView::restore_session_from_plan()`
- `DefaultSessionManager::create_session()`

### Target End State

- Session creation becomes a background bootstrap job.
- The UI thread only initiates creation and applies the result.
- Restore queues multiple background bootstrap jobs without blocking render.

### Design

Introduce a `SessionBootstrapJob` with a completion message:

```rust
struct SessionBootstrapResult {
    session_id: SessionId,
    session: Session,
    child_pid: Option<u32>,
    io_sender: SessionIoSender,
    output_handle: SessionOutputHandle,
    pending_restore_commands: Vec<Vec<u8>>,
}
```

The UI thread should be able to:

- reserve a slot or pending placeholder
- kick off the job
- receive success or failure
- attach the finished session to workspace state

### Detailed Steps

1. Extract synchronous creation logic from `create_session_inner()` into a background bootstrap function.
2. Validate working directory and resolve shell in the bootstrap job.
3. Spawn the PTY and IO worker in the bootstrap job.
4. Return session metadata and handles to the UI thread.
5. Only after success:
   - create the `TerminalRuntime`
   - attach the session to the workspace
   - start detector monitoring
   - replay resume commands through the command queue
6. For restore:
   - keep restore plans immutable
   - queue one bootstrap job per saved session
   - attach sessions incrementally as results arrive
7. Add a visible pending state so the UI does not look frozen during slow shell startup.

### Acceptance Criteria

- Creating or restoring sessions never blocks `render()` or input handlers.
- Slow shell startup or bad working directory validation does not freeze the UI.
- Restore replay preserves ordering of resume commands after the PTY is ready.

### Risks

- Session slot assignment can race with late-arriving bootstrap results.
- Failed creation jobs can leave orphaned placeholder state.

### Risk Mitigations

- Use a creation token per pending session slot.
- Only commit bootstrap results if the token still matches.
- Define explicit cleanup for failed bootstrap jobs.

## Workstream 4: Move Detector Tick And Process-State Polling Off The UI Thread

### Current Entry Points

- `WorkspaceView::poll_maintenance()`
- `WorkspaceView::tick_detector_statuses()`
- `Detector::tick()`

### Target End State

- Detector maintenance runs independently of UI frame work.
- The UI thread receives targeted `SessionStatusDelta` messages.
- Stale cache reconciliation no longer requires a UI-driven sweep across session caches.

### Design

Introduce a dedicated maintenance worker that owns:

- detector tick cadence
- stale cache review cadence
- per-session process-state updates

The worker should emit:

- changed detector status
- idle-time updates when needed
- stale-cache reconciliation requests or already-reconciled status results

The cleanest version is to move both detector ticking and status reconciliation into the worker and send a final `SessionStatusPatch` to the UI thread. If that is too large for phase one, move detector ticking first and keep UI-side targeted reconcile as an intermediate state.

### Detailed Steps

1. Create a background maintenance task that ticks the detector at the current cadence.
2. Return changed session IDs and any needed status metadata over a channel.
3. Move stale cached-status sweep out of the UI path.
4. Decide the final ownership of `status_engine::reconcile()`:
   - preferred: background worker
   - acceptable intermediate step: UI applies only the changed patches
5. Keep notifications, task transitions, and UI header refreshes on the UI thread, but only as a reaction to precomputed status changes.
6. Remove detector polling from `poll_maintenance()`.

### Acceptance Criteria

- `poll_maintenance()` no longer performs detector tick work on the UI thread.
- Idle-to-working and working-to-idle transitions still behave correctly.
- Sessions without OSC integration still decay back to idle correctly.

### Risks

- Status ordering bugs can produce flicker or stale headers.
- Split ownership between detector and reconciler can create temporary inconsistency.

### Risk Mitigations

- Include sequence numbers or timestamps in status deltas.
- Prefer moving full reconciliation with the detector when feasible.

## Workstream 5: Remove `sync_ui_state()` From The Render Path

### Current Entry Points

- `WorkspaceView::render()`
- `WorkspaceView::sync_ui_state()`

### Target End State

- `render()` becomes read-only with respect to derived UI state.
- Task board state, counts, pending assignments, and similar aggregates are rebuilt on mutation, not as a fallback in the hot render path.

### Design

Introduce a dedicated `UiDerivedState` struct that is updated incrementally from mutation sources:

- task manager changes
- session header changes
- layout changes
- workspace session add/remove
- settings changes

`render()` should only:

- read the already-prepared state
- update layout-dependent caches that strictly require current window metrics
- build the element tree

### Detailed Steps

1. Split `sync_ui_state()` into smaller reducers:
   - task-board reducer
   - session-list reducer
   - layout-derived reducer
2. Identify the mutation sources that currently rely on fallback sync.
3. Trigger the appropriate reducer directly from those mutation paths.
4. Replace the `render()` fallback sync with debug assertions or lightweight diagnostics.
5. Keep a temporary kill-switch during rollout in case an invalidation path is missed.
6. Once stable, remove the fallback interval entirely.

### Acceptance Criteria

- `render()` no longer calls a full derived-state recomputation function.
- Task board and session metadata remain correct after all existing mutation paths.
- Missed invalidations can be detected in debug builds.

### Risks

- Missing invalidation hooks can leave stale UI sections.
- Partial reducers can drift apart if ownership is unclear.

### Risk Mitigations

- Add explicit reducer tests.
- Use narrow reducer APIs with clear callers.
- Keep temporary diagnostics that compare reducer output against old full recompute in debug mode.

## Recommended Rollout Order

The five workstreams are coupled. The order below minimizes architectural churn.

### Phase 0: Instrumentation And Baseline

Before behavior changes:

- add tracing spans around:
  - output apply
  - render_terminal_content
  - shaped_rows
  - send_input
  - resize
  - create_session
  - tick_detector_statuses
  - sync_ui_state
- capture frame time and input latency under a synthetic high-output session
- record baseline CPU use in single-session focus mode

Deliverable:

- reproducible before/after benchmark script or manual test recipe

### Phase 1: PTY Command Queue

Do Workstream 2 first.

Why first:

- It is self-contained.
- It removes synchronous PTY writes/resizes from multiple hot input paths.
- It provides the command infrastructure needed by async session creation and restore.

### Phase 2: Async Session Bootstrap

Do Workstream 3 second.

Why second:

- It builds on the PTY command queue.
- It removes another blocking class from UI mutation paths.
- It creates the right lifecycle hook for the terminal runtime.

### Phase 3: Terminal Runtime Offload

Do Workstream 1 third.

Why third:

- It is the biggest performance win.
- It depends on stable session bootstrap and PTY ownership boundaries.
- It changes the data flow between session output and rendering.

### Phase 4: Detector Worker

Do Workstream 4 fourth.

Why fourth:

- It is logically separate once session runtime ownership is clear.
- It simplifies maintenance polling after the terminal path is no longer UI-bound.

### Phase 5: Derived UI State Cleanup

Do Workstream 5 last.

Why last:

- It benefits from the new event boundaries created in earlier phases.
- It should be done after background result application paths are stable.

## Per-Phase Implementation Verification

Each phase needs its own implementation test plan. A phase is not complete when the code compiles; it is complete when the new ownership boundary is tested directly, the affected UX path is manually verified, and the full repo verification gate passes.

### Phase 0: Instrumentation And Baseline

Implementation checks:

- Confirm new tracing spans compile and are emitted in debug logs.
- Verify baseline metrics can be collected for:
  - frame duration
  - terminal render duration
  - output apply duration
  - detector tick duration

Manual checks:

- Record a reproducible single-session focus-mode high-output scenario.
- Record a reproducible session-create / restore scenario.
- Record a reproducible window-drag / terminal-resize scenario.

Phase exit criteria:

- Baseline numbers are written down and can be compared after each later phase.
- There is a repeatable manual test recipe for every hot path being refactored.

### Phase 1: PTY Command Queue

Implementation tests:

- Unit tests for per-session write ordering.
- Unit tests for resize coalescing with interleaved writes.
- Unit tests for worker shutdown and channel teardown.
- Unit tests for command send failure behavior after session close.

Integration tests:

- Session input path still reaches the PTY in order.
- Deferred-enter and VTE response forwarding still reach the PTY in order.
- Resize storms collapse to the latest geometry without dropping writes.

Manual checks:

- Type continuously into an active session while output is streaming.
- Drag-resize the window and verify terminal size keeps up without UI stalls.
- Trigger task assignment and confirm the queued task prompt still arrives correctly.

Phase exit criteria:

- No direct synchronous PTY write/resize path remains in UI handlers.
- Input ordering is preserved.
- Resize behavior is stable under rapid window drag.

### Phase 2: Async Session Bootstrap

Implementation tests:

- Unit tests for successful bootstrap result assembly.
- Unit tests for invalid working directory failure.
- Unit tests for shell resolution failure and cleanup.
- Unit tests for placeholder token matching and stale result rejection.

Integration tests:

- Create-session path attaches a live session after background bootstrap succeeds.
- Restore-session path replays resume commands after PTY readiness.
- Failed bootstrap does not leave orphaned workspace/session state.

Manual checks:

- Create a new session while another session is producing heavy output.
- Restore multiple sessions and verify the UI remains responsive while they appear incrementally.
- Test a deliberately bad working directory and confirm the UI reports failure without freezing.

Phase exit criteria:

- Session creation and restore do not block UI interaction.
- Placeholder and cleanup behavior is correct on both success and failure paths.

### Phase 3: Terminal Runtime Offload

Implementation tests:

- Unit tests for output chunk ingest into the background runtime.
- Unit tests for damage generation and dirty-row snapshot publication.
- Unit tests for snapshot generation ordering and stale-generation drop behavior.
- Unit tests for metadata extraction ordering relative to rendered content.

Integration tests:

- High-output sessions produce snapshots without calling `Terminal::process_output()` from the UI path.
- CLI detection, cwd updates, and shell-state updates still arrive correctly.
- Focused session and non-focused session output both render correctly.

Manual checks:

- Run a high-output producer in single-session focus mode and verify clicks and typing remain responsive.
- Switch focus between noisy and quiet sessions and verify no stale frame artifacts.
- Confirm terminal selection, cursor rendering, and IME behavior still work.

Phase exit criteria:

- PTY output parsing is off the UI thread.
- UI-side snapshot application is bounded and lightweight.
- The original freeze symptom is materially reduced or eliminated in the baseline scenario.

### Phase 4: Detector Worker

Implementation tests:

- Unit tests for detector tick result publication.
- Unit tests for sessions without OSC integration returning to idle.
- Unit tests for stale cached-status sweep behavior.
- Unit tests for out-of-order or duplicate detector events.

Integration tests:

- Detector changes reach session headers and status caches correctly.
- Notifications and task state changes still trigger from detector-driven transitions.
- Hook-driven sessions still prefer hook status over detector fallbacks.

Manual checks:

- Exercise generic shell sessions that rely on detector decay.
- Exercise hook-capable sessions and confirm there is no status regression.
- Leave sessions idle long enough to verify stale-state cleanup behavior.

Phase exit criteria:

- `poll_maintenance()` no longer performs detector tick work on the UI thread.
- Detector-driven transitions remain correct across supported session types.

### Phase 5: Derived UI State Cleanup

Implementation tests:

- Unit tests for task-board reducer updates across all task-status transitions.
- Unit tests for session-summary reducer updates on add/remove/focus/group changes.
- Unit tests for layout-derived state invalidation.
- Debug-only parity checks between reducer output and legacy full recompute while the fallback still exists.

Integration tests:

- Task board remains correct during rapid session and task updates.
- Session headers, counts, and pending-assignment state stay synchronized.
- No mutation path depends on `render()` to repair stale UI state.

Manual checks:

- Exercise task creation, assignment, verification, and completion flows.
- Rapidly switch layouts and focus while sessions update in the background.
- Verify no stale sidebar or task-board state appears after large batches of updates.

Phase exit criteria:

- `render()` is read-only with respect to full derived UI state.
- The fallback recompute path is removed or reduced to debug-only diagnostics.

## Required Verification Gate After Each Phase

Every implementation phase must end with a full local verification pass. The fast targeted tests above are not enough by themselves.

Required commands:

```bash
cargo clean
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features
cargo build --workspace --all-features
cargo test --all --all-targets --all-features
cargo clippy --all --all-targets --all-features -- -D warnings
```

Additional CI-parity check:

```bash
cargo check -p codirigent-ui --features gpui-full
```

Notes:

- `cargo fmt --all -- --check` is the formatting gate. This repo does not have a separate lint tool beyond formatting and clippy.
- `cargo clippy --all --all-targets --all-features -- -D warnings` is the local lint gate.
- `cargo clean` is required at phase completion, not just at the very end of the entire refactor.
- If local platform constraints prevent full cross-platform verification, the local gate must still pass on the active platform and CI must be allowed to validate the Windows/macOS matrix.

## Detailed Validation Plan

### Automated Tests

- Session creation tests
  - async bootstrap success
  - invalid working directory failure
  - restore replay order

- PTY command queue tests
  - write ordering
  - resize coalescing
  - shutdown cleanup

- Terminal runtime tests
  - output parsing produces correct row snapshots
  - dirty-row updates do not require full snapshot rebuild
  - stale generation snapshots are dropped

- Detector tests
  - sessions without OSC integration return to idle
  - stale cached status transitions still occur
  - out-of-order detector events do not regress state

- UI reducer tests
  - task-board counts update on each task status transition
  - session list updates on add/remove/focus/group changes
  - no render-path recomputation dependency remains

### Manual Verification

- Focus mode, single session, high-output producer
  - clicks remain responsive
  - keyboard input remains responsive
  - render remains visually correct

- Multi-session fairness
  - one noisy session does not starve others

- Window drag/resize
  - no visible stalls
  - terminal size catches up correctly

- Session restore
  - many-session restore does not freeze the app

- Codex / Claude / generic shell sessions
  - status transitions still behave as expected
  - cwd and git refresh still update correctly

### Instrumentation Metrics

- UI thread frame duration percentile
- time spent applying output on UI thread
- time spent inside `render_terminal_content`
- terminal snapshot publish rate
- terminal snapshot size
- PTY command queue depth
- detector tick duration
- derived UI reducer duration

## Migration Invariants

These invariants must hold throughout the refactor:

1. Terminal data ordering per session must remain correct.
2. PTY write ordering per session must remain correct.
3. A session must not accept output updates after shutdown.
4. Resize commands may collapse, but writes may not reorder around each other.
5. UI state must only display committed session snapshots.
6. Status updates must remain monotonic with respect to the latest known event timestamp or generation.

## Open Questions

1. Can GPUI text shaping be safely performed off-thread, or must shaped rows remain UI-owned for now?
2. Should `Terminal` remain in `codirigent-ui`, or is a later extraction into a non-UI crate desirable after this refactor?
3. Should status reconciliation move fully into the detector worker, or is a staged split acceptable long term?
4. Do we want bounded PTY command queues with backpressure, or unbounded queues with diagnostics in the first pass?
5. Should session bootstrap create a visible placeholder pane immediately, or only attach the pane after PTY creation succeeds?

## Recommended First Implementation Slice

The first implementation PR should not attempt all five workstreams at once.

Recommended first slice:

1. Add instrumentation.
2. Add the per-session PTY command queue and worker.
3. Convert `send_input()` and `resize()` to queued commands.
4. Land tests for ordering and resize coalescing.

This slice is low-risk, directly reduces UI-thread blocking, and creates the foundation needed for the remaining four workstreams.
