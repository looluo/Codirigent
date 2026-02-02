//! Codirigent Plugin System
//!
//! Provides plugin management infrastructure for Codirigent.
//!
//! ## Overview
//!
//! The plugin system allows extending Codirigent's functionality without modifying core code.
//! Plugins can:
//! - Subscribe to and handle events
//! - Provide custom functionality
//! - Integrate with the session system
//!
//! ## Example
//!
//! ```ignore
//! use codirigent_plugin::{DefaultPluginManager, PluginRegistry};
//! use std::path::PathBuf;
//! use std::sync::Arc;
//!
//! // Create a plugin manager
//! let event_bus = Arc::new(DefaultEventBus::new(16));
//! let mut manager = DefaultPluginManager::new(
//!     PathBuf::from("~/.codirigent/plugins"),
//!     event_bus,
//! );
//!
//! // Register a built-in plugin
//! manager.register_builtin(Box::new(MyPlugin::new())).unwrap();
//! ```

#![warn(missing_docs)]
#![warn(clippy::all)]

mod manager;
mod registry;

pub use manager::DefaultPluginManager;
pub use registry::PluginRegistry;
