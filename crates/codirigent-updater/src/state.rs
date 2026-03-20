//! Persistent state for the update checker.
//!
//! Tracks the last-known version, the last time we checked for updates, and
//! any staged (downloaded but not yet applied) update. State is stored as a
//! JSON file in the platform config directory so it survives restarts.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Persistent state for the auto-updater.
///
/// Serialized to `update-state.json` in the platform config directory. All
/// fields are optional so the file can evolve without breaking older installs.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdatePersistentState {
    /// The last version we know about (may be newer than the running version
    /// if an update was found but not yet applied).
    pub last_known_version: Option<String>,

    /// Timestamp of the most recent successful check against the GitHub API.
    pub last_check_timestamp: Option<DateTime<Utc>>,

    /// A discovered update that has not been downloaded yet.
    pub available_update: Option<AvailableUpdateState>,

    /// A downloaded update that is ready to apply on next restart.
    pub staged_update: Option<StagedUpdateState>,
}

/// Metadata for an available update that should be restored on restart.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AvailableUpdateState {
    /// Semantic version string of the available release.
    pub version: String,

    /// URL to the GitHub release page.
    pub release_url: String,

    /// Direct download URL for the platform artifact.
    pub asset_url: String,

    /// Direct download URL for checksums-sha256.txt.
    pub checksum_url: String,
}

/// Metadata for a staged (downloaded) update artifact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StagedUpdateState {
    /// Semantic version string of the staged release.
    pub version: String,

    /// Path to the downloaded artifact on disk.
    pub artifact_path: PathBuf,

    /// URL to the GitHub release page (for user-facing links).
    pub release_url: String,

    /// Expected SHA256 hash of the artifact (hex-encoded).
    ///
    /// Persisted so we can re-verify the artifact before applying, even after
    /// a restart. Empty string means the hash was not recorded (e.g. from an
    /// older state file format).
    pub expected_sha256: String,
}

impl Default for StagedUpdateState {
    fn default() -> Self {
        Self {
            version: String::new(),
            artifact_path: PathBuf::new(),
            release_url: String::new(),
            expected_sha256: String::new(),
        }
    }
}

/// Returns the default path for `update-state.json`.
///
/// Uses `dirs::config_dir()` which maps to:
/// - macOS: `~/Library/Application Support/codirigent/update-state.json`
/// - Windows: `{FOLDERID_RoamingAppData}\codirigent\update-state.json`
/// - Linux: `$XDG_CONFIG_HOME/codirigent/update-state.json`
pub fn state_file_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("codirigent").join("update-state.json"))
}

/// Returns the platform cache directory for Codirigent.
///
/// Used for storing downloaded update artifacts. Maps to:
/// - macOS: `~/Library/Caches/codirigent`
/// - Windows: `{FOLDERID_LocalAppData}\codirigent`
/// - Linux: `$XDG_CACHE_HOME/codirigent`
pub fn cache_dir() -> Option<PathBuf> {
    dirs::cache_dir().map(|d| d.join("codirigent"))
}

/// Load the update state from the default location.
///
/// Returns `Ok(Default)` if the file does not exist.
pub fn load_state() -> Result<UpdatePersistentState> {
    let path = state_file_path().context("Could not determine config directory")?;
    load_state_from(&path)
}

/// Save the update state to the default location.
pub fn save_state(state: &UpdatePersistentState) -> Result<()> {
    let path = state_file_path().context("Could not determine config directory")?;
    save_state_to(state, &path)
}

/// Load the update state from an explicit path.
///
/// Returns `Ok(Default)` if the file does not exist.
pub fn load_state_from(path: &Path) -> Result<UpdatePersistentState> {
    if !path.exists() {
        return Ok(UpdatePersistentState::default());
    }
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    serde_json::from_str(&content).with_context(|| format!("Failed to parse {}", path.display()))
}

