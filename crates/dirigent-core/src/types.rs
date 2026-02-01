//! Core types for the Dirigent application.
//!
//! This module contains all shared types used throughout the application,
//! including identifiers, enums, and core data structures.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

/// Unique identifier for a session.
///
/// Sessions are the core unit of work in Dirigent, each representing
/// a terminal instance running an AI CLI tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub u64);

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "session-{}", self.0)
    }
}

/// Unique identifier for a task.
///
/// Tasks are work items that can be assigned to sessions.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskId(pub String);

impl std::fmt::Display for TaskId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Session status detected by the Input Detector module.
///
/// This represents the current state of a session as determined
/// by process monitoring and output pattern detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SessionStatus {
    /// No active process, shell is idle.
    #[default]
    Idle,
    /// Process is actively running (CPU activity detected).
    Working,
    /// Process is waiting for user input.
    WaitingForInput,
    /// Task completed successfully.
    Done,
    /// Error detected in output.
    Error,
}

/// Task priority levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TaskPriority {
    /// Critical priority - must be done first.
    Critical,
    /// High priority.
    High,
    /// Medium priority (default).
    #[default]
    Medium,
    /// Low priority.
    Low,
}

/// Task status in the workflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TaskStatus {
    /// Waiting in queue.
    #[default]
    Queued,
    /// Assigned to a session.
    Assigned,
    /// Currently being worked on.
    Working,
    /// Awaiting verification.
    Verifying,
    /// Ready for human review.
    Review,
    /// Completed successfully.
    Done,
    /// Blocked by dependency or error.
    Blocked,
}

/// Verification configuration for a task.
///
/// Defines how to verify task completion (run tests, custom scripts, etc.)
/// When a task has verification configured, it will transition to the
/// `Verifying` status after the AI completes its work.
///
/// # Example
///
/// ```
/// use dirigent_core::VerificationConfig;
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
/// use dirigent_core::RetryConfig;
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

/// Grid position for custom layouts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GridPosition {
    /// Row index (0-based).
    pub row: u32,
    /// Column index (0-based).
    pub col: u32,
}

/// Layout mode for the workspace grid.
///
/// Supports standard grid configurations, single-pane mode,
/// and custom layouts with explicit positioning.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LayoutMode {
    /// Standard grid layout with specified rows and columns.
    /// Common configurations: 2x2, 1x4, 2x3, 3x3.
    Grid {
        /// Number of rows.
        rows: u32,
        /// Number of columns.
        cols: u32,
    },
    /// Single session takes full window.
    Single,
    /// Custom layout with explicit session positions.
    Custom {
        /// Session positions.
        positions: Vec<(SessionId, GridPosition)>,
    },
}

impl Default for LayoutMode {
    fn default() -> Self {
        LayoutMode::Grid { rows: 2, cols: 2 }
    }
}

/// Session metadata and state.
///
/// This is the persistent representation of a session,
/// stored in state.json and used throughout the application.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Unique session identifier.
    pub id: SessionId,
    /// Human-readable session name.
    pub name: String,
    /// Current session status.
    pub status: SessionStatus,
    /// Working directory for this session.
    pub working_directory: PathBuf,
    /// Currently assigned task, if any.
    pub current_task: Option<TaskId>,
    /// Context window usage (0.0 - 1.0), if available.
    pub context_usage: Option<f32>,
    /// When the session was created.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Session group name for visual grouping.
    pub group: Option<String>,
    /// Group color for visual identification.
    pub color: Option<String>,
}

impl Session {
    /// Create a new session with default values.
    pub fn new(id: SessionId, name: String, working_directory: PathBuf) -> Self {
        Self {
            id,
            name,
            status: SessionStatus::default(),
            working_directory,
            current_task: None,
            context_usage: None,
            created_at: chrono::Utc::now(),
            group: None,
            color: None,
        }
    }
}

