//! GPUI rendering components for WorkspaceView.
//!
//! This module contains the rendering logic for the workspace sidebar and grid,
//! separated from the main WorkspaceView to keep file sizes manageable.

use super::gpui::WorkspaceView;
use crate::empty_session::EmptySessionRenderHints;
use crate::status_bar::StatusBarItem;
use crate::task_board::TaskBoardTab;
use crate::terminal_header::TerminalHeaderRenderHints;
use crate::theme::CodirigentTheme;
use codirigent_core::SessionId;
use gpui::{
    div, px, ClickEvent, Context, FontWeight, InteractiveElement, IntoElement, ParentElement,
    SharedString, StatefulInteractiveElement, Styled,
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

    /// Render the title bar component.
    ///
    /// Returns a GPUI element representing the title bar with window controls,
    /// logo, project path, and settings button.
    pub(super) fn render_title_bar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let hints = self.title_bar.render_hints();
        let theme = self.workspace().theme();
        let bg: gpui::Hsla = theme.panel_background.into();
        let border_color: gpui::Hsla = theme.border.into();
        let fg: gpui::Hsla = theme.foreground.into();
        let muted: gpui::Hsla = theme.muted.into();

        let mut bar = div()
            .id("title-bar")
            .h(px(hints.height))
            .w_full()
            .bg(bg)
            .border_b_1()
            .border_color(border_color)
            .flex()
            .items_center()
            .px_3()
            .gap_4();

        // Window controls (macOS-style traffic lights)
        #[cfg(target_os = "macos")]
        {
            let mut controls = div().flex().gap_2().items_center();
            for btn in &hints.controls {
                let color: gpui::Hsla = btn.current_color().into();
                controls = controls.child(
                    div()
                        .w(px(12.0))
                        .h(px(12.0))
                        .rounded_full()
                        .bg(color),
                );
            }
            bar = bar.child(controls);
        }

        // Logo
        bar = bar.child(
            div()
                .text_sm()
                .font_weight(FontWeight::BOLD)
                .text_color(fg)
                .child(hints.logo),
        );

        // Spacer
        bar = bar.child(div().flex_1());

        // Project path
        if let Some(path) = &hints.project_path {
            bar = bar.child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(path.clone()),
            );
        }

        // Settings button
        let settings_color = if hints.settings_hovered {
            fg
        } else {
            muted
        };
        bar = bar.child(
            div()
                .id("settings-btn")
                .px_2()
                .cursor_pointer()
                .text_sm()
                .text_color(settings_color)
                .on_click(cx.listener(|this, _: &ClickEvent, _window, _cx| {
                    this.title_bar.click_settings();
                }))
                .child("⚙"),
        );

        bar
    }

    /// Render the status bar component.
    ///
    /// Returns a GPUI element representing the status bar with session counts,
    /// task queue status, and version information.
    pub(super) fn render_status_bar(&self) -> impl IntoElement {
        let hints = self.status_bar.render_hints();
        let bg: gpui::Hsla = hints.background.into();
        let text_color: gpui::Hsla = hints.text_color.into();

        let mut bar = div()
            .id("status-bar")
            .h(px(hints.height))
            .w_full()
            .bg(bg)
            .flex()
            .items_center()
            .justify_between()
            .px_3();

        // Left section
        let mut left = div().flex().gap_4().items_center();
        for item in &hints.left {
            left = left.child(self.render_status_bar_item(item));
        }
        bar = bar.child(left);

        // Right section
        let mut right = div().flex().gap_4().items_center();
        for item in &hints.right {
            right = right.child(self.render_status_bar_item(item));
        }
        bar = bar.child(right);

        bar
    }

    /// Render a single status bar item.
    fn render_status_bar_item(&self, item: &StatusBarItem) -> impl IntoElement {
        let theme = self.workspace().theme();
        let muted: gpui::Hsla = theme.muted.into();

        match item {
            StatusBarItem::SessionCount { total, color } => {
                let dot_color: gpui::Hsla = (*color).into();
                div()
                    .flex()
                    .gap_2()
                    .items_center()
                    .child(
                        div()
                            .w(px(6.0))
                            .h(px(6.0))
                            .rounded_full()
                            .bg(dot_color),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child(format!("{} sessions", total)),
                    )
            }
            StatusBarItem::SessionStatus { label, count, color } => {
                let status_color: gpui::Hsla = (*color).into();
                div()
                    .flex()
                    .gap_1()
                    .items_center()
                    .child(
                        div()
                            .text_xs()
                            .text_color(status_color)
                            .child(format!("{}: {}", label, count)),
                    )
            }
            StatusBarItem::TaskQueue { in_queue, in_progress } => {
                let text = if *in_queue == 0 && *in_progress == 0 {
                    "No tasks".to_string()
                } else {
                    format!("Tasks: {} queued, {} active", in_queue, in_progress)
                };
                div().text_xs().text_color(muted).child(text)
            }
            StatusBarItem::Version(v) => {
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(format!("v{}", v))
            }
            StatusBarItem::Separator => {
                div().text_xs().text_color(muted).child("│")
            }
        }
    }

    /// Render the sessions toolbar component.
    ///
    /// Returns a GPUI element representing the toolbar with layout tabs,
    /// broadcast toggle, and new session button.
    pub(super) fn render_toolbar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let hints = self.toolbar.render_hints();
        let theme = self.workspace().theme();
        let bg: gpui::Hsla = theme.panel_background.into();
        let border_color: gpui::Hsla = theme.border.into();
        let fg: gpui::Hsla = theme.foreground.into();
        let muted: gpui::Hsla = theme.muted.into();
        let active: gpui::Hsla = theme.active.into();
        let primary: gpui::Hsla = theme.primary.into();

        let mut bar = div()
            .id("sessions-toolbar")
            .h(px(hints.height))
            .w_full()
            .bg(bg)
            .border_b_1()
            .border_color(border_color)
            .flex()
            .items_center()
            .px_3()
            .gap_2();

        // Layout tabs
        let mut tabs = div().flex().gap_1().items_center();
        for (i, tab) in hints.tabs.iter().enumerate() {
            let tab_bg = if tab.is_active {
                active
            } else {
                gpui::Hsla::transparent_black()
            };
            let tab_color = if tab.is_active { fg } else { muted };
            let tab_idx = i;

            tabs = tabs.child(
                div()
                    .id(SharedString::from(format!("layout-tab-{}", i)))
                    .px_3()
                    .py_1()
                    .rounded_md()
                    .bg(tab_bg)
                    .text_xs()
                    .font_weight(if tab.is_active {
                        FontWeight::SEMIBOLD
                    } else {
                        FontWeight::NORMAL
                    })
                    .text_color(tab_color)
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                        this.toolbar.click_tab(tab_idx);
                        // Apply the layout change to workspace
                        if let Some(tab) = this.toolbar.tabs().get(tab_idx) {
                            this.workspace.set_layout(tab.profile);
                        }
                        cx.notify();
                    }))
                    .child(tab.label.clone()),
            );
        }
        bar = bar.child(tabs);

        // Spacer
        bar = bar.child(div().flex_1());

        // Broadcast toggle
        let broadcast_color = if hints.broadcast_enabled {
            primary
        } else {
            muted
        };
        bar = bar.child(
            div()
                .id("broadcast-toggle")
                .px_2()
                .py_1()
                .rounded_md()
                .text_xs()
                .text_color(broadcast_color)
                .cursor_pointer()
                .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                    this.toolbar.toggle_broadcast();
                    this.broadcast_enabled = this.toolbar.is_broadcast_enabled();
                    cx.notify();
                }))
                .child(if hints.broadcast_enabled {
                    "● Broadcast"
                } else {
                    "○ Broadcast"
                }),
        );

        // New session button
        bar = bar.child(
            div()
                .id("new-session-toolbar-btn")
                .px_3()
                .py_1()
                .rounded_md()
                .bg(primary.opacity(0.1))
                .text_xs()
                .text_color(primary)
                .cursor_pointer()
                .hover(|style| style.bg(primary.opacity(0.2)))
                .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                    this.toolbar.request_new_session();
                    this.create_session(cx);
                }))
                .child("+ New"),
        );

        bar
    }

    /// Render a terminal header for a session.
    ///
    /// Returns a GPUI element representing the terminal header with session name,
    /// status indicator, task badge, and context usage.
    pub(super) fn render_terminal_header(
        &self,
        session_id: SessionId,
        hints: &TerminalHeaderRenderHints,
    ) -> impl IntoElement {
        let theme = self.workspace().theme();
        let bg: gpui::Hsla = theme.panel_background.into();
        let border_color: gpui::Hsla = theme.border.into();
        let fg: gpui::Hsla = theme.foreground.into();

        let header_border = if hints.is_focused {
            let primary: gpui::Hsla = theme.primary.into();
            primary
        } else {
            border_color
        };

        let mut header = div()
            .id(SharedString::from(format!("terminal-header-{}", session_id.0)))
            .h(px(hints.height))
            .w_full()
            .bg(bg)
            .border_b_1()
            .border_color(header_border)
            .flex()
            .items_center()
            .px_2()
            .gap_2();

        // Color indicator bar
        let color_indicator: gpui::Hsla = hints.color_indicator.into();
        header = header.child(
            div()
                .w(px(3.0))
                .h(px(16.0))
                .rounded_sm()
                .bg(color_indicator),
        );

        // Status dot
        let status_color: gpui::Hsla = hints.status.color.into();
        header = header.child(
            div()
                .w(px(8.0))
                .h(px(8.0))
                .rounded_full()
                .bg(status_color),
        );

        // Session name
        header = header.child(
            div()
                .text_xs()
                .font_weight(FontWeight::MEDIUM)
                .text_color(fg)
                .overflow_hidden()
                .text_ellipsis()
                .child(hints.name.clone()),
        );

        // Spacer
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

        header
    }

    /// Render an empty session cell.
    ///
    /// Returns a GPUI element representing an empty grid slot with a dashed border
    /// and a plus icon.
    pub(super) fn render_empty_cell(
        &self,
        hints: &EmptySessionRenderHints,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let bg: gpui::Hsla = hints.background.into();
        let border_color: gpui::Hsla = hints.border.color.into();
        let icon_color: gpui::Hsla = hints.icon_color.into();
        let text_color: gpui::Hsla = hints.text_color.into();
        let position = hints.position;

        div()
            .id(SharedString::from(format!("empty-cell-{}-{}", position.row, position.col)))
            .flex_1()
            .bg(bg)
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
                info!(?position, "Empty cell clicked");
                this.empty_cells.click(position);
                // Create a new session when clicking an empty cell
                this.create_session(cx);
            }))
            .child(
                div()
                    .text_xl()
                    .text_color(icon_color)
                    .child(hints.icon),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(text_color)
                    .child(hints.message),
            )
    }

    /// Render the task board panel.
    ///
    /// Returns a GPUI element representing the collapsible task board with tabs
    /// for different task states.
    pub(super) fn render_task_board(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let hints = self.task_board.render_hints();
        let theme = self.workspace().theme();
        let bg: gpui::Hsla = theme.panel_background.into();
        let border_color: gpui::Hsla = theme.border.into();
        let fg: gpui::Hsla = theme.foreground.into();
        let muted: gpui::Hsla = theme.muted.into();
        let active: gpui::Hsla = theme.active.into();
        let primary: gpui::Hsla = theme.primary.into();

        let mut panel = div()
            .id("task-board")
            .h(px(hints.height))
            .w_full()
            .bg(bg)
            .border_t_1()
            .border_color(border_color)
            .flex()
            .flex_col();

        // Header row
        let mut header = div()
            .h(px(hints.header_height))
            .w_full()
            .flex()
            .items_center()
            .px_3()
            .gap_2();

        // Expand/collapse button
        let toggle_icon = if hints.is_expanded { "▼" } else { "▶" };
        header = header.child(
            div()
                .id("task-board-toggle")
                .px_1()
                .cursor_pointer()
                .text_xs()
                .text_color(muted)
                .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                    this.task_board.toggle_expanded();
                    cx.notify();
                }))
                .child(toggle_icon),
        );

        // Tab buttons
        let mut tabs = div().flex().gap_1().items_center();
        for tab_btn in &hints.tabs {
            let tab = tab_btn.tab;
            let tab_bg = if tab_btn.is_active {
                active
            } else {
                gpui::Hsla::transparent_black()
            };
            let tab_color = if tab_btn.is_active { fg } else { muted };

            tabs = tabs.child(
                div()
                    .id(SharedString::from(format!("task-tab-{:?}", tab)))
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .bg(tab_bg)
                    .text_xs()
                    .text_color(tab_color)
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                        this.task_board.click_tab(tab);
                        cx.notify();
                    }))
                    .child(format!("{} ({})", tab_btn.label, tab_btn.count)),
            );
        }
        header = header.child(tabs);

        // Spacer
        header = header.child(div().flex_1());

        // Auto-assign toggle
        let auto_color = if hints.auto_assign.enabled {
            primary
        } else {
            muted
        };
        header = header.child(
            div()
                .id("auto-assign-toggle")
                .px_2()
                .py_1()
                .text_xs()
                .text_color(auto_color)
                .cursor_pointer()
                .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                    this.task_board.toggle_auto_assign();
                    cx.notify();
                }))
                .child(if hints.auto_assign.enabled {
                    "● Auto-assign"
                } else {
                    "○ Auto-assign"
                }),
        );

        // Add task button
        header = header.child(
            div()
                .id("add-task-btn")
                .px_2()
                .py_1()
                .rounded_md()
                .text_xs()
                .text_color(primary)
                .cursor_pointer()
                .on_click(cx.listener(|this, _: &ClickEvent, _window, _cx| {
                    this.task_board.click_add_task();
                }))
                .child("+ Add"),
        );

        panel = panel.child(header);

        // Content area (only if expanded)
        if hints.is_expanded {
            let content_height = hints.height - hints.header_height;
            panel = panel.child(
                div()
                    .h(px(content_height))
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child("Task list placeholder"),
                    ),
            );
        }

        panel
    }

    /// Render the grid with terminal headers using the new UI components.
    ///
    /// This is the updated grid renderer that uses TerminalHeader for sessions
    /// and EmptySessionCell for empty slots.
    pub(super) fn render_grid_with_headers(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        // Clone all theme values upfront to avoid borrow issues
        let theme = self.workspace().theme();
        let panel_bg: gpui::Hsla = theme.panel_background.into();
        let border_color: gpui::Hsla = theme.border.into();
        let primary_color: gpui::Hsla = theme.primary.into();
        let muted: gpui::Hsla = theme.muted.into();
        let grid_gap = theme.grid_gap;

        // Clone cell info and layout dimensions
        let cells = self.workspace().cell_info();
        let profile = self.workspace().layout_profile();
        let (rows, cols) = profile.dimensions();

        let mut grid = div()
            .flex_1()
            .flex()
            .flex_col()
            .gap(px(grid_gap));

        for row in 0..rows {
            let mut row_div = div()
                .flex_1()
                .flex()
                .flex_row()
                .gap(px(grid_gap));

            for col in 0..cols {
                let index = (row * cols + col) as usize;
                let position = codirigent_core::GridPosition { row, col };

                let cell_div = if let Some(info) = cells.get(index) {
                    // Session cell with terminal header
                    let cell_border = if info.is_focused {
                        primary_color
                    } else {
                        border_color
                    };

                    // Get or create terminal header hints
                    let header_hints = if let Some(header) = self.get_terminal_header(info.session_id) {
                        header.render_hints()
                    } else {
                        // Create default hints from cell info
                        crate::terminal_header::TerminalHeader::new(&info.name, info.status)
                            .with_focused(info.is_focused)
                            .render_hints()
                    };

                    // Render session cell
                    self.render_session_cell_inline(
                        info.session_id,
                        &header_hints,
                        panel_bg,
                        cell_border,
                        border_color,
                        info.status,
                    )
                } else {
                    // Empty cell - render inline
                    self.render_empty_cell_inline_with_colors(position, panel_bg, border_color, muted, cx)
                };

                row_div = row_div.child(cell_div);
            }

            grid = grid.child(row_div);
        }

        grid
    }

    /// Render a session cell with its terminal header and content area.
    fn render_session_cell_inline(
        &self,
        session_id: SessionId,
        hints: &TerminalHeaderRenderHints,
        panel_bg: gpui::Hsla,
        cell_border: gpui::Hsla,
        border_color: gpui::Hsla,
        status: codirigent_core::SessionStatus,
    ) -> gpui::Stateful<gpui::Div> {
        let theme = self.workspace().theme();
        let fg: gpui::Hsla = theme.foreground.into();

        let header_border = if hints.is_focused {
            let primary: gpui::Hsla = theme.primary.into();
            primary
        } else {
            border_color
        };

        // Color indicator bar
        let color_indicator: gpui::Hsla = hints.color_indicator.into();
        let status_color: gpui::Hsla = hints.status.color.into();

        let mut header = div()
            .id(SharedString::from(format!("terminal-header-{}", session_id.0)))
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
                    .child(hints.name.clone()),
            )
            .child(div().flex_1());

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

        div()
            .id(SharedString::from(format!("session-cell-{}", session_id.0)))
            .flex_1()
            .bg(panel_bg)
            .border_1()
            .border_color(cell_border)
            .rounded_lg()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(header)
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
                                CodirigentTheme::status_name(status)
                            )),
                    ),
            )
    }

    /// Render terminal header inline (returns Stateful<Div> for type consistency).
    fn render_terminal_header_inline(
        &self,
        session_id: SessionId,
        hints: &TerminalHeaderRenderHints,
    ) -> gpui::Stateful<gpui::Div> {
        let theme = self.workspace().theme();
        let bg: gpui::Hsla = theme.panel_background.into();
        let border_color: gpui::Hsla = theme.border.into();
        let fg: gpui::Hsla = theme.foreground.into();

        let header_border = if hints.is_focused {
            let primary: gpui::Hsla = theme.primary.into();
            primary
        } else {
            border_color
        };

        let mut header = div()
            .id(SharedString::from(format!("terminal-header-{}", session_id.0)))
            .h(px(hints.height))
            .w_full()
            .bg(bg)
            .border_b_1()
            .border_color(header_border)
            .flex()
            .items_center()
            .px_2()
            .gap_2();

        // Color indicator bar
        let color_indicator: gpui::Hsla = hints.color_indicator.into();
        header = header.child(
            div()
                .w(px(3.0))
                .h(px(16.0))
                .rounded_sm()
                .bg(color_indicator),
        );

        // Status dot
        let status_color: gpui::Hsla = hints.status.color.into();
        header = header.child(
            div()
                .w(px(8.0))
                .h(px(8.0))
                .rounded_full()
                .bg(status_color),
        );

        // Session name
        header = header.child(
            div()
                .text_xs()
                .font_weight(FontWeight::MEDIUM)
                .text_color(fg)
                .overflow_hidden()
                .text_ellipsis()
                .child(hints.name.clone()),
        );

        // Spacer
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

        header
    }

    /// Render empty cell inline with pre-computed colors (returns Stateful<Div>).
    fn render_empty_cell_inline_with_colors(
        &mut self,
        position: codirigent_core::GridPosition,
        panel_bg: gpui::Hsla,
        border_color: gpui::Hsla,
        muted: gpui::Hsla,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        div()
            .id(SharedString::from(format!("empty-cell-{}-{}", position.row, position.col)))
            .flex_1()
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
                info!(?position, "Empty cell clicked");
                this.empty_cells.click(position);
                // Create a new session when clicking an empty cell
                this.create_session(cx);
            }))
            .child(
                div()
                    .text_xl()
                    .text_color(muted)
                    .child("+"),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child("Idle - Ready for next task"),
            )
    }

    /// Render empty cell inline (returns Stateful<Div>).
    fn render_empty_cell_inline(
        &mut self,
        position: codirigent_core::GridPosition,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let theme = self.workspace().theme();
        let panel_bg: gpui::Hsla = theme.panel_background.into();
        let border_color: gpui::Hsla = theme.border.into();
        let muted: gpui::Hsla = theme.muted.into();

        self.render_empty_cell_inline_with_colors(position, panel_bg, border_color, muted, cx)
    }
}
