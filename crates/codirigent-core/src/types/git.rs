//! Git repository information types.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Git repository information for a session's working directory.
///
/// Contains branch name, dirty file count, staged status, and HEAD SHA.
/// Populated by the git status service when the session's working directory
/// is inside a git repository.
///
/// # Example
///
/// ```
/// use codirigent_core::GitRepoInfo;
/// use std::path::PathBuf;
///
/// let info = GitRepoInfo {
///     repo_root: PathBuf::from("/home/user/project"),
///     branch: "main".to_string(),
///     dirty_count: 3,
///     has_staged: true,
///     head_sha: Some("abc12345".to_string()),
///     unstaged_files: vec![],
///     staged_files: vec![],
/// };
/// assert_eq!(info.branch, "main");
/// assert_eq!(info.dirty_count, 3);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GitRepoInfo {
    /// Absolute path to the git repository root.
    pub repo_root: PathBuf,
    /// Current branch name (or "HEAD detached" if detached).
    pub branch: String,
    /// Number of modified + untracked files.
    pub dirty_count: usize,
    /// Whether there are any staged changes.
    pub has_staged: bool,
    /// Short HEAD SHA (8 characters), if available.
    pub head_sha: Option<String>,
    /// Files with unstaged changes (working tree).
    pub unstaged_files: Vec<GitChangedFile>,
    /// Files with staged changes (index).
    pub staged_files: Vec<GitChangedFile>,
}

/// A file with changes in the git repository.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GitChangedFile {
    /// Relative path from repo root.
    pub path: String,
    /// Type of change.
    pub change: GitChangeKind,
}

/// Kind of change for a git file.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum GitChangeKind {
    /// File content modified.
    Modified,
    /// New file (untracked or added).
    Added,
    /// File deleted.
    Deleted,
    /// File renamed.
    Renamed,
}
