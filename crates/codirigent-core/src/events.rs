//! Event types for cross-module communication.
//!
//! This module defines the [`CodirigentEvent`] enum which is used for
//! loose coupling between modules. All cross-module communication
//! should happen through events, allowing modules to react to changes
//! without tight coupling.

use crate::skill::TokenBudget;
use crate::types::*;
use std::path::PathBuf;

/// Type of content detected on clipboard.
///
/// Used to determine how to handle clipboard content when pasting
/// into sessions or saving to files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardContentType {
    /// Plain text content.
    Text,
    /// Image data (PNG/JPEG).
    Image,
    /// File paths.
    Files,
    /// Empty clipboard.
    Empty,
}

/// Events for loose coupling between modules.
///
/// All cross-module communication should happen through events,
/// allowing modules to react to changes without tight coupling.
///
/// # Event Categories
///
/// - **Session Events**: Session lifecycle and state changes
/// - **Input Detection Events**: User input requirements
/// - **Layout Events**: UI layout changes
/// - **Task Events**: Task lifecycle (Phase 2 foundation)
/// - **File Tree Events**: File drag-and-drop operations
/// - **Skill Events**: Skill management and token budget tracking
///
/// # Example
///
/// ```
/// use codirigent_core::events::CodirigentEvent;
/// use codirigent_core::types::{SessionId, SessionStatus};
///
/// let event = CodirigentEvent::SessionStatusChanged {
///     id: SessionId(1),
///     old: SessionStatus::Idle,
///     new: SessionStatus::Working,
/// };
/// ```
#[derive(Debug, Clone)]
pub enum CodirigentEvent {
    // === Session Events ===
    /// A new session was created.
    SessionCreated {
        /// The ID of the newly created session.
        id: SessionId,
    },

    /// A session was closed.
    SessionClosed {
        /// The ID of the closed session.
        id: SessionId,
    },

    /// Session status changed (detected by Input Detector).
    SessionStatusChanged {
        /// The session ID.
        id: SessionId,
        /// The previous status.
        old: SessionStatus,
        /// The new status.
        new: SessionStatus,
    },

    /// Output was received from a session's PTY.
    SessionOutputReceived {
        /// The session ID.
        id: SessionId,
        /// The raw output data.
        data: Vec<u8>,
    },

    /// Session was renamed.
    SessionRenamed {
        /// The session ID.
        id: SessionId,
        /// The old name.
        old_name: String,
        /// The new name.
        new_name: String,
    },

    /// Session group/color changed.
    SessionGroupChanged {
        /// The session ID.
        id: SessionId,
        /// The new group name (None to ungroup).
        group: Option<String>,
        /// The new color (None to use default).
        color: Option<String>,
    },

    // === Input Detection Events ===
    /// Session needs user attention (input required or permission prompt).
    AttentionRequired {
        /// The session ID.
        session_id: SessionId,
        /// Optional detail (tool name from permission prompt, or pattern from input detection).
        detail: Option<String>,
    },

    /// User provided input (attention resolved).
    InputProvided {
        /// The session ID.
        session_id: SessionId,
    },

    // === Layout Events ===
    /// Layout mode changed.
    LayoutChanged {
        /// The new layout mode.
        mode: LayoutMode,
    },

    /// A session was focused.
    SessionFocused {
        /// The focused session ID.
        id: SessionId,
    },

    // === Task Events (Phase 2 foundation) ===
    /// A new task was created.
    TaskCreated {
        /// The task ID.
        id: TaskId,
    },

    /// A task assignment was proposed, pending user confirmation.
    ///
    /// Only published when `confirm_before_assign` is enabled.
    /// `TaskAssigned` is published only after confirmation.
    TaskProposed {
        /// The task ID.
        task_id: TaskId,
        /// The session ID.
        session_id: SessionId,
    },

    /// A task was assigned to a session.
    TaskAssigned {
        /// The task ID.
        task_id: TaskId,
        /// The session ID.
        session_id: SessionId,
    },

    /// A task was completed.
    TaskCompleted {
        /// The task ID.
        task_id: TaskId,
        /// Whether the task succeeded.
        success: bool,
    },

