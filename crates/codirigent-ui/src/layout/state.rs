//! Layout state management for the workspace.
//!
//! Provides [`LayoutState`] for grid-based layouts, [`SplitLayoutState`] for
//! split tree layouts, [`WorkspaceLayoutState`] as a unified wrapper, and
//! [`FocusDirection`] for directional focus navigation.

use codirigent_core::{GridPosition, LayoutNode, SessionId, SlotId, SplitDirection};

use super::profile::LayoutProfile;
use super::split::SplitLayout;

/// Direction for focus navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusDirection {
    /// Move focus up.
    Up,
    /// Move focus down.
    Down,
    /// Move focus left.
    Left,
    /// Move focus right.
    Right,
}

/// Layout state manager for workspace.
///
/// Tracks the current layout profile and provides session-to-cell mapping.
#[derive(Debug, Clone)]
pub struct LayoutState {
    /// Current layout profile.
    profile: LayoutProfile,
    /// Session assignments to grid positions.
    assignments: Vec<SessionId>,
    /// Currently focused session index.
    focused_index: Option<usize>,
}

impl Default for LayoutState {
    fn default() -> Self {
        Self::new()
    }
}

impl LayoutState {
    /// Create a new layout state with default profile.
    pub fn new() -> Self {
        Self {
            profile: LayoutProfile::default(),
            assignments: Vec::new(),
            focused_index: None,
        }
    }

    /// Create a new layout state with a specific profile.
    pub fn with_profile(profile: LayoutProfile) -> Self {
        Self {
            profile,
            assignments: Vec::new(),
            focused_index: None,
        }
    }

    /// Get the current layout profile.
    pub fn profile(&self) -> LayoutProfile {
        self.profile
    }

    /// Set the layout profile.
    pub fn set_profile(&mut self, profile: LayoutProfile) {
        self.profile = profile;
    }

    /// Cycle to the next layout profile.
    pub fn next_profile(&mut self) {
        self.profile = self.profile.next();
    }

    /// Cycle to the previous layout profile.
    pub fn previous_profile(&mut self) {
        self.profile = self.profile.previous();
    }

    /// Get the session assignments.
    pub fn assignments(&self) -> &[SessionId] {
        &self.assignments
    }

    /// Set the session assignments.
    pub fn set_assignments(&mut self, assignments: Vec<SessionId>) {
        self.assignments = assignments;
    }

    /// Add a session to the layout.
    ///
    /// The session is added to the next available slot.
    ///
    /// # Returns
    ///
    /// `true` if the session was added, `false` if the layout is full.
    pub fn add_session(&mut self, session_id: SessionId) -> bool {
        if self.assignments.len() < self.profile.max_sessions() {
            self.assignments.push(session_id);
            true
        } else {
            false
        }
    }

    /// Remove a session from the layout.
    ///
    /// # Returns
    ///
    /// `true` if the session was removed, `false` if not found.
    pub fn remove_session(&mut self, session_id: SessionId) -> bool {
        if let Some(pos) = self.assignments.iter().position(|&id| id == session_id) {
            self.assignments.remove(pos);
            // Adjust focused index if needed
            if let Some(focused) = self.focused_index {
                if focused == pos {
                    self.focused_index = self.assignments.first().map(|_| 0);
                } else if focused > pos {
                    self.focused_index = Some(focused - 1);
                }
            }
            true
        } else {
            false
        }
    }

    /// Get the session at a given index.
    pub fn session_at(&self, index: usize) -> Option<SessionId> {
        self.assignments.get(index).copied()
    }

    /// Get the session at a given grid position.
    pub fn session_at_position(&self, position: GridPosition) -> Option<SessionId> {
        let (rows, cols) = self.profile.dimensions();
        if position.row >= rows || position.col >= cols {
            return None;
        }
        let index = (position.row * cols + position.col) as usize;
        self.session_at(index)
    }

    /// Get the focused session index.
    pub fn focused_index(&self) -> Option<usize> {
        self.focused_index
    }

    /// Get the focused session ID.
    pub fn focused_session(&self) -> Option<SessionId> {
        self.focused_index.and_then(|i| self.session_at(i))
    }

    /// Set the focused session by index.
    pub fn focus_index(&mut self, index: usize) {
        if index < self.assignments.len() {
            self.focused_index = Some(index);
        }
    }

    /// Set the focused session by ID.
    ///
    /// # Returns
    ///
    /// `true` if the session was found and focused.
    pub fn focus_session(&mut self, session_id: SessionId) -> bool {
        if let Some(index) = self.assignments.iter().position(|&id| id == session_id) {
            self.focused_index = Some(index);
            true
        } else {
            false
        }
    }

    /// Focus the next session in the layout.
    pub fn focus_next(&mut self) {
        if self.assignments.is_empty() {
            return;
        }
        let next = match self.focused_index {
            Some(i) => (i + 1) % self.assignments.len(),
            None => 0,
        };
        self.focused_index = Some(next);
    }

