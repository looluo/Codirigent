//! Session notes generation and management.
//!
//! This module provides implementations for generating session notes
//! from task completions. Notes are markdown documents designed for
//! human review (not AI context) that summarize what was accomplished.
//!
//! ## Components
//!
//! - [`DefaultNotesGenerator`] - Generates and saves session notes
//! - [`PatternLearningsExtractor`] - Extracts learnings from session output
//!
//! ## Example
//!
//! ```
//! use codirigent_verification::notes::{DefaultNotesGenerator, PatternLearningsExtractor};
//! use codirigent_core::{
//!     NotesGenerator, LearningsExtractor, CompletionStatus, SessionId, TaskId,
//! };
//!
//! // Generate a note
//! let generator = DefaultNotesGenerator::new();
//! let note = generator.generate(
//!     TaskId("task-001".to_string()),
//!     SessionId(1),
//!     "Refactor Auth".to_string(),
//!     45,
//!     CompletionStatus::Completed,
//!     None,
//!     None,
//! ).unwrap();
//!
//! // Render to markdown
//! let markdown = generator.render_markdown(&note);
//! assert!(markdown.contains("Refactor Auth"));
//!
//! // Extract learnings from output
//! let extractor = PatternLearningsExtractor::new();
//! let learnings = extractor.extract("I recommend using jose instead of jsonwebtoken");
//! ```

pub mod extractor;
pub mod generator;

pub use extractor::PatternLearningsExtractor;
pub use generator::DefaultNotesGenerator;
