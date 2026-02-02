//! Workspace view for Dirigent.
//!
//! This module provides the main workspace view that contains the grid
//! of session panes and manages layout switching.
//!
//! # Architecture
//!
//! The workspace is the root UI component that:
//! - Manages the current layout profile
//! - Tracks session assignments to grid cells
//! - Handles focus navigation between sessions
//! - Renders the grid of session panes
//!
//! # Example
//!
//! ```
//! use dirigent_ui::workspace::Workspace;
//! use dirigent_ui::layout::LayoutProfile;
//! use dirigent_core::{Session, SessionId};
//! use std::path::PathBuf;
//!
//! let mut workspace = Workspace::new();
//! workspace.set_layout(LayoutProfile::Grid2x2);
//!
//! let session = Session::new(SessionId(1), "Session 1".to_string(), PathBuf::from("/tmp"));
//! workspace.add_session(session);
//! ```

use crate::layout::{Bounds, FocusDirection, GridLayout, LayoutProfile, LayoutState, Point};
use crate::theme::DirigentTheme;
use dirigent_core::{Session, SessionId, SessionStatus};

/// Main workspace containing the grid of sessions.
///
/// The workspace is responsible for:
/// - Managing session layout and assignment
/// - Tracking which session is focused
/// - Handling layout profile changes
/// - Providing session navigation
#[derive(Debug)]
pub struct Workspace {
    /// Layout state (profile and assignments).
    layout_state: LayoutState,
    /// Sessions in the workspace.
    sessions: Vec<Session>,
    /// Theme configuration.
    theme: DirigentTheme,
    /// Whether the sidebar is visible.
    show_sidebar: bool,
    /// Sidebar width in pixels.
    sidebar_width: f32,
    /// Current workspace bounds.
    bounds: Bounds,
}

impl Default for Workspace {
    fn default() -> Self {
        Self::new()
    }
}

impl Workspace {
    /// Create a new workspace with default settings.
    pub fn new() -> Self {
        Self {
            layout_state: LayoutState::new(),
            sessions: Vec::new(),
            theme: DirigentTheme::default(),
            show_sidebar: true,
            sidebar_width: 200.0,
            bounds: Bounds::from_size(1280.0, 720.0),
        }
    }

    /// Create a new workspace with a specific layout profile.
    pub fn with_profile(profile: LayoutProfile) -> Self {
        Self {
            layout_state: LayoutState::with_profile(profile),
            sessions: Vec::new(),
            theme: DirigentTheme::default(),
            show_sidebar: true,
            sidebar_width: 200.0,
            bounds: Bounds::from_size(1280.0, 720.0),
        }
    }

    // --- Layout Management ---

    /// Get the current layout profile.
    pub fn layout_profile(&self) -> LayoutProfile {
        self.layout_state.profile()
    }

    /// Set the layout profile.
    pub fn set_layout(&mut self, profile: LayoutProfile) {
        self.layout_state.set_profile(profile);
    }

    /// Cycle to the next layout profile.
    pub fn next_layout(&mut self) {
        self.layout_state.next_profile();
    }

    /// Cycle to the previous layout profile.
    pub fn previous_layout(&mut self) {
        self.layout_state.previous_profile();
    }

    /// Get the grid layout calculator for the current state.
    pub fn grid_layout(&self) -> GridLayout {
        let grid_bounds = self.grid_bounds();
        GridLayout::from_profile(
            self.layout_state.profile(),
            grid_bounds,
            self.theme.grid_gap,
        )
    }

    // --- Session Management ---

    /// Get all sessions.
    pub fn sessions(&self) -> &[Session] {
        &self.sessions
    }

    /// Get a session by ID.
    pub fn session(&self, id: SessionId) -> Option<&Session> {
        self.sessions.iter().find(|s| s.id == id)
    }

