//! Git worktree management.
//!
//! Provides functionality for managing git worktrees to enable
//! parallel development across multiple AI sessions. Each session
//! can be bound to its own worktree, allowing work on different
//! branches without conflicts.
//!
//! # Overview
//!
//! Git worktrees allow multiple working directories to be associated
//! with a single repository. This module provides:
//!
//! - Listing existing worktrees
//! - Creating new worktrees from branches
//! - Removing worktrees
//! - Binding sessions to worktrees
//! - Cleaning up merged worktrees
//!
//! # Example
//!
//! ```no_run
//! use dirigent_session::WorktreeManager;
//! use std::path::Path;
//!
//! let manager = WorktreeManager::new(Path::new("/path/to/repo")).unwrap();
//!
//! // List existing worktrees
//! for wt in manager.list() {
//!     println!("{}: {}", wt.branch, wt.path.display());
//! }
//! ```

use anyhow::{Context, Result};
use dirigent_core::Worktree;
use git2::Repository;
use std::path::{Path, PathBuf};
use tracing::debug;

/// Git worktree manager.
///
/// Manages git worktrees for isolated parallel development across sessions.
/// This allows multiple AI sessions to work on different branches simultaneously
/// without conflicts.
///
/// # Example
///
/// ```no_run
/// use dirigent_session::WorktreeManager;
/// use std::path::Path;
///
/// let manager = WorktreeManager::new(Path::new("/path/to/repo")).unwrap();
/// println!("Found {} worktrees", manager.list().len());
/// ```
#[derive(Debug)]
pub struct WorktreeManager {
    /// Path to the main repository.
    repo_path: PathBuf,
    /// Cached worktree list.
    worktrees: Vec<Worktree>,
}

impl WorktreeManager {
    /// Create a new worktree manager for the given repository.
    ///
    /// This will verify that the path is a valid git repository and
    /// refresh the list of worktrees.
    ///
    /// # Arguments
    ///
    /// * `repo_path` - Path to the git repository
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The path cannot be canonicalized
    /// - The path is not a valid git repository
    ///
    /// # Example
    ///
    /// ```no_run
    /// use dirigent_session::WorktreeManager;
    /// use std::path::Path;
    ///
    /// let manager = WorktreeManager::new(Path::new("/path/to/repo")).unwrap();
    /// ```
    pub fn new(repo_path: &Path) -> Result<Self> {
        let repo_path = repo_path
            .canonicalize()
            .context("Failed to canonicalize repo path")?;

        // Verify it's a git repository
        Repository::open(&repo_path).context("Not a git repository")?;

        let mut manager = Self {
            repo_path,
            worktrees: Vec::new(),
        };
        manager.refresh()?;
        Ok(manager)
    }

    /// Get the repository path.
    ///
    /// Returns the canonical path to the main repository.
    pub fn repo_path(&self) -> &Path {
        &self.repo_path
    }

    /// List all worktrees.
    ///
    /// Returns a slice of all known worktrees, including the main worktree.
    /// Call `refresh()` to update this list from git.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use dirigent_session::WorktreeManager;
    /// use std::path::Path;
    ///
    /// let manager = WorktreeManager::new(Path::new("/path/to/repo")).unwrap();
    /// for wt in manager.list() {
    ///     println!("{}: {}", wt.branch, wt.path.display());
    /// }
    /// ```
    pub fn list(&self) -> &[Worktree] {
        &self.worktrees
    }

    /// Refresh the worktree list from git.
    ///
    /// This method reads the current worktree state from the git repository
    /// and updates the internal list. It includes the main worktree and all
    /// linked worktrees.
    ///
    /// # Errors
    ///
    /// Returns an error if the repository cannot be opened or the worktree
    /// information cannot be read.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use dirigent_session::WorktreeManager;
    /// use std::path::Path;
    ///
    /// let mut manager = WorktreeManager::new(Path::new("/path/to/repo")).unwrap();
    /// manager.refresh().unwrap();
    /// ```
    pub fn refresh(&mut self) -> Result<()> {
        let repo = Repository::open(&self.repo_path)?;
        let mut worktrees = Vec::new();

        // Add main worktree
        let head = repo.head().ok();
        let main_branch = head
            .as_ref()
            .and_then(|h| h.shorthand())
            .unwrap_or("HEAD")
            .to_string();
        let head_sha = head
            .and_then(|h| h.target())
            .map(|oid| oid.to_string()[..8].to_string());

        let mut main_wt = Worktree::new(self.repo_path.clone(), main_branch, true);
        if let Some(sha) = head_sha {
            main_wt = main_wt.with_head_sha(sha);
        }
        worktrees.push(main_wt);

        // List linked worktrees
        if let Ok(wt_names) = repo.worktrees() {
            for name in wt_names.iter().flatten() {
                if let Ok(wt) = repo.find_worktree(name) {
                    let path = wt.path().to_path_buf();
                    // Try to get branch info from the worktree
                    if let Ok(wt_repo) = Repository::open(&path) {
                        let branch = wt_repo
                            .head()
                            .ok()
                            .and_then(|h| h.shorthand().map(String::from))
                            .unwrap_or_else(|| name.to_string());
                        let sha = wt_repo
                            .head()
                            .ok()
                            .and_then(|h| h.target())
                            .map(|oid| oid.to_string()[..8].to_string());

                        let mut linked_wt = Worktree::new(path, branch, false);
                        if let Some(sha) = sha {
                            linked_wt = linked_wt.with_head_sha(sha);
                        }
                        worktrees.push(linked_wt);
                    }
                }
            }
        }

        self.worktrees = worktrees;
        debug!(count = self.worktrees.len(), "Refreshed worktree list");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::Repository;
    use tempfile::TempDir;

    /// Create a test repository with an initial commit.
    fn setup_test_repo() -> (TempDir, PathBuf) {
        let temp = TempDir::new().unwrap();
        let repo_path = temp.path().to_path_buf();
        let repo = Repository::init(&repo_path).unwrap();

        // Create an initial commit (required for worktree operations)
        let sig = repo.signature().unwrap();
        let tree_id = repo.index().unwrap().write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])
            .unwrap();

