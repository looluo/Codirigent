//! Dirigent Core
//!
//! Core types, traits, events, and services for the Dirigent application.
//!
//! ## Modules
//!
//! - [`types`] - Core data types (SessionId, Session, Task, etc.)
//! - [`events`] - Event types for cross-module communication
//! - [`traits`] - Service trait definitions
//! - [`event_bus`] - Default EventBus implementation
//! - [`storage`] - File-based storage service
//! - [`plugin`] - Plugin system types and traits
//! - [`verification`] - Verification types and events
//! - [`context`] - Context window tracking for AI sessions
//! - [`config`] - Configuration types (ProjectConfig, UserSettings)
//! - [`config_service`] - Configuration loading and saving service
//! - [`skill`] - Skill management types (Skill, SkillPreset, TokenBudget)
//! - [`error`] - Error types
//!
//! ## Quick Start
//!
//! ```
//! use dirigent_core::{
//!     SessionId, Session, SessionStatus,
//!     DirigentEvent, DefaultEventBus, EventBus,
//! };
//! use std::path::PathBuf;
//!
//! // Create an event bus
//! let bus = DefaultEventBus::new(16);
//!
//! // Subscribe to events
//! let mut rx = bus.subscribe();
//!
//! // Publish an event
//! bus.publish(DirigentEvent::SessionCreated { id: SessionId(1) });
//! ```
//!
//! ## Storage Example
//!
//! ```no_run
//! use dirigent_core::{FileStorageService, StorageService};
//! use std::path::Path;
//!
//! let storage = FileStorageService::new(Path::new("/path/to/project")).unwrap();
//! let state = storage.load_state().unwrap();
//! println!("Loaded {} sessions", state.sessions.len());
//! ```
//!
//! ## Verification Example
//!
//! ```
//! use dirigent_core::verification::{
//!     VerificationResult, VerificationCheckType, VerificationConfig,
//! };
//!
//! // Create a passed verification result
//! let result = VerificationResult::passed(VerificationCheckType::UnitTest, 1500);
//! assert!(result.passed);
//!
//! // Use default verification config
//! let config = VerificationConfig::default();
//! assert!(config.enabled);
//! ```

#![warn(missing_docs)]
#![warn(clippy::all)]

pub mod config;
pub mod config_service;
pub mod context;
pub mod error;
pub mod event_bus;
pub mod events;
pub mod plugin;
pub mod scheduler;
pub mod skill;
pub mod storage;
pub mod traits;
pub mod types;
pub mod verification;

// Re-export commonly used items
pub use config::{ProjectConfig, UserSettings};
pub use config_service::{ConfigChange, ConfigService, DefaultConfigService, EffectiveConfig};
pub use context::{ContextConfig, ContextPattern, ContextTracker, ContextTrackingService, ContextUsage};
pub use error::{DirigentError, Result};
pub use event_bus::DefaultEventBus;
pub use events::DirigentEvent;
pub use scheduler::{SchedulerConfig, SchedulerMode, TaskQueue, TaskQueueService};
pub use skill::{Skill, SkillPreset, SkillType, TokenBudget};
pub use storage::FileStorageService;
pub use traits::{EventBus, ProcessMonitor, SessionManager, SkillManager, StorageService};
pub use traits::{FailureFormatter, ProjectType, VerificationDetector, Verifier};
pub use types::*;