    /// Get a mutable reference to a session by ID.
    pub fn session_mut(&mut self, id: SessionId) -> Option<&mut Session> {
        self.sessions.iter_mut().find(|s| s.id == id)
    }

    /// Add a session to the workspace.
    ///
    /// The session is automatically assigned to the next available grid slot.
    ///
    /// # Returns
    ///
    /// `true` if the session was added successfully.
    pub fn add_session(&mut self, session: Session) -> bool {
        let id = session.id;

        // Check if session already exists
        if self.sessions.iter().any(|s| s.id == id) {
            return false;
        }

        // Try to add to layout
        if !self.layout_state.add_session(id) {
            return false;
        }

        self.sessions.push(session);

        // Focus the new session if it's the first one
        if self.layout_state.focused_index().is_none() {
            self.layout_state.focus_index(0);
        }

        true
    }

    /// Remove a session from the workspace.
    ///
    /// # Returns
    ///
    /// The removed session, if found.
    pub fn remove_session(&mut self, id: SessionId) -> Option<Session> {
        // Remove from layout
        self.layout_state.remove_session(id);

        // Remove from sessions list
        if let Some(pos) = self.sessions.iter().position(|s| s.id == id) {
            Some(self.sessions.remove(pos))
        } else {
            None
        }
    }

    /// Update a session's status.
    pub fn update_session_status(&mut self, id: SessionId, status: SessionStatus) {
        if let Some(session) = self.session_mut(id) {
            session.status = status;
        }
    }

    /// Get the visible sessions (those assigned to grid cells).
    pub fn visible_sessions(&self) -> Vec<&Session> {
        self.layout_state
            .assignments()
            .iter()
            .filter_map(|&id| self.session(id))
            .collect()
    }

    /// Get the number of sessions that can still be added.
    pub fn available_slots(&self) -> usize {
        self.layout_state.profile().max_sessions() - self.layout_state.assignments().len()
    }

    // --- Focus Management ---

    /// Get the focused session ID.
    pub fn focused_session_id(&self) -> Option<SessionId> {
        self.layout_state.focused_session()
    }

    /// Get the focused session.
    pub fn focused_session(&self) -> Option<&Session> {
        self.focused_session_id().and_then(|id| self.session(id))
    }

    /// Focus a session by ID.
    ///
    /// # Returns
    ///
    /// `true` if the session was found and focused.
    pub fn focus_session(&mut self, id: SessionId) -> bool {
        self.layout_state.focus_session(id)
    }

    /// Focus a session by grid index (1-based, for keyboard shortcuts).
    ///
    /// # Returns
    ///
    /// `true` if the index is valid and session was focused.
    pub fn focus_session_number(&mut self, number: usize) -> bool {
        if number == 0 || number > self.layout_state.assignments().len() {
            return false;
        }
        self.layout_state.focus_index(number - 1);
        true
    }

    /// Focus the next session.
    pub fn focus_next(&mut self) {
        self.layout_state.focus_next();
    }

    /// Focus the previous session.
    pub fn focus_previous(&mut self) {
        self.layout_state.focus_previous();
    }

    /// Focus in a direction (for arrow key navigation).
    pub fn focus_direction(&mut self, direction: FocusDirection) {
        self.layout_state.focus_direction(direction);
    }

    // --- Sidebar ---

    /// Check if the sidebar is visible.
    pub fn is_sidebar_visible(&self) -> bool {
        self.show_sidebar
    }

    /// Toggle sidebar visibility.
    pub fn toggle_sidebar(&mut self) {
        self.show_sidebar = !self.show_sidebar;
    }

    /// Set sidebar visibility.
    pub fn set_sidebar_visible(&mut self, visible: bool) {
        self.show_sidebar = visible;
    }

    /// Get the sidebar width.
    pub fn sidebar_width(&self) -> f32 {
        self.sidebar_width
    }

    /// Set the sidebar width.
    pub fn set_sidebar_width(&mut self, width: f32) {
        self.sidebar_width = width.clamp(100.0, 400.0);
    }

