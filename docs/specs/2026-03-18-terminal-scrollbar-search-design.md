# Terminal Scrollbar & Search Design

**Date:** 2026-03-18
**Status:** Approved

## Overview

Add two features to the terminal pane:
1. An interactive scrollbar with drag-to-scroll, click-to-jump, and auto-hide
2. A find-in-terminal overlay (Cmd+F / Ctrl+F) with match highlighting, navigation, and scrollbar match markers

## Prerequisite APIs

### Total Scrollback Lines

The scrollbar and search both need to know the total scrollback size. Alacritty's `Term` provides this via `grid().total_lines() - grid().screen_lines()` (history size) and `topmost_line().0.unsigned_abs()` (max scroll offset). Currently `TerminalSize::total_lines()` in `terminal.rs` returns only the visible row count.

**Changes required:**
- Add a `history_size: usize` field to `TerminalRenderSnapshot` in `terminal_runtime.rs`, populated from `term.topmost_line().0.unsigned_abs()` during snapshot generation
- Add `total_scrollback_lines() -> usize` to `TerminalView`, reading from the snapshot
- The scrollbar uses this value for thumb sizing and position math

### Scroll-to-Absolute-Position

The scrollbar drag and track-click need to set the viewport to an arbitrary position. Only relative scroll APIs exist today (`scroll_up`, `scroll_down`, `scroll_to_bottom`).

**Approach:** Compute a delta from the current `display_offset` to the target offset using `i32` arithmetic to avoid underflow: `Scroll::Delta((target as i32) - (current_display_offset as i32))`. Alacritty clamps the result to `[0, history_size]`. Add a `scroll_to_offset(target: usize)` method on `TerminalRuntimeHandle` that performs this computation internally.

### Search Grid Access

The search engine needs to iterate the `Term` grid cells. The `Term` is owned by `TerminalRuntime` behind `Arc<Mutex<>>`.

**Approach:** Add a `search(query: &str) -> Vec<SearchMatch>` method on `TerminalRuntimeHandle` that acquires the mutex lock and runs the scan, consistent with how `get_selected_text()` already works. The debounce timer resets on each keystroke so only the final query triggers a scan. If large-scrollback performance becomes an issue, this can be moved to a background task later.

## Feature 1: Interactive Scrollbar

### Rendering

Overlay div on the right edge of the terminal pane, rendered as a sibling of the terminal canvas inside `grid_render.rs`. Positioned absolute, sits on top of terminal content.

- **Track:** Full-height div, transparent by default, semi-transparent on hover
- **Thumb:** Colored div inside the track
  - Height: `max(30px, (visible_rows / total_lines) * track_height)`
  - Position: proportional to `display_offset / total_scrollback_lines`
- **Width:** 8px default, expands to 12px on hover

### Interaction

- **Drag thumb:** `on_mouse_down` on thumb captures drag start offset. `on_mouse_move` on track converts pixel Y delta to proportional scrollback position. `on_mouse_up` releases.
- **Click track:** Jump to proportional position — `(click_y / track_height) * total_scrollback_lines`
- **Mouse wheel:** Existing handler unchanged. Thumb position updates reactively from `display_offset`.

### Auto-Hide

- Default opacity: 0 (hidden)
- Fade in on: mouse enters terminal area, scroll wheel activity, scrollback position changes
- Fade out after: 1.5s of no scroll activity AND mouse not hovering the scrollbar
- While mouse hovers the scrollbar: stay visible, expand width
- Timer: use `cx.spawn()` with `Timer::after(Duration::from_millis(1500))` to schedule fade-out; cancel and restart on any scroll activity or hover. Update opacity via `cx.notify()`.
- The scrollbar track height accounts for `TERMINAL_CONTENT_PADDING` so the thumb range matches the visible content area.

### State

```rust
struct ScrollbarState {
    /// Current opacity (0.0 = hidden, 1.0 = fully visible).
    opacity: f32,
    /// Mouse is hovering the scrollbar track or thumb.
    hovered: bool,
    /// Active drag: stores Y offset from thumb top at drag start.
    dragging: Option<f32>,
    /// Timestamp of last scroll activity (for auto-hide timer).
    last_scroll_activity: Instant,
}
```

## Feature 2: Terminal Search

### Search Overlay

Floating bar at the top-right of the terminal pane, approximately 300px wide. Contains:
- Text input field (focused on open)
- Match count label: "3 of 47"
- Prev/Next buttons (up/down arrow icons, or keyboard Enter/Shift+Enter)
- Close button (X) or Escape to dismiss

