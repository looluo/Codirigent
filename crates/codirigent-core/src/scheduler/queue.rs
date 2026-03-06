//! Task queue struct and core operations.
//!
//! Contains the [`TaskQueue`] struct and its methods for enqueueing,
//! dequeueing, assigning, completing, and requeuing tasks.

use crate::events::CodirigentEvent;
use crate::traits::EventBus;
use crate::types::*;
use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::sync::Arc;

use super::config::{SchedulerConfig, SchedulerMode};
use super::selection::priority_to_value;

/// Task queue manager handles task ordering and scheduling.
///
/// The `TaskQueue` manages a collection of tasks, maintaining their order
/// based on the configured scheduling mode. It tracks dependencies between
/// tasks and publishes events when tasks are created, assigned, or completed.
///
/// # Thread Safety
///
/// `TaskQueue` is not thread-safe by itself. For concurrent access,
/// wrap it in an `Arc<Mutex<TaskQueue>>` or similar synchronization primitive.
///
/// # Example
///
/// ```
/// use codirigent_core::{
///     TaskQueue, SchedulerConfig, Task, TaskId, TaskPriority,
///     DefaultEventBus,
/// };
/// use codirigent_core::traits::EventBus;
/// use std::sync::Arc;
///
/// let event_bus = Arc::new(DefaultEventBus::new(16));
/// let mut queue = TaskQueue::new(SchedulerConfig::default(), event_bus);
///
/// // Create and enqueue tasks with different priorities
/// let mut high_task = Task::new(
///     TaskId::from("high"),
///     "High priority".to_string(),
///     "".to_string(),
/// );
/// high_task.priority = TaskPriority::High;
///
/// let low_task = Task::new(
///     TaskId::from("low"),
///     "Low priority".to_string(),
///     "".to_string(),
/// );
///
/// queue.enqueue(low_task).unwrap();
/// queue.enqueue(high_task).unwrap();
///
/// // High priority task should be selected first
/// let next = queue.next_task(&[]).unwrap();
/// assert_eq!(next.id, TaskId::from("high"));
/// ```
pub struct TaskQueue {
    /// Queue state (order, blocked tasks).
    state: QueueState,

    /// All tasks indexed by ID.
    tasks: HashMap<TaskId, Task>,

    /// Scheduler configuration.
    config: SchedulerConfig,

    /// Event bus for publishing events.
    event_bus: Arc<dyn EventBus>,
}

impl TaskQueue {
    /// Create a new task queue with the given configuration.
    ///
    /// # Arguments
    ///
    /// * `config` - Scheduler configuration
    /// * `event_bus` - Event bus for publishing task events
    ///
    /// # Example
    ///
    /// ```
    /// use codirigent_core::{TaskQueue, SchedulerConfig, DefaultEventBus};
    /// use std::sync::Arc;
    ///
    /// let event_bus = Arc::new(DefaultEventBus::new(16));
    /// let queue = TaskQueue::new(SchedulerConfig::default(), event_bus);
    /// ```
    pub fn new(config: SchedulerConfig, event_bus: Arc<dyn EventBus>) -> Self {
        Self {
            state: QueueState::default(),
            tasks: HashMap::new(),
            config,
            event_bus,
        }
    }

    /// Load queue state from storage.
    ///
    /// This should be called after creating a new `TaskQueue` to restore
    /// previously persisted state.
    ///
    /// # Arguments
    ///
    /// * `state` - The queue state to restore
    /// * `tasks` - All tasks to load into the queue
    ///
    /// # Example
    ///
    /// ```
    /// use codirigent_core::{TaskQueue, SchedulerConfig, QueueState, Task, DefaultEventBus};
    /// use std::sync::Arc;
    ///
    /// let event_bus = Arc::new(DefaultEventBus::new(16));
    /// let mut queue = TaskQueue::new(SchedulerConfig::default(), event_bus);
    ///
    /// // Load previously saved state
    /// let state = QueueState::default();
    /// let tasks: Vec<Task> = vec![];
    /// queue.load_state(state, tasks);
    /// ```
    pub fn load_state(&mut self, state: QueueState, tasks: Vec<Task>) {
        self.state = state;
        self.tasks = tasks.into_iter().map(|t| (t.id.clone(), t)).collect();
    }

    /// Get current queue state for persistence.
    ///
    /// Returns a reference to the internal queue state which can be
    /// serialized and saved to disk.
    ///
    /// # Example
    ///
    /// ```
    /// use codirigent_core::{TaskQueue, SchedulerConfig, DefaultEventBus};
    /// use std::sync::Arc;
    ///
    /// let event_bus = Arc::new(DefaultEventBus::new(16));
    /// let queue = TaskQueue::new(SchedulerConfig::default(), event_bus);
    /// let state = queue.get_state();
    /// assert!(state.order.is_empty());
    /// ```
    pub fn get_state(&self) -> &QueueState {
        &self.state
    }

