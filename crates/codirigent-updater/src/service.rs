//! Update state machine and orchestration.
//!
//! The [`UpdateService`] manages the full lifecycle of an update: checking for
//! newer releases, downloading and verifying artifacts, and applying them.
//! State transitions are communicated via the [`EventBus`].

use crate::checker::{self, UpdateInfo};
use crate::downloader;
use crate::state::{self, StagedUpdateState};
use codirigent_core::{CodirigentEvent, EventBus};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

/// Interval between automatic update checks (24 hours).
const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

/// Current state of the update process.
#[derive(Debug, Clone, PartialEq)]
pub enum UpdateState {
    /// No update activity.
    Idle,
    /// Checking GitHub for a new release.
    Checking,
    /// A newer version is available.
    UpdateAvailable(UpdateInfo),
    /// Downloading the update artifact.
    Downloading {
        /// Download progress percentage (0-100).
        percent: u8,
    },
    /// Download complete, ready to apply.
    Staged(StagedUpdate),
    /// Applying the update (app is about to quit).
    Applying,
}

/// A downloaded update ready to apply.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StagedUpdate {
    /// The version of the staged update.
    pub version: semver::Version,
    /// Path to the downloaded artifact.
    pub artifact_path: PathBuf,
    /// URL to the GitHub release page.
    pub release_url: String,
    /// Expected SHA256 hash of the artifact (for re-verification before apply).
    pub expected_sha256: String,
}

/// Orchestrates update checking, downloading, and applying.
pub struct UpdateService {
    current_version: semver::Version,
    event_bus: Arc<dyn EventBus>,
    state: Arc<Mutex<UpdateState>>,
    client: reqwest::Client,
    download_cancel: Arc<Mutex<CancellationToken>>,
}

impl UpdateService {
    /// Create a new `UpdateService`.
    ///
    /// # Arguments
    ///
    /// * `current_version` - The currently running version string (e.g. "0.1.0").
    /// * `event_bus` - The event bus for publishing update events.
    ///
    /// # Errors
    ///
    /// Returns an error if `current_version` is not valid semver.
    pub fn new(current_version: &str, event_bus: Arc<dyn EventBus>) -> anyhow::Result<Self> {
        let version: semver::Version = current_version
            .parse()
            .map_err(|e| anyhow::anyhow!("Invalid current version '{}': {}", current_version, e))?;

        Ok(Self {
            current_version: version,
            event_bus,
            state: Arc::new(Mutex::new(UpdateState::Idle)),
            client: reqwest::Client::new(),
            download_cancel: Arc::new(Mutex::new(CancellationToken::new())),
        })
    }

    /// Get the current update state.
    pub fn state(&self) -> UpdateState {
        self.state.lock().unwrap().clone()
    }