/// Save the update state to an explicit path.
///
/// Uses an atomic write pattern (PID-scoped temp file + rename) to avoid
/// corruption if two instances write simultaneously.
pub fn save_state_to(state: &UpdatePersistentState, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }

    let json = serde_json::to_string_pretty(state).context("Failed to serialize update state")?;

    // Atomic write: write to a PID-scoped temp file then rename.
    // Using the process ID prevents two concurrent Codirigent instances from
    // clobbering each other's temp file during simultaneous startup.
    let tmp = path.with_file_name(format!(".update-state-{}.tmp", std::process::id()));
    std::fs::write(&tmp, &json).with_context(|| format!("Failed to write {}", tmp.display()))?;
    std::fs::rename(&tmp, path)
        .with_context(|| format!("Failed to rename {} to {}", tmp.display(), path.display()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn default_state_round_trip() {
        let state = UpdatePersistentState::default();
        let json = serde_json::to_string(&state).expect("serialize default");
        let restored: UpdatePersistentState =
            serde_json::from_str(&json).expect("deserialize default");
        assert_eq!(state, restored);
    }

    #[test]
    fn full_state_round_trip() {
        let state = UpdatePersistentState {
            last_known_version: Some("0.2.0".to_string()),
            last_check_timestamp: Some(Utc.with_ymd_and_hms(2026, 3, 15, 10, 30, 0).unwrap()),
            available_update: Some(AvailableUpdateState {
                version: "0.2.0".to_string(),
                release_url: "https://github.com/oso95/Codirigent/releases/tag/v0.2.0".to_string(),
                asset_url: "https://example.com/codirigent-v0.2.0.dmg".to_string(),
                checksum_url: "https://example.com/checksums-sha256.txt".to_string(),
            }),
            staged_update: Some(StagedUpdateState {
                version: "0.2.0".to_string(),
                artifact_path: PathBuf::from("/tmp/codirigent-0.2.0.dmg"),
                release_url: "https://github.com/oso95/Codirigent/releases/tag/v0.2.0".to_string(),
                expected_sha256: "abc123def456".to_string(),
            }),
        };
        let json = serde_json::to_string_pretty(&state).expect("serialize full");
        let restored: UpdatePersistentState =
            serde_json::from_str(&json).expect("deserialize full");
        assert_eq!(state, restored);
    }

    #[test]
    fn load_save_to_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("update-state.json");

        let state = UpdatePersistentState {
            last_known_version: Some("1.0.0".to_string()),
            last_check_timestamp: Some(Utc::now()),
            available_update: Some(AvailableUpdateState {
                version: "1.1.0".to_string(),
                release_url: "https://example.com/release".to_string(),
                asset_url: "https://example.com/app.dmg".to_string(),
                checksum_url: "https://example.com/checksums.txt".to_string(),
            }),
            staged_update: None,
        };

        save_state_to(&state, &path).expect("save");
        let loaded = load_state_from(&path).expect("load");
        assert_eq!(state, loaded);
    }

    #[test]
    fn load_missing_file_returns_default() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("nonexistent.json");

        let loaded = load_state_from(&path).expect("load missing");
        assert_eq!(loaded, UpdatePersistentState::default());
    }

    #[test]
    fn state_file_path_returns_some() {
        // On macOS, Windows, and Linux with a home directory, this should return Some.
        let path = state_file_path();
        assert!(
            path.is_some(),
            "state_file_path() should return Some on supported platforms"
        );
        let path = path.unwrap();
        assert!(path.ends_with("codirigent/update-state.json"));
    }

    #[test]
    fn cache_dir_returns_some() {
        let dir = cache_dir();
        assert!(
            dir.is_some(),
            "cache_dir() should return Some on supported platforms"
        );
        let dir = dir.unwrap();
        assert!(dir.ends_with("codirigent"));
    }

    #[test]
    fn save_creates_parent_directories() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir
            .path()
            .join("nested")
            .join("deep")
            .join("update-state.json");

        let state = UpdatePersistentState::default();
        save_state_to(&state, &path).expect("save with nested dirs");
        assert!(path.exists());
    }

    #[test]
    fn deserialize_with_missing_fields() {
        // Simulate an older file that only has `last_known_version`.
        let json = r#"{"last_known_version": "0.1.0"}"#;
        let state: UpdatePersistentState = serde_json::from_str(json).expect("partial parse");
        assert_eq!(state.last_known_version, Some("0.1.0".to_string()));
        assert_eq!(state.last_check_timestamp, None);
        assert_eq!(state.available_update, None);
        assert_eq!(state.staged_update, None);
    }

    #[test]
    fn deserialize_empty_object() {
        let state: UpdatePersistentState = serde_json::from_str("{}").expect("empty object");
        assert_eq!(state, UpdatePersistentState::default());
    }
}
