//! Update state machine and orchestration.

use crate::checker::UpdateInfo;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
pub struct UpdateService;
