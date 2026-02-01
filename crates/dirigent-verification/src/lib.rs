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
//! [`Verifier`]: dirigent_core::Verifier

#![warn(missing_docs)]
#![warn(clippy::all)]

pub mod detector;
pub mod executor;
pub mod parser;
pub mod verifier;

pub use detector::DefaultDetector;
pub use executor::{CommandExecutor, ExecutionResult};
pub use parser::{CargoTestParser, GenericParser, JestParser, OutputParser, PytestParser};
pub use verifier::VerificationGate;
