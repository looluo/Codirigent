//! GPUI rendering components for WorkspaceView.
//!
//! This module contains the rendering logic for the workspace sidebar and grid,
//! separated from the main WorkspaceView to keep file sizes manageable.

use super::gpui::WorkspaceView;
use crate::terminal_view::CursorShape;
use crate::theme::CodirigentTheme;
use codirigent_core::SessionId;
use gpui::{
    div, px, ClickEvent, Context, FontWeight, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};
use tracing::info;

impl WorkspaceView {
    /// Render the sidebar.
    pub(super) fn render_sidebar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.workspace().theme();
        let sidebar_bg: gpui::Hsla = theme.sidebar_background.into();
        let border_color: gpui::Hsla = theme.border.into();
        let fg: gpui::Hsla = theme.foreground.into();

        let width = self.workspace().sidebar_width();
        let sessions = self.workspace().sessions().to_vec(); // Clone to avoid borrow issues

        // Top padding for macOS transparent titlebar (traffic lights area)
        let titlebar_height = 28.0;

        let mut sidebar = div()
            .w(px(width))
            .h_full()
            .pt(px(titlebar_height))
            .bg(sidebar_bg)
            .border_r_1()
            .border_color(border_color)
            .flex()
            .flex_col();

        // Header
        sidebar = sidebar.child(
            div()
                .h(px(40.0))
                .px_3()
                .flex()
                .items_center()
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(fg)
                        .child("Sessions"),
                ),
        );

        // Session list
        let mut list = div().flex_1().overflow_hidden().flex().flex_col();

        for session in sessions {
            let status_color: gpui::Hsla = theme.status_color(session.status).into();
            let is_focused = self.workspace().focused_session_id() == Some(session.id);
            let item_bg = if is_focused {
                let active: gpui::Hsla = theme.active.into();
                active.opacity(0.2)
            } else {
                gpui::Hsla::transparent_black()
            };

            list = list.child(
                div()
                    .h(px(32.0))
                    .px_3()
                    .bg(item_bg)
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        // Status indicator dot
                        div()
                            .w(px(8.0))
                            .h(px(8.0))
                            .rounded_full()
                            .bg(status_color),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(fg)
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(session.name.clone()),
                    ),
            );
        }

        sidebar = sidebar.child(list);

        // New session button with click handler
        let muted: gpui::Hsla = theme.muted.into();
        let hover_bg: gpui::Hsla = theme.active.into();
        sidebar = sidebar.child(
            div()
                .id("new-session-btn")
                .h(px(44.0))
                .px_3()
                .border_t_1()
                .border_color(border_color)
                .flex()
                .items_center()
                .cursor_pointer()
                .hover(|style| style.bg(hover_bg.opacity(0.1)))
                .on_click(cx.listener(|this, _event: &ClickEvent, _window, cx| {
                    info!("New Session button clicked");
                    this.create_session(cx);
                }))
                .child(
                    div()
                        .text_sm()
                        .text_color(muted)
                        .child("+ New Session (Cmd+N)"),
                ),
        );

        sidebar
    }

    /// Render the grid of session panes.
    pub(super) fn render_grid(&self) -> impl IntoElement {
        let theme = self.workspace().theme();
        let panel_bg: gpui::Hsla = theme.panel_background.into();
        let border_color: gpui::Hsla = theme.border.into();
        let fg: gpui::Hsla = theme.foreground.into();

        let cells = self.workspace().cell_info();
        let profile = self.workspace().layout_profile();
        let (rows, cols) = profile.dimensions();

        let mut grid = div().flex_1().flex().flex_col().gap(px(theme.grid_gap));

        for row in 0..rows {
            let mut row_div = div().flex_1().flex().flex_row().gap(px(theme.grid_gap));

            for col in 0..cols {
                let index = (row * cols + col) as usize;
                let cell = cells.get(index);

                let cell_div = if let Some(info) = cell {
                    let status_color: gpui::Hsla = theme.status_color(info.status).into();
                    let cell_border = if info.is_focused {
                        let active: gpui::Hsla = theme.active.into();
                        active
                    } else {
                        border_color
                    };

                    div()
                        .flex_1()
                        .bg(panel_bg)
                        .border_1()
                        .border_color(cell_border)
                        .rounded_md()
                        .flex()
                        .flex_col()
                        .overflow_hidden()
                        .child(
                            // Header with session name
                            div()
                                .h(px(28.0))
                                .px_2()
                                .border_b_1()
                                .border_color(border_color)
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .w(px(8.0))
                                        .h(px(8.0))
                                        .rounded_full()
                                        .bg(status_color),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(fg)
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .child(info.name.clone()),
                                ),
                        )
                        .child(
                            // Terminal content area
                            self.render_terminal_content(info.session_id, &theme),
                        )
                } else {
                    // Empty cell
                    div()
                        .flex_1()
                        .bg(panel_bg)
                        .border_1()
                        .border_color(border_color)
                        .rounded_md()
                        .border_dashed()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            div()
                                .text_xs()
                                .text_color(border_color)
                                .child("[Empty]"),
                        )
                };

                row_div = row_div.child(cell_div);
            }

            grid = grid.child(row_div);
        }

        grid
    }

    /// Render the terminal content for a session.
    fn render_terminal_content(
        &self,
        session_id: SessionId,
        theme: &CodirigentTheme,
    ) -> impl IntoElement {
        let terminal_bg: gpui::Hsla = theme.terminal_background.into();
        let terminal_fg: gpui::Hsla = theme.terminal_foreground.into();

        // Get the terminal view for this session
        let Some(terminal_view) = self.terminals().get(&session_id) else {
            // No terminal yet, show placeholder
            return div()
                .flex_1()
                .bg(terminal_bg)
                .flex()
                .items_center()
                .justify_center()
                .child(div().text_xs().text_color(terminal_fg).child("[No Terminal]"))
                .into_any_element();
        };

        let cell_width = terminal_view.cell_width();
        let cell_height = terminal_view.cell_height();
        let cells_by_row = terminal_view.cells_by_row();
        let cursor_rect = terminal_view.cursor_rect();
        let term_rows = terminal_view.terminal().rows() as usize;
        let term_cols = terminal_view.terminal().cols() as usize;

        // Build terminal grid
        let mut terminal_div = div()
            .flex_1()
            .bg(terminal_bg)
            .p_1()
            .overflow_hidden()
            .flex()
            .flex_col()
            .font_family("Monaco")
            .text_size(px(terminal_view.font_size()));

        // Render each row
        for row_idx in 0..term_rows {
            let row_cells = cells_by_row.get(row_idx);
            let mut row_div = div().h(px(cell_height)).flex().flex_row();

            // Create a map of column -> cell for quick lookup
            let cell_map: std::collections::HashMap<usize, _> = row_cells
                .map(|cells| cells.iter().map(|c| (c.column, c)).collect())
                .unwrap_or_default();

            // Render each column
            for col_idx in 0..term_cols {
                let cell_div = if let Some(cell) = cell_map.get(&col_idx) {
                    let fg: gpui::Hsla = cell.foreground.into();
                    let bg: gpui::Hsla = cell.background.into();

                    let mut d = div()
                        .w(px(cell_width))
                        .h(px(cell_height))
                        .text_color(fg)
                        .child(cell.character.to_string());

                    // Only set background if not default
                    if cell.background != theme.terminal_background {
                        d = d.bg(bg);
                    }

                    // Apply text decorations
                    if cell.bold {
                        d = d.font_weight(FontWeight::BOLD);
                    }
                    if cell.italic {
                        d = d.italic();
                    }
                    if cell.underline {
                        d = d.underline();
                    }

                    d
                } else {
                    // Empty cell
                    div()
                        .w(px(cell_width))
                        .h(px(cell_height))
                        .text_color(terminal_fg)
                        .child(" ")
                };

                row_div = row_div.child(cell_div);
            }

            terminal_div = terminal_div.child(row_div);
        }

        // Add cursor overlay if visible
        if let Some(cursor) = cursor_rect {
            let cursor_color: gpui::Hsla = cursor.color.into();
            let cursor_x = cursor.x;
            let cursor_y = cursor.y;
            let cursor_w = cursor.width;
            let cursor_h = cursor.height;

            let cursor_div = match cursor.shape {
                CursorShape::Block => div()
                    .absolute()
                    .left(px(cursor_x + 4.0)) // +4 for padding
                    .top(px(cursor_y + 4.0))
                    .w(px(cursor_w))
                    .h(px(cursor_h))
                    .bg(cursor_color.opacity(0.7)),
                CursorShape::HollowBlock => div()
                    .absolute()
                    .left(px(cursor_x + 4.0))
                    .top(px(cursor_y + 4.0))
                    .w(px(cursor_w))
                    .h(px(cursor_h))
                    .border_1()
                    .border_color(cursor_color),
                CursorShape::Beam => div()
                    .absolute()
                    .left(px(cursor_x + 4.0))
                    .top(px(cursor_y + 4.0))
                    .w(px(2.0))
                    .h(px(cursor_h))
                    .bg(cursor_color),
                CursorShape::Underline => div()
                    .absolute()
                    .left(px(cursor_x + 4.0))
                    .top(px(cursor_y + cursor_h - 2.0 + 4.0))
                    .w(px(cursor_w))
                    .h(px(2.0))
                    .bg(cursor_color),
            };

            // Wrap in relative container for cursor positioning
            terminal_div = div()
                .flex_1()
                .relative()
                .overflow_hidden()
                .child(terminal_div)
                .child(cursor_div);
        }

        terminal_div.into_any_element()
    }
}
