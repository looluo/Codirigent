# Resizable Split Pane Plan

## Purpose

Define the implementation plan for resizable split panes so users can adjust split proportions beyond the current fixed `50/50` behavior, such as `75/25`, by dragging split dividers directly in the workspace.

## Problem

The current split-tree layout model supports ratios internally, but the primary user-facing split actions create panes at `0.5` and there is no direct workspace interaction for changing that proportion afterward.

Users want to keep compact custom layouts while controlling how much space each pane gets. A common example is making one pane take roughly `3/4` of the available height or width.

## Goals

- Let users resize split-tree panes by dragging dividers.
- Support arbitrary split ratios within reasonable limits.
- Preserve the existing header drag behavior for session movement/reordering.
- Persist custom split ratios so session resume restores the same layout proportions.
- Keep the behavior predictable and visually clear.

## Non-Goals

- No divider resizing for fixed grid layouts.
- No keyboard-only resize controls in this change.
- No preset-ratio-only solution; direct dragging is required.
- No refactor of the existing pane header drag model beyond what is needed to coordinate drag modes safely.

## Current State

The codebase already contains most of the underlying ratio support:

- split-tree nodes already store a `ratio: f32`
- the layout tree already supports `set_ratio_for_slot()`
- split layout state already supports `resize_split()`
- divider hit-testing already exists in the split layout calculator
- layout persistence already serializes `LayoutMode::SplitTree { root }`

This means the missing feature is primarily the workspace interaction layer and drag-state coordination, not the core layout math.

## Proposed UX

### Divider Drag

- When the workspace is in split-tree mode, the divider between two panes should be draggable.
- Hovering a divider should show the correct resize cursor:
  - horizontal split divider: vertical resize cursor
  - vertical split divider: horizontal resize cursor
- Mouse down on a divider enters split-resize drag mode.
- Dragging updates the ratio continuously as the pointer moves.
- Mouse up commits the new ratio.

### Pane/Header Drag Compatibility

Header drag and divider drag must remain separate interactions:

- pane header drag starts only from header bounds
- divider drag starts only from divider bounds
- terminal selection starts only from terminal content

Once one drag mode has started, the others must be suppressed until mouse-up.

## Interaction Model

The workspace should treat pointer interactions as mutually exclusive modes.

Recommended conceptual state:

- `None`
- `SessionReorderDrag`
- `SplitResizeDrag`

`SplitResizeDrag` should carry enough information to update the correct split ratio as the pointer moves, including:

- the divider/split being resized
- the drag start point
- the original ratio
- the relevant layout bounds

## Ratio Behavior

- Ratios should remain clamped to safe limits.
- The existing core clamp behavior should remain authoritative.
- Dragging should feel continuous, not snap to a few preset percentages.

The first implementation should preserve minimum pane sizes through the existing layout clamp behavior and any current split-tree minimum-cell logic.

## Layout Scope

Resizable dividers should apply only to split-tree layouts.

Grid layouts should remain fixed-profile layouts such as `2x2`, `2x3`, and `3x3`. If users want custom uneven pane sizing, they should be in split-tree mode.

## Persistence And Resume

Custom split ratios must survive app restart and session restore.

This should work through the existing layout persistence path:

- current layout is saved as `LayoutMode::SplitTree { root }`
- split ratios are stored within the layout tree
- restore re-applies the saved split tree

This feature must validate that resized split ratios are correctly restored, not only that the split tree shape is restored.

## Implementation Outline

### Workspace Interaction State

Extend the workspace pointer interaction model to support split-resize dragging alongside existing pane/header drag behavior.

Expected responsibilities:

- detect divider hover/hit in split-tree mode
- start resize drag on divider mouse down
- update ratio during pointer move
- finish resize drag on mouse up
- prevent interaction overlap with header drag and terminal selection

### Divider Hit Testing

Use the existing split layout divider hit-testing to determine whether the pointer is over a divider and which split should be resized.

The divider hit area should be explicit and reliable so users can easily discover and use it.

### Ratio Update Path

Translate pointer movement into a new ratio for the affected split and apply it through the existing split-tree resize API.

This should update the layout in real time during drag so users can see the effect immediately.

### Cursor Feedback

Add cursor feedback on divider hover and drag so users understand when a resize gesture will occur instead of a header move or terminal selection.

### Persistence

Ensure ratio changes trigger the normal layout persistence path so resized layouts are included in saved state without requiring separate persistence logic.

## Testing Plan

Add or update tests for:

- divider hit-testing identifies the correct divider in split-tree layouts
- dragging a divider updates split ratio away from `0.5`
- ratio updates are clamped correctly at the allowed bounds
- header drag does not start when the pointer begins on a divider
- divider drag does not start when the pointer begins on a pane header
- terminal selection does not interfere with an active divider drag
- resized split layouts persist to saved state with the updated ratio
- restored split layouts preserve the saved ratio, not only the split-tree shape
- nested split trees resize the intended parent split rather than an unrelated branch

## Rollout Notes

This feature should be implemented as a split-tree enhancement, not as a general grid-layout resize system.

The finished behavior should be:

- create split layout
- drag divider to any practical ratio such as `3/4`
- keep using the workspace normally
- quit and resume later with the same split proportions intact
