//! Workspace pointer interaction reducers.
//!
//! This module owns workspace-global pointer gesture coordination for pane drag
//! and split resize interactions. The top-level GPUI view wires mouse events
//! into these methods, while the gesture state transitions live here.

use crate::workspace::gpui::WorkspaceView;
use gpui::{Context, MouseMoveEvent, MouseUpEvent};

pub(super) fn apply_session_drag_drop(
    workspace: &mut super::core::Workspace,
    drag: &super::types::DragState,
    target: &super::types::DragTarget,
) -> bool {
    match target.kind {
        super::types::DragTargetKind::PaneBody => {
            if target.active_session_id.is_none() {
                workspace.group_session_into_pane(drag.source_session_id, target.pane_id.clone())
            } else {
                workspace.swap_sessions(drag.source_index, target.index)
            }
        }
        super::types::DragTargetKind::PaneHeader => {
            workspace.group_session_into_pane(drag.source_session_id, target.pane_id.clone())
        }
    }
}

impl WorkspaceView {
    pub(super) fn handle_workspace_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        cx: &mut Context<Self>,
    ) {
        let pos = crate::layout::Point::new(event.position.x.into(), event.position.y.into());

        if self.selection.split_resize.is_some() {
            if !event.dragging() {
                self.finish_split_resize(cx);
                cx.notify();
                return;
            }

            if self.update_split_resize(pos) {
                cx.notify();
            }
            return;
        }

        let Some(drag) = &mut self.selection.drag else {
            return;
        };

        drag.update_pointer(pos, &self.cache.render_pane_drop_targets);
        cx.notify();
    }

    pub(super) fn handle_workspace_mouse_up_left(
        &mut self,
        _event: &MouseUpEvent,
        cx: &mut Context<Self>,
    ) {
        if self.selection.split_resize.is_some() {
            self.finish_split_resize(cx);
            cx.notify();
            return;
        }

        self.finish_session_drag(cx);
    }

    pub(super) fn update_split_resize(&mut self, position: crate::layout::Point) -> bool {
        let Some(resize) = self.selection.split_resize.as_ref().copied() else {
            return false;
        };

        let total = match resize.direction {
            codirigent_core::SplitDirection::Horizontal => resize.bounds.size.width - resize.gap,
            codirigent_core::SplitDirection::Vertical => resize.bounds.size.height - resize.gap,
        };
        if total <= 0.0 {
            return false;
        }

        let offset = match resize.direction {
            codirigent_core::SplitDirection::Horizontal => {
                position.x - resize.bounds.origin.x - resize.grab_offset
            }
            codirigent_core::SplitDirection::Vertical => {
                position.y - resize.bounds.origin.y - resize.grab_offset
            }
        };
        let ratio = offset / total;
        let changed =
            self.workspace
                .resize_split_divider(resize.first_slot, resize.second_slot, ratio);
        if changed {
            if let Some(active_resize) = self.selection.split_resize.as_mut() {
                active_resize.changed = true;
            }
            self.mark_layout_cache_dirty();
        }
        changed
    }

    fn finish_split_resize(&mut self, cx: &mut Context<Self>) {
        if let Some(resize) = self.selection.split_resize.take() {
            if resize.changed {
                self.save_state_to_disk(cx);
            }
        }
    }

    fn finish_session_drag(&mut self, cx: &mut Context<Self>) {
        if let Some(drag) = self.selection.drag.take() {
            if drag.active {
                if let Some(target) = drag.target.clone() {
                    let changed = apply_session_drag_drop(&mut self.workspace, &drag, &target);
                    if changed {
                        self.mark_layout_cache_dirty();
                        self.sync_layout_derived_state();
                        self.save_state_to_disk(cx);
                    }
                }
            }
            cx.notify();
        }
    }
}
