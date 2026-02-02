//! Input handling for terminal.
//!
//! This module provides keyboard and mouse input translation for the terminal.
//! It converts GPUI input events into terminal escape sequences that can be
//! sent to the PTY.
//!
//! # Submodules
//!
//! - [`keyboard`] - Keyboard input translation (key-to-bytes)
//! - [`mouse`] - Mouse input handling and selection

mod keyboard;
mod mouse;

// Re-export all public types
pub use keyboard::{key_to_bytes, TerminalKeystroke, TerminalModifiers};
pub use mouse::{TerminalMouseButton, TerminalMouseEvent, TerminalMouseHandler};
