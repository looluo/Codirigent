//! Workspace view for Codirigent.
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
//! use codirigent_ui::workspace::Workspace;
//! use codirigent_ui::layout::LayoutProfile;
//! use codirigent_core::{Session, SessionId};
//! use std::path::PathBuf;
//!
//! let mut workspace = Workspace::new();
//! workspace.set_layout(LayoutProfile::Grid2x2);
//!
//! let session = Session::new(SessionId(1), "Session 1".to_string(), PathBuf::from("/tmp"));
//! workspace.add_session(session);
//! ```

use crate::layout::{
    Bounds, FocusDirection, GridLayout, LayoutProfile, LayoutState, Point, SplitLayout,
    SplitLayoutState, WorkspaceLayoutState, TOP_BAR_HEIGHT,
};
use crate::theme::CodirigentTheme;
use codirigent_core::{
    LayoutNode, PaneId, PaneStackState, PaneTabGroup, Session, SessionId, SessionStatus, SlotId,
    SplitDirection,
};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
struct PaneStack {
    session_ids: Vec<SessionId>,
    active_session_id: SessionId,
}

/// Main workspace containing the grid of sessions.
///
/// The workspace is responsible for:
/// - Managing session layout and assignment
/// - Tracking which session is focused
/// - Handling layout profile changes
/// - Providing session navigation
#[derive(Debug)]
pub struct Workspace {
    /// Unified layout state supporting both grid and split tree modes.
    layout_state: WorkspaceLayoutState,
    /// Pane-local tab stacks keyed by visible pane identifier.
    pane_tab_groups: HashMap<PaneId, PaneTabGroup>,
    /// Ordered tab stacks that no longer fit in the current visible layout.
    hidden_pane_stacks: Vec<PaneStack>,
    /// Sessions in the workspace.
    sessions: Vec<Session>,
    /// Theme configuration.
    theme: CodirigentTheme,
    /// Whether the sidebar is visible.
    show_sidebar: bool,
    /// Sidebar width in pixels.
    sidebar_width: f32,
    /// Right panel width in pixels (0.0 when closed).
    right_panel_width: f32,
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
            layout_state: WorkspaceLayoutState::default(),
            pane_tab_groups: HashMap::new(),
            hidden_pane_stacks: Vec::new(),
            sessions: Vec::new(),
            theme: CodirigentTheme::default(),
            show_sidebar: true,
            sidebar_width: 56.0,
            right_panel_width: 0.0,
            bounds: Bounds::from_size(1280.0, 720.0),
        }
    }

    /// Create a new workspace with a specific layout profile.
    pub fn with_profile(profile: LayoutProfile) -> Self {
        Self {
            layout_state: WorkspaceLayoutState::with_profile(profile),
            pane_tab_groups: HashMap::new(),
            hidden_pane_stacks: Vec::new(),
            sessions: Vec::new(),
            theme: CodirigentTheme::default(),
            show_sidebar: true,
            sidebar_width: 56.0,
            right_panel_width: 0.0,
            bounds: Bounds::from_size(1280.0, 720.0),
        }
    }

    // --- Layout Management ---

    /// Get the current layout profile.
    ///
    /// Returns the grid profile if in grid mode, or `Grid2x2` as fallback
    /// if in split tree mode (split trees don't map to a single profile).
    pub fn layout_profile(&self) -> LayoutProfile {
        match &self.layout_state {
            WorkspaceLayoutState::Grid(s) => s.profile(),
            WorkspaceLayoutState::SplitTree(_) => LayoutProfile::Grid2x2,
        }
    }

    /// Set the layout profile, switching to grid mode.
    ///
    /// When switching from split tree to grid, sessions are re-assigned
    /// in their current order to the new grid.
    pub fn set_layout(&mut self, profile: LayoutProfile) {
        let stacks = self.layout_transition_stacks();
        let rebuilt_layout = WorkspaceLayoutState::Grid(self.rebuild_grid_state(profile, &stacks));
        let ordered_stacks = Self::stacks_ordered_for_layout_state(&rebuilt_layout, &stacks);
        self.layout_state = rebuilt_layout;
        self.apply_pane_stacks_to_current_layout(ordered_stacks);
    }

    /// Cycle to the next layout profile.
    pub fn next_layout(&mut self) {
        let next = match &self.layout_state {
            WorkspaceLayoutState::Grid(s) => s.profile().next(),
            WorkspaceLayoutState::SplitTree(_) => LayoutProfile::Grid2x2,
        };
        self.set_layout(next);
    }

    /// Cycle to the previous layout profile.
    pub fn previous_layout(&mut self) {
        let previous = match &self.layout_state {
            WorkspaceLayoutState::Grid(s) => s.profile().previous(),
            WorkspaceLayoutState::SplitTree(_) => LayoutProfile::Single,
        };
        self.set_layout(previous);
    }

    /// Get the grid layout calculator for the current state.
    ///
    /// Only meaningful in grid mode; in split tree mode, returns a 1x1 grid.
    pub fn grid_layout(&self) -> GridLayout {
        let grid_bounds = self.grid_bounds();
        match &self.layout_state {
            WorkspaceLayoutState::Grid(s) => {
                GridLayout::from_profile(s.profile(), grid_bounds, self.theme.grid_gap)
            }
            WorkspaceLayoutState::SplitTree(_) => {
                GridLayout::from_profile(LayoutProfile::Single, grid_bounds, self.theme.grid_gap)
            }
        }
    }

    /// Get a split layout calculator, if in split tree mode.
    pub fn split_layout(&self) -> Option<SplitLayout> {
        match &self.layout_state {
            WorkspaceLayoutState::SplitTree(s) => Some(SplitLayout::new(
                s.tree().clone(),
                self.grid_bounds(),
                self.theme.grid_gap,
            )),
            _ => None,
        }
    }

    /// Check if the workspace is in split tree mode.
    pub fn is_split_tree_mode(&self) -> bool {
        self.layout_state.is_split_tree()
    }

    /// Get the underlying layout state.
    pub fn layout_state(&self) -> &WorkspaceLayoutState {
        &self.layout_state
    }

    /// Get persisted pane tab groups for the current layout.
    pub fn pane_tab_groups(&self) -> Vec<PaneTabGroup> {
        let mut groups = self.pane_tab_groups.values().cloned().collect::<Vec<_>>();
        groups.sort_by_key(|group| match group.pane {
            PaneId::GridCell { index } => (0u8, index),
            PaneId::SplitSlot { slot } => (1u8, slot.0 as usize),
        });
        groups
    }

    /// Get persisted pane stacks for the entire workspace, including hidden stacks.
    pub fn pane_stacks(&self) -> Vec<PaneStackState> {
        self.current_pane_stacks_in_order()
            .into_iter()
            .map(|stack| PaneStackState {
                session_ids: stack.session_ids,
                active_session_id: stack.active_session_id,
            })
            .collect()
    }

    fn visible_pane_ids(&self) -> Vec<PaneId> {
        Self::visible_pane_ids_for_state(&self.layout_state)
    }

    fn visible_pane_ids_for_state(layout_state: &WorkspaceLayoutState) -> Vec<PaneId> {
        match layout_state {
            WorkspaceLayoutState::Grid(state) => (0..state.assignments().len())
                .map(|index| PaneId::GridCell { index })
                .collect(),
            WorkspaceLayoutState::SplitTree(state) => state
                .assignments()
                .iter()
                .map(|(slot, _)| PaneId::SplitSlot { slot: *slot })
                .collect(),
        }
    }

    fn active_session_for_pane(&self, pane_id: PaneId) -> Option<SessionId> {
        if let Some(group) = self.pane_tab_groups.get(&pane_id) {
            return Some(group.active_session_id);
        }

        match pane_id {
            PaneId::GridCell { index } => self
                .layout_state
                .as_grid()
                .and_then(|state| state.session_at(index)),
            PaneId::SplitSlot { slot } => self
                .layout_state
                .as_split_tree()
                .and_then(|state| state.session_at_slot(slot)),
        }
    }

    fn pane_id_for_session(&self, session_id: SessionId) -> Option<PaneId> {
        if let Some((pane_id, _)) = self
            .pane_tab_groups
            .iter()
            .find(|(_, group)| group.session_ids.contains(&session_id))
        {
            return Some(pane_id.clone());
        }

        match &self.layout_state {
            WorkspaceLayoutState::Grid(state) => state
                .assignments()
                .iter()
                .enumerate()
                .find(|(_, assigned)| **assigned == Some(session_id))
                .map(|(index, _)| PaneId::GridCell { index }),
            WorkspaceLayoutState::SplitTree(state) => state
                .slot_for_session(session_id)
                .map(|slot| PaneId::SplitSlot { slot }),
        }
    }

    fn pane_sessions(&self, pane_id: PaneId) -> Vec<SessionId> {
        if let Some(group) = self.pane_tab_groups.get(&pane_id) {
            return group.session_ids.clone();
        }

        self.active_session_for_pane(pane_id).into_iter().collect()
    }

    fn current_pane_stacks_in_order(&self) -> Vec<PaneStack> {
        if self
            .layout_state
            .as_grid()
            .is_some_and(|state| state.profile() == LayoutProfile::Single)
            && self.pane_tab_groups.is_empty()
            && self
                .hidden_pane_stacks
                .iter()
                .all(|stack| stack.session_ids.len() == 1)
        {
            return self
                .sessions
                .iter()
                .map(|session| PaneStack {
                    session_ids: vec![session.id],
                    active_session_id: session.id,
                })
                .collect();
        }

        let mut stacks = self
            .visible_pane_ids()
            .into_iter()
            .filter_map(|pane_id| {
                let session_ids = self.pane_sessions(pane_id.clone());
                let active_session_id = self.active_session_for_pane(pane_id)?;
                (!session_ids.is_empty()).then_some(PaneStack {
                    session_ids,
                    active_session_id,
                })
            })
            .collect::<Vec<_>>();

        let mut assigned: HashSet<SessionId> = stacks
            .iter()
            .flat_map(|stack| stack.session_ids.iter().copied())
            .collect();
        stacks.extend(self.hidden_pane_stacks.iter().filter_map(|stack| {
            let session_ids = stack
                .session_ids
                .iter()
                .copied()
                .filter(|session_id| {
                    self.session(*session_id).is_some() && !assigned.contains(session_id)
                })
                .collect::<Vec<_>>();
            if session_ids.is_empty() {
                return None;
            }

            assigned.extend(session_ids.iter().copied());
            Some(PaneStack {
                active_session_id: if session_ids.contains(&stack.active_session_id) {
                    stack.active_session_id
                } else {
                    session_ids[0]
                },
                session_ids,
            })
        }));
        stacks.extend(
            self.sessions
                .iter()
                .map(|session| session.id)
                .filter(|session_id| !assigned.contains(session_id))
                .map(|session_id| PaneStack {
                    session_ids: vec![session_id],
                    active_session_id: session_id,
                }),
        );

        stacks
    }

    fn layout_transition_stacks(&self) -> Vec<PaneStack> {
        let mut stacks = self.current_pane_stacks_in_order();
        let session_positions = self
            .sessions
            .iter()
            .enumerate()
            .map(|(index, session)| (session.id, index))
            .collect::<HashMap<_, _>>();

        stacks.sort_by_key(|stack| {
            stack
                .session_ids
                .iter()
                .filter_map(|session_id| session_positions.get(session_id))
                .copied()
                .min()
                .unwrap_or(usize::MAX)
        });

        stacks
    }

    fn apply_pane_stacks_to_current_layout(&mut self, stacks: Vec<PaneStack>) {
        let pane_ids = self.visible_pane_ids();
        self.pane_tab_groups.clear();
        self.hidden_pane_stacks.clear();

        for (index, stack) in stacks.into_iter().enumerate() {
            if let Some(pane_id) = pane_ids.get(index).cloned() {
                if stack.session_ids.len() > 1 {
                    self.pane_tab_groups.insert(
                        pane_id.clone(),
                        PaneTabGroup {
                            pane: pane_id,
                            session_ids: stack.session_ids,
                            active_session_id: stack.active_session_id,
                        },
                    );
                }
            } else if !stack.session_ids.is_empty() {
                self.hidden_pane_stacks.push(stack);
            }
        }

        self.cleanup_pane_tab_groups();
    }

    fn stacks_ordered_for_layout_state(
        layout_state: &WorkspaceLayoutState,
        stacks: &[PaneStack],
    ) -> Vec<PaneStack> {
        let visible_session_order = match layout_state {
            WorkspaceLayoutState::Grid(state) => state
                .assignments()
                .iter()
                .flatten()
                .copied()
                .collect::<Vec<_>>(),
            WorkspaceLayoutState::SplitTree(state) => state.assigned_sessions(),
        };

        let mut ordered = Vec::with_capacity(stacks.len());
        let mut used_indices = HashSet::new();

        for visible_session_id in visible_session_order {
            if let Some((index, stack)) = stacks.iter().enumerate().find(|(index, stack)| {
                !used_indices.contains(index) && stack.session_ids.contains(&visible_session_id)
            }) {
                used_indices.insert(index);
                ordered.push(stack.clone());
            }
        }

        ordered.extend(
            stacks
                .iter()
                .enumerate()
                .filter(|(index, _)| !used_indices.contains(index))
                .map(|(_, stack)| stack.clone()),
        );

        ordered
    }

    fn stack_session_order(stack: &PaneStack) -> Vec<SessionId> {
        let mut ordered = vec![stack.active_session_id];
        ordered.extend(
            stack
                .session_ids
                .iter()
                .copied()
                .filter(|session_id| *session_id != stack.active_session_id),
        );
        ordered
    }

    fn pane_exists(&self, pane_id: &PaneId) -> bool {
        match pane_id {
            PaneId::GridCell { index } => self
                .layout_state
                .as_grid()
                .is_some_and(|state| *index < state.assignments().len()),
            PaneId::SplitSlot { slot } => self
                .layout_state
                .as_split_tree()
                .is_some_and(|state| state.tree().contains_slot(*slot)),
        }
    }

    fn cleanup_pane_tab_groups(&mut self) {
        let valid_sessions: HashSet<SessionId> =
            self.sessions.iter().map(|session| session.id).collect();
        let valid_panes: HashSet<PaneId> = self.visible_pane_ids().into_iter().collect();
        self.pane_tab_groups.retain(|pane_id, group| {
            if !valid_panes.contains(pane_id) {
                return false;
            }

            group
                .session_ids
                .retain(|session_id| valid_sessions.contains(session_id));
            if !group.session_ids.contains(&group.active_session_id) {
                if let Some(session_id) = group.session_ids.first().copied() {
                    group.active_session_id = session_id;
                }
            }

            group.session_ids.len() > 1
        });

        let mut claimed_sessions: HashSet<SessionId> = self
            .visible_pane_ids()
            .into_iter()
            .flat_map(|pane_id| self.pane_sessions(pane_id).into_iter())
            .collect();
        self.hidden_pane_stacks.retain_mut(|stack| {
            stack.session_ids.retain(|session_id| {
                valid_sessions.contains(session_id) && !claimed_sessions.contains(session_id)
            });
            if stack.session_ids.is_empty() {
                return false;
            }
            if !stack.session_ids.contains(&stack.active_session_id) {
                stack.active_session_id = stack.session_ids[0];
            }
            claimed_sessions.extend(stack.session_ids.iter().copied());
            true
        });
    }

    fn set_pane_active_session(&mut self, pane_id: PaneId, session_id: SessionId) -> bool {
        match pane_id {
            PaneId::GridCell { index } => self
                .layout_state
                .as_grid_mut()
                .and_then(|state| state.replace_session_in_index(index, session_id))
                .is_some(),
            PaneId::SplitSlot { slot } => self
                .layout_state
                .as_split_tree_mut()
                .and_then(|state| state.replace_session_in_slot(slot, session_id))
                .is_some(),
        }
    }

    fn remove_session_from_pane_group(&mut self, pane_id: PaneId, session_id: SessionId) {
        let Some(mut group) = self.pane_tab_groups.remove(&pane_id) else {
            return;
        };

        let was_active = group.active_session_id == session_id;
        group.session_ids.retain(|current| *current != session_id);

        if group.session_ids.len() < 2 {
            if was_active {
                if let Some(next_active) = group.session_ids.first().copied() {
                    let _ = self.set_pane_active_session(pane_id.clone(), next_active);
                }
            }
            return;
        }

        if was_active {
            if let Some(next_active) = group.session_ids.first().copied() {
                if self.set_pane_active_session(pane_id.clone(), next_active) {
                    group.active_session_id = next_active;
                }
            }
        }

        if group.session_ids.len() > 1 {
            self.pane_tab_groups.insert(pane_id, group);
        }
    }

    fn insert_session_into_pane_group(
        &mut self,
        pane_id: PaneId,
        session_id: SessionId,
        make_active: bool,
    ) -> bool {
        let mut group = self
            .pane_tab_groups
            .remove(&pane_id)
            .unwrap_or(PaneTabGroup {
                pane: pane_id.clone(),
                session_ids: self.pane_sessions(pane_id.clone()),
                active_session_id: self
                    .active_session_for_pane(pane_id.clone())
                    .unwrap_or(session_id),
            });

        if group.session_ids.is_empty() {
            group.session_ids.push(session_id);
            group.active_session_id = session_id;
        } else if !group.session_ids.contains(&session_id) {
            group.session_ids.push(session_id);
        }

        if make_active {
            if !self.set_pane_active_session(pane_id.clone(), session_id) {
                self.pane_tab_groups.insert(pane_id, group);
                return false;
            }
            group.active_session_id = session_id;
        }

        if group.session_ids.len() > 1 {
            self.pane_tab_groups.insert(pane_id, group);
        }
        true
    }

    fn swap_pane_groups(&mut self, pane_a: PaneId, pane_b: PaneId) {
        let group_a = self.pane_tab_groups.remove(&pane_a);
        let group_b = self.pane_tab_groups.remove(&pane_b);

        if let Some(mut group) = group_a {
            group.pane = pane_b.clone();
            self.pane_tab_groups.insert(pane_b, group);
        }

        if let Some(mut group) = group_b {
            group.pane = pane_a.clone();
            self.pane_tab_groups.insert(pane_a, group);
        }
    }

    fn rebuild_grid_state(&self, profile: LayoutProfile, stacks: &[PaneStack]) -> LayoutState {
        let focused = self.layout_state.focused_session();
        let mut grid_state = LayoutState::with_profile(profile);
        let visible_panes = profile.max_sessions();
        let visible_sessions = stacks
            .iter()
            .take(visible_panes)
            .map(|stack| stack.active_session_id)
            .collect::<Vec<_>>();
        grid_state.set_assignments(visible_sessions);

        for stack in stacks.iter().skip(visible_panes) {
            for session_id in Self::stack_session_order(stack) {
                let _ = grid_state.append_hidden_session(session_id);
            }
        }

        if let Some(session_id) = focused {
            if !grid_state.focus_session(session_id) && profile == LayoutProfile::Single {
                let _ = grid_state.swap_hidden_into_index(session_id, 0);
            }
        }

        if grid_state.focused_session().is_none() {
            if let Some(session_id) = grid_state.assignments().iter().flatten().copied().next() {
                grid_state.focus_session(session_id);
            }
        }

        if profile != LayoutProfile::Single {
            let visible_len = grid_state.occupied_indices().len();
            if grid_state
                .focused_index()
                .is_some_and(|index| index >= profile.max_sessions())
                && visible_len > 0
            {
                if let Some(index) = grid_state.occupied_indices().into_iter().next() {
                    grid_state.focus_index(index);
                }
            }
        }

        grid_state
    }

    fn rebuild_split_state(&self, tree: LayoutNode, stacks: &[PaneStack]) -> SplitLayoutState {
        let focused = self.layout_state.focused_session();
        let mut split_state = SplitLayoutState::new(tree);

        for session_id in stacks
            .iter()
            .take(split_state.slot_count())
            .map(|stack| stack.active_session_id)
        {
            if !split_state.add_session(session_id) {
                break;
            }
        }

        if let Some(session_id) = focused {
            split_state.focus_session(session_id);
        }

        if split_state.focused_session().is_none() {
            if let Some(session_id) = split_state.assigned_sessions().first().copied() {
                split_state.focus_session(session_id);
            }
        }

        split_state
    }

    /// Fill any newly available split panes with sessions that were hidden
    /// while the layout had fewer slots than active sessions.
    fn promote_hidden_sessions_into_split_slots(&mut self) {
        let hidden_session_ids = {
            let Some(split_state) = self.layout_state.as_split_tree() else {
                return;
            };

            if split_state.available_slots() == 0 {
                return;
            }

            let assigned: HashSet<SessionId> =
                split_state.assigned_sessions().into_iter().collect();
            self.sessions
                .iter()
                .map(|session| session.id)
                .filter(|session_id| !assigned.contains(session_id))
                .collect::<Vec<_>>()
        };

        if hidden_session_ids.is_empty() {
            return;
        }

        if let Some(split_state) = self.layout_state.as_split_tree_mut() {
            for session_id in hidden_session_ids {
                if !split_state.add_session(session_id) {
                    break;
                }
            }

            if split_state.focused_session().is_none() {
                if let Some(session_id) = split_state.assigned_sessions().first().copied() {
                    split_state.focus_session(session_id);
                }
            }
        }
    }

    /// Fill any newly available grid cells with hidden sessions in overflow
    /// order so deleting a visible session keeps the grid as full as possible.
    fn promote_hidden_sessions_into_grid_slots(&mut self) {
        let Some(grid_state) = self.layout_state.as_grid_mut() else {
            return;
        };

        while let (Some(index), Some(session_id)) = (
            grid_state.first_empty_index(),
            grid_state.overflow().first().copied(),
        ) {
            if !grid_state.assign_session_to_index(session_id, index) {
                break;
            }
        }

        if grid_state.focused_session().is_none() {
            if let Some(index) = grid_state.occupied_indices().into_iter().next() {
                grid_state.focus_index(index);
            }
        }
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
    /// The session is automatically assigned to the next available slot.
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

        match &mut self.layout_state {
            WorkspaceLayoutState::Grid(s) => s.add_session(id),
            WorkspaceLayoutState::SplitTree(s) => s.add_session(id),
        };

        self.sessions.push(session);

        // Focus the new session if it's the first one
        if self.layout_state.focused_session().is_none() {
            self.layout_state.focus_session(id);
        }

        true
    }

    /// Add a session to a specific slot in the split tree.
    pub fn add_session_to_slot(&mut self, session: Session, slot: SlotId) -> bool {
        let id = session.id;
        if self.sessions.iter().any(|s| s.id == id) {
            return false;
        }
        if !self.layout_state.add_session_to_slot(id, slot) {
            return false;
        }
        self.sessions.push(session);
        if self.layout_state.focused_session().is_none() {
            self.layout_state.focus_session(id);
        }
        true
    }

    /// Add a session to a specific visible pane as a new active tab.
    pub fn add_session_to_pane(&mut self, session: Session, pane_id: PaneId) -> bool {
        let id = session.id;
        if self.sessions.iter().any(|existing| existing.id == id) {
            return false;
        }

        self.sessions.push(session);

        if self.active_session_for_pane(pane_id.clone()).is_none() {
            match pane_id {
                PaneId::GridCell { index } => {
                    let Some(state) = self.layout_state.as_grid_mut() else {
                        self.sessions.retain(|session| session.id != id);
                        return false;
                    };
                    if !state.assign_session_to_index(id, index) {
                        self.sessions.retain(|session| session.id != id);
                        return false;
                    }
                    state.focus_index(index);
                }
                PaneId::SplitSlot { slot } => {
                    let Some(state) = self.layout_state.as_split_tree_mut() else {
                        self.sessions.retain(|session| session.id != id);
                        return false;
                    };
                    if !state.assign_session_to_slot(id, slot) {
                        self.sessions.retain(|session| session.id != id);
                        return false;
                    }
                    state.focus_slot(slot);
                }
            }
            return true;
        }

        if !self.insert_session_into_pane_group(pane_id, id, true) {
            self.sessions.retain(|session| session.id != id);
            return false;
        }

        true
    }

    /// Remove a session from the workspace.
    ///
    /// # Returns
    ///
    /// The removed session, if found.
    pub fn remove_session(&mut self, id: SessionId) -> Option<Session> {
        if let Some(pane_id) = self.pane_id_for_session(id) {
            self.remove_session_from_pane_group(pane_id, id);
        }

        // Remove from layout
        self.layout_state.remove_session(id);

        // Remove from sessions list
        if let Some(pos) = self.sessions.iter().position(|s| s.id == id) {
            let removed = self.sessions.remove(pos);
            self.cleanup_pane_tab_groups();
            self.promote_hidden_sessions_into_grid_slots();
            self.promote_hidden_sessions_into_split_slots();
            Some(removed)
        } else {
            None
        }
    }

    /// Update a session's status.
    pub fn update_session_status(&mut self, id: SessionId, status: SessionStatus) -> bool {
        if let Some(session) = self.session_mut(id) {
            if session.status != status {
                session.status = status;
                return true;
            }
        }
        false
    }

    /// Sync session metadata from the authoritative source (SessionManager).
    ///
    /// Copies all fields from `manager_sessions` into the workspace's cached
    /// sessions, **except `status`** which is owned by the detector/UI side.
    /// This replaces the previous piecemeal dual-write pattern and ensures the
    /// workspace cache never drifts from the manager.
    pub fn sync_sessions_from_manager(&mut self, manager_sessions: &[Session]) {
        for src in manager_sessions {
            if let Some(dst) = self.session_mut(src.id) {
                dst.name = src.name.clone();
                dst.working_directory = src.working_directory.clone();
                dst.shell = src.shell.clone();
                dst.current_task = src.current_task.clone();
                dst.context_usage = src.context_usage;
                dst.group = src.group.clone();
                dst.color = src.color.clone();
                dst.git_info = src.git_info.clone();
                dst.claude_session_id = src.claude_session_id.clone();
                dst.codex_session_id = src.codex_session_id.clone();
                dst.codex_execution_mode = src.codex_execution_mode;
                dst.codex_started_at = src.codex_started_at;
                dst.gemini_session_id = src.gemini_session_id.clone();
                // `status` is NOT synced — the detector is the authority.
                // `id` and `created_at` are immutable.
            }
        }
    }

    /// Get the visible sessions (those assigned to cells/slots).
    pub fn visible_sessions(&self) -> Vec<&Session> {
        self.visible_session_ids()
            .into_iter()
            .filter_map(|id| self.session(id))
            .collect()
    }

    /// Get the IDs of sessions currently visible in rendered panes.
    pub fn visible_session_ids(&self) -> Vec<SessionId> {
        self.visible_pane_ids()
            .into_iter()
            .filter_map(|pane_id| self.active_session_for_pane(pane_id))
            .collect()
    }

    /// Returns whether a session is currently visible in the active layout.
    pub fn is_session_visible(&self, session_id: SessionId) -> bool {
        self.pane_id_for_session(session_id).is_some()
    }

    /// Get the number of sessions that can still be added.
    pub fn available_slots(&self) -> usize {
        match &self.layout_state {
            WorkspaceLayoutState::Grid(s) => {
                s.assignments().iter().filter(|cell| cell.is_none()).count()
            }
            WorkspaceLayoutState::SplitTree(s) => s.available_slots(),
        }
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

    /// Get the currently focused visible pane.
    pub fn focused_pane_id(&self) -> Option<PaneId> {
        match &self.layout_state {
            WorkspaceLayoutState::Grid(state) => state
                .focused_index()
                .map(|index| PaneId::GridCell { index }),
            WorkspaceLayoutState::SplitTree(state) => {
                state.focused_slot().map(|slot| PaneId::SplitSlot { slot })
            }
        }
    }

    /// Get all sessions currently attached to the focused visible pane.
    pub fn focused_pane_session_ids(&self) -> Vec<SessionId> {
        self.focused_pane_id()
            .map(|pane_id| self.pane_sessions(pane_id))
            .unwrap_or_default()
    }

    /// Focus a session by ID.
    ///
    /// # Returns
    ///
    /// `true` if the session was found and focused.
    pub fn focus_session(&mut self, id: SessionId) -> bool {
        if self.session(id).is_none() {
            return false;
        }

        if let Some(pane_id) = self.pane_id_for_session(id) {
            let should_activate = self
                .pane_tab_groups
                .get(&pane_id)
                .is_some_and(|group| group.session_ids.contains(&id))
                && self.active_session_for_pane(pane_id.clone()) != Some(id);

            if should_activate {
                if !self.set_pane_active_session(pane_id.clone(), id) {
                    return false;
                }
                if let Some(group) = self.pane_tab_groups.get_mut(&pane_id) {
                    group.active_session_id = id;
                }
            }
        }

        let focused = match &mut self.layout_state {
            WorkspaceLayoutState::Grid(s) => {
                if s.profile() == LayoutProfile::Single {
                    if let Some(visible_index) =
                        s.assignments().iter().position(|cell| *cell == Some(id))
                    {
                        s.focus_index(visible_index);
                        true
                    } else {
                        let replacement_index = s.focused_index().or(Some(0));
                        let Some(replacement_index) = replacement_index else {
                            return false;
                        };
                        s.swap_hidden_into_index(id, replacement_index).is_some()
                    }
                } else if let Some(target_index) =
                    s.assignments().iter().position(|cell| *cell == Some(id))
                {
                    s.focus_index(target_index);
                    true
                } else {
                    let replacement_index = s
                        .first_empty_index()
                        .or_else(|| s.focused_index())
                        .or_else(|| s.occupied_indices().into_iter().next())
                        .or_else(|| s.first_empty_index());

                    let Some(replacement_index) = replacement_index else {
                        return false;
                    };

                    if s.session_at(replacement_index).is_none() {
                        if s.assign_session_to_index(id, replacement_index) {
                            s.focus_index(replacement_index);
                            true
                        } else {
                            false
                        }
                    } else {
                        s.swap_hidden_into_index(id, replacement_index).is_some()
                    }
                }
            }
            WorkspaceLayoutState::SplitTree(s) => {
                if s.focus_session(id) {
                    true
                } else {
                    let target_slot = s
                        .focused_slot()
                        .or_else(|| {
                            s.assignments().iter().find_map(|(slot, session)| {
                                if session.is_some()
                                    || self
                                        .pane_tab_groups
                                        .contains_key(&PaneId::SplitSlot { slot: *slot })
                                {
                                    Some(*slot)
                                } else {
                                    None
                                }
                            })
                        })
                        .or_else(|| {
                            s.assignments()
                                .iter()
                                .find_map(|(slot, session)| session.is_none().then_some(*slot))
                        });

                    let Some(target_slot) = target_slot else {
                        return false;
                    };

                    s.replace_session_in_slot(target_slot, id).is_some()
                }
            }
        };

        if focused {
            self.cleanup_pane_tab_groups();
        }
        focused
    }

    /// Focus a session by grid index (1-based, for keyboard shortcuts).
    ///
    /// # Returns
    ///
    /// `true` if the index is valid and session was focused.
    pub fn focus_session_number(&mut self, number: usize) -> bool {
        match &mut self.layout_state {
            WorkspaceLayoutState::Grid(s) => {
                let visible_indices = s.occupied_indices();
                if number == 0 || number > visible_indices.len() {
                    return false;
                }
                s.focus_index(visible_indices[number - 1]);
                true
            }
            WorkspaceLayoutState::SplitTree(s) => {
                let sessions = s.assigned_sessions();
                if number == 0 || number > sessions.len() {
                    return false;
                }
                s.focus_session(sessions[number - 1])
            }
        }
    }

    /// Focus the next session.
    pub fn focus_next(&mut self) {
        match &mut self.layout_state {
            WorkspaceLayoutState::Grid(s) if s.profile() != LayoutProfile::Single => {
                let occupied = s.occupied_indices();
                if occupied.is_empty() {
                    return;
                }
                let next = match s
                    .focused_index()
                    .and_then(|index| occupied.iter().position(|&current| current == index))
                {
                    Some(index) => (index + 1) % occupied.len(),
                    None => 0,
                };
                s.focus_index(occupied[next]);
            }
            _ => self.layout_state.focus_next(),
        }
    }

    /// Focus the previous session.
    pub fn focus_previous(&mut self) {
        match &mut self.layout_state {
            WorkspaceLayoutState::Grid(s) if s.profile() != LayoutProfile::Single => {
                let occupied = s.occupied_indices();
                if occupied.is_empty() {
                    return;
                }
                let prev = match s
                    .focused_index()
                    .and_then(|index| occupied.iter().position(|&current| current == index))
                {
                    Some(index) if index > 0 => index - 1,
                    Some(_) => occupied.len() - 1,
                    None => 0,
                };
                s.focus_index(occupied[prev]);
            }
            _ => self.layout_state.focus_previous(),
        }
    }

    /// Focus in a direction (for arrow key navigation).
    pub fn focus_direction(&mut self, direction: FocusDirection) {
        // Extract bounds and gap before mutable borrow of layout_state
        let bounds = self.grid_bounds();
        let gap = self.theme.grid_gap;
        match &mut self.layout_state {
            WorkspaceLayoutState::Grid(s) => s.focus_direction(direction),
            WorkspaceLayoutState::SplitTree(s) => {
                let layout = SplitLayout::new(s.tree().clone(), bounds, gap);
                s.focus_direction(direction, &layout);
            }
        }
    }

    /// Apply a split tree layout directly.
    ///
    /// Transfers current sessions and focus to the new tree.
    pub fn set_split_tree(&mut self, tree: LayoutNode) {
        let stacks = self.layout_transition_stacks();
        let rebuilt_layout =
            WorkspaceLayoutState::SplitTree(self.rebuild_split_state(tree, &stacks));
        let ordered_stacks = Self::stacks_ordered_for_layout_state(&rebuilt_layout, &stacks);
        self.layout_state = rebuilt_layout;
        self.apply_pane_stacks_to_current_layout(ordered_stacks);
    }

    // --- Split Pane Operations ---

    /// Split the focused pane into two.
    ///
    /// Switches to split tree mode if currently in grid mode.
    /// Returns the new slot ID, or None if no pane is focused.
    pub fn split_pane(&mut self, direction: SplitDirection, ratio: f32) -> Option<SlotId> {
        // Ensure we're in split tree mode
        if self.layout_state.is_grid() {
            self.convert_to_split_tree();
        }

        let new_slot = if let WorkspaceLayoutState::SplitTree(s) = &mut self.layout_state {
            let target = s.focused_slot()?;
            s.split_slot(target, direction, ratio)
        } else {
            None
        };

        if new_slot.is_some() {
            self.promote_hidden_sessions_into_split_slots();
        }

        new_slot
    }

    /// Close a pane (slot), promoting its sibling.
    ///
    /// If only one pane remains, switches back to grid mode.
    pub fn close_pane(&mut self) -> bool {
        let (result, should_switch_to_single) =
            if let WorkspaceLayoutState::SplitTree(s) = &mut self.layout_state {
                if let Some(target) = s.focused_slot() {
                    self.pane_tab_groups
                        .remove(&PaneId::SplitSlot { slot: target });
                    let result = s.close_slot(target);
                    let should_switch = result && s.slot_count() == 1;
                    (result, should_switch)
                } else {
                    return false;
                }
            } else {
                return false;
            };

        if should_switch_to_single {
            let stacks = self.layout_transition_stacks();
            let rebuilt_layout =
                WorkspaceLayoutState::Grid(self.rebuild_grid_state(LayoutProfile::Single, &stacks));
            let ordered_stacks = Self::stacks_ordered_for_layout_state(&rebuilt_layout, &stacks);
            self.layout_state = rebuilt_layout;
            self.apply_pane_stacks_to_current_layout(ordered_stacks);
        }

        result
    }

    /// Resize a split by updating the ratio for the parent of the focused slot.
    pub fn resize_split(&mut self, new_ratio: f32) -> bool {
        if let WorkspaceLayoutState::SplitTree(s) = &mut self.layout_state {
            if let Some(target) = s.focused_slot() {
                return s.resize_split(target, new_ratio);
            }
        }
        false
    }

    /// Resize a specific split divider identified by representative slots on
    /// either side of the split.
    pub fn resize_split_divider(
        &mut self,
        first_slot: SlotId,
        second_slot: SlotId,
        new_ratio: f32,
    ) -> bool {
        if let WorkspaceLayoutState::SplitTree(s) = &mut self.layout_state {
            return s.resize_divider(first_slot, second_slot, new_ratio);
        }
        false
    }

    /// Convert the current grid layout to an equivalent split tree.
    fn convert_to_split_tree(&mut self) {
        let stacks = self.layout_transition_stacks();
        let tree = if let WorkspaceLayoutState::Grid(grid_state) = &self.layout_state {
            let (rows, cols) = grid_state.profile().dimensions();
            Some(LayoutNode::from_grid(rows, cols))
        } else {
            None
        };

        if let Some(tree) = tree {
            let rebuilt_layout =
                WorkspaceLayoutState::SplitTree(self.rebuild_split_state(tree, &stacks));
            let ordered_stacks = Self::stacks_ordered_for_layout_state(&rebuilt_layout, &stacks);
            self.layout_state = rebuilt_layout;
            self.apply_pane_stacks_to_current_layout(ordered_stacks);
        }
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

    /// Set the sidebar width (no-op if unchanged within epsilon).
    pub fn set_sidebar_width(&mut self, width: f32) {
        let clamped = width.max(0.0);
        if (self.sidebar_width - clamped).abs() < 0.1 {
            return;
        }
        self.sidebar_width = clamped;
    }

    /// Set the right panel width (0.0 when closed, no-op if unchanged within epsilon).
    pub fn set_right_panel_width(&mut self, width: f32) {
        let clamped = width.max(0.0);
        if (self.right_panel_width - clamped).abs() < 0.1 {
            return;
        }
        self.right_panel_width = clamped;
    }

    // --- Theme ---

    /// Get the current theme.
    pub fn theme(&self) -> &CodirigentTheme {
        &self.theme
    }

    /// Set the theme.
    pub fn set_theme(&mut self, theme: CodirigentTheme) {
        self.theme = theme;
    }

    /// Get a mutable reference to the theme for live settings updates.
    pub fn theme_mut(&mut self) -> &mut CodirigentTheme {
        &mut self.theme
    }

    // --- Bounds ---

    /// Set the workspace bounds (called on window resize, no-op if unchanged).
    pub fn set_bounds(&mut self, bounds: Bounds) {
        if self.bounds == bounds {
            return;
        }
        self.bounds = bounds;
    }

    /// Get the workspace bounds.
    pub fn bounds(&self) -> Bounds {
        self.bounds
    }

    /// Get the bounds available for the grid (excluding sidebar and chrome).
    ///
    /// Subtracts vertical chrome (title bar, top bar, grid padding) and
    /// horizontal chrome (sidebar, grid container padding) so that
    /// `cell_size()` and `cell_bounds_for_index()` return dimensions
    /// matching the actual GPUI flex-allocated space.
    pub fn grid_bounds(&self) -> Bounds {
        // Title bar (32px) + Top bar (48px) + grid container padding (gap * 2)
        let chrome_height = 32.0 + TOP_BAR_HEIGHT + self.theme.grid_gap * 2.0;
        // Grid container has .p(px(grid_gap)) padding on all sides
        let grid_padding_h = self.theme.grid_gap * 2.0;

        let x = if self.show_sidebar {
            self.sidebar_width
        } else {
            self.bounds.origin.x
        };

        let width = if self.show_sidebar {
            (self.bounds.size.width - self.sidebar_width - self.right_panel_width - grid_padding_h)
                .max(0.0)
        } else {
            (self.bounds.size.width - self.right_panel_width - grid_padding_h).max(0.0)
        };

        // Subtract chrome heights from vertical space
        let y = self.bounds.origin.y + chrome_height;
        let height = (self.bounds.size.height - chrome_height).max(0.0);

        Bounds::new(x, y, width, height)
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
        match self.pane_id_for_session(id)? {
            PaneId::GridCell { index } => self.grid_layout().cell_bounds_for_index(index),
            PaneId::SplitSlot { slot } => {
                let layout = self.split_layout()?;
                layout
                    .leaf_bounds()
                    .into_iter()
                    .find(|(current, _)| *current == slot)
                    .map(|(_, bounds)| bounds)
            }
        }
    }

    /// Get the session at a point in the grid.
    pub fn session_at_point(&self, point: Point) -> Option<SessionId> {
        let grid_bounds = self.grid_bounds();

        // Check if point is in grid area
        if !grid_bounds.contains(point) {
            return None;
        }

        match &self.layout_state {
            WorkspaceLayoutState::Grid(s) => {
                // Adjust point relative to grid origin
                let adjusted_point = Point::new(
                    point.x - grid_bounds.origin.x,
                    point.y - grid_bounds.origin.y,
                );
                let local_bounds =
                    Bounds::from_size(grid_bounds.size.width, grid_bounds.size.height);
                let layout =
                    GridLayout::from_profile(s.profile(), local_bounds, self.theme.grid_gap);
                let position = layout.cell_at_point(adjusted_point)?;
                s.session_at_position(position)
            }
            WorkspaceLayoutState::SplitTree(s) => {
                let layout = self.split_layout()?;
                let slot = layout.slot_at_point(point)?;
                s.session_at_slot(slot)
            }
        }
    }

    /// Swap two sessions by their cell/slot index.
    ///
    /// This only changes which session is assigned to which position —
    /// the layout structure (grid dimensions or split tree shape) is unchanged.
    ///
    /// Returns `true` if the swap was performed.
    pub fn swap_sessions(&mut self, index_a: usize, index_b: usize) -> bool {
        match &mut self.layout_state {
            WorkspaceLayoutState::Grid(s) => {
                let swapped = s.swap_assignments(index_a, index_b);
                if swapped {
                    self.swap_pane_groups(
                        PaneId::GridCell { index: index_a },
                        PaneId::GridCell { index: index_b },
                    );
                }
                swapped
            }
            WorkspaceLayoutState::SplitTree(s) => {
                let Some(pane_a) = s
                    .assignments()
                    .get(index_a)
                    .map(|(slot, _)| PaneId::SplitSlot { slot: *slot })
                else {
                    return false;
                };
                let Some(pane_b) = s
                    .assignments()
                    .get(index_b)
                    .map(|(slot, _)| PaneId::SplitSlot { slot: *slot })
                else {
                    return false;
                };
                let swapped = s.swap_assignments(index_a, index_b);
                if swapped {
                    self.swap_pane_groups(pane_a, pane_b);
                }
                swapped
            }
        }
    }

    // --- State for Rendering ---

    /// Get information about each visible cell for rendering.
    ///
    /// In Single layout mode, only returns the focused session.
    /// This ensures that when switching to Single layout, the currently
    /// focused session is displayed, not just the first session.
    pub fn cell_info(&self) -> Vec<CellInfo> {
        match &self.layout_state {
            WorkspaceLayoutState::Grid(s) => self.grid_cell_info(s),
            WorkspaceLayoutState::SplitTree(s) => self.split_cell_info(s),
        }
    }

    fn grid_cell_info(&self, state: &LayoutState) -> Vec<CellInfo> {
        let layout = self.grid_layout();

        // Special handling for Single layout: only show the focused session
        if state.profile() == LayoutProfile::Single {
            if let Some(focused_session_id) = state.focused_session() {
                if self.session(focused_session_id).is_some() {
                    if let Some(bounds) = layout.cell_bounds_for_index(0) {
                        return vec![CellInfo {
                            pane_id: PaneId::GridCell { index: 0 },
                            session_id: focused_session_id,
                            index: 0,
                            bounds,
                        }];
                    }
                }
            }
            // If no focused session, return empty (shouldn't happen in practice)
            return vec![];
        }

        // For other layouts, show all assigned sessions
        state
            .assignments()
            .iter()
            .enumerate()
            .filter_map(|(index, session_id)| {
                let session_id = (*session_id)?;
                let bounds = layout.cell_bounds_for_index(index)?;
                self.session(session_id)?;
                Some(CellInfo {
                    pane_id: PaneId::GridCell { index },
                    session_id,
                    index,
                    bounds,
                })
            })
            .collect()
    }

    fn split_cell_info(&self, state: &SplitLayoutState) -> Vec<CellInfo> {
        let Some(layout) = self.split_layout() else {
            return vec![];
        };
        let leaf_bounds = layout.leaf_bounds();

        leaf_bounds
            .into_iter()
            .enumerate()
            .filter_map(|(index, (slot, bounds))| {
                let session_id = state.session_at_slot(slot)?;
                self.session(session_id)?;
                Some(CellInfo {
                    pane_id: PaneId::SplitSlot { slot },
                    session_id,
                    index,
                    bounds,
                })
            })
            .collect()
    }

    /// Get pane bounds for drag/drop hit testing, including empty panes.
    pub fn pane_drop_target_info(&self) -> Vec<PaneDropTargetInfo> {
        match &self.layout_state {
            WorkspaceLayoutState::Grid(state) => self.grid_pane_drop_target_info(state),
            WorkspaceLayoutState::SplitTree(state) => self.split_pane_drop_target_info(state),
        }
    }

    fn grid_pane_drop_target_info(&self, state: &LayoutState) -> Vec<PaneDropTargetInfo> {
        let layout = self.grid_layout();
        state
            .assignments()
            .iter()
            .enumerate()
            .filter_map(|(index, session_id)| {
                let bounds = layout.cell_bounds_for_index(index)?;
                Some(PaneDropTargetInfo {
                    pane_id: PaneId::GridCell { index },
                    active_session_id: *session_id,
                    index,
                    bounds,
                })
            })
            .collect()
    }

    fn split_pane_drop_target_info(&self, state: &SplitLayoutState) -> Vec<PaneDropTargetInfo> {
        let Some(layout) = self.split_layout() else {
            return Vec::new();
        };

        layout
            .leaf_bounds()
            .into_iter()
            .enumerate()
            .map(|(index, (slot, bounds))| PaneDropTargetInfo {
                pane_id: PaneId::SplitSlot { slot },
                active_session_id: state.session_at_slot(slot),
                index,
                bounds,
            })
            .collect()
    }

    /// Get all tab sessions for a pane in display order.
    pub fn pane_tab_session_ids(&self, pane_id: PaneId) -> Vec<SessionId> {
        self.pane_sessions(pane_id)
    }

    /// Get the active session rendered in a pane.
    pub fn pane_active_session_id(&self, pane_id: PaneId) -> Option<SessionId> {
        self.active_session_for_pane(pane_id)
    }

    /// Activate a tab within a visible pane.
    pub fn activate_pane_tab(&mut self, pane_id: PaneId, session_id: SessionId) -> bool {
        let Some(group) = self.pane_tab_groups.get(&pane_id) else {
            return false;
        };
        if !group.session_ids.contains(&session_id) {
            return false;
        }
        if !self.set_pane_active_session(pane_id.clone(), session_id) {
            return false;
        }
        if let Some(group) = self.pane_tab_groups.get_mut(&pane_id) {
            group.active_session_id = session_id;
        }
        self.focus_session(session_id)
    }

    /// Group a session into the target pane as an active tab.
    pub fn group_session_into_pane(&mut self, session_id: SessionId, target_pane: PaneId) -> bool {
        let Some(source_pane) = self.pane_id_for_session(session_id) else {
            return false;
        };
        if source_pane == target_pane {
            return false;
        }

        if !self.pane_exists(&target_pane) {
            return false;
        }

        let removed_from_source = match source_pane.clone() {
            PaneId::GridCell { index } => {
                if self.pane_tab_groups.contains_key(&source_pane) {
                    self.remove_session_from_pane_group(source_pane.clone(), session_id);
                    true
                } else {
                    self.layout_state
                        .as_grid_mut()
                        .and_then(|state| state.clear_index(index))
                        == Some(session_id)
                }
            }
            PaneId::SplitSlot { .. } => {
                if self.pane_tab_groups.contains_key(&source_pane) {
                    self.remove_session_from_pane_group(source_pane.clone(), session_id);
                    true
                } else {
                    self.layout_state.remove_session(session_id)
                }
            }
        };

        if !removed_from_source {
            return false;
        }

        if self.active_session_for_pane(target_pane.clone()).is_none() {
            let assigned = match target_pane.clone() {
                PaneId::GridCell { index } => self
                    .layout_state
                    .as_grid_mut()
                    .is_some_and(|state| state.assign_session_to_index(session_id, index)),
                PaneId::SplitSlot { slot } => self
                    .layout_state
                    .as_split_tree_mut()
                    .is_some_and(|state| state.assign_session_to_slot(session_id, slot)),
            };
            if assigned {
                self.cleanup_pane_tab_groups();
                return self.focus_session(session_id);
            }
            return false;
        }

        let grouped = self.insert_session_into_pane_group(target_pane, session_id, true);
        self.cleanup_pane_tab_groups();
        grouped && self.focus_session(session_id)
    }

    /// Restore persisted pane stacks after sessions have been recreated.
    pub fn restore_pane_stacks(
        &mut self,
        saved_stacks: &[PaneStackState],
        restored_session_ids: &HashMap<SessionId, SessionId>,
    ) {
        let stacks = saved_stacks
            .iter()
            .filter_map(|saved_stack| {
                let session_ids = saved_stack
                    .session_ids
                    .iter()
                    .filter_map(|saved_id| restored_session_ids.get(saved_id).copied())
                    .collect::<Vec<_>>();
                if session_ids.is_empty() {
                    return None;
                }

                Some(PaneStack {
                    active_session_id: restored_session_ids
                        .get(&saved_stack.active_session_id)
                        .copied()
                        .filter(|session_id| session_ids.contains(session_id))
                        .unwrap_or(session_ids[0]),
                    session_ids,
                })
            })
            .collect::<Vec<_>>();
        self.restore_runtime_pane_stacks(stacks);
    }

    fn restore_runtime_pane_stacks(&mut self, mut stacks: Vec<PaneStack>) {
        let assigned: HashSet<SessionId> = stacks
            .iter()
            .flat_map(|stack| stack.session_ids.iter().copied())
            .collect();
        stacks.extend(
            self.sessions
                .iter()
                .map(|session| session.id)
                .filter(|session_id| !assigned.contains(session_id))
                .map(|session_id| PaneStack {
                    session_ids: vec![session_id],
                    active_session_id: session_id,
                }),
        );

        let rebuilt_layout = match &self.layout_state {
            WorkspaceLayoutState::Grid(state) => {
                WorkspaceLayoutState::Grid(self.rebuild_grid_state(state.profile(), &stacks))
            }
            WorkspaceLayoutState::SplitTree(state) => WorkspaceLayoutState::SplitTree(
                self.rebuild_split_state(state.tree().clone(), &stacks),
            ),
        };
        let ordered_stacks = Self::stacks_ordered_for_layout_state(&rebuilt_layout, &stacks);
        self.layout_state = rebuilt_layout;
        self.apply_pane_stacks_to_current_layout(ordered_stacks);
    }

    /// Restore legacy persisted pane tab groups after sessions have been recreated.
    pub fn restore_pane_tab_groups(
        &mut self,
        saved_groups: &[PaneTabGroup],
        restored_session_ids: &HashMap<SessionId, SessionId>,
    ) {
        for group in saved_groups {
            if !self.pane_exists(&group.pane) {
                continue;
            }
            let session_ids = group
                .session_ids
                .iter()
                .filter_map(|saved_id| restored_session_ids.get(saved_id).copied())
                .collect::<Vec<_>>();
            if session_ids.is_empty() {
                continue;
            }

            let active_session_id = restored_session_ids
                .get(&group.active_session_id)
                .copied()
                .filter(|session_id| session_ids.contains(session_id))
                .unwrap_or(session_ids[0]);

            for session_id in &session_ids {
                if self.pane_id_for_session(*session_id) != Some(group.pane.clone()) {
                    let _ = self.group_session_into_pane(*session_id, group.pane.clone());
                }
            }

            if let Some(restored_group) = self.pane_tab_groups.get_mut(&group.pane) {
                restored_group.session_ids = session_ids.clone();
                restored_group.active_session_id = active_session_id;
            }
            let _ = self.activate_pane_tab(group.pane.clone(), active_session_id);
        }

        self.cleanup_pane_tab_groups();
    }
}

/// Information about a grid cell for rendering.
#[derive(Debug, Clone)]
pub struct CellInfo {
    /// Visible pane identifier.
    pub pane_id: PaneId,
    /// Session ID.
    pub session_id: SessionId,
    /// Grid index (0-based).
    pub index: usize,
    /// Cell bounds.
    pub bounds: Bounds,
}

/// Pane bounds used for drag/drop hit testing.
#[derive(Debug, Clone)]
pub struct PaneDropTargetInfo {
    /// Visible pane identifier.
    pub pane_id: PaneId,
    /// Active session assigned to the pane, if any.
    pub active_session_id: Option<SessionId>,
    /// Logical grid/split ordering index.
    pub index: usize,
    /// Pane bounds in workspace coordinates.
    pub bounds: Bounds,
}
