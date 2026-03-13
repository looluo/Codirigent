//! Workspace grid rendering and layout.
//!
//! This module handles rendering of the workspace grid layout, including:
//! - Traditional NxM grid layout
//! - Split tree (binary tree) layout
//! - Session cells with terminals
//! - Empty cells and placeholders

use crate::icons;
use crate::terminal_header::TerminalHeaderRenderHints;
use crate::theme::CodirigentTheme;
use crate::workspace::gpui::WorkspaceView;
use crate::workspace::types::HEADER_HEIGHT;
use codirigent_core::{LayoutNode, SessionId, SlotId, SplitDirection};
use gpui::{
    div, px, relative, ClickEvent, Context, Focusable, FontWeight, InteractiveElement, IntoElement,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, ScrollWheelEvent,
    SharedString, StatefulInteractiveElement, Styled, Window,
};
use std::rc::Rc;
use tracing::info;

/// Visual state of a cell during drag-and-drop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DragVisual {
    /// This cell is being dragged (source) — dim it.
    Source,
    /// This cell is the current drop target — highlight it.
    Target,
}

impl WorkspaceView {
    /// Dispatch workspace rendering to the appropriate layout: split-tree or NxM grid.
    pub(super) fn render_grid_with_headers(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        if self.workspace().is_split_tree_mode() {
            self.render_split_tree_layout(window, cx).into_any_element()
        } else {
            self.render_grid_layout(window, cx).into_any_element()
        }
    }

    /// Render the traditional NxM grid layout.
    fn render_grid_layout(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        // Clone all theme values upfront to avoid borrow issues
        let theme = self.workspace().theme().clone();
        let panel_bg: gpui::Hsla = theme.panel_background.into();
        let border_color: gpui::Hsla = theme.border.into();
        let muted: gpui::Hsla = theme.muted.into();
        let grid_gap = theme.grid_gap;

        let profile = self.workspace().layout_profile();
        let (rows, cols) = profile.dimensions();

        // Get cell height from grid layout (height is calculated from available vertical space)
        let layout = self.grid_layout_with_task_board();
        let cell_height = layout.cell_size().height;

        let mut grid = div().flex_1().flex().flex_col().gap(px(grid_gap));

        for row in 0..rows {
            let mut row_div = div()
                .flex_1() // Equal row heights via flex distribution
                .flex()
                .flex_row()
                .gap(px(grid_gap));

            for col in 0..cols {
                let index = (row * cols + col) as usize;
                let position = codirigent_core::GridPosition { row, col };

                let cell_div = if let Some(info) = self
                    .cache
                    .render_cell_info
                    .iter()
                    .find(|info| info.index == index)
                    .cloned()
                {
                    // Get or create terminal header hints
                    let header_hints =
                        if let Some(header) = self.get_terminal_header(info.session_id) {
                            header.render_hints()
                        } else {
                            let focused =
                                self.workspace().focused_session_id() == Some(info.session_id);
                            self.workspace()
                                .session(info.session_id)
                                .map(|session| {
                                    crate::terminal_header::TerminalHeader::new(
                                        &session.name,
                                        session.status,
                                    )
                                    .with_focused(focused)
                                    .render_hints()
                                })
                                .unwrap_or_else(|| {
                                    crate::terminal_header::TerminalHeader::new(
                                        "Session",
                                        codirigent_core::SessionStatus::Idle,
                                    )
                                    .with_focused(focused)
                                    .render_hints()
                                })
                        };

                    // Render session cell with actual terminal content
                    self.render_session_cell_with_terminal(
                        info.pane_id.clone(),
                        info.session_id,
                        &header_hints,
                        &theme,
                        Some(cell_height),
                        window,
                        cx,
                    )
                } else {
                    // Empty cell - render inline
                    self.render_empty_cell_inline_with_colors(
                        position,
                        panel_bg,
                        border_color,
                        muted,
                        cell_height,
                        cx,
                    )
                };

                // Let flex distribute equal widths; use size_full so
                // the child fills the flex-allocated area
                row_div = row_div.child(div().flex_1().size_full().child(cell_div));
            }

            grid = grid.child(row_div);
        }

        grid
    }

