# Codirigent Fix Tasks

## Rounds 1-4: All Complete

## Round 5: 29 Findings

### CRITICAL
- [x] C1: requeue_task has no status guard (queue.rs)
- [x] C2: start_task silently ignores Assigned→Working transition (task_manager.rs)

### HIGH
- [x] H1: on_task_complete uses stale clone for retry count (task_manager.rs)
- [x] H2: ContextTrackerSettings thresholds not validated (config.rs)
- [x] H3: idle_threshold_seconds ignored in on_session_idle (assignment.rs)
- [x] H4: pending Vec unbounded in AssignmentManager (assignment.rs)
- [x] H5: TaskAssigned event published before confirmation (assignment.rs)
- [x] H6: update_blocked_status redundant full rescan (queue.rs)

### MEDIUM
- [ ] M1: done_ids() allocates Vec per idle poll — remove completed_tasks param, build done set internally in next_task
- [x] M2: FIFO scoring O(n²) in selection.rs
- [x] M3: age_score hard cap at 60 minutes
- [x] M4: Ctrl+B maps to toggle_sidebar instead of toggle_task_board (gpui.rs)
- [ ] M5: Ctrl+K / Ctrl+Shift+P shortcuts unhandled (gpui.rs) — add handlers
- [ ] M6: line_height setting not applied — add terminal_line_height to theme, pass to compute_cell_dimensions
- [ ] M7: color_scheme setting dead config field — remove from TerminalSettings
- [x] M8: format_binding always shows "Cmd" on non-macOS (keybindings.rs)
- [ ] M9: Hardcoded 36px row height in render_session_menu (render.rs) — extract to constant
- [x] M10: from_config silently drops invalid bindings (keybindings.rs)
- [x] M11: ToggleSidebar has no default keybinding

### LOW
- [x] L1: action_from_name missing set_layout/send_input round-trip (keybindings.rs)
- [x] L2: TerminalView::new rebinds terminal unnecessarily
- [ ] L3: DefaultEventBus::new(0) has no explicit assert — add assert!(capacity > 0)
- [ ] L4: max_concurrent u32 vs usize inconsistency — SKIP (low benefit, high change cost)
- [ ] L5: Dropdown backdrop hardcoded 9999px (settings_panels.rs) — replace with inset_0()
- [ ] L6: default_shell empty string sentinel — SKIP (well-documented pattern, filter() handles it)
- [x] L7: sync_ui_state always runs — already throttled at 100ms (line 1414 in gpui.rs)
- [ ] L8: process_deferred_enters creates two Vecs — combine into single pass
- [x] L9: _completed_tasks dead parameter in calculate_score

---
## Remaining Round 5 Work

### In progress batch (not yet committed):
All C/H/M2-M4/M8/M10-M11/L1-L2/L7/L9 items applied to files but not committed.

### Still to implement:
1. L3 - event_bus.rs: assert!(capacity > 0)
2. L5 - settings_panels.rs: replace 9999px backdrop with inset_0()
3. M9 - render.rs: extract 36px to SESSION_ROW_HEIGHT constant
4. L8 - impl_output_polling.rs: single-pass process_deferred_enters
5. M5 - gpui.rs: Ctrl+K → toggle_drawer; Ctrl+Shift+P → open_task_creation_modal
6. M6 - theme.rs + terminal_view.rs + gpui.rs: apply line_height to cell height
7. M7 - config.rs: remove dead color_scheme field from TerminalSettings
8. M1 - selection.rs + assignment.rs + task_manager.rs: remove completed_tasks param

### After all fixes: cargo fmt, cargo check, cargo test, commit, start Round 6 review

---
Legend: [ ] pending, [x] done
