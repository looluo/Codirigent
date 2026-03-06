//! Task selection and scoring algorithms.
//!
//! Contains the scheduling logic for selecting the next task to assign,
//! including priority scoring, age scoring, and tag matching.

use crate::types::*;

use super::config::SchedulerMode;
use super::queue::TaskQueue;

impl TaskQueue {
    /// Get the next task to assign based on scheduling mode.
    ///
    /// Returns the highest-priority unblocked task that is still in
    /// the `Queued` status and has all dependencies satisfied.
    /// Done tasks are determined from the queue's own task map.
    ///
    /// # Returns
    ///
    /// The next task to assign, or `None` if no tasks are available.
    ///
    /// # Example
    ///
    /// ```
    /// use codirigent_core::{TaskQueue, SchedulerConfig, Task, TaskId, DefaultEventBus};
    /// use std::sync::Arc;
    ///
    /// let event_bus = Arc::new(DefaultEventBus::new(16));
    /// let mut queue = TaskQueue::new(SchedulerConfig::default(), event_bus);
    ///
    /// let task = Task::new(TaskId::from("test"), "Test".to_string(), "".to_string());
    /// queue.enqueue(task).unwrap();
    ///
    /// let next = queue.next_task();
    /// assert!(next.is_some());
    /// assert_eq!(next.unwrap().id, TaskId::from("test"));
    /// ```
    pub fn next_task(&self) -> Option<&Task> {
        // Build done set from the queue's own task map to avoid external allocation.
        let done_set: std::collections::HashSet<&TaskId> = self
            .tasks()
            .values()
            .filter(|t| t.status == TaskStatus::Done)
            .map(|t| &t.id)
            .collect();

        let mut eligible = self
            .state()
            .order
            .iter()
            .filter_map(|id| self.tasks().get(id))
            .filter(|task| {
                task.status == TaskStatus::Queued
                    && !self.is_blocked(&task.id)
                    && task.dependencies_satisfied_fast(&done_set)
            });

        if self.config().mode == SchedulerMode::Fifo {
            // FIFO: the order Vec already encodes priority — take the first match in O(n).
            eligible.next()
        } else {
            // Other modes: score all eligible tasks and return the highest-scoring one.
            // Pre-compute scores to avoid calling calculate_score twice per comparison
            // (which in Smart mode would invoke chrono::Utc::now() O(n) extra times).
            eligible
                .map(|task| (task, self.calculate_score(task)))
                .max_by(|(_, sa), (_, sb)| sa.partial_cmp(sb).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(task, _)| task)
        }
    }

    /// Get the next task for a specific session (considers tag matching).
    ///
    /// Similar to `next_task`, but also considers tag matching between
    /// the session's group and task tags for better assignment.
    ///
    /// # Arguments
    ///
    /// * `session` - The session to find a task for
    ///
    /// # Returns
    ///
    /// The best matching task for the session, or `None` if no tasks are available.
    ///
    /// # Example
    ///
    /// ```
    /// use codirigent_core::{
    ///     TaskQueue, SchedulerConfig, Task, TaskId, Session, SessionId,
    ///     DefaultEventBus,
    /// };
    /// use std::sync::Arc;
    /// use std::path::PathBuf;
    ///
    /// let event_bus = Arc::new(DefaultEventBus::new(16));
    /// let mut queue = TaskQueue::new(SchedulerConfig::default(), event_bus);
    ///
    /// let mut task = Task::new(TaskId::from("backend"), "Backend".to_string(), "".to_string());
    /// task.tags = vec!["backend".to_string()];
    /// queue.enqueue(task).unwrap();
    ///
    /// let mut session = Session::new(SessionId(1), "Backend Session".to_string(), PathBuf::from("/tmp"));
    /// session.group = Some("backend".to_string());
    ///
    /// let next = queue.next_task_for_session(&session);
    /// assert!(next.is_some());
    /// ```
    pub fn next_task_for_session(&self, session: &Session) -> Option<&Task> {
        let done_set: std::collections::HashSet<&TaskId> = self
            .tasks()
            .values()
            .filter(|t| t.status == TaskStatus::Done)
            .map(|t| &t.id)
            .collect();

        let mut eligible = self
            .state()
            .order
            .iter()
            .filter_map(|id| self.tasks().get(id))
            .filter(|task| {
                task.status == TaskStatus::Queued
                    && !self.is_blocked(&task.id)
                    && task.dependencies_satisfied_fast(&done_set)
                    && task.project_dir.as_ref().map_or(true, |pd| {
                        super::session_matches_project(&session.working_directory, pd)
                    })
            });

        if self.config().mode == SchedulerMode::Fifo {
            eligible.next()
        } else {
            eligible
                .map(|task| (task, self.calculate_score_for_session(task, session)))
                .max_by(|(_, sa), (_, sb)| sa.partial_cmp(sb).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(task, _)| task)
        }
    }

