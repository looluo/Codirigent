# Codirigent Fix Tasks

## Fix --all-features Build Errors (19 errors in codirigent-ui)
- [x] Fix `font_size` field missing on `AppearanceSettings` (9x E0609)
- [x] Fix borrow checker in `impl_task_board.rs` (4x E0502)
- [x] Fix `sync_terminal_dimensions_and_resize` in wrong impl block (2x E0407/E0599)
- [x] Fix `Fill: From<Hsla>` trait bound (1x E0277)
- [x] Fix `Arc<Vec<...>>` not an iterator (2x E0277)
- [x] Fix mismatched types (1x E0308)

## Round 1 Passes (all committed)
- [x] Pass 1: Dead code removal (handle_worktree_event, setting_dropdown/number/text/path, cursor_position cfg(test), etc.)
- [x] Pass 2: One-line fixes (is_ascii, div_ceil, redundant cast, is_some_and, wrapping_add, min(255))
- [x] Pass 3: Pattern modernisation (collapsed if-let, is_some_and, to_path_buf)
- [x] Pass 4: Type aliases (SplashCallback, JsonlStatusResult, remove terminal paint type annotation)
- [x] Pass 5: Parameter reduction (render_session_cell_with_terminal: 9→5 params, remove render_split_node cell_height, clippy allows)

## Round 2: 25 Findings

### HIGH
- [ ] H-1: gpui.rs ~sync_ui_state — O(4n) four-pass task counting → single fold
- [ ] H-2: terminal_view.rs — CachedTerminalContent stores raw `background_rects`+`text_runs` never read after build → remove raw fields, use into_iter
- [ ] H-3: impl_task_board.rs:233 — find_assignable_session_for_task clones all sessions → return reference
- [ ] H-4: grid_render.rs — render_split_node re-converts theme colors on every recursive call → pre-compute once
- [ ] H-5: gpui.rs:741 — apply_terminal_font_family clones String per terminal → SharedString or final move

### MEDIUM
- [ ] M-1: terminal_view.rs:455 — visible_cells() Vec without capacity hint
- [ ] M-2: render.rs:282 — render_session_menu clones sessions to find index → direct .position()
- [ ] M-3: terminal_view.rs:780 — build_cached_content clones TextRunSegment → into_iter after H-2
- [ ] M-4: gpui.rs:447 — sync_ui_state clones session.name/group every 100ms → dirty check
- [ ] M-5: terminal_view.rs:128+ — broad pub visibility on TerminalView methods → pub(crate)
- [ ] M-6: terminal_view.rs:57 — TextRunSegment fields pub → pub(crate)
- [ ] M-7: impl_session_lifecycle.rs:56 — configured_shell() reads disk on each session create → cache
- [ ] M-8: terminal_view.rs:628 — end_selection() is no-op → remove or document
- [ ] M-9: terminal_view.rs:83 — CachedTerminalContent derives Clone unnecessarily → remove
- [ ] M-10: impl_output_polling.rs:104 — multiple sequential mutex acquisitions → combine

### LOW
- [ ] L-1: terminal_view.rs:54 — TERMINAL_LINE_HEIGHT_FACTOR=1.0 multiplication redundant → remove
- [ ] L-2: workspace/types.rs:241+ — pub fields in pub(super) structs → pub(super)
- [ ] L-3: gpui.rs:1306 — selected_text_range clones Copy type → remove .clone()
- [ ] L-4: gpui.rs:1324 — marked_text_range clones Copy type → remove .clone()
- [ ] L-5: terminal_view.rs:126 — Selection::new() duplicates Default → remove, use default()
- [ ] L-6: drawer_render.rs:75 + render.rs:293 — magic number 40.0 → DRAWER_HEADER_HEIGHT const
- [ ] L-7: gpui.rs:1386 — _element_bounds underscore misleading → remove underscore
- [ ] L-8: gpui.rs:1396 — character_index_for_point always returns Some(0) → add TODO comment
- [ ] L-9: impl_task_board.rs:10 — Session import may become unused after H-3
- [ ] L-10: terminal_view.rs:867 — CursorRect width/height duplicate cell dims → consider removing

---
Legend: [ ] pending, [x] done
