//! Auto-save manager for automatic state persistence.
//!
//! This module provides the [`AutoSaveManager`] which handles automatic
//! saving of application state at configured intervals and on events.
//!
//! ## Features
//!
//! - Interval-based auto-save
//! - Event-triggered saves (on status change)
//! - Manual save-now functionality
//! - Graceful shutdown

use crate::events::CodirigentEvent;
use crate::persistence::PersistentState;
use crate::persistence_service::{AutoSaveConfig, PersistenceService};
use crate::traits::EventBus;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};
use tracing::{debug, error, info};

/// Manages automatic state saving.
///
/// The auto-save manager runs background tasks that periodically save
/// application state and optionally save on events like status changes.
///
/// # Example
///
/// ```ignore
/// use codirigent_core::auto_save::AutoSaveManager;
/// use codirigent_core::persistence_service::{AutoSaveConfig, DefaultPersistenceService, PersistenceService};
/// use std::sync::Arc;
/// use std::path::Path;
///
/// let persistence = Arc::new(DefaultPersistenceService::new(Path::new("/project")));
/// let config = AutoSaveConfig::default();
/// let mut manager = AutoSaveManager::new(persistence, config);
///
/// // Start auto-save with event bus
/// manager.start(event_bus);
///
/// // Later, force an immediate save
/// manager.save_now().await.unwrap();
///
/// // Graceful shutdown
/// manager.stop();
/// ```
pub struct AutoSaveManager {
    persistence: Arc<dyn PersistenceService>,
    config: AutoSaveConfig,
    state: Arc<RwLock<PersistentState>>,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
    event_shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl AutoSaveManager {
    /// Create a new auto-save manager.
    ///
    /// # Arguments
    ///
    /// * `persistence` - The persistence service to use for saving
    /// * `config` - Auto-save configuration
    ///
    /// # Example
    ///
    /// ```
    /// use codirigent_core::auto_save::AutoSaveManager;
    /// use codirigent_core::persistence_service::{AutoSaveConfig, DefaultPersistenceService};
    /// use std::sync::Arc;
    /// use std::path::Path;
    ///
    /// let persistence = Arc::new(DefaultPersistenceService::new(Path::new("/tmp")));
    /// let config = AutoSaveConfig::default();
    /// let manager = AutoSaveManager::new(persistence, config);
    /// ```
    pub fn new(persistence: Arc<dyn PersistenceService>, config: AutoSaveConfig) -> Self {
        Self {
            persistence,
            config,
            state: Arc::new(RwLock::new(PersistentState::new())),
            shutdown_tx: None,
            event_shutdown_tx: None,
        }
    }

    /// Create a new auto-save manager with initial state.
    ///
    /// # Arguments
    ///
    /// * `persistence` - The persistence service to use for saving
    /// * `config` - Auto-save configuration
    /// * `initial_state` - Initial state to manage
    pub fn with_initial_state(
        persistence: Arc<dyn PersistenceService>,
        config: AutoSaveConfig,
        initial_state: PersistentState,
    ) -> Self {
        Self {
            persistence,
            config,
            state: Arc::new(RwLock::new(initial_state)),
            shutdown_tx: None,
            event_shutdown_tx: None,
        }
    }

    /// Start the auto-save background tasks.
    ///
    /// This spawns background tasks for:
    /// - Interval-based auto-save (if enabled)
    /// - Event-based auto-save (if on_status_change is enabled)
    ///
    /// # Arguments
    ///
    /// * `event_bus` - Event bus for subscribing to status changes
    pub fn start(&mut self, event_bus: Arc<dyn EventBus>) {
        if !self.config.enabled {
            debug!("Auto-save is disabled, not starting background tasks");
            return;
        }

        self.start_interval_save();
        self.start_event_save(event_bus);
    }

    /// Start only the interval-based auto-save.
    ///
    /// This can be called if you don't want event-based saves.
    pub fn start_interval_save(&mut self) {
        if !self.config.enabled {
            return;
        }

        let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel();
        self.shutdown_tx = Some(shutdown_tx);

        let persistence = Arc::clone(&self.persistence);
        let state = Arc::clone(&self.state);
        let interval_secs = self.config.interval_seconds;

        tokio::spawn(async move {
            let mut save_interval = interval(Duration::from_secs(interval_secs as u64));
            // Skip the first immediate tick
            save_interval.tick().await;

            loop {
                tokio::select! {
                    _ = save_interval.tick() => {
                        let current_state = state.read().await.clone();
                        if let Err(e) = persistence.save_state(&current_state) {
                            error!("Auto-save failed: {}", e);
                        } else {
                            debug!("Auto-saved state");
                        }
                    }
                    _ = &mut shutdown_rx => {
                        info!("Interval auto-save manager shutting down");
                        break;
                    }
                }
            }
        });
    }

    /// Start only the event-based auto-save.
    ///
    /// # Arguments
    ///
    /// * `event_bus` - Event bus for subscribing to events
    pub fn start_event_save(&mut self, event_bus: Arc<dyn EventBus>) {
        if !self.config.enabled || !self.config.on_status_change {
            return;
        }

        let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel();
        self.event_shutdown_tx = Some(shutdown_tx);

        let persistence = Arc::clone(&self.persistence);
        let state = Arc::clone(&self.state);
        let mut rx = event_bus.subscribe();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    event_result = rx.recv() => {
                        match event_result {
                            Ok(event) => {
                                if Self::should_save_on_event(&event) {
                                    let current_state = state.read().await.clone();
                                    if let Err(e) = persistence.save_state(&current_state) {
                                        error!("Event-triggered save failed: {}", e);
                                    } else {
                                        debug!("Event-triggered save completed");
                                    }
                                }
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                                info!("Event bus closed, stopping event-based auto-save");
                                break;
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                                debug!("Event receiver lagged by {} events", n);
                            }
                        }
                    }
                    _ = &mut shutdown_rx => {
                        info!("Event-based auto-save manager shutting down");
                        break;
                    }
                }
            }
        });
    }

    /// Check if an event should trigger a save.
    fn should_save_on_event(event: &CodirigentEvent) -> bool {
        matches!(
            event,
            CodirigentEvent::SessionStatusChanged { .. }
                | CodirigentEvent::SessionCreated { .. }
                | CodirigentEvent::SessionClosed { .. }
        )
    }

    /// Stop the auto-save manager.
    ///
    /// This signals all background tasks to stop. After calling this,
    /// you should call `save_now()` if you want to ensure the final
    /// state is persisted.
    pub fn stop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        if let Some(tx) = self.event_shutdown_tx.take() {
            let _ = tx.send(());
        }
    }

    /// Update the state to be saved.
    ///
    /// # Arguments
    ///
    /// * `state` - The new state to manage
    pub async fn update_state(&self, state: PersistentState) {
        *self.state.write().await = state;
    }

    /// Get a clone of the current state.
    pub async fn get_state(&self) -> PersistentState {
        self.state.read().await.clone()
    }

    /// Force an immediate save.
    ///
    /// # Errors
    ///
    /// Returns an error if the save fails.
    pub async fn save_now(&self) -> anyhow::Result<()> {
        let current_state = self.state.read().await.clone();
        self.persistence.save_state(&current_state)
    }

    /// Check if auto-save is enabled.
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Get the auto-save interval in seconds.
    pub fn interval_seconds(&self) -> u32 {
        self.config.interval_seconds
    }

    /// Check if the manager is running (has active tasks).
    pub fn is_running(&self) -> bool {
        self.shutdown_tx.is_some() || self.event_shutdown_tx.is_some()
    }
}