        (temp, repo_path)
    }

    /// Create a bare test repository (no initial commit).
    fn setup_bare_test_repo() -> (TempDir, PathBuf) {
        let temp = TempDir::new().unwrap();
        let repo_path = temp.path().to_path_buf();
        Repository::init(&repo_path).unwrap();
        (temp, repo_path)
    }

    #[test]
    fn test_worktree_manager_new() {
        let (_temp, path) = setup_test_repo();
        let manager = WorktreeManager::new(&path);
        assert!(manager.is_ok());
    }

    #[test]
    fn test_worktree_manager_not_a_repo() {
        let temp = TempDir::new().unwrap();
        let result = WorktreeManager::new(temp.path());
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Not a git repository"));
    }

    #[test]
    fn test_worktree_manager_invalid_path() {
        let result = WorktreeManager::new(Path::new("/nonexistent/path/to/repo"));
        assert!(result.is_err());
    }

    #[test]
    fn test_worktree_manager_repo_path() {
        let (_temp, path) = setup_test_repo();
        let manager = WorktreeManager::new(&path).unwrap();
        // On macOS, paths may be canonicalized through /private, so we compare
        // canonical forms
        let expected = path.canonicalize().unwrap();
        assert_eq!(manager.repo_path(), expected);
    }

    #[test]
    fn test_worktree_manager_repo_path_is_canonical() {
        let (_temp, path) = setup_test_repo();
        // Add a .. segment to the path to test canonicalization
        let non_canonical = path.join("subdir").join("..");
        std::fs::create_dir(path.join("subdir")).unwrap();
        let manager = WorktreeManager::new(&non_canonical).unwrap();
        // The manager should store the canonical path (no .. segments)
        assert!(!manager.repo_path().to_string_lossy().contains(".."));
    }

    #[test]
    fn test_worktree_manager_list_initial() {
        let (_temp, path) = setup_test_repo();
        let manager = WorktreeManager::new(&path).unwrap();
        // Should have at least the main worktree
        assert!(!manager.list().is_empty());
        assert!(manager.list()[0].is_main);
    }

    #[test]
    fn test_worktree_manager_list_returns_main_worktree() {
        let (_temp, path) = setup_test_repo();
        let manager = WorktreeManager::new(&path).unwrap();

        let main_wt = manager.list().iter().find(|wt| wt.is_main);
        assert!(main_wt.is_some());

        let main_wt = main_wt.unwrap();
        // On macOS, paths may be canonicalized through /private
        let expected = path.canonicalize().unwrap();
        assert_eq!(main_wt.path, expected);
    }

    #[test]
    fn test_worktree_manager_refresh() {
        let (_temp, path) = setup_test_repo();
        let mut manager = WorktreeManager::new(&path).unwrap();

        // Refresh should succeed
        assert!(manager.refresh().is_ok());

        // Should still have the main worktree
        assert!(!manager.list().is_empty());
    }

    #[test]
    fn test_worktree_manager_refresh_updates_head() {
        let (_temp, path) = setup_test_repo();
        let mut manager = WorktreeManager::new(&path).unwrap();

        // Main worktree should have a head SHA
        let main_wt = manager.list().iter().find(|wt| wt.is_main).unwrap();
        assert!(main_wt.head_sha.is_some());

        // Refresh and check again
        manager.refresh().unwrap();
        let main_wt = manager.list().iter().find(|wt| wt.is_main).unwrap();
        assert!(main_wt.head_sha.is_some());
    }

    #[test]
    fn test_worktree_manager_list_empty_slice() {
        let (_temp, path) = setup_test_repo();
        let manager = WorktreeManager::new(&path).unwrap();

        // list() should return a slice that can be iterated
        let count = manager.list().iter().count();
        assert!(count >= 1);
    }

    #[test]
    fn test_worktree_manager_refresh_bare_repo() {
        let (_temp, path) = setup_bare_test_repo();
        let mut manager = WorktreeManager::new(&path).unwrap();

        // Refresh should work even on a repo without commits
        // (though branch name will be HEAD)
        assert!(manager.refresh().is_ok());
    }

    #[test]
    fn test_worktree_manager_main_branch_name() {
        let (_temp, path) = setup_test_repo();
        let manager = WorktreeManager::new(&path).unwrap();

        let main_wt = manager.list().iter().find(|wt| wt.is_main).unwrap();
        // Default branch on new repos is usually 'master' or 'main'
        assert!(
            main_wt.branch == "master" || main_wt.branch == "main",
            "Expected 'master' or 'main', got '{}'",
            main_wt.branch
        );
    }
}