    /// Get internal state reference (used by selection module).
    pub(crate) fn state(&self) -> &QueueState {
        &self.state
    }

    /// Get internal tasks map reference (used by selection module).
    pub(crate) fn tasks(&self) -> &HashMap<TaskId, Task> {
        &self.tasks
    }

    /// Get the scheduler configuration.
    ///
    /// # Example
    ///
    /// ```
    /// use codirigent_core::{TaskQueue, SchedulerConfig, SchedulerMode, DefaultEventBus};
    /// use std::sync::Arc;
    ///
    /// let event_bus = Arc::new(DefaultEventBus::new(16));
    /// let config = SchedulerConfig {
    ///     mode: SchedulerMode::Fifo,
    ///     ..Default::default()
    /// };
    /// let queue = TaskQueue::new(config, event_bus);
    /// assert_eq!(queue.config().mode, SchedulerMode::Fifo);
    /// ```
    pub fn config(&self) -> &SchedulerConfig {
        &self.config
    }

    /// Get a task by ID.
    ///
    /// # Arguments
    ///
    /// * `id` - The task ID to look up
    ///
    /// # Returns
    ///
    /// The task if found, `None` otherwise.
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
    /// assert!(queue.get_task(&TaskId::from("test")).is_some());
    /// assert!(queue.get_task(&TaskId::from("nonexistent")).is_none());
    /// ```
    pub fn get_task(&self, id: &TaskId) -> Option<&Task> {
        self.tasks.get(id)
    }

    /// Get a mutable reference to a task by ID.
    ///
    /// Used for in-place status updates (e.g., moving to review).
    pub fn get_task_mut(&mut self, id: &TaskId) -> Option<&mut Task> {
        self.tasks.get_mut(id)
    }

    /// Add a task to the queue.
    ///
    /// The task is inserted into the queue order based on the current
    /// scheduling mode. If the task has dependencies, it may be marked
    /// as blocked until those dependencies are satisfied.
    ///
    /// # Arguments
    ///
    /// * `task` - The task to add
    ///
    /// # Errors
    ///
    /// Returns an error if a task with the same ID already exists.
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
    /// let task = Task::new(
    ///     TaskId::from("task-001"),
    ///     "Test Task".to_string(),
    ///     "Description".to_string(),
    /// );
    ///
    /// queue.enqueue(task).unwrap();
    /// assert_eq!(queue.get_state().order.len(), 1);
    /// ```
    pub fn enqueue(&mut self, task: Task) -> Result<()> {
        let id = task.id.clone();

        // Check for duplicate
        if self.tasks.contains_key(&id) {
            return Err(anyhow!("Task {} already exists", id));
        }

        // Add to tasks map
        self.tasks.insert(id.clone(), task);

        // Add to queue order based on mode
        self.insert_by_priority(&id);

        // Update blocked status
        self.update_blocked_for_task(&id);

        // Update timestamp
        self.state.updated_at = Some(chrono::Utc::now());

        // Publish event
        self.event_bus.publish(CodirigentEvent::TaskCreated { id });

        Ok(())
    }

    /// Remove a task from the queue.
    ///
    /// Removes the task from both the order list and the tasks map.
    ///
    /// # Arguments
    ///
    /// * `id` - The ID of the task to remove
    ///
    /// # Returns
    ///
    /// The removed task if found, `None` otherwise.
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
    /// let removed = queue.dequeue(&TaskId::from("test"));
    /// assert!(removed.is_some());
    /// assert!(queue.get_state().order.is_empty());
    /// ```
    pub fn dequeue(&mut self, id: &TaskId) -> Option<Task> {
        // Remove from order
        self.state.order.retain(|t| t != id);

        // Remove from blocked
        self.state.blocked.remove(id);

        // Update timestamp
        self.state.updated_at = Some(chrono::Utc::now());

        // Remove and return task
        self.tasks.remove(id)
    }

