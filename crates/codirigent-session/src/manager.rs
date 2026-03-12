//! Session manager implementation.
//!
//! This module provides the default implementation of the [`SessionManager`]
//! trait, managing session lifecycle, PTY I/O, and event publishing.

use crate::git_status::GitStatusService;
use crate::pty::{spawn_output_reader_with_notify, PtyHandle};
use crate::session::SessionState;
use crate::session_io::SessionIoHandle;
use anyhow::{anyhow, Context, Result};
use codirigent_core::{
    CodirigentEvent, EventBus, GitRepoInfo, Session, SessionId, SessionManager, SessionStatus,
    SessionUpdate,
};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;
use tracing::{debug, info, trace};

/// Default terminal height in rows.
const DEFAULT_PTY_ROWS: u16 = 24;
/// Default terminal width in columns.
const DEFAULT_PTY_COLS: u16 = 80;

/// Output drained from a session without exhausting its full backlog.
#[derive(Debug, Default)]
pub struct DrainedOutput {
    /// Bytes drained from the PTY output queue.
    pub data: Vec<u8>,
    /// Whether more output is still queued for this session.
    pub has_more: bool,
}

use crate::normalize_path;

/// Channel capacity for the `SessionUpdate` mpsc channel.
///
/// Sized to handle bursts from multiple concurrent PTY readers without
/// back-pressure. Each `SessionUpdate` is small (enum + SessionId).
const SESSION_UPDATE_CHANNEL_CAPACITY: usize = 512;

/// Default implementation of [`SessionManager`].
///
/// Manages terminal sessions including PTY spawning, I/O handling,
/// and event publishing. Sessions are stored in a HashMap for O(1)
/// lookup by ID.
///
/// # Example
///
/// ```ignore
/// use codirigent_session::DefaultSessionManager;
/// use codirigent_core::{DefaultEventBus, SessionManager};
/// use std::sync::Arc;
///
/// let event_bus = Arc::new(DefaultEventBus::new(16));
/// let mut manager = DefaultSessionManager::new(event_bus);
///
/// let id = manager.create_session(
///     "My Session".to_string(),
///     std::path::PathBuf::from("/tmp"),
///     None,
/// ).unwrap();
///
/// manager.send_input(id, b"echo hello\n").unwrap();
/// ```
pub struct DefaultSessionManager {
    /// Active sessions indexed by ID.
    ///
    /// Wrapped in Mutex to satisfy Sync requirement. The inner types
    /// (PtyHandle) are Send but not Sync due to raw I/O handles.
    sessions: Mutex<HashMap<SessionId, SessionState>>,
    /// Counter for generating unique session IDs.
    next_id: AtomicU64,
    /// Event bus for publishing session events.
    event_bus: Arc<dyn EventBus>,
    /// Git status detection service.
    git_status: Mutex<GitStatusService>,
    /// Sessions whose PTY readers have queued unread output since the last poll.
    pending_output_sessions: Arc<Mutex<HashSet<SessionId>>>,
    /// Sender for the internal hot-path `SessionUpdate` channel.
    ///
    /// Cloned into PTY reader callbacks and OSC parser paths so they can
    /// emit events without going through the legacy `pending_output_sessions`
    /// mechanism. The receiver is taken once via [`take_update_receiver`].
    update_tx: tokio::sync::mpsc::Sender<SessionUpdate>,
    /// Receiver for the internal hot-path `SessionUpdate` channel.
    ///
    /// Wrapped in `Option` because it is taken once by the output dispatcher.
    update_rx: Mutex<Option<tokio::sync::mpsc::Receiver<SessionUpdate>>>,
}

impl DefaultSessionManager {
    /// Create a new session manager.
    ///
    /// # Arguments
    ///
    /// * `event_bus` - The event bus for publishing session events
    pub fn new(event_bus: Arc<dyn EventBus>) -> Self {
        let (update_tx, update_rx) = tokio::sync::mpsc::channel(SESSION_UPDATE_CHANNEL_CAPACITY);
        Self {
            sessions: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            event_bus,
            git_status: Mutex::new(GitStatusService::new()),
            pending_output_sessions: Arc::new(Mutex::new(HashSet::new())),
            update_tx,
            update_rx: Mutex::new(Some(update_rx)),
        }
    }

    /// Take the `SessionUpdate` receiver.
    ///
    /// This can only be called once — the receiver is consumed by the output
    /// dispatcher. Returns `None` on subsequent calls.
    pub fn take_update_receiver(&self) -> Option<tokio::sync::mpsc::Receiver<SessionUpdate>> {
        self.update_rx
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .take()
    }

    /// Get a clone of the `SessionUpdate` sender.
    ///
    /// Used by external components that need to emit `SessionUpdate` events
    /// (e.g., hook signal readers, JSONL parsers).
    pub fn update_sender(&self) -> tokio::sync::mpsc::Sender<SessionUpdate> {
        self.update_tx.clone()
    }

