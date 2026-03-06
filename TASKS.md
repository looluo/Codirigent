# Codirigent Fix Tasks

## Fix --all-features Build Errors (19 errors in codirigent-ui)

- [x] 1. Fix `font_size` field missing on `AppearanceSettings` (9x E0609)
- [x] 2. Fix borrow checker in `impl_task_board.rs` (4x E0502)
- [x] 3. Fix `sync_terminal_dimensions_and_resize` in wrong impl block (2x E0407/E0599)
- [x] 4. Fix `Fill: From<Hsla>` trait bound (1x E0277)
- [x] 5. Fix `Arc<Vec<...>>` not an iterator (2x E0277)
- [x] 6. Fix mismatched types (1x E0308)

## Deep Code Review Iterations

- [~] Round 1: Post-fix review — 32 findings identified

### Pass 1: Dead code removal (F2-F8)
- [ ] F1: `handle_session_menu_action` visibility (CRITICAL)
- [ ] F2: Delete `handle_worktree_event` (never used)
- [ ] F3-F6: Delete unused functions in settings/controls.rs
- [ ] F7: Delete `cursor_position` on Terminal
- [ ] F8: Delete `cells_by_row` and `pixel_size` on TerminalView

### Pass 2: One-line fixes (F9,F11,F18,F20,F26,F27,F29-F32)
- [ ] F9: `!key_char.is_ascii()` instead of `chars().any()`
- [ ] F11: `div_ceil()` instead of manual ceiling division
- [ ] F18: Remove redundant `as isize` cast
- [ ] F20: `and_then(hex_to_hsla)` remove redundant closure
- [ ] F26-F27: Remove `..Default::default()` from TitlebarOptions
- [ ] F29-F32: Remove redundant casts and `.min(255)` in mouse.rs

### Pass 3: Pattern modernisation (F10,F12,F13,F15,F16,F17)
- [ ] F10, F15: Collapse nested if-let patterns
- [ ] F12, F13, F17: Replace `map_or(false, ...)` with `is_some_and()`
- [ ] F16: Remove `unnecessary_to_owned`

### Pass 4: Type aliases (F14,F19,F28)
- [ ] F14: Type alias for complex Vec tuple
- [ ] F19: TerminalPrepaintData struct
- [ ] F28: SplashCallback type alias

### Pass 5: Parameter structs (F21-F25)
- [ ] F21-F22: IconLabelStyle struct for icon_utils.rs
- [ ] F23: PriorityButtonConfig struct
- [ ] F24: Remove `cell_height` from render_split_node
- [ ] F25: Reduce render_session_cell_with_terminal params

- [ ] Round 2: Continue until no more issues found

---
Legend: [ ] pending, [x] done, [~] in progress
