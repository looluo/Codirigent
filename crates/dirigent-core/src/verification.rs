//! Verification types for the Dirigent application.
//!
//! This module contains all types related to the verification pipeline,
//! including verification results, configurations, status tracking, and events.
//!
//! ## Overview
//!
//! The verification system is responsible for validating task completions
//! by running automated checks (tests, linting, type checking, etc.) and
//! managing the retry workflow when checks fail.
//!
//! ## Key Types
//!
//! - [`VerificationResult`] - Result of a single verification check
//! - [`VerificationCheckType`] - Types of checks (unit test, lint, etc.)
//! - [`VerificationFailure`] - Details of a single failure
//! - [`VerificationConfig`] - Configuration for verification commands
//! - [`VerificationStatus`] - Current status of verification for a task
//! - [`VerificationState`] - State machine for the verification pipeline
//! - [`VerificationEvent`] - Events emitted by the verification system

use crate::types::{SessionId, TaskId};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Result of a single verification check.
///
/// Contains the outcome of running a verification command, including
/// pass/fail status, test counts, failure details, and timing.
///
/// # Example
///
/// ```
/// use dirigent_core::verification::{VerificationResult, VerificationCheckType};
///
/// let result = VerificationResult {
///     check_type: VerificationCheckType::UnitTest,
///     passed: true,
///     passed_count: Some(23),
///     total_count: Some(23),
///     failures: vec![],
///     duration_ms: 1500,
///     raw_output: None,
/// };
/// assert!(result.passed);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    /// Type of verification that was performed.
    pub check_type: VerificationCheckType,
    /// Whether the check passed.
    pub passed: bool,
    /// Number of tests that passed (if applicable).
    pub passed_count: Option<u32>,
    /// Total number of tests (if applicable).
    pub total_count: Option<u32>,
    /// Detailed failure messages.
    pub failures: Vec<VerificationFailure>,
    /// Time taken to run the check in milliseconds.
    pub duration_ms: u64,
    /// Raw output from the verification command.
    pub raw_output: Option<String>,
}

impl VerificationResult {
    /// Create a new passed verification result.
    ///
    /// # Arguments
    ///
    /// * `check_type` - The type of verification check
    /// * `duration_ms` - Time taken in milliseconds
    pub fn passed(check_type: VerificationCheckType, duration_ms: u64) -> Self {
        Self {
            check_type,
            passed: true,
            passed_count: None,
            total_count: None,
            failures: vec![],
            duration_ms,
            raw_output: None,
        }
    }

    /// Create a new failed verification result.
    ///
    /// # Arguments
    ///
    /// * `check_type` - The type of verification check
    /// * `failures` - List of verification failures
    /// * `duration_ms` - Time taken in milliseconds
    pub fn failed(
        check_type: VerificationCheckType,
        failures: Vec<VerificationFailure>,
        duration_ms: u64,
    ) -> Self {
        Self {
            check_type,
            passed: false,
            passed_count: None,
            total_count: None,
            failures,
            duration_ms,
            raw_output: None,
        }
    }

    /// Set the test counts for this result.
    ///
    /// # Arguments
    ///
    /// * `passed` - Number of tests that passed
    /// * `total` - Total number of tests
    pub fn with_counts(mut self, passed: u32, total: u32) -> Self {
        self.passed_count = Some(passed);
        self.total_count = Some(total);
        self
    }

    /// Set the raw output for this result.
    ///
    /// # Arguments
    ///
    /// * `output` - Raw command output
    pub fn with_raw_output(mut self, output: String) -> Self {
        self.raw_output = Some(output);
        self
    }
}

/// Type of verification check.
///
/// Represents the different categories of verification that can be performed
/// on a codebase after task completion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VerificationCheckType {
    /// Unit tests (npm test, cargo test, pytest, etc.)
    UnitTest,
    /// Integration tests.
    IntegrationTest,
    /// Type checking (tsc, mypy, etc.)
    TypeCheck,
    /// Linting (eslint, clippy, etc.)
    Lint,
    /// Code formatting check.
    Format,
    /// Custom verification script.
    Custom,
}

