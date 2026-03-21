//! UI actions for Codirigent.
//!
//! This module defines the actions that can be triggered by keyboard
//! shortcuts or other input methods.
//!
//! # Action Types
//!
//! Actions are grouped by functionality:
//! - Layout actions: Change grid layout, toggle sidebar
//! - Focus actions: Navigate between sessions
//! - Session actions: Create, close, rename sessions
//!
//! # Example
//!
//! ```
//! use codirigent_ui::actions::{NextLayout, FocusSession, ToggleSidebar};
//!
//! let action = NextLayout;
//! let focus = FocusSession { number: 1 };
//! let toggle = ToggleSidebar;
//! ```

use crate::layout::FocusDirection;

/// Quit the application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Quit;

/// Cycle to the next layout profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NextLayout;

/// Cycle to the previous layout profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreviousLayout;

/// Toggle sidebar visibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToggleSidebar;

/// Focus a specific session by number (1-9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FocusSession {
    /// Session number (1-based).
    pub number: usize,
}

impl FocusSession {
    /// Create a focus action for session 1.
    pub const fn session1() -> Self {
        Self { number: 1 }
    }

    /// Create a focus action for session 2.
    pub const fn session2() -> Self {
        Self { number: 2 }
    }

    /// Create a focus action for session 3.
    pub const fn session3() -> Self {
        Self { number: 3 }
    }

    /// Create a focus action for session 4.
    pub const fn session4() -> Self {
        Self { number: 4 }
    }

    /// Create a focus action for session 5.
    pub const fn session5() -> Self {
        Self { number: 5 }
    }

    /// Create a focus action for session 6.
    pub const fn session6() -> Self {
        Self { number: 6 }
    }

    /// Create a focus action for session 7.
    pub const fn session7() -> Self {
        Self { number: 7 }
    }

    /// Create a focus action for session 8.
    pub const fn session8() -> Self {
        Self { number: 8 }
    }

    /// Create a focus action for session 9.
    pub const fn session9() -> Self {
        Self { number: 9 }
    }
}

/// Focus the next session in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FocusNextSession;

/// Focus the previous session in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FocusPreviousSession;

/// Focus session in a direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FocusDirectionAction {
    /// Direction to focus.
    pub direction: FocusDirection,
}

impl FocusDirectionAction {
    /// Focus up.
    pub const fn up() -> Self {
        Self {
            direction: FocusDirection::Up,
        }
    }

    /// Focus down.
    pub const fn down() -> Self {
        Self {
            direction: FocusDirection::Down,
        }
    }

    /// Focus left.
    pub const fn left() -> Self {
        Self {
            direction: FocusDirection::Left,
        }
    }

    /// Focus right.
    pub const fn right() -> Self {
        Self {
            direction: FocusDirection::Right,
        }
    }
}

/// Create a new session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateSession {
    /// Optional name for the session.
    pub name: Option<String>,
}

impl CreateSession {
    /// Create action with auto-generated name.
    pub fn new() -> Self {
        Self { name: None }
    }

    /// Create action with a specific name.
    pub fn with_name(name: String) -> Self {
        Self { name: Some(name) }
    }
}

impl Default for CreateSession {
    fn default() -> Self {
        Self::new()
    }
}

/// Close the currently focused session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CloseSession;

/// Close a specific session by ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CloseSessionById {
    /// Session ID value.
    pub id: u64,
}

/// Rename the currently focused session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenameSession {
    /// New name for the session.
    pub name: String,
}

/// Send input to the focused session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendInput {
    /// Input data to send.
    pub data: Vec<u8>,
}

impl SendInput {
    /// Create from a string.
    pub fn from_text(s: &str) -> Self {
        Self {
            data: s.as_bytes().to_vec(),
        }
    }

    /// Create from bytes.
    pub fn from_bytes(data: Vec<u8>) -> Self {
        Self { data }
    }
}

/// Toggle maximized state for focused session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToggleMaximize;

/// Set layout to a specific profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetLayout {
    /// Layout index (0-4).
    pub index: usize,
}

impl SetLayout {
    /// Set to 2x2 grid.
    pub const fn grid_2x2() -> Self {
        Self { index: 0 }
    }

    /// Set to 1x4 stack.
    pub const fn stack_1x4() -> Self {
        Self { index: 1 }
    }

