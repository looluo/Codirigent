//! Dirigent Verification
//!
//! Verification gate for running tests and checks on completed tasks.
//!
//! This crate provides the implementation of the verification system that
//! validates task completions by running automated checks (tests, linting,
//! type checking, etc.).
//!
//! ## Key Components
//!
//! - [`DefaultDetector`] - Auto-detects project type and verification commands
//! - [`CommandExecutor`] - Executes verification commands with timeout support
//! - [`OutputParser`] - Parses test output into structured results
//! - [`VerificationGate`] - Main implementation of the [`Verifier`] trait
//! - [`GitChangeDetector`] - Detects file changes using git
//! - [`RuleBasedRiskAssessor`] - Assesses risk levels for file changes
//!
//! ## Example
//!
//! ```no_run
//! use dirigent_verification::{DefaultDetector, VerificationGate};
//! use dirigent_core::VerificationDetector;
//! use std::path::Path;
//!
//! // Auto-detect project type and commands
//! let detector = DefaultDetector::new();
//! let commands = detector.detect(Path::new("/path/to/project"));
//!
//! // Create verification gate with default config
//! let gate = VerificationGate::new();
//! ```
//!
//! ## Change Detection Example
//!
//! ```no_run
//! use dirigent_verification::{GitChangeDetector, RuleBasedRiskAssessor};
//! use dirigent_core::{ChangeDetector, RiskAssessor, SessionId, TaskId};
//! use std::path::Path;
//!
//! // Create a change detector
//! let detector = GitChangeDetector::new();
//!
//! // Detect changes since last commit
//! let changes = detector.detect_changes(Path::new("/path/to/repo"), None).unwrap();
//!
//! // Generate a full change summary
//! let summary = detector.generate_summary(
//!     TaskId("task-001".to_string()),
//!     SessionId(1),
//!     Path::new("/path/to/repo"),
//!     Some("HEAD~1"),
//! ).unwrap();
//!
//! println!("Risk level: {:?}", summary.risk_assessment.overall_risk);
//! ```
//!
//! [`Verifier`]: dirigent_core::Verifier

#![warn(missing_docs)]
#![warn(clippy::all)]

pub mod change_detector;
pub mod detector;
pub mod executor;
pub mod parser;
pub mod risk_assessor;
pub mod verifier;

pub use change_detector::GitChangeDetector;
pub use detector::DefaultDetector;
pub use executor::{CommandExecutor, ExecutionResult};
pub use parser::{CargoTestParser, GenericParser, JestParser, OutputParser, PytestParser};
pub use risk_assessor::RuleBasedRiskAssessor;
pub use verifier::VerificationGate;