    /// Task status changed (automatically synced from session status).
    TaskStatusChanged {
        /// The task ID.
        task_id: TaskId,
        /// The previous status.
        old: TaskStatus,
        /// The new status.
        new: TaskStatus,
        /// Optional reason for the change.
        reason: Option<String>,
    },

    // === File Tree Events ===
    /// Path was dragged to a session.
    PathDraggedToSession {
        /// The session ID.
        session_id: SessionId,
        /// The path that was dragged.
        path: PathBuf,
    },

    // === Context Tracking Events ===
    /// Context usage was updated for a session.
    ContextUsageUpdated {
        /// The session ID.
        session_id: SessionId,
        /// Raw context usage percentage (0.0-1.0).
        percentage: f32,
        /// Effective context usage after MCP overhead (0.0-1.0).
        effective_percentage: f32,
    },

    /// Context threshold was reached (warning or critical).
    ContextThresholdReached {
        /// The session ID.
        session_id: SessionId,
        /// The threshold value that was reached (0.0-1.0).
        threshold: f32,
        /// The threshold state (Warning or Critical).
        state: ContextThresholdState,
    },

    // === Compaction Events ===
    /// Auto-compaction was started for a task-board session.
    ///
    /// Fired when `/compact` is sent to a session before verification.
    CompactionStarted {
        /// The session ID being compacted.
        session_id: SessionId,
        /// The focus instructions sent with the compact command, if any.
        focus: Option<String>,
    },

    /// Auto-compaction completed for a task-board session.
    ///
    /// Fired when the session returns to Idle after compaction,
    /// or on timeout/error.
    CompactionCompleted {
        /// The session ID that was compacted.
        session_id: SessionId,
        /// Whether compaction completed successfully.
        success: bool,
    },

    // === Skill Events ===
    /// A skill was enabled.
    SkillEnabled {
        /// Name of the enabled skill.
        name: String,
    },

    /// A skill was disabled.
    SkillDisabled {
        /// Name of the disabled skill.
        name: String,
    },

    /// Token budget warning threshold reached.
    TokenBudgetWarning {
        /// Current budget state.
        budget: TokenBudget,
    },

    /// Token budget exceeded.
    TokenBudgetExceeded {
        /// Current budget state.
        budget: TokenBudget,
    },

    /// A skill preset was applied.
    SkillPresetApplied {
        /// Name of the applied preset.
        preset_name: String,
        /// Number of skills enabled.
        enabled_count: usize,
    },

    /// Skills were refreshed from disk.
    SkillsRefreshed {
        /// Total number of skills found.
        count: usize,
    },

    // === Ralph Loop Events ===
    /// A Ralph Loop was started for a session.
    RalphLoopStarted {
        /// The session ID.
        session_id: SessionId,
        /// The configuration for the loop.
        config: crate::ralph::RalphLoopConfig,
    },

    /// A Ralph Loop was paused.
    RalphLoopPaused {
        /// The session ID.
        session_id: SessionId,
    },

    /// A Ralph Loop was resumed.
    RalphLoopResumed {
        /// The session ID.
        session_id: SessionId,
    },

    /// A Ralph Loop was cancelled.
    RalphLoopCancelled {
        /// The session ID.
        session_id: SessionId,
    },

    /// A Ralph Loop completed an iteration.
    RalphLoopIteration {
        /// The session ID.
        session_id: SessionId,
        /// The iteration number.
        iteration: u32,
        /// Whether verification passed.
        passed: bool,
        /// Number of test failures (if applicable).
        test_failures: Option<u32>,
    },

    /// A Ralph Loop completed successfully.
    RalphLoopCompleted {
        /// The session ID.
        session_id: SessionId,
        /// Total number of iterations executed.
        total_iterations: u32,
    },

    /// A Ralph Loop failed.
    RalphLoopFailed {
        /// The session ID.
        session_id: SessionId,
        /// Reason for failure.
        reason: String,
        /// Number of iterations executed.
        iterations: u32,
    },

    /// A Ralph Loop detected a stuck state.
    RalphLoopStuck {
        /// The session ID.
        session_id: SessionId,
        /// Number of iterations without progress.
        iterations_without_progress: u32,
    },