    /// Render the split tree layout using recursive binary tree traversal.
    fn render_split_tree_layout(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = self.workspace().theme().clone();
        let grid_gap = theme.grid_gap;

        // Collect split tree state needed for rendering
        let tree = match self.workspace().layout_state() {
            crate::layout::WorkspaceLayoutState::SplitTree(s) => s.tree().clone(),
            _ => return div().flex_1().into_any_element(),
        };

        // Pre-compute colors once to avoid redundant conversions on every recursive call
        let panel_bg: gpui::Hsla = theme.panel_background.into();
        let border_color: gpui::Hsla = theme.border.into();
        let muted: gpui::Hsla = theme.muted.into();

        self.render_split_node(
            &tree,
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
                    // Empty slot
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
                // Render children recursively (pass pre-computed colors to avoid per-call conversion)
                let first_elem = self.render_split_node(
                    first,
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
                    theme,
                    gap,
                    panel_bg,
                    border_color,
                    muted,
                    window,
                    cx,
                );

                // Use flex ratio to distribute space: first gets `ratio`, second gets `1 - ratio`
                // Multiply by 1000 for precision in flex-grow values
                let first_flex = *ratio * 1000.0;
                let second_flex = (1.0 - *ratio) * 1000.0;

                // Horizontal: children are flex-col, container is flex-row
                // Vertical: children are flex-row, container is flex-col
                let is_horizontal = *direction == SplitDirection::Horizontal;

                let make_child_div = |elem: gpui::AnyElement, flex: f32| -> gpui::Div {
                    let mut d = div().flex().size_full();
                    d = if is_horizontal {
                        d.flex_col()
                    } else {
                        d.flex_row()
                    };
                    d.style().flex_grow = Some(flex);
                    d.style().flex_shrink = Some(1.0);
                    d.style().flex_basis = Some(relative(0.).into());
                    d.child(elem)
                };

                let mut container = div().flex_1().flex().gap(px(gap));
                container = if is_horizontal {
                    container.flex_row()
                } else {
                    container.flex_col()
                };
                let container = container
                    .child(make_child_div(first_elem, first_flex))
                    .child(make_child_div(second_elem, second_flex));

                container.into_any_element()
            }
        }
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