    // --- Theme ---

    /// Get the current theme.
    pub fn theme(&self) -> &DirigentTheme {
        &self.theme
    }

    /// Set the theme.
    pub fn set_theme(&mut self, theme: DirigentTheme) {
        self.theme = theme;
    }

    // --- Bounds ---

    /// Set the workspace bounds (called on window resize).
    pub fn set_bounds(&mut self, bounds: Bounds) {
        self.bounds = bounds;
    }

    /// Get the workspace bounds.
    pub fn bounds(&self) -> Bounds {
        self.bounds
    }

    /// Get the bounds available for the grid (excluding sidebar).
    pub fn grid_bounds(&self) -> Bounds {
        if self.show_sidebar {
            let x = self.sidebar_width;
            let width = (self.bounds.size.width - self.sidebar_width).max(0.0);
            Bounds::new(x, self.bounds.origin.y, width, self.bounds.size.height)
        } else {
            self.bounds
        }
    }

    /// Get the sidebar bounds.
    pub fn sidebar_bounds(&self) -> Option<Bounds> {
        if self.show_sidebar {
            Some(Bounds::new(
                self.bounds.origin.x,
                self.bounds.origin.y,
                self.sidebar_width,
                self.bounds.size.height,
            ))
        } else {
            None
        }
    }

    // --- Cell Information ---

    /// Get the bounds for a session's cell.
    pub fn session_cell_bounds(&self, id: SessionId) -> Option<Bounds> {
        let index = self
            .layout_state
            .assignments()
            .iter()
            .position(|&s| s == id)?;
        self.grid_layout().cell_bounds_for_index(index)
    }

    /// Get the session at a point in the grid.
    pub fn session_at_point(&self, point: Point) -> Option<SessionId> {
        let grid_bounds = self.grid_bounds();

        // Check if point is in grid area
        if !grid_bounds.contains(point) {
            return None;
        }

        // Adjust point relative to grid origin
        let adjusted_point = Point::new(
            point.x - grid_bounds.origin.x,
            point.y - grid_bounds.origin.y,
        );

        let local_bounds = Bounds::from_size(grid_bounds.size.width, grid_bounds.size.height);
        let layout = GridLayout::from_profile(
            self.layout_state.profile(),
            local_bounds,
            self.theme.grid_gap,
        );

        let position = layout.cell_at_point(adjusted_point)?;
        self.layout_state.session_at_position(position)
    }

    // --- State for Rendering ---

    /// Get information about each visible cell for rendering.
    pub fn cell_info(&self) -> Vec<CellInfo> {
        let layout = self.grid_layout();
        let focused_id = self.focused_session_id();

        self.layout_state
            .assignments()
            .iter()
            .enumerate()
            .filter_map(|(index, &session_id)| {
                let bounds = layout.cell_bounds_for_index(index)?;
                let session = self.session(session_id)?;

                Some(CellInfo {
                    session_id,
                    index,
                    bounds,
                    name: session.name.clone(),
                    status: session.status,
                    is_focused: focused_id == Some(session_id),
                })
            })
            .collect()
    }
}

/// Information about a grid cell for rendering.
#[derive(Debug, Clone)]
pub struct CellInfo {
    /// Session ID.
    pub session_id: SessionId,
    /// Grid index (0-based).
    pub index: usize,
    /// Cell bounds.
    pub bounds: Bounds,
    /// Session name.
    pub name: String,
    /// Session status.
    pub status: SessionStatus,
    /// Whether this cell is focused.
    pub is_focused: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_session(id: u64, name: &str) -> Session {
        Session::new(SessionId(id), name.to_string(), PathBuf::from("/tmp"))
    }

    #[test]
    fn test_workspace_new() {
        let ws = Workspace::new();
        assert_eq!(ws.layout_profile(), LayoutProfile::Grid2x2);
        assert!(ws.sessions().is_empty());
        assert!(ws.is_sidebar_visible());
    }

