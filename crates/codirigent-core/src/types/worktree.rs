//! Git worktree types for parallel development.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::ids::SessionId;

/// Git worktree information.
///
/// Represents a git worktree, which allows multiple working directories
/// to be associated with a single repository. This enables parallel
/// development across multiple AI sessions without branch conflicts.
///
/// # Example
///
/// ```
/// use codirigent_core::Worktree;
/// use std::path::PathBuf;
///
/// let worktree = Worktree::new(
///     PathBuf::from("/repo/worktrees/feature"),
///     "feature-branch".to_string(),
///     false,
/// );
/// assert_eq!(worktree.branch, "feature-branch");
/// assert!(!worktree.is_main);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Worktree {
    /// Absolute path to the worktree directory.
    pub path: PathBuf,
    /// Branch name associated with this worktree.
    pub branch: String,
    /// Head commit SHA (short form, typically 8 characters).
    pub head_sha: Option<String>,
    /// Whether this is the main worktree (the original repository).
    pub is_main: bool,
    /// Session bound to this worktree, if any.
    pub bound_session: Option<SessionId>,
    /// When the worktree was created or first detected.
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl Worktree {
    /// Create a new worktree instance.
    ///
    /// # Arguments
    ///
    /// * `path` - Absolute path to the worktree directory
    /// * `branch` - Branch name associated with this worktree
    /// * `is_main` - Whether this is the main worktree
    ///
    /// # Example
    ///
    /// ```
    /// use codirigent_core::Worktree;
    /// use std::path::PathBuf;
    ///
    /// let wt = Worktree::new(
    ///     PathBuf::from("/repo"),
    ///     "main".to_string(),
    ///     true,
    /// );
    /// assert!(wt.is_main);
    /// ```
    pub fn new(path: PathBuf, branch: String, is_main: bool) -> Self {
        Self {
            path,
            branch,
            head_sha: None,
            is_main,
            bound_session: None,
            created_at: chrono::Utc::now(),
        }
    }

    /// Set the head SHA for this worktree.
    ///
    /// # Arguments
    ///
    /// * `sha` - The commit SHA (typically truncated to 8 characters)
    pub fn with_head_sha(mut self, sha: String) -> Self {
        self.head_sha = Some(sha);
        self
    }
}

/// Options for creating a new worktree.
///
/// Specifies the branch name and optional configuration for
/// creating a new git worktree.
///
/// # Example
///
/// ```
/// use codirigent_core::WorktreeCreateOptions;
///
/// let options = WorktreeCreateOptions::new("feature-branch".to_string())
///     .with_base_branch("main".to_string());
/// assert_eq!(options.branch, "feature-branch");
/// assert_eq!(options.base_branch, Some("main".to_string()));
/// ```
#[derive(Debug, Clone)]
pub struct WorktreeCreateOptions {
    /// Branch name to checkout (creates if doesn't exist).
    pub branch: String,
    /// Base branch to create from (if creating new branch).
    pub base_branch: Option<String>,
    /// Custom path for the worktree (defaults to ./worktrees/<branch>).
    pub path: Option<PathBuf>,
}

impl WorktreeCreateOptions {
    /// Create new worktree options with the given branch name.
    ///
    /// # Arguments
    ///
    /// * `branch` - The branch name to checkout or create
    pub fn new(branch: String) -> Self {
        Self {
            branch,
            base_branch: None,
            path: None,
        }
    }

    /// Set the base branch to create from.
    ///
    /// If the branch doesn't exist, it will be created from this base.
    ///
    /// # Arguments
    ///
    /// * `base` - The base branch name (e.g., "main", "develop")
    pub fn with_base_branch(mut self, base: String) -> Self {
        self.base_branch = Some(base);
        self
    }

    /// Set a custom path for the worktree.
    ///
    /// By default, worktrees are created in ./worktrees/<branch>.
    ///
    /// # Arguments
    ///
    /// * `path` - Custom path for the worktree directory
    pub fn with_path(mut self, path: PathBuf) -> Self {
        self.path = Some(path);
        self
    }
}
