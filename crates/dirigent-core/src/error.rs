//! Error types for the Dirigent application.
//!
//! This module defines the [`DirigentError`] enum which provides
//! structured error handling across all Dirigent crates.

use crate::types::{SessionId, TaskId};
use thiserror::Error;

/// Errors that can occur in the Dirigent application.
///
/// This enum provides structured error handling with context
/// for all operations in the Dirigent application.
///
/// # Example
///
/// ```
/// use dirigent_core::error::DirigentError;
/// use dirigent_core::types::SessionId;
///
/// fn get_session() -> Result<(), DirigentError> {
///     Err(DirigentError::SessionNotFound(SessionId(42)))
/// }
///
/// match get_session() {
///     Err(DirigentError::SessionNotFound(id)) => {
///         println!("Session {} not found", id);
///     }
///     _ => {}
/// }
/// ```
#[derive(Error, Debug)]
pub enum DirigentError {
    /// Session was not found.
    #[error("session not found: {0}")]
    SessionNotFound(SessionId),

    /// Task was not found.
    #[error("task not found: {0}")]
    TaskNotFound(TaskId),

    /// PTY operation failed.
    #[error("PTY error: {0}")]
    Pty(String),

    /// I/O operation failed.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Storage operation failed.
    #[error("storage error: {0}")]
    Storage(String),

    /// Serialization/deserialization failed.
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Invalid configuration.
    #[error("configuration error: {0}")]
    Config(String),

    /// Session already exists.
    #[error("session already exists: {0}")]
    SessionExists(SessionId),

    /// Task already exists.
    #[error("task already exists: {0}")]
    TaskExists(TaskId),

    /// Invalid session state for operation.
    #[error("invalid session state: {0}")]
    InvalidSessionState(String),

    /// Process monitoring error.
    #[error("process monitor error: {0}")]
    ProcessMonitor(String),

    /// Layout error.
    #[error("layout error: {0}")]
    Layout(String),
}

impl DirigentError {
    /// Create a new PTY error with the given message.
    pub fn pty(msg: impl Into<String>) -> Self {
        Self::Pty(msg.into())
    }

    /// Create a new storage error with the given message.
    pub fn storage(msg: impl Into<String>) -> Self {
        Self::Storage(msg.into())
    }

    /// Create a new configuration error with the given message.
    pub fn config(msg: impl Into<String>) -> Self {
        Self::Config(msg.into())
    }

    /// Create a new invalid session state error with the given message.
    pub fn invalid_state(msg: impl Into<String>) -> Self {
        Self::InvalidSessionState(msg.into())
    }

    /// Create a new process monitor error with the given message.
    pub fn process_monitor(msg: impl Into<String>) -> Self {
        Self::ProcessMonitor(msg.into())
    }

    /// Create a new layout error with the given message.
    pub fn layout(msg: impl Into<String>) -> Self {
        Self::Layout(msg.into())
    }

    /// Check if this error is a "not found" error.
    pub fn is_not_found(&self) -> bool {
        matches!(
            self,
            DirigentError::SessionNotFound(_) | DirigentError::TaskNotFound(_)
        )
    }

    /// Check if this error is an I/O error.
    pub fn is_io(&self) -> bool {
        matches!(self, DirigentError::Io(_))
    }
}