    /// Focus the previous session in the layout.
    pub fn focus_previous(&mut self) {
        if self.assignments.is_empty() {
            return;
        }
        let prev = match self.focused_index {
            Some(i) if i > 0 => i - 1,
            Some(_) => self.assignments.len() - 1,
            None => 0,
        };
        self.focused_index = Some(prev);
    }

    /// Focus the session in the given direction from current focus.
    ///
    /// # Arguments
    ///
    /// * `direction` - Direction to move focus (Up, Down, Left, Right)
    pub fn focus_direction(&mut self, direction: FocusDirection) {
        let Some(current_index) = self.focused_index else {
            if !self.assignments.is_empty() {
                self.focused_index = Some(0);
            }
            return;
        };

        let (rows, cols) = self.profile.dimensions();
        let current_row = current_index as u32 / cols;
        let current_col = current_index as u32 % cols;

        let (new_row, new_col) = match direction {
            FocusDirection::Up => {
                if current_row > 0 {
                    (current_row - 1, current_col)
                } else {
                    (rows - 1, current_col)
                }
            }
            FocusDirection::Down => {
                if current_row < rows - 1 {
                    (current_row + 1, current_col)
                } else {
                    (0, current_col)
                }
            }
            FocusDirection::Left => {
                if current_col > 0 {
                    (current_row, current_col - 1)
                } else {
                    (current_row, cols - 1)
                }
            }
            FocusDirection::Right => {
                if current_col < cols - 1 {
                    (current_row, current_col + 1)
                } else {
                    (current_row, 0)
                }
            }
        };

        let new_index = (new_row * cols + new_col) as usize;
        if new_index < self.assignments.len() {
            self.focused_index = Some(new_index);
        }
    }

    /// Swap two session assignments by index.
    ///
    /// Focus follows the session: if the focused session was at index `a`,
    /// focus moves to index `b` (and vice versa).
    ///
    /// Returns `true` if both indices are valid and different.
    pub fn swap_assignments(&mut self, a: usize, b: usize) -> bool {
        if a == b || a >= self.assignments.len() || b >= self.assignments.len() {
            return false;
        }
        self.assignments.swap(a, b);
        // Keep focus on the same session (it moved to the other index)
        if let Some(fi) = self.focused_index {
            if fi == a {
                self.focused_index = Some(b);
            } else if fi == b {
                self.focused_index = Some(a);
            }
        }
        true
    }
}

/// Split layout state manager.
///
/// Manages session assignments to split tree slots, focus tracking, and slot ID generation.
#[derive(Debug, Clone)]
pub struct SplitLayoutState {
    /// The split tree defining the pane arrangement.
    tree: LayoutNode,
    /// Session assignments: (SlotId, Option<SessionId>). Empty slots have None.
    pub(crate) assignments: Vec<(SlotId, Option<SessionId>)>,
    /// Currently focused slot.
    focused_slot: Option<SlotId>,
    /// Next slot ID to allocate.
    next_slot_id: u32,
}

impl SplitLayoutState {
    fn sync_assignment_order(&mut self) {
        let slot_order = self.tree.slots_in_order();
        let mut ordered = Vec::with_capacity(slot_order.len());

        for slot in slot_order {
            if let Some((_, session)) = self
                .assignments
                .iter()
                .find(|(current, _)| *current == slot)
            {
                ordered.push((slot, *session));
            }
        }

        self.assignments = ordered;
    }

    /// Create from a layout node tree.
    pub fn new(tree: LayoutNode) -> Self {
        let slots = tree.slots_in_order();
        let max_id = slots.iter().map(|s| s.0).max().unwrap_or(0);
        let assignments = slots.iter().map(|&s| (s, None)).collect();
        Self {
            tree,
            assignments,
            focused_slot: None,
            next_slot_id: max_id + 1,
        }
    }

    /// Create from a grid dimensions (converts to equivalent tree).
    pub fn from_grid(rows: u32, cols: u32) -> Self {
        Self::new(LayoutNode::from_grid(rows, cols))
    }

    /// Get the tree.
    pub fn tree(&self) -> &LayoutNode {
        &self.tree
    }

    /// Get the assignments.
    pub fn assignments(&self) -> &[(SlotId, Option<SessionId>)] {
        &self.assignments
    }

    /// Get the focused slot.
    pub fn focused_slot(&self) -> Option<SlotId> {
        self.focused_slot
    }

    /// Get the focused session ID.
    pub fn focused_session(&self) -> Option<SessionId> {
        self.focused_slot.and_then(|slot| {
            self.assignments
                .iter()
                .find(|(s, _)| *s == slot)
                .and_then(|(_, sess)| *sess)
        })
    }

    /// Get all assigned session IDs in DFS order.
    pub fn assigned_sessions(&self) -> Vec<SessionId> {
        self.assignments
            .iter()
            .filter_map(|(_, sess)| *sess)
            .collect()
    }

    /// Find the slot a session is assigned to.
    pub fn slot_for_session(&self, session_id: SessionId) -> Option<SlotId> {
        self.assignments
            .iter()
            .find(|(_, sess)| *sess == Some(session_id))
            .map(|(slot, _)| *slot)
    }