    /// Acquire the sessions lock.
    ///
    fn lock_sessions(&self) -> MutexGuard<'_, HashMap<SessionId, SessionState>> {
        self.sessions.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn lock_pending_output_sessions(&self) -> MutexGuard<'_, HashSet<SessionId>> {
        self.pending_output_sessions
            .lock()
            .unwrap_or_else(|p| p.into_inner())
    }

    /// Generate a unique session ID.
    fn next_session_id(&self) -> SessionId {
        SessionId(self.next_id.fetch_add(1, Ordering::SeqCst))
    }

    /// Execute a function with mutable access to a session state.
    ///
    /// This method provides safe access to session state by acquiring
    /// the internal lock and calling the provided closure.
    ///
    /// # Arguments
    ///
    /// * `id` - The session ID to access
    /// * `f` - Function to execute with the session state
    ///
    /// # Returns
    ///
    /// `None` if the session doesn't exist, otherwise the result of `f`.
    pub fn with_session_state_mut<T, F>(&self, id: SessionId, f: F) -> Option<T>
    where
        F: FnOnce(&mut SessionState) -> T,
    {
        let mut sessions = self.lock_sessions();
        sessions.get_mut(&id).map(f)
    }

    /// Execute a function with immutable access to a session state.
    ///
    /// This method provides safe access to session state by acquiring
    /// the internal lock and calling the provided closure.
    ///
    /// # Arguments
    ///
    /// * `id` - The session ID to access
    /// * `f` - Function to execute with the session state
    ///
    /// # Returns
    ///
    /// `None` if the session doesn't exist, otherwise the result of `f`.
    pub fn with_session_state<T, F>(&self, id: SessionId, f: F) -> Option<T>
    where
        F: FnOnce(&SessionState) -> T,
    {
        let sessions = self.lock_sessions();
        sessions.get(&id).map(f)
    }

    /// Drain output from a session's channel (non-blocking).
    ///
    /// Collects all available output from the PTY output channel
    /// without blocking. Returns `None` if no output is available
    /// or the session doesn't exist.
    ///
    /// # Arguments
    ///
    /// * `id` - The session ID to drain output from
    pub fn try_drain_output(&self, id: SessionId) -> Option<Vec<u8>> {
        self.try_drain_output_bounded(id, usize::MAX, usize::MAX)
            .map(|drained| drained.data)
    }

    /// Drain output from a session with soft chunk/byte budgets.
    ///
    /// This keeps a single noisy session from monopolizing the UI thread while
    /// still preserving the unread backlog in the channel for the next poll.
    pub fn try_drain_output_bounded(
        &self,
        id: SessionId,
        max_chunks: usize,
        max_bytes: usize,
    ) -> Option<DrainedOutput> {
        let (output, has_more) = {
            let mut sessions = self.lock_sessions();
            let state = sessions.get_mut(&id)?;
            let chunk_budget = max_chunks.max(1);
            let byte_budget = max_bytes.max(1);
            let mut output = Vec::with_capacity(byte_budget.min(64 * 1024));
            let mut chunks_drained = 0usize;
            let mut hit_budget = false;

            while let Ok(data) = state.output_rx.try_recv() {
                output.extend(data);
                chunks_drained += 1;
                if chunks_drained >= chunk_budget || output.len() >= byte_budget {
                    hit_budget = true;
                    break;
                }
            }

            // Reset the producer-side dedup flag so the PTY reader sends a
            // new OutputReady notification for any subsequent output.
            //
            // Race window: a PTY chunk may arrive between this reset and the
            // `is_empty()` check below. If so, the producer's swap(true) succeeds
            // and sends OutputReady to the channel, but `has_more` may be false
            // for this cycle. Data is NOT lost — the OutputReady event in the
            // channel is drained on the next poll_output cycle (~16ms). The 1s
            // legacy fallback poll acts as an additional safety net.
            state
                .output_notified
                .store(false, std::sync::atomic::Ordering::Relaxed);

            let has_more = !output.is_empty() && (hit_budget || !state.output_rx.is_empty());
            (output, has_more)
        };

        if has_more {
            self.lock_pending_output_sessions().insert(id);
        }

        if output.is_empty() {
            None
        } else {
            Some(DrainedOutput {
                data: output,
                has_more,
            })
        }
    }

    /// Publish an event to the event bus.
    fn publish(&self, event: CodirigentEvent) {
        self.event_bus.publish(event);
    }

    /// Get the number of active sessions.
    pub fn session_count(&self) -> usize {
        self.lock_sessions().len()
    }

    /// Check if a session exists.
    pub fn session_exists(&self, id: SessionId) -> bool {
        self.lock_sessions().contains_key(&id)
    }

    /// Get all session IDs.
    pub fn session_ids(&self) -> Vec<SessionId> {
        self.lock_sessions().keys().copied().collect()
    }

    /// Get all sessions that currently have unread PTY output queued.
    pub fn sessions_with_pending_output(&self) -> Vec<SessionId> {
        let ids: Vec<SessionId> = self.lock_pending_output_sessions().drain().collect();
        if !ids.is_empty() {
            trace!(count = ids.len(), "sessions_with_pending_output");
        }
        ids
    }

    /// Mark a session as having unread PTY output ready for UI polling.
    pub fn mark_output_pending(&self, id: SessionId) {
        if self.session_exists(id) {
            self.lock_pending_output_sessions().insert(id);
        }
    }

    /// Get the child PID for a session.
    ///
    /// Returns the process ID of the PTY child process for the given session.
    pub fn get_child_pid(&self, id: SessionId) -> Option<u32> {
        self.lock_sessions().get(&id).map(|s| s.child_pid())
    }

    /// Update the working directory for a session (detected via OSC 7).
    ///
    /// If the new directory differs from the current one, updates the session,
    /// invalidates the git cache for the old repo root, and publishes a
    /// `WorkingDirectoryChanged` event.
    ///
    /// Returns `true` if the directory actually changed.
    pub fn update_working_directory(&self, id: SessionId, new_dir: PathBuf) -> bool {
        // Normalize the new path: canonicalize + strip \\?\ prefix on Windows
        // so PowerShell shows normal C:\... paths instead of UNC paths in its prompt.
        let new_dir = normalize_path(&new_dir);

        let old_dir = {
            let mut sessions = self.lock_sessions();
            let state = match sessions.get_mut(&id) {
                Some(s) => s,
                None => return false,
            };

            // Normalize stored path for comparison (same stripping as new_dir above)
            let current = normalize_path(&state.session.working_directory);
            if current == new_dir {
                return false;
            }

            let old = state.session.working_directory.clone();
            state.session.working_directory = new_dir.clone();

            // Clear stale git info so the next refresh picks up the new repo
            if let Some(ref info) = state.session.git_info {
                let repo_root = info.repo_root.clone();
                drop(sessions); // release session lock before git lock
                self.git_status
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .invalidate(&repo_root);
            } else {
                drop(sessions);
            }

            old
        };

        info!(%id, old=?old_dir, new=?new_dir, "Session working directory changed (OSC 7)");

        self.publish(CodirigentEvent::WorkingDirectoryChanged {
            id,
            old_dir,
            new_dir,
        });

        true
    }

    /// Refresh git status for a session.
    ///
    /// Detects or refreshes git repository information for the session's
    /// working directory. Uses cached results within a 15-second TTL so
    /// the 3-second polling loop hits cache most of the time.
    ///
    /// Returns the updated git info, or None if the session doesn't exist
    /// or isn't in a git repository.
    pub fn refresh_git_status(&self, id: SessionId) -> Option<GitRepoInfo> {
        self.refresh_git_status_impl(id, false)
    }

    /// Refresh git status for a session, bypassing the TTL cache.
    ///
    /// Use this after an explicit working-directory change so the UI does not
    /// inherit a stale cached snapshot from another session's recent visit to
    /// the destination worktree.
    pub fn refresh_git_status_fresh(&self, id: SessionId) -> Option<GitRepoInfo> {
        self.refresh_git_status_impl(id, true)
    }

    fn refresh_git_status_impl(&self, id: SessionId, force_fresh: bool) -> Option<GitRepoInfo> {
        let working_dir = {
            let sessions = self.lock_sessions();
            sessions.get(&id)?.session.working_directory.clone()
        };

        let info = {
            let mut git_status = self.git_status.lock().unwrap_or_else(|p| p.into_inner());
            if force_fresh {
                git_status.detect_fresh(&working_dir)
            } else {
                git_status.detect_cached(&working_dir, Duration::from_secs(15))
            }
        };

        // Update session
        let mut sessions = self.lock_sessions();
        if let Some(state) = sessions.get_mut(&id) {
            state.session.git_info = info.clone();
        }

        info
    }
}

