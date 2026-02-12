//! Helper utilities for integration tests.

use codirigent_core::{
    CodirigentEvent, DefaultEventBus, EventBus, FileStorageService,
    SessionId, SessionManager,
};
use codirigent_detector::InputDetector;
use codirigent_session::DefaultSessionManager;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tempfile::TempDir;

/// Test fixture providing a complete Codirigent environment.
pub struct TestFixture {
    /// Temporary directory for test data.
    pub temp_dir: TempDir,
    /// Event bus for communication.
    pub event_bus: Arc<DefaultEventBus>,
    /// Session manager.
    pub session_manager: Arc<Mutex<DefaultSessionManager>>,
    /// Input detector.
    pub detector: Arc<Mutex<InputDetector>>,
    /// Storage service.
    pub storage: FileStorageService,
}

impl TestFixture {
    /// Create a new test fixture with all services initialized.
    pub fn new() -> anyhow::Result<Self> {
        let temp_dir = TempDir::new()?;
        let event_bus = Arc::new(DefaultEventBus::new(16));
        let storage = FileStorageService::new(temp_dir.path())?;

        let session_manager = Arc::new(Mutex::new(
            DefaultSessionManager::new(event_bus.clone())
        ));

        let detector = Arc::new(Mutex::new(
            InputDetector::new(Default::default(), event_bus.clone())
        ));

        Ok(Self {
            temp_dir,
            event_bus,
            session_manager,
            detector,
            storage,
        })
    }

    /// Create a new session in this fixture.
    pub fn create_session(&self, name: &str) -> anyhow::Result<SessionId> {
        let manager = self.session_manager.lock().unwrap();
        let id = manager.create_session(name.to_string(), self.temp_dir.path().to_path_buf(), None)?;
        Ok(id)
    }

    /// Wait for a specific event with timeout.
    pub async fn wait_for_event(
        &self,
        predicate: impl Fn(&CodirigentEvent) -> bool,
        timeout_ms: u64,
    ) -> Option<CodirigentEvent> {
        let mut rx = self.event_bus.subscribe();
        let deadline = std::time::Instant::now()
            + std::time::Duration::from_millis(timeout_ms);

        while std::time::Instant::now() < deadline {
            if let Ok(event) = tokio::time::timeout(
                std::time::Duration::from_millis(100),
                rx.recv()
            ).await {
                if let Ok(event) = event {
                    if predicate(&event) {
                        return Some(event);
                    }
                }
            }
        }
        None
    }

    /// Get the project path for this fixture.
    pub fn project_path(&self) -> &Path {
        self.temp_dir.path()
    }
}