    /// Assign a session to the first empty slot.
    /// Returns true if assigned successfully.
    pub fn add_session(&mut self, session_id: SessionId) -> bool {
        // Don't add duplicates
        if self.assignments.iter().any(|(_, s)| *s == Some(session_id)) {
            return false;
        }
        for entry in &mut self.assignments {
            if entry.1.is_none() {
                entry.1 = Some(session_id);
                return true;
            }
        }
        false
    }

    /// Assign a session to a specific slot.
    pub fn assign_session_to_slot(&mut self, session_id: SessionId, slot: SlotId) -> bool {
        // Don't add duplicates
        if self.assignments.iter().any(|(_, s)| *s == Some(session_id)) {
            return false;
        }
        for entry in &mut self.assignments {
            if entry.0 == slot && entry.1.is_none() {
                entry.1 = Some(session_id);
                return true;
            }
        }
        false
    }

    /// Remove a session from its slot.
    pub fn remove_session(&mut self, session_id: SessionId) -> bool {
        for entry in &mut self.assignments {
            if entry.1 == Some(session_id) {
                entry.1 = None;
                // If the focused slot was this session's slot, try to move focus
                if self.focused_slot == Some(entry.0) {
                    self.focused_slot = self
                        .assignments
                        .iter()
                        .find(|(_, s)| s.is_some())
                        .map(|(slot, _)| *slot);
                }
                return true;
            }
        }
        false
    }

    /// Focus a session by ID.
    pub fn focus_session(&mut self, session_id: SessionId) -> bool {
        if let Some(slot) = self.slot_for_session(session_id) {
            self.focused_slot = Some(slot);
            true
        } else {
            false
        }
    }

    /// Focus a specific slot.
    pub fn focus_slot(&mut self, slot: SlotId) {
        if self.tree.contains_slot(slot) {
            self.focused_slot = Some(slot);
        }
    }

    /// Focus the next occupied slot in DFS order.
    pub fn focus_next(&mut self) {
        let occupied: Vec<SlotId> = self
            .assignments
            .iter()
            .filter(|(_, s)| s.is_some())
            .map(|(slot, _)| *slot)
            .collect();
        if occupied.is_empty() {
            return;
        }
        let current_idx = self
            .focused_slot
            .and_then(|s| occupied.iter().position(|&o| o == s));
        let next = match current_idx {
            Some(i) => (i + 1) % occupied.len(),
            None => 0,
        };
        self.focused_slot = Some(occupied[next]);
    }

    /// Focus the previous occupied slot in DFS order.
    pub fn focus_previous(&mut self) {
        let occupied: Vec<SlotId> = self
            .assignments
            .iter()
            .filter(|(_, s)| s.is_some())
            .map(|(slot, _)| *slot)
            .collect();
        if occupied.is_empty() {
            return;
        }
        let current_idx = self
            .focused_slot
            .and_then(|s| occupied.iter().position(|&o| o == s));
        let prev = match current_idx {
            Some(0) => occupied.len() - 1,
            Some(i) => i - 1,
            None => 0,
        };
        self.focused_slot = Some(occupied[prev]);
    }

    /// Spatial focus navigation — find the nearest slot in the given direction.
    ///
    /// Uses computed bounds centers to find the best candidate.
    pub fn focus_direction(&mut self, direction: FocusDirection, layout: &SplitLayout) {
        let Some(current_slot) = self.focused_slot else {
            // No focus — pick first occupied slot
            if let Some((slot, _)) = self.assignments.iter().find(|(_, s)| s.is_some()) {
                self.focused_slot = Some(*slot);
            }
            return;
        };

        let leaf_bounds = layout.leaf_bounds();
        let current_center = leaf_bounds
            .iter()
            .find(|(s, _)| *s == current_slot)
            .map(|(_, b)| {
                (
                    b.origin.x + b.size.width / 2.0,
                    b.origin.y + b.size.height / 2.0,
                )
            });
        let Some((cx, cy)) = current_center else {
            return;
        };

        let mut best: Option<(SlotId, f32)> = None;

        for (slot, bounds) in &leaf_bounds {
            if *slot == current_slot {
                continue;
            }
            // Only consider occupied slots
            if !self
                .assignments
                .iter()
                .any(|(s, sess)| *s == *slot && sess.is_some())
            {
                continue;
            }

            let sx = bounds.origin.x + bounds.size.width / 2.0;
            let sy = bounds.origin.y + bounds.size.height / 2.0;

            let in_direction = match direction {
                FocusDirection::Right => sx > cx,
                FocusDirection::Left => sx < cx,
                FocusDirection::Down => sy > cy,
                FocusDirection::Up => sy < cy,
            };

            if in_direction {
                let dist = (sx - cx).powi(2) + (sy - cy).powi(2);
                if best.map_or(true, |(_, best_dist)| dist < best_dist) {
                    best = Some((*slot, dist));
                }
            }
        }

        if let Some((slot, _)) = best {
            self.focused_slot = Some(slot);
        }
    }

