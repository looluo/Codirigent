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

Follows the existing pattern used by `cursor_style` (string field + match in rendering code).

## UI Changes

### Remove header status dot

In `pane_header_render.rs`, remove the 8x8 status dot from the pane header (currently around line 62). The header retains all other information (session name, git branch, CLI name, task badge, etc.).

### New file: `tab_status_render.rs`

A new file in the `workspace` module containing:

- `render_tab_status_indicator(style: &str, status: SessionStatus, is_active: bool) -> impl IntoElement`
  - Reads the style string and dispatches to the appropriate renderer
  - Unknown style values fall back to "dot"
- Dot renderer: returns a colored 8x8 circle element
- Badge renderer: returns a colored 8x8 circle element (same as dot, just positioned differently by the caller)
- Glow renderer: returns background color + border styling to apply to the tab container
- Animation wrapper: for NeedsAttention/ResponseReady on non-active tabs, wraps the element with GPUI's `with_animation` to pulse opacity between 0.4 and 1.0 on a 1.5s ease-in-out cycle

### Tab strip rendering changes (pane_header_render.rs)

In `render_pane_tab_strip()`, for each tab:

1. Look up `tab_status_style` from user settings (in-memory, no async)
2. Look up `SessionStatus` from the session's cached state (already available)
3. Determine `is_active` from the current pane's active session
4. Call `render_tab_status_indicator()` from the new module
5. For **dot** style: prepend the returned element before the name text
6. For **badge** style: append the returned element after the name text
7. For **glow** style: apply the returned styling to the tab container div

### Settings page (settings_panels.rs)

Add a dropdown in the **Appearance** section:

- Label: "Tab status style"
- Description: "How session status is shown on tabs"
- Options: `["Dot", "Badge", "Glow"]`
- On change: update `page.user_settings.appearance.tab_status_style` and set `user_save_pending = true`

Follows the existing dropdown pattern used by cursor style.

## Threading & Performance

- **No new async work**: tab rendering reads cached `SessionStatus` from the session struct, which is updated by the existing polling/reconciliation loop
- **No UI thread blocking**: settings are read from an in-memory struct, status is read from cached state
- **Animation**: uses GPUI's built-in animation primitives running on the render pipeline, not the main event loop
- **Settings wiring**: setting changes trigger a debounced save via the existing `schedule_settings_save()` mechanism

## File Changes Summary

| File | Change |
|------|--------|
| `codirigent-core/src/config.rs` | Add `tab_status_style` to `AppearanceSettings` with default |
| `codirigent-ui/src/workspace/tab_status_render.rs` | **New** — status rendering per style + animation |
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