impl Drop for AutoSaveManager {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::{PersistentSession, PersistentState};
    use crate::persistence_service::DefaultPersistenceService;
    use crate::types::{Session, SessionId, SessionStatus};
    use crate::DefaultEventBus;
    use std::path::PathBuf;
    use tempfile::tempdir;

    /// Helper function to create a persistence service for tests.
    fn create_persistence(path: &std::path::Path) -> Arc<dyn PersistenceService> {
        Arc::new(DefaultPersistenceService::new(path))
    }

    #[test]
    fn test_auto_save_manager_new() {
        let temp = tempdir().unwrap();
        let persistence = create_persistence(temp.path());
        let config = AutoSaveConfig::default();
        let manager = AutoSaveManager::new(persistence, config);

        assert!(manager.is_enabled());
        assert_eq!(manager.interval_seconds(), 30);
        assert!(!manager.is_running());
    }

    #[test]
    fn test_auto_save_manager_with_initial_state() {
        let temp = tempdir().unwrap();
        let persistence = create_persistence(temp.path());
        let config = AutoSaveConfig::default();

        let mut initial_state = PersistentState::new();
        let session = Session::new(SessionId(1), "Test".to_string(), PathBuf::from("/tmp"));
        initial_state.add_session(PersistentSession::from_session(&session));

        let manager =
            AutoSaveManager::with_initial_state(persistence, config, initial_state.clone());

        // State should be set
        let rt = tokio::runtime::Runtime::new().unwrap();
        let state = rt.block_on(manager.get_state());
        assert_eq!(state.sessions.len(), 1);
    }

    #[test]
    fn test_auto_save_manager_disabled() {
        let temp = tempdir().unwrap();
        let persistence = create_persistence(temp.path());
        let config = AutoSaveConfig::disabled();
        let manager = AutoSaveManager::new(persistence, config);

        assert!(!manager.is_enabled());
    }