    /// Split a slot into two. Returns the new slot ID, or None if the slot wasn't found.
    pub fn split_slot(
        &mut self,
        target: SlotId,
        direction: SplitDirection,
        ratio: f32,
    ) -> Option<SlotId> {
        let new_slot = SlotId(self.next_slot_id);
        if let Some(new_tree) = self.tree.split_slot(target, direction, ratio, new_slot) {
            self.tree = new_tree;
            self.next_slot_id += 1;
            // Add the new slot to assignments (empty)
            self.assignments.push((new_slot, None));
            self.sync_assignment_order();
            Some(new_slot)
        } else {
            None
        }
    }

    /// Close a slot, promoting its sibling. Returns true if successful.
    pub fn close_slot(&mut self, target: SlotId) -> bool {
        if let Some(new_tree) = self.tree.close_slot(target) {
            self.tree = new_tree;
            // Remove the closed slot from assignments
            self.assignments.retain(|(s, _)| *s != target);
            self.sync_assignment_order();
            // Fix focus if needed
            if self.focused_slot == Some(target) {
                self.focused_slot = self
                    .assignments
                    .iter()
                    .find(|(_, s)| s.is_some())
                    .map(|(slot, _)| *slot);
            }
            true
        } else {
            false
        }
    }

    /// Resize a split by updating its ratio.
    pub fn resize_split(&mut self, target: SlotId, new_ratio: f32) -> bool {
        if let Some(new_tree) = self.tree.set_ratio_for_slot(target, new_ratio) {
            self.tree = new_tree;
            true
        } else {
            false
        }
    }

    /// Get the number of leaf slots.
    pub fn slot_count(&self) -> usize {
        self.tree.leaf_count()
    }

    /// Get the number of available (empty) slots.
    pub fn available_slots(&self) -> usize {
        self.assignments.iter().filter(|(_, s)| s.is_none()).count()
    }

    /// Get the session at a specific slot.
    pub fn session_at_slot(&self, slot: SlotId) -> Option<SessionId> {
        self.assignments
            .iter()
            .find(|(s, _)| *s == slot)
            .and_then(|(_, sess)| *sess)
    }

    /// Swap session assignments between two slots by index into the assignments vec.
    ///
    /// Swaps the `Option<SessionId>` values (not the SlotIds).
    /// Returns `true` if both indices are valid and different.
    pub fn swap_assignments(&mut self, a: usize, b: usize) -> bool {
        if a == b || a >= self.assignments.len() || b >= self.assignments.len() {
            return false;
        }
        let session_a = self.assignments[a].1;
        let session_b = self.assignments[b].1;
        self.assignments[a].1 = session_b;
        self.assignments[b].1 = session_a;
        true
    }
}

/// Unified workspace layout state wrapping both grid and split tree modes.
#[derive(Debug, Clone)]
pub enum WorkspaceLayoutState {
    /// Traditional grid layout using predefined profiles.
    Grid(LayoutState),
    /// Binary split tree for custom asymmetric layouts.
    SplitTree(SplitLayoutState),
}

impl Default for WorkspaceLayoutState {
    fn default() -> Self {
        WorkspaceLayoutState::Grid(LayoutState::new())
    }
}

impl WorkspaceLayoutState {
    /// Create a grid-mode state with a specific profile.
    pub fn with_profile(profile: LayoutProfile) -> Self {
        WorkspaceLayoutState::Grid(LayoutState::with_profile(profile))
    }

    /// Create a split-tree-mode state from a tree.
    pub fn with_split_tree(tree: LayoutNode) -> Self {
        WorkspaceLayoutState::SplitTree(SplitLayoutState::new(tree))
    }

    /// Check if currently in grid mode.
    pub fn is_grid(&self) -> bool {
        matches!(self, WorkspaceLayoutState::Grid(_))
    }

    /// Check if currently in split tree mode.
    pub fn is_split_tree(&self) -> bool {
        matches!(self, WorkspaceLayoutState::SplitTree(_))
    }

    /// Get the grid state, if in grid mode.
    pub fn as_grid(&self) -> Option<&LayoutState> {
        match self {
            WorkspaceLayoutState::Grid(s) => Some(s),
            _ => None,
        }
    }

    /// Get mutable grid state, if in grid mode.
    pub fn as_grid_mut(&mut self) -> Option<&mut LayoutState> {
        match self {
            WorkspaceLayoutState::Grid(s) => Some(s),
            _ => None,
        }
    }

    /// Get the split tree state, if in split tree mode.
    pub fn as_split_tree(&self) -> Option<&SplitLayoutState> {
        match self {
            WorkspaceLayoutState::SplitTree(s) => Some(s),
            _ => None,
        }
    }

    /// Get mutable split tree state, if in split tree mode.
    pub fn as_split_tree_mut(&mut self) -> Option<&mut SplitLayoutState> {
        match self {
            WorkspaceLayoutState::SplitTree(s) => Some(s),
            _ => None,
        }
    }

