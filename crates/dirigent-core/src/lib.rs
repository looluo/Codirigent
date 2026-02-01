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

#![warn(missing_docs)]
#![warn(clippy::all)]

pub mod error;
pub mod event_bus;
pub mod events;
pub mod storage;
pub mod traits;
pub mod types;

// Re-export commonly used items
pub use error::{DirigentError, Result};
pub use event_bus::DefaultEventBus;
pub use events::DirigentEvent;
pub use storage::FileStorageService;
pub use traits::{EventBus, ProcessMonitor, SessionManager, StorageService};
pub use types::*;
