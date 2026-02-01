//! Event types for cross-module communication.
//!
//! This module defines the [`DirigentEvent`] enum which is used for
//! loose coupling between modules. All cross-module communication
//! should happen through events, allowing modules to react to changes
//! without tight coupling.

use crate::types::*;
use std::path::PathBuf;

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
///
/// # Example
///
/// ```
/// use dirigent_core::events::DirigentEvent;
/// use dirigent_core::types::{SessionId, SessionStatus};
///
/// let event = DirigentEvent::SessionStatusChanged {
///     id: SessionId(1),
///     old: SessionStatus::Idle,
///     new: SessionStatus::Working,
/// };
/// ```
#[derive(Debug, Clone)]
pub enum DirigentEvent {
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
    /// Input is required from the user (detected pattern).
    InputRequired {
        /// The session ID.
        session_id: SessionId,
        /// The pattern that triggered the detection.
        pattern: Option<String>,
    },

    /// User provided input (pattern resolved).
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

    // === File Tree Events ===
    /// Path was dragged to a session.
    PathDraggedToSession {
        /// The session ID.
        session_id: SessionId,
        /// The path that was dragged.
        path: PathBuf,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_created_event() {
        let event = DirigentEvent::SessionCreated { id: SessionId(1) };
        assert!(matches!(event, DirigentEvent::SessionCreated { .. }));
    }

    #[test]
    fn test_session_closed_event() {
        let event = DirigentEvent::SessionClosed { id: SessionId(42) };
        if let DirigentEvent::SessionClosed { id } = event {
            assert_eq!(id, SessionId(42));
        } else {
            panic!("Wrong event type");
        }
    }

    #[test]
    fn test_session_status_changed_event() {
        let event = DirigentEvent::SessionStatusChanged {
            id: SessionId(1),
            old: SessionStatus::Idle,
            new: SessionStatus::Working,
        };
        if let DirigentEvent::SessionStatusChanged { old, new, .. } = event {
            assert_eq!(old, SessionStatus::Idle);
            assert_eq!(new, SessionStatus::Working);
        } else {
            panic!("Wrong event type");
        }
    }

    #[test]
    fn test_session_output_received_event() {
        let data = vec![72, 101, 108, 108, 111]; // "Hello"
        let event = DirigentEvent::SessionOutputReceived {
            id: SessionId(1),
            data: data.clone(),
        };
        if let DirigentEvent::SessionOutputReceived {
            id,
            data: received_data,
        } = event
        {
            assert_eq!(id, SessionId(1));
            assert_eq!(received_data, data);
        } else {
            panic!("Wrong event type");
        }
    }

    #[test]
    fn test_session_renamed_event() {
        let event = DirigentEvent::SessionRenamed {
            id: SessionId(1),
            old_name: "old".to_string(),
            new_name: "new".to_string(),
        };
        if let DirigentEvent::SessionRenamed {
            id,
            old_name,
            new_name,
        } = event
        {
            assert_eq!(id, SessionId(1));
            assert_eq!(old_name, "old");
            assert_eq!(new_name, "new");
        } else {
            panic!("Wrong event type");
        }
    }

    #[test]
    fn test_session_group_changed_event() {
        let event = DirigentEvent::SessionGroupChanged {
            id: SessionId(1),
            group: Some("backend".to_string()),
            color: Some("#FF0000".to_string()),
        };
        if let DirigentEvent::SessionGroupChanged { id, group, color } = event {
            assert_eq!(id, SessionId(1));
            assert_eq!(group, Some("backend".to_string()));
            assert_eq!(color, Some("#FF0000".to_string()));
        } else {
            panic!("Wrong event type");
        }
    }

    #[test]
    fn test_input_required_event() {
        let event = DirigentEvent::InputRequired {
            session_id: SessionId(1),
            pattern: Some("y/n".to_string()),
        };
        if let DirigentEvent::InputRequired {
            session_id,
            pattern,
        } = event
        {
            assert_eq!(session_id, SessionId(1));
            assert_eq!(pattern, Some("y/n".to_string()));
        } else {
            panic!("Wrong event type");
        }
    }

    #[test]
    fn test_input_required_event_no_pattern() {
        let event = DirigentEvent::InputRequired {
            session_id: SessionId(1),
            pattern: None,
        };
        if let DirigentEvent::InputRequired { pattern, .. } = event {
            assert!(pattern.is_none());
        } else {
            panic!("Wrong event type");
        }
    }

    #[test]
    fn test_input_provided_event() {
        let event = DirigentEvent::InputProvided {
            session_id: SessionId(1),
        };
        assert!(matches!(event, DirigentEvent::InputProvided { .. }));
    }

    #[test]
    fn test_layout_changed_event() {
        let event = DirigentEvent::LayoutChanged {
            mode: LayoutMode::Grid { rows: 2, cols: 3 },
        };
        if let DirigentEvent::LayoutChanged { mode } = event {
            assert!(matches!(mode, LayoutMode::Grid { rows: 2, cols: 3 }));
        } else {
            panic!("Wrong event type");
        }
    }

    #[test]
    fn test_session_focused_event() {
        let event = DirigentEvent::SessionFocused { id: SessionId(1) };
        assert!(matches!(event, DirigentEvent::SessionFocused { .. }));
    }

    #[test]
    fn test_task_created_event() {
        let event = DirigentEvent::TaskCreated {
            id: TaskId("task-001".to_string()),
        };
        if let DirigentEvent::TaskCreated { id } = event {
            assert_eq!(id, TaskId("task-001".to_string()));
        } else {
            panic!("Wrong event type");
        }
    }

    #[test]
    fn test_task_assigned_event() {
        let event = DirigentEvent::TaskAssigned {
            task_id: TaskId("task-001".to_string()),
            session_id: SessionId(1),
        };
        if let DirigentEvent::TaskAssigned {
            task_id,
            session_id,
        } = event
        {
            assert_eq!(task_id, TaskId("task-001".to_string()));
            assert_eq!(session_id, SessionId(1));
        } else {
            panic!("Wrong event type");
        }
    }

    #[test]
    fn test_task_completed_event_success() {
        let event = DirigentEvent::TaskCompleted {
            task_id: TaskId("task-001".to_string()),
            success: true,
        };
        if let DirigentEvent::TaskCompleted { task_id, success } = event {
            assert_eq!(task_id, TaskId("task-001".to_string()));
            assert!(success);
        } else {
            panic!("Wrong event type");
        }
    }

    #[test]
    fn test_task_completed_event_failure() {
        let event = DirigentEvent::TaskCompleted {
            task_id: TaskId("task-001".to_string()),
            success: false,
        };
        if let DirigentEvent::TaskCompleted { success, .. } = event {
            assert!(!success);
        } else {
            panic!("Wrong event type");
        }
    }

    #[test]
    fn test_path_dragged_to_session_event() {
        let event = DirigentEvent::PathDraggedToSession {
            session_id: SessionId(1),
            path: PathBuf::from("/home/user/file.txt"),
        };
        if let DirigentEvent::PathDraggedToSession { session_id, path } = event {
            assert_eq!(session_id, SessionId(1));
            assert_eq!(path, PathBuf::from("/home/user/file.txt"));
        } else {
            panic!("Wrong event type");
        }
    }

    #[test]
    fn test_event_clone() {
        let event = DirigentEvent::SessionFocused { id: SessionId(1) };
        let cloned = event.clone();
        assert!(matches!(cloned, DirigentEvent::SessionFocused { .. }));
    }

    #[test]
    fn test_event_debug() {
        let event = DirigentEvent::SessionCreated { id: SessionId(42) };
        let debug_str = format!("{:?}", event);
        assert!(debug_str.contains("SessionCreated"));
        assert!(debug_str.contains("42"));
    }

    #[test]
    fn test_all_event_variants_clone() {
        // Test that all variants can be cloned
        let events: Vec<DirigentEvent> = vec![
            DirigentEvent::SessionCreated { id: SessionId(1) },
            DirigentEvent::SessionClosed { id: SessionId(1) },
            DirigentEvent::SessionStatusChanged {
                id: SessionId(1),
                old: SessionStatus::Idle,
                new: SessionStatus::Working,
            },
            DirigentEvent::SessionOutputReceived {
                id: SessionId(1),
                data: vec![1, 2, 3],
            },
            DirigentEvent::SessionRenamed {
                id: SessionId(1),
                old_name: "old".to_string(),
                new_name: "new".to_string(),
            },
            DirigentEvent::SessionGroupChanged {
                id: SessionId(1),
                group: None,
                color: None,
            },
            DirigentEvent::InputRequired {
                session_id: SessionId(1),
                pattern: None,
            },
            DirigentEvent::InputProvided {
                session_id: SessionId(1),
            },
            DirigentEvent::LayoutChanged {
                mode: LayoutMode::Single,
            },
            DirigentEvent::SessionFocused { id: SessionId(1) },
            DirigentEvent::TaskCreated {
                id: TaskId("t".to_string()),
            },
            DirigentEvent::TaskAssigned {
                task_id: TaskId("t".to_string()),
                session_id: SessionId(1),
            },
            DirigentEvent::TaskCompleted {
                task_id: TaskId("t".to_string()),
                success: true,
            },
            DirigentEvent::PathDraggedToSession {
                session_id: SessionId(1),
                path: PathBuf::from("/tmp"),
            },
        ];

        for event in events {
            let _ = event.clone();
        }
    }
}