    #[tokio::test]
    async fn test_update_state() {
        let temp = tempdir().unwrap();
        let persistence = create_persistence(temp.path());
        let config = AutoSaveConfig::default();
        let manager = AutoSaveManager::new(persistence, config);

        let mut state = PersistentState::new();
        let session = Session::new(SessionId(1), "Test".to_string(), PathBuf::from("/tmp"));
        state.add_session(PersistentSession::from_session(&session));

        manager.update_state(state).await;

        let current = manager.get_state().await;
        assert_eq!(current.sessions.len(), 1);
    }

    #[tokio::test]
    async fn test_save_now() {
        let temp = tempdir().unwrap();
        let persistence = create_persistence(temp.path());
        let config = AutoSaveConfig::default();
        let manager = AutoSaveManager::new(Arc::clone(&persistence), config);

        let mut state = PersistentState::new();
        let session = Session::new(SessionId(1), "Test".to_string(), PathBuf::from("/tmp"));
        state.add_session(PersistentSession::from_session(&session));
        manager.update_state(state).await;

        manager.save_now().await.unwrap();

        // Verify state was saved
        let loaded = persistence.load_state().unwrap().unwrap();
        assert_eq!(loaded.sessions.len(), 1);
    }

    #[tokio::test]
    async fn test_start_and_stop() {
        let temp = tempdir().unwrap();
        let persistence = create_persistence(temp.path());
        let config = AutoSaveConfig::with_interval(1); // 1 second for testing
        let mut manager = AutoSaveManager::new(persistence, config);

        let event_bus = Arc::new(DefaultEventBus::new(16));
        manager.start(event_bus);

        assert!(manager.is_running());

        manager.stop();

        // Give time for shutdown
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    #[tokio::test]
    async fn test_start_disabled() {
        let temp = tempdir().unwrap();
        let persistence = create_persistence(temp.path());
        let config = AutoSaveConfig::disabled();
        let mut manager = AutoSaveManager::new(persistence, config);

        let event_bus = Arc::new(DefaultEventBus::new(16));
        manager.start(event_bus);

        // Should not be running since disabled
        assert!(!manager.is_running());
    }

    #[test]
    fn test_should_save_on_event() {
        // Should trigger save
        assert!(AutoSaveManager::should_save_on_event(
            &CodirigentEvent::SessionStatusChanged {
                id: SessionId(1),
                old: SessionStatus::Idle,
                new: SessionStatus::Working,
            }
        ));
        assert!(AutoSaveManager::should_save_on_event(
            &CodirigentEvent::SessionCreated { id: SessionId(1) }
        ));
        assert!(AutoSaveManager::should_save_on_event(
            &CodirigentEvent::SessionClosed { id: SessionId(1) }
        ));

        // Should not trigger save
        assert!(!AutoSaveManager::should_save_on_event(
            &CodirigentEvent::SessionFocused { id: SessionId(1) }
        ));
        assert!(!AutoSaveManager::should_save_on_event(
            &CodirigentEvent::LayoutChanged {
                mode: crate::types::LayoutMode::Single
            }
        ));
    }

    #[tokio::test]
    async fn test_interval_save() {
        let temp = tempdir().unwrap();
        let persistence: Arc<dyn PersistenceService> =
            Arc::new(DefaultPersistenceService::new(temp.path()));
        let config = AutoSaveConfig::with_interval(1); // 1 second for testing
        let mut manager = AutoSaveManager::new(Arc::clone(&persistence), config);

        // Set up state
        let mut state = PersistentState::new();
        let session = Session::new(SessionId(1), "Test".to_string(), PathBuf::from("/tmp"));
        state.add_session(PersistentSession::from_session(&session));
        manager.update_state(state).await;

        // Start only interval save
        manager.start_interval_save();

        // Wait for interval to trigger
        tokio::time::sleep(Duration::from_millis(1200)).await;

        // Verify state was saved
        let loaded = persistence.load_state().unwrap();
        assert!(loaded.is_some());

        manager.stop();
    }

    #[tokio::test]
    async fn test_drop_stops_tasks() {
        let temp = tempdir().unwrap();
        let persistence = create_persistence(temp.path());
        let config = AutoSaveConfig::with_interval(1);

        {
            let mut manager = AutoSaveManager::new(persistence, config);
            let event_bus = Arc::new(DefaultEventBus::new(16));
            manager.start(event_bus);
            assert!(manager.is_running());
            // Drop should call stop
        }

        // Give time for cleanup
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    #[test]
    fn test_auto_save_manager_clone_state() {
        let temp = tempdir().unwrap();
        let persistence = create_persistence(temp.path());
        let config = AutoSaveConfig::default();
        let manager = AutoSaveManager::new(persistence, config.clone());

        // Verify config is as expected
        assert_eq!(manager.interval_seconds(), config.interval_seconds);
    }
}
