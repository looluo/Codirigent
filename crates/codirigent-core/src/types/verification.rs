//! Verification result types for task testing.

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Verification result from running tests.
///
/// Captures the output and results from running a verification command
/// (typically a test suite) for a task.
///
/// # Example
///
/// ```
/// use codirigent_core::{VerificationResult, TestResults};
/// use std::time::Duration;
///
/// let result = VerificationResult {
///     success: true,
///     exit_code: Some(0),
///     stdout: "All tests passed".to_string(),
///     stderr: "".to_string(),
///     test_results: None,
///     duration: Duration::from_secs(5),
///     run_at: chrono::Utc::now(),
/// };
/// assert!(result.success);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VerificationResult {
    /// Whether verification passed.
    pub success: bool,

    /// Exit code of the verification command.
    pub exit_code: Option<i32>,

    /// Standard output from the command.
    pub stdout: String,

    /// Standard error from the command.
    pub stderr: String,

    /// Parsed test results if available.
    pub test_results: Option<TestResults>,

    /// Duration of the verification run.
    #[serde(with = "humantime_serde")]
    pub duration: Duration,

    /// When verification was run.
    pub run_at: chrono::DateTime<chrono::Utc>,
}

/// Parsed test results from verification output.
///
/// Contains aggregate counts and individual failure details
/// extracted from the test runner output.
///
/// # Example
///
/// ```
/// use codirigent_core::TestResults;
///
/// let results = TestResults {
///     total: 10,
///     passed: 8,
///     failed: 2,
///     skipped: 0,
///     failures: vec![],
/// };
/// assert_eq!(results.total, results.passed + results.failed + results.skipped);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TestResults {
    /// Total tests run.
    pub total: u32,

    /// Tests passed.
    pub passed: u32,

    /// Tests failed.
    pub failed: u32,

    /// Tests skipped.
    pub skipped: u32,

    /// Individual failure details.
    pub failures: Vec<TestFailure>,
}

/// Details of a single test failure.
///
/// Contains information about a specific test that failed during
/// verification, including the error message and optional stack trace.
///
/// # Example
///
/// ```
/// use codirigent_core::TestFailure;
///
/// let failure = TestFailure {
///     name: "test_user_login".to_string(),
///     message: "Expected status 200, got 401".to_string(),
///     stack_trace: None,
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TestFailure {
    /// Test name/path.
    pub name: String,

    /// Error message.
    pub message: String,

    /// Stack trace if available.
    pub stack_trace: Option<String>,
}