    /// Mark a task as assigned to a session.
    ///
    /// Updates the task status to `Assigned`, records the session ID,
    /// removes it from the queue order, and publishes a `TaskAssigned` event.
    ///
    /// # Arguments
    ///
    /// * `task_id` - The ID of the task to assign
    /// * `session_id` - The session to assign the task to
    ///
    /// # Errors
    ///
    /// Returns an error if the task doesn't exist or is not in `Queued` status.
    ///
    /// # Example
    ///
    /// ```
    /// use codirigent_core::{
    ///     TaskQueue, SchedulerConfig, Task, TaskId, TaskStatus, SessionId,
    ///     DefaultEventBus,
    /// };
    /// use std::sync::Arc;
    ///
    /// let event_bus = Arc::new(DefaultEventBus::new(16));
    /// let mut queue = TaskQueue::new(SchedulerConfig::default(), event_bus);
    ///
    /// let task = Task::new(TaskId::from("test"), "Test".to_string(), "".to_string());
    /// queue.enqueue(task).unwrap();
    ///
    /// queue.assign_task(&TaskId::from("test"), SessionId(1)).unwrap();
    ///
    /// let task = queue.get_task(&TaskId::from("test")).unwrap();
    /// assert_eq!(task.status, TaskStatus::Assigned);
    /// assert_eq!(task.assigned_session, Some(SessionId(1)));
    /// ```
    pub fn assign_task(&mut self, task_id: &TaskId, session_id: SessionId) -> Result<()> {
        let task = self
            .tasks
            .get_mut(task_id)
            .ok_or_else(|| anyhow!("Task {} not found", task_id))?;

        if task.status != TaskStatus::Queued {
            return Err(anyhow!(
                "Task {} is not queued (status: {:?})",
                task_id,
                task.status
            ));
        }

        task.status = TaskStatus::Assigned;
        task.assigned_session = Some(session_id);
        task.assigned_at = Some(chrono::Utc::now());

        // Remove from queue order
        self.state.order.retain(|id| id != task_id);

        // Update timestamp
        self.state.updated_at = Some(chrono::Utc::now());

        // Publish event
        self.event_bus.publish(CodirigentEvent::TaskAssigned {
            task_id: task_id.clone(),
            session_id,
        });

        Ok(())
    }

    /// Mark a task as completed.
    ///
    /// Updates the task status to `Done`, records the completion time,
    /// updates blocked status for dependent tasks, and publishes a
    /// `TaskCompleted` event.
    ///
    /// # Arguments
    ///
    /// * `task_id` - The ID of the task to complete
    /// * `success` - Whether the task completed successfully
    ///
    /// # Errors
    ///
    /// Returns an error if the task doesn't exist.
    ///
    /// # Example
    ///
    /// ```
    /// use codirigent_core::{
    ///     TaskQueue, SchedulerConfig, Task, TaskId, TaskStatus, SessionId,
    ///     DefaultEventBus,
    /// };
    /// use std::sync::Arc;
    ///
    /// let event_bus = Arc::new(DefaultEventBus::new(16));
    /// let mut queue = TaskQueue::new(SchedulerConfig::default(), event_bus);
    ///
    /// let task = Task::new(TaskId::from("test"), "Test".to_string(), "".to_string());
    /// queue.enqueue(task).unwrap();
    /// queue.assign_task(&TaskId::from("test"), SessionId(1)).unwrap();
    ///
    /// queue.complete_task(&TaskId::from("test"), true).unwrap();
    ///
    /// let task = queue.get_task(&TaskId::from("test")).unwrap();
    /// assert_eq!(task.status, TaskStatus::Done);
    /// assert!(task.completed_at.is_some());
    /// ```
    pub fn complete_task(&mut self, task_id: &TaskId, success: bool) -> Result<()> {
        let task = self
            .tasks
            .get_mut(task_id)
            .ok_or_else(|| anyhow!("Task {} not found", task_id))?;

        match task.status {
            TaskStatus::Assigned
            | TaskStatus::Working
            | TaskStatus::Verifying
            | TaskStatus::Review => {}
            other => {
                return Err(anyhow!(
                    "Task {} cannot be completed from {:?} state",
                    task_id,
                    other
                ));
            }
        }

        task.status = TaskStatus::Done;
        task.completed_at = Some(chrono::Utc::now());

        // Update blocked status for tasks depending on this one
        self.update_blocked_status_after_completion(task_id);

        // Update timestamp
        self.state.updated_at = Some(chrono::Utc::now());

        // Publish event
        self.event_bus.publish(CodirigentEvent::TaskCompleted {
            task_id: task_id.clone(),
            success,
        });

        Ok(())
    }

