//! Project and file tree state for WorkspaceView.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::sidebar::{FileTreePanel, WorktreePanel};
use codirigent_filetree::FileTree;

/// Groups all project/file-tree state for the workspace.
pub(super) struct ProjectState {
    /// File tree sidebar panel.
    pub(super) file_tree: FileTreePanel,
    /// In-memory file tree model.
    pub(super) file_tree_model: Option<FileTree>,
    /// Root directory of the current project.
    pub(super) project_root: Option<PathBuf>,
    /// Worktree management panel.
    pub(super) worktree_panel: WorktreePanel,
    /// Shared worktree manager for git worktree operations.
    pub(super) worktree_manager: Option<Arc<Mutex<codirigent_session::WorktreeManager>>>,
}

impl ProjectState {
    /// Format a filesystem path for insertion into a terminal command line.
    ///
    /// When a file tree model is available, delegates to its path formatting
    /// (e.g. relative paths, shell escaping). Falls back to the raw path string.
    pub(super) fn format_path_for_terminal(&self, path: &Path) -> String {
        if let Some(tree) = &self.file_tree_model {
            tree.path_for_terminal(path)
        } else {
            path.to_string_lossy().to_string()
        }
    }
}
