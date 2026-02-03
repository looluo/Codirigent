//! Git worktree panel component.
//!
//! Provides UI for managing git worktrees, allowing users to:
//! - View existing worktrees with branch names
//! - See session bindings
//! - Create new worktrees
//! - Remove worktrees
//! - Cleanup merged worktrees

use codirigent_core::{SessionId, Worktree, WorktreeCreateOptions};
use std::path::PathBuf;

/// Worktree panel events.
#[derive(Debug, Clone, PartialEq)]
pub enum WorktreeEvent {
    /// Create worktree button clicked.
    CreateClicked,
    /// Remove worktree requested.
    RemoveRequested(PathBuf),
    /// Bind session to worktree.
    BindSession {
        /// Worktree path.
        worktree_path: PathBuf,
        /// Session ID to bind.
        session_id: SessionId,
    },
    /// Unbind session from worktree.
    UnbindSession(SessionId),
    /// Cleanup merged worktrees.
    CleanupMerged,
    /// Refresh worktree list.
    Refresh,
    /// Confirm worktree creation.
    ConfirmCreate {
        /// Branch name.
        branch: String,
        /// Base branch (if creating new).
        base_branch: Option<String>,
    },
    /// Cancel create modal.
    CancelCreate,
}

/// Worktree panel state.
#[derive(Debug)]
pub struct WorktreePanel {
    /// List of worktrees.
    worktrees: Vec<Worktree>,
    /// Whether the create modal is open.
    create_modal_open: bool,
    /// Branch name input for create modal.
    branch_input: String,
    /// Base branch selection for create modal.
    base_branch_input: String,
    /// Whether to create from existing branch.
    use_existing_branch: bool,
    /// Available branches for selection.
    available_branches: Vec<String>,
    /// Whether the existing-branch dropdown is open.
    branch_dropdown_open: bool,
    /// Panel height.
    height: f32,
    /// Which input field has focus (0 = branch, 1 = base_branch, None = no focus).
    focused_input: Option<usize>,
}

impl Default for WorktreePanel {
    fn default() -> Self {
        Self::new()
    }
}

impl WorktreePanel {
    /// Default panel height.
    pub const DEFAULT_HEIGHT: f32 = 300.0;
    /// Item height in pixels.
    pub const ITEM_HEIGHT: f32 = 40.0;
    /// Header height in pixels.
    pub const HEADER_HEIGHT: f32 = 36.0;

    /// Create a new worktree panel.
    pub fn new() -> Self {
        Self {
            worktrees: Vec::new(),
            create_modal_open: false,
            branch_input: String::new(),
            base_branch_input: String::from("main"),
            use_existing_branch: false,
            available_branches: Vec::new(),
            height: Self::DEFAULT_HEIGHT,
            focused_input: None,
            branch_dropdown_open: false,
        }
    }

    /// Update the worktree list.
    pub fn set_worktrees(&mut self, worktrees: Vec<Worktree>) {
        self.worktrees = worktrees;
    }

    /// Get the current worktree list.
    pub fn worktrees(&self) -> &[Worktree] {
        &self.worktrees
    }

    /// Set available branches for selection.
    pub fn set_available_branches(&mut self, branches: Vec<String>) {
        self.available_branches = branches;
    }

    /// Open the create worktree modal.
    pub fn open_create_modal(&mut self) {
        self.create_modal_open = true;
        self.branch_input.clear();
        self.base_branch_input = String::from("main");
        self.use_existing_branch = false;
        self.focused_input = Some(0); // Focus branch input by default
        self.branch_dropdown_open = false;
    }

    /// Close the create worktree modal.
    pub fn close_create_modal(&mut self) {
        self.create_modal_open = false;
    }

    /// Check if the create modal is open.
    pub fn is_create_modal_open(&self) -> bool {
        self.create_modal_open
    }

    /// Get the branch input value.
    pub fn branch_input(&self) -> &str {
        &self.branch_input
    }

    /// Set the branch input value.
    pub fn set_branch_input(&mut self, value: String) {
        self.branch_input = value;
    }

    /// Get the base branch input value.
    pub fn base_branch_input(&self) -> &str {
        &self.base_branch_input
    }

    /// Set the base branch input value.
    pub fn set_base_branch_input(&mut self, value: String) {
        self.base_branch_input = value;
    }

    /// Toggle use existing branch.
    pub fn toggle_use_existing_branch(&mut self) {
        self.use_existing_branch = !self.use_existing_branch;
        self.branch_dropdown_open = false;
        if self.use_existing_branch {
            self.focused_input = None;
        } else {
            self.focused_input = Some(0);
        }
    }

    /// Check if using existing branch.
    pub fn use_existing_branch(&self) -> bool {
        self.use_existing_branch
    }

