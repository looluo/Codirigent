//! Codirigent File Tree
//!
//! File tree browser component providing directory browsing and path
//! drag-and-drop functionality for Codirigent.
//!
//! # Example
//!
//! ```no_run
//! use codirigent_filetree::{FileTree, FileEntry, FileIcon};
//! use std::path::PathBuf;
//!
//! // Create a file tree from a directory
//! let mut tree = FileTree::new(PathBuf::from("/home/user/project")).unwrap();
//!
//! // Expand a directory
//! tree.expand(std::path::Path::new("/home/user/project/src")).unwrap();
//!
//! // Get visible entries for rendering
//! for (depth, entry) in tree.visible_entries() {
//!     let icon = entry.icon().emoji();
//!     println!("{}{} {}", "  ".repeat(depth), icon, entry.name);
//! }
//! ```

#![warn(missing_docs)]
#![warn(clippy::all)]

mod entry;
mod error;
mod tree;

pub use entry::{FileEntry, FileIcon};
pub use error::{FileTreeError, Result};
pub use tree::{quote_path_for_terminal, FileTree, TerminalPathStyle};
