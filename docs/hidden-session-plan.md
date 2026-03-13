# Hidden Session Access Plan

## Purpose

Define the first-pass fix for GitHub issue `#13`: when the active layout shows fewer sessions than exist in the workspace, every session must remain reachable without forcing a layout change.

This plan intentionally does not solve tab grouping. Tab grouping remains a separate enhancement.

## Problem

Today, a layout such as `2x2` can only display four panes at once. If the workspace has a fifth session, that session exists but may not be reachable from the current layout unless the user changes the layout and repositions panes.

That behavior is a UX bug because the session list shows the workspace contains the session, but the user cannot directly bring it into view.

## Goals

- Keep compact layouts such as `2x2` and `3x3` viable even when more sessions exist.
- Preserve visible pane positions unless the user explicitly rearranges them.
- Reuse an interaction users already understand from single/focus layout behavior.
- Fix discoverability through the existing Sessions drawer instead of introducing a new modal or overflow manager.

## Non-Goals

- No automatic tab creation.
- No layout auto-expansion.
- No pane reflow or global session reshuffle.
- No new overflow tray, modal picker, or separate hidden-session panel.

## Proposed UX

Use the Sessions drawer as the source of truth for all sessions:

- If the user clicks a session that is already visible in the current layout, focus it.
- If the user clicks a session that is not currently visible, show it in the currently focused pane.
- The session that was previously displayed in the focused pane becomes hidden.
- No other visible panes move.

This makes the interaction consistent with focus mode:

- Click a session to view it in the current visible context.

## Visibility Model

The workspace should treat sessions as one of two states:

- Visible: assigned to a currently rendered pane/slot.
- Hidden: exists in the workspace but is not assigned to a visible pane because the layout capacity is smaller than the total session count.

The Sessions drawer should continue to show all sessions, not only visible ones.

## Interaction Rules

### Clicking From The Sessions Drawer

- Visible session row:
  Focus that session normally.
- Hidden session row:
  Replace the session in the currently focused pane with the clicked hidden session.

### Focus Requirement

- The replacement target is always the currently focused pane.
- If there is no focused pane but at least one visible session exists, fall back to the layout's current focused session semantics.
- If there are no visible sessions, do nothing.

### Ordering Rule

- Hidden-session reveal should behave as a true swap between:
  - the clicked hidden session, and
  - the session currently shown in the focused pane.
- This keeps ordering stable and avoids silently re-packing the workspace.

## UI Expectations

The Sessions drawer should expose visibility clearly:

- Visible sessions render as normal.
- Hidden sessions should display a subtle `Hidden` indicator, dimmed styling, or equivalent compact affordance.

No confirmation dialog should appear for the swap. The action should be immediate.

## Implementation Outline

### Workspace/Core

Add explicit support for swapping a hidden session with a visible session without changing the layout structure.

Expected core behavior:

- Detect whether a clicked session is currently visible.
- If hidden, replace the focused visible assignment with the hidden session.
- Move the replaced session into the hidden set while preserving stable ordering.

The layout structure must remain unchanged.

### Drawer/UI

Update the Sessions drawer row interaction:

- Visible row click keeps current focus behavior.
- Hidden row click triggers the hidden-to-focused swap behavior.

Add visual differentiation for hidden rows.

### Derived State

Any session reveal/swap must refresh:

- focused session state
- drawer selection state
- file tree synchronization
- terminal header focus state
- cached layout-derived UI state

## Testing Plan

Add or update tests for:

- Hidden sessions remain listed in the Sessions drawer.
- Clicking a hidden session swaps it into the focused pane.
- The replaced visible session becomes hidden.
- Other visible panes remain unchanged.
- Focus follows the revealed session.
- Single layout behavior is unchanged.
- Split-tree layouts use the same focused-pane replacement rule.

## Rollout Notes

This should land as the narrow fix for issue `#13`.

Tab grouping can build on top of this later, but should not be coupled to this change. Keeping them separate reduces risk and keeps the hidden-session behavior understandable on its own.
