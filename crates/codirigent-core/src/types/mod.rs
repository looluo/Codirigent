//! Core types for the Codirigent application.
//!
//! This module contains all shared types used throughout the application,
//! including identifiers, enums, and core data structures.

pub mod git;
pub mod ids;
pub mod layout;
pub mod session;
pub mod state;
pub mod status;
pub mod task;
pub mod verification;
pub mod worktree;

#[cfg(test)]
mod tests;

// Re-export all public types for backward compatibility.
// This ensures `use codirigent_core::types::*` and
// `use codirigent_core::SessionId` etc. continue to work.

pub use git::{GitChangeKind, GitChangedFile, GitRepoInfo};
pub use ids::{SessionId, TaskId};
pub use layout::{GridPosition, LayoutMode, LayoutNode, SlotId, SplitDirection};
pub use session::{CodexExecutionMode, Session};
pub use state::{AppState, PaneId, PaneStackState, PaneTabGroup, QueueState, WindowState};
pub use status::{ContextThresholdState, SessionStatus, ShellState, TaskPriority, TaskStatus};
pub use task::{RetryConfig, Task, VerificationConfig};
pub use verification::{TestFailure, TestResults, VerificationResult};
pub use worktree::{Worktree, WorktreeCreateOptions};
