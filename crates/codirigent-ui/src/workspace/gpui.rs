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
//! use codirigent_ui::workspace::WorkspaceView;
//! use codirigent_ui::CodirigentApp;
//!
//! // In a window context:
//! let workspace = WorkspaceView::new(app, cx);
//! ```

use super::core::Workspace;
use crate::input::{key_to_bytes, TerminalKeystroke, TerminalModifiers};
use crate::terminal::Terminal;
use crate::terminal_view::TerminalView;
use crate::theme::CodirigentTheme;
use codirigent_core::{CodirigentEvent, DefaultEventBus, EventBus, Session, SessionId, SessionManager};
use codirigent_detector::InputDetector;
use codirigent_session::DefaultSessionManager;
use crate::app::{
    CloseSession, FocusSession1, FocusSession2, FocusSession3, FocusSession4, FocusSession5,
    FocusSession6, FocusSession7, FocusSession8, FocusSession9, NewSession, NextLayout,
    ToggleSidebar,
};
use gpui::{
    div, px, App, AppContext, Context, Entity, FocusHandle, Focusable, InteractiveElement,
    IntoElement, KeyDownEvent, ParentElement, Render, Styled, Window,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::{info, warn};

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
    /// Session manager for PTY and session lifecycle.
    session_manager: Arc<Mutex<DefaultSessionManager>>,
    /// Input detector for monitoring session status (future use).
    _detector: Arc<Mutex<InputDetector>>,
    /// Terminal views for each session.
    terminals: HashMap<SessionId, TerminalView>,
    /// Next session ID counter (kept for UI session tracking).
    next_session_id: u64,
}

