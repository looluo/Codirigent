//! Task definition and related configuration types.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

use super::ids::{SessionId, TaskId};
use super::status::{TaskPriority, TaskStatus};

/// Verification configuration for a task.
///
/// Defines how to verify task completion (run tests, custom scripts, etc.)
/// When a task has verification configured, it will transition to the
/// `Verifying` status after the AI completes its work.
///
/// # Example
///
/// ```
/// use codirigent_core::VerificationConfig;
/// use std::time::Duration;
///
/// let config = VerificationConfig {
///     command: "cargo test".to_string(),
///     working_dir: None,
///     timeout: Duration::from_secs(300),
///     requires_human_review: true,
///     success_patterns: vec!["test result: ok".to_string()],
///     failure_patterns: vec!["FAILED".to_string()],
/// };
/// assert_eq!(config.timeout, Duration::from_secs(300));
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VerificationConfig {
    /// Command to execute for verification (e.g., "npm test", "cargo test").
    pub command: String,

    /// Working directory for the command. If None, uses task's assigned session's directory.
    pub working_dir: Option<PathBuf>,

    /// Timeout for verification command.
    #[serde(with = "humantime_serde")]
    pub timeout: Duration,

    /// Whether human review is required after verification passes.
    pub requires_human_review: bool,

    /// Custom success patterns to look for in output.
    pub success_patterns: Vec<String>,

    /// Custom failure patterns that indicate test failure.
    pub failure_patterns: Vec<String>,
}

impl Default for VerificationConfig {
    fn default() -> Self {
        Self {
            command: String::new(),
            working_dir: None,
            timeout: Duration::from_secs(300), // 5 minutes
            requires_human_review: true,
            success_patterns: Vec::new(),
            failure_patterns: Vec::new(),
        }
    }
}

/// Retry configuration for a task.
///
/// Controls how many times a task can be retried if it fails,
/// and the delay between retry attempts.
///
/// # Example
///
/// ```
/// use codirigent_core::RetryConfig;
/// use std::time::Duration;
///
/// let mut config = RetryConfig::default();
/// assert_eq!(config.max_retries, 3);
/// assert_eq!(config.retry_count, 0);
///
/// // Simulate retries
/// config.retry_count = 2;
/// assert!(config.retry_count < config.max_retries);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RetryConfig {
    /// Maximum number of retry attempts.
    pub max_retries: u32,

    /// Current retry count.
    pub retry_count: u32,

    /// Delay between retries.
    #[serde(with = "humantime_serde")]
    pub retry_delay: Duration,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            retry_count: 0,
            retry_delay: Duration::from_secs(0),
        }
    }
}

/// Task definition for the task queue.
///
/// A task represents a unit of work that can be assigned to an AI session.
/// Tasks support verification (running tests), retry logic, and dependency
/// tracking for proper ordering.
///
/// # Example
///
/// ```
/// use codirigent_core::{Task, TaskId, VerificationConfig, RetryConfig};
///
/// let mut task = Task::new(
///     TaskId::from("task-001"),
///     "Implement login".to_string(),
///     "Add user authentication".to_string(),
/// );
///
/// // Add verification
/// task.verification = Some(VerificationConfig {
///     command: "cargo test auth".to_string(),
///     ..Default::default()
/// });
///
/// assert!(task.can_retry());
/// assert!(task.dependencies_satisfied(&[]));
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// Unique task identifier.
    pub id: TaskId,
    /// Task title (short description).
    pub title: String,
    /// Detailed description/instructions.
    pub description: String,
    /// Priority level.
    pub priority: TaskPriority,
    /// Current status in workflow.
    pub status: TaskStatus,
    /// Dependencies on other tasks (must complete first).
    pub dependencies: Vec<TaskId>,
    /// Tags for categorization and filtering.
    pub tags: Vec<String>,
    /// Estimated time to complete in minutes.
    pub estimated_minutes: Option<u32>,
    /// Assigned session, if any.
    pub assigned_session: Option<SessionId>,
    /// When assigned to a session.
    pub assigned_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Verification configuration.
    pub verification: Option<VerificationConfig>,
    /// Retry configuration.
    pub retry: RetryConfig,
    /// When the task was created.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// When work started (status changed to Working).
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    /// When the task was completed.
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Error message if task failed.
    pub error_message: Option<String>,

    /// Project root directory for hard session filtering.
    /// When set, only sessions whose working_directory is under this path can be assigned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_dir: Option<PathBuf>,

    /// Plan file path relative to project_dir.
    /// Included in assignment prompt so the CLI LLM can read it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_file: Option<String>,
}

