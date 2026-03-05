//! Scheduler configuration types.
//!
//! Contains [`SchedulerMode`] and [`SchedulerConfig`] which control
//! how tasks are ordered and selected for assignment.

use serde::{Deserialize, Serialize};

/// Scheduling mode for task assignment.
///
/// Determines how tasks are ordered and selected for assignment to sessions.
///
/// # Example
///
/// ```
/// use codirigent_core::SchedulerMode;
///
/// let mode = SchedulerMode::default();
/// assert_eq!(mode, SchedulerMode::Smart);
///
/// let mode = SchedulerMode::Priority;
/// assert_eq!(mode, SchedulerMode::Priority);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SchedulerMode {
    /// First-in, first-out ordering.
    ///
    /// Tasks are processed in the order they were added to the queue.
    Fifo,

    /// Order by priority level.
    ///
    /// Higher priority tasks are selected first (Critical > High > Medium > Low).
    Priority,

    /// Consider dependencies only.
    ///
    /// Tasks with fewer unmet dependencies are prioritized.
    Dependency,

    /// Combine priority, age, and dependencies (default).
    ///
    /// Uses a weighted scoring system considering priority level,
    /// time spent waiting in queue, and tag matching with sessions.
    #[default]
    Smart,
}

/// Configuration for the task scheduler.
///
/// Controls how tasks are ordered, when they are auto-assigned,
/// and the weighting factors for smart scheduling.
///
/// # Example
///
/// ```
/// use codirigent_core::{SchedulerConfig, SchedulerMode};
///
/// let config = SchedulerConfig::default();
/// assert_eq!(config.mode, SchedulerMode::Smart);
/// assert!(config.auto_assign);
/// assert_eq!(config.idle_threshold_seconds, 5);
///
/// // Custom configuration
/// let config = SchedulerConfig {
///     mode: SchedulerMode::Priority,
///     auto_assign: false,
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SchedulerConfig {
    /// Scheduling mode determining task ordering.
    pub mode: SchedulerMode,

    /// Whether to auto-assign tasks when sessions become idle.
    pub auto_assign: bool,

    /// Whether to confirm before auto-assigning.
    pub confirm_before_assign: bool,

    /// Seconds of idle time before considering a session available.
    pub idle_threshold_seconds: u32,

    /// Weight for priority in smart mode (0.0-1.0).
    pub priority_weight: f32,

    /// Weight for waiting time in smart mode (0.0-1.0).
    pub age_weight: f32,

    /// Weight for tag matching in smart mode (0.0-1.0).
    pub tag_match_weight: f32,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            mode: SchedulerMode::default(),
            auto_assign: true,
            confirm_before_assign: false,
            idle_threshold_seconds: 5,
            priority_weight: 0.5,
            age_weight: 0.3,
            tag_match_weight: 0.2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========== SchedulerMode Tests ==========

    #[test]
    fn test_scheduler_mode_default() {
        assert_eq!(SchedulerMode::default(), SchedulerMode::Smart);
    }

    #[test]
    fn test_scheduler_mode_equality() {
        assert_eq!(SchedulerMode::Fifo, SchedulerMode::Fifo);
        assert_eq!(SchedulerMode::Priority, SchedulerMode::Priority);
        assert_eq!(SchedulerMode::Dependency, SchedulerMode::Dependency);
        assert_eq!(SchedulerMode::Smart, SchedulerMode::Smart);
        assert_ne!(SchedulerMode::Fifo, SchedulerMode::Priority);
    }

    #[test]
    fn test_scheduler_mode_serialization() {
        let modes = [
            SchedulerMode::Fifo,
            SchedulerMode::Priority,
            SchedulerMode::Dependency,
            SchedulerMode::Smart,
        ];

        for mode in modes {
            let json = serde_json::to_string(&mode).unwrap();
            let parsed: SchedulerMode = serde_json::from_str(&json).unwrap();
            assert_eq!(mode, parsed);
        }
    }

    #[test]
    fn test_scheduler_mode_debug() {
        let mode = SchedulerMode::Smart;
        let debug_str = format!("{:?}", mode);
        assert!(debug_str.contains("Smart"));
    }

    #[test]
    fn test_scheduler_mode_clone() {
        let mode = SchedulerMode::Priority;
        let cloned = mode;
        assert_eq!(mode, cloned);
    }

    // ========== SchedulerConfig Tests ==========

    #[test]
    fn test_scheduler_config_default() {
        let config = SchedulerConfig::default();
        assert_eq!(config.mode, SchedulerMode::Smart);
        assert!(config.auto_assign);
        assert!(!config.confirm_before_assign);
        assert_eq!(config.idle_threshold_seconds, 5);
        assert!((config.priority_weight - 0.5).abs() < f32::EPSILON);
        assert!((config.age_weight - 0.3).abs() < f32::EPSILON);
        assert!((config.tag_match_weight - 0.2).abs() < f32::EPSILON);
    }

    #[test]
    fn test_scheduler_config_serialization() {
        let config = SchedulerConfig {
            mode: SchedulerMode::Priority,
            auto_assign: false,
            confirm_before_assign: true,
            idle_threshold_seconds: 10,
            priority_weight: 0.7,
            age_weight: 0.2,
            tag_match_weight: 0.1,
        };
        let json = serde_json::to_string(&config).unwrap();
        let parsed: SchedulerConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.mode, SchedulerMode::Priority);
        assert!(!parsed.auto_assign);
        assert!(parsed.confirm_before_assign);
        assert_eq!(parsed.idle_threshold_seconds, 10);
    }

    #[test]
    fn test_scheduler_config_equality() {
        let config1 = SchedulerConfig::default();
        let config2 = SchedulerConfig::default();
        let config3 = SchedulerConfig {
            mode: SchedulerMode::Fifo,
            ..Default::default()
        };
        assert_eq!(config1, config2);
        assert_ne!(config1, config3);
    }

    #[test]
    fn test_scheduler_config_clone() {
        let config = SchedulerConfig::default();
        let cloned = config.clone();
        assert_eq!(config, cloned);
    }

    #[test]
    fn test_scheduler_config_debug() {
        let config = SchedulerConfig::default();
        let debug_str = format!("{:?}", config);
        assert!(debug_str.contains("SchedulerConfig"));
        assert!(debug_str.contains("auto_assign"));
    }
}
