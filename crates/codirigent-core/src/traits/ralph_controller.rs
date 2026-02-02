//! Ralph Loop controller trait for autonomous task execution.
//!
//! This module defines the [`RalphLoopController`] trait which provides the contract
//! for controlling autonomous execution loops on AI sessions.

use crate::ralph::{IterationResult, RalphLoopConfig, RalphLoopState};
use crate::types::SessionId;
use anyhow::Result;

/// Ralph Loop controller trait.
///
/// Controls autonomous execution loops for sessions, enabling
/// run-test-fix-repeat workflows until success or max iterations.
///
/// The Ralph Loop (named after Geoffrey Huntley's technique) follows a cycle:
/// 1. Execute the next action (code change, test run, etc.)
/// 2. Run verification command
/// 3. If passed, complete; if failed, iterate
/// 4. Repeat until success or max iterations
///
/// # Thread Safety
///
/// All implementations must be `Send + Sync` to allow sharing across threads.
///
/// # Example
///
/// ```ignore
/// use codirigent_core::traits::RalphLoopController;
/// use codirigent_core::ralph::RalphLoopConfig;
/// use codirigent_core::SessionId;
///
/// fn use_controller(controller: &mut dyn RalphLoopController) {
///     let session_id = SessionId(1);
///     let config = RalphLoopConfig::for_rust();
///
///     // Start a loop
///     controller.start(session_id, config).unwrap();
///
///     // Check status
///     if controller.is_active(session_id) {
///         println!("Loop is running");
///     }
///
///     // Pause and resume
///     controller.pause(session_id).unwrap();
///     controller.resume(session_id).unwrap();
///
///     // Stop when done
///     controller.stop(session_id).unwrap();
/// }
/// ```
pub trait RalphLoopController: Send + Sync {
    /// Start a new Ralph Loop for a session.
    ///
    /// Returns error if session already has an active loop.
    ///
    /// # Arguments
    ///
    /// * `session_id` - The session to start a loop for
    /// * `config` - Configuration for the loop
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Session already has an active loop
    /// - Configuration is invalid
    fn start(&mut self, session_id: SessionId, config: RalphLoopConfig) -> Result<()>;

    /// Pause a running loop.
    ///
    /// The loop can be resumed later with `resume()`.
    ///
    /// # Arguments
    ///
    /// * `session_id` - The session whose loop to pause
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - No loop found for session
    /// - Loop is not in Running status
    fn pause(&mut self, session_id: SessionId) -> Result<()>;

    /// Resume a paused loop.
    ///
    /// # Arguments
    ///
    /// * `session_id` - The session whose loop to resume
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - No loop found for session
    /// - Loop is not in Paused status
    fn resume(&mut self, session_id: SessionId) -> Result<()>;

    /// Stop/cancel a loop.
    ///
    /// The loop cannot be resumed after stopping.
    ///
    /// # Arguments
    ///
    /// * `session_id` - The session whose loop to stop
    ///
    /// # Errors
    ///
    /// Returns an error if no loop found for session.
    fn stop(&mut self, session_id: SessionId) -> Result<()>;

    /// Get the current state of a session's loop.
    ///
    /// # Arguments
    ///
    /// * `session_id` - The session to query
    ///
    /// # Returns
    ///
    /// The loop state if one exists, `None` otherwise.
    fn get_state(&self, session_id: SessionId) -> Option<&RalphLoopState>;

    /// Get mutable state for a session's loop.
    ///
    /// # Arguments
    ///
    /// * `session_id` - The session to query
    ///
    /// # Returns
    ///
    /// A mutable reference to the loop state if one exists, `None` otherwise.
    fn get_state_mut(&mut self, session_id: SessionId) -> Option<&mut RalphLoopState>;

    /// Get the configuration for a session's loop.
    ///
    /// # Arguments
    ///
    /// * `session_id` - The session to query
    ///
    /// # Returns
    ///
    /// The loop configuration if one exists, `None` otherwise.
    fn get_config(&self, session_id: SessionId) -> Option<&RalphLoopConfig>;