    /// Start a background update check.
    ///
    /// Spawns a tokio task that:
    /// 1. Loads persistent state
    /// 2. Detects post-update launch (version changed since last known)
    /// 3. Handles stale staged updates (artifact missing, version already applied)
    /// 4. Restores a valid staged update if present
    /// 5. Checks for updates if 24h have elapsed since last check
    /// 6. Schedules periodic checks every 24h
    pub fn start_background_check(&self) {
        let version = self.current_version.clone();
        let client = self.client.clone();
        let event_bus = self.event_bus.clone();
        let state = self.state.clone();

        tokio::spawn(async move {
            // 1. Load persistent state.
            let mut persistent = match state::load_state() {
                Ok(s) => s,
                Err(e) => {
                    warn!("Failed to load update persistent state: {e}");
                    state::UpdatePersistentState::default()
                }
            };

            // 2. Detect post-update launch: if the running version differs from
            //    last_known_version, the user just updated.
            if let Some(ref last_known) = persistent.last_known_version {
                if last_known != &version.to_string() {
                    info!(
                        last_known = %last_known,
                        current = %version,
                        "Post-update launch detected — clearing staged update"
                    );
                    // Clear any staged update from the old version.
                    if let Some(ref staged) = persistent.staged_update {
                        let _ = std::fs::remove_file(&staged.artifact_path);
                    }
                    persistent.staged_update = None;
                    persistent.last_known_version = Some(version.to_string());
                    if let Err(e) = state::save_state(&persistent) {
                        warn!("Failed to save update state after post-update clear: {e}");
                    }
                }
            } else {
                // First launch — record the current version.
                persistent.last_known_version = Some(version.to_string());
                if let Err(e) = state::save_state(&persistent) {
                    warn!("Failed to save initial version: {e}");
                }
            }

            // 3. Handle stale staged update.
            if let Some(ref staged) = persistent.staged_update {
                let staged_version: Option<semver::Version> = staged.version.parse().ok();

                if !staged.artifact_path.exists() {
                    // Artifact is gone — clear staged state.
                    info!(
                        path = %staged.artifact_path.display(),
                        "Staged artifact missing — clearing stale staged update"
                    );
                    persistent.staged_update = None;
                    if let Err(e) = state::save_state(&persistent) {
                        warn!("Failed to save state after clearing missing artifact: {e}");
                    }
                } else if staged_version.as_ref() == Some(&version) {
                    // Already running the staged version — clear it.
                    info!(
                        version = %version,
                        "Already running staged version — clearing and deleting artifact"
                    );
                    let _ = std::fs::remove_file(&staged.artifact_path);
                    persistent.staged_update = None;
                    if let Err(e) = state::save_state(&persistent) {
                        warn!("Failed to save state after clearing same-version staged: {e}");
                    }
                }
            }

            // 4. Auto-apply valid staged update on startup.
            //    The user already acknowledged this update (clicked "Later" in a
            //    previous session), so apply it now while no sessions are active.
            //    The helper script waits for this process to exit, swaps the app,
            //    and relaunches.
            if let Some(staged) = persistent.staged_update.clone() {
                if let Ok(staged_ver) = staged.version.parse::<semver::Version>() {
                    if staged.artifact_path.exists() && staged_ver > version {
                        info!(
                            staged_version = %staged_ver,
                            artifact = %staged.artifact_path.display(),
                            "Auto-applying staged update on startup"
                        );

                        // Verify SHA256 if available.
                        let mut verified = true;
                        if !staged.expected_sha256.is_empty() {
                            match downloader::verify_sha256(
                                &staged.artifact_path,
                                &staged.expected_sha256,
                            ) {
                                Ok(true) => {}
                                Ok(false) => {
                                    warn!("SHA256 mismatch on staged artifact — clearing");
                                    let _ = std::fs::remove_file(&staged.artifact_path);
                                    persistent.staged_update = None;
                                    let _ = state::save_state(&persistent);
                                    verified = false;
                                }
                                Err(e) => {
                                    warn!(
                                        "SHA256 verification error: {e} — clearing staged update"
                                    );
                                    let _ = std::fs::remove_file(&staged.artifact_path);
                                    persistent.staged_update = None;
                                    let _ = state::save_state(&persistent);
                                    verified = false;
                                }
                            }
                        }

                        // If staged update is still valid after verification, apply it.
                        if verified {
                            let pid = std::process::id();
                            match crate::platform::apply_update(&staged.artifact_path, pid) {
                                Ok(()) => {
                                    *state.lock().unwrap() = UpdateState::Applying;
                                    event_bus.publish(CodirigentEvent::UpdateApplyingOnStartup);
                                    return;
                                }
                                Err(e) => {
                                    warn!("Failed to auto-apply staged update: {e}");
                                    let _ = std::fs::remove_file(&staged.artifact_path);
                                    persistent.staged_update = None;
                                    let _ = state::save_state(&persistent);
                                }
                            }
                        }
                    }
                }
            }

            // 5. Check if enough time has elapsed since the last check.
            let should_check_now = match persistent.last_check_timestamp {
                Some(last) => {
                    let elapsed = chrono::Utc::now().signed_duration_since(last);
                    elapsed.num_seconds() >= CHECK_INTERVAL.as_secs() as i64
                }
                None => true, // Never checked before.
            };

            if should_check_now {
                do_check(&version, &client, &event_bus, &state).await;
            }

            // 6. Schedule periodic checks every 24h.
            let mut interval = tokio::time::interval(CHECK_INTERVAL);
            // The first tick fires immediately — skip it since we just checked.
            interval.tick().await;

            loop {
                interval.tick().await;
                do_check(&version, &client, &event_bus, &state).await;
            }
        });
    }

