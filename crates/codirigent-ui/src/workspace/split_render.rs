//! Split-tree workspace rendering.
//!
//! This module owns the recursive rendering path for split-tree layouts,
//! including divider hit targets and empty split slots.

use crate::icons;
use crate::theme::CodirigentTheme;
use crate::workspace::gpui::WorkspaceView;
use codirigent_core::{LayoutNode, SlotId, SplitDirection};
use gpui::{
    div, prelude::FluentBuilder, px, relative, ClickEvent, Context, InteractiveElement,
    IntoElement, MouseButton, MouseDownEvent, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, Window,
};
use tracing::info;

impl WorkspaceView {
    /// Render the split tree layout using recursive binary tree traversal.
    pub(super) fn render_split_tree_layout(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let theme = self.workspace().theme().clone();
        let grid_gap = theme.grid_gap;
        let grid_bounds = self.workspace().grid_bounds();

        let tree = match self.workspace().layout_state() {
            crate::layout::WorkspaceLayoutState::SplitTree(s) => s.tree().clone(),
            _ => return div().flex_1().into_any_element(),
        };

        let panel_bg: gpui::Hsla = theme.panel_background.into();
        let border_color: gpui::Hsla = theme.border.into();
        let muted: gpui::Hsla = theme.muted.into();

        self.render_split_node(
            &tree,
            grid_bounds,
            &theme,
            grid_gap,
            panel_bg,
            border_color,
            muted,
            window,
            cx,
        )
    }

    /// Recursively render a layout node in the split tree.
    #[allow(clippy::too_many_arguments)]
    fn render_split_node(
        &mut self,
        node: &LayoutNode,
        available: crate::layout::Bounds,
        theme: &CodirigentTheme,
        gap: f32,
        panel_bg: gpui::Hsla,
        border_color: gpui::Hsla,
        muted: gpui::Hsla,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        match node {
            LayoutNode::Leaf { slot } => {
                let focused_session = self.workspace().focused_session_id();
                let session_data = self
                    .workspace()
                    .layout_state()
                    .as_split_tree()
                    .and_then(|state| state.session_at_slot(*slot))
                    .and_then(|session_id| {
                        self.workspace().session(session_id).map(|session| {
                            (
                                session_id,
                                focused_session == Some(session_id),
                                session.name.clone(),
                                session.status,
                            )
                        })
                    });

                if let Some((session_id, is_focused, name, status)) = session_data {
                    let header_hints = if let Some(header) = self.get_terminal_header(session_id) {
                        header.render_hints()
                    } else {
                        crate::terminal_header::TerminalHeader::new(name, status)
                            .with_focused(is_focused)
                            .render_hints()
                    };

                    self.render_session_cell_with_terminal(
                        codirigent_core::PaneId::SplitSlot { slot: *slot },
                        session_id,
                        &header_hints,
                        theme,
                        None,
                        window,
                        cx,
                    )
                    .into_any_element()
                } else {
                    self.render_split_empty_slot(*slot, panel_bg, border_color, muted, cx)
                        .into_any_element()
                }
            }
            LayoutNode::Split {
                direction,
                ratio,
                first,
                second,
            } => {
                let (first_bounds, second_bounds, divider_bounds) =
                    split_child_bounds(*direction, *ratio, available, gap);
                let first_slot = first.slots_in_order().first().copied().unwrap_or(SlotId(0));
                let second_slot = second
                    .slots_in_order()
                    .first()
                    .copied()
                    .unwrap_or(SlotId(0));
                let is_resizing = self.selection.split_resize.as_ref().is_some_and(|resize| {
                    resize.first_slot == first_slot && resize.second_slot == second_slot
                });

                let first_elem = self.render_split_node(
                    first,
                    first_bounds,
                    theme,
                    gap,
                    panel_bg,
                    border_color,
                    muted,
                    window,
                    cx,
                );
                let second_elem = self.render_split_node(
                    second,
                    second_bounds,
                    theme,
                    gap,
                    panel_bg,
                    border_color,
                    muted,
                    window,
                    cx,
                );

                let first_flex = *ratio * 1000.0;
                let second_flex = (1.0 - *ratio) * 1000.0;
                let is_horizontal = *direction == SplitDirection::Horizontal;

                let make_child_div = |elem: gpui::AnyElement, flex: f32| -> gpui::Div {
                    let mut child = div().flex().size_full();
                    child = if is_horizontal {
                        child.flex_col()
                    } else {
                        child.flex_row()
                    };
                    child.style().flex_grow = Some(flex);
                    child.style().flex_shrink = Some(1.0);
                    child.style().flex_basis = Some(relative(0.).into());
                    child.child(elem)
                };

                let divider = self.render_split_divider(
                    *direction,
                    available,
                    divider_bounds,
                    gap,
                    first_slot,
                    second_slot,
                    theme,
                    border_color,
                    is_resizing,
                    cx,
                );

                let mut container = div().flex_1().flex();
                container = if is_horizontal {
                    container.flex_row()
                } else {
                    container.flex_col()
                };
                container
                    .child(make_child_div(first_elem, first_flex))
                    .child(divider)
                    .child(make_child_div(second_elem, second_flex))
                    .into_any_element()
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_split_divider(
        &mut self,
        direction: SplitDirection,
        resize_bounds: crate::layout::Bounds,
        divider_bounds: crate::layout::Bounds,
        gap: f32,
        first_slot: SlotId,
        second_slot: SlotId,
        theme: &CodirigentTheme,
        border_color: gpui::Hsla,
        is_resizing: bool,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let primary: gpui::Hsla = theme.primary.into();
        let divider_bg = if is_resizing {
            primary.opacity(0.35)
        } else {
            border_color.opacity(0.45)
        };
        let divider_hover = if is_resizing {
            primary.opacity(0.45)
        } else {
            primary.opacity(0.22)
        };
        let divider_origin = match direction {
            SplitDirection::Horizontal => divider_bounds.origin.x,
            SplitDirection::Vertical => divider_bounds.origin.y,
        };

        div()
            .id(SharedString::from(format!(
                "split-divider-{}-{}",
                first_slot.0, second_slot.0
            )))
            .flex_shrink_0()
            .bg(divider_bg)
            .when(direction == SplitDirection::Horizontal, |this| {
                this.w(px(divider_bounds.size.width))
                    .h_full()
                    .cursor_col_resize()
            })
            .when(direction == SplitDirection::Vertical, |this| {
                this.h(px(divider_bounds.size.height))
                    .w_full()
                    .cursor_row_resize()
            })
            .hover(|style| style.bg(divider_hover))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                    cx.stop_propagation();
                    let pos =
                        crate::layout::Point::new(event.position.x.into(), event.position.y.into());
                    this.selection.drag = None;
                    this.selection.split_resize = Some(super::types::SplitResizeState {
                        first_slot,
                        second_slot,
                        direction,
                        bounds: resize_bounds,
                        gap,
                        grab_offset: match direction {
                            SplitDirection::Horizontal => (pos.x - divider_origin).max(0.0),
                            SplitDirection::Vertical => (pos.y - divider_origin).max(0.0),
                        },
                        changed: false,
                    });
                    this.selection.is_selecting = false;
                    this.selection.selecting_session_id = None;
                    cx.notify();
                }),
            )
    }