/// Task definition for the task queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// Unique task identifier.
    pub id: TaskId,
    /// Task title.
    pub title: String,
    /// Detailed description.
    pub description: String,
    /// Priority level.
    pub priority: TaskPriority,
    /// Current status.
    pub status: TaskStatus,
    /// Dependencies on other tasks.
    pub dependencies: Vec<TaskId>,
    /// Tags for categorization.
    pub tags: Vec<String>,
    /// Assigned session, if any.
    pub assigned_session: Option<SessionId>,
    /// When assigned.
    pub assigned_at: Option<chrono::DateTime<chrono::Utc>>,
    /// When created.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// When started.
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    /// When completed.
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl Task {
    /// Create a new task with default values.
    pub fn new(id: TaskId, title: String, description: String) -> Self {
        Self {
            id,
            title,
            description,
            priority: TaskPriority::default(),
            status: TaskStatus::default(),
            dependencies: Vec::new(),
            tags: Vec::new(),
            assigned_session: None,
            assigned_at: None,
            created_at: chrono::Utc::now(),
            started_at: None,
            completed_at: None,
        }
    }
}

/// Application state persisted to disk.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppState {
    /// All active sessions.
    pub sessions: Vec<Session>,
    /// Current layout mode.
    pub layout: LayoutMode,
    /// Last updated timestamp.
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Git worktree information.
///
/// Represents a git worktree, which allows multiple working directories
/// to be associated with a single repository. This enables parallel
/// development across multiple AI sessions without branch conflicts.
///
/// # Example
///
/// ```
/// use dirigent_core::Worktree;
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
    /// use dirigent_core::Worktree;
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
/// use dirigent_core::WorktreeCreateOptions;
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

#[cfg(test)]
mod tests {
    use super::*;

    // SessionId tests
    #[test]
    fn test_session_id_display() {
        let id = SessionId(42);
        assert_eq!(format!("{}", id), "session-42");
    }

    #[test]
    fn test_session_id_equality() {
        assert_eq!(SessionId(1), SessionId(1));
        assert_ne!(SessionId(1), SessionId(2));
    }

