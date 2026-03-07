//! Project and file tree state for WorkspaceView.

use codirigent_core::Worktree;
use codirigent_filetree::{quote_path_for_terminal, TerminalPathStyle};
use codirigent_session::WorktreeManager;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::sidebar::{FileTreePanel, WorktreePanel};
use codirigent_filetree::FileTree;

/// Cached project-root state used for stale-while-revalidate switching between repos.
pub(super) struct CachedProjectRootState {
    /// In-memory file tree model for the root.
    pub(super) file_tree_model: Option<FileTree>,
    /// Shared worktree manager for git worktree operations.
    pub(super) worktree_manager: Option<Arc<Mutex<WorktreeManager>>>,
    /// Cached worktree list for immediate rendering.
    pub(super) worktrees: Vec<Worktree>,
    /// Cached branch list for immediate rendering.
    pub(super) available_branches: Vec<String>,
}

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
    /// Cached file-tree/worktree state by project root.
    pub(super) root_cache: HashMap<PathBuf, CachedProjectRootState>,
}

impl ProjectState {
    /// Format a filesystem path for insertion into a terminal command line.
    ///
    /// When a file tree model is available, delegates to its path formatting
    /// (e.g. relative paths, shell escaping). Falls back to the raw path string.
    pub(super) fn format_path_for_terminal(
        &self,
        path: &Path,
        style: TerminalPathStyle,
    ) -> Option<String> {
        if let Some(tree) = &self.file_tree_model {
            tree.path_for_terminal(path, style)
        } else {
            quote_path_for_terminal(path, style)
        }
    }

    /// Returns true when the path resolves within the current project root.
    ///
    /// Symlinks that escape the project root are treated as unsafe.
    pub(super) fn is_safe_project_path(&self, path: &Path) -> bool {
        let Some(root) = &self.project_root else {
            return true;
        };

        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            root.join(path)
        };
        if !absolute.starts_with(root) {
            return false;
        }

        let root_canonical = root.canonicalize().unwrap_or_else(|_| root.clone());
        match absolute.canonicalize() {
            Ok(canonical) => canonical.starts_with(&root_canonical),
            Err(_) => match fs::symlink_metadata(&absolute) {
                Ok(metadata) if metadata.file_type().is_symlink() => false,
                _ => false,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_project_state(root: PathBuf) -> ProjectState {
        ProjectState {
            file_tree: FileTreePanel::default(),
            file_tree_model: None,
            project_root: Some(root),
            worktree_panel: WorktreePanel::new(),
            worktree_manager: None,
            root_cache: HashMap::new(),
        }
    }

    #[cfg(unix)]
    #[test]
    fn external_symlink_path_is_not_safe() {
        use std::os::unix::fs::symlink;

        let project = TempDir::new().unwrap();
        let external = TempDir::new().unwrap();
        let secret = external.path().join("secret.txt");
        let link = project.path().join("secret-link.txt");
        fs::write(&secret, "secret").unwrap();
        symlink(&secret, &link).unwrap();

        let state = create_project_state(project.path().to_path_buf());
        assert!(!state.is_safe_project_path(&link));
    }

    #[test]
    fn project_file_path_is_safe() {
        let project = TempDir::new().unwrap();
        let file = project.path().join("safe.txt");
        fs::write(&file, "safe").unwrap();

        let state = create_project_state(project.path().to_path_buf());
        assert!(state.is_safe_project_path(&file));
    }
}