    /// Render an empty slot in split tree mode.
    fn render_split_empty_slot(
        &mut self,
        slot: SlotId,
        panel_bg: gpui::Hsla,
        border_color: gpui::Hsla,
        muted: gpui::Hsla,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        div()
            .id(SharedString::from(format!("empty-slot-{}", slot.0)))
            .size_full()
            .bg(panel_bg)
            .border_1()
            .border_color(border_color)
            .rounded_lg()
            .border_dashed()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_2()
            .cursor_pointer()
            .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                info!(?slot, "Empty split slot clicked — creating session");
                this.create_session_in_slot(slot, cx);
            }))
            .child(
                div()
                    .text_xl()
                    .text_color(muted)
                    .font_family(icons::LUCIDE_FONT_FAMILY)
                    .child(icons::circle_plus()),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(super::types::EMPTY_CELL_MESSAGE),
            )
    }
}

fn split_child_bounds(
    direction: SplitDirection,
    ratio: f32,
    available: crate::layout::Bounds,
    gap: f32,
) -> (
    crate::layout::Bounds,
    crate::layout::Bounds,
    crate::layout::Bounds,
) {
    match direction {
        SplitDirection::Horizontal => {
            let total_w = (available.size.width - gap).max(0.0);
            let first_w = (total_w * ratio).max(0.0);
            let second_w = (total_w - first_w).max(0.0);
            let divider_x = available.origin.x + first_w;
            (
                crate::layout::Bounds::new(
                    available.origin.x,
                    available.origin.y,
                    first_w,
                    available.size.height,
                ),
                crate::layout::Bounds::new(
                    divider_x + gap,
                    available.origin.y,
                    second_w,
                    available.size.height,
                ),
                crate::layout::Bounds::new(
                    divider_x,
                    available.origin.y,
                    gap,
                    available.size.height,
                ),
            )
        }
        SplitDirection::Vertical => {
            let total_h = (available.size.height - gap).max(0.0);
            let first_h = (total_h * ratio).max(0.0);
            let second_h = (total_h - first_h).max(0.0);
            let divider_y = available.origin.y + first_h;
            (
                crate::layout::Bounds::new(
                    available.origin.x,
                    available.origin.y,
                    available.size.width,
                    first_h,
                ),
                crate::layout::Bounds::new(
                    available.origin.x,
                    divider_y + gap,
                    available.size.width,
                    second_h,
                ),
                crate::layout::Bounds::new(
                    available.origin.x,
                    divider_y,
                    available.size.width,
                    gap,
                ),
            )
        }
    }
}