impl std::fmt::Display for VerificationCheckType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VerificationCheckType::UnitTest => write!(f, "Unit Test"),
            VerificationCheckType::IntegrationTest => write!(f, "Integration Test"),
            VerificationCheckType::TypeCheck => write!(f, "Type Check"),
            VerificationCheckType::Lint => write!(f, "Lint"),
            VerificationCheckType::Format => write!(f, "Format"),
            VerificationCheckType::Custom => write!(f, "Custom"),
        }
    }
}

/// Details of a single verification failure.
///
/// Provides structured information about what went wrong during verification,
/// including the location of the failure and expected vs actual values.
///
/// # Example
///
/// ```
/// use dirigent_core::verification::VerificationFailure;
/// use std::path::PathBuf;
///
/// let failure = VerificationFailure {
///     name: "test_auth_expired_token".to_string(),
///     file: Some(PathBuf::from("src/auth.test.ts")),
///     line: Some(42),
///     expected: Some("401".to_string()),
///     actual: Some("200".to_string()),
///     message: "Expected 401, received 200".to_string(),
/// };
/// assert_eq!(failure.line, Some(42));
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationFailure {
    /// Test or check name that failed.
    pub name: String,
    /// File where the failure occurred (if known).
    pub file: Option<PathBuf>,
    /// Line number (if known).
    pub line: Option<u32>,
    /// Expected value or condition.
    pub expected: Option<String>,
    /// Actual value or condition.
    pub actual: Option<String>,
    /// Full error message.
    pub message: String,
}

impl VerificationFailure {
    /// Create a new verification failure with just a name and message.
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the failed test or check
    /// * `message` - Error message
    pub fn new(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            file: None,
            line: None,
            expected: None,
            actual: None,
            message: message.into(),
        }
    }

    /// Set the file location for this failure.
    pub fn with_file(mut self, file: PathBuf) -> Self {
        self.file = Some(file);
        self
    }

    /// Set the line number for this failure.
    pub fn with_line(mut self, line: u32) -> Self {
        self.line = Some(line);
        self
    }

    /// Set the expected and actual values for this failure.
    pub fn with_comparison(mut self, expected: String, actual: String) -> Self {
        self.expected = Some(expected);
        self.actual = Some(actual);
        self
    }
}

/// Configuration for verification commands.
///
/// Defines how verification should be performed for a project,
/// including which commands to run and retry behavior.
///
/// # Example
///
/// ```
/// use dirigent_core::verification::{VerificationConfig, VerificationCommands};
///
/// let config = VerificationConfig::default();
/// assert!(config.enabled);
/// assert!(config.auto_detect);
/// assert_eq!(config.max_retries, 3);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationConfig {
    /// Whether verification is enabled.
    pub enabled: bool,
    /// Whether to auto-detect verification commands.
    pub auto_detect: bool,
    /// Maximum retry attempts before marking as blocked.
    pub max_retries: u32,
    /// Specific commands for each check type.
    pub commands: VerificationCommands,
    /// Whether human review is required after verification passes.
    pub requires_human_review: bool,
}

impl Default for VerificationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            auto_detect: true,
            max_retries: 3,
            commands: VerificationCommands::default(),
            requires_human_review: true,
        }
    }
}

/// Verification commands for different check types.
///
/// Stores the actual shell commands to run for each type of verification.
/// Commands are optional to allow auto-detection or selective verification.
///
/// # Example
///
/// ```
/// use dirigent_core::verification::VerificationCommands;
///
/// let mut commands = VerificationCommands::default();
/// commands.unit = Some("npm test".to_string());
/// commands.lint = Some("npm run lint".to_string());
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VerificationCommands {
    /// Unit test command (e.g., "npm test", "cargo test").
    pub unit: Option<String>,
    /// Integration test command.
    pub integration: Option<String>,
    /// Type check command (e.g., "tsc --noEmit").
    pub type_check: Option<String>,
    /// Lint command (e.g., "npm run lint").
    pub lint: Option<String>,
    /// Format check command.
    pub format: Option<String>,
    /// Custom verification scripts.
    pub custom: Vec<String>,
}

impl VerificationCommands {
    /// Check if any commands are configured.
    pub fn has_any(&self) -> bool {
        self.unit.is_some()
            || self.integration.is_some()
            || self.type_check.is_some()
            || self.lint.is_some()
            || self.format.is_some()
            || !self.custom.is_empty()
    }

