//! Dirigent Session
//!
//! Session management crate providing PTY abstraction, process tree
//! management, and session state tracking for Dirigent.
//!
//! # Overview
//!
//! This crate provides the foundational PTY (pseudo-terminal) handling
//! for Dirigent sessions. Each session represents a terminal running
//! an AI coding CLI tool.
//!
//! # Modules
//!
//! - [`pty`] - PTY creation, I/O, and async output reading
//!
//! # Example
//!
//! ```no_run
//! use dirigent_session::{PtyHandle, PtySize, spawn_output_reader};
//! use std::path::Path;
//!
//! // Spawn a PTY with the default shell
//! let mut pty = PtyHandle::spawn(Path::new("/tmp"), 24, 80).unwrap();
//!
//! // Send input to the terminal
//! pty.send_input(b"echo hello\n").unwrap();
//!
//! // Take the reader for async processing
//! let reader = pty.take_reader().unwrap();
//! let mut rx = spawn_output_reader(reader);
//!
//! // Output can be received via the channel
//! // rx.recv().await
//! ```

#![warn(missing_docs)]
#![warn(clippy::all)]

pub mod pty;

pub use pty::{spawn_output_reader, PtyHandle, PtySize};