    #[test]
    fn test_workspace_with_profile() {
        let ws = Workspace::with_profile(LayoutProfile::Grid3x3);
        assert_eq!(ws.layout_profile(), LayoutProfile::Grid3x3);
    }

    #[test]
    fn test_workspace_layout_cycle() {
        let mut ws = Workspace::new();
        ws.next_layout();
        assert_eq!(ws.layout_profile(), LayoutProfile::Stack1x4);
        ws.previous_layout();
        assert_eq!(ws.layout_profile(), LayoutProfile::Grid2x2);
    }

    #[test]
    fn test_workspace_set_layout() {
        let mut ws = Workspace::new();
        ws.set_layout(LayoutProfile::Single);
        assert_eq!(ws.layout_profile(), LayoutProfile::Single);
    }

    #[test]
    fn test_workspace_add_session() {
        let mut ws = Workspace::new();
        let session = make_session(1, "Session 1");

        assert!(ws.add_session(session));
        assert_eq!(ws.sessions().len(), 1);
        assert_eq!(ws.focused_session_id(), Some(SessionId(1)));
    }

    #[test]
    fn test_workspace_add_duplicate_session() {
        let mut ws = Workspace::new();
        ws.add_session(make_session(1, "Session 1"));

        // Adding duplicate should fail
        assert!(!ws.add_session(make_session(1, "Session 1 again")));
        assert_eq!(ws.sessions().len(), 1);
    }

    #[test]
    fn test_workspace_add_session_full() {
        let mut ws = Workspace::with_profile(LayoutProfile::Grid2x2);

        // Fill all 4 slots
        for i in 1..=4 {
            assert!(ws.add_session(make_session(i, &format!("Session {}", i))));
        }

        // 5th should fail
        assert!(!ws.add_session(make_session(5, "Session 5")));
        assert_eq!(ws.sessions().len(), 4);
    }

    #[test]
    fn test_workspace_remove_session() {
        let mut ws = Workspace::new();
        ws.add_session(make_session(1, "Session 1"));
        ws.add_session(make_session(2, "Session 2"));

        let removed = ws.remove_session(SessionId(1));
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().id, SessionId(1));
        assert_eq!(ws.sessions().len(), 1);

