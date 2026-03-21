//! GPUI action handlers for WorkspaceView.
//!
//! This module contains keyboard shortcut action handlers for:
//! - Session management (New, Close, Focus)
//! - Layout operations (Split, ClosePane, NextLayout)
//! - Sidebar toggle

use super::gpui::WorkspaceView;
use crate::app::*;
use codirigent_core::SplitDirection;
use gpui::{Context, Window};
use tracing::info;

impl WorkspaceView {
    /// Handle NewSession action.
    pub(super) fn handle_new_session(
        &mut self,
        _action: &NewSession,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("NewSession action triggered");
        self.create_session(cx);
    }

    /// Handle CloseSession action.
    pub(super) fn handle_close_session(
        &mut self,
        _action: &CloseSession,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("CloseSession action triggered");
        self.close_focused_session(cx);
    }

    /// Handle SplitHorizontal action.
    pub(super) fn handle_split_horizontal(
        &mut self,
        _action: &SplitHorizontal,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("SplitHorizontal action triggered");
        if let Some(slot) = self.workspace.split_pane(SplitDirection::Horizontal, 0.5) {
            info!(?slot, "Split pane horizontally, new slot created");
            self.mark_layout_cache_dirty();
            cx.notify();
        }
    }

    /// Handle SplitVertical action.
    pub(super) fn handle_split_vertical(
        &mut self,
        _action: &SplitVertical,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("SplitVertical action triggered");
        if let Some(slot) = self.workspace.split_pane(SplitDirection::Vertical, 0.5) {
            info!(?slot, "Split pane vertically, new slot created");
            self.mark_layout_cache_dirty();
            cx.notify();
        }
    }

    /// Handle ClosePane action.
    pub(super) fn handle_close_pane(
        &mut self,
        _action: &ClosePane,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("ClosePane action triggered");
        // Capture every tab in the focused pane before the layout removes it.
        let sessions_to_close = self.workspace.focused_pane_session_ids();

        if self.workspace.close_pane() {
            info!("Closed focused pane");
            // Close every session that belonged to the removed pane.
            if sessions_to_close.is_empty() {
                self.mark_layout_cache_dirty();
                cx.notify();
            } else {
                for id in sessions_to_close {
                    self.close_session(id, cx);
                }
            }
        }
    }

    /// Handle NextLayout action.
    pub(super) fn handle_next_layout(
        &mut self,
        _action: &NextLayout,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("NextLayout action triggered");
        self.next_layout(cx);
    }

    /// Handle ToggleSidebar action.
    pub(super) fn handle_toggle_sidebar(
        &mut self,
        _action: &ToggleSidebar,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("ToggleSidebar action triggered");
        self.toggle_sidebar(cx);
    }

    /// Handle ToggleTaskBoard action.
    pub(super) fn handle_toggle_task_board(
        &mut self,
        _action: &ToggleTaskBoard,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("ToggleTaskBoard action triggered");
        self.toggle_task_board(cx);
    }

    /// Handle QuickSwitch action.
    ///
    /// This is a placeholder — no dedicated session-picker UI exists yet.
    /// Toggles the sidebar as a stand-in until a real session picker is built.
    pub(super) fn handle_quick_switch(
        &mut self,
        _action: &QuickSwitch,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("QuickSwitch action triggered (toggling sidebar as placeholder)");
        self.toggle_sidebar(cx);
    }

    /// Handle SearchTerminal action.
    pub(super) fn handle_search_terminal(
        &mut self,
        _action: &SearchTerminal,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_terminal_search(cx);
    }
}

/// Generate FocusSession handler methods for WorkspaceView.
///
/// Each GPUI action type requires a distinct handler method signature.
/// This macro eliminates the repetition of nine identical handler bodies.
macro_rules! impl_focus_session_handlers {
    ($($num:literal => $action:ident => $handler:ident),+ $(,)?) => {
        impl WorkspaceView {
            $(
                pub(super) fn $handler(
                    &mut self,
                    _action: &$action,
                    _window: &mut Window,
                    cx: &mut Context<Self>,
                ) {
                    self.focus_session_number($num, cx);
                }
            )+
        }
    };
}

impl_focus_session_handlers! {
    1 => FocusSession1 => handle_focus_session1,
    2 => FocusSession2 => handle_focus_session2,
    3 => FocusSession3 => handle_focus_session3,
    4 => FocusSession4 => handle_focus_session4,
    5 => FocusSession5 => handle_focus_session5,
    6 => FocusSession6 => handle_focus_session6,
    7 => FocusSession7 => handle_focus_session7,
    8 => FocusSession8 => handle_focus_session8,
    9 => FocusSession9 => handle_focus_session9,
}