/// Result type alias using [`DirigentError`].
pub type Result<T> = std::result::Result<T, DirigentError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_not_found_error() {
        let err = DirigentError::SessionNotFound(SessionId(42));
        let msg = format!("{}", err);
        assert!(msg.contains("session not found"));
        assert!(msg.contains("42"));
    }

    #[test]
    fn test_task_not_found_error() {
        let err = DirigentError::TaskNotFound(TaskId("task-001".to_string()));
        let msg = format!("{}", err);
        assert!(msg.contains("task not found"));
        assert!(msg.contains("task-001"));
    }

    #[test]
    fn test_pty_error() {
        let err = DirigentError::pty("failed to spawn process");
        let msg = format!("{}", err);
        assert!(msg.contains("PTY error"));
        assert!(msg.contains("failed to spawn process"));
    }

    #[test]
    fn test_io_error_from() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err: DirigentError = io_err.into();
        assert!(matches!(err, DirigentError::Io(_)));
        let msg = format!("{}", err);
        assert!(msg.contains("I/O error"));
    }

    #[test]
    fn test_storage_error() {
        let err = DirigentError::storage("failed to write file");
        let msg = format!("{}", err);
        assert!(msg.contains("storage error"));
        assert!(msg.contains("failed to write file"));
    }

    #[test]
    fn test_serialization_error_from() {
        let json_str = "invalid json";
        let json_err: std::result::Result<(), serde_json::Error> =
            serde_json::from_str::<()>(json_str);
        let err: DirigentError = json_err.unwrap_err().into();
        assert!(matches!(err, DirigentError::Serialization(_)));
    }

    #[test]
    fn test_config_error() {
        let err = DirigentError::config("invalid config value");
        let msg = format!("{}", err);
        assert!(msg.contains("configuration error"));
        assert!(msg.contains("invalid config value"));
    }

    #[test]
    fn test_session_exists_error() {
        let err = DirigentError::SessionExists(SessionId(1));
        let msg = format!("{}", err);
        assert!(msg.contains("session already exists"));
    }

    #[test]
    fn test_task_exists_error() {
        let err = DirigentError::TaskExists(TaskId("task-001".to_string()));
        let msg = format!("{}", err);
        assert!(msg.contains("task already exists"));
    }

    #[test]
    fn test_invalid_session_state_error() {
        let err = DirigentError::invalid_state("cannot close session while task is running");
        let msg = format!("{}", err);
        assert!(msg.contains("invalid session state"));
    }

    #[test]
    fn test_process_monitor_error() {
        let err = DirigentError::process_monitor("failed to read process status");
        let msg = format!("{}", err);
        assert!(msg.contains("process monitor error"));
    }

    #[test]
    fn test_layout_error() {
        let err = DirigentError::layout("invalid grid dimensions");
        let msg = format!("{}", err);
        assert!(msg.contains("layout error"));
    }

    #[test]
    fn test_is_not_found() {
        assert!(DirigentError::SessionNotFound(SessionId(1)).is_not_found());
        assert!(DirigentError::TaskNotFound(TaskId("t".to_string())).is_not_found());
        assert!(!DirigentError::storage("err").is_not_found());
    }

    #[test]
    fn test_is_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "not found");
        assert!(DirigentError::Io(io_err).is_io());
        assert!(!DirigentError::storage("err").is_io());
    }

    #[test]
    fn test_error_debug() {
        let err = DirigentError::SessionNotFound(SessionId(42));
        let debug_str = format!("{:?}", err);
        assert!(debug_str.contains("SessionNotFound"));
        assert!(debug_str.contains("42"));
    }

    #[test]
    fn test_result_type_alias() {
        fn example_fn() -> Result<i32> {
            Ok(42)
        }

        fn failing_fn() -> Result<i32> {
            Err(DirigentError::SessionNotFound(SessionId(1)))
        }

        assert_eq!(example_fn().unwrap(), 42);
        assert!(failing_fn().is_err());
    }

    #[test]
    fn test_all_error_variants_debug() {
        let errors: Vec<DirigentError> = vec![
            DirigentError::SessionNotFound(SessionId(1)),
            DirigentError::TaskNotFound(TaskId("t".to_string())),
            DirigentError::Pty("err".to_string()),
            DirigentError::Io(std::io::Error::new(std::io::ErrorKind::Other, "err")),
            DirigentError::Storage("err".to_string()),
            DirigentError::Config("err".to_string()),
            DirigentError::SessionExists(SessionId(1)),
            DirigentError::TaskExists(TaskId("t".to_string())),
            DirigentError::InvalidSessionState("err".to_string()),
            DirigentError::ProcessMonitor("err".to_string()),
            DirigentError::Layout("err".to_string()),
        ];

        for err in errors {
            let _ = format!("{:?}", err);
            let _ = format!("{}", err);
        }
    }
}