Rendered as an absolute-positioned div inside the terminal pane container.

### Activation

- **Open:** Cmd+F (macOS) / Ctrl+F (Windows/Linux) — registers a `SearchTerminal` GPUI action
- **Close:** Escape key or click X — clears highlights and returns focus to terminal
- **Input routing:** While search bar is open, keystrokes go to the search input, not the terminal PTY

### Search Engine

Location: `crates/codirigent-ui/src/terminal_search.rs`

- Scans alacritty `Term` grid from bottom of scrollback to top (most recent content first)
- Iterates cells row by row, concatenating characters into line strings
- Handles wrapped lines as a single logical line
- Case-insensitive matching
- Returns match positions:

```rust
struct SearchMatch {
    /// Absolute grid line coordinate (matches alacritty's Line(i32) convention).
    /// Negative = scrollback history, 0 = top of visible screen, positive = below.
    /// This is independent of display_offset — the viewport maps absolute lines
    /// to screen rows. Scrollbar marker positions are computed from these absolute
    /// coordinates relative to the total scrollback range.
    grid_line: i32,
    /// Start column (inclusive).
    start_col: usize,
    /// End column (exclusive).
    end_col: usize,
}
```

- Wrapped lines detected via alacritty's `WRAPLINE` cell flag — consecutive flagged rows are concatenated into a single logical line for matching, with column offsets adjusted accordingly
- Debounce: 150ms, timer resets on each keystroke so only the final query triggers a scan; search runs synchronously under the `TerminalRuntimeHandle` mutex (consistent with `get_selected_text()`)
- **Output during search:** When new terminal output arrives while search is active, matches are kept as-is (stale) until the user modifies the query. Match indices may shift due to new output; if the user navigates to a match whose text no longer matches the query at that position, skip to the next valid match. This avoids re-scanning on every output event.

### Match Highlighting

- All matches: colored background rects rendered during terminal paint phase (same layer as selection rects in `terminal_render.rs`)
- Active/current match: brighter highlight color to distinguish from other matches
- Only matches within the current viewport need rects computed — filter by visible row range during render

### Navigation

- Enter or Down arrow: jump to next match
- Shift+Enter or Up arrow: jump to previous match
- Jumping scrolls the viewport to center the match on screen
- Match index wraps around (last match → first match)

### Scrollbar Match Markers

- While search is active, render small horizontal ticks on the scrollbar track
- Each tick: 2px tall, full scrollbar width, positioned at proportional Y for the match's grid line
- Uses the search highlight color
- Only visible while search overlay is open
- Marker Y position formula: `marker_y_fraction = (history_size + grid_line) / (history_size + screen_lines)` — this maps absolute grid line coordinates to the same proportional space used by the scrollbar thumb

### Search State

```rust
struct SearchState {
    /// Whether the search overlay is open.
    active: bool,
    /// Current search query.
    query: String,
    /// All matches found in the terminal grid.
    matches: Vec<SearchMatch>,
    /// Index of the currently focused match (for navigation).
    current_match: Option<usize>,
}
```

## File Organization

### New Files

| File | Purpose |
|------|---------|
| `crates/codirigent-ui/src/workspace/scrollbar_render.rs` | Scrollbar rendering helper called from within the session cell render path in `grid_render.rs`; not a standalone workspace component |
| `crates/codirigent-ui/src/workspace/search_render.rs` | Search overlay: text input, match count, prev/next buttons |
| `crates/codirigent-ui/src/terminal_search.rs` | Search engine: grid scanning, match collection, result types |

### Modified Files

| File | Change |
|------|--------|
| `terminal_view.rs` | Add `ScrollbarState`, `SearchState` fields; expose `total_scrollback_lines()` |
| `grid_render.rs` | Compose scrollbar and search overlay into terminal pane div; wire Cmd+F |
| `terminal_render.rs` | Render search match highlight rects during paint phase |
| `app.rs` | Define `SearchTerminal` action struct and register keybinding (Cmd+F / Ctrl+F), alongside existing actions like `Copy`, `Paste` |
| `workspace/mod.rs` | Declare new modules |

### Modified (minimal)

- `terminal_runtime.rs` — add `history_size` to snapshot, add `scroll_to_offset()` and `search()` methods on handle

### Unchanged

- `terminal.rs` — alacritty wrapper stays untouched
- Session management, persistence, layout systems
- Existing mouse scroll and text selection behavior (selection continues to work normally beneath the search overlay; search matches are purely visual and do not interact with the selection system)