    /// Add a session. Delegates to the active variant.
    pub fn add_session(&mut self, session_id: SessionId) -> bool {
        match self {
            WorkspaceLayoutState::Grid(s) => s.add_session(session_id),
            WorkspaceLayoutState::SplitTree(s) => s.add_session(session_id),
        }
    }

    /// Add a session to a specific slot (split tree mode only).
    pub fn add_session_to_slot(&mut self, session_id: SessionId, slot: SlotId) -> bool {
        match self {
            WorkspaceLayoutState::SplitTree(s) => s.assign_session_to_slot(session_id, slot),
            _ => false,
        }
    }

    /// Remove a session. Delegates to the active variant.
    pub fn remove_session(&mut self, session_id: SessionId) -> bool {
        match self {
            WorkspaceLayoutState::Grid(s) => s.remove_session(session_id),
            WorkspaceLayoutState::SplitTree(s) => s.remove_session(session_id),
        }
    }

    /// Get the focused session ID.
    pub fn focused_session(&self) -> Option<SessionId> {
        match self {
            WorkspaceLayoutState::Grid(s) => s.focused_session(),
            WorkspaceLayoutState::SplitTree(s) => s.focused_session(),
        }
    }

    /// Focus a session by ID.
    pub fn focus_session(&mut self, session_id: SessionId) -> bool {
        match self {
            WorkspaceLayoutState::Grid(s) => s.focus_session(session_id),
            WorkspaceLayoutState::SplitTree(s) => s.focus_session(session_id),
        }
    }

    /// Focus next session.
    pub fn focus_next(&mut self) {
        match self {
            WorkspaceLayoutState::Grid(s) => s.focus_next(),
            WorkspaceLayoutState::SplitTree(s) => s.focus_next(),
        }
    }

    /// Focus previous session.
    pub fn focus_previous(&mut self) {
        match self {
            WorkspaceLayoutState::Grid(s) => s.focus_previous(),
            WorkspaceLayoutState::SplitTree(s) => s.focus_previous(),
        }
    }