    /// Start downloading the available update.
    ///
    /// Only works when the current state is `UpdateAvailable`. Spawns a tokio
    /// task that downloads and verifies the artifact, transitions through
    /// `Downloading` to `Staged`, and publishes appropriate events.
    pub fn start_download(&self) {
        let state = self.state.clone();
        let event_bus = self.event_bus.clone();
        let client = self.client.clone();
        let current_version = self.current_version.clone();
        let cancel_store = self.download_cancel.clone();

        // Create a fresh cancellation token.
        let token = CancellationToken::new();
        *cancel_store.lock().unwrap() = token.clone();

        tokio::spawn(async move {
            // Extract UpdateInfo — only proceed from UpdateAvailable.
            let info = {
                let guard = state.lock().unwrap();
                match &*guard {
                    UpdateState::UpdateAvailable(info) => info.clone(),
                    other => {
                        warn!(
                            state = ?other,
                            "start_download called in wrong state — expected UpdateAvailable"
                        );
                        return;
                    }
                }
            };

            // Transition to Downloading.
            *state.lock().unwrap() = UpdateState::Downloading { percent: 0 };

            // Determine download directory.
            let dest_dir = match state::cache_dir() {
                Some(d) => d.join("updates"),
                None => {
                    let msg = "Could not determine cache directory for download";
                    error!(msg);
                    event_bus.publish(CodirigentEvent::UpdateFailed {
                        error: msg.to_string(),
                    });
                    *state.lock().unwrap() = UpdateState::UpdateAvailable(info);
                    return;
                }
            };

            // Clean up old staged artifacts in the download directory.
            if dest_dir.exists() {
                if let Ok(entries) = std::fs::read_dir(&dest_dir) {
                    for entry in entries.flatten() {
                        let _ = std::fs::remove_file(entry.path());
                    }
                }
            }

            // Progress callback — publishes events and updates state.
            let state_for_progress = state.clone();
            let bus_for_progress = event_bus.clone();
            let on_progress = move |percent: u8| {
                *state_for_progress.lock().unwrap() = UpdateState::Downloading { percent };
                bus_for_progress.publish(CodirigentEvent::UpdateDownloadProgress { percent });
            };

            let user_agent = format!("codirigent/{current_version}");

            // Download and verify, respecting cancellation.
            let result = tokio::select! {
                _ = token.cancelled() => {
                    info!("Download cancelled by user");
                    *state.lock().unwrap() = UpdateState::UpdateAvailable(info);
                    return;
                }
                result = downloader::download_and_verify(
                    &client,
                    &info.asset_url,
                    &info.checksum_url,
                    &dest_dir,
                    &user_agent,
                    on_progress,
                ) => result
            };

            match result {
                Ok((artifact_path, expected_sha256)) => {
                    let staged = StagedUpdate {
                        version: info.version.clone(),
                        artifact_path: artifact_path.clone(),
                        release_url: info.release_url.clone(),
                        expected_sha256,
                    };

                    // Persist staged update for crash recovery.
                    let mut persistent = state::load_state().unwrap_or_default();
                    persistent.staged_update = Some(StagedUpdateState {
                        version: info.version.to_string(),
                        artifact_path,
                        release_url: info.release_url.clone(),
                        expected_sha256: staged.expected_sha256.clone(),
                    });
                    if let Err(e) = state::save_state(&persistent) {
                        warn!("Failed to persist staged update: {e}");
                    }

                    *state.lock().unwrap() = UpdateState::Staged(staged);
                    event_bus.publish(CodirigentEvent::UpdateReadyToApply);

                    info!(
                        version = %info.version,
                        "Update downloaded and staged successfully"
                    );
                }
                Err(e) => {
                    error!("Download failed: {e:#}");
                    event_bus.publish(CodirigentEvent::UpdateFailed {
                        error: format!("{e:#}"),
                    });
                    *state.lock().unwrap() = UpdateState::UpdateAvailable(info);
                }
            }
        });
    }

