//! Application and queue state types.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::ids::{SessionId, TaskId};
use super::layout::{LayoutMode, SlotId};
use super::session::Session;

/// Persistent identifier for a visible pane.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum PaneId {
    /// Grid-layout pane by stable cell index.
    GridCell {
        /// Zero-based grid cell index in row-major order.
        index: usize,
    },
    /// Split-tree pane by slot identifier.
    SplitSlot {
        /// Stable split-tree slot identifier.
        slot: SlotId,
    },
}

/// Persistent tab state for a visible pane.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaneTabGroup {
    /// Pane that owns the tab stack.
    pub pane: PaneId,
    /// Ordered session IDs in the tab strip.
    pub session_ids: Vec<SessionId>,
    /// Active session currently rendered in the pane.
    pub active_session_id: SessionId,
}

/// Persisted ordered pane stack state, including hidden stacks.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaneStackState {
    /// Ordered session IDs in the stack.
    pub session_ids: Vec<SessionId>,
    /// Active session currently rendered when the stack is visible.
    pub active_session_id: SessionId,
}

/// Persisted window position and size.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowState {
    /// Window X position in pixels.
    pub x: f32,
    /// Window Y position in pixels.
    pub y: f32,
    /// Window width in pixels.
    pub width: f32,
    /// Window height in pixels.
    pub height: f32,
    /// Whether the window was maximized.
    pub is_maximized: bool,
}

/// Application state persisted to disk.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppState {
    /// All active sessions.
    pub sessions: Vec<Session>,
    /// Current layout mode.
    pub layout: LayoutMode,
    /// Persisted per-pane tab stacks.
    #[serde(default)]
    pub pane_tab_groups: Vec<PaneTabGroup>,
    /// Persisted pane stacks in workspace order, including hidden stacks.
    #[serde(default)]
    pub pane_stacks: Vec<PaneStackState>,
    /// Last updated timestamp.
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Saved window position and size.
    #[serde(default)]
    pub window_bounds: Option<WindowState>,
}

/// Queue state persisted to queue.json.
///
/// Tracks the ordered list of queued tasks and blocked task dependencies.
/// This is used by the task scheduler to determine which tasks can be
/// assigned to sessions.
///
/// # Example
///
/// ```
/// use codirigent_core::{QueueState, TaskId};
///
/// let mut state = QueueState::default();
/// state.order.push(TaskId::from("task-001"));
/// state.order.push(TaskId::from("task-002"));
///
/// // task-003 is blocked by task-001
/// state.blocked.insert(
///     TaskId::from("task-003"),
///     vec![TaskId::from("task-001")],
/// );
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct QueueState {
    /// Ordered list of queued task IDs (priority order).
    pub order: Vec<TaskId>,

    /// Map of blocked task ID to blocking task IDs.
    pub blocked: HashMap<TaskId, Vec<TaskId>>,

    /// Last updated timestamp.
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
}