    /// Render a session cell with terminal header and actual terminal content.
    #[allow(clippy::too_many_arguments)]
    fn render_session_cell_with_terminal(
        &mut self,
        pane_id: codirigent_core::PaneId,
        session_id: SessionId,
        hints: &TerminalHeaderRenderHints,
        theme: &CodirigentTheme,
        cell_height: Option<f32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        // Uses HEADER_HEIGHT from types.rs
        let panel_bg: gpui::Hsla = theme.panel_background.into();
        let border_color: gpui::Hsla = theme.border.into();
        let cell_border: gpui::Hsla = if hints.is_focused {
            theme.primary.into()
        } else {
            border_color
        };
        let fg: gpui::Hsla = theme.foreground.into();
        let muted: gpui::Hsla = theme.muted.into();
        let orange: gpui::Hsla = theme.orange.into();

        // Drag-and-drop: find this cell's position in render_cell_info and its logical index.
        // `drag_vec_pos` is the Vec position (for visual state comparison).
        // `drag_logical_index` is `CellInfo.index` (for swap_sessions).
        let drag_vec_pos = self
            .cache
            .render_cell_info
            .iter()
            .position(|c| c.session_id == session_id);
        let drag_logical_index = drag_vec_pos.map(|pos| self.cache.render_cell_info[pos].index);

        let drag_visual = drag_vec_pos.and_then(|_pos| {
            let drag = self.selection.drag.as_ref()?;
            if !drag.active {
                return None;
            }
            if drag.source_index == drag_logical_index.unwrap_or(usize::MAX) {
                Some(DragVisual::Source)
            } else if drag.target.map(|target| target.index) == drag_logical_index {
                Some(DragVisual::Target)
            } else {
                None
            }
        });

        // Override border color for drop target
        let cell_border = match drag_visual {
            Some(DragVisual::Target) => {
                let primary: gpui::Hsla = theme.primary.into();
                primary
            }
            _ => cell_border,
        };

        let header_border = cell_border;

        // Color indicator bar
        let color_indicator: gpui::Hsla = hints.color_indicator.into();
        let status_color: gpui::Hsla = hints.status.color.into();
        let pane_tab_ids = self.workspace().pane_tab_session_ids(pane_id.clone());
        let show_plus_button = self
            .workspace()
            .pane_active_session_id(pane_id.clone())
            .is_some();

        let mut header = div()
            .id(SharedString::from(format!(
                "terminal-header-{}",
                session_id.0
            )))
            .h(px(hints.height))
            .w_full()
            .bg(panel_bg)
            .border_b_1()
            .border_color(header_border)
            .flex()
            .items_center()
            .px_2()
            .gap_2()
            .child(
                div()
                    .w(px(3.0))
                    .h(px(16.0))
                    .rounded_sm()
                    .bg(color_indicator),
            )
            .child(div().w(px(8.0)).h(px(8.0)).rounded_full().bg(status_color))
            .child({
                let mut tab_strip = div().flex().items_center().gap_1().overflow_hidden();

                for tab_session_id in pane_tab_ids {
                    let tab_is_active = tab_session_id == session_id;
                    let tab_name = self
                        .workspace()
                        .session(tab_session_id)
                        .map(|session| session.name.clone())
                        .unwrap_or_else(|| hints.name.clone());
                    let tab_bg = if tab_is_active {
                        theme.active.into()
                    } else {
                        border_color.opacity(0.35)
                    };
                    let tab_fg = if tab_is_active {
                        fg
                    } else {
                        muted.opacity(0.9)
                    };

                    let mut tab = div()
                        .id(SharedString::from(format!(
                            "terminal-tab-{}-{}",
                            session_id.0, tab_session_id.0
                        )))
                        .px_2()
                        .h(px(22.0))
                        .rounded_md()
                        .bg(tab_bg)
                        .flex()
                        .items_center()
                        .gap_1()
                        .overflow_hidden()
                        .cursor_pointer()
                        .on_click(cx.listener({
                            let pane_id = pane_id.clone();
                            move |this, _: &ClickEvent, _window, cx| {
                                if this
                                    .workspace
                                    .activate_pane_tab(pane_id.clone(), tab_session_id)
                                {
                                    this.select_session_with_cx(tab_session_id, cx);
                                    this.mark_layout_cache_dirty();
                                    this.sync_layout_derived_state();
                                    this.save_state_to_disk(cx);
                                    cx.notify();
                                }
                            }
                        }))
                        .child(
                            div()
                                .text_xs()
                                .font_weight(if tab_is_active {
                                    FontWeight::SEMIBOLD
                                } else {
                                    FontWeight::MEDIUM
                                })
                                .text_color(tab_fg)
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(tab_name),
                        );

                    if tab_is_active {
                        tab = tab
                            .cursor_grab()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                                    let pos = crate::layout::Point::new(
                                        event.position.x.into(),
                                        event.position.y.into(),
                                    );
                                    this.selection.drag = Some(super::types::DragState {
                                        source_session_id: tab_session_id,
                                        source_index: drag_logical_index.unwrap_or(0),
                                        start_position: pos,
                                        current_position: pos,
                                        active: false,
                                        target: None,
                                    });
                                    cx.notify();
                                }),
                            )
                            .on_mouse_move(cx.listener(
                                move |this, event: &MouseMoveEvent, _window, cx| {
                                    let Some(drag) = &mut this.selection.drag else {
                                        return;
                                    };
                                    if drag.source_session_id != tab_session_id {
                                        return;
                                    }
                                    let pos = crate::layout::Point::new(
                                        event.position.x.into(),
                                        event.position.y.into(),
                                    );
                                    drag.update_pointer(pos, &this.cache.render_cell_info);
                                    cx.notify();
                                },
                            ));
                    }

                    tab_strip = tab_strip.child(tab);
                }