    /// Apply a staged update.
    ///
    /// Only works from the `Staged` state. Re-verifies the artifact SHA256,
    /// then delegates to the platform-specific apply logic.
    ///
    /// # Errors
    ///
    /// Returns an error if the state is not `Staged`, the SHA256 verification
    /// fails, or the platform apply fails.
    pub fn apply(&self) -> anyhow::Result<()> {
        let staged = {
            let guard = self.state.lock().unwrap();
            match &*guard {
                UpdateState::Staged(s) => s.clone(),
                other => {
                    anyhow::bail!(
                        "Cannot apply update: expected Staged state, got {:?}",
                        std::mem::discriminant(other)
                    );
                }
            }
        };

        // Re-verify SHA256 before applying (unless the hash is empty, which
        // means it was restored from persistent state without hash).
        if !staged.expected_sha256.is_empty() {
            let valid = downloader::verify_sha256(&staged.artifact_path, &staged.expected_sha256)
                .map_err(|e| anyhow::anyhow!("SHA256 re-verification failed: {e:#}"))?;

            if !valid {
                // Delete the corrupt artifact and clear state.
                let _ = std::fs::remove_file(&staged.artifact_path);
                *self.state.lock().unwrap() = UpdateState::Idle;
                anyhow::bail!("SHA256 mismatch on re-verification — artifact may be corrupt");
            }
        }

        // Transition to Applying.
        *self.state.lock().unwrap() = UpdateState::Applying;

        let current_pid = std::process::id();
        crate::platform::apply_update(&staged.artifact_path, current_pid)?;

        Ok(())
    }