    /// Context compaction was triggered for a Ralph Loop.
    RalphLoopCompacted {
        /// The session ID.
        session_id: SessionId,
        /// The iteration at which compaction was triggered.
        iteration: u32,
    },

    // === Working Directory Events ===
    /// Session working directory changed (detected via OSC 7).
    WorkingDirectoryChanged {
        /// The session ID.
        id: SessionId,
        /// The previous working directory.
        old_dir: PathBuf,
        /// The new working directory.
        new_dir: PathBuf,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_created_event() {
        let event = CodirigentEvent::SessionCreated { id: SessionId(1) };
        assert!(matches!(event, CodirigentEvent::SessionCreated { .. }));
    }

    #[test]
    fn test_session_closed_event() {
        let event = CodirigentEvent::SessionClosed { id: SessionId(42) };
        let CodirigentEvent::SessionClosed { id } = event else {
            panic!("Expected SessionClosed, got {event:?}");
        };
        assert_eq!(id, SessionId(42));
    }

    #[test]
    fn test_session_status_changed_event() {
        let event = CodirigentEvent::SessionStatusChanged {
            id: SessionId(1),
            old: SessionStatus::Idle,
            new: SessionStatus::Working,
        };
        let CodirigentEvent::SessionStatusChanged { old, new, .. } = event else {
            panic!("Expected SessionStatusChanged, got {event:?}");
        };
        assert_eq!(old, SessionStatus::Idle);
        assert_eq!(new, SessionStatus::Working);
    }

    #[test]
    fn test_session_output_received_event() {
        let data = vec![72, 101, 108, 108, 111]; // "Hello"
        let event = CodirigentEvent::SessionOutputReceived {
            id: SessionId(1),
            data: data.clone(),
        };
        let CodirigentEvent::SessionOutputReceived {
            id,
            data: received_data,
        } = event
        else {
            panic!("Expected SessionOutputReceived, got {event:?}");
        };
        assert_eq!(id, SessionId(1));
        assert_eq!(received_data, data);
    }

    #[test]
    fn test_session_renamed_event() {
        let event = CodirigentEvent::SessionRenamed {
            id: SessionId(1),
            old_name: "old".to_string(),
            new_name: "new".to_string(),
        };
        let CodirigentEvent::SessionRenamed {
            id,
            old_name,
            new_name,
        } = event
        else {
            panic!("Expected SessionRenamed, got {event:?}");
        };
        assert_eq!(id, SessionId(1));
        assert_eq!(old_name, "old");
        assert_eq!(new_name, "new");
    }

    #[test]
    fn test_session_group_changed_event() {
        let event = CodirigentEvent::SessionGroupChanged {
            id: SessionId(1),
            group: Some("backend".to_string()),
            color: Some("#FF0000".to_string()),
        };
        let CodirigentEvent::SessionGroupChanged { id, group, color } = event else {
            panic!("Expected SessionGroupChanged, got {event:?}");
        };
        assert_eq!(id, SessionId(1));
        assert_eq!(group, Some("backend".to_string()));
        assert_eq!(color, Some("#FF0000".to_string()));
    }

    #[test]
    fn test_attention_required_event() {
        let event = CodirigentEvent::AttentionRequired {
            session_id: SessionId(1),
            detail: Some("y/n".to_string()),
        };
        let CodirigentEvent::AttentionRequired { session_id, detail } = event else {
            panic!("Expected AttentionRequired, got {event:?}");
        };
        assert_eq!(session_id, SessionId(1));
        assert_eq!(detail, Some("y/n".to_string()));
    }

    #[test]
    fn test_attention_required_event_no_detail() {
        let event = CodirigentEvent::AttentionRequired {
            session_id: SessionId(1),
            detail: None,
        };
        let CodirigentEvent::AttentionRequired { detail, .. } = event else {
            panic!("Expected AttentionRequired, got {event:?}");
        };
        assert!(detail.is_none());
    }

    #[test]
    fn test_input_provided_event() {
        let event = CodirigentEvent::InputProvided {
            session_id: SessionId(1),
        };
        assert!(matches!(event, CodirigentEvent::InputProvided { .. }));
    }

    #[test]
    fn test_layout_changed_event() {
        let event = CodirigentEvent::LayoutChanged {
            mode: LayoutMode::Grid { rows: 2, cols: 3 },
        };
        let CodirigentEvent::LayoutChanged { mode } = event else {
            panic!("Expected LayoutChanged, got {event:?}");
        };
        assert!(matches!(mode, LayoutMode::Grid { rows: 2, cols: 3 }));
    }

    #[test]
    fn test_session_focused_event() {
        let event = CodirigentEvent::SessionFocused { id: SessionId(1) };
        assert!(matches!(event, CodirigentEvent::SessionFocused { .. }));
    }

    #[test]
    fn test_task_created_event() {
        let event = CodirigentEvent::TaskCreated {
            id: TaskId::from("task-001"),
        };
        let CodirigentEvent::TaskCreated { id } = event else {
            panic!("Expected TaskCreated, got {event:?}");
        };
        assert_eq!(id, TaskId::from("task-001"));
    }

    #[test]
    fn test_task_assigned_event() {
        let event = CodirigentEvent::TaskAssigned {
            task_id: TaskId::from("task-001"),
            session_id: SessionId(1),
        };
        let CodirigentEvent::TaskAssigned {
            task_id,
            session_id,
        } = event
        else {
            panic!("Expected TaskAssigned, got {event:?}");
        };
        assert_eq!(task_id, TaskId::from("task-001"));
        assert_eq!(session_id, SessionId(1));
    }

    #[test]
    fn test_task_completed_event_success() {
        let event = CodirigentEvent::TaskCompleted {
            task_id: TaskId::from("task-001"),
            success: true,
        };
        let CodirigentEvent::TaskCompleted { task_id, success } = event else {
            panic!("Expected TaskCompleted, got {event:?}");
        };
        assert_eq!(task_id, TaskId::from("task-001"));
        assert!(success);
    }

    #[test]
    fn test_task_completed_event_failure() {
        let event = CodirigentEvent::TaskCompleted {
            task_id: TaskId::from("task-001"),
            success: false,
        };
        let CodirigentEvent::TaskCompleted { success, .. } = event else {
            panic!("Expected TaskCompleted, got {event:?}");
        };
        assert!(!success);
    }

    #[test]
    fn test_task_status_changed_event() {
        let event = CodirigentEvent::TaskStatusChanged {
            task_id: TaskId::from("task-001"),
            old: TaskStatus::Assigned,
            new: TaskStatus::Working,
            reason: Some("Session started working".to_string()),
        };
        let CodirigentEvent::TaskStatusChanged {
            task_id,
            old,
            new,
            reason,
        } = event
        else {
            panic!("Expected TaskStatusChanged, got {event:?}");
        };
        assert_eq!(task_id, TaskId::from("task-001"));
        assert_eq!(old, TaskStatus::Assigned);
        assert_eq!(new, TaskStatus::Working);
        assert_eq!(reason, Some("Session started working".to_string()));
    }

    #[test]
    fn test_path_dragged_to_session_event() {
        let event = CodirigentEvent::PathDraggedToSession {
            session_id: SessionId(1),
            path: PathBuf::from("/home/user/file.txt"),
        };
        let CodirigentEvent::PathDraggedToSession { session_id, path } = event else {
            panic!("Expected PathDraggedToSession, got {event:?}");
        };
        assert_eq!(session_id, SessionId(1));
        assert_eq!(path, PathBuf::from("/home/user/file.txt"));
    }

    #[test]
    fn test_event_clone() {
        let event = CodirigentEvent::SessionFocused { id: SessionId(1) };
        let cloned = event.clone();
        assert!(matches!(cloned, CodirigentEvent::SessionFocused { .. }));
    }

    #[test]
    fn test_event_debug() {
        let event = CodirigentEvent::SessionCreated { id: SessionId(42) };
        let debug_str = format!("{:?}", event);
        assert!(debug_str.contains("SessionCreated"));
        assert!(debug_str.contains("42"));
    }

    #[test]
    fn test_context_usage_updated_event() {
        let event = CodirigentEvent::ContextUsageUpdated {
            session_id: SessionId(1),
            percentage: 0.5,
            effective_percentage: 0.625,
        };
        let CodirigentEvent::ContextUsageUpdated {
            session_id,
            percentage,
            effective_percentage,
        } = event
        else {
            panic!("Expected ContextUsageUpdated, got {event:?}");
        };
        assert_eq!(session_id, SessionId(1));
        assert!((percentage - 0.5).abs() < f32::EPSILON);
        assert!((effective_percentage - 0.625).abs() < f32::EPSILON);
    }

    #[test]
    fn test_context_threshold_reached_event_warning() {
        let event = CodirigentEvent::ContextThresholdReached {
            session_id: SessionId(1),
            threshold: 0.7,
            state: ContextThresholdState::Warning,
        };
        let CodirigentEvent::ContextThresholdReached {
            session_id,
            threshold,
            state,
        } = event
        else {
            panic!("Expected ContextThresholdReached, got {event:?}");
        };
        assert_eq!(session_id, SessionId(1));
        assert!((threshold - 0.7).abs() < f32::EPSILON);
        assert_eq!(state, ContextThresholdState::Warning);
    }

    #[test]
    fn test_context_threshold_reached_event_critical() {
        let event = CodirigentEvent::ContextThresholdReached {
            session_id: SessionId(1),
            threshold: 0.9,
            state: ContextThresholdState::Critical,
        };
        let CodirigentEvent::ContextThresholdReached {
            session_id,
            threshold,
            state,
        } = event
        else {
            panic!("Expected ContextThresholdReached, got {event:?}");
        };
        assert_eq!(session_id, SessionId(1));
        assert!((threshold - 0.9).abs() < f32::EPSILON);
        assert_eq!(state, ContextThresholdState::Critical);
    }

    #[test]
    fn test_skill_enabled_event() {
        let event = CodirigentEvent::SkillEnabled {
            name: "commit".to_string(),
        };
        let CodirigentEvent::SkillEnabled { name } = event else {
            panic!("Expected SkillEnabled, got {event:?}");
        };
        assert_eq!(name, "commit");
    }

    #[test]
    fn test_skill_disabled_event() {
        let event = CodirigentEvent::SkillDisabled {
            name: "commit".to_string(),
        };
        let CodirigentEvent::SkillDisabled { name } = event else {
            panic!("Expected SkillDisabled, got {event:?}");
        };
        assert_eq!(name, "commit");
    }

    #[test]
    fn test_token_budget_warning_event() {
        let budget = TokenBudget {
            max_tokens: 15000,
            used_tokens: 12500,
            warning_threshold: 12000,
        };
        let event = CodirigentEvent::TokenBudgetWarning { budget };
        let CodirigentEvent::TokenBudgetWarning { budget } = event else {
            panic!("Expected TokenBudgetWarning, got {event:?}");
        };
        assert_eq!(budget.used_tokens, 12500);
        assert!(budget.used_tokens >= budget.warning_threshold);
    }

    #[test]
    fn test_token_budget_exceeded_event() {
        let budget = TokenBudget {
            max_tokens: 15000,
            used_tokens: 16000,
            warning_threshold: 12000,
        };
        let event = CodirigentEvent::TokenBudgetExceeded { budget };
        let CodirigentEvent::TokenBudgetExceeded { budget } = event else {
            panic!("Expected TokenBudgetExceeded, got {event:?}");
        };
        assert_eq!(budget.used_tokens, 16000);
        assert!(budget.used_tokens > budget.max_tokens);
    }

    #[test]
    fn test_skill_preset_applied_event() {
        let event = CodirigentEvent::SkillPresetApplied {
            preset_name: "dev".to_string(),
            enabled_count: 5,
        };
        let CodirigentEvent::SkillPresetApplied {
            preset_name,
            enabled_count,
        } = event
        else {
            panic!("Expected SkillPresetApplied, got {event:?}");
        };
        assert_eq!(preset_name, "dev");
        assert_eq!(enabled_count, 5);
    }

    #[test]
    fn test_skills_refreshed_event() {
        let event = CodirigentEvent::SkillsRefreshed { count: 10 };
        let CodirigentEvent::SkillsRefreshed { count } = event else {
            panic!("Expected SkillsRefreshed, got {event:?}");
        };
        assert_eq!(count, 10);
    }

    // === Ralph Loop Event Tests ===

    #[test]
    fn test_ralph_loop_started_event() {
        let event = CodirigentEvent::RalphLoopStarted {
            session_id: SessionId(1),
            config: crate::ralph::RalphLoopConfig::default(),
        };
        let CodirigentEvent::RalphLoopStarted { session_id, config } = event else {
            panic!("Expected RalphLoopStarted, got {event:?}");
        };
        assert_eq!(session_id, SessionId(1));
        assert_eq!(config.max_iterations, 20); // Default is 20
    }

    #[test]
    fn test_ralph_loop_paused_event() {
        let event = CodirigentEvent::RalphLoopPaused {
            session_id: SessionId(1),
        };
        let CodirigentEvent::RalphLoopPaused { session_id } = event else {
            panic!("Expected RalphLoopPaused, got {event:?}");
        };
        assert_eq!(session_id, SessionId(1));
    }

    #[test]
    fn test_ralph_loop_resumed_event() {
        let event = CodirigentEvent::RalphLoopResumed {
            session_id: SessionId(1),
        };
        let CodirigentEvent::RalphLoopResumed { session_id } = event else {
            panic!("Expected RalphLoopResumed, got {event:?}");
        };
        assert_eq!(session_id, SessionId(1));
    }

    #[test]
    fn test_ralph_loop_cancelled_event() {
        let event = CodirigentEvent::RalphLoopCancelled {
            session_id: SessionId(1),
        };
        let CodirigentEvent::RalphLoopCancelled { session_id } = event else {
            panic!("Expected RalphLoopCancelled, got {event:?}");
        };
        assert_eq!(session_id, SessionId(1));
    }

    #[test]
    fn test_ralph_loop_iteration_event() {
        let event = CodirigentEvent::RalphLoopIteration {
            session_id: SessionId(1),
            iteration: 3,
            passed: true,
            test_failures: Some(0),
        };
        let CodirigentEvent::RalphLoopIteration {
            session_id,
            iteration,
            passed,
            test_failures,
        } = event
        else {
            panic!("Expected RalphLoopIteration, got {event:?}");
        };
        assert_eq!(session_id, SessionId(1));
        assert_eq!(iteration, 3);
        assert!(passed);
        assert_eq!(test_failures, Some(0));
    }

    #[test]
    fn test_ralph_loop_completed_event() {
        let event = CodirigentEvent::RalphLoopCompleted {
            session_id: SessionId(1),
            total_iterations: 5,
        };
        let CodirigentEvent::RalphLoopCompleted {
            session_id,
            total_iterations,
        } = event
        else {
            panic!("Expected RalphLoopCompleted, got {event:?}");
        };
        assert_eq!(session_id, SessionId(1));
        assert_eq!(total_iterations, 5);
    }

    #[test]
    fn test_ralph_loop_failed_event() {
        let event = CodirigentEvent::RalphLoopFailed {
            session_id: SessionId(1),
            reason: "Max iterations reached".to_string(),
            iterations: 10,
        };
        let CodirigentEvent::RalphLoopFailed {
            session_id,
            reason,
            iterations,
        } = event
        else {
            panic!("Expected RalphLoopFailed, got {event:?}");
        };
        assert_eq!(session_id, SessionId(1));
        assert_eq!(reason, "Max iterations reached");
        assert_eq!(iterations, 10);
    }

    #[test]
    fn test_ralph_loop_stuck_event() {
        let event = CodirigentEvent::RalphLoopStuck {
            session_id: SessionId(1),
            iterations_without_progress: 3,
        };
        let CodirigentEvent::RalphLoopStuck {
            session_id,
            iterations_without_progress,
        } = event
        else {
            panic!("Expected RalphLoopStuck, got {event:?}");
        };
        assert_eq!(session_id, SessionId(1));
        assert_eq!(iterations_without_progress, 3);
    }

    #[test]
    fn test_ralph_loop_compacted_event() {
        let event = CodirigentEvent::RalphLoopCompacted {
            session_id: SessionId(1),
            iteration: 7,
        };
        let CodirigentEvent::RalphLoopCompacted {
            session_id,
            iteration,
        } = event
        else {
            panic!("Expected RalphLoopCompacted, got {event:?}");
        };
        assert_eq!(session_id, SessionId(1));
        assert_eq!(iteration, 7);
    }

    #[test]
    fn test_all_event_variants_clone() {
        // Test that all variants can be cloned
        let events: Vec<CodirigentEvent> = vec![
            CodirigentEvent::SessionCreated { id: SessionId(1) },
            CodirigentEvent::SessionClosed { id: SessionId(1) },
            CodirigentEvent::SessionStatusChanged {
                id: SessionId(1),
                old: SessionStatus::Idle,
                new: SessionStatus::Working,
            },
            CodirigentEvent::SessionOutputReceived {
                id: SessionId(1),
                data: vec![1, 2, 3],
            },
            CodirigentEvent::SessionRenamed {
                id: SessionId(1),
                old_name: "old".to_string(),
                new_name: "new".to_string(),
            },
            CodirigentEvent::SessionGroupChanged {
                id: SessionId(1),
                group: None,
                color: None,
            },
            CodirigentEvent::AttentionRequired {
                session_id: SessionId(1),
                detail: None,
            },
            CodirigentEvent::InputProvided {
                session_id: SessionId(1),
            },
            CodirigentEvent::LayoutChanged {
                mode: LayoutMode::Single,
            },
            CodirigentEvent::SessionFocused { id: SessionId(1) },
            CodirigentEvent::TaskCreated {
                id: TaskId::from("t"),
            },
            CodirigentEvent::TaskAssigned {
                task_id: TaskId::from("t"),
                session_id: SessionId(1),
            },
            CodirigentEvent::TaskCompleted {
                task_id: TaskId::from("t"),
                success: true,
            },
            CodirigentEvent::TaskStatusChanged {
                task_id: TaskId::from("t"),
                old: TaskStatus::Assigned,
                new: TaskStatus::Working,
                reason: None,
            },
            CodirigentEvent::PathDraggedToSession {
                session_id: SessionId(1),
                path: PathBuf::from("/tmp"),
            },
            CodirigentEvent::ContextUsageUpdated {
                session_id: SessionId(1),
                percentage: 0.5,
                effective_percentage: 0.625,
            },
            CodirigentEvent::ContextThresholdReached {
                session_id: SessionId(1),
                threshold: 0.7,
                state: ContextThresholdState::Warning,
            },
            // Compaction events
            CodirigentEvent::CompactionStarted {
                session_id: SessionId(1),
                focus: Some("test focus".to_string()),
            },
            CodirigentEvent::CompactionStarted {
                session_id: SessionId(2),
                focus: None,
            },
            CodirigentEvent::CompactionCompleted {
                session_id: SessionId(1),
                success: true,
            },
            CodirigentEvent::CompactionCompleted {
                session_id: SessionId(2),
                success: false,
            },
            CodirigentEvent::SkillEnabled {
                name: "commit".to_string(),
            },
            CodirigentEvent::SkillDisabled {
                name: "commit".to_string(),
            },
            CodirigentEvent::TokenBudgetWarning {
                budget: TokenBudget {
                    max_tokens: 15000,
                    used_tokens: 12500,
                    warning_threshold: 12000,
                },
            },
            CodirigentEvent::TokenBudgetExceeded {
                budget: TokenBudget {
                    max_tokens: 15000,
                    used_tokens: 16000,
                    warning_threshold: 12000,
                },
            },
            CodirigentEvent::SkillPresetApplied {
                preset_name: "dev".to_string(),
                enabled_count: 5,
            },
            CodirigentEvent::SkillsRefreshed { count: 10 },
            // Ralph Loop events
            CodirigentEvent::RalphLoopStarted {
                session_id: SessionId(1),
                config: crate::ralph::RalphLoopConfig::default(),
            },
            CodirigentEvent::RalphLoopPaused {
                session_id: SessionId(1),
            },
            CodirigentEvent::RalphLoopResumed {
                session_id: SessionId(1),
            },
            CodirigentEvent::RalphLoopCancelled {
                session_id: SessionId(1),
            },
            CodirigentEvent::RalphLoopIteration {
                session_id: SessionId(1),
                iteration: 1,
                passed: true,
                test_failures: Some(0),
            },
            CodirigentEvent::RalphLoopCompleted {
                session_id: SessionId(1),
                total_iterations: 5,
            },
            CodirigentEvent::RalphLoopFailed {
                session_id: SessionId(1),
                reason: "Max iterations reached".to_string(),
                iterations: 10,
            },
            CodirigentEvent::RalphLoopStuck {
                session_id: SessionId(1),
                iterations_without_progress: 3,
            },
            CodirigentEvent::RalphLoopCompacted {
                session_id: SessionId(1),
                iteration: 7,
            },
            // Working directory events
            CodirigentEvent::WorkingDirectoryChanged {
                id: SessionId(1),
                old_dir: PathBuf::from("/old/dir"),
                new_dir: PathBuf::from("/new/dir"),
            },
        ];

        for event in events {
            let _ = event.clone();
        }
    }

    // === Compaction Event Tests ===

    #[test]
    fn test_compaction_started_event_with_focus() {
        let event = CodirigentEvent::CompactionStarted {
            session_id: SessionId(1),
            focus: Some("Focus on implementation".to_string()),
        };
        let CodirigentEvent::CompactionStarted { session_id, focus } = event else {
            panic!("Expected CompactionStarted, got {event:?}");
        };
        assert_eq!(session_id, SessionId(1));
        assert_eq!(focus, Some("Focus on implementation".to_string()));
    }

    #[test]
    fn test_compaction_started_event_no_focus() {
        let event = CodirigentEvent::CompactionStarted {
            session_id: SessionId(2),
            focus: None,
        };
        let CodirigentEvent::CompactionStarted { session_id, focus } = event else {
            panic!("Expected CompactionStarted, got {event:?}");
        };
        assert_eq!(session_id, SessionId(2));
        assert!(focus.is_none());
    }

    #[test]
    fn test_compaction_completed_event_success() {
        let event = CodirigentEvent::CompactionCompleted {
            session_id: SessionId(1),
            success: true,
        };
        let CodirigentEvent::CompactionCompleted {
            session_id,
            success,
        } = event
        else {
            panic!("Expected CompactionCompleted, got {event:?}");
        };
        assert_eq!(session_id, SessionId(1));
        assert!(success);
    }

    #[test]
    fn test_compaction_completed_event_failure() {
        let event = CodirigentEvent::CompactionCompleted {
            session_id: SessionId(1),
            success: false,
        };
        let CodirigentEvent::CompactionCompleted {
            session_id,
            success,
        } = event
        else {
            panic!("Expected CompactionCompleted, got {event:?}");
        };
        assert_eq!(session_id, SessionId(1));
        assert!(!success);
    }

    // === Clipboard Event Tests ===

    #[test]
    fn test_clipboard_content_type_variants() {
        // Test all variants
        let text = ClipboardContentType::Text;
        let image = ClipboardContentType::Image;
        let files = ClipboardContentType::Files;
        let empty = ClipboardContentType::Empty;

        // Test equality
        assert_eq!(text, ClipboardContentType::Text);
        assert_eq!(image, ClipboardContentType::Image);
        assert_eq!(files, ClipboardContentType::Files);
        assert_eq!(empty, ClipboardContentType::Empty);

        // Test inequality
        assert_ne!(text, image);
        assert_ne!(image, files);
        assert_ne!(files, empty);

        // Test debug
        assert!(format!("{:?}", text).contains("Text"));
        assert!(format!("{:?}", image).contains("Image"));
        assert!(format!("{:?}", files).contains("Files"));
        assert!(format!("{:?}", empty).contains("Empty"));

        // Test copy
        let text_copy = text;
        assert_eq!(text_copy, ClipboardContentType::Text);

        // Test clone
        let text_cloned = text;
        assert_eq!(text_cloned, ClipboardContentType::Text);
    }
}
