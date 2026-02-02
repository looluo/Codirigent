//! GPUI rendering implementation for Workspace.
//!
//! This module provides the GPUI View implementation for the workspace,
//! including rendering the grid of session panes with proper theming.
//!
//! # Architecture
//!
//! The `WorkspaceView` wraps a `Workspace` and provides:
//! - GPUI `Render` trait implementation for drawing the UI
//! - GPUI `Focusable` trait for keyboard focus management
//!
//! # Example
//!
//! ```ignore
//! use dirigent_ui::workspace::WorkspaceView;
//! use dirigent_ui::DirigentApp;
//!
//! // In a window context:
//! let workspace = WorkspaceView::new(app, cx);
//! ```

use super::core::Workspace;
use crate::theme::DirigentTheme;
use dirigent_core::{DefaultEventBus, DirigentEvent, EventBus, Session, SessionId};
use dirigent_detector::InputDetector;
use dirigent_session::DefaultSessionManager;
use gpui::{
    div, px, App, AppContext, Context, Entity, FocusHandle, Focusable, FontWeight,
    InteractiveElement, IntoElement, ParentElement, Render, Styled, Window,
};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tracing::info;

/// GPUI View wrapper for Workspace.
///
/// This is the main workspace view that renders the grid of session panes.
/// It wraps the core `Workspace` struct and provides GPUI rendering.
pub struct WorkspaceView {
    /// The underlying workspace state.
    workspace: Workspace,
    /// Focus handle for keyboard navigation.
    focus_handle: FocusHandle,
    /// Event bus for cross-module communication.
    event_bus: Arc<DefaultEventBus>,
    /// Next session ID counter.
    next_session_id: u64,
}

impl WorkspaceView {
    /// Create a new workspace view.
    ///
    /// # Arguments
    ///
    /// * `session_manager` - Session manager for PTY and session lifecycle (unused currently)
    /// * `detector` - Input detector for monitoring session status (unused currently)
    /// * `event_bus` - Event bus for cross-module communication
    /// * `theme` - Theme configuration
    /// * `cx` - GPUI context
    pub fn new(
        _session_manager: Arc<Mutex<DefaultSessionManager>>,
        _detector: Arc<Mutex<InputDetector>>,
        event_bus: Arc<DefaultEventBus>,
        theme: DirigentTheme,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut workspace = Workspace::new();
        workspace.set_theme(theme);

        // Note: Event subscription via spawn will be added in a future task
        // For now, the workspace view renders the current state

        Self {
            workspace,
            focus_handle: cx.focus_handle(),
            event_bus,
            next_session_id: 1,
        }
    }

    /// Create a new session.
    pub fn create_session(&mut self, cx: &mut Context<Self>) {
        let id = SessionId(self.next_session_id);
        self.next_session_id += 1;

        let name = format!("Session {}", id.0);
        let working_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/tmp"));

        let session = Session::new(id, name.clone(), working_dir);

        if self.workspace.add_session(session) {
            // Notify through event bus
            self.event_bus.publish(DirigentEvent::SessionCreated { id });
            info!(%name, "Created new session");
            cx.notify();
        }
    }

    /// Close the focused session.
    pub fn close_focused_session(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.workspace.focused_session_id() {
            self.workspace.remove_session(id);
            self.event_bus.publish(DirigentEvent::SessionClosed { id });
            info!(?id, "Closed session");
            cx.notify();
        }
    }

    /// Cycle to next layout.
    pub fn next_layout(&mut self, cx: &mut Context<Self>) {
        self.workspace.next_layout();
        self.event_bus.publish(DirigentEvent::LayoutChanged {
            mode: self.workspace.layout_profile().to_mode(),
        });
        cx.notify();
    }

    /// Toggle sidebar visibility.
    pub fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.workspace.toggle_sidebar();
        cx.notify();
    }

    /// Focus a session by number (1-9).
    pub fn focus_session_number(&mut self, number: usize, cx: &mut Context<Self>) {
        if self.workspace.focus_session_number(number) {
            if let Some(id) = self.workspace.focused_session_id() {
                self.event_bus.publish(DirigentEvent::SessionFocused { id });
            }
            cx.notify();
        }
    }

    /// Render the sidebar.
    fn render_sidebar(&self) -> impl IntoElement {
        let theme = self.workspace.theme();
        let sidebar_bg: gpui::Hsla = theme.sidebar_background.into();
        let border_color: gpui::Hsla = theme.border.into();
        let fg: gpui::Hsla = theme.foreground.into();

        let width = self.workspace.sidebar_width();
        let sessions = self.workspace.sessions();

        let mut sidebar = div()
            .w(px(width))
            .h_full()
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
            let is_focused = self.workspace.focused_session_id() == Some(session.id);
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

        // New session button
        let muted: gpui::Hsla = theme.muted.into();
        sidebar = sidebar.child(
            div()
                .h(px(44.0))
                .px_3()
                .border_t_1()
                .border_color(border_color)
                .flex()
                .items_center()
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
    fn render_grid(&self) -> impl IntoElement {
        let theme = self.workspace.theme();
        let panel_bg: gpui::Hsla = theme.panel_background.into();
        let border_color: gpui::Hsla = theme.border.into();
        let fg: gpui::Hsla = theme.foreground.into();

        let cells = self.workspace.cell_info();
        let profile = self.workspace.layout_profile();
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
                                            DirigentTheme::status_name(info.status)
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

impl Focusable for WorkspaceView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for WorkspaceView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.workspace.theme();
        let bg: gpui::Hsla = theme.background.into();

        let mut container = div()
            .size_full()
            .track_focus(&self.focus_handle(cx))
            .bg(bg)
            .flex()
            .flex_row();

        // Render sidebar if visible
        if self.workspace.is_sidebar_visible() {
            container = container.child(self.render_sidebar());
        }

        // Render grid
        container = container.child(
            div()
                .flex_1()
                .p(px(theme.grid_gap))
                .flex()
                .child(self.render_grid()),
        );

        container
    }
}

/// Create a complete workspace view with all components wired up.
///
/// # Arguments
///
/// * `session_manager` - Session manager for PTY and session lifecycle
/// * `detector` - Input detector for monitoring session status
/// * `event_bus` - Event bus for cross-module communication
/// * `theme` - Theme configuration
/// * `cx` - App context (from window creation callback)
///
/// # Returns
///
/// A GPUI Entity containing the workspace.
pub fn create_workspace_view<C: AppContext>(
    session_manager: Arc<Mutex<DefaultSessionManager>>,
    detector: Arc<Mutex<InputDetector>>,
    event_bus: Arc<DefaultEventBus>,
    theme: DirigentTheme,
    cx: &mut C,
) -> C::Result<Entity<WorkspaceView>> {
    cx.new(|cx| WorkspaceView::new(session_manager, detector, event_bus, theme, cx))
}

#[cfg(test)]
mod tests {
    // Note: Most tests require GPUI test infrastructure
    // These are documented for future implementation

    #[test]
    fn test_workspace_view_module_exists() {
        // Verify the module compiles correctly
        assert!(true);
    }
}
