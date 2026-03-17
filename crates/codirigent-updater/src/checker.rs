//! GitHub Releases API polling and version comparison.

use serde::{Deserialize, Serialize};

/// Information about an available update.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpdateInfo {
    /// The new version.
    pub version: semver::Version,
    /// URL to the GitHub release page.
    pub release_url: String,
    /// Direct download URL for the platform artifact.
    pub asset_url: String,
    /// Direct download URL for checksums-sha256.txt.
    pub checksum_url: String,
}