    /// Calculate priority score for a task.
    pub(crate) fn calculate_score(&self, task: &Task) -> f32 {
        match self.config().mode {
            SchedulerMode::Fifo => {
                // Earlier position = higher score
                let pos = self.state().order.iter().position(|id| id == &task.id);
                pos.map(|p| 1.0 / (p as f32 + 1.0)).unwrap_or(0.0)
            }
            SchedulerMode::Priority => priority_to_value(&task.priority) as f32,
            SchedulerMode::Dependency => {
                // Fewer dependencies = higher score
                1.0 / (task.dependencies.len() as f32 + 1.0)
            }
            SchedulerMode::Smart => {
                let priority_score = priority_to_value(&task.priority) as f32 / 4.0;
                let age_score = self.calculate_age_score(task);
                self.config().priority_weight * priority_score
                    + self.config().age_weight * age_score
            }
        }
    }

    /// Calculate score for a task considering session tag matching.
    pub(crate) fn calculate_score_for_session(&self, task: &Task, session: &Session) -> f32 {
        let base_score = self.calculate_score(task);

        // Add tag matching bonus
        let tag_score = if let Some(ref group) = session.group {
            if task.tags.iter().any(|t| t == group) {
                1.0
            } else {
                0.0
            }
        } else {
            0.0
        };

        base_score + self.config().tag_match_weight * tag_score
    }

    /// Calculate age score based on how long the task has been waiting.
    ///
    /// Uses a logarithmic curve so older tasks asymptotically approach 1.0
    /// without a hard cap. A task waiting 60 minutes scores ≈ 0.5; tasks
    /// waiting longer continue to differentiate rather than collapsing to the
    /// same score.
    pub(crate) fn calculate_age_score(&self, task: &Task) -> f32 {
        let age_minutes = chrono::Utc::now()
            .signed_duration_since(task.created_at)
            .num_minutes()
            .max(0) as f32;
        // Asymptotic: score → 1.0 as age → ∞, equals 0.5 at 60 minutes.
        age_minutes / (age_minutes + 60.0)
    }
}