impl SessionManager for DefaultSessionManager {
    fn list_sessions(&self) -> Vec<Session> {
        self.lock_sessions()
            .values()
            .map(|s| s.session.clone())
            .collect()
    }

    fn get_session(&self, id: SessionId) -> Option<Session> {
        self.lock_sessions().get(&id).map(|s| s.session.clone())
    }

    fn create_session(
        &self,
        name: String,
        working_dir: PathBuf,
        shell: Option<String>,
    ) -> Result<SessionId> {
        let id = self.next_session_id();

        // Normalize before validation and PTY spawn so PowerShell sees a clean
        // C:\... path rather than the \\?\ extended-length form.
        let working_dir = normalize_path(&working_dir);
        info!(%id, %name, ?working_dir, ?shell, "Creating session");

        // Validate working directory exists and is a directory
        if !working_dir.exists() {
            return Err(anyhow!(
                "Working directory does not exist: {}",
                working_dir.display()
            ));
        }
        if !working_dir.is_dir() {
            return Err(anyhow!(
                "Working directory path is not a directory: {}",
                working_dir.display()
            ));
        }

        // Inject CODIRIGENT_SESSION_ID so codirigent-hook can match hook signals
        // back to this exact session without relying on CWD heuristics.
        let id_str = id.0.to_string();
        let env_vars: &[(&str, &str)] = &[("CODIRIGENT_SESSION_ID", &id_str)];

        // Spawn PTY: use specific shell if provided, otherwise auto-detect
        let mut pty = if let Some(ref shell_name) = shell {
            if !shell_name.is_empty() {
                let shell_cmd = crate::shell_detection::resolve_shell(shell_name);
                let args: Vec<&str> = shell_cmd.args.iter().map(|a| a.as_str()).collect();
                PtyHandle::spawn_command(
                    &working_dir,
                    &shell_cmd.program,
                    &args,
                    DEFAULT_PTY_ROWS,
                    DEFAULT_PTY_COLS,
                    env_vars,
                )
                .context("Failed to spawn PTY with selected shell")?
            } else {
                PtyHandle::spawn(&working_dir, DEFAULT_PTY_ROWS, DEFAULT_PTY_COLS, env_vars)
                    .context("Failed to spawn PTY")?
            }
        } else {
            PtyHandle::spawn(&working_dir, DEFAULT_PTY_ROWS, DEFAULT_PTY_COLS, env_vars)
                .context("Failed to spawn PTY")?
        };

        // Take reader and spawn output task
        let reader = pty
            .take_reader()
            .ok_or_else(|| anyhow!("Failed to get PTY reader"))?;
        let pending_output_sessions = self.pending_output_sessions.clone();
        let pending_output_for_exit = self.pending_output_sessions.clone();
        let update_tx = self.update_tx.clone();
        let exit_tx = self.update_tx.clone();
        // Producer-side deduplication: the flag is set before try_send and
        // only sends when transitioning from false → true. The consumer
        // (output_dispatcher::drain_updates) deduplicates into a HashSet,
        // so clearing the flag is not required — it auto-resets when the
        // next drain_updates call consumes all pending OutputReady events.
        // The flag prevents a noisy session (e.g. `yes`) from saturating
        // the bounded channel with redundant OutputReady events.
        let output_notified = Arc::new(AtomicBool::new(false));
        let output_notified_clone = output_notified.clone();
        let output_rx = spawn_output_reader_with_notify(
            reader,
            move || {
                // Legacy path: mark in the pending set (consumed by broad poll)
                pending_output_sessions
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .insert(id);
                // New path: emit event for the output dispatcher.
                // NOTE: This runs on a bare std::thread (PTY reader), not a tokio
                // task. Use try_send(), NOT .send().await, as there is no async
                // runtime here. Failure is non-fatal: the legacy path above ensures
                // output is not lost.
                //
                // Producer-side dedup: skip try_send if a notification is already
                // pending in the channel. This prevents a single noisy session from
                // filling the bounded 512-slot channel.
                if !output_notified.swap(true, Ordering::Relaxed) {
                    if let Err(e) =
                        update_tx.try_send(SessionUpdate::OutputReady { session_id: id })
                    {
                        tracing::warn!("SessionUpdate channel full for session {}: {e}", id.0);
                        // Reset so the next chunk retries
                        output_notified.store(false, Ordering::Relaxed);
                    }
                }
            },
            move || {
                // Notify the event-driven pipeline that the PTY child exited.
                // If the channel is full, the fallback inserts into
                // pending_output_sessions which is drained every ~1s. The exit
                // semantic (ChildProcessExited) is lost in this case, but the
                // consumer's `prepared == None` handler in
                // schedule_session_output_preparation still runs
                // sync_session_status(), ensuring the session does not get
                // stuck in Working indefinitely.
                if let Err(e) =
                    exit_tx.try_send(SessionUpdate::ChildProcessExited { session_id: id })
                {
                    tracing::warn!("ChildProcessExited channel full for session {}: {e}", id.0);
                    pending_output_for_exit
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .insert(id);
                }
            },
        );

        // Create session metadata
        let mut session = Session::new(id, name, working_dir.clone());

        // Detect git info for the working directory
        session.git_info = self
            .git_status
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .detect(&working_dir);

        let child_pid = pty.child_pid();
        let io_handle = SessionIoHandle::spawn(id, pty)?;

        // Create session state
        let state = SessionState::new(
            session,
            child_pid,
            io_handle,
            output_rx,
            output_notified_clone,
        );
        self.lock_sessions().insert(id, state);

        // Publish event
        self.publish(CodirigentEvent::SessionCreated { id });

        Ok(id)
    }