    /// Get all configured commands as a list of (check_type, command) pairs.
    pub fn to_list(&self) -> Vec<(VerificationCheckType, String)> {
        let mut list = Vec::new();
        if let Some(ref cmd) = self.unit {
            list.push((VerificationCheckType::UnitTest, cmd.clone()));
        }
        if let Some(ref cmd) = self.integration {
            list.push((VerificationCheckType::IntegrationTest, cmd.clone()));
        }
        if let Some(ref cmd) = self.type_check {
            list.push((VerificationCheckType::TypeCheck, cmd.clone()));
        }
        if let Some(ref cmd) = self.lint {
            list.push((VerificationCheckType::Lint, cmd.clone()));
        }
        if let Some(ref cmd) = self.format {
            list.push((VerificationCheckType::Format, cmd.clone()));
        }
        for cmd in &self.custom {
            list.push((VerificationCheckType::Custom, cmd.clone()));
        }
        list
    }
}

/// Status of the verification pipeline for a task.
///
/// Tracks the complete verification state for a task including
/// all results, retry counts, and timing information.
///
/// # Example
///
/// ```
/// use dirigent_core::verification::{VerificationStatus, VerificationState};
/// use dirigent_core::{TaskId, SessionId};
///
/// let status = VerificationStatus {
///     task_id: TaskId("task-001".to_string()),
///     session_id: SessionId(1),
///     state: VerificationState::Running,
///     retry_count: 0,
///     results: vec![],
///     started_at: chrono::Utc::now(),
///     completed_at: None,
/// };
/// assert_eq!(status.state, VerificationState::Running);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationStatus {
    /// Task being verified.
    pub task_id: TaskId,
    /// Session that completed the task.
    pub session_id: SessionId,
    /// Current state of verification.
    pub state: VerificationState,
    /// Number of retry attempts made.
    pub retry_count: u32,
    /// Results from each verification check.
    pub results: Vec<VerificationResult>,
    /// When verification started.
    pub started_at: chrono::DateTime<chrono::Utc>,
    /// When verification completed (if finished).
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl VerificationStatus {
    /// Create a new verification status in the pending state.
    pub fn new(task_id: TaskId, session_id: SessionId) -> Self {
        Self {
            task_id,
            session_id,
            state: VerificationState::Pending,
            retry_count: 0,
            results: vec![],
            started_at: chrono::Utc::now(),
            completed_at: None,
        }
    }

    /// Check if all verification checks passed.
    pub fn all_passed(&self) -> bool {
        !self.results.is_empty() && self.results.iter().all(|r| r.passed)
    }

    /// Get all failures from all results.
    pub fn all_failures(&self) -> Vec<&VerificationFailure> {
        self.results
            .iter()
            .flat_map(|r| &r.failures)
            .collect()
    }

    /// Get the total duration of all checks in milliseconds.
    pub fn total_duration_ms(&self) -> u64 {
        self.results.iter().map(|r| r.duration_ms).sum()
    }

    /// Mark the verification as completed.
    pub fn complete(&mut self, state: VerificationState) {
        self.state = state;
        self.completed_at = Some(chrono::Utc::now());
    }
}

/// Current state of the verification pipeline.
///
/// Represents the state machine for verification workflow,
/// from initial pending state through completion or blocking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum VerificationState {
    /// Waiting to start verification.
    #[default]
    Pending,
    /// Verification checks are running.
    Running,
    /// All checks passed.
    Passed,
    /// One or more checks failed.
    Failed,
    /// Failure was sent back to session for retry.
    RetryingInSession,
    /// Max retries exceeded, blocked for human intervention.
    Blocked,
    /// Verification was skipped.
    Skipped,
}

impl std::fmt::Display for VerificationState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VerificationState::Pending => write!(f, "Pending"),
            VerificationState::Running => write!(f, "Running"),
            VerificationState::Passed => write!(f, "Passed"),
            VerificationState::Failed => write!(f, "Failed"),
            VerificationState::RetryingInSession => write!(f, "Retrying in Session"),
            VerificationState::Blocked => write!(f, "Blocked"),
            VerificationState::Skipped => write!(f, "Skipped"),
        }
    }
}