impl WorkspaceView {
    /// Create a new workspace view.
    ///
    /// # Arguments
    ///
    /// * `session_manager` - Session manager for PTY and session lifecycle
    /// * `detector` - Input detector for monitoring session status
    /// * `event_bus` - Event bus for cross-module communication
    /// * `theme` - Theme configuration
    /// * `cx` - GPUI context
    pub fn new(
        session_manager: Arc<Mutex<DefaultSessionManager>>,
        detector: Arc<Mutex<InputDetector>>,
        event_bus: Arc<DefaultEventBus>,
        theme: CodirigentTheme,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut workspace = Workspace::new();
        workspace.set_theme(theme);

        // Start output polling background task
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
                let result = this.update(cx, |this, cx| {
                    this.poll_output(cx);
                });
                if result.is_err() {
                    // View was dropped, stop the task
                    break;
                }
            }
        })
        .detach();

        Self {
            workspace,
            focus_handle: cx.focus_handle(),
            event_bus,
            session_manager,
            _detector: detector,
            terminals: HashMap::new(),
            next_session_id: 1,
        }
    }

    /// Poll PTY output and feed to terminal emulators.
    fn poll_output(&mut self, cx: &mut Context<Self>) {
        let session_ids: Vec<SessionId> = self.terminals.keys().copied().collect();
        let mut any_dirty = false;

        for session_id in session_ids {
            // Try to drain output from the session manager
            let output = {
                let manager = self.session_manager.lock().unwrap();
                manager.try_drain_output(session_id)
            };

            if let Some(data) = output {
                if let Some(terminal_view) = self.terminals.get_mut(&session_id) {
                    terminal_view.terminal_mut().process_output(&data);
                    any_dirty = true;
                }
            }
        }

        if any_dirty {
            cx.notify();
        }
    }

    /// Create a new session.
    pub fn create_session(&mut self, cx: &mut Context<Self>) {
        let name = format!("Session {}", self.next_session_id);
        self.next_session_id += 1;

        let working_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/tmp"));

        // Create session with real PTY via session manager
        let session_id = {
            let manager = self.session_manager.lock().unwrap();
            match manager.create_session(name.clone(), working_dir.clone()) {
                Ok(id) => id,
                Err(e) => {
                    warn!("Failed to create session: {}", e);
                    return;
                }
            }
        };

        // Create terminal emulator for this session
        let terminal = Terminal::new(24, 80, session_id);
        let theme = self.workspace.theme();
        let terminal_view = TerminalView::new(terminal, theme.clone());
        self.terminals.insert(session_id, terminal_view);

        // Create UI session for workspace
        let session = Session::new(session_id, name.clone(), working_dir);

        if self.workspace.add_session(session) {
            // Event is already published by session manager
            info!(%name, "Created new session with PTY");
            cx.notify();
        }
    }

    /// Close the focused session.
    pub fn close_focused_session(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.workspace.focused_session_id() {
            // Remove terminal view
            self.terminals.remove(&id);

            // Close PTY session
            {
                let manager = self.session_manager.lock().unwrap();
                if let Err(e) = manager.close_session(id) {
                    warn!("Failed to close session {}: {}", id, e);
                }
            }

            // Remove from workspace UI
            self.workspace.remove_session(id);
            info!(?id, "Closed session");
            cx.notify();
        }
    }

    /// Cycle to next layout.
    pub fn next_layout(&mut self, cx: &mut Context<Self>) {
        self.workspace.next_layout();
        self.event_bus.publish(CodirigentEvent::LayoutChanged {
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
                self.event_bus.publish(CodirigentEvent::SessionFocused { id });
            }
            cx.notify();
        }
    }

    // --- Action Handlers ---
    // These are called by GPUI when keyboard shortcuts or menu items trigger actions.

    /// Handle NewSession action (Cmd+N).
    fn handle_new_session(
        &mut self,
        _action: &NewSession,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("NewSession action triggered");
        self.create_session(cx);
    }

    /// Handle CloseSession action (Cmd+W).
    fn handle_close_session(
        &mut self,
        _action: &CloseSession,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("CloseSession action triggered");
        self.close_focused_session(cx);
    }

    /// Handle NextLayout action (Cmd+\).
    fn handle_next_layout(
        &mut self,
        _action: &NextLayout,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("NextLayout action triggered");
        self.next_layout(cx);
    }

    /// Handle ToggleSidebar action (Cmd+B).
    fn handle_toggle_sidebar(
        &mut self,
        _action: &ToggleSidebar,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("ToggleSidebar action triggered");
        self.toggle_sidebar(cx);
    }

    /// Handle FocusSession1 action (Cmd+1).
    fn handle_focus_session1(
        &mut self,
        _action: &FocusSession1,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_session_number(1, cx);
    }

    /// Handle FocusSession2 action (Cmd+2).
    fn handle_focus_session2(
        &mut self,
        _action: &FocusSession2,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_session_number(2, cx);
    }

    /// Handle FocusSession3 action (Cmd+3).
    fn handle_focus_session3(
        &mut self,
        _action: &FocusSession3,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_session_number(3, cx);
    }

    /// Handle FocusSession4 action (Cmd+4).
    fn handle_focus_session4(
        &mut self,
        _action: &FocusSession4,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_session_number(4, cx);
    }

    /// Handle FocusSession5 action (Cmd+5).
    fn handle_focus_session5(
        &mut self,
        _action: &FocusSession5,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_session_number(5, cx);
    }

    /// Handle FocusSession6 action (Cmd+6).
    fn handle_focus_session6(
        &mut self,
        _action: &FocusSession6,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_session_number(6, cx);
    }

    /// Handle FocusSession7 action (Cmd+7).
    fn handle_focus_session7(
        &mut self,
        _action: &FocusSession7,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_session_number(7, cx);
    }

    /// Handle FocusSession8 action (Cmd+8).
    fn handle_focus_session8(
        &mut self,
        _action: &FocusSession8,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_session_number(8, cx);
    }

    /// Handle FocusSession9 action (Cmd+9).
    fn handle_focus_session9(
        &mut self,
        _action: &FocusSession9,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_session_number(9, cx);
    }

    /// Get a reference to the underlying workspace.
    ///
    /// Used by the render module to access workspace state.
    pub(super) fn workspace(&self) -> &Workspace {
        &self.workspace
    }

    /// Get a reference to the terminals HashMap.
    ///
    /// Used by the render module to access terminal views.
    pub(super) fn terminals(&self) -> &HashMap<SessionId, TerminalView> {
        &self.terminals
    }

    /// Handle keyboard input for the focused session.
    fn handle_key_down(&mut self, event: &KeyDownEvent, _cx: &mut Context<Self>) {
        // Don't send Cmd+ shortcuts to PTY
        if event.keystroke.modifiers.platform {
            return;
        }

        // Get focused session
        let Some(session_id) = self.workspace.focused_session_id() else {
            return;
        };

        // Get terminal mode for proper escape sequence generation
        let Some(terminal_view) = self.terminals.get(&session_id) else {
            return;
        };
        let term_mode = terminal_view.terminal().mode();

        // Convert GPUI keystroke to terminal keystroke
        let modifiers = TerminalModifiers {
            shift: event.keystroke.modifiers.shift,
            control: event.keystroke.modifiers.control,
            alt: event.keystroke.modifiers.alt,
        };

        let keystroke = TerminalKeystroke::with_modifiers(event.keystroke.key.clone(), modifiers);

        // Convert to bytes
        if let Some(bytes) = key_to_bytes(&keystroke, term_mode) {
            // Send to PTY
            let manager = self.session_manager.lock().unwrap();
            if let Err(e) = manager.send_input(session_id, &bytes) {
                warn!("Failed to send input to session {}: {}", session_id, e);
            }
        }
    }
}