impl Task {
    /// Create a new task with default values.
    ///
    /// # Arguments
    ///
    /// * `id` - Unique task identifier
    /// * `title` - Short task title
    /// * `description` - Detailed instructions
    ///
    /// # Example
    ///
    /// ```
    /// use codirigent_core::{Task, TaskId, TaskStatus};
    ///
    /// let task = Task::new(
    ///     TaskId::from("task-001"),
    ///     "Test Task".to_string(),
    ///     "Do something".to_string(),
    /// );
    /// assert_eq!(task.status, TaskStatus::Queued);
    /// assert!(task.can_retry());
    /// ```
    pub fn new(id: TaskId, title: String, description: String) -> Self {
        Self {
            id,
            title,
            description,
            priority: TaskPriority::default(),
            status: TaskStatus::default(),
            dependencies: Vec::new(),
            tags: Vec::new(),
            estimated_minutes: None,
            assigned_session: None,
            assigned_at: None,
            verification: None,
            retry: RetryConfig::default(),
            created_at: chrono::Utc::now(),
            started_at: None,
            completed_at: None,
            error_message: None,
            project_dir: None,
            plan_file: None,
        }
    }

    /// Check if all dependencies are satisfied.
    ///
    /// Returns true if all tasks in the dependencies list are present
    /// in the completed_tasks list.
    ///
    /// # Arguments
    ///
    /// * `completed_tasks` - List of task IDs that have been completed
    ///
    /// # Example
    ///
    /// ```
    /// use codirigent_core::{Task, TaskId};
    ///
    /// let mut task = Task::new(
    ///     TaskId::from("task-002"),
    ///     "Has deps".to_string(),
    ///     "".to_string(),
    /// );
    /// task.dependencies = vec![TaskId::from("task-001")];
    ///
    /// assert!(!task.dependencies_satisfied(&[]));
    /// assert!(task.dependencies_satisfied(&[TaskId::from("task-001")]));
    /// ```
    pub fn dependencies_satisfied(&self, completed_tasks: &[TaskId]) -> bool {
        self.dependencies
            .iter()
            .all(|dep| completed_tasks.contains(dep))
    }

    /// O(n) version using a pre-built set — use this in hot loops.
    pub(crate) fn dependencies_satisfied_fast(
        &self,
        completed_set: &std::collections::HashSet<&TaskId>,
    ) -> bool {
        self.dependencies
            .iter()
            .all(|dep| completed_set.contains(dep))
    }

    /// Check if the task can be retried.
    ///
    /// Returns true if the current retry count is less than the maximum
    /// allowed retries.
    ///
    /// # Example
    ///
    /// ```
    /// use codirigent_core::{Task, TaskId};
    ///
    /// let mut task = Task::new(
    ///     TaskId::from("task-001"),
    ///     "Retry Task".to_string(),
    ///     "".to_string(),
    /// );
    /// assert!(task.can_retry()); // Default: 0 retries out of 3
    ///
    /// task.retry.retry_count = 3;
    /// assert!(!task.can_retry()); // 3 retries out of 3, no more allowed
    /// ```
    pub fn can_retry(&self) -> bool {
        self.retry.retry_count < self.retry.max_retries
    }

    /// Increment the retry count.
    ///
    /// This should be called when a task fails and is being retried.
    ///
    /// # Example
    ///
    /// ```
    /// use codirigent_core::{Task, TaskId};
    ///
    /// let mut task = Task::new(
    ///     TaskId::from("task-001"),
    ///     "Retry Task".to_string(),
    ///     "".to_string(),
    /// );
    /// assert_eq!(task.retry.retry_count, 0);
    ///
    /// task.increment_retry();
    /// assert_eq!(task.retry.retry_count, 1);
    /// ```
    pub fn increment_retry(&mut self) {
        self.retry.retry_count += 1;
    }
}
