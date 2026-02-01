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

#![warn(missing_docs)]
#![warn(clippy::all)]

pub mod error;
pub mod event_bus;
pub mod events;
pub mod traits;
pub mod types;

// Re-export commonly used items
pub use error::{DirigentError, Result};
pub use event_bus::DefaultEventBus;
pub use events::DirigentEvent;
pub use traits::{EventBus, ProcessMonitor, SessionManager, StorageService};
pub use types::*;
