//! Session state management.
//!
//! This module provides the internal session state representation
//! that combines session metadata with runtime PTY handles.

use crate::session_io::SessionIoHandle;
use anyhow::Result;
use codirigent_core::{Session, SessionId, SessionStatus};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Internal session state combining metadata with runtime handles.
///
/// This struct holds both the persistent session metadata and the
/// runtime resources needed for terminal I/O. It is used internally
/// by the session manager and not exposed directly to other crates.
pub struct SessionState {
    /// Session metadata (persisted).
    pub session: Session,
    /// PTY child process ID.
    child_pid: u32,
    /// Background PTY command worker handle.
    io_handle: SessionIoHandle,
    /// Channel receiving PTY output.
    pub output_rx: mpsc::Receiver<Vec<u8>>,
    /// Producer-side dedup flag for the `OutputReady` channel notification.
    /// Shared with the PTY reader callback; cleared when output is drained.
    pub output_notified: Arc<AtomicBool>,
}

impl SessionState {
    /// Create a new session state.
    ///
    /// # Arguments
    ///
    /// * `session` - The session metadata
    /// * `child_pid` - The PTY child process ID
    /// * `io_handle` - Background PTY command worker handle
    /// * `output_rx` - Channel for receiving PTY output
    /// * `output_notified` - Shared dedup flag for output notifications
    pub(crate) fn new(
        session: Session,
        child_pid: u32,
        io_handle: SessionIoHandle,
        output_rx: mpsc::Receiver<Vec<u8>>,
        output_notified: Arc<AtomicBool>,
    ) -> Self {
        Self {
            session,
            child_pid,
            io_handle,
            output_rx,
            output_notified,
        }
    }

    /// Get session ID.
    pub fn id(&self) -> SessionId {
        self.session.id
    }

    /// Get current status.
    pub fn status(&self) -> SessionStatus {
        self.session.status
    }

    /// Update status.
    pub fn set_status(&mut self, status: SessionStatus) {
        self.session.status = status;
    }

    /// Get session name.
    pub fn name(&self) -> &str {
        &self.session.name
    }

    /// Get working directory.
    pub fn working_directory(&self) -> &std::path::Path {
        &self.session.working_directory
    }

    /// Queue PTY input for background delivery.
    pub(crate) fn send_input(&self, input: &[u8]) -> Result<()> {
        self.io_handle.send_input(input)
    }

    /// Queue a PTY resize for background delivery.
    pub(crate) fn resize(&self, rows: u16, cols: u16) -> Result<()> {
        self.io_handle.resize(rows, cols)
    }

    /// Request the background PTY worker to shut down.
    pub(crate) fn shutdown_io(&self) {
        self.io_handle.shutdown();
    }

    /// Get the PTY child process ID.
    pub fn child_pid(&self) -> u32 {
        self.child_pid
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pty::{spawn_output_reader, PtyHandle};
    use crate::session_io::SessionIoHandle;
    use tempfile::TempDir;

    fn create_test_session_state() -> (SessionState, TempDir) {
        let temp = TempDir::new().unwrap();
        let mut pty = PtyHandle::spawn(temp.path(), 24, 80, &[]).unwrap();
        let reader = pty.take_reader().unwrap();
        let output_rx = spawn_output_reader(reader);
        let child_pid = pty.child_pid();
        let io_handle = SessionIoHandle::spawn(SessionId(1), pty).unwrap();

        let session = Session::new(
            SessionId(1),
            "Test Session".to_string(),
            temp.path().to_path_buf(),
        );

        let output_notified = Arc::new(AtomicBool::new(false));
        (
            SessionState::new(session, child_pid, io_handle, output_rx, output_notified),
            temp,
        )
    }

    #[test]
    fn test_session_state_new() {
        let (state, _temp) = create_test_session_state();

        assert_eq!(state.id(), SessionId(1));
        assert_eq!(state.name(), "Test Session");
        assert_eq!(state.status(), SessionStatus::Idle);
    }

    #[test]
    fn test_session_state_id() {
        let (state, _temp) = create_test_session_state();
        assert_eq!(state.id(), SessionId(1));
    }

    #[test]
    fn test_session_state_status() {
        let (state, _temp) = create_test_session_state();
        assert_eq!(state.status(), SessionStatus::Idle);
    }

    #[test]
    fn test_session_state_set_status() {
        let (mut state, _temp) = create_test_session_state();

        state.set_status(SessionStatus::Working);
        assert_eq!(state.status(), SessionStatus::Working);

        state.set_status(SessionStatus::NeedsAttention);
        assert_eq!(state.status(), SessionStatus::NeedsAttention);
    }

    #[test]
    fn test_session_state_name() {
        let (state, _temp) = create_test_session_state();
        assert_eq!(state.name(), "Test Session");
    }

    #[test]
    fn test_session_state_working_directory() {
        let (state, temp) = create_test_session_state();
        assert_eq!(state.working_directory(), temp.path());
    }

    #[test]
    fn test_session_state_child_pid() {
        let (state, _temp) = create_test_session_state();
        assert!(state.child_pid() > 0);
    }

    #[test]
    fn test_session_state_access_session_fields() {
        let (state, _temp) = create_test_session_state();

        // Access session metadata directly
        assert_eq!(state.session.id, SessionId(1));
        assert_eq!(state.session.name, "Test Session");
        assert!(state.session.current_task.is_none());
        assert!(state.session.group.is_none());
        assert!(state.session.color.is_none());
    }

    #[test]
    fn test_session_state_modify_session() {
        let (mut state, _temp) = create_test_session_state();

        state.session.name = "Renamed Session".to_string();
        assert_eq!(state.name(), "Renamed Session");

        state.session.group = Some("my-group".to_string());
        assert_eq!(state.session.group, Some("my-group".to_string()));
    }
}
