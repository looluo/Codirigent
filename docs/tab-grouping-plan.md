# Tab Grouping Plan

## Purpose

Define the first-pass design for GitHub issue `#14`: allow users to group multiple sessions into tabs within a single pane.

This feature is separate from the hidden-session fix. Hidden-session access remains the fallback for reaching sessions outside the current visible working set. Tab grouping is a manual layout tool for keeping more sessions active while preserving a compact visible layout such as `2x2` or `3x3`.

## Problem

Users may run more sessions than they want to display as panes at one time. The current workspace supports compact custom layouts, but each visible pane can only host one session. That forces users to either increase pane count or keep reshuffling layouts.

Tabs should let users keep a stable layout while intentionally compressing related sessions into the same pane.

## Goals

- Keep visible layouts compact and stable.
- Let a pane hold multiple sessions as tabs.
- Make tab grouping a direct workspace interaction, not a menu-only action.
- Provide a pane-local way to create a new session once the grid is full.

## Non-Goals

- No automatic overflow-to-tabs behavior.
- No layout auto-expansion.
- No global session reshuffle.
- No tab tear-off or drag-out in v1.
- No cross-pane tab reordering UI in v1.
- No changes to the existing logical session grouping feature in the menu.

## Key Distinction

Two features named "grouping" must remain separate:

- Existing session grouping:
  Logical organization and color/group metadata in the session menu and drawer.
- New tab grouping:
  Multiple sessions sharing a single visible pane.

The existing menu grouping should not be repurposed for tabs.

## Proposed UX

### Tab Creation By Drag And Drop

- Drag a pane header onto another pane header to group the dragged session into the target pane.
- Dropping on the target pane header creates or extends a tab stack in that pane.
- The dropped session becomes the active tab immediately.
- The target pane keeps its layout position.
- The source pane is removed from visible assignment if it becomes empty.

### Existing Pane Drag Behavior

- Drop on pane body:
  Keep current swap/move behavior.
- Drop on pane header:
  Group into tabs.

This keeps the interaction explicit and avoids conflict with current reordering behavior.

### Tab Switching

- Clicking a tab in a pane header switches the visible session for that pane.
- Switching tabs does not change layout structure.
- Focus remains in the same pane.

### Pane-Level New Session Button

- Each pane header should include a small `+` button, similar to a browser tab strip.
- Clicking `+` creates a new session in that pane as a new tab.
- The new session becomes the active tab immediately.
- The pane keeps its current visible position.

This addresses the current gap where adding a session is awkward once the visible layout is already full.

## Initial Tab Rules

- When dropping session `A` onto a pane currently showing session `B`, the resulting tab order is `[B, A]`.
- Session `A` becomes the active tab immediately after grouping.
- If a pane already has tabs, the dropped session is appended to the target tab stack and becomes active.
- If a pane contains only one session, the header still supports grouping and `+`.

## Session Creation Rules

For the pane-level `+` action in v1:

- The new session should inherit the current pane's working directory/context.
- The new session should use the existing default new-session settings.
- This does not include per-session shell selection yet. That remains part of issue `#12`.

## Close Behavior

- Closing the active tab in a multi-tab pane reveals the next available tab in that pane.
- Closing a non-active tab removes it without affecting the active tab.
- Closing the last remaining tab behaves like closing the pane's only session today.

## Layout Model

Tabs should be modeled as a real part of workspace state, not as a render-only illusion.

Recommended conceptual state:

- each visible slot/pane owns an ordered tab stack of session IDs
- each slot/pane tracks which tab is active

This is a better long-term fit than the current one-session-per-slot assumption and will make switching, persistence, closing, and future enhancements more coherent.

## Sessions Drawer

For v1, the Sessions drawer should continue to list sessions normally.

Deferred for later:

- showing tab membership in the drawer
- dragging from the drawer into tabs
- any drawer-specific tab-management UI

The primary interaction surface for tabs should be the pane header itself.

## Visual Expectations

- A pane with one session can keep the current header look with a subtle `+` affordance added.
- A pane with multiple sessions should render a tab strip in its header.
- Active tab styling should remain visually aligned with the current workspace theme.
- The `+` affordance should be compact and clearly separate from existing session actions.

## Implementation Outline

### Workspace/Core

Refactor pane assignment state so a visible pane can host multiple sessions and track one active session.

Expected responsibilities:

- create a tab stack in a target pane
- append sessions to an existing tab stack
- switch the active tab for a pane
- create a new session directly into a pane/tab stack
- close tabs while preserving pane stability

### Drag And Drop

Extend the existing drag system to distinguish between:

- header-target drop for tab grouping
- body-target drop for swap/move

This will likely require a more precise drop-target model than the current single target-index state.

### Header Rendering

Update pane header rendering so it can display:

- a single-session header state
- a multi-tab strip state
- a pane-level `+` button

### Persistence

Tab stacks and active-tab selection should be persisted as part of workspace state so layout restoration preserves tab grouping.

## Testing Plan

Add or update tests for:

- dragging a session onto another pane header creates a tab stack
- dropped session becomes active
- tab order matches `[target-existing..., dropped]`
- dropping on pane body preserves current swap behavior
- clicking a tab switches the visible session in place
- clicking `+` creates a new session as a tab in that pane
- closing tabs preserves the remaining tab stack correctly
- focus remains stable within the pane during tab switching
- persistence restores tab stacks and active-tab state

## Rollout Notes

This should land as a focused manual-grouping feature for issue `#14`.

It should not absorb hidden-session overflow policy and should not depend on issue `#12`. Keeping those concerns separate will make the first version of tab grouping easier to reason about and lower-risk to ship.
