# Tab Status Indicator Design

**Date:** 2026-03-17
**Status:** Draft

## Problem

The status dot (Idle/Working/Attention/Ready/Error) only exists in the pane header. When a pane has multiple tabs, there is no way to tell which background session needs attention without switching to it.

## Solution

Move the status indicator from the pane header into each session tab. Users can choose from three visual styles via a setting. NeedsAttention and ResponseReady states flash on background tabs to draw the eye.

## Status States

| State | Color | Animated (background tab) |
|-------|-------|---------------------------|
| Idle | #52525b (gray) | No |
| Working | #f59e0b (amber) | No |
| NeedsAttention | #f43f5e (rose) | Yes — pulse |
| ResponseReady | #22c55e (green) | Yes — pulse |
| Error | #ef4444 (red) | No |

Active tabs show the status indicator but never animate (you're already looking at it).

**Note:** Tab animation rules differ from `StatusIndicator::animated` (which flags Working and NeedsAttention for the pane header). The tab module defines its own animation policy: only NeedsAttention and ResponseReady pulse, and only on background tabs. The existing `StatusIndicator::animated` field is not reused — the tab renderer makes its own decision based on `SessionStatus` and `is_active`.

## Tab Status Styles (Configurable)

A new `tab_status_style` field on `AppearanceSettings`, exposed as a dropdown in the Appearance settings page.

### Dot (default)

Small 8x8 colored circle to the **left** of the session name inside the tab pill. Mirrors VS Code / chat app conventions. Natural left-to-right scanning: see status, then read name.

### Badge

Small 8x8 colored circle to the **right** of the session name. Trailing indicator style, keeps left edge aligned.

### Glow

Subtle status-colored tint on the **entire tab background** (`status_color` at ~15% opacity) with a matching border (~25% opacity). No dot element. Flash is a background pulse rather than a dot pulse.

## Config Changes

### `AppearanceSettings` (codirigent-core, config.rs)

Add field:

```rust
#[serde(default = "default_tab_status_style")]
pub tab_status_style: String,
```

Default: `"dot"`. Valid values: `"dot"`, `"badge"`, `"glow"`.

Follows the existing dropdown pattern used by `theme` in `AppearanceSettings` for the settings UI, and the string-match dispatch pattern used by `cursor_style` in `TerminalSettings` for the rendering code.

## UI Changes

### Remove header status dot

In `pane_header_render.rs`, remove the 8x8 status dot from the pane header (currently around line 62). The header retains all other information (session name, git branch, CLI name, task badge, etc.).

### New file: `tab_status_render.rs`

A new file in the `workspace` module containing:

**Return type:**

```rust
pub struct TabStatusDecoration {
    /// Optional child element (dot/badge circle). None for glow style.
    pub child: Option<gpui::AnyElement>,
    /// Optional background color to apply to the tab container. Used by glow style.
    pub tab_bg: Option<gpui::Hsla>,
    /// Optional border color to apply to the tab container. Used by glow style.
    pub tab_border: Option<gpui::Hsla>,
    /// Whether this tab should pulse (NeedsAttention/ResponseReady on background tabs).
    pub should_pulse: bool,
}
```

**Functions:**

- `render_tab_status(style: &str, status: SessionStatus, is_active: bool) -> TabStatusDecoration`
  - Reads the style string and dispatches to the appropriate renderer
  - Unknown style values fall back to "dot"
  - Sets `should_pulse = true` only for NeedsAttention/ResponseReady when `is_active == false`
- Dot renderer: returns `TabStatusDecoration` with `child` set to a colored 8x8 circle
- Badge renderer: same as dot (caller decides prepend vs append based on style)
- Glow renderer: returns `TabStatusDecoration` with `tab_bg` and `tab_border` set, no `child`

**Animation:** The codebase does not currently use GPUI's animation API anywhere. Animation piggybacks on the **existing maintenance polling loop** (250ms interval) to avoid adding a new background timer:

- Add a `pulse_counter: u8` field to `WorkspaceView`
- Increment it each maintenance poll cycle
- Derive pulse phase: `pulse_counter % 6` — 3 ticks on, 3 ticks off gives ~750ms equal duty cycle
- The render code reads `self.pulse_counter` to choose between full and reduced opacity (1.0 vs 0.4) for pulsing elements
- No new `cx.spawn()`, no new background task — reuses existing infrastructure

### Tab strip rendering changes (pane_header_render.rs)

In `render_pane_tab_strip()`, for each tab:

1. Look up `tab_status_style` via `self.effective_user_settings().appearance.tab_status_style` (always available, in-memory)
2. Look up `SessionStatus` from the session's cached state (already available)
3. Determine `is_active` from the current pane's active session
4. Call `render_tab_status()` from the new module, receiving a `TabStatusDecoration`
5. If `decoration.child` is `Some`: for **dot** style, prepend before the name; for **badge** style, append after the name
6. If `decoration.tab_bg` / `decoration.tab_border` are `Some` (glow style): apply to the tab container div
7. If `decoration.should_pulse`: apply the current pulse phase opacity from the workspace's timer-driven toggle

### Settings page (settings_panels.rs)

Add a dropdown in the **Appearance** section:

- Label: "Tab status style"
- Description: "How session status is shown on tabs"
- Options: `["Dot", "Badge", "Glow"]`
- On change: update `page.user_settings.appearance.tab_status_style` and set `user_save_pending = true`

Follows the existing dropdown pattern used by `theme` in the Appearance section.

## Threading & Performance

- **No new async work in render path**: tab rendering reads cached `SessionStatus` from the session struct and `tab_status_style` from `effective_user_settings()`. Both are in-memory reads — no I/O, no locks, no blocking
- **No new timer**: pulse animation piggybacks on the existing 250ms maintenance polling loop by incrementing a counter. No new `cx.spawn()` or background task
- **No additional render cost**: reading `pulse_counter` is a single integer check. The status color computation is a match on a 5-variant enum. Both are negligible in the render pass
- **Settings wiring**: setting changes set `user_save_pending = true`, which is flushed by the render loop via `maybe_schedule_settings_save()`

## File Changes Summary

| File | Change |
|------|--------|
| `codirigent-core/src/config.rs` | Add `tab_status_style` to `AppearanceSettings` with default |
| `codirigent-ui/src/workspace/tab_status_render.rs` | **New** — status rendering per style + animation |
| `codirigent-ui/src/workspace/gpui.rs` | Add `pulse_counter` field to `WorkspaceView` |
| `codirigent-ui/src/workspace/impl_output_polling/*.rs` | Increment `pulse_counter` in maintenance loop |
| `codirigent-ui/src/workspace/pane_header_render.rs` | Remove header dot, integrate tab status rendering |
| `codirigent-ui/src/workspace/settings_panels.rs` | Add dropdown to Appearance section |
| `codirigent-ui/src/workspace/mod.rs` | Add `mod tab_status_render` |

## Test Coverage

### Config tests (codirigent-core)

- Default value is `"dot"`
- Serialization roundtrip for all 3 variants
- Unknown/invalid value falls back to `"dot"` on deserialization
- Backward compatibility: missing `tab_status_style` field deserializes to `"dot"`

### Tab status rendering tests (codirigent-ui)

- Each style variant (dot/badge/glow) produces the correct element structure for each `SessionStatus`
- Animation flag is set only for NeedsAttention and ResponseReady
- Animation flag is never set for active tabs regardless of status
- Unknown style string falls back to dot behavior

### Settings wiring tests (codirigent-ui)

- Dropdown selection updates `user_settings.appearance.tab_status_style`
- Save is marked pending after selection change
- All 3 dropdown options are present and correctly mapped