    /// Toggle the existing-branch dropdown.
    pub fn toggle_branch_dropdown(&mut self) {
        self.branch_dropdown_open = !self.branch_dropdown_open;
    }

    /// Close the existing-branch dropdown.
    pub fn close_branch_dropdown(&mut self) {
        self.branch_dropdown_open = false;
    }

    /// Check if the existing-branch dropdown is open.
    pub fn is_branch_dropdown_open(&self) -> bool {
        self.branch_dropdown_open
    }

    /// Select an existing branch from the dropdown.
    pub fn select_existing_branch(&mut self, branch: String) {
        self.branch_input = branch;
        self.branch_dropdown_open = false;
    }

    /// Get available branches.
    pub fn available_branches(&self) -> &[String] {
        &self.available_branches
    }

    /// Create worktree options from current input.
    pub fn create_options(&self) -> Option<WorktreeCreateOptions> {
        if self.branch_input.is_empty() {
            return None;
        }

        let base_branch = if self.use_existing_branch {
            None
        } else {
            Some(self.base_branch_input.clone())
        };

        Some(WorktreeCreateOptions {
            branch: self.branch_input.clone(),
            base_branch,
            path: None, // Use default path
        })
    }

    /// Set focus to a specific input field.
    pub fn set_focus(&mut self, field: usize) {
        self.focused_input = Some(field);
    }

    /// Get the currently focused input field.
    pub fn focused_input(&self) -> Option<usize> {
        self.focused_input
    }

    /// Handle a character input.
    pub fn handle_char_input(&mut self, c: char) {
        match self.focused_input {
            Some(0) => {
                // Branch name input
                self.branch_input.push(c);
            }
            Some(1) => {
                // Base branch input
                self.base_branch_input.push(c);
            }
            _ => {}
        }
    }

    /// Handle backspace.
    pub fn handle_backspace(&mut self) {
        match self.focused_input {
            Some(0) => {
                self.branch_input.pop();
            }
            Some(1) => {
                self.base_branch_input.pop();
            }
            _ => {}
        }
    }

    /// Clear the focused input field.
    pub fn clear_focused_input(&mut self) {
        match self.focused_input {
            Some(0) => {
                self.branch_input.clear();
            }
            Some(1) => {
                self.base_branch_input.clear();
            }
            _ => {}
        }
    }

    /// Generate rendering hints for GPUI.
    pub fn render_hints(&self) -> WorktreeRenderHints {
        WorktreeRenderHints {
            worktrees: self.worktrees.clone(),
            create_modal_open: self.create_modal_open,
            branch_input: self.branch_input.clone(),
            base_branch_input: self.base_branch_input.clone(),
            use_existing_branch: self.use_existing_branch,
            available_branches: self.available_branches.clone(),
            height: self.height,
            header_height: Self::HEADER_HEIGHT,
            item_height: Self::ITEM_HEIGHT,
            focused_input: self.focused_input,
            branch_dropdown_open: self.branch_dropdown_open,
        }
    }
}

/// Rendering hints for the worktree panel.
#[derive(Debug, Clone)]
pub struct WorktreeRenderHints {
    /// List of worktrees.
    pub worktrees: Vec<Worktree>,
    /// Whether the create modal is open.
    pub create_modal_open: bool,
    /// Branch name input.
    pub branch_input: String,
    /// Base branch input.
    pub base_branch_input: String,
    /// Whether to use existing branch.
    pub use_existing_branch: bool,
    /// Available branches.
    pub available_branches: Vec<String>,
    /// Panel height.
    pub height: f32,
    /// Header height.
    pub header_height: f32,
    /// Item height.
    pub item_height: f32,
    /// Which input field has focus.
    pub focused_input: Option<usize>,
    /// Whether the existing-branch dropdown is open.
    pub branch_dropdown_open: bool,
}

/// Worktree item for rendering.
#[derive(Debug, Clone)]
pub struct WorktreeItem {
    /// Worktree path.
    pub path: PathBuf,
    /// Branch name.
    pub branch: String,
    /// Short commit SHA.
    pub head_sha: Option<String>,
    /// Whether this is the main worktree.
    pub is_main: bool,
    /// Bound session ID.
    pub bound_session: Option<SessionId>,
    /// Whether this item is hovered.
    pub is_hovered: bool,
}

impl From<&Worktree> for WorktreeItem {
    fn from(wt: &Worktree) -> Self {
        Self {
            path: wt.path.clone(),
            branch: wt.branch.clone(),
            head_sha: wt.head_sha.clone(),
            is_main: wt.is_main,
            bound_session: wt.bound_session,
            is_hovered: false,
        }
    }
}
