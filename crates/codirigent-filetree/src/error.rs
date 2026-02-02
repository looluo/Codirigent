//! Error types for the file tree module.

use std::path::PathBuf;
use thiserror::Error;

/// Errors that can occur during file tree operations.
#[derive(Error, Debug)]
pub enum FileTreeError {
    /// The specified path does not exist.
    #[error("path does not exist: {0}")]
    PathNotFound(PathBuf),

    /// The specified path is not a directory.
    #[error("path is not a directory: {0}")]
    NotADirectory(PathBuf),

    /// Permission denied when accessing a path.
    #[error("permission denied: {0}")]
    PermissionDenied(PathBuf),

    /// IO error occurred.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Entry not found in tree.
    #[error("entry not found: {0}")]
    EntryNotFound(PathBuf),
}

/// Result type for file tree operations.
pub type Result<T> = std::result::Result<T, FileTreeError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = FileTreeError::PathNotFound(PathBuf::from("/test/path"));
        assert!(err.to_string().contains("/test/path"));
    }

    #[test]
    fn test_not_a_directory_display() {
        let err = FileTreeError::NotADirectory(PathBuf::from("/test/file.txt"));
        assert!(err.to_string().contains("/test/file.txt"));
    }

    #[test]
    fn test_permission_denied_display() {
        let err = FileTreeError::PermissionDenied(PathBuf::from("/root/secret"));
        assert!(err.to_string().contains("/root/secret"));
    }

    #[test]
    fn test_entry_not_found_display() {
        let err = FileTreeError::EntryNotFound(PathBuf::from("/missing/entry"));
        assert!(err.to_string().contains("/missing/entry"));
    }

    #[test]
    fn test_io_error_from() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err: FileTreeError = io_err.into();
        assert!(matches!(err, FileTreeError::Io(_)));
    }
}