    /// Move a task back to queue (for retry).
    ///
    /// Resets the task status to `Queued`, clears assignment info,
    /// increments the retry count, and re-inserts it into the queue.
    ///
    /// # Arguments
    ///
    /// * `task_id` - The ID of the task to requeue
    ///
    /// # Errors
    ///
    /// Returns an error if the task doesn't exist or has exceeded max retries.
    ///
    /// # Example
    ///
    /// ```
    /// use codirigent_core::{
    ///     TaskQueue, SchedulerConfig, Task, TaskId, TaskStatus, SessionId,
    ///     DefaultEventBus,
    /// };
    /// use std::sync::Arc;
    ///
    /// let event_bus = Arc::new(DefaultEventBus::new(16));
    /// let mut queue = TaskQueue::new(SchedulerConfig::default(), event_bus);
    ///
    /// let task = Task::new(TaskId::from("test"), "Test".to_string(), "".to_string());
    /// queue.enqueue(task).unwrap();
    /// queue.assign_task(&TaskId::from("test"), SessionId(1)).unwrap();
    ///
    /// queue.requeue_task(&TaskId::from("test")).unwrap();
    ///
    /// let task = queue.get_task(&TaskId::from("test")).unwrap();
    /// assert_eq!(task.status, TaskStatus::Queued);
    /// assert_eq!(task.retry.retry_count, 1);
    /// ```
    pub fn requeue_task(&mut self, task_id: &TaskId) -> Result<()> {
        let task = self
            .tasks
            .get_mut(task_id)
            .ok_or_else(|| anyhow!("Task {} not found", task_id))?;

        if !task.can_retry() {
            return Err(anyhow!("Task {} has exceeded max retries", task_id));
        }

        task.increment_retry();
        task.status = TaskStatus::Queued;
        task.assigned_session = None;
        task.assigned_at = None;

        // Re-add to queue
        self.insert_by_priority(task_id);

        // Update timestamp
        self.state.updated_at = Some(chrono::Utc::now());

        Ok(())
    }

    /// Update blocked status for all tasks.
    ///
    /// Recalculates which tasks are blocked based on their dependencies
    /// and the provided list of completed tasks.
    ///
    /// # Arguments
    ///
    /// * `completed_tasks` - List of task IDs that have been completed
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
    /// let task1 = Task::new(TaskId::from("first"), "First".to_string(), "".to_string());
    /// let mut task2 = Task::new(TaskId::from("second"), "Second".to_string(), "".to_string());
    /// task2.dependencies = vec![TaskId::from("first")];
    ///
    /// queue.enqueue(task1).unwrap();
    /// queue.enqueue(task2).unwrap();
    ///
    /// // task2 is initially blocked
    /// assert!(queue.blocked_tasks().iter().any(|t| t.id == TaskId::from("second")));
    ///
    /// // After marking task1 as complete, update blocked status
    /// queue.update_blocked_status(&[TaskId::from("first")]);
    /// ```
    pub fn update_blocked_status(&mut self, completed_tasks: &[TaskId]) {
        let task_ids: Vec<TaskId> = self.tasks.keys().cloned().collect();
        for id in task_ids {
            self.update_blocked_for_task(&id);
        }

        // Also check against provided completed list
        for blocking in self.state.blocked.values_mut() {
            blocking.retain(|b| !completed_tasks.contains(b));
        }

        // Remove entries with no blockers
        self.state.blocked.retain(|_, v| !v.is_empty());

        // Update timestamp
        self.state.updated_at = Some(chrono::Utc::now());
    }

    /// Get all queued tasks in order.
    ///
    /// Returns tasks that are in `Queued` status, ordered according
    /// to the queue's current ordering.
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
    /// let queued = queue.queued_tasks();
    /// assert_eq!(queued.len(), 1);
    /// ```
    pub fn queued_tasks(&self) -> Vec<&Task> {
        self.state
            .order
            .iter()
            .filter_map(|id| self.tasks.get(id))
            .filter(|t| t.status == TaskStatus::Queued)
            .collect()
    }

    /// Get all blocked tasks.
    ///
    /// Returns tasks that have unmet dependencies and cannot be assigned.
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
    /// let mut task = Task::new(TaskId::from("test"), "Test".to_string(), "".to_string());
    /// task.dependencies = vec![TaskId::from("nonexistent")];
    /// queue.enqueue(task).unwrap();
    ///
    /// // Note: Only tasks with existing dependencies in the queue are marked blocked
    /// let blocked = queue.blocked_tasks();
    /// ```
    pub fn blocked_tasks(&self) -> Vec<&Task> {
        self.state
            .blocked
            .keys()
            .filter_map(|id| self.tasks.get(id))
            .collect()
    }

    /// Get all tasks (both queued and non-queued).
    ///
    /// Returns a reference to all tasks managed by this queue.
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
    /// assert_eq!(queue.all_tasks().len(), 1);
    /// ```
    pub fn all_tasks(&self) -> &HashMap<TaskId, Task> {
        &self.tasks
    }

    /// Check if a task is blocked.
    ///
    /// # Arguments
    ///
    /// * `id` - The task ID to check
    ///
    /// # Returns
    ///
    /// `true` if the task has unmet dependencies, `false` otherwise.
    pub fn is_blocked(&self, id: &TaskId) -> bool {
        self.state.blocked.contains_key(id)
    }