/// Event emitted by the verification system.
///
/// These events are published to the event bus to notify other
/// modules about verification progress and outcomes.
#[derive(Debug, Clone)]
pub enum VerificationEvent {
    /// Verification started for a task.
    Started {
        /// The task being verified.
        task_id: TaskId,
        /// The session that completed the task.
        session_id: SessionId,
    },
    /// A single check completed.
    CheckCompleted {
        /// The task being verified.
        task_id: TaskId,
        /// The result of the check.
        result: VerificationResult,
    },
    /// All verification passed.
    Passed {
        /// The task that passed verification.
        task_id: TaskId,
    },
    /// Verification failed, retrying.
    FailedRetrying {
        /// The task that failed verification.
        task_id: TaskId,
        /// Current retry count.
        retry_count: u32,
        /// List of failures to address.
        failures: Vec<VerificationFailure>,
    },
    /// Verification failed, max retries exceeded.
    FailedBlocked {
        /// The task that is now blocked.
        task_id: TaskId,
        /// List of failures that could not be resolved.
        failures: Vec<VerificationFailure>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    // VerificationResult tests

    #[test]
    fn test_verification_result_passed() {
        let result = VerificationResult {
            check_type: VerificationCheckType::UnitTest,
            passed: true,
            passed_count: Some(23),
            total_count: Some(23),
            failures: vec![],
            duration_ms: 1500,
            raw_output: None,
        };
        assert!(result.passed);
        assert!(result.failures.is_empty());
    }

    #[test]
    fn test_verification_result_failed() {
        let failure = VerificationFailure {
            name: "test_auth_expired_token".to_string(),
            file: Some(PathBuf::from("src/auth.test.ts")),
            line: Some(42),
            expected: Some("401".to_string()),
            actual: Some("200".to_string()),
            message: "Expected 401, received 200".to_string(),
        };
        let result = VerificationResult {
            check_type: VerificationCheckType::UnitTest,
            passed: false,
            passed_count: Some(21),
            total_count: Some(23),
            failures: vec![failure],
            duration_ms: 2000,
            raw_output: None,
        };
        assert!(!result.passed);
        assert_eq!(result.failures.len(), 1);
    }

    #[test]
    fn test_verification_result_passed_constructor() {
        let result = VerificationResult::passed(VerificationCheckType::Lint, 500);
        assert!(result.passed);
        assert!(result.failures.is_empty());
        assert_eq!(result.duration_ms, 500);
    }

    #[test]
    fn test_verification_result_failed_constructor() {
        let failures = vec![VerificationFailure::new("test_foo", "assertion failed")];
        let result = VerificationResult::failed(VerificationCheckType::UnitTest, failures, 1000);
        assert!(!result.passed);
        assert_eq!(result.failures.len(), 1);
        assert_eq!(result.duration_ms, 1000);
    }

    #[test]
    fn test_verification_result_with_counts() {
        let result = VerificationResult::passed(VerificationCheckType::UnitTest, 100)
            .with_counts(10, 10);
        assert_eq!(result.passed_count, Some(10));
        assert_eq!(result.total_count, Some(10));
    }

    #[test]
    fn test_verification_result_with_raw_output() {
        let result = VerificationResult::passed(VerificationCheckType::UnitTest, 100)
            .with_raw_output("test output".to_string());
        assert_eq!(result.raw_output, Some("test output".to_string()));
    }

    #[test]
    fn test_verification_result_serialization() {
        let result = VerificationResult {
            check_type: VerificationCheckType::UnitTest,
            passed: true,
            passed_count: Some(5),
            total_count: Some(5),
            failures: vec![],
            duration_ms: 100,
            raw_output: Some("all tests passed".to_string()),
        };
        let json = serde_json::to_string(&result).unwrap();
        let parsed: VerificationResult = serde_json::from_str(&json).unwrap();
        assert_eq!(result.check_type, parsed.check_type);
        assert_eq!(result.passed, parsed.passed);
        assert_eq!(result.passed_count, parsed.passed_count);
    }

    // VerificationCheckType tests

    #[test]
    fn test_verification_check_type_serialization() {
        let check = VerificationCheckType::UnitTest;
        let json = serde_json::to_string(&check).unwrap();
        assert_eq!(json, "\"UnitTest\"");
    }

    #[test]
    fn test_verification_check_type_all_variants() {
        let variants = [
            VerificationCheckType::UnitTest,
            VerificationCheckType::IntegrationTest,
            VerificationCheckType::TypeCheck,
            VerificationCheckType::Lint,
            VerificationCheckType::Format,
            VerificationCheckType::Custom,
        ];
        for variant in variants {
            let json = serde_json::to_string(&variant).unwrap();
            let parsed: VerificationCheckType = serde_json::from_str(&json).unwrap();
            assert_eq!(variant, parsed);
        }
    }

    #[test]
    fn test_verification_check_type_display() {
        assert_eq!(format!("{}", VerificationCheckType::UnitTest), "Unit Test");
        assert_eq!(format!("{}", VerificationCheckType::IntegrationTest), "Integration Test");
        assert_eq!(format!("{}", VerificationCheckType::TypeCheck), "Type Check");
        assert_eq!(format!("{}", VerificationCheckType::Lint), "Lint");
        assert_eq!(format!("{}", VerificationCheckType::Format), "Format");
        assert_eq!(format!("{}", VerificationCheckType::Custom), "Custom");
    }

    // VerificationFailure tests

    #[test]
    fn test_verification_failure_new() {
        let failure = VerificationFailure::new("test_foo", "assertion failed");
        assert_eq!(failure.name, "test_foo");
        assert_eq!(failure.message, "assertion failed");
        assert!(failure.file.is_none());
        assert!(failure.line.is_none());
    }

    #[test]
    fn test_verification_failure_with_file() {
        let failure = VerificationFailure::new("test_foo", "failed")
            .with_file(PathBuf::from("src/test.rs"));
        assert_eq!(failure.file, Some(PathBuf::from("src/test.rs")));
    }

    #[test]
    fn test_verification_failure_with_line() {
        let failure = VerificationFailure::new("test_foo", "failed")
            .with_line(42);
        assert_eq!(failure.line, Some(42));
    }

    #[test]
    fn test_verification_failure_with_comparison() {
        let failure = VerificationFailure::new("test_foo", "mismatch")
            .with_comparison("expected".to_string(), "actual".to_string());
        assert_eq!(failure.expected, Some("expected".to_string()));
        assert_eq!(failure.actual, Some("actual".to_string()));
    }

    #[test]
    fn test_verification_failure_serialization() {
        let failure = VerificationFailure {
            name: "test_auth".to_string(),
            file: Some(PathBuf::from("src/auth.rs")),
            line: Some(10),
            expected: Some("true".to_string()),
            actual: Some("false".to_string()),
            message: "Expected true, got false".to_string(),
        };
        let json = serde_json::to_string(&failure).unwrap();
        let parsed: VerificationFailure = serde_json::from_str(&json).unwrap();
        assert_eq!(failure.name, parsed.name);
        assert_eq!(failure.file, parsed.file);
        assert_eq!(failure.line, parsed.line);
    }

    // VerificationConfig tests

    #[test]
    fn test_verification_config_default() {
        let config = VerificationConfig::default();
        assert!(config.enabled);
        assert!(config.auto_detect);
        assert_eq!(config.max_retries, 3);
        assert!(config.requires_human_review);
    }

    #[test]
    fn test_verification_config_serialization() {
        let config = VerificationConfig {
            enabled: true,
            auto_detect: false,
            max_retries: 5,
            commands: VerificationCommands {
                unit: Some("npm test".to_string()),
                integration: None,
                type_check: Some("tsc --noEmit".to_string()),
                lint: None,
                format: None,
                custom: vec![],
            },
            requires_human_review: false,
        };
        let json = serde_json::to_string_pretty(&config).unwrap();
        let parsed: VerificationConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config.max_retries, parsed.max_retries);
        assert_eq!(config.enabled, parsed.enabled);
    }

