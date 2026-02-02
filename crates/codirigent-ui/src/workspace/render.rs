//! GPUI rendering components for WorkspaceView.
//!
//! This module contains the rendering logic for the workspace sidebar and grid,
//! separated from the main WorkspaceView to keep file sizes manageable.

use super::gpui::WorkspaceView;
use crate::theme::CodirigentTheme;
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
                            // Content area (placeholder for terminal)
                            div()
                                .flex_1()
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(border_color)
                                        .child(format!(
                                            "[{}]",
                                            CodirigentTheme::status_name(info.status)
                                        )),
                                ),
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
}