        // Remove non-existent
        assert!(ws.remove_session(SessionId(99)).is_none());
    }

    #[test]
    fn test_workspace_session_access() {
        let mut ws = Workspace::new();
        ws.add_session(make_session(1, "Session 1"));

        assert!(ws.session(SessionId(1)).is_some());
        assert!(ws.session(SessionId(99)).is_none());

        // Mutable access
        assert!(ws.session_mut(SessionId(1)).is_some());
    }

    #[test]
    fn test_workspace_update_status() {
        let mut ws = Workspace::new();
        ws.add_session(make_session(1, "Session 1"));

        ws.update_session_status(SessionId(1), SessionStatus::Working);
        assert_eq!(
            ws.session(SessionId(1)).unwrap().status,
            SessionStatus::Working
        );
    }

    #[test]
    fn test_workspace_visible_sessions() {
        let mut ws = Workspace::new();
        ws.add_session(make_session(1, "Session 1"));
        ws.add_session(make_session(2, "Session 2"));

        let visible = ws.visible_sessions();
        assert_eq!(visible.len(), 2);
    }

    #[test]
    fn test_workspace_available_slots() {
        let mut ws = Workspace::with_profile(LayoutProfile::Grid2x2);
        assert_eq!(ws.available_slots(), 4);

        ws.add_session(make_session(1, "Session 1"));
        assert_eq!(ws.available_slots(), 3);
    }

    #[test]
    fn test_workspace_focus_session() {
        let mut ws = Workspace::new();
        ws.add_session(make_session(1, "Session 1"));
        ws.add_session(make_session(2, "Session 2"));

        assert!(ws.focus_session(SessionId(2)));
        assert_eq!(ws.focused_session_id(), Some(SessionId(2)));

        assert!(!ws.focus_session(SessionId(99)));
    }

    #[test]
    fn test_workspace_focus_session_number() {
        let mut ws = Workspace::new();
        ws.add_session(make_session(1, "Session 1"));
        ws.add_session(make_session(2, "Session 2"));

        assert!(ws.focus_session_number(2));
        assert_eq!(ws.focused_session_id(), Some(SessionId(2)));

        // Invalid numbers
        assert!(!ws.focus_session_number(0));
        assert!(!ws.focus_session_number(10));
    }

    #[test]
    fn test_workspace_focus_navigation() {
        let mut ws = Workspace::new();
        ws.add_session(make_session(1, "Session 1"));
        ws.add_session(make_session(2, "Session 2"));
        ws.add_session(make_session(3, "Session 3"));

        assert_eq!(ws.focused_session_id(), Some(SessionId(1)));

        ws.focus_next();
        assert_eq!(ws.focused_session_id(), Some(SessionId(2)));

        ws.focus_previous();
        assert_eq!(ws.focused_session_id(), Some(SessionId(1)));
    }

    #[test]
    fn test_workspace_focus_direction() {
        let mut ws = Workspace::with_profile(LayoutProfile::Grid2x2);
        ws.add_session(make_session(1, "S1"));
        ws.add_session(make_session(2, "S2"));
        ws.add_session(make_session(3, "S3"));
        ws.add_session(make_session(4, "S4"));

        // Start at top-left (index 0)
        ws.focus_direction(FocusDirection::Right);
        assert_eq!(ws.focused_session_id(), Some(SessionId(2)));

        ws.focus_direction(FocusDirection::Down);
        assert_eq!(ws.focused_session_id(), Some(SessionId(4)));
    }

    #[test]
    fn test_workspace_focused_session() {
        let mut ws = Workspace::new();
        ws.add_session(make_session(1, "Session 1"));

        let focused = ws.focused_session();
        assert!(focused.is_some());
        assert_eq!(focused.unwrap().id, SessionId(1));
    }

    #[test]
    fn test_workspace_sidebar() {
        let mut ws = Workspace::new();

        assert!(ws.is_sidebar_visible());

        ws.toggle_sidebar();
        assert!(!ws.is_sidebar_visible());

        ws.toggle_sidebar();
        assert!(ws.is_sidebar_visible());

        ws.set_sidebar_visible(false);
        assert!(!ws.is_sidebar_visible());
    }

    #[test]
    fn test_workspace_sidebar_width() {
        let mut ws = Workspace::new();

        ws.set_sidebar_width(250.0);
        assert_eq!(ws.sidebar_width(), 250.0);

        // Clamp to min
        ws.set_sidebar_width(50.0);
        assert_eq!(ws.sidebar_width(), 100.0);

        // Clamp to max
        ws.set_sidebar_width(500.0);
        assert_eq!(ws.sidebar_width(), 400.0);
    }

    #[test]
    fn test_workspace_theme() {
        let mut ws = Workspace::new();

        let theme = DirigentTheme::light();
        ws.set_theme(theme.clone());
        assert_eq!(*ws.theme(), theme);
    }

    #[test]
    fn test_workspace_bounds() {
        let mut ws = Workspace::new();

        let new_bounds = Bounds::from_size(1920.0, 1080.0);
        ws.set_bounds(new_bounds);

        assert_eq!(ws.bounds().size.width, 1920.0);
        assert_eq!(ws.bounds().size.height, 1080.0);
    }

    #[test]
    fn test_workspace_grid_bounds_with_sidebar() {
        let mut ws = Workspace::new();
        ws.set_bounds(Bounds::from_size(1000.0, 800.0));
        ws.set_sidebar_width(200.0);

        let grid_bounds = ws.grid_bounds();
        assert_eq!(grid_bounds.origin.x, 200.0);
        assert_eq!(grid_bounds.size.width, 800.0);
    }

    #[test]
    fn test_workspace_grid_bounds_without_sidebar() {
        let mut ws = Workspace::new();
        ws.set_bounds(Bounds::from_size(1000.0, 800.0));
        ws.set_sidebar_visible(false);

        let grid_bounds = ws.grid_bounds();
        assert_eq!(grid_bounds.origin.x, 0.0);
        assert_eq!(grid_bounds.size.width, 1000.0);
    }

    #[test]
    fn test_workspace_sidebar_bounds() {
        let mut ws = Workspace::new();
        ws.set_bounds(Bounds::from_size(1000.0, 800.0));
        ws.set_sidebar_width(200.0);

        let sidebar = ws.sidebar_bounds().unwrap();
        assert_eq!(sidebar.origin.x, 0.0);
        assert_eq!(sidebar.size.width, 200.0);

        ws.set_sidebar_visible(false);
        assert!(ws.sidebar_bounds().is_none());
    }

    #[test]
    fn test_workspace_session_cell_bounds() {
        let mut ws = Workspace::new();
        ws.set_bounds(Bounds::from_size(1000.0, 800.0));
        ws.set_sidebar_visible(false);
        ws.add_session(make_session(1, "Session 1"));

        let bounds = ws.session_cell_bounds(SessionId(1));
        assert!(bounds.is_some());
        assert!(bounds.unwrap().size.width > 0.0);

        assert!(ws.session_cell_bounds(SessionId(99)).is_none());
    }

    #[test]
    fn test_workspace_session_at_point() {
        let mut ws = Workspace::with_profile(LayoutProfile::Grid2x2);
        ws.set_bounds(Bounds::from_size(1000.0, 800.0));
        ws.set_sidebar_visible(false);

        ws.add_session(make_session(1, "S1"));
        ws.add_session(make_session(2, "S2"));

        // Point in first cell
        let id = ws.session_at_point(Point::new(100.0, 100.0));
        assert_eq!(id, Some(SessionId(1)));

        // Point in second cell (right side)
        let id = ws.session_at_point(Point::new(600.0, 100.0));
        assert_eq!(id, Some(SessionId(2)));
    }

    #[test]
    fn test_workspace_cell_info() {
        let mut ws = Workspace::new();
        ws.set_bounds(Bounds::from_size(1000.0, 800.0));
        ws.add_session(make_session(1, "Session 1"));
        ws.add_session(make_session(2, "Session 2"));

        let cells = ws.cell_info();
        assert_eq!(cells.len(), 2);

        assert_eq!(cells[0].session_id, SessionId(1));
        assert!(cells[0].is_focused);
        assert_eq!(cells[0].name, "Session 1");

        assert_eq!(cells[1].session_id, SessionId(2));
        assert!(!cells[1].is_focused);
    }

    #[test]
    fn test_cell_info_fields() {
        let info = CellInfo {
            session_id: SessionId(1),
            index: 0,
            bounds: Bounds::from_size(100.0, 100.0),
            name: "Test".to_string(),
            status: SessionStatus::Working,
            is_focused: true,
        };

        assert_eq!(info.session_id, SessionId(1));
        assert_eq!(info.index, 0);
        assert_eq!(info.name, "Test");
        assert_eq!(info.status, SessionStatus::Working);
        assert!(info.is_focused);
    }

    #[test]
    fn test_workspace_grid_layout() {
        let mut ws = Workspace::with_profile(LayoutProfile::Grid2x2);
        ws.set_bounds(Bounds::from_size(1000.0, 800.0));
        ws.set_sidebar_visible(false);

        let layout = ws.grid_layout();
        assert_eq!(layout.dimensions(), (2, 2));
        assert_eq!(layout.cell_count(), 4);
    }
}