    /// Insert a task into the queue order based on priority.
    pub(crate) fn insert_by_priority(&mut self, id: &TaskId) {
        let task = match self.tasks.get(id) {
            Some(t) => t,
            None => return,
        };

        match self.config.mode {
            SchedulerMode::Fifo => {
                self.state.order.push(id.clone());
            }
            SchedulerMode::Priority | SchedulerMode::Smart => {
                // Find insertion point based on priority
                let priority_value = priority_to_value(&task.priority);
                let pos = self.state.order.iter().position(|other_id| {
                    self.tasks
                        .get(other_id)
                        .map(|t| priority_to_value(&t.priority) < priority_value)
                        .unwrap_or(false)
                });

                match pos {
                    Some(p) => self.state.order.insert(p, id.clone()),
                    None => self.state.order.push(id.clone()),
                }
            }
            SchedulerMode::Dependency => {
                self.state.order.push(id.clone());
            }
        }
    }

    /// Update blocked status for a specific task.
    pub(crate) fn update_blocked_for_task(&mut self, id: &TaskId) {
        let task = match self.tasks.get(id) {
            Some(t) => t,
            None => return,
        };

        // Find incomplete dependencies (only those that exist in the queue)
        let blocking: Vec<TaskId> = task
            .dependencies
            .iter()
            .filter(|dep_id| {
                self.tasks
                    .get(*dep_id)
                    .map(|t| t.status != TaskStatus::Done)
                    .unwrap_or(false)
            })
            .cloned()
            .collect();

        if blocking.is_empty() {
            self.state.blocked.remove(id);
        } else {
            self.state.blocked.insert(id.clone(), blocking);
        }
    }

    /// Update blocked status after a task completion.
    fn update_blocked_status_after_completion(&mut self, completed_id: &TaskId) {
        // Remove completed task from all blocking lists
        for blocking in self.state.blocked.values_mut() {
            blocking.retain(|id| id != completed_id);
        }

        // Remove entries with no blockers
        self.state.blocked.retain(|_, v| !v.is_empty());
    }
}

impl std::fmt::Debug for TaskQueue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TaskQueue")
            .field("task_count", &self.tasks.len())
            .field("queue_order_len", &self.state.order.len())
            .field("blocked_count", &self.state.blocked.len())
            .field("config", &self.config)
            .finish()
    }
}

/// Trait for task queue operations.
///
/// This trait provides a common interface for task queue operations,
/// allowing for different implementations and easier testing.
pub trait TaskQueueService: Send + Sync {
    /// Add a task to the queue.
    fn enqueue(&mut self, task: Task) -> Result<()>;

    /// Get next task to assign.
    fn next_task(&self, completed_tasks: &[TaskId]) -> Option<&Task>;

    /// Assign a task to a session.
    fn assign(&mut self, task_id: &TaskId, session_id: SessionId) -> Result<()>;

    /// Complete a task.
    fn complete(&mut self, task_id: &TaskId, success: bool) -> Result<()>;

    /// Requeue a task for retry.
    fn requeue(&mut self, task_id: &TaskId) -> Result<()>;

    /// Get queue state.
    fn state(&self) -> &QueueState;
}

impl TaskQueueService for TaskQueue {
    fn enqueue(&mut self, task: Task) -> Result<()> {
        TaskQueue::enqueue(self, task)
    }

    fn next_task(&self, completed_tasks: &[TaskId]) -> Option<&Task> {
        TaskQueue::next_task(self, completed_tasks)
    }

    fn assign(&mut self, task_id: &TaskId, session_id: SessionId) -> Result<()> {
        self.assign_task(task_id, session_id)
    }

    fn complete(&mut self, task_id: &TaskId, success: bool) -> Result<()> {
        self.complete_task(task_id, success)
    }

    fn requeue(&mut self, task_id: &TaskId) -> Result<()> {
        self.requeue_task(task_id)
    }