    /// Set to 2x3 grid.
    pub const fn grid_2x3() -> Self {
        Self { index: 2 }
    }

    /// Set to 3x3 grid.
    pub const fn grid_3x3() -> Self {
        Self { index: 3 }
    }

    /// Set to single view.
    pub const fn single() -> Self {
        Self { index: 4 }
    }
}

/// Open settings/preferences.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenSettings;

/// Split the focused pane horizontally (left-to-right).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SplitHorizontal;

/// Split the focused pane vertically (top-to-bottom).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SplitVertical;

/// Close the focused pane (unsplit), promoting its sibling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClosePane;

/// Open command palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenCommandPalette;

/// Reload configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReloadConfig;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quit_action() {
        let action = Quit;
        assert_eq!(action, Quit);
    }

    #[test]
    fn test_layout_actions() {
        let next = NextLayout;
        let prev = PreviousLayout;
        assert_eq!(next, NextLayout);
        assert_eq!(prev, PreviousLayout);
    }

    #[test]
    fn test_toggle_sidebar() {
        let action = ToggleSidebar;
        assert_eq!(action, ToggleSidebar);
    }

    #[test]
    fn test_focus_session() {
        let f1 = FocusSession::session1();
        assert_eq!(f1.number, 1);

        let f9 = FocusSession::session9();
        assert_eq!(f9.number, 9);

        let f5 = FocusSession { number: 5 };
        assert_eq!(f5.number, 5);
    }

    #[test]
    fn test_focus_next_previous() {
        let next = FocusNextSession;
        let prev = FocusPreviousSession;
        assert_eq!(next, FocusNextSession);
        assert_eq!(prev, FocusPreviousSession);
    }

    #[test]
    fn test_focus_direction() {
        let up = FocusDirectionAction::up();
        assert_eq!(up.direction, FocusDirection::Up);

        let down = FocusDirectionAction::down();
        assert_eq!(down.direction, FocusDirection::Down);

        let left = FocusDirectionAction::left();
        assert_eq!(left.direction, FocusDirection::Left);

        let right = FocusDirectionAction::right();
        assert_eq!(right.direction, FocusDirection::Right);
    }

    #[test]
    fn test_create_session() {
        let action = CreateSession::new();
        assert!(action.name.is_none());

        let action = CreateSession::with_name("Test".to_string());
        assert_eq!(action.name, Some("Test".to_string()));

        let action = CreateSession::default();
        assert!(action.name.is_none());
    }

    #[test]
    fn test_close_session() {
        let close = CloseSession;
        assert_eq!(close, CloseSession);

        let close_by_id = CloseSessionById { id: 42 };
        assert_eq!(close_by_id.id, 42);
    }

    #[test]
    fn test_rename_session() {
        let action = RenameSession {
            name: "New Name".to_string(),
        };
        assert_eq!(action.name, "New Name");
    }

    #[test]
    fn test_send_input() {
        let action = SendInput::from_text("hello");
        assert_eq!(action.data, b"hello");

        let action = SendInput::from_bytes(vec![0x1b, 0x5b, 0x41]);
        assert_eq!(action.data, vec![0x1b, 0x5b, 0x41]);
    }

    #[test]
    fn test_toggle_maximize() {
        let action = ToggleMaximize;
        assert_eq!(action, ToggleMaximize);
    }

    #[test]
    fn test_set_layout() {
        assert_eq!(SetLayout::grid_2x2().index, 0);
        assert_eq!(SetLayout::stack_1x4().index, 1);
        assert_eq!(SetLayout::grid_2x3().index, 2);
        assert_eq!(SetLayout::grid_3x3().index, 3);
        assert_eq!(SetLayout::single().index, 4);
    }

    #[test]
    fn test_split_pane_actions() {
        let h = SplitHorizontal;
        let v = SplitVertical;
        let c = ClosePane;
        assert_eq!(h, SplitHorizontal);
        assert_eq!(v, SplitVertical);
        assert_eq!(c, ClosePane);
    }

    #[test]
    fn test_other_actions() {
        let settings = OpenSettings;
        let palette = OpenCommandPalette;
        let reload = ReloadConfig;

        assert_eq!(settings, OpenSettings);
        assert_eq!(palette, OpenCommandPalette);
        assert_eq!(reload, ReloadConfig);
    }
}
