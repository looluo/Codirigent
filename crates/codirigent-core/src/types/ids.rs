//! Identifier types for sessions and tasks.

use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Unique identifier for a session.
///
/// Sessions are the core unit of work in Codirigent, each representing
/// a terminal instance running an AI CLI tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub u64);

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "session-{}", self.0)
    }
}

/// Unique identifier for a task.
///
/// Tasks are work items that can be assigned to sessions.
/// Uses Arc<str> internally for cheap cloning across threads.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TaskId(pub Arc<str>);

impl std::fmt::Display for TaskId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// Convenience conversions
impl From<String> for TaskId {
    fn from(s: String) -> Self {
        TaskId(Arc::from(s))
    }
}

impl From<&str> for TaskId {
    fn from(s: &str) -> Self {
        TaskId(Arc::from(s))
    }
}

impl From<Arc<str>> for TaskId {
    fn from(arc: Arc<str>) -> Self {
        TaskId(arc)
    }
}

impl serde::Serialize for TaskId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.as_ref().serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for TaskId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(TaskId(Arc::from(s)))
    }
}