    /// Cancel an in-progress download.
    ///
    /// If a download is running, cancels it via the cancellation token and
    /// transitions the state back to `UpdateAvailable`.
    pub fn cancel_download(&self) {
        let token = self.download_cancel.lock().unwrap().clone();
        token.cancel();
        // The download task will handle the state transition when it observes
        // the cancellation.
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Perform a single update check against the GitHub Releases API.
///
/// On success: updates state to `UpdateAvailable` and publishes event.
/// On failure: publishes `UpdateFailed` and returns state to `Idle`.
/// Saves `last_check_timestamp` on successful API call regardless of result.
async fn do_check(
    version: &semver::Version,
    client: &reqwest::Client,
    event_bus: &Arc<dyn EventBus>,
    state: &Arc<Mutex<UpdateState>>,
) {
    info!("Checking for updates...");
    *state.lock().unwrap() = UpdateState::Checking;

    match checker::check_for_update(version, client).await {
        Ok(Some(info)) => {
            info!(
                new_version = %info.version,
                "Update available"
            );
            event_bus.publish(CodirigentEvent::UpdateAvailable {
                version: info.version.to_string(),
                release_url: info.release_url.clone(),
            });
            *state.lock().unwrap() = UpdateState::UpdateAvailable(info);
        }
        Ok(None) => {
            info!("Already up to date");
            *state.lock().unwrap() = UpdateState::Idle;
        }
        Err(e) => {
            error!("Update check failed: {e:#}");
            event_bus.publish(CodirigentEvent::UpdateFailed {
                error: format!("Update check failed: {e:#}"),
            });
            *state.lock().unwrap() = UpdateState::Idle;
        }
    }

    // Save the last check timestamp regardless of result.
    let mut persistent = state::load_state().unwrap_or_default();
    persistent.last_check_timestamp = Some(chrono::Utc::now());
    if let Err(e) = state::save_state(&persistent) {
        warn!("Failed to save last_check_timestamp: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codirigent_core::CodirigentEvent;
    use std::sync::Mutex as StdMutex;
    use tokio::sync::broadcast;

    /// A minimal EventBus implementation for testing.
    struct TestEventBus {
        tx: broadcast::Sender<CodirigentEvent>,
        events: Arc<StdMutex<Vec<CodirigentEvent>>>,
    }

    impl TestEventBus {
        fn new() -> Self {
            let (tx, _) = broadcast::channel(64);
            Self {
                tx,
                events: Arc::new(StdMutex::new(Vec::new())),
            }
        }

        #[allow(dead_code)]
        fn events(&self) -> Vec<CodirigentEvent> {
            self.events.lock().unwrap().clone()
        }
    }

    impl EventBus for TestEventBus {
        fn subscribe(&self) -> broadcast::Receiver<CodirigentEvent> {
            self.tx.subscribe()
        }

        fn publish(&self, event: CodirigentEvent) {
            self.events.lock().unwrap().push(event.clone());
            let _ = self.tx.send(event);
        }
    }

    #[test]
    fn new_parses_valid_version() {
        let bus = Arc::new(TestEventBus::new());
        let svc = UpdateService::new("0.1.0", bus).unwrap();
        assert_eq!(
            svc.current_version,
            "0.1.0".parse::<semver::Version>().unwrap()
        );
    }

    #[test]
    fn new_parses_prerelease_version() {
        let bus = Arc::new(TestEventBus::new());
        let svc = UpdateService::new("0.1.0-alpha.1", bus).unwrap();
        assert_eq!(
            svc.current_version,
            "0.1.0-alpha.1".parse::<semver::Version>().unwrap()
        );
    }

    #[test]
    fn new_rejects_invalid_version() {
        let bus = Arc::new(TestEventBus::new());
        let result = UpdateService::new("not-a-version", bus);
        assert!(result.is_err());
    }

    #[test]
    fn initial_state_is_idle() {
        let bus = Arc::new(TestEventBus::new());
        let svc = UpdateService::new("0.1.0", bus).unwrap();
        assert_eq!(svc.state(), UpdateState::Idle);
    }

    #[test]
    fn apply_rejects_non_staged_state() {
        let bus = Arc::new(TestEventBus::new());
        let svc = UpdateService::new("0.1.0", bus).unwrap();
        let result = svc.apply();
        assert!(result.is_err());
        assert!(
            format!("{}", result.unwrap_err()).contains("Staged"),
            "Error should mention expected Staged state"
        );
    }

    #[test]
    fn cancel_download_does_not_panic_when_idle() {
        let bus = Arc::new(TestEventBus::new());
        let svc = UpdateService::new("0.1.0", bus).unwrap();
        // Cancelling when nothing is downloading should not panic.
        svc.cancel_download();
    }

    #[test]
    fn state_clone_returns_current() {
        let bus = Arc::new(TestEventBus::new());
        let svc = UpdateService::new("0.1.0", bus).unwrap();

        // Manually set state to verify the accessor works.
        let info = UpdateInfo {
            version: "0.2.0".parse().unwrap(),
            release_url: "https://example.com/release".to_string(),
            asset_url: "https://example.com/asset.dmg".to_string(),
            checksum_url: "https://example.com/checksums.txt".to_string(),
        };
        *svc.state.lock().unwrap() = UpdateState::UpdateAvailable(info.clone());
        assert_eq!(svc.state(), UpdateState::UpdateAvailable(info));
    }
}