    fn close_session(&self, id: SessionId) -> Result<()> {
        info!(%id, "Closing session");

        let state = self
            .lock_sessions()
            .remove(&id)
            .ok_or_else(|| anyhow!("Session not found: {}", id))?;

        state.shutdown_io();

        self.lock_pending_output_sessions().remove(&id);

        // PTY and output_rx will be dropped, cleaning up resources

        self.publish(CodirigentEvent::SessionClosed { id });

        Ok(())
    }

    fn send_input(&self, id: SessionId, input: &[u8]) -> Result<()> {
        let sessions = self.lock_sessions();
        let state = sessions
            .get(&id)
            .ok_or_else(|| anyhow!("Session not found: {}", id))?;

        state
            .send_input(input)
            .context("Failed to queue input for PTY")?;

        Ok(())
    }

    fn resize(&self, id: SessionId, rows: u16, cols: u16) -> Result<()> {
        let sessions = self.lock_sessions();
        let state = sessions
            .get(&id)
            .ok_or_else(|| anyhow!("Session not found: {}", id))?;

        state
            .resize(rows, cols)
            .context("Failed to queue PTY resize")?;

        Ok(())
    }

    fn update_status(&self, id: SessionId, status: SessionStatus) {
        let mut sessions = self.lock_sessions();
        if let Some(state) = sessions.get_mut(&id) {
            let old = state.status();
            if old != status {
                debug!(%id, ?old, ?status, "Session status changed");
                state.set_status(status);
                // Drop lock before publishing to avoid deadlock
                drop(sessions);
                self.publish(CodirigentEvent::SessionStatusChanged {
                    id,
                    old,
                    new: status,
                });
            }
        }
    }

    fn rename_session(&self, id: SessionId, new_name: String) -> Result<()> {
        info!(%id, %new_name, "Renaming session");

        let old_name = {
            let mut sessions = self.lock_sessions();
            let state = sessions
                .get_mut(&id)
                .ok_or_else(|| anyhow!("Session not found: {}", id))?;

            let old_name = state.session.name.clone();
            state.session.name = new_name.clone();
            old_name
        };

        debug!(%id, %old_name, %new_name, "Session renamed successfully");

        self.publish(CodirigentEvent::SessionRenamed {
            id,
            old_name,
            new_name,
        });

        Ok(())
    }

    fn set_session_group(
        &self,
        id: SessionId,
        group: Option<String>,
        color: Option<String>,
    ) -> Result<()> {
        info!(%id, ?group, ?color, "Setting session group");

        {
            let mut sessions = self.lock_sessions();
            let state = sessions
                .get_mut(&id)
                .ok_or_else(|| anyhow!("Session not found: {}", id))?;

            state.session.group = group.clone();
            state.session.color = color.clone();
        }

        debug!(%id, ?group, ?color, "Session group updated successfully");

        self.publish(CodirigentEvent::SessionGroupChanged { id, group, color });

        Ok(())
    }

    fn update_context_usage(&self, id: SessionId, usage: Option<f32>) {
        let mut sessions = self.lock_sessions();
        if let Some(state) = sessions.get_mut(&id) {
            state.session.context_usage = usage;
        }
    }

    fn get_context_file_path(&self, _id: SessionId) -> Option<PathBuf> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pty::PtyHandle;
    use crate::session::SessionState;
    use codirigent_core::{DefaultEventBus, Session};
    use git2::Repository;
    use std::fs;
    use std::io::Write;
    use std::path::Path;
    use tempfile::TempDir;
    use tokio::sync::mpsc;

    fn create_manager() -> DefaultSessionManager {
        let event_bus = Arc::new(DefaultEventBus::new(16));
        DefaultSessionManager::new(event_bus)
    }