    /// Check if a session has an active (non-finished) loop.
    ///
    /// A loop is considered active if its status is Running or Paused.
    ///
    /// # Arguments
    ///
    /// * `session_id` - The session to check
    ///
    /// # Returns
    ///
    /// `true` if the session has an active loop, `false` otherwise.
    fn is_active(&self, session_id: SessionId) -> bool {
        self.get_state(session_id)
            .map(|s| !s.status.is_finished())
            .unwrap_or(false)
    }

    /// Get all sessions with active loops.
    ///
    /// Returns a list of session IDs that have loops in Running or Paused status.
    fn active_sessions(&self) -> Vec<SessionId>;

    /// Record an iteration result.
    ///
    /// This should be called after each iteration completes to update
    /// the loop state and potentially transition to Completed or Failed status.
    ///
    /// # Arguments
    ///
    /// * `session_id` - The session to record for
    /// * `result` - The iteration result
    ///
    /// # Errors
    ///
    /// Returns an error if no loop found for session.
    fn record_iteration(&mut self, session_id: SessionId, result: IterationResult) -> Result<()>;

    /// Get total number of managed loops (active and finished).
    ///
    /// This is a convenience method with a default implementation.
    fn loop_count(&self) -> usize {
        self.active_sessions().len()
    }

    /// Check if any loops are stuck.
    ///
    /// Returns a list of sessions whose loops are detected as stuck
    /// (iterations without progress exceeds threshold).
    fn stuck_sessions(&self) -> Vec<SessionId> {
        self.active_sessions()
            .into_iter()
            .filter(|&id| {
                if let (Some(state), Some(config)) = (self.get_state(id), self.get_config(id)) {
                    state.iterations_without_progress >= config.stuck_threshold
                } else {
                    false
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ralph::RalphLoopStatus;
    use std::collections::HashMap;

    // Mock implementation for testing
    struct MockRalphController {
        loops: HashMap<SessionId, (RalphLoopConfig, RalphLoopState)>,
    }

    impl MockRalphController {
        fn new() -> Self {
            Self {
                loops: HashMap::new(),
            }
        }
    }

    impl RalphLoopController for MockRalphController {
        fn start(&mut self, session_id: SessionId, config: RalphLoopConfig) -> Result<()> {
            if self.is_active(session_id) {
                anyhow::bail!("Session already has an active loop");
            }
            let state = RalphLoopState::new();
            self.loops.insert(session_id, (config, state));
            Ok(())
        }

        fn pause(&mut self, session_id: SessionId) -> Result<()> {
            let (_, state) = self
                .loops
                .get_mut(&session_id)
                .ok_or_else(|| anyhow::anyhow!("No loop found for session"))?;

            if state.status != RalphLoopStatus::Running {
                anyhow::bail!("Loop is not running");
            }

            state.status = RalphLoopStatus::Paused;
            Ok(())
        }

        fn resume(&mut self, session_id: SessionId) -> Result<()> {
            let (_, state) = self
                .loops
                .get_mut(&session_id)
                .ok_or_else(|| anyhow::anyhow!("No loop found for session"))?;

            if !state.status.can_resume() {
                anyhow::bail!("Loop cannot be resumed");
            }

            state.status = RalphLoopStatus::Running;
            Ok(())
        }

        fn stop(&mut self, session_id: SessionId) -> Result<()> {
            let (_, state) = self
                .loops
                .get_mut(&session_id)
                .ok_or_else(|| anyhow::anyhow!("No loop found for session"))?;

            state.mark_cancelled();
            Ok(())
        }

        fn get_state(&self, session_id: SessionId) -> Option<&RalphLoopState> {
            self.loops.get(&session_id).map(|(_, s)| s)
        }

        fn get_state_mut(&mut self, session_id: SessionId) -> Option<&mut RalphLoopState> {
            self.loops.get_mut(&session_id).map(|(_, s)| s)
        }

        fn get_config(&self, session_id: SessionId) -> Option<&RalphLoopConfig> {
            self.loops.get(&session_id).map(|(c, _)| c)
        }

        fn active_sessions(&self) -> Vec<SessionId> {
            self.loops
                .iter()
                .filter(|(_, (_, s))| !s.status.is_finished())
                .map(|(id, _)| *id)
                .collect()
        }

        fn record_iteration(
            &mut self,
            session_id: SessionId,
            result: IterationResult,
        ) -> Result<()> {
            let (config, state) = self
                .loops
                .get_mut(&session_id)
                .ok_or_else(|| anyhow::anyhow!("No loop found for session"))?;

            let passed = result.verification_passed;
            let iteration = result.iteration;

            // Track progress
            if passed {
                state.iterations_without_progress = 0;
            } else {
                state.iterations_without_progress += 1;
            }

            state.current_iteration = iteration;
            state.iteration_history.push(result);

            // Check for completion
            if passed {
                state.mark_completed();
            } else if iteration >= config.max_iterations {
                state.mark_failed("Max iterations reached".to_string());
            }

            Ok(())
        }
    }

    #[test]
    fn test_ralph_controller_trait_is_object_safe() {
        // This compiles only if RalphLoopController is object-safe
        fn _takes_controller(_: &dyn RalphLoopController) {}
    }

    #[test]
    fn test_mock_controller_start_stop() {
        let mut controller = MockRalphController::new();
        let session_id = SessionId(1);

        controller
            .start(session_id, RalphLoopConfig::default())
            .unwrap();
        assert!(controller.is_active(session_id));

        controller.stop(session_id).unwrap();
        assert!(!controller.is_active(session_id));
    }

    #[test]
    fn test_mock_controller_pause_resume() {
        let mut controller = MockRalphController::new();
        let session_id = SessionId(1);

        controller
            .start(session_id, RalphLoopConfig::default())
            .unwrap();
        controller.pause(session_id).unwrap();

        let state = controller.get_state(session_id).unwrap();
        assert_eq!(state.status, RalphLoopStatus::Paused);

        controller.resume(session_id).unwrap();
        let state = controller.get_state(session_id).unwrap();
        assert_eq!(state.status, RalphLoopStatus::Running);
    }

    #[test]
    fn test_mock_controller_record_iteration_complete() {
        let mut controller = MockRalphController::new();
        let session_id = SessionId(1);

        controller
            .start(session_id, RalphLoopConfig::default())
            .unwrap();

        let result = IterationResult::new(1, true, "Fixed".to_string(), 1000);
        controller.record_iteration(session_id, result).unwrap();

        let state = controller.get_state(session_id).unwrap();
        assert_eq!(state.status, RalphLoopStatus::Completed);
    }

    #[test]
    fn test_mock_controller_record_iteration_max_reached() {
        let mut controller = MockRalphController::new();
        let session_id = SessionId(1);

        let config = RalphLoopConfig {
            max_iterations: 2,
            ..Default::default()
        };
        controller.start(session_id, config).unwrap();

        for i in 1..=2 {
            let result = IterationResult::new(i, false, "Still failing".to_string(), 1000);
            controller.record_iteration(session_id, result).unwrap();
        }

        let state = controller.get_state(session_id).unwrap();
        assert_eq!(state.status, RalphLoopStatus::Failed);
    }

    #[test]
    fn test_mock_controller_active_sessions() {
        let mut controller = MockRalphController::new();

        controller
            .start(SessionId(1), RalphLoopConfig::default())
            .unwrap();
        controller
            .start(SessionId(2), RalphLoopConfig::default())
            .unwrap();

        let active = controller.active_sessions();
        assert_eq!(active.len(), 2);

        controller.stop(SessionId(1)).unwrap();
        let active = controller.active_sessions();
        assert_eq!(active.len(), 1);
    }

    #[test]
    fn test_mock_controller_get_config() {
        let mut controller = MockRalphController::new();
        let session_id = SessionId(1);

        controller
            .start(session_id, RalphLoopConfig::for_rust())
            .unwrap();

        let config = controller.get_config(session_id).unwrap();
        assert_eq!(config.verification_command, "cargo test");
    }

    #[test]
    fn test_mock_controller_cannot_start_twice() {
        let mut controller = MockRalphController::new();
        let session_id = SessionId(1);

        controller
            .start(session_id, RalphLoopConfig::default())
            .unwrap();
        assert!(controller
            .start(session_id, RalphLoopConfig::default())
            .is_err());
    }

    #[test]
    fn test_mock_controller_cannot_pause_non_running() {
        let mut controller = MockRalphController::new();
        let session_id = SessionId(1);

        controller
            .start(session_id, RalphLoopConfig::default())
            .unwrap();
        controller.pause(session_id).unwrap();

        // Already paused, cannot pause again
        assert!(controller.pause(session_id).is_err());
    }

    #[test]
    fn test_mock_controller_cannot_resume_non_paused() {
        let mut controller = MockRalphController::new();
        let session_id = SessionId(1);

        controller
            .start(session_id, RalphLoopConfig::default())
            .unwrap();

        // Running, cannot resume
        assert!(controller.resume(session_id).is_err());
    }

    #[test]
    fn test_mock_controller_loop_count() {
        let mut controller = MockRalphController::new();

        assert_eq!(controller.loop_count(), 0);

        controller
            .start(SessionId(1), RalphLoopConfig::default())
            .unwrap();
        assert_eq!(controller.loop_count(), 1);

        controller
            .start(SessionId(2), RalphLoopConfig::default())
            .unwrap();
        assert_eq!(controller.loop_count(), 2);
    }

    #[test]
    fn test_mock_controller_stuck_sessions() {
        let mut controller = MockRalphController::new();
        let session_id = SessionId(1);

        let config = RalphLoopConfig {
            stuck_threshold: 3,
            ..Default::default()
        };
        controller.start(session_id, config).unwrap();

        // Record 3 failed iterations
        for i in 1..=3 {
            let result = IterationResult::new(i, false, "Failed".to_string(), 1000);
            controller.record_iteration(session_id, result).unwrap();
        }

        // Should not be stuck yet (max_iterations might have been reached)
        // Let's test with higher max_iterations
        let mut controller2 = MockRalphController::new();
        let config = RalphLoopConfig {
            stuck_threshold: 3,
            max_iterations: 20,
            ..Default::default()
        };
        controller2.start(session_id, config).unwrap();

        for i in 1..=3 {
            let result = IterationResult::new(i, false, "Failed".to_string(), 1000);
            controller2.record_iteration(session_id, result).unwrap();
        }

        let stuck = controller2.stuck_sessions();
        assert_eq!(stuck.len(), 1);
        assert_eq!(stuck[0], session_id);
    }

    #[test]
    fn test_mock_controller_get_state_mut() {
        let mut controller = MockRalphController::new();
        let session_id = SessionId(1);

        controller
            .start(session_id, RalphLoopConfig::default())
            .unwrap();

        if let Some(state) = controller.get_state_mut(session_id) {
            state.last_error = Some("Test error".to_string());
        }

        let state = controller.get_state(session_id).unwrap();
        assert_eq!(state.last_error, Some("Test error".to_string()));
    }

    #[test]
    fn test_mock_controller_no_loop_errors() {
        let mut controller = MockRalphController::new();
        let session_id = SessionId(999);

        assert!(controller.pause(session_id).is_err());
        assert!(controller.resume(session_id).is_err());
        assert!(controller.stop(session_id).is_err());
        assert!(controller
            .record_iteration(
                session_id,
                IterationResult::new(1, true, "Done".to_string(), 1000)
            )
            .is_err());
    }
}