    #[test]
    fn test_session_id_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(SessionId(1));
        assert!(set.contains(&SessionId(1)));
        assert!(!set.contains(&SessionId(2)));
    }

    #[test]
    fn test_session_id_serialization() {
        let id = SessionId(42);
        let json = serde_json::to_string(&id).unwrap();
        let parsed: SessionId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn test_session_id_clone() {
        let id = SessionId(42);
        let cloned = id;
        assert_eq!(id, cloned);
    }

    // TaskId tests
    #[test]
    fn test_task_id_display() {
        let id = TaskId("task-001".to_string());
        assert_eq!(format!("{}", id), "task-001");
    }

    #[test]
    fn test_task_id_equality() {
        let id1 = TaskId("task-001".to_string());
        let id2 = TaskId("task-001".to_string());
        let id3 = TaskId("task-002".to_string());
        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
    }

    #[test]
    fn test_task_id_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(TaskId("task-001".to_string()));
        assert!(set.contains(&TaskId("task-001".to_string())));
        assert!(!set.contains(&TaskId("task-002".to_string())));
    }

    #[test]
    fn test_task_id_serialization() {
        let id = TaskId("task-001".to_string());
        let json = serde_json::to_string(&id).unwrap();
        let parsed: TaskId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, parsed);
    }

    // SessionStatus tests
    #[test]
    fn test_session_status_default() {
        assert_eq!(SessionStatus::default(), SessionStatus::Idle);
    }

    #[test]
    fn test_session_status_serialization() {
        let status = SessionStatus::WaitingForInput;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"WaitingForInput\"");

        let parsed: SessionStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(status, parsed);
    }

    #[test]
    fn test_session_status_all_variants() {
        let variants = [
            SessionStatus::Idle,
            SessionStatus::Working,
            SessionStatus::WaitingForInput,
            SessionStatus::Done,
            SessionStatus::Error,
        ];
        for variant in variants {
            let json = serde_json::to_string(&variant).unwrap();
            let parsed: SessionStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(variant, parsed);
        }
    }

    // TaskPriority tests
    #[test]
    fn test_task_priority_default() {
        assert_eq!(TaskPriority::default(), TaskPriority::Medium);
    }

    #[test]
    fn test_task_priority_serialization() {
        let priority = TaskPriority::Critical;
        let json = serde_json::to_string(&priority).unwrap();
        assert_eq!(json, "\"Critical\"");

        let parsed: TaskPriority = serde_json::from_str(&json).unwrap();
        assert_eq!(priority, parsed);
    }

    #[test]
    fn test_task_priority_all_variants() {
        let variants = [
            TaskPriority::Critical,
            TaskPriority::High,
            TaskPriority::Medium,
            TaskPriority::Low,
        ];
        for variant in variants {
            let json = serde_json::to_string(&variant).unwrap();
            let parsed: TaskPriority = serde_json::from_str(&json).unwrap();
            assert_eq!(variant, parsed);
        }
    }

    // TaskStatus tests
    #[test]
    fn test_task_status_default() {
        assert_eq!(TaskStatus::default(), TaskStatus::Queued);
    }

    #[test]
    fn test_task_status_serialization() {
        let status = TaskStatus::Verifying;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"Verifying\"");

        let parsed: TaskStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(status, parsed);
    }

    #[test]
    fn test_task_status_all_variants() {
        let variants = [
            TaskStatus::Queued,
            TaskStatus::Assigned,
            TaskStatus::Working,
            TaskStatus::Verifying,
            TaskStatus::Review,
            TaskStatus::Done,
            TaskStatus::Blocked,
        ];
        for variant in variants {
            let json = serde_json::to_string(&variant).unwrap();
            let parsed: TaskStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(variant, parsed);
        }
    }

    // VerificationConfig tests
    #[test]
    fn test_verification_config_default() {
        let config = VerificationConfig::default();
        assert!(config.command.is_empty());
        assert!(config.requires_human_review);
        assert_eq!(config.timeout, Duration::from_secs(300));
        assert!(config.working_dir.is_none());
        assert!(config.success_patterns.is_empty());
        assert!(config.failure_patterns.is_empty());
    }

    #[test]
    fn test_verification_config_serialization() {
        let config = VerificationConfig {
            command: "npm test".to_string(),
            working_dir: None,
            timeout: Duration::from_secs(60),
            requires_human_review: false,
            success_patterns: vec!["PASS".to_string()],
            failure_patterns: vec!["FAIL".to_string()],
        };
        let json = serde_json::to_string(&config).unwrap();
        let parsed: VerificationConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.command, "npm test");
        assert_eq!(parsed.timeout, Duration::from_secs(60));
        assert!(!parsed.requires_human_review);
        assert_eq!(parsed.success_patterns, vec!["PASS".to_string()]);
        assert_eq!(parsed.failure_patterns, vec!["FAIL".to_string()]);
    }

    #[test]
    fn test_verification_config_with_working_dir() {
        let config = VerificationConfig {
            command: "cargo test".to_string(),
            working_dir: Some(PathBuf::from("/project")),
            timeout: Duration::from_secs(120),
            requires_human_review: true,
            success_patterns: Vec::new(),
            failure_patterns: Vec::new(),
        };
        let json = serde_json::to_string(&config).unwrap();
        let parsed: VerificationConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.working_dir, Some(PathBuf::from("/project")));
    }

    #[test]
    fn test_verification_config_humantime_serialization() {
        let config = VerificationConfig {
            command: "test".to_string(),
            working_dir: None,
            timeout: Duration::from_secs(300),
            requires_human_review: true,
            success_patterns: Vec::new(),
            failure_patterns: Vec::new(),
        };
        let json = serde_json::to_string(&config).unwrap();
        // humantime-serde serializes Duration as human-readable strings like "5m"
        assert!(json.contains("5m") || json.contains("300"));
    }

    // RetryConfig tests
    #[test]
    fn test_retry_config_default() {
        let config = RetryConfig::default();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.retry_count, 0);
        assert_eq!(config.retry_delay, Duration::from_secs(0));
    }

    #[test]
    fn test_retry_config_serialization() {
        let config = RetryConfig {
            max_retries: 5,
            retry_count: 2,
            retry_delay: Duration::from_secs(30),
        };
        let json = serde_json::to_string(&config).unwrap();
        let parsed: RetryConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.max_retries, 5);
        assert_eq!(parsed.retry_count, 2);
        assert_eq!(parsed.retry_delay, Duration::from_secs(30));
    }

    #[test]
    fn test_retry_config_equality() {
        let config1 = RetryConfig::default();
        let config2 = RetryConfig::default();
        let config3 = RetryConfig {
            max_retries: 5,
            ..Default::default()
        };
        assert_eq!(config1, config2);
        assert_ne!(config1, config3);
    }

    // GridPosition tests
    #[test]
    fn test_grid_position_creation() {
        let pos = GridPosition { row: 1, col: 2 };
        assert_eq!(pos.row, 1);
        assert_eq!(pos.col, 2);
    }

    #[test]
    fn test_grid_position_equality() {
        let pos1 = GridPosition { row: 0, col: 0 };
        let pos2 = GridPosition { row: 0, col: 0 };
        let pos3 = GridPosition { row: 1, col: 0 };
        assert_eq!(pos1, pos2);
        assert_ne!(pos1, pos3);
    }

    #[test]
    fn test_grid_position_serialization() {
        let pos = GridPosition { row: 1, col: 2 };
        let json = serde_json::to_string(&pos).unwrap();
        let parsed: GridPosition = serde_json::from_str(&json).unwrap();
        assert_eq!(pos, parsed);
    }

    // LayoutMode tests
    #[test]
    fn test_layout_mode_default() {
        let layout = LayoutMode::default();
        assert!(matches!(layout, LayoutMode::Grid { rows: 2, cols: 2 }));
    }

    #[test]
    fn test_layout_mode_grid_serialization() {
        let layout = LayoutMode::Grid { rows: 3, cols: 3 };
        let json = serde_json::to_string(&layout).unwrap();
        let parsed: LayoutMode = serde_json::from_str(&json).unwrap();
        assert_eq!(layout, parsed);
    }

    #[test]
    fn test_layout_mode_single() {
        let layout = LayoutMode::Single;
        let json = serde_json::to_string(&layout).unwrap();
        let parsed: LayoutMode = serde_json::from_str(&json).unwrap();
        assert_eq!(layout, parsed);
    }

    #[test]
    fn test_layout_mode_custom() {
        let positions = vec![
            (SessionId(1), GridPosition { row: 0, col: 0 }),
            (SessionId(2), GridPosition { row: 0, col: 1 }),
        ];
        let layout = LayoutMode::Custom {
            positions: positions.clone(),
        };
        let json = serde_json::to_string(&layout).unwrap();
        let parsed: LayoutMode = serde_json::from_str(&json).unwrap();
        assert_eq!(layout, parsed);
    }

    // Session tests
    #[test]
    fn test_session_new() {
        let session = Session::new(
            SessionId(1),
            "Test Session".to_string(),
            PathBuf::from("/tmp"),
        );
        assert_eq!(session.id, SessionId(1));
        assert_eq!(session.name, "Test Session");
        assert_eq!(session.status, SessionStatus::Idle);
        assert_eq!(session.working_directory, PathBuf::from("/tmp"));
        assert!(session.current_task.is_none());
        assert!(session.context_usage.is_none());
        assert!(session.group.is_none());
        assert!(session.color.is_none());
    }

    #[test]
    fn test_session_serialization() {
        let session = Session::new(SessionId(1), "Test".to_string(), PathBuf::from("/tmp"));
        let json = serde_json::to_string_pretty(&session).unwrap();
        let parsed: Session = serde_json::from_str(&json).unwrap();
        assert_eq!(session.id, parsed.id);
        assert_eq!(session.name, parsed.name);
        assert_eq!(session.status, parsed.status);
    }

    #[test]
    fn test_session_with_all_fields() {
        let mut session = Session::new(
            SessionId(1),
            "Full Session".to_string(),
            PathBuf::from("/home/user"),
        );
        session.status = SessionStatus::Working;
        session.current_task = Some(TaskId("task-001".to_string()));
        session.context_usage = Some(0.75);
        session.group = Some("backend".to_string());
        session.color = Some("#FF5733".to_string());

        let json = serde_json::to_string(&session).unwrap();
        let parsed: Session = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.status, SessionStatus::Working);
        assert_eq!(parsed.current_task, Some(TaskId("task-001".to_string())));
        assert_eq!(parsed.context_usage, Some(0.75));
        assert_eq!(parsed.group, Some("backend".to_string()));
        assert_eq!(parsed.color, Some("#FF5733".to_string()));
    }

    // Task tests
    #[test]
    fn test_task_new() {
        let task = Task::new(
            TaskId("task-001".to_string()),
            "Test Task".to_string(),
            "A test task description".to_string(),
        );
        assert_eq!(task.id, TaskId("task-001".to_string()));
        assert_eq!(task.title, "Test Task");
        assert_eq!(task.description, "A test task description");
        assert_eq!(task.priority, TaskPriority::Medium);
        assert_eq!(task.status, TaskStatus::Queued);
        assert!(task.dependencies.is_empty());
        assert!(task.tags.is_empty());
        assert!(task.assigned_session.is_none());
    }

    #[test]
    fn test_task_serialization() {
        let task = Task::new(
            TaskId("task-001".to_string()),
            "Test".to_string(),
            "Description".to_string(),
        );
        let json = serde_json::to_string_pretty(&task).unwrap();
        let parsed: Task = serde_json::from_str(&json).unwrap();
        assert_eq!(task.id, parsed.id);
        assert_eq!(task.title, parsed.title);
    }

    #[test]
    fn test_task_with_all_fields() {
        let mut task = Task::new(
            TaskId("task-001".to_string()),
            "Full Task".to_string(),
            "Full description".to_string(),
        );
        task.priority = TaskPriority::High;
        task.status = TaskStatus::Working;
        task.dependencies = vec![TaskId("task-000".to_string())];
        task.tags = vec!["backend".to_string(), "urgent".to_string()];
        task.assigned_session = Some(SessionId(1));
        task.assigned_at = Some(chrono::Utc::now());
        task.started_at = Some(chrono::Utc::now());

        let json = serde_json::to_string(&task).unwrap();
        let parsed: Task = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.priority, TaskPriority::High);
        assert_eq!(parsed.status, TaskStatus::Working);
        assert_eq!(parsed.dependencies.len(), 1);
        assert_eq!(parsed.tags.len(), 2);
        assert_eq!(parsed.assigned_session, Some(SessionId(1)));
    }

    // AppState tests
    #[test]
    fn test_app_state_default() {
        let state = AppState::default();
        assert!(state.sessions.is_empty());
        assert!(matches!(
            state.layout,
            LayoutMode::Grid { rows: 2, cols: 2 }
        ));
        assert!(state.updated_at.is_none());
    }

    #[test]
    fn test_app_state_serialization() {
        let mut state = AppState::default();
        state.sessions.push(Session::new(
            SessionId(1),
            "Test".to_string(),
            PathBuf::from("/tmp"),
        ));
        state.updated_at = Some(chrono::Utc::now());

        let json = serde_json::to_string_pretty(&state).unwrap();
        let parsed: AppState = serde_json::from_str(&json).unwrap();

        assert_eq!(state.sessions.len(), parsed.sessions.len());
        assert_eq!(parsed.sessions[0].id, SessionId(1));
    }

    #[test]
    fn test_app_state_with_multiple_sessions() {
        let mut state = AppState::default();
        state.sessions.push(Session::new(
            SessionId(1),
            "Session 1".to_string(),
            PathBuf::from("/tmp/1"),
        ));
        state.sessions.push(Session::new(
            SessionId(2),
            "Session 2".to_string(),
            PathBuf::from("/tmp/2"),
        ));
        state.layout = LayoutMode::Grid { rows: 1, cols: 2 };

        let json = serde_json::to_string(&state).unwrap();
        let parsed: AppState = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.sessions.len(), 2);
        assert!(matches!(
            parsed.layout,
            LayoutMode::Grid { rows: 1, cols: 2 }
        ));
    }
}