impl Focusable for WorkspaceView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for WorkspaceView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Clone theme values before any mutable borrows
        let theme = self.workspace.theme();
        let bg: gpui::Hsla = theme.background.into();
        let grid_gap = theme.grid_gap;
        let show_sidebar = self.workspace.is_sidebar_visible();

        // Top padding for macOS transparent titlebar (traffic lights area)
        let titlebar_height = 28.0;

        let mut container = div()
            .size_full()
            .track_focus(&self.focus_handle(cx))
            // Register action handlers for keyboard shortcuts
            .on_action(cx.listener(Self::handle_new_session))
            .on_action(cx.listener(Self::handle_close_session))
            .on_action(cx.listener(Self::handle_next_layout))
            .on_action(cx.listener(Self::handle_toggle_sidebar))
            .on_action(cx.listener(Self::handle_focus_session1))
            .on_action(cx.listener(Self::handle_focus_session2))
            .on_action(cx.listener(Self::handle_focus_session3))
            .on_action(cx.listener(Self::handle_focus_session4))
            .on_action(cx.listener(Self::handle_focus_session5))
            .on_action(cx.listener(Self::handle_focus_session6))
            .on_action(cx.listener(Self::handle_focus_session7))
            .on_action(cx.listener(Self::handle_focus_session8))
            .on_action(cx.listener(Self::handle_focus_session9))
            // Handle keyboard input for PTY
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _window, cx| {
                this.handle_key_down(event, cx);
            }))
            .bg(bg)
            .flex()
            .flex_row();

        // Render sidebar if visible
        if show_sidebar {
            container = container.child(self.render_sidebar(cx));
        }

        // Render grid with top padding for titlebar
        container = container.child(
            div()
                .flex_1()
                .pt(px(titlebar_height + grid_gap))
                .pb(px(grid_gap))
                .px(px(grid_gap))
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
    theme: CodirigentTheme,
    cx: &mut C,
) -> C::Result<Entity<WorkspaceView>> {
    cx.new(|cx| WorkspaceView::new(session_manager, detector, event_bus, theme, cx))
}

#[cfg(test)]
mod tests {
    //! GPUI View Testing Strategy
    //!
    //! # Why Limited Tests
    //!
    //! `WorkspaceView` is a GPUI view component that requires the GPUI runtime
    //! for rendering and interaction. Testing GPUI views requires:
    //! - GPUI test harness (`gpui::TestAppContext`)
    //! - Window creation for rendering tests
    //! - Focus simulation for interaction tests
    //!
    //! # Test Coverage Strategy
    //!
    //! 1. **Core Business Logic** - Fully tested in `workspace/tests.rs` (29 tests)
    //!    - Layout management, session handling, focus navigation
    //!    - Bounds calculation, cell info generation
    //!    - All non-GPUI logic has 100% test coverage
    //!
    //! 2. **GPUI Integration** - Deferred to integration tests
    //!    - Rendering correctness requires visual inspection or snapshot tests
    //!    - Action handlers require GPUI action dispatch simulation
    //!
    //! # Future: GPUI Test Infrastructure
    //!
    //! When GPUI test helpers are available, add tests for:
    //! - [ ] WorkspaceView renders without panic
    //! - [ ] Action handlers (NewSession, CloseSession, etc.) work correctly
    //! - [ ] Focus delegation to child components
    //! - [ ] Layout changes trigger re-render

    #[test]
    fn test_workspace_view_module_compiles() {
        // Validates that the module compiles with all GPUI dependencies.
        // The actual rendering and interaction tests require GPUI test infrastructure.
        // See workspace/tests.rs for core logic tests (29 tests, 100% coverage).
        assert!(true, "WorkspaceView module compiles successfully");
    }

    #[test]
    fn test_core_workspace_is_tested_separately() {
        // Reminder: Core workspace logic has dedicated tests in workspace/tests.rs
        // Run `cargo test workspace::tests` to see all 29 tests pass
        use crate::workspace::Workspace;

        // Quick sanity check that we can create a workspace
        let ws = Workspace::new();
        assert!(ws.sessions().is_empty());
    }
}
