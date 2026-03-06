//! Task queue management and scheduling.
//!
//! This module provides the [`TaskQueue`] system for managing task ordering,
//! priority-based scheduling, and dependency tracking. It supports multiple
//! scheduling modes and integrates with the event bus for state change notifications.
//!
//! ## Scheduling Modes
//!
//! - [`SchedulerMode::Fifo`]: First-in, first-out ordering
//! - [`SchedulerMode::Priority`]: Order by priority level (Critical > High > Medium > Low)
//! - [`SchedulerMode::Dependency`]: Consider only dependency ordering
//! - [`SchedulerMode::Smart`]: Combine priority, age, and tag matching (default)
//!
//! ## Example
//!
//! ```
//! use codirigent_core::{
//!     TaskQueue, SchedulerConfig, SchedulerMode,
//!     Task, TaskId, DefaultEventBus,
//! };
//! use codirigent_core::traits::EventBus;
//! use std::sync::Arc;
//!
//! // Create a task queue with default configuration
//! let event_bus = Arc::new(DefaultEventBus::new(16));
//! let mut queue = TaskQueue::new(SchedulerConfig::default(), event_bus);
//!
//! // Add a task
//! let task = Task::new(
//!     TaskId::from("task-001"),
//!     "Implement feature".to_string(),
//!     "Add new feature X".to_string(),
//! );
//! queue.enqueue(task).unwrap();
//!
//! // Get the next task to work on
//! if let Some(next) = queue.next_task() {
//!     println!("Next task: {}", next.title);
//! }
//! ```

pub mod config;
pub mod queue;
pub mod selection;

// Re-export everything for backward compatibility
pub use config::{SchedulerConfig, SchedulerMode};
pub use queue::{TaskQueue, TaskQueueService};

use std::path::Path;

/// Check if a session's working_directory is within a task's project_dir.
///
/// Uses canonicalized prefix matching: the session directory must be equal to
/// or a subdirectory of the project directory.
///
/// # Arguments
///
/// * `session_dir` - The session's working directory
/// * `project_dir` - The task's required project directory
///
/// # Returns
///
/// `true` if the session directory is within the project directory.
pub fn session_matches_project(session_dir: &Path, project_dir: &Path) -> bool {
    let canon_session =
        std::fs::canonicalize(session_dir).unwrap_or_else(|_| session_dir.to_path_buf());
    let canon_project =
        std::fs::canonicalize(project_dir).unwrap_or_else(|_| project_dir.to_path_buf());
    canon_session.starts_with(&canon_project)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_matches_project_helper() {
        // Exact match (non-existent paths, falls back to raw comparison)
        assert!(session_matches_project(
            std::path::Path::new("/project"),
            std::path::Path::new("/project"),
        ));

        // Subdirectory
        assert!(session_matches_project(
            std::path::Path::new("/project/src"),
            std::path::Path::new("/project"),
        ));

        // No match
        assert!(!session_matches_project(
            std::path::Path::new("/project-b"),
            std::path::Path::new("/project-a"),
        ));
    }
}
