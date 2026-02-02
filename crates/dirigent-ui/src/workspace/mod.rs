//! Workspace view for Dirigent.
//!
//! This module provides the main workspace view that contains the grid
//! of session panes and manages layout switching.
//!
//! # Architecture
//!
//! The workspace module is split into:
//! - [`core`] - Core workspace logic (layout, sessions, focus management)
//! - [`gpui`] - GPUI rendering implementation (requires `gpui-full` feature)
//!
//! # Example
//!
//! ```
//! use dirigent_ui::workspace::Workspace;
//! use dirigent_ui::layout::LayoutProfile;
//! use dirigent_core::{Session, SessionId};
//! use std::path::PathBuf;
//!
//! let mut workspace = Workspace::new();
//! workspace.set_layout(LayoutProfile::Grid2x2);
//!
//! let session = Session::new(SessionId(1), "Session 1".to_string(), PathBuf::from("/tmp"));
//! workspace.add_session(session);
//! ```

mod core;

#[cfg(feature = "gpui-full")]
pub mod gpui;

// Re-export core types
pub use core::{CellInfo, Workspace};

// Re-export GPUI view type when feature is enabled
#[cfg(feature = "gpui-full")]
pub use gpui::WorkspaceView;