    fn create_manager_with_bus() -> (DefaultSessionManager, Arc<DefaultEventBus>) {
        let event_bus = Arc::new(DefaultEventBus::new(16));
        let manager = DefaultSessionManager::new(event_bus.clone());
        (manager, event_bus)
    }

    fn create_manual_session_state(
        id: SessionId,
    ) -> (SessionState, mpsc::Sender<Vec<u8>>, TempDir) {
        let temp = TempDir::new().unwrap();
        let pty = PtyHandle::spawn(temp.path(), DEFAULT_PTY_ROWS, DEFAULT_PTY_COLS, &[]).unwrap();
        let (tx, rx) = mpsc::channel(8);
        let session = Session::new(id, format!("Session {}", id.0), temp.path().to_path_buf());
        let output_notified = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let child_pid = pty.child_pid();
        let io_handle = SessionIoHandle::spawn(id, pty).unwrap();
        (
            SessionState::new(session, child_pid, io_handle, rx, output_notified),
            tx,
            temp,
        )
    }

    fn create_test_repo() -> TempDir {
        let dir = tempfile::tempdir().unwrap();
        let repo = Repository::init(dir.path()).unwrap();

        {
            let mut config = repo.config().unwrap();
            config.set_str("user.name", "Test User").unwrap();
            config.set_str("user.email", "test@example.com").unwrap();
        }

        let readme = dir.path().join("README.md");
        let mut file = fs::File::create(&readme).unwrap();
        file.write_all(b"hello").unwrap();

        let sig = repo.signature().unwrap();
        let tree_id = {
            let mut index = repo.index().unwrap();
            index.add_path(Path::new("README.md")).unwrap();
            index.write().unwrap();
            index.write_tree().unwrap()
        };
        let tree = repo.find_tree(tree_id).unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])
            .unwrap();

        dir
    }

    fn deterministic_test_shell() -> Option<String> {
        #[cfg(windows)]
        {
            Some("cmd.exe".to_string())
        }
        #[cfg(not(windows))]
        {
            Some("/bin/sh".to_string())
        }
    }

    fn create_linked_worktree(repo_path: &Path, branch: &str) -> PathBuf {
        let repo = Repository::open(repo_path).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch(branch, &head, false).unwrap();

        let worktree_path = repo_path.join("worktrees").join(branch);
        fs::create_dir_all(worktree_path.parent().unwrap()).unwrap();

        let reference = repo
            .find_reference(&format!("refs/heads/{}", branch))
            .unwrap();
        let mut add_options = git2::WorktreeAddOptions::new();
        add_options.reference(Some(&reference));
        repo.worktree(branch, &worktree_path, Some(&add_options))
            .unwrap();

        worktree_path
    }

    #[test]
    fn test_new_manager() {
        let manager = create_manager();
        assert_eq!(manager.session_count(), 0);
    }

    #[test]
    fn test_create_session() {
        let manager = create_manager();

        let id = manager
            .create_session("Test Session".to_string(), std::env::temp_dir(), None)
            .unwrap();

        assert!(manager.get_session(id).is_some());
        assert_eq!(manager.list_sessions().len(), 1);
        assert_eq!(manager.session_count(), 1);
    }

    #[test]
    fn test_refresh_git_status_fresh_bypasses_stale_destination_worktree_cache() {
        let manager = create_manager();
        let repo_dir = create_test_repo();
        let worktree_path = create_linked_worktree(repo_dir.path(), "feature-fresh");

        let source_session = manager
            .create_session("Source".to_string(), worktree_path.clone(), None)
            .unwrap();
        let cached_clean = manager.refresh_git_status(source_session).unwrap();
        assert_eq!(cached_clean.branch, "feature-fresh");
        assert_eq!(cached_clean.dirty_count, 0);

        fs::write(worktree_path.join("dirty.txt"), "dirty").unwrap();

        let switched_session = manager
            .create_session("Switch".to_string(), repo_dir.path().to_path_buf(), None)
            .unwrap();
        assert!(manager.update_working_directory(switched_session, worktree_path.clone()));

        let fresh = manager.refresh_git_status_fresh(switched_session).unwrap();
        assert_eq!(fresh.branch, "feature-fresh");
        assert_eq!(fresh.dirty_count, 1);
    }

    #[test]
    fn test_create_multiple_sessions() {
        let manager = create_manager();

        let id1 = manager
            .create_session("Session 1".to_string(), std::env::temp_dir(), None)
            .unwrap();
        let id2 = manager
            .create_session("Session 2".to_string(), std::env::temp_dir(), None)
            .unwrap();
        let id3 = manager
            .create_session("Session 3".to_string(), std::env::temp_dir(), None)
            .unwrap();

        assert_ne!(id1, id2);
        assert_ne!(id2, id3);
        assert_eq!(manager.session_count(), 3);
    }

    #[test]
    fn test_create_session_publishes_event() {
        let (manager, event_bus) = create_manager_with_bus();
        let mut rx = event_bus.subscribe();

        let id = manager
            .create_session("Test".to_string(), std::env::temp_dir(), None)
            .unwrap();

        let event = rx.try_recv().unwrap();
        assert!(
            matches!(event, CodirigentEvent::SessionCreated { id: created_id } if created_id == id)
        );
    }

    #[test]
    fn test_close_session() {
        let manager = create_manager();

        let id = manager
            .create_session("Test".to_string(), std::env::temp_dir(), None)
            .unwrap();

        manager.close_session(id).unwrap();
        assert!(manager.get_session(id).is_none());
        assert_eq!(manager.session_count(), 0);
    }

    #[test]
    fn test_close_session_publishes_event() {
        let (manager, event_bus) = create_manager_with_bus();
        let mut rx = event_bus.subscribe();

        let id = manager
            .create_session("Test".to_string(), std::env::temp_dir(), None)
            .unwrap();

        // Consume create event
        let _ = rx.try_recv();

        manager.close_session(id).unwrap();

        let event = rx.try_recv().unwrap();
        assert!(
            matches!(event, CodirigentEvent::SessionClosed { id: closed_id } if closed_id == id)
        );
    }

    #[test]
    fn test_close_nonexistent_session() {
        let manager = create_manager();

        let result = manager.close_session(SessionId(999));
        assert!(result.is_err());
    }

    #[test]
    fn test_get_session() {
        let manager = create_manager();

        let id = manager
            .create_session("Test Session".to_string(), std::env::temp_dir(), None)
            .unwrap();

        let session = manager.get_session(id).unwrap();
        assert_eq!(session.id, id);
        assert_eq!(session.name, "Test Session");
    }

    #[test]
    fn test_get_nonexistent_session() {
        let manager = create_manager();
        assert!(manager.get_session(SessionId(999)).is_none());
    }

    #[test]
    fn test_list_sessions() {
        let manager = create_manager();

        manager
            .create_session("Session 1".to_string(), std::env::temp_dir(), None)
            .unwrap();
        manager
            .create_session("Session 2".to_string(), std::env::temp_dir(), None)
            .unwrap();

        let sessions = manager.list_sessions();
        assert_eq!(sessions.len(), 2);
    }

    #[test]
    fn test_send_input() {
        let manager = create_manager();

        let id = manager
            .create_session("Test".to_string(), std::env::temp_dir(), None)
            .unwrap();

        let result = manager.send_input(id, b"echo hello\n");
        assert!(result.is_ok());
    }

    #[test]
    fn test_send_input_nonexistent_session() {
        let manager = create_manager();

        let result = manager.send_input(SessionId(999), b"test");
        assert!(result.is_err());
    }

    #[test]
    fn test_resize() {
        let manager = create_manager();

        let id = manager
            .create_session("Test".to_string(), std::env::temp_dir(), None)
            .unwrap();

        let result = manager.resize(id, 48, 120);
        assert!(result.is_ok());
    }

    #[test]
    fn test_resize_nonexistent_session() {
        let manager = create_manager();

        let result = manager.resize(SessionId(999), 48, 120);
        assert!(result.is_err());
    }

    #[test]
    fn test_update_status() {
        let manager = create_manager();

        let id = manager
            .create_session("Test".to_string(), std::env::temp_dir(), None)
            .unwrap();

        manager.update_status(id, SessionStatus::Working);

        let session = manager.get_session(id).unwrap();
        assert_eq!(session.status, SessionStatus::Working);
    }

    #[test]
    fn test_update_status_publishes_event() {
        let (manager, event_bus) = create_manager_with_bus();
        let mut rx = event_bus.subscribe();

        let id = manager
            .create_session("Test".to_string(), std::env::temp_dir(), None)
            .unwrap();

        // Consume create event
        let _ = rx.try_recv();

        manager.update_status(id, SessionStatus::Working);

        let event = rx.try_recv().unwrap();
        assert!(matches!(
            event,
            CodirigentEvent::SessionStatusChanged {
                id: changed_id,
                old: SessionStatus::Idle,
                new: SessionStatus::Working
            } if changed_id == id
        ));
    }

    #[test]
    fn test_update_status_no_event_if_unchanged() {
        let (manager, event_bus) = create_manager_with_bus();
        let mut rx = event_bus.subscribe();

        let id = manager
            .create_session("Test".to_string(), std::env::temp_dir(), None)
            .unwrap();

        // Consume create event
        let _ = rx.try_recv();

        // Set to Idle (same as default) - should not publish event
        manager.update_status(id, SessionStatus::Idle);

        // Channel should be empty
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn test_update_status_nonexistent_session() {
        let manager = create_manager();

        // Should not panic
        manager.update_status(SessionId(999), SessionStatus::Working);
    }

    #[test]
    fn test_rename_session() {
        let manager = create_manager();

        let id = manager
            .create_session("Original".to_string(), std::env::temp_dir(), None)
            .unwrap();

        manager.rename_session(id, "Renamed".to_string()).unwrap();

        let session = manager.get_session(id).unwrap();
        assert_eq!(session.name, "Renamed");
    }

    #[test]
    fn test_rename_session_publishes_event() {
        let (manager, event_bus) = create_manager_with_bus();
        let mut rx = event_bus.subscribe();

        let id = manager
            .create_session("Original".to_string(), std::env::temp_dir(), None)
            .unwrap();

        // Consume create event
        let _ = rx.try_recv();

        manager.rename_session(id, "Renamed".to_string()).unwrap();

        let event = rx.try_recv().unwrap();
        assert!(matches!(
            event,
            CodirigentEvent::SessionRenamed {
                id: renamed_id,
                old_name,
                new_name
            } if renamed_id == id && old_name == "Original" && new_name == "Renamed"
        ));
    }

    #[test]
    fn test_rename_nonexistent_session() {
        let manager = create_manager();

        let result = manager.rename_session(SessionId(999), "New".to_string());
        assert!(result.is_err());
    }

    #[test]
    fn test_set_session_group() {
        let manager = create_manager();

        let id = manager
            .create_session("Test".to_string(), std::env::temp_dir(), None)
            .unwrap();

        manager
            .set_session_group(
                id,
                Some("my-project".to_string()),
                Some("#ff0000".to_string()),
            )
            .unwrap();

        let session = manager.get_session(id).unwrap();
        assert_eq!(session.group, Some("my-project".to_string()));
        assert_eq!(session.color, Some("#ff0000".to_string()));
    }

    #[test]
    fn test_set_session_group_clear() {
        let manager = create_manager();

        let id = manager
            .create_session("Test".to_string(), std::env::temp_dir(), None)
            .unwrap();

        // Set group
        manager
            .set_session_group(id, Some("group".to_string()), Some("#000".to_string()))
            .unwrap();

        // Clear group
        manager.set_session_group(id, None, None).unwrap();

        let session = manager.get_session(id).unwrap();
        assert!(session.group.is_none());
        assert!(session.color.is_none());
    }

    #[test]
    fn test_set_session_group_publishes_event() {
        let (manager, event_bus) = create_manager_with_bus();
        let mut rx = event_bus.subscribe();

        let id = manager
            .create_session("Test".to_string(), std::env::temp_dir(), None)
            .unwrap();

        // Consume create event
        let _ = rx.try_recv();

        manager
            .set_session_group(id, Some("backend".to_string()), Some("#00ff00".to_string()))
            .unwrap();

        let event = rx.try_recv().unwrap();
        assert!(matches!(
            event,
            CodirigentEvent::SessionGroupChanged {
                id: changed_id,
                group: Some(g),
                color: Some(c)
            } if changed_id == id && g == "backend" && c == "#00ff00"
        ));
    }

    #[test]
    fn test_set_session_group_nonexistent() {
        let manager = create_manager();

        let result = manager.set_session_group(SessionId(999), None, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_with_session_state() {
        let manager = create_manager();

        let id = manager
            .create_session("Test".to_string(), std::env::temp_dir(), None)
            .unwrap();

        let session_id = manager.with_session_state(id, |state| state.id());
        assert_eq!(session_id, Some(id));
    }

    #[test]
    fn test_with_session_state_mut() {
        let manager = create_manager();

        let id = manager
            .create_session("Test".to_string(), std::env::temp_dir(), None)
            .unwrap();

        manager.with_session_state_mut(id, |state| {
            state.set_status(SessionStatus::Working);
        });

        let session = manager.get_session(id).unwrap();
        assert_eq!(session.status, SessionStatus::Working);
    }

    #[test]
    fn test_session_exists() {
        let manager = create_manager();

        let id = manager
            .create_session("Test".to_string(), std::env::temp_dir(), None)
            .unwrap();

        assert!(manager.session_exists(id));
        assert!(!manager.session_exists(SessionId(999)));
    }

    #[test]
    fn test_session_ids() {
        let manager = create_manager();

        let id1 = manager
            .create_session("Session 1".to_string(), std::env::temp_dir(), None)
            .unwrap();
        let id2 = manager
            .create_session("Session 2".to_string(), std::env::temp_dir(), None)
            .unwrap();

        let ids = manager.session_ids();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&id1));
        assert!(ids.contains(&id2));
    }

    #[test]
    fn test_get_child_pid() {
        let manager = create_manager();

        let id = manager
            .create_session("Test".to_string(), std::env::temp_dir(), None)
            .unwrap();

        let pid = manager.get_child_pid(id).unwrap();
        assert!(pid > 0);
    }

    #[test]
    fn test_get_child_pid_nonexistent() {
        let manager = create_manager();
        assert!(manager.get_child_pid(SessionId(999)).is_none());
    }

    #[tokio::test]
    async fn test_try_drain_output() {
        let manager = create_manager();

        let id = manager
            .create_session("Test".to_string(), std::env::temp_dir(), None)
            .unwrap();

        // Send a command
        manager.send_input(id, b"echo test_drain\n").unwrap();

        // Wait for output
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        // Drain output
        let output = manager.try_drain_output(id);

        // Should have some output (even if it's just the prompt)
        // Note: output may or may not contain our echo depending on timing
        // The important thing is that the drain works without blocking
        let _ = output;
    }

    #[tokio::test]
    async fn test_send_input_preserves_command_order() {
        let manager = create_manager();

        let id = manager
            .create_session(
                "Test".to_string(),
                std::env::temp_dir(),
                deterministic_test_shell(),
            )
            .unwrap();

        let ready_marker = format!("phase1_ready_{}", std::process::id());
        manager
            .send_input(id, format!("echo {ready_marker}\n").as_bytes())
            .unwrap();

        let ready_deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut ready_output = String::new();
        while tokio::time::Instant::now() < ready_deadline {
            if let Some(output) = manager.try_drain_output(id) {
                ready_output.push_str(&String::from_utf8_lossy(&output));
                if ready_output.contains(&ready_marker) {
                    break;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        assert!(
            ready_output.contains(&ready_marker),
            "expected shell readiness marker before order assertions: {ready_output}"
        );

        manager.send_input(id, b"echo phase1_order_a\n").unwrap();
        manager.send_input(id, b"echo phase1_order_b\n").unwrap();
        manager.send_input(id, b"echo phase1_order_c\n").unwrap();

        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut combined = String::new();

        while tokio::time::Instant::now() < deadline {
            if let Some(output) = manager.try_drain_output(id) {
                combined.push_str(&String::from_utf8_lossy(&output));
                if combined.contains("phase1_order_a")
                    && combined.contains("phase1_order_b")
                    && combined.contains("phase1_order_c")
                {
                    break;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }

        let first = combined.find("phase1_order_a").unwrap_or(usize::MAX);
        let second = combined.find("phase1_order_b").unwrap_or(usize::MAX);
        let third = combined.find("phase1_order_c").unwrap_or(usize::MAX);

        assert!(
            first < second,
            "expected phase1_order_a before phase1_order_b in output: {combined}"
        );
        assert!(
            second < third,
            "expected phase1_order_b before phase1_order_c in output: {combined}"
        );
    }

    #[test]
    fn test_try_drain_output_nonexistent() {
        let manager = create_manager();
        assert!(manager.try_drain_output(SessionId(999)).is_none());
    }

    #[test]
    fn test_try_drain_output_empty() {
        let manager = create_manager();

        let id = manager
            .create_session("Test".to_string(), std::env::temp_dir(), None)
            .unwrap();

        // Drain immediately - might be empty or have shell prompt
        // Just verify it doesn't panic or block
        let _ = manager.try_drain_output(id);
    }

    #[test]
    fn test_sessions_with_pending_output_drains_ready_set() {
        let manager = create_manager();
        let id = SessionId(42);
        let (state, tx, _temp) = create_manual_session_state(id);
        manager.lock_sessions().insert(id, state);

        tx.try_send(b"hello".to_vec()).unwrap();
        manager.lock_pending_output_sessions().insert(id);

        let pending = manager.sessions_with_pending_output();
        assert_eq!(pending.len(), 1);
        assert!(pending.contains(&id));
        assert!(manager.sessions_with_pending_output().is_empty());

        let drained = manager.try_drain_output_bounded(id, 4, 1024).unwrap();
        assert_eq!(drained.data, b"hello".to_vec());
        assert!(!drained.has_more);
    }

    #[test]
    fn test_try_drain_output_bounded_requeues_when_more_output_remains() {
        let manager = create_manager();
        let id = SessionId(43);
        let (state, tx, _temp) = create_manual_session_state(id);
        manager.lock_sessions().insert(id, state);

        tx.try_send(b"hello".to_vec()).unwrap();
        tx.try_send(b"world".to_vec()).unwrap();
        manager.lock_pending_output_sessions().insert(id);

        let drained = manager.try_drain_output_bounded(id, 1, 1024).unwrap();
        assert_eq!(drained.data, b"hello".to_vec());
        assert!(drained.has_more);

        let pending = manager.sessions_with_pending_output();
        assert_eq!(pending.len(), 1);
        assert!(pending.contains(&id));

        let drained = manager.try_drain_output_bounded(id, 2, 1024).unwrap();
        assert_eq!(drained.data, b"world".to_vec());
        assert!(!drained.has_more);
        assert!(manager.sessions_with_pending_output().is_empty());
    }

    #[test]
    fn test_close_session_clears_pending_output_ready_flag() {
        let manager = create_manager();
        let id = SessionId(44);
        let (state, _tx, _temp) = create_manual_session_state(id);
        manager.lock_sessions().insert(id, state);
        manager.lock_pending_output_sessions().insert(id);

        manager.close_session(id).unwrap();

        assert!(manager.sessions_with_pending_output().is_empty());
    }

    #[test]
    fn test_mark_output_pending_ignores_unknown_session() {
        let manager = create_manager();

        manager.mark_output_pending(SessionId(999));

        assert!(manager.sessions_with_pending_output().is_empty());
    }

    #[test]
    fn test_session_increments_ids() {
        let manager = create_manager();

        let id1 = manager
            .create_session("1".to_string(), std::env::temp_dir(), None)
            .unwrap();
        let id2 = manager
            .create_session("2".to_string(), std::env::temp_dir(), None)
            .unwrap();
        let id3 = manager
            .create_session("3".to_string(), std::env::temp_dir(), None)
            .unwrap();

        assert_eq!(id1.0 + 1, id2.0);
        assert_eq!(id2.0 + 1, id3.0);
    }

    #[test]
    fn test_create_session_with_nonexistent_working_dir() {
        let manager = create_manager();
        let temp = tempfile::tempdir().unwrap();
        let missing = temp.path().join("missing-working-dir");

        let result = manager.create_session("Test".to_string(), missing, None);

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("does not exist"));
    }

    #[test]
    fn test_create_session_with_file_as_working_dir() {
        use std::io::Write;

        let manager = create_manager();

        // Create a temporary file (not a directory)
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join(format!("codirigent_test_file_{}", std::process::id()));
        {
            let mut file = std::fs::File::create(&temp_file).unwrap();
            file.write_all(b"test").unwrap();
        }

        let result = manager.create_session("Test".to_string(), temp_file.clone(), None);

        // Clean up
        let _ = std::fs::remove_file(&temp_file);

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not a directory"));
    }

    #[test]
    fn test_update_context_usage() {
        let manager = create_manager();

        let id = manager
            .create_session("Test".to_string(), std::env::temp_dir(), None)
            .unwrap();

        assert!(manager.get_session(id).unwrap().context_usage.is_none());

        manager.update_context_usage(id, Some(0.65));
        let session = manager.get_session(id).unwrap();
        assert!((session.context_usage.unwrap() - 0.65).abs() < f32::EPSILON);

        manager.update_context_usage(id, Some(0.85));
        let session = manager.get_session(id).unwrap();
        assert!((session.context_usage.unwrap() - 0.85).abs() < f32::EPSILON);

        manager.update_context_usage(id, None);
        assert!(manager.get_session(id).unwrap().context_usage.is_none());
    }

    #[test]
    fn test_update_context_usage_nonexistent() {
        let manager = create_manager();
        manager.update_context_usage(SessionId(999), Some(0.5));
    }

    #[test]
    fn test_take_update_receiver_returns_some_then_none() {
        let manager = create_manager();
        let rx1 = manager.take_update_receiver();
        assert!(rx1.is_some(), "first call should return Some");
        let rx2 = manager.take_update_receiver();
        assert!(rx2.is_none(), "second call should return None");
    }

    #[test]
    fn test_update_sender_returns_working_sender() {
        let manager = create_manager();
        let tx = manager.update_sender();
        // Should be able to send without error (receiver still held by manager)
        tx.try_send(SessionUpdate::OutputReady {
            session_id: SessionId(1),
        })
        .expect("send should succeed while receiver exists");
    }
}