    /// Get all assigned session IDs in order.
    pub fn assigned_sessions(&self) -> Vec<SessionId> {
        match self {
            WorkspaceLayoutState::Grid(s) => s.assignments().to_vec(),
            WorkspaceLayoutState::SplitTree(s) => s.assigned_sessions(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Bounds;

    // LayoutState tests
    #[test]
    fn test_layout_state_new() {
        let state = LayoutState::new();
        assert_eq!(state.profile(), LayoutProfile::Grid2x2);
        assert!(state.assignments().is_empty());
        assert!(state.focused_index().is_none());
    }

    #[test]
    fn test_layout_state_with_profile() {
        let state = LayoutState::with_profile(LayoutProfile::Grid3x3);
        assert_eq!(state.profile(), LayoutProfile::Grid3x3);
    }

    #[test]
    fn test_layout_state_set_profile() {
        let mut state = LayoutState::new();
        state.set_profile(LayoutProfile::Single);
        assert_eq!(state.profile(), LayoutProfile::Single);
    }

    #[test]
    fn test_layout_state_next_previous_profile() {
        let mut state = LayoutState::new();
        state.next_profile();
        assert_eq!(state.profile(), LayoutProfile::Stack1x4);
        state.previous_profile();
        assert_eq!(state.profile(), LayoutProfile::Grid2x2);
    }

    #[test]
    fn test_layout_state_add_session() {
        let mut state = LayoutState::with_profile(LayoutProfile::Grid2x2);
        assert!(state.add_session(SessionId(1)));
        assert!(state.add_session(SessionId(2)));
        assert!(state.add_session(SessionId(3)));
        assert!(state.add_session(SessionId(4)));
        assert!(!state.add_session(SessionId(5))); // Full

        assert_eq!(state.assignments().len(), 4);
    }

    #[test]
    fn test_layout_state_remove_session() {
        let mut state = LayoutState::new();
        state.add_session(SessionId(1));
        state.add_session(SessionId(2));
        state.focus_index(1);

        assert!(state.remove_session(SessionId(1)));
        assert_eq!(state.assignments().len(), 1);
        assert_eq!(state.focused_index(), Some(0)); // Adjusted

        assert!(!state.remove_session(SessionId(99))); // Not found
    }

    #[test]
    fn test_layout_state_session_at() {
        let mut state = LayoutState::new();
        state.add_session(SessionId(1));
        state.add_session(SessionId(2));

        assert_eq!(state.session_at(0), Some(SessionId(1)));
        assert_eq!(state.session_at(1), Some(SessionId(2)));
        assert_eq!(state.session_at(2), None);
    }

    #[test]
    fn test_layout_state_session_at_position() {
        let mut state = LayoutState::with_profile(LayoutProfile::Grid2x2);
        state.add_session(SessionId(1));
        state.add_session(SessionId(2));
        state.add_session(SessionId(3));

        assert_eq!(
            state.session_at_position(GridPosition { row: 0, col: 0 }),
            Some(SessionId(1))
        );
        assert_eq!(
            state.session_at_position(GridPosition { row: 0, col: 1 }),
            Some(SessionId(2))
        );
        assert_eq!(
            state.session_at_position(GridPosition { row: 1, col: 0 }),
            Some(SessionId(3))
        );
        assert_eq!(
            state.session_at_position(GridPosition { row: 1, col: 1 }),
            None
        );
    }

    #[test]
    fn test_layout_state_focus_index() {
        let mut state = LayoutState::new();
        state.add_session(SessionId(1));
        state.add_session(SessionId(2));

        state.focus_index(0);
        assert_eq!(state.focused_index(), Some(0));
        assert_eq!(state.focused_session(), Some(SessionId(1)));

        state.focus_index(1);
        assert_eq!(state.focused_index(), Some(1));
        assert_eq!(state.focused_session(), Some(SessionId(2)));

        // Out of bounds does nothing
        state.focus_index(10);
        assert_eq!(state.focused_index(), Some(1));
    }

    #[test]
    fn test_layout_state_focus_session() {
        let mut state = LayoutState::new();
        state.add_session(SessionId(1));
        state.add_session(SessionId(2));

        assert!(state.focus_session(SessionId(2)));
        assert_eq!(state.focused_index(), Some(1));

        assert!(!state.focus_session(SessionId(99)));
        assert_eq!(state.focused_index(), Some(1)); // Unchanged
    }

    #[test]
    fn test_layout_state_focus_next() {
        let mut state = LayoutState::new();
        state.add_session(SessionId(1));
        state.add_session(SessionId(2));
        state.add_session(SessionId(3));

        // No focus initially
        state.focus_next();
        assert_eq!(state.focused_index(), Some(0));

        state.focus_next();
        assert_eq!(state.focused_index(), Some(1));

        state.focus_next();
        assert_eq!(state.focused_index(), Some(2));

        // Wrap around
        state.focus_next();
        assert_eq!(state.focused_index(), Some(0));
    }

    #[test]
    fn test_layout_state_focus_previous() {
        let mut state = LayoutState::new();
        state.add_session(SessionId(1));
        state.add_session(SessionId(2));
        state.add_session(SessionId(3));

        state.focus_index(0);
        state.focus_previous();
        assert_eq!(state.focused_index(), Some(2)); // Wrap around
    }

    #[test]
    fn test_layout_state_focus_direction_2x2() {
        let mut state = LayoutState::with_profile(LayoutProfile::Grid2x2);
        state.add_session(SessionId(1));
        state.add_session(SessionId(2));
        state.add_session(SessionId(3));
        state.add_session(SessionId(4));
        state.focus_index(0); // Top-left

        // Move right
        state.focus_direction(FocusDirection::Right);
        assert_eq!(state.focused_index(), Some(1));

        // Move down
        state.focus_direction(FocusDirection::Down);
        assert_eq!(state.focused_index(), Some(3));

        // Move left
        state.focus_direction(FocusDirection::Left);
        assert_eq!(state.focused_index(), Some(2));

        // Move up
        state.focus_direction(FocusDirection::Up);
        assert_eq!(state.focused_index(), Some(0));
    }

    #[test]
    fn test_layout_state_focus_direction_wraps() {
        let mut state = LayoutState::with_profile(LayoutProfile::Grid2x2);
        state.add_session(SessionId(1));
        state.add_session(SessionId(2));
        state.add_session(SessionId(3));
        state.add_session(SessionId(4));

        // Start at top-left, go up (wraps to bottom)
        state.focus_index(0);
        state.focus_direction(FocusDirection::Up);
        assert_eq!(state.focused_index(), Some(2));

        // Start at top-left, go left (wraps to right)
        state.focus_index(0);
        state.focus_direction(FocusDirection::Left);
        assert_eq!(state.focused_index(), Some(1));
    }

    #[test]
    fn test_focus_direction_equality() {
        assert_eq!(FocusDirection::Up, FocusDirection::Up);
        assert_ne!(FocusDirection::Up, FocusDirection::Down);
    }

    #[test]
    fn test_layout_state_empty_focus_operations() {
        let mut state = LayoutState::new();

        // Focus operations on empty state should do nothing
        state.focus_next();
        assert!(state.focused_index().is_none());

        state.focus_previous();
        assert!(state.focused_index().is_none());

        state.focus_direction(FocusDirection::Up);
        assert!(state.focused_index().is_none());
    }

    #[test]
    fn test_layout_state_focus_direction_no_current_focus() {
        let mut state = LayoutState::new();
        state.add_session(SessionId(1));
        state.add_session(SessionId(2));

        // No focus, should set to first session
        state.focus_direction(FocusDirection::Right);
        assert_eq!(state.focused_index(), Some(0));
    }

    // ============================================================
    // SplitLayoutState tests
    // ============================================================

    #[test]
    fn test_split_layout_state_new() {
        let tree = LayoutNode::from_grid(2, 2);
        let state = SplitLayoutState::new(tree);
        assert_eq!(state.slot_count(), 4);
        assert_eq!(state.available_slots(), 4);
        assert!(state.focused_slot().is_none());
    }

    #[test]
    fn test_split_layout_state_from_grid() {
        let state = SplitLayoutState::from_grid(2, 3);
        assert_eq!(state.slot_count(), 6);
    }

    #[test]
    fn test_split_layout_state_add_session() {
        let mut state = SplitLayoutState::from_grid(2, 2);
        assert!(state.add_session(SessionId(1)));
        assert!(state.add_session(SessionId(2)));
        assert_eq!(state.available_slots(), 2);

        // Duplicate rejected
        assert!(!state.add_session(SessionId(1)));
    }

    #[test]
    fn test_split_layout_state_add_session_full() {
        let mut state = SplitLayoutState::new(LayoutNode::Leaf { slot: SlotId(0) });
        assert!(state.add_session(SessionId(1)));
        assert!(!state.add_session(SessionId(2))); // Only one slot
    }

    #[test]
    fn test_split_layout_state_remove_session() {
        let mut state = SplitLayoutState::from_grid(2, 2);
        state.add_session(SessionId(1));
        state.add_session(SessionId(2));
        assert!(state.remove_session(SessionId(1)));
        assert_eq!(state.available_slots(), 3);
        assert!(!state.remove_session(SessionId(99)));
    }

    #[test]
    fn test_split_layout_state_focus_session() {
        let mut state = SplitLayoutState::from_grid(2, 2);
        state.add_session(SessionId(1));
        state.add_session(SessionId(2));

        assert!(state.focus_session(SessionId(2)));
        assert_eq!(state.focused_session(), Some(SessionId(2)));

        assert!(!state.focus_session(SessionId(99)));
    }

    #[test]
    fn test_split_layout_state_focus_next_previous() {
        let mut state = SplitLayoutState::from_grid(1, 3);
        state.add_session(SessionId(1));
        state.add_session(SessionId(2));
        state.add_session(SessionId(3));
        state.focus_session(SessionId(1));

        state.focus_next();
        assert_eq!(state.focused_session(), Some(SessionId(2)));

        state.focus_next();
        assert_eq!(state.focused_session(), Some(SessionId(3)));

        state.focus_next();
        assert_eq!(state.focused_session(), Some(SessionId(1))); // wrap

        state.focus_previous();
        assert_eq!(state.focused_session(), Some(SessionId(3))); // wrap back
    }

    #[test]
    fn test_split_layout_state_focus_direction() {
        let mut state = SplitLayoutState::from_grid(2, 2);
        state.add_session(SessionId(1));
        state.add_session(SessionId(2));
        state.add_session(SessionId(3));
        state.add_session(SessionId(4));
        state.focus_session(SessionId(1)); // top-left

        let layout = SplitLayout::new(state.tree().clone(), Bounds::from_size(1000.0, 800.0), 4.0);

        // Move right
        state.focus_direction(FocusDirection::Right, &layout);
        assert_eq!(state.focused_session(), Some(SessionId(2)));

        // Move down
        state.focus_direction(FocusDirection::Down, &layout);
        assert_eq!(state.focused_session(), Some(SessionId(4)));

        // Move left
        state.focus_direction(FocusDirection::Left, &layout);
        assert_eq!(state.focused_session(), Some(SessionId(3)));

        // Move up
        state.focus_direction(FocusDirection::Up, &layout);
        assert_eq!(state.focused_session(), Some(SessionId(1)));
    }

    #[test]
    fn test_split_layout_state_split_and_close() {
        let mut state = SplitLayoutState::new(LayoutNode::Leaf { slot: SlotId(0) });
        state.add_session(SessionId(1));

        // Split the slot
        let new_slot = state
            .split_slot(SlotId(0), SplitDirection::Horizontal, 0.5)
            .unwrap();
        assert_eq!(state.slot_count(), 2);
        assert_eq!(state.available_slots(), 1);

        // Add a session to the new slot
        state.add_session(SessionId(2));
        assert_eq!(state.available_slots(), 0);

        // Close the new slot
        assert!(state.close_slot(new_slot));
        assert_eq!(state.slot_count(), 1);
    }

    #[test]
    fn test_split_layout_state_resize() {
        let mut state = SplitLayoutState::from_grid(1, 2);
        assert!(state.resize_split(SlotId(0), 0.3));
    }

    #[test]
    fn test_split_layout_state_assigned_sessions() {
        let mut state = SplitLayoutState::from_grid(2, 2);
        state.add_session(SessionId(1));
        state.add_session(SessionId(2));
        let sessions = state.assigned_sessions();
        assert_eq!(sessions, vec![SessionId(1), SessionId(2)]);
    }

    #[test]
    fn test_split_layout_state_session_at_slot() {
        let mut state = SplitLayoutState::from_grid(2, 2);
        state.add_session(SessionId(42));
        assert_eq!(state.session_at_slot(SlotId(0)), Some(SessionId(42)));
        assert_eq!(state.session_at_slot(SlotId(1)), None);
    }

    // ============================================================
    // WorkspaceLayoutState tests
    // ============================================================

    #[test]
    fn test_workspace_layout_state_default_is_grid() {
        let wls = WorkspaceLayoutState::default();
        assert!(wls.is_grid());
        assert!(!wls.is_split_tree());
    }

    #[test]
    fn test_workspace_layout_state_with_profile() {
        let wls = WorkspaceLayoutState::with_profile(LayoutProfile::Grid3x3);
        assert!(wls.is_grid());
        assert_eq!(wls.as_grid().unwrap().profile(), LayoutProfile::Grid3x3);
    }

    #[test]
    fn test_workspace_layout_state_with_split_tree() {
        let tree = LayoutNode::from_grid(2, 2);
        let wls = WorkspaceLayoutState::with_split_tree(tree);
        assert!(wls.is_split_tree());
        assert!(!wls.is_grid());
    }

    #[test]
    fn test_workspace_layout_state_grid_operations() {
        let mut wls = WorkspaceLayoutState::default();
        assert!(wls.add_session(SessionId(1)));
        assert!(wls.add_session(SessionId(2)));
        assert_eq!(wls.focused_session(), None);

        assert!(wls.focus_session(SessionId(1)));
        assert_eq!(wls.focused_session(), Some(SessionId(1)));

        wls.focus_next();
        assert_eq!(wls.focused_session(), Some(SessionId(2)));

        wls.focus_previous();
        assert_eq!(wls.focused_session(), Some(SessionId(1)));

        let sessions = wls.assigned_sessions();
        assert_eq!(sessions, vec![SessionId(1), SessionId(2)]);

        assert!(wls.remove_session(SessionId(1)));
    }

    #[test]
    fn test_workspace_layout_state_split_operations() {
        let tree = LayoutNode::from_grid(2, 2);
        let mut wls = WorkspaceLayoutState::with_split_tree(tree);
        assert!(wls.add_session(SessionId(1)));
        assert!(wls.add_session(SessionId(2)));

        assert!(wls.focus_session(SessionId(2)));
        assert_eq!(wls.focused_session(), Some(SessionId(2)));

        wls.focus_next();
        assert_eq!(wls.focused_session(), Some(SessionId(1))); // wrap

        wls.focus_previous();
        assert_eq!(wls.focused_session(), Some(SessionId(2))); // wrap back
    }

    #[test]
    fn test_assign_session_to_slot() {
        let tree = LayoutNode::from_grid(1, 3); // 3 leaf slots
        let mut state = SplitLayoutState::new(tree);
        let slots: Vec<SlotId> = state.assignments.iter().map(|(s, _)| *s).collect();
        assert_eq!(slots.len(), 3);

        // Assign to the second slot specifically
        assert!(state.assign_session_to_slot(SessionId(1), slots[1]));
        assert_eq!(state.assignments[1].1, Some(SessionId(1)));
        // First and third slots remain empty
        assert_eq!(state.assignments[0].1, None);
        assert_eq!(state.assignments[2].1, None);
    }

    #[test]
    fn test_assign_session_to_slot_rejects_duplicate() {
        let tree = LayoutNode::from_grid(1, 3);
        let mut state = SplitLayoutState::new(tree);
        let slots: Vec<SlotId> = state.assignments.iter().map(|(s, _)| *s).collect();

        assert!(state.assign_session_to_slot(SessionId(1), slots[0]));
        // Same session to a different slot should fail (duplicate)
        assert!(!state.assign_session_to_slot(SessionId(1), slots[1]));
    }

    #[test]
    fn test_assign_session_to_slot_rejects_occupied() {
        let tree = LayoutNode::from_grid(1, 3);
        let mut state = SplitLayoutState::new(tree);
        let slots: Vec<SlotId> = state.assignments.iter().map(|(s, _)| *s).collect();

        assert!(state.assign_session_to_slot(SessionId(1), slots[0]));
        // Different session to the same (now-occupied) slot should fail
        assert!(!state.assign_session_to_slot(SessionId(2), slots[0]));
    }

    #[test]
    fn test_workspace_layout_state_add_session_to_slot() {
        let tree = LayoutNode::from_grid(1, 3);
        let mut wls = WorkspaceLayoutState::with_split_tree(tree);

        // Delegates to SplitLayoutState
        let slots: Vec<SlotId> = wls
            .as_split_tree()
            .unwrap()
            .assignments
            .iter()
            .map(|(s, _)| *s)
            .collect();
        assert!(wls.add_session_to_slot(SessionId(1), slots[2]));
        assert_eq!(wls.assigned_sessions(), vec![SessionId(1)]);

        // Grid mode returns false
        let mut grid_wls = WorkspaceLayoutState::with_profile(LayoutProfile::Grid2x2);
        assert!(!grid_wls.add_session_to_slot(SessionId(1), SlotId(0)));
    }
}