    // VerificationCommands tests

    #[test]
    fn test_verification_commands_default() {
        let commands = VerificationCommands::default();
        assert!(commands.unit.is_none());
        assert!(commands.integration.is_none());
        assert!(commands.custom.is_empty());
    }

    #[test]
    fn test_verification_commands_has_any() {
        let mut commands = VerificationCommands::default();
        assert!(!commands.has_any());

        commands.unit = Some("npm test".to_string());
        assert!(commands.has_any());
    }

    #[test]
    fn test_verification_commands_has_any_custom() {
        let mut commands = VerificationCommands::default();
        commands.custom.push("./scripts/verify.sh".to_string());
        assert!(commands.has_any());
    }

    #[test]
    fn test_verification_commands_to_list() {
        let commands = VerificationCommands {
            unit: Some("npm test".to_string()),
            integration: None,
            type_check: Some("tsc --noEmit".to_string()),
            lint: None,
            format: None,
            custom: vec!["./verify.sh".to_string()],
        };
        let list = commands.to_list();
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].0, VerificationCheckType::UnitTest);
        assert_eq!(list[1].0, VerificationCheckType::TypeCheck);
        assert_eq!(list[2].0, VerificationCheckType::Custom);
    }

    #[test]
    fn test_verification_commands_serialization() {
        let commands = VerificationCommands {
            unit: Some("cargo test".to_string()),
            integration: Some("cargo test --test integration".to_string()),
            type_check: None,
            lint: Some("cargo clippy".to_string()),
            format: Some("cargo fmt --check".to_string()),
            custom: vec!["./check.sh".to_string()],
        };
        let json = serde_json::to_string(&commands).unwrap();
        let parsed: VerificationCommands = serde_json::from_str(&json).unwrap();
        assert_eq!(commands.unit, parsed.unit);
        assert_eq!(commands.custom, parsed.custom);
    }

    // VerificationState tests

    #[test]
    fn test_verification_state_default() {
        assert_eq!(VerificationState::default(), VerificationState::Pending);
    }

    #[test]
    fn test_verification_state_display() {
        assert_eq!(format!("{}", VerificationState::Pending), "Pending");
        assert_eq!(format!("{}", VerificationState::Running), "Running");
        assert_eq!(format!("{}", VerificationState::Passed), "Passed");
        assert_eq!(format!("{}", VerificationState::Failed), "Failed");
        assert_eq!(format!("{}", VerificationState::RetryingInSession), "Retrying in Session");
        assert_eq!(format!("{}", VerificationState::Blocked), "Blocked");
        assert_eq!(format!("{}", VerificationState::Skipped), "Skipped");
    }

    #[test]
    fn test_verification_state_serialization() {
        let states = [
            VerificationState::Pending,
            VerificationState::Running,
            VerificationState::Passed,
            VerificationState::Failed,
            VerificationState::RetryingInSession,
            VerificationState::Blocked,
            VerificationState::Skipped,
        ];
        for state in states {
            let json = serde_json::to_string(&state).unwrap();
            let parsed: VerificationState = serde_json::from_str(&json).unwrap();
            assert_eq!(state, parsed);
        }
    }

    // VerificationStatus tests

    #[test]
    fn test_verification_status_creation() {
        let status = VerificationStatus {
            task_id: TaskId("task-001".to_string()),
            session_id: SessionId(1),
            state: VerificationState::Running,
            retry_count: 0,
            results: vec![],
            started_at: chrono::Utc::now(),
            completed_at: None,
        };
        assert_eq!(status.state, VerificationState::Running);
        assert!(status.completed_at.is_none());
    }

    #[test]
    fn test_verification_status_new() {
        let status = VerificationStatus::new(
            TaskId("task-001".to_string()),
            SessionId(1),
        );
        assert_eq!(status.state, VerificationState::Pending);
        assert_eq!(status.retry_count, 0);
        assert!(status.results.is_empty());
    }

    #[test]
    fn test_verification_status_all_passed() {
        let mut status = VerificationStatus::new(
            TaskId("task-001".to_string()),
            SessionId(1),
        );
        assert!(!status.all_passed()); // No results

        status.results.push(VerificationResult::passed(VerificationCheckType::UnitTest, 100));
        status.results.push(VerificationResult::passed(VerificationCheckType::Lint, 50));
        assert!(status.all_passed());

        let failures = vec![VerificationFailure::new("test", "failed")];
        status.results.push(VerificationResult::failed(VerificationCheckType::TypeCheck, failures, 100));
        assert!(!status.all_passed());
    }

    #[test]
    fn test_verification_status_all_failures() {
        let mut status = VerificationStatus::new(
            TaskId("task-001".to_string()),
            SessionId(1),
        );

        status.results.push(VerificationResult::passed(VerificationCheckType::Lint, 100));

        let failures1 = vec![
            VerificationFailure::new("test1", "failed1"),
            VerificationFailure::new("test2", "failed2"),
        ];
        status.results.push(VerificationResult::failed(VerificationCheckType::UnitTest, failures1, 100));

        let failures2 = vec![VerificationFailure::new("test3", "failed3")];
        status.results.push(VerificationResult::failed(VerificationCheckType::TypeCheck, failures2, 100));

        let all_failures = status.all_failures();
        assert_eq!(all_failures.len(), 3);
    }

    #[test]
    fn test_verification_status_total_duration() {
        let mut status = VerificationStatus::new(
            TaskId("task-001".to_string()),
            SessionId(1),
        );
        status.results.push(VerificationResult::passed(VerificationCheckType::UnitTest, 100));
        status.results.push(VerificationResult::passed(VerificationCheckType::Lint, 50));
        status.results.push(VerificationResult::passed(VerificationCheckType::TypeCheck, 200));

        assert_eq!(status.total_duration_ms(), 350);
    }

    #[test]
    fn test_verification_status_complete() {
        let mut status = VerificationStatus::new(
            TaskId("task-001".to_string()),
            SessionId(1),
        );
        assert!(status.completed_at.is_none());

        status.complete(VerificationState::Passed);
        assert_eq!(status.state, VerificationState::Passed);
        assert!(status.completed_at.is_some());
    }

    #[test]
    fn test_verification_status_serialization() {
        let status = VerificationStatus {
            task_id: TaskId("task-001".to_string()),
            session_id: SessionId(1),
            state: VerificationState::Passed,
            retry_count: 1,
            results: vec![],
            started_at: chrono::Utc::now(),
            completed_at: Some(chrono::Utc::now()),
        };
        let json = serde_json::to_string_pretty(&status).unwrap();
        let parsed: VerificationStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(status.task_id, parsed.task_id);
        assert_eq!(status.state, parsed.state);
    }

    // VerificationEvent tests

    #[test]
    fn test_verification_event_started() {
        let event = VerificationEvent::Started {
            task_id: TaskId("task-001".to_string()),
            session_id: SessionId(1),
        };
        assert!(matches!(event, VerificationEvent::Started { .. }));
    }

    #[test]
    fn test_verification_event_check_completed() {
        let result = VerificationResult::passed(VerificationCheckType::UnitTest, 100);
        let event = VerificationEvent::CheckCompleted {
            task_id: TaskId("task-001".to_string()),
            result,
        };
        assert!(matches!(event, VerificationEvent::CheckCompleted { .. }));
    }

    #[test]
    fn test_verification_event_passed() {
        let event = VerificationEvent::Passed {
            task_id: TaskId("task-001".to_string()),
        };
        assert!(matches!(event, VerificationEvent::Passed { .. }));
    }

    #[test]
    fn test_verification_event_failed_retrying() {
        let event = VerificationEvent::FailedRetrying {
            task_id: TaskId("task-001".to_string()),
            retry_count: 2,
            failures: vec![],
        };
        if let VerificationEvent::FailedRetrying { retry_count, .. } = event {
            assert_eq!(retry_count, 2);
        } else {
            panic!("Expected FailedRetrying");
        }
    }

    #[test]
    fn test_verification_event_failed_blocked() {
        let failures = vec![VerificationFailure::new("test", "failed")];
        let event = VerificationEvent::FailedBlocked {
            task_id: TaskId("task-001".to_string()),
            failures,
        };
        if let VerificationEvent::FailedBlocked { failures, .. } = event {
            assert_eq!(failures.len(), 1);
        } else {
            panic!("Expected FailedBlocked");
        }
    }

    #[test]
    fn test_verification_event_clone() {
        let event = VerificationEvent::Passed {
            task_id: TaskId("task-001".to_string()),
        };
        let cloned = event.clone();
        assert!(matches!(cloned, VerificationEvent::Passed { .. }));
    }
}