/// Convert task priority to numeric value for comparison.
pub(crate) fn priority_to_value(priority: &TaskPriority) -> u8 {
    match priority {
        TaskPriority::Critical => 4,
        TaskPriority::High => 3,
        TaskPriority::Medium => 2,
        TaskPriority::Low => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_bus::DefaultEventBus;
    use crate::scheduler::config::{SchedulerConfig, SchedulerMode};
    use std::path::PathBuf;
    use std::sync::Arc;

    fn create_queue() -> TaskQueue {
        let event_bus = Arc::new(DefaultEventBus::new(16));
        TaskQueue::new(SchedulerConfig::default(), event_bus)
    }

    fn create_queue_with_mode(mode: SchedulerMode) -> TaskQueue {
        let event_bus = Arc::new(DefaultEventBus::new(16));
        let config = SchedulerConfig {
            mode,
            ..Default::default()
        };
        TaskQueue::new(config, event_bus)
    }

    // ========== Next Task Tests ==========

    #[test]
    fn test_next_task_empty_queue() {
        let queue = create_queue();
        assert!(queue.next_task().is_none());
    }

    #[test]
    fn test_next_task_single() {
        let mut queue = create_queue();
        let task = Task::new(TaskId::from("task-001"), "Test".to_string(), "".to_string());
        queue.enqueue(task).unwrap();

        let next = queue.next_task();
        assert!(next.is_some());
        assert_eq!(next.unwrap().id, TaskId::from("task-001"));
    }

    #[test]
    fn test_next_task_priority_order() {
        let mut queue = create_queue();

        let mut low_task = Task::new(TaskId::from("low"), "Low".to_string(), "".to_string());
        low_task.priority = TaskPriority::Low;

        let mut high_task = Task::new(TaskId::from("high"), "High".to_string(), "".to_string());
        high_task.priority = TaskPriority::High;

        queue.enqueue(low_task).unwrap();
        queue.enqueue(high_task).unwrap();

        let next = queue.next_task();
        assert_eq!(next.unwrap().id, TaskId::from("high"));
    }

    #[test]
    fn test_next_task_critical_priority() {
        let mut queue = create_queue();

        let mut high_task = Task::new(TaskId::from("high"), "High".to_string(), "".to_string());
        high_task.priority = TaskPriority::High;

        let mut critical_task = Task::new(
            TaskId::from("critical"),
            "Critical".to_string(),
            "".to_string(),
        );
        critical_task.priority = TaskPriority::Critical;

        queue.enqueue(high_task).unwrap();
        queue.enqueue(critical_task).unwrap();

        let next = queue.next_task();
        assert_eq!(next.unwrap().id, TaskId::from("critical"));
    }

    #[test]
    fn test_next_task_respects_dependencies() {
        let mut queue = create_queue();

        let task1 = Task::new(TaskId::from("task-1"), "First".to_string(), "".to_string());

        let mut task2 = Task::new(TaskId::from("task-2"), "Second".to_string(), "".to_string());
        task2.dependencies = vec![TaskId::from("task-1")];

        queue.enqueue(task1).unwrap();
        queue.enqueue(task2).unwrap();

        // task-2 should be blocked, so next should return task-1
        let next = queue.next_task();
        assert_eq!(next.unwrap().id, TaskId::from("task-1"));

        // Assign and complete task-1
        queue
            .assign_task(&TaskId::from("task-1"), SessionId(1))
            .unwrap();
        queue.complete_task(&TaskId::from("task-1"), true).unwrap();

        // After task-1 completes, task-2 is unblocked
        let next = queue.next_task();
        assert_eq!(next.unwrap().id, TaskId::from("task-2"));
    }

    #[test]
    fn test_next_task_skips_assigned() {
        let mut queue = create_queue();

        let task1 = Task::new(TaskId::from("task-1"), "First".to_string(), "".to_string());
        let task2 = Task::new(TaskId::from("task-2"), "Second".to_string(), "".to_string());

        queue.enqueue(task1).unwrap();
        queue.enqueue(task2).unwrap();

        // Assign first task
        queue
            .assign_task(&TaskId::from("task-1"), SessionId(1))
            .unwrap();

        // Next should return second task
        let next = queue.next_task();
        assert_eq!(next.unwrap().id, TaskId::from("task-2"));
    }

    // ========== Next Task for Session Tests ==========

    #[test]
    fn test_next_task_for_session_tag_matching() {
        let mut queue = create_queue();

        let mut backend_task = Task::new(
            TaskId::from("backend"),
            "Backend Task".to_string(),
            "".to_string(),
        );
        backend_task.tags = vec!["backend".to_string()];

        let mut frontend_task = Task::new(
            TaskId::from("frontend"),
            "Frontend Task".to_string(),
            "".to_string(),
        );
        frontend_task.tags = vec!["frontend".to_string()];

        queue.enqueue(backend_task).unwrap();
        queue.enqueue(frontend_task).unwrap();

        // Session with backend group should prefer backend task
        let mut session = Session::new(SessionId(1), "Backend".to_string(), PathBuf::from("/tmp"));
        session.group = Some("backend".to_string());

        let next = queue.next_task_for_session(&session);
        assert_eq!(next.unwrap().id, TaskId::from("backend"));
    }

    #[test]
    fn test_next_task_for_session_no_group() {
        let mut queue = create_queue();

        let task = Task::new(TaskId::from("task-1"), "Task".to_string(), "".to_string());
        queue.enqueue(task).unwrap();

        // Session without group
        let session = Session::new(SessionId(1), "Session".to_string(), PathBuf::from("/tmp"));

        let next = queue.next_task_for_session(&session);
        assert!(next.is_some());
    }

    // ========== FIFO Mode Tests ==========

    #[test]
    fn test_fifo_mode_ordering() {
        let mut queue = create_queue_with_mode(SchedulerMode::Fifo);

        let mut high_task = Task::new(TaskId::from("high"), "High".to_string(), "".to_string());
        high_task.priority = TaskPriority::High;

        let low_task = Task::new(TaskId::from("low"), "Low".to_string(), "".to_string());

        // Add low first, then high
        queue.enqueue(low_task).unwrap();
        queue.enqueue(high_task).unwrap();

        // In FIFO mode, low should come first (despite lower priority)
        let next = queue.next_task();
        assert_eq!(next.unwrap().id, TaskId::from("low"));
    }

    // ========== Dependency Mode Tests ==========

    #[test]
    fn test_dependency_mode_ordering() {
        let mut queue = create_queue_with_mode(SchedulerMode::Dependency);

        let task1 = Task::new(
            TaskId::from("no-deps"),
            "No deps".to_string(),
            "".to_string(),
        );

        let mut task2 = Task::new(
            TaskId::from("has-deps"),
            "Has deps".to_string(),
            "".to_string(),
        );
        task2.dependencies = vec![TaskId::from("external-1"), TaskId::from("external-2")];

        queue.enqueue(task2).unwrap();
        queue.enqueue(task1).unwrap();

        // Task with no deps should score higher in dependency mode
        // (because the dependencies are external and don't block)
        let next = queue.next_task();
        assert!(next.is_some());
    }

    // ========== Priority to Value Tests ==========

    #[test]
    fn test_priority_to_value() {
        assert_eq!(priority_to_value(&TaskPriority::Critical), 4);
        assert_eq!(priority_to_value(&TaskPriority::High), 3);
        assert_eq!(priority_to_value(&TaskPriority::Medium), 2);
        assert_eq!(priority_to_value(&TaskPriority::Low), 1);
    }

    // ========== Project Dir Filter Tests ==========

    #[test]
    fn test_next_task_for_session_project_dir_filter() {
        let mut queue = create_queue();

        let mut task_a = Task::new(TaskId::from("task-a"), "Task A".to_string(), "".to_string());
        task_a.project_dir = Some(PathBuf::from("/project-a"));

        let mut task_b = Task::new(TaskId::from("task-b"), "Task B".to_string(), "".to_string());
        task_b.project_dir = Some(PathBuf::from("/project-b"));

        queue.enqueue(task_a).unwrap();
        queue.enqueue(task_b).unwrap();

        // Session in /project-a should only get task-a
        let session_a = Session::new(
            SessionId(1),
            "Session A".to_string(),
            PathBuf::from("/project-a"),
        );
        let next = queue.next_task_for_session(&session_a);
        assert_eq!(next.unwrap().id, TaskId::from("task-a"));

        // Session in /project-b should only get task-b
        let session_b = Session::new(
            SessionId(2),
            "Session B".to_string(),
            PathBuf::from("/project-b"),
        );
        let next = queue.next_task_for_session(&session_b);
        assert_eq!(next.unwrap().id, TaskId::from("task-b"));
    }

    #[test]
    fn test_next_task_for_session_subdirectory_match() {
        let mut queue = create_queue();

        let mut task = Task::new(TaskId::from("task-1"), "Task".to_string(), "".to_string());
        task.project_dir = Some(PathBuf::from("/project"));

        queue.enqueue(task).unwrap();

        // Session in subdirectory of /project should match
        let session = Session::new(
            SessionId(1),
            "Session".to_string(),
            PathBuf::from("/project/src"),
        );
        let next = queue.next_task_for_session(&session);
        assert!(next.is_some());
    }

    #[test]
    fn test_next_task_for_session_no_project_dir_matches_any() {
        let mut queue = create_queue();

        let task = Task::new(TaskId::from("task-1"), "Task".to_string(), "".to_string());
        // project_dir is None - should match any session

        queue.enqueue(task).unwrap();

        let session = Session::new(
            SessionId(1),
            "Session".to_string(),
            PathBuf::from("/any/directory"),
        );
        let next = queue.next_task_for_session(&session);
        assert!(next.is_some());
    }

    #[test]
    fn test_next_task_for_session_project_dir_no_match_skips() {
        let mut queue = create_queue();

        let mut task = Task::new(TaskId::from("task-1"), "Task".to_string(), "".to_string());
        task.project_dir = Some(PathBuf::from("/project-a"));

        queue.enqueue(task).unwrap();

        // Session in completely different directory should get nothing
        let session = Session::new(
            SessionId(1),
            "Session".to_string(),
            PathBuf::from("/project-b"),
        );
        let next = queue.next_task_for_session(&session);
        assert!(next.is_none());
    }

    #[test]
    fn test_external_dependency_not_blocking() {
        let mut queue = create_queue();

        let mut task = Task::new(TaskId::from("task-1"), "Task".to_string(), "".to_string());
        // Dependency on another task
        task.dependencies = vec![TaskId::from("dep-task")];

        queue.enqueue(task).unwrap();

        // dep-task not in queue, so is_blocked returns false (no blocking entry)
        assert!(!queue.is_blocked(&TaskId::from("task-1")));

        // task-1 not selectable because dep-task not done
        let next = queue.next_task();
        assert!(next.is_none());

        // Enqueue dep-task, assign and complete it so it's Done in the queue
        let dep = Task::new(TaskId::from("dep-task"), "Dep".to_string(), "".to_string());
        queue.enqueue(dep).unwrap();
        queue
            .assign_task(&TaskId::from("dep-task"), SessionId(9))
            .unwrap();
        queue
            .complete_task(&TaskId::from("dep-task"), true)
            .unwrap();

        // Now dep-task is Done in queue — task-1 becomes eligible
        let next = queue.next_task();
        assert!(next.is_some());
        assert_eq!(next.unwrap().id, TaskId::from("task-1"));
    }
}