                tab_strip
            });

        // Project/directory name (after session name)
        if let Some(project) = &hints.project_name {
            header = header.child(
                div()
                    .text_xs()
                    .text_color(muted.opacity(0.7))
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(project.clone()),
            );
        }

        // Git branch badge (after session name)
        if let Some(branch) = &hints.git_branch {
            let git_fg = muted.opacity(0.8);
            let git_badge_bg = border_color.opacity(0.25);
            let branch_label = if branch.chars().count() > 16 {
                let truncated: String = branch.chars().take(13).collect();
                format!("{}...", truncated)
            } else {
                branch.clone()
            };
            let mut git_badge = div()
                .px(px(4.0))
                .py_px()
                .rounded_sm()
                .bg(git_badge_bg)
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap_1()
                .child(
                    div()
                        .text_xs()
                        .text_color(git_fg)
                        .font_family(icons::LUCIDE_FONT_FAMILY)
                        .child(icons::git_branch()),
                )
                .child(div().text_xs().text_color(git_fg).child(branch_label));

            if let Some(count) = hints.git_dirty_count {
                if count > 0 {
                    git_badge = git_badge.child(
                        div()
                            .text_xs()
                            .text_color(orange)
                            .child(format!("+{}", count)),
                    );
                }
            }

            header = header.child(git_badge);
        }

        if let Some(shell_label) = &hints.shell_label {
            let shell_warning = hints.shell_warning.is_some();
            let shell_fg = if shell_warning {
                orange
            } else {
                muted.opacity(0.8)
            };
            let shell_bg = if shell_warning {
                orange.opacity(0.12)
            } else {
                border_color.opacity(0.25)
            };
            header = header.child(
                div()
                    .px(px(4.0))
                    .py_px()
                    .rounded_sm()
                    .bg(shell_bg)
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap_1()
                    .child(
                        div()
                            .text_xs()
                            .text_color(shell_fg)
                            .font_family(icons::LUCIDE_FONT_FAMILY)
                            .child(icons::terminal()),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(shell_fg)
                            .child(shell_label.clone()),
                    ),
            );
        }

        header = header.child(div().flex_1());

        // Task badge (if any)
        if let Some(task) = &hints.task {
            let task_bg: gpui::Hsla = task.bg_color.into();
            let task_color: gpui::Hsla = task.text_color.into();
            header = header.child(
                div()
                    .px_2()
                    .py_px()
                    .rounded_sm()
                    .bg(task_bg)
                    .text_xs()
                    .text_color(task_color)
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(task.display_text.clone()),
            );
        }

        // Context usage (if any)
        if let Some(context) = &hints.context {
            let context_color: gpui::Hsla = context.color.into();
            header = header.child(
                div()
                    .text_xs()
                    .text_color(context_color)
                    .child(context.text().to_string()),
            );
        }

        if show_plus_button {
            header = header.child(
                div()
                    .id(SharedString::from(format!("pane-add-tab-{}", session_id.0)))
                    .w(px(20.0))
                    .h(px(20.0))
                    .rounded_md()
                    .bg(border_color.opacity(0.25))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .hover(|style| style.bg(border_color.opacity(0.45)))
                    .on_click(cx.listener({
                        let pane_id = pane_id.clone();
                        move |this, _: &ClickEvent, _window, cx| {
                            this.create_session_in_pane(pane_id.clone(), cx);
                        }
                    }))
                    .child(
                        div()
                            .text_xs()
                            .font_family(icons::LUCIDE_FONT_FAMILY)
                            .text_color(fg)
                            .child(icons::plus()),
                    ),
            );
        }

        // Set cursor for draggable header
        header = if matches!(drag_visual, Some(DragVisual::Source)) {
            header.cursor_grabbing()
        } else {
            header
        };

        // Mouse-up handling for active-tab drags lives on the workspace root.

        // Render terminal content before building the div tree so the
        // mutable borrow on `self` is released before `cx.listener()`.
        let entity = cx.entity();
        let fh = self.focus_handle(cx);
        let is_focused = self.workspace.focused_session_id() == Some(session_id);
        let input_enabled = !self.has_blocking_modal();
        let (terminal_content, canvas_origin) = self.render_terminal_content(
            session_id,
            theme,
            Some((entity, fh, is_focused, input_enabled)),
            window,
        );

        // Clone canvas_origin for each mouse handler closure
        let origin_for_down = Rc::clone(&canvas_origin);
        let origin_for_move = Rc::clone(&canvas_origin);

        let mut outer = div()
            .id(SharedString::from(format!("session-cell-{}", session_id.0)))
            .w_full()
            .bg(panel_bg)
            .border_color(cell_border)
            .rounded_lg()
            .flex()
            .flex_col()
            .overflow_hidden()
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    this.select_session_with_cx(session_id, cx);
                    window.focus(&this.focus_handle(cx));
                    cx.notify();
                }),
            );

        // Drag visual: thicker border for drop target, normal otherwise
        outer = if matches!(drag_visual, Some(DragVisual::Target)) {
            outer.border_2()
        } else {
            outer.border_1()
        };

        // Drag visual: dim the source cell
        if matches!(drag_visual, Some(DragVisual::Source)) {
            outer = outer.opacity(0.5);
        }

        // Fixed height for grid mode, flexible for split tree mode
        if let Some(h) = cell_height {
            outer = outer.h(px(h));
        } else {
            outer = outer.size_full().flex_1();
        }

        let mut terminal_area = div()
            .id(SharedString::from(format!(
                "terminal-area-{}",
                session_id.0
            )))
            .w_full();

        // Fixed height for grid mode, flexible for split tree mode
        if let Some(h) = cell_height {
            terminal_area = terminal_area.h(px(h - HEADER_HEIGHT));
        } else {
            terminal_area = terminal_area.flex_1();
        }

        outer.child(header).child(
            terminal_area
                .overflow_hidden()
                .on_scroll_wheel(
                    cx.listener(move |this, event: &ScrollWheelEvent, _window, cx| {
                        if let Some(tv) = this.terminals_mut().get_mut(&session_id) {
                            let cell_h: f32 = tv.cell_height();
                            let delta_y: f32 = event.delta.pixel_delta(px(cell_h)).y.into();
                            // Positive delta_y = scroll up = show older content (scrollback)
                            if delta_y > 0.0 {
                                let lines = (delta_y / cell_h).ceil().max(1.0) as usize;
                                tv.scroll_up(lines);
                            } else if delta_y < 0.0 {
                                let lines = (-delta_y / cell_h).ceil().max(1.0) as usize;
                                tv.scroll_down(lines);
                            }
                            cx.notify();
                        }
                    }),
                )
                // Mouse down: start text selection
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                        // Don't start text selection during drag
                        if this.selection.drag.as_ref().is_some_and(|d| d.active) {
                            return;
                        }
                        let (ox, oy) = origin_for_down.get();
                        let mouse_x: f32 = event.position.x.into();
                        let mouse_y: f32 = event.position.y.into();
                        let rel_x = mouse_x - ox;
                        let rel_y = mouse_y - oy;

                        if let Some(tv) = this.terminals_mut().get_mut(&session_id) {
                            let cell_pos: Option<(usize, usize)> = tv.pixel_to_cell(rel_x, rel_y);
                            if let Some((row, col)) = cell_pos {
                                tv.start_selection(row, col);
                                this.selection.is_selecting = true;
                                this.selection.selecting_session_id = Some(session_id);
                                cx.notify();
                            }
                        }
                    }),
                )
                // Mouse move: update selection during drag (with auto-scroll)
                .on_mouse_move(
                    cx.listener(move |this, event: &MouseMoveEvent, _window, cx| {
                        // Don't update text selection during drag
                        if this.selection.drag.as_ref().is_some_and(|d| d.active) {
                            return;
                        }
                        if !this.selection.is_selecting {
                            return;
                        }
                        if this.selection.selecting_session_id != Some(session_id) {
                            return;
                        }

                        let (ox, oy) = origin_for_move.get();
                        let mouse_x: f32 = event.position.x.into();
                        let mouse_y: f32 = event.position.y.into();
                        let rel_x = mouse_x - ox;
                        let rel_y = mouse_y - oy;

                        if let Some(tv) = this.terminals_mut().get_mut(&session_id) {
                            let (row, col, scroll_dir) = tv.pixel_to_cell_clamped(rel_x, rel_y);

                            // Auto-scroll when dragging above or below the viewport
                            if scroll_dir < 0 {
                                tv.scroll_up(1);
                            } else if scroll_dir > 0 {
                                tv.scroll_down(1);
                            }

                            tv.update_selection(row, col);
                            cx.notify();
                        }
                    }),
                )
                // Right-click: paste from clipboard (standard terminal behavior)
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                        this.handle_paste(&crate::app::Paste, window, cx);
                    }),
                )
                // Mouse up: end selection
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(move |this, _event: &MouseUpEvent, _window, cx| {
                        if this.selection.is_selecting
                            && this.selection.selecting_session_id == Some(session_id)
                        {
                            // Selection stays active (for copy) until next click or clear
                            this.selection.is_selecting = false;
                            this.selection.selecting_session_id = None;
                            cx.notify();
                        }
                    }),
                )
                .child(terminal_content),
        )
    }
}