    fn state(&self) -> &QueueState {
        self.get_state()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_bus::DefaultEventBus;
    use crate::scheduler::config::SchedulerConfig;

    fn create_queue() -> TaskQueue {
        let event_bus = Arc::new(DefaultEventBus::new(16));
        TaskQueue::new(SchedulerConfig::default(), event_bus)
    }

    // ========== TaskQueue Basic Tests ==========

    #[test]
    fn test_task_queue_new() {
        let queue = create_queue();
        assert!(queue.get_state().order.is_empty());
        assert!(queue.all_tasks().is_empty());
    }

    #[test]
    fn test_task_queue_config() {
        let event_bus = Arc::new(DefaultEventBus::new(16));
        let config = SchedulerConfig {
            mode: SchedulerMode::Fifo,
            ..Default::default()
        };
        let queue = TaskQueue::new(config.clone(), event_bus);
        assert_eq!(queue.config().mode, SchedulerMode::Fifo);
    }

    #[test]
    fn test_task_queue_load_state() {
        let mut queue = create_queue();
        let mut state = QueueState::default();
        state.order.push(TaskId::from("task-001"));

        let task = Task::new(TaskId::from("task-001"), "Test".to_string(), "".to_string());

        queue.load_state(state, vec![task]);

        assert_eq!(queue.get_state().order.len(), 1);
        assert!(queue.get_task(&TaskId::from("task-001")).is_some());
    }

    #[test]
    fn test_task_queue_debug() {
        let queue = create_queue();
        let debug_str = format!("{:?}", queue);
        assert!(debug_str.contains("TaskQueue"));
        assert!(debug_str.contains("task_count"));
    }

    // ========== Enqueue Tests ==========

    #[test]
    fn test_enqueue_task() {
        let mut queue = create_queue();
        let task = Task::new(TaskId::from("task-001"), "Test".to_string(), "".to_string());

        queue.enqueue(task).unwrap();

        assert_eq!(queue.get_state().order.len(), 1);
        assert!(queue.get_task(&TaskId::from("task-001")).is_some());
    }

    #[test]
    fn test_enqueue_duplicate_fails() {
        let mut queue = create_queue();
        let task1 = Task::new(TaskId::from("task-001"), "Test".to_string(), "".to_string());
        let task2 = Task::new(
            TaskId::from("task-001"),
            "Duplicate".to_string(),
            "".to_string(),
        );

        queue.enqueue(task1).unwrap();
        let result = queue.enqueue(task2);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));
    }

    #[test]
    fn test_enqueue_updates_timestamp() {
        let mut queue = create_queue();
        assert!(queue.get_state().updated_at.is_none());

        let task = Task::new(TaskId::from("task-001"), "Test".to_string(), "".to_string());
        queue.enqueue(task).unwrap();

        assert!(queue.get_state().updated_at.is_some());
    }

    // ========== Dequeue Tests ==========

    #[test]
    fn test_dequeue_task() {
        let mut queue = create_queue();
        let task = Task::new(TaskId::from("task-001"), "Test".to_string(), "".to_string());

        queue.enqueue(task).unwrap();
        let removed = queue.dequeue(&TaskId::from("task-001"));

        assert!(removed.is_some());
        assert_eq!(removed.unwrap().id, TaskId::from("task-001"));
        assert!(queue.get_state().order.is_empty());
        assert!(queue.get_task(&TaskId::from("task-001")).is_none());
    }

    #[test]
    fn test_dequeue_nonexistent() {
        let mut queue = create_queue();
        let removed = queue.dequeue(&TaskId::from("nonexistent"));
        assert!(removed.is_none());
    }

    #[test]
    fn test_dequeue_removes_from_blocked() {
        let mut queue = create_queue();

        let task1 = Task::new(
            TaskId::from("task-001"),
            "First".to_string(),
            "".to_string(),
        );
        let mut task2 = Task::new(
            TaskId::from("task-002"),
            "Second".to_string(),
            "".to_string(),
        );
        task2.dependencies = vec![TaskId::from("task-001")];

        queue.enqueue(task1).unwrap();
        queue.enqueue(task2).unwrap();

        // task-002 should be blocked
        assert!(queue.is_blocked(&TaskId::from("task-002")));

        // Dequeue task-002 should remove it from blocked
        queue.dequeue(&TaskId::from("task-002"));
        assert!(!queue.is_blocked(&TaskId::from("task-002")));
    }

    // ========== Assign Task Tests ==========

    #[test]
    fn test_assign_task() {
        let mut queue = create_queue();
        let task = Task::new(TaskId::from("task-001"), "Test".to_string(), "".to_string());
        queue.enqueue(task).unwrap();

        queue
            .assign_task(&TaskId::from("task-001"), SessionId(1))
            .unwrap();

        let task = queue.get_task(&TaskId::from("task-001")).unwrap();
        assert_eq!(task.status, TaskStatus::Assigned);
        assert_eq!(task.assigned_session, Some(SessionId(1)));
        assert!(task.assigned_at.is_some());
    }

    #[test]
    fn test_assign_task_removes_from_order() {
        let mut queue = create_queue();
        let task = Task::new(TaskId::from("task-001"), "Test".to_string(), "".to_string());
        queue.enqueue(task).unwrap();

        assert_eq!(queue.get_state().order.len(), 1);

        queue
            .assign_task(&TaskId::from("task-001"), SessionId(1))
            .unwrap();

        assert!(queue.get_state().order.is_empty());
    }

    #[test]
    fn test_assign_task_not_found() {
        let mut queue = create_queue();
        let result = queue.assign_task(&TaskId::from("nonexistent"), SessionId(1));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_assign_task_not_queued() {
        let mut queue = create_queue();
        let task = Task::new(TaskId::from("task-001"), "Test".to_string(), "".to_string());
        queue.enqueue(task).unwrap();

        // Assign once
        queue
            .assign_task(&TaskId::from("task-001"), SessionId(1))
            .unwrap();

        // Try to assign again
        let result = queue.assign_task(&TaskId::from("task-001"), SessionId(2));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not queued"));
    }

    // ========== Complete Task Tests ==========

    #[test]
    fn test_complete_task() {
        let mut queue = create_queue();
        let task = Task::new(TaskId::from("task-001"), "Test".to_string(), "".to_string());
        queue.enqueue(task).unwrap();
        queue
            .assign_task(&TaskId::from("task-001"), SessionId(1))
            .unwrap();

        queue
            .complete_task(&TaskId::from("task-001"), true)
            .unwrap();

        let task = queue.get_task(&TaskId::from("task-001")).unwrap();
        assert_eq!(task.status, TaskStatus::Done);
        assert!(task.completed_at.is_some());
    }

    #[test]
    fn test_complete_task_unblocks_dependents() {
        let mut queue = create_queue();

        let task1 = Task::new(TaskId::from("task-1"), "First".to_string(), "".to_string());

        let mut task2 = Task::new(TaskId::from("task-2"), "Second".to_string(), "".to_string());
        task2.dependencies = vec![TaskId::from("task-1")];

        queue.enqueue(task1).unwrap();
        queue.enqueue(task2).unwrap();

        // task-2 should be blocked
        assert!(queue.is_blocked(&TaskId::from("task-2")));

        // Complete task-1
        queue
            .assign_task(&TaskId::from("task-1"), SessionId(1))
            .unwrap();
        queue.complete_task(&TaskId::from("task-1"), true).unwrap();

        // task-2 should no longer be blocked
        assert!(!queue.is_blocked(&TaskId::from("task-2")));
    }

    #[test]
    fn test_complete_task_not_found() {
        let mut queue = create_queue();
        let result = queue.complete_task(&TaskId::from("nonexistent"), true);
        assert!(result.is_err());
    }

    // ========== Requeue Task Tests ==========

    #[test]
    fn test_requeue_task() {
        let mut queue = create_queue();
        let task = Task::new(TaskId::from("task-001"), "Test".to_string(), "".to_string());
        queue.enqueue(task).unwrap();
        queue
            .assign_task(&TaskId::from("task-001"), SessionId(1))
            .unwrap();

        queue.requeue_task(&TaskId::from("task-001")).unwrap();

        let task = queue.get_task(&TaskId::from("task-001")).unwrap();
        assert_eq!(task.status, TaskStatus::Queued);
        assert_eq!(task.retry.retry_count, 1);
        assert!(task.assigned_session.is_none());
        assert!(task.assigned_at.is_none());
    }

    #[test]
    fn test_requeue_task_max_retries() {
        let mut queue = create_queue();
        let mut task = Task::new(TaskId::from("task-001"), "Test".to_string(), "".to_string());
        task.retry.retry_count = 3; // Already at max
        queue.enqueue(task).unwrap();
        queue
            .assign_task(&TaskId::from("task-001"), SessionId(1))
            .unwrap();

        let result = queue.requeue_task(&TaskId::from("task-001"));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("max retries"));
    }

    #[test]
    fn test_requeue_task_not_found() {
        let mut queue = create_queue();
        let result = queue.requeue_task(&TaskId::from("nonexistent"));
        assert!(result.is_err());
    }

    // ========== Update Blocked Status Tests ==========

    #[test]
    fn test_update_blocked_status() {
        let mut queue = create_queue();

        let task1 = Task::new(TaskId::from("task-1"), "First".to_string(), "".to_string());

        let mut task2 = Task::new(TaskId::from("task-2"), "Second".to_string(), "".to_string());
        task2.dependencies = vec![TaskId::from("task-1")];

        queue.enqueue(task1).unwrap();
        queue.enqueue(task2).unwrap();

        // Initially blocked
        assert!(queue.is_blocked(&TaskId::from("task-2")));

        // Update with completed task
        queue.update_blocked_status(&[TaskId::from("task-1")]);

        // No longer blocked
        assert!(!queue.is_blocked(&TaskId::from("task-2")));
    }

    // ========== Queued Tasks Tests ==========

    #[test]
    fn test_queued_tasks() {
        let mut queue = create_queue();

        let task1 = Task::new(TaskId::from("task-1"), "First".to_string(), "".to_string());
        let task2 = Task::new(TaskId::from("task-2"), "Second".to_string(), "".to_string());

        queue.enqueue(task1).unwrap();
        queue.enqueue(task2).unwrap();

        let queued = queue.queued_tasks();
        assert_eq!(queued.len(), 2);

        // Assign one
        queue
            .assign_task(&TaskId::from("task-1"), SessionId(1))
            .unwrap();

        let queued = queue.queued_tasks();
        assert_eq!(queued.len(), 1);
        assert_eq!(queued[0].id, TaskId::from("task-2"));
    }

    // ========== Blocked Tasks Tests ==========

    #[test]
    fn test_blocked_tasks() {
        let mut queue = create_queue();

        let task1 = Task::new(TaskId::from("task-1"), "First".to_string(), "".to_string());

        let mut task2 = Task::new(TaskId::from("task-2"), "Second".to_string(), "".to_string());
        task2.dependencies = vec![TaskId::from("task-1")];

        queue.enqueue(task1).unwrap();
        queue.enqueue(task2).unwrap();

        let blocked = queue.blocked_tasks();
        assert_eq!(blocked.len(), 1);
        assert_eq!(blocked[0].id, TaskId::from("task-2"));
    }

    // ========== All Tasks Tests ==========

    #[test]
    fn test_all_tasks() {
        let mut queue = create_queue();

        let task1 = Task::new(TaskId::from("task-1"), "First".to_string(), "".to_string());
        let task2 = Task::new(TaskId::from("task-2"), "Second".to_string(), "".to_string());

        queue.enqueue(task1).unwrap();
        queue.enqueue(task2).unwrap();

        let all = queue.all_tasks();
        assert_eq!(all.len(), 2);
        assert!(all.contains_key(&TaskId::from("task-1")));
        assert!(all.contains_key(&TaskId::from("task-2")));
    }

    // ========== Edge Cases Tests ==========

    #[test]
    fn test_multiple_dependencies() {
        let mut queue = create_queue();

        let task1 = Task::new(TaskId::from("task-1"), "First".to_string(), "".to_string());
        let task2 = Task::new(TaskId::from("task-2"), "Second".to_string(), "".to_string());
        let mut task3 = Task::new(TaskId::from("task-3"), "Third".to_string(), "".to_string());
        task3.dependencies = vec![TaskId::from("task-1"), TaskId::from("task-2")];

        queue.enqueue(task1).unwrap();
        queue.enqueue(task2).unwrap();
        queue.enqueue(task3).unwrap();

        // task-3 should be blocked
        assert!(queue.is_blocked(&TaskId::from("task-3")));

        // Complete task-1
        queue
            .assign_task(&TaskId::from("task-1"), SessionId(1))
            .unwrap();
        queue.complete_task(&TaskId::from("task-1"), true).unwrap();

        // task-3 should still be blocked (waiting for task-2)
        assert!(queue.is_blocked(&TaskId::from("task-3")));

        // Complete task-2
        queue
            .assign_task(&TaskId::from("task-2"), SessionId(1))
            .unwrap();
        queue.complete_task(&TaskId::from("task-2"), true).unwrap();

        // task-3 should no longer be blocked
        assert!(!queue.is_blocked(&TaskId::from("task-3")));
    }

    // ========== TaskQueueService Trait Tests ==========

    #[test]
    fn test_task_queue_service_enqueue() {
        let mut queue = create_queue();
        let task = Task::new(TaskId::from("task-001"), "Test".to_string(), "".to_string());

        TaskQueueService::enqueue(&mut queue, task).unwrap();
        assert_eq!(TaskQueueService::state(&queue).order.len(), 1);
    }

    #[test]
    fn test_task_queue_service_next_task() {
        let mut queue = create_queue();
        let task = Task::new(TaskId::from("task-001"), "Test".to_string(), "".to_string());
        queue.enqueue(task).unwrap();

        let next = TaskQueueService::next_task(&queue, &[]);
        assert!(next.is_some());
    }

    #[test]
    fn test_task_queue_service_assign() {
        let mut queue = create_queue();
        let task = Task::new(TaskId::from("task-001"), "Test".to_string(), "".to_string());
        queue.enqueue(task).unwrap();

        TaskQueueService::assign(&mut queue, &TaskId::from("task-001"), SessionId(1)).unwrap();

        let task = queue.get_task(&TaskId::from("task-001")).unwrap();
        assert_eq!(task.status, TaskStatus::Assigned);
    }

    #[test]
    fn test_task_queue_service_complete() {
        let mut queue = create_queue();
        let task = Task::new(TaskId::from("task-001"), "Test".to_string(), "".to_string());
        queue.enqueue(task).unwrap();
        queue
            .assign_task(&TaskId::from("task-001"), SessionId(1))
            .unwrap();

        TaskQueueService::complete(&mut queue, &TaskId::from("task-001"), true).unwrap();

        let task = queue.get_task(&TaskId::from("task-001")).unwrap();
        assert_eq!(task.status, TaskStatus::Done);
    }

    #[test]
    fn test_task_queue_service_requeue() {
        let mut queue = create_queue();
        let task = Task::new(TaskId::from("task-001"), "Test".to_string(), "".to_string());
        queue.enqueue(task).unwrap();
        queue
            .assign_task(&TaskId::from("task-001"), SessionId(1))
            .unwrap();

        TaskQueueService::requeue(&mut queue, &TaskId::from("task-001")).unwrap();

        let task = queue.get_task(&TaskId::from("task-001")).unwrap();
        assert_eq!(task.status, TaskStatus::Queued);
    }
}
