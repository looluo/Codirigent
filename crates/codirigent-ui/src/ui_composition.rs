//! UI Composition Guide
//!
//! This module provides documentation and design constants for composing
//! all the UI components together into a complete application.
//!
//! # Component Overview
//!
//! The UI consists of the following components:
//!
//! ## Layout Structure
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                        TitleBar                                  │
//! │  [●●●] CODIRIGENT            (drag area)          [─] [□] [✕]  │
//! ├─────────────┬───────────────────────────────────────────────────┤
//! │  Sidebar    │              SessionsToolbar                       │
//! │             │  [2×2] [3×2] [3×3] [Custom]    [Broadcast] [+ New] │
//! │  Sessions   ├───────────────────────────────────────────────────┤
//! │  ● Sess 1   │     ┌──────────┬──────────┬──────────┐            │
//! │  ● Sess 2   │     │TermHeader│ TermHeader│ Terminal │            │
//! │  ● Sess 3   │     │ Terminal │ Terminal │  Header  │            │
//! │             │     │          │          │          │            │
//! │  Files      │     ├──────────┼──────────┼──────────┤            │
//! │  📁 src/    │     │ Empty    │TermHeader│ Terminal │            │
//! │    📄 main  │     │ Session  │ Terminal │  Header  │            │
//! │             │     │  Cell    │          │          │            │
//! │             │     └──────────┴──────────┴──────────┘            │
//! ├─────────────┴───────────────────────────────────────────────────┤
//! │                     TaskBoardPanel                               │
//! │  [Queue] [In Progress] [Review] [Done]     [Auto-assign] [+ Add]│
//! │  ┌─────────────────────────────────────────────────────────────┐│
//! │  │ TaskItem | TaskItem | TaskItem                              ││
//! │  └─────────────────────────────────────────────────────────────┘│
//! ├─────────────────────────────────────────────────────────────────┤
//! │ StatusBar: ● 4 sessions | Working: 2 | Waiting: 1    v0.1.0    │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Components
//!
//! - [`title_bar::TitleBar`] - Logo and drag area
//! - [`sidebar::SessionSidebar`] - Session list with status indicators
//! - [`sidebar::FileTreePanel`] - File tree with expandable folders
//! - [`toolbar::SessionsToolbar`] - Layout tabs and new session button
//! - [`terminal_header::TerminalHeader`] - Session header with status and context
//! - [`empty_session::EmptySessionCell`] - Placeholder for empty grid slots
//! - [`task_board::TaskBoardPanel`] - Task queue management panel
//! - [`status_bar::StatusBar`] - Status information bar

/// Color palette from the mockup design.
///
/// Use these colors for consistency across all components.
pub mod colors {
    /// Darkest background color.
    pub const BACKGROUND: &str = "#0a0a0c";
    /// Panel background color.
    pub const PANEL_BG: &str = "#0d0d10";
    /// Header background color.
    pub const HEADER_BG: &str = "#141418";
    /// Border color.
    pub const BORDER: &str = "#1a1a1f";
    /// Hover state color.
    pub const HOVER: &str = "#151518";
    /// Active state color.
    pub const ACTIVE: &str = "#1a1a22";

    /// Primary accent color (GitHub commit green).
    pub const PRIMARY: &str = "#39d353";
    /// Secondary accent color (blue).
    pub const SECONDARY: &str = "#5B8DEF";
    /// Purple accent.
    pub const PURPLE: &str = "#A78BFA";
    /// Orange accent.
    pub const ORANGE: &str = "#F59E0B";
    /// Error/waiting color (red).
    pub const ERROR: &str = "#FF6B6B";
    /// Warning color (yellow).
    pub const WARNING: &str = "#febc2e";

    /// Primary text color.
    pub const TEXT_PRIMARY: &str = "#e0e0e0";
    /// Secondary text color.
    pub const TEXT_SECONDARY: &str = "#888888";
    /// Muted text color.
    pub const TEXT_MUTED: &str = "#666666";
}

/// Layout constants for the UI.
pub mod layout {
    /// Default title bar height.
    pub const TITLE_BAR_HEIGHT: f32 = 32.0;
    /// Default sidebar width.
    pub const SIDEBAR_WIDTH: f32 = 220.0;
    /// Default status bar height.
    pub const STATUS_BAR_HEIGHT: f32 = 24.0;
    /// Default task board collapsed height.
    pub const TASK_BOARD_COLLAPSED: f32 = 40.0;
    /// Default task board expanded height.
    pub const TASK_BOARD_EXPANDED: f32 = 200.0;
    /// Default terminal header height.
    pub const TERMINAL_HEADER_HEIGHT: f32 = 32.0;
    /// Default grid gap.
    pub const GRID_GAP: f32 = 4.0;
    /// Default toolbar height.
    pub const TOOLBAR_HEIGHT: f32 = 44.0;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_constants() {
        assert_eq!(colors::BACKGROUND, "#0a0a0c");
        assert_eq!(colors::PRIMARY, "#39d353");
        assert_eq!(colors::TEXT_PRIMARY, "#e0e0e0");
    }

    #[test]
    fn test_layout_constants() {
        let title_bar = layout::TITLE_BAR_HEIGHT;
        let sidebar = layout::SIDEBAR_WIDTH;
        let status_bar = layout::STATUS_BAR_HEIGHT;
        let grid_gap = layout::GRID_GAP;
        assert!(title_bar > 0.0);
        assert!(sidebar > 0.0);
        assert!(status_bar > 0.0);
        assert!(grid_gap > 0.0);
    }
}
