//! Core service traits for the Dirigent application.
//!
//! This module defines the trait contracts that govern inter-crate
//! communication. Each trait represents a service that can be
//! implemented by different crates.
//!
//! ## Core Traits
//!
//! - [`EventBus`]: Cross-module event publication and subscription
//! - [`SessionManager`]: Session lifecycle management
//! - [`ProcessMonitor`]: Process state monitoring
//! - [`StorageService`]: File-based persistence
//!
//! ## Verification Traits
//!
//! - [`Verifier`]: Run verification checks on task completions
//! - [`VerificationDetector`]: Auto-detect verification commands
//! - [`FailureFormatter`]: Format verification failures for display
//!
//! ## Project Types
//!
//! - [`ProjectType`]: Known project types for auto-detection

mod services;
pub mod verifier;

// Re-export core service traits
pub use services::*;

// Re-export verification traits
pub use verifier::{FailureFormatter, ProjectType, VerificationDetector, Verifier};
