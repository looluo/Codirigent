# Auto-Update Notifications Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add automatic update checking, download, and installation so users are notified of new releases and can update with a single click.

**Architecture:** New `codirigent-updater` crate with checker (GitHub API), downloader (artifact + SHA256), and platform-specific apply logic (macOS DMG swap, Windows MSI). Communicates with UI via existing EventBus. Toast notification in workspace view.

**Tech Stack:** reqwest (HTTP), semver (version comparison), sha2 + hex (checksum verification), dirs (platform paths), tokio (async runtime)

**Spec:** `docs/superpowers/specs/2026-03-16-auto-update-notifications-design.md`

---

## Chunk 1: Foundation — Workspace Setup + Core Events

### Task 1: Add workspace dependencies and create crate skeleton

**Files:**
- Modify: `Cargo.toml` (root workspace)
- Create: `crates/codirigent-updater/Cargo.toml`
- Create: `crates/codirigent-updater/src/lib.rs`

- [ ] **Step 1: Add new dependencies to workspace Cargo.toml**

In the root `Cargo.toml`, add to `[workspace]` `members` array:

```toml
"crates/codirigent-updater",
```

Add to `[workspace.dependencies]`:

```toml
# Auto-update
reqwest = { version = "0.12", default-features = false, features = ["json", "stream", "rustls-tls"] }
semver = "1"
sha2 = "0.10"
hex = "0.4"
futures-util = "0.3"
tokio-util = { version = "0.7", features = ["rt"] }

# Internal crate
codirigent-updater = { path = "crates/codirigent-updater" }
```

- [ ] **Step 2: Create the crate directory and Cargo.toml**

Create `crates/codirigent-updater/Cargo.toml`:

```toml
[package]
name = "codirigent-updater"
description = "Auto-update checking and installation for Codirigent"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[dependencies]
codirigent-core.workspace = true
anyhow.workspace = true
thiserror.workspace = true
serde.workspace = true
serde_json.workspace = true
tokio.workspace = true
tracing.workspace = true
reqwest.workspace = true
semver.workspace = true
sha2.workspace = true
hex.workspace = true
dirs.workspace = true
chrono.workspace = true
futures-util.workspace = true
tokio-util.workspace = true

[dev-dependencies]
tempfile.workspace = true
tokio-test.workspace = true
```

- [ ] **Step 3: Create lib.rs skeleton**

Create `crates/codirigent-updater/src/lib.rs`:

```rust
//! Codirigent Updater
//!
//! Automatic update checking and installation for Codirigent.
//!
//! This crate provides:
//! - Background version checking against GitHub Releases
//! - Artifact downloading with SHA256 verification
//! - Platform-specific update application (macOS DMG, Windows MSI)
//!
//! # Overview
//!
//! The updater checks `api.github.com/repos/oso95/Codirigent/releases/latest`
//! on startup and every 24 hours. When a newer stable version is found, it
//! publishes an `UpdateAvailable` event on the EventBus. The UI shows a toast
//! notification, and the user can choose when to download and apply the update.
//!
//! # Modules
//!
//! - [`checker`] - GitHub Releases API polling and semver comparison
//! - [`downloader`] - Artifact download and SHA256 verification
//! - [`service`] - Update state machine and orchestration
//! - [`platform`] - Platform-specific update application

#![warn(missing_docs)]
#![warn(clippy::all)]

pub mod checker;
pub mod downloader;
pub mod platform;
pub mod service;

pub use checker::UpdateInfo;
pub use service::{StagedUpdate, UpdateService, UpdateState};
```

- [ ] **Step 4: Create empty module files so the crate compiles**

Create the following empty module files (each with a one-line module doc):

`crates/codirigent-updater/src/checker.rs`:
```rust
//! GitHub Releases API polling and version comparison.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
```

`crates/codirigent-updater/src/downloader.rs`:
```rust
//! Artifact download and SHA256 checksum verification.
```

`crates/codirigent-updater/src/platform/mod.rs`:
```rust
//! Platform-specific update application.
//!
//! Dispatches to macOS or Windows implementations via `#[cfg(target_os)]`.

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(target_os = "windows")]
pub mod windows;

use anyhow::Result;
use std::path::Path;

/// Apply a staged update. Platform-specific.
pub fn apply_update(
    artifact_path: &Path,
    current_app_path: &Path,
    current_pid: u32,
) -> Result<()> {
    #[cfg(target_os = "macos")]
    return macos::apply_update(artifact_path, current_app_path, current_pid);

    #[cfg(target_os = "windows")]
    return windows::apply_update(artifact_path, current_app_path, current_pid);

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    anyhow::bail!("Auto-update is not supported on this platform")
}
```

`crates/codirigent-updater/src/platform/macos.rs`:
```rust
//! macOS update application — mount DMG, swap .app bundle, relaunch.

use anyhow::Result;
use std::path::Path;

/// Apply the update on macOS by writing and launching a helper script.
pub fn apply_update(
    _artifact_path: &Path,
    _current_app_path: &Path,
    _current_pid: u32,
) -> Result<()> {
    todo!("macOS apply_update")
}
```

`crates/codirigent-updater/src/platform/windows.rs`:
```rust
//! Windows update application — run MSI installer via msiexec.

use anyhow::Result;
use std::path::Path;

/// Apply the update on Windows by writing and launching a helper batch script.
pub fn apply_update(
    _artifact_path: &Path,
    _current_app_path: &Path,
    _current_pid: u32,
) -> Result<()> {
    todo!("Windows apply_update")
}
```

`crates/codirigent-updater/src/service.rs`:
```rust
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
}

/// Orchestrates update checking, downloading, and applying.
pub struct UpdateService;
```

- [ ] **Step 5: Verify the crate compiles**

Run: `cd /Users/cyw/Desktop/github/Dirigent/.worktrees/auto-update && cargo check -p codirigent-updater`

Expected: Compiles with no errors (warnings about unused items are fine).

- [ ] **Step 6: Commit**

```bash
git add crates/codirigent-updater/ Cargo.toml Cargo.lock
git commit -m "feat: scaffold codirigent-updater crate with module structure"
```

---

### Task 2: Add update event variants to CodirigentEvent

**Files:**
- Modify: `crates/codirigent-core/src/events.rs`

**Context:** The `CodirigentEvent` enum is at line 54 of `events.rs`. Add new variants at the end of the enum, before the closing brace. The enum already uses `#[derive(Debug, Clone)]`.

- [ ] **Step 1: Add update event variants**

Add these variants to the `CodirigentEvent` enum in `crates/codirigent-core/src/events.rs`, in a new section after the `WorkingDirectoryChanged` variants (around line 354):

```rust
    // ── Update Events ───────────────────────────────────────────────

    /// A newer stable version is available on GitHub.
    UpdateAvailable {
        /// The new version string (e.g., "0.2.0").
        version: String,
        /// URL to the GitHub release page.
        release_url: String,
    },

    /// Download progress for an update artifact.
    UpdateDownloadProgress {
        /// Percentage complete (0–100).
        percent: u8,
    },

    /// The update artifact has been downloaded and verified, ready to apply.
    UpdateReadyToApply,

    /// An update operation failed.
    UpdateFailed {
        /// Human-readable error description.
        error: String,
    },
```

- [ ] **Step 2: Verify the workspace compiles**

Run: `cd /Users/cyw/Desktop/github/Dirigent/.worktrees/auto-update && cargo check --all`

Expected: Compiles. Existing code that matches on `CodirigentEvent` with `_ => {}` wildcard arms will still compile.

- [ ] **Step 3: Commit**

```bash
git add crates/codirigent-core/src/events.rs
git commit -m "feat: add update event variants to CodirigentEvent"
```

---

## Chunk 2: Data Layer — Checker + Persistent State

### Task 3: Implement persistent state (update-state.json)

**Files:**
- Create: `crates/codirigent-updater/src/state.rs`
- Modify: `crates/codirigent-updater/src/lib.rs`

**Why first:** Both the checker (needs `last_check_timestamp`) and the service (needs `staged_update`, `last_known_version`) depend on persistent state. Build this first.

- [ ] **Step 1: Write tests for state persistence**

Create `crates/codirigent-updater/src/state.rs`:

```rust
//! Persistent update state stored at `dirs::config_dir()/codirigent/update-state.json`.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Persistent update state across app restarts.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdatePersistentState {
    /// The last version the app was known to be running.
    #[serde(default)]
    pub last_known_version: Option<String>,

    /// Timestamp of the last update check.
    #[serde(default)]
    pub last_check_timestamp: Option<DateTime<Utc>>,

    /// A staged (downloaded but not yet applied) update.
    #[serde(default)]
    pub staged_update: Option<StagedUpdateState>,
}

/// Serializable representation of a staged update.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StagedUpdateState {
    /// Version string of the staged update.
    pub version: String,
    /// Path to the downloaded artifact.
    pub artifact_path: PathBuf,
    /// URL to the GitHub release page.
    pub release_url: String,
}

/// Returns the path to the update state file.
pub fn state_file_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("codirigent").join("update-state.json"))
}

/// Returns the path to the artifact cache directory.
pub fn cache_dir() -> Option<PathBuf> {
    dirs::cache_dir().map(|d| d.join("codirigent"))
}

/// Load persistent state from disk. Returns default state if file doesn't exist.
pub fn load_state() -> Result<UpdatePersistentState> {
    let Some(path) = state_file_path() else {
        return Ok(UpdatePersistentState::default());
    };
    if !path.exists() {
        return Ok(UpdatePersistentState::default());
    }
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse {}", path.display()))
}

/// Save persistent state to disk. Uses atomic write (temp + rename).
pub fn save_state(state: &UpdatePersistentState) -> Result<()> {
    let Some(path) = state_file_path() else {
        anyhow::bail!("Could not determine config directory");
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create {}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(state)?;
    let tmp = path.with_file_name(format!(".update-state-{}.tmp", std::process::id()));
    std::fs::write(&tmp, &json)
        .with_context(|| format!("Failed to write {}", tmp.display()))?;
    std::fs::rename(&tmp, &path)
        .with_context(|| format!("Failed to rename {} to {}", tmp.display(), path.display()))?;
    Ok(())
}

/// Load state from a specific path (for testing).
pub fn load_state_from(path: &Path) -> Result<UpdatePersistentState> {
    if !path.exists() {
        return Ok(UpdatePersistentState::default());
    }
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse {}", path.display()))
}

/// Save state to a specific path (for testing).
pub fn save_state_to(state: &UpdatePersistentState, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(state)?;
    let tmp = path.with_file_name(format!(".update-state-{}.tmp", std::process::id()));
    std::fs::write(&tmp, &json)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_state_serializes() {
        let state = UpdatePersistentState::default();
        let json = serde_json::to_string(&state).unwrap();
        let loaded: UpdatePersistentState = serde_json::from_str(&json).unwrap();
        assert!(loaded.last_known_version.is_none());
        assert!(loaded.last_check_timestamp.is_none());
        assert!(loaded.staged_update.is_none());
    }

    #[test]
    fn test_full_state_round_trips() {
        let state = UpdatePersistentState {
            last_known_version: Some("0.1.0".to_string()),
            last_check_timestamp: Some(Utc::now()),
            staged_update: Some(StagedUpdateState {
                version: "0.2.0".to_string(),
                artifact_path: PathBuf::from("/tmp/test.dmg"),
                release_url: "https://github.com/test".to_string(),
            }),
        };
        let json = serde_json::to_string_pretty(&state).unwrap();
        let loaded: UpdatePersistentState = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.last_known_version, Some("0.1.0".to_string()));
        assert!(loaded.staged_update.is_some());
    }

    #[test]
    fn test_load_save_to_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("update-state.json");

        let state = UpdatePersistentState {
            last_known_version: Some("0.1.0".to_string()),
            last_check_timestamp: None,
            staged_update: None,
        };
        save_state_to(&state, &path).unwrap();
        let loaded = load_state_from(&path).unwrap();
        assert_eq!(loaded.last_known_version, Some("0.1.0".to_string()));
    }

    #[test]
    fn test_load_missing_file_returns_default() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("nonexistent.json");
        let loaded = load_state_from(&path).unwrap();
        assert!(loaded.last_known_version.is_none());
    }

    #[test]
    fn test_state_file_path_returns_some() {
        // dirs::config_dir() returns Some on macOS and Windows
        let path = state_file_path();
        if cfg!(any(target_os = "macos", target_os = "windows")) {
            assert!(path.is_some());
        }
    }

    #[test]
    fn test_cache_dir_returns_some() {
        let path = cache_dir();
        if cfg!(any(target_os = "macos", target_os = "windows")) {
            assert!(path.is_some());
        }
    }
}
```

- [ ] **Step 2: Add state module to lib.rs**

Add `pub mod state;` and `pub use state::{UpdatePersistentState, StagedUpdateState};` to `lib.rs`.

- [ ] **Step 3: Run tests**

Run: `cd /Users/cyw/Desktop/github/Dirigent/.worktrees/auto-update && cargo test -p codirigent-updater`

Expected: All tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/codirigent-updater/
git commit -m "feat: add persistent state for update checker"
```

---

### Task 4: Implement version checker (GitHub API + semver)

**Files:**
- Modify: `crates/codirigent-updater/src/checker.rs`

**Context:** The checker calls `GET https://api.github.com/repos/oso95/Codirigent/releases/latest`, parses the response, selects the correct platform asset, and compares versions using semver. This is a pure-logic module with async HTTP — tests mock the HTTP responses.

- [ ] **Step 1: Write tests for version comparison and response parsing**

Replace `crates/codirigent-updater/src/checker.rs` with the full implementation including tests:

```rust
//! GitHub Releases API polling and version comparison.

use anyhow::{Context, Result};
use semver::Version;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

/// GitHub repository to check for releases.
const GITHUB_REPO: &str = "oso95/Codirigent";

/// Information about an available update.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpdateInfo {
    /// The new version.
    pub version: Version,
    /// URL to the GitHub release page.
    pub release_url: String,
    /// Direct download URL for the platform artifact.
    pub asset_url: String,
    /// Direct download URL for checksums-sha256.txt.
    pub checksum_url: String,
}

/// Subset of the GitHub Release API response we care about.
#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
    assets: Vec<GitHubAsset>,
}

/// Subset of the GitHub Asset API response.
#[derive(Debug, Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

/// Determine the target triple and asset suffix for the current platform.
fn platform_asset_filter() -> Option<(&'static str, &'static str)> {
    match (std::env::consts::ARCH, std::env::consts::OS) {
        ("aarch64", "macos") => Some(("aarch64-apple-darwin", ".dmg")),
        ("x86_64", "windows") => Some(("x86_64-pc-windows-msvc", ".msi")),
        _ => None,
    }
}

/// Parse a GitHub release response and extract update info for the current platform.
///
/// Returns `None` if the release version is not newer than `current_version`,
/// or if no matching platform asset is found.
pub fn parse_release(
    response_json: &str,
    current_version: &Version,
) -> Result<Option<UpdateInfo>> {
    let release: GitHubRelease =
        serde_json::from_str(response_json).context("Failed to parse GitHub release JSON")?;

    let version_str = release.tag_name.strip_prefix('v').unwrap_or(&release.tag_name);
    let latest_version =
        Version::parse(version_str).context("Failed to parse release version")?;

    if latest_version <= *current_version {
        debug!(
            current = %current_version,
            latest = %latest_version,
            "Already up to date"
        );
        return Ok(None);
    }

    let Some((target_triple, suffix)) = platform_asset_filter() else {
        warn!("Unsupported platform for auto-update");
        return Ok(None);
    };

    let artifact = release
        .assets
        .iter()
        .find(|a| a.name.contains(target_triple) && a.name.ends_with(suffix));

    let checksum_asset = release
        .assets
        .iter()
        .find(|a| a.name == "checksums-sha256.txt");

    match (artifact, checksum_asset) {
        (Some(art), Some(chk)) => {
            info!(
                current = %current_version,
                latest = %latest_version,
                "Update available"
            );
            Ok(Some(UpdateInfo {
                version: latest_version,
                release_url: release.html_url,
                asset_url: art.browser_download_url.clone(),
                checksum_url: chk.browser_download_url.clone(),
            }))
        }
        _ => {
            warn!(
                %target_triple,
                "No matching asset or checksum file found in release"
            );
            Ok(None)
        }
    }
}

/// Check GitHub for the latest release.
pub async fn check_for_update(
    current_version: &Version,
    client: &reqwest::Client,
) -> Result<Option<UpdateInfo>> {
    let url = format!(
        "https://api.github.com/repos/{}/releases/latest",
        GITHUB_REPO
    );

    let response = client
        .get(&url)
        .header("User-Agent", format!("codirigent/{}", current_version))
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .context("Failed to reach GitHub API")?;

    if response.status() == reqwest::StatusCode::FORBIDDEN
        || response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS
    {
        warn!(status = %response.status(), "GitHub API rate limit or forbidden");
        anyhow::bail!("GitHub API rate limited (status {})", response.status());
    }

    if response.status() == reqwest::StatusCode::NOT_FOUND {
        debug!("No releases found");
        return Ok(None);
    }

    let body = response
        .text()
        .await
        .context("Failed to read GitHub API response")?;

    parse_release(&body, current_version)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_release_json(tag: &str) -> String {
        format!(
            r#"{{
                "tag_name": "{}",
                "html_url": "https://github.com/oso95/Codirigent/releases/tag/{}",
                "assets": [
                    {{
                        "name": "codirigent-{}-aarch64-apple-darwin.dmg",
                        "browser_download_url": "https://github.com/download/{}/darwin.dmg"
                    }},
                    {{
                        "name": "codirigent-{}-x86_64-pc-windows-msvc.msi",
                        "browser_download_url": "https://github.com/download/{}/windows.msi"
                    }},
                    {{
                        "name": "checksums-sha256.txt",
                        "browser_download_url": "https://github.com/download/{}/checksums-sha256.txt"
                    }}
                ]
            }}"#,
            tag, tag, tag, tag, tag, tag, tag
        )
    }

    #[test]
    fn test_newer_version_returns_update_info() {
        let current = Version::parse("0.1.0").unwrap();
        let json = sample_release_json("v0.2.0");
        let result = parse_release(&json, &current).unwrap();
        // Result depends on platform — on macOS it finds the .dmg, on Windows the .msi
        if platform_asset_filter().is_some() {
            let info = result.expect("should find update on supported platform");
            assert_eq!(info.version, Version::parse("0.2.0").unwrap());
            assert!(info.release_url.contains("v0.2.0"));
        }
    }

    #[test]
    fn test_same_version_returns_none() {
        let current = Version::parse("0.2.0").unwrap();
        let json = sample_release_json("v0.2.0");
        let result = parse_release(&json, &current).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_older_version_returns_none() {
        let current = Version::parse("0.3.0").unwrap();
        let json = sample_release_json("v0.2.0");
        let result = parse_release(&json, &current).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_prerelease_user_gets_notified_for_stable() {
        let current = Version::parse("0.2.0-alpha").unwrap();
        let json = sample_release_json("v0.2.0");
        let result = parse_release(&json, &current).unwrap();
        if platform_asset_filter().is_some() {
            assert!(result.is_some(), "alpha user should see stable update");
        }
    }

    #[test]
    fn test_prerelease_user_ahead_of_stable_no_update() {
        let current = Version::parse("0.3.0-alpha").unwrap();
        let json = sample_release_json("v0.2.0");
        let result = parse_release(&json, &current).unwrap();
        assert!(result.is_none(), "alpha user ahead of stable should not see update");
    }

    #[test]
    fn test_missing_checksum_asset_returns_none() {
        let json = r#"{
            "tag_name": "v0.2.0",
            "html_url": "https://github.com/test",
            "assets": [
                {
                    "name": "codirigent-v0.2.0-aarch64-apple-darwin.dmg",
                    "browser_download_url": "https://example.com/test.dmg"
                }
            ]
        }"#;
        let current = Version::parse("0.1.0").unwrap();
        let result = parse_release(json, &current).unwrap();
        // No checksums-sha256.txt asset → returns None
        assert!(result.is_none());
    }

    #[test]
    fn test_tag_without_v_prefix_parses() {
        let json = r#"{
            "tag_name": "0.2.0",
            "html_url": "https://github.com/test",
            "assets": []
        }"#;
        let current = Version::parse("0.1.0").unwrap();
        // Should not error, just return None (no assets)
        let result = parse_release(json, &current).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_invalid_json_returns_error() {
        let current = Version::parse("0.1.0").unwrap();
        let result = parse_release("not json", &current);
        assert!(result.is_err());
    }

    #[test]
    fn test_platform_asset_filter_returns_some_on_supported() {
        let filter = platform_asset_filter();
        if cfg!(all(target_arch = "aarch64", target_os = "macos")) {
            assert_eq!(filter, Some(("aarch64-apple-darwin", ".dmg")));
        } else if cfg!(all(target_arch = "x86_64", target_os = "windows")) {
            assert_eq!(filter, Some(("x86_64-pc-windows-msvc", ".msi")));
        }
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cd /Users/cyw/Desktop/github/Dirigent/.worktrees/auto-update && cargo test -p codirigent-updater -- checker`

Expected: All tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/codirigent-updater/src/checker.rs
git commit -m "feat: implement GitHub release checker with semver comparison"
```

---

## Chunk 3: Download + Platform Apply

### Task 5: Implement artifact downloader with SHA256 verification

**Files:**
- Modify: `crates/codirigent-updater/src/downloader.rs`

- [ ] **Step 1: Write the downloader with tests**

Replace `crates/codirigent-updater/src/downloader.rs`:

```rust
//! Artifact download and SHA256 checksum verification.

use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;
use tracing::{debug, info, warn};

/// Download timeout in seconds.
const DOWNLOAD_TIMEOUT_SECS: u64 = 600; // 10 minutes

/// Parse a `checksums-sha256.txt` file and find the hash for a given filename.
///
/// Format: `<hex_hash>  <filename>` (two spaces between hash and name).
pub fn find_checksum(checksums_content: &str, filename: &str) -> Option<String> {
    for line in checksums_content.lines() {
        // sha256sum format: hash followed by two spaces then filename
        if let Some((hash, name)) = line.split_once("  ") {
            if name.trim() == filename {
                return Some(hash.trim().to_lowercase());
            }
        }
    }
    None
}

/// Verify a file's SHA256 hash matches the expected value.
pub fn verify_sha256(file_path: &Path, expected_hex: &str) -> Result<bool> {
    let data = std::fs::read(file_path)
        .with_context(|| format!("Failed to read {}", file_path.display()))?;
    let hash = Sha256::digest(&data);
    let actual_hex = hex::encode(hash);
    Ok(actual_hex == expected_hex.to_lowercase())
}

/// Download the checksums file and return its content.
pub async fn download_checksums(
    client: &reqwest::Client,
    checksum_url: &str,
    user_agent: &str,
) -> Result<String> {
    let response = client
        .get(checksum_url)
        .header("User-Agent", user_agent)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .context("Failed to download checksums file")?;

    response
        .text()
        .await
        .context("Failed to read checksums response body")
}

/// Download an artifact to the given path, reporting progress via callback.
///
/// Returns the path to the downloaded file.
pub async fn download_artifact<F>(
    client: &reqwest::Client,
    asset_url: &str,
    dest_dir: &Path,
    user_agent: &str,
    on_progress: F,
) -> Result<PathBuf>
where
    F: Fn(u8) + Send,
{
    std::fs::create_dir_all(dest_dir)
        .with_context(|| format!("Failed to create cache dir {}", dest_dir.display()))?;

    let filename = asset_url
        .rsplit('/')
        .next()
        .unwrap_or("update-artifact");
    let dest_path = dest_dir.join(filename);

    info!(url = %asset_url, dest = %dest_path.display(), "Downloading update artifact");

    let response = client
        .get(asset_url)
        .header("User-Agent", user_agent)
        .timeout(std::time::Duration::from_secs(DOWNLOAD_TIMEOUT_SECS))
        .send()
        .await
        .context("Failed to start artifact download")?;

    let total_size = response.content_length().unwrap_or(0);
    let mut downloaded: u64 = 0;

    let mut file = tokio::fs::File::create(&dest_path)
        .await
        .with_context(|| format!("Failed to create {}", dest_path.display()))?;

    let mut stream = response.bytes_stream();
    use futures_util::StreamExt;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("Error reading download stream")?;
        file.write_all(&chunk)
            .await
            .context("Error writing to file")?;
        downloaded += chunk.len() as u64;
        if total_size > 0 {
            let percent = ((downloaded as f64 / total_size as f64) * 100.0).min(100.0) as u8;
            on_progress(percent);
        }
    }

    file.flush().await?;
    info!(path = %dest_path.display(), "Download complete");

    Ok(dest_path)
}

/// Full download + verify flow.
pub async fn download_and_verify<F>(
    client: &reqwest::Client,
    asset_url: &str,
    checksum_url: &str,
    dest_dir: &Path,
    user_agent: &str,
    on_progress: F,
) -> Result<PathBuf>
where
    F: Fn(u8) + Send,
{
    // Download checksums first
    let checksums = download_checksums(client, checksum_url, user_agent).await?;

    // Download artifact
    let artifact_path =
        download_artifact(client, asset_url, dest_dir, user_agent, on_progress).await?;

    // Extract filename for checksum lookup
    let filename = artifact_path
        .file_name()
        .and_then(|n| n.to_str())
        .context("Invalid artifact filename")?;

    // Verify checksum
    let expected_hash = find_checksum(&checksums, filename)
        .with_context(|| format!("No checksum found for {} in checksums file", filename))?;

    if !verify_sha256(&artifact_path, &expected_hash)? {
        // Clean up failed download
        let _ = std::fs::remove_file(&artifact_path);
        bail!("SHA256 checksum mismatch for {}", filename);
    }

    debug!(file = %filename, "SHA256 checksum verified");
    Ok(artifact_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_checksum_standard_format() {
        let content = "abc123def456  codirigent-v0.2.0-aarch64-apple-darwin.dmg\nfed321cba654  codirigent-v0.2.0-x86_64-pc-windows-msvc.msi\n";
        let result = find_checksum(content, "codirigent-v0.2.0-aarch64-apple-darwin.dmg");
        assert_eq!(result, Some("abc123def456".to_string()));
    }

    #[test]
    fn test_find_checksum_windows_asset() {
        let content = "abc123  darwin.dmg\nfed321  windows.msi\n";
        let result = find_checksum(content, "windows.msi");
        assert_eq!(result, Some("fed321".to_string()));
    }

    #[test]
    fn test_find_checksum_not_found() {
        let content = "abc123  other-file.dmg\n";
        let result = find_checksum(content, "nonexistent.dmg");
        assert!(result.is_none());
    }

    #[test]
    fn test_find_checksum_empty_content() {
        assert!(find_checksum("", "any.dmg").is_none());
    }

    #[test]
    fn test_verify_sha256_correct() {
        let tmp = tempfile::TempDir::new().unwrap();
        let file_path = tmp.path().join("test.bin");
        std::fs::write(&file_path, b"hello world").unwrap();

        // SHA256 of "hello world"
        let expected = "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";
        assert!(verify_sha256(&file_path, expected).unwrap());
    }

    #[test]
    fn test_verify_sha256_incorrect() {
        let tmp = tempfile::TempDir::new().unwrap();
        let file_path = tmp.path().join("test.bin");
        std::fs::write(&file_path, b"hello world").unwrap();

        assert!(!verify_sha256(&file_path, "0000000000000000").unwrap());
    }

    #[test]
    fn test_verify_sha256_case_insensitive() {
        let tmp = tempfile::TempDir::new().unwrap();
        let file_path = tmp.path().join("test.bin");
        std::fs::write(&file_path, b"hello world").unwrap();

        let expected = "B94D27B9934D3E08A52E52D7DA7DABFAC484EFE37A5380EE9088F7ACE2EFCDE9";
        assert!(verify_sha256(&file_path, expected).unwrap());
    }

    #[test]
    fn test_verify_sha256_missing_file() {
        let result = verify_sha256(Path::new("/nonexistent/file"), "abc");
        assert!(result.is_err());
    }
}
```

- [ ] **Step 2: Add `futures-util` dependency**

The downloader uses `futures_util::StreamExt` for streaming.

Add to root `Cargo.toml` `[workspace.dependencies]`:
```toml
futures-util = "0.3"
```

Add to `crates/codirigent-updater/Cargo.toml` under `[dependencies]`:
```toml
futures-util.workspace = true
```

- [ ] **Step 3: Run tests**

Run: `cd /Users/cyw/Desktop/github/Dirigent/.worktrees/auto-update && cargo test -p codirigent-updater -- downloader`

Expected: All tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/codirigent-updater/
git commit -m "feat: implement artifact downloader with SHA256 verification"
```

---

### Task 6: Implement platform-specific apply logic

**Files:**
- Modify: `crates/codirigent-updater/src/platform/mod.rs`
- Modify: `crates/codirigent-updater/src/platform/macos.rs`
- Modify: `crates/codirigent-updater/src/platform/windows.rs`

- [ ] **Step 1: Implement platform/mod.rs with app path detection**

Replace `crates/codirigent-updater/src/platform/mod.rs`:

```rust
//! Platform-specific update application.
//!
//! Dispatches to macOS or Windows implementations via `#[cfg(target_os)]`.
//! On unsupported platforms, returns an error.

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(target_os = "windows")]
pub mod windows;

use anyhow::Result;
use std::path::{Path, PathBuf};

/// Apply a staged update. Platform-specific.
///
/// This function writes a helper script, launches it detached, and returns.
/// The caller should quit the app immediately after this returns successfully.
pub fn apply_update(
    artifact_path: &Path,
    current_pid: u32,
) -> Result<()> {
    let app_path = detect_app_path()?;

    #[cfg(target_os = "macos")]
    return macos::apply_update(artifact_path, &app_path, current_pid);

    #[cfg(target_os = "windows")]
    return windows::apply_update(artifact_path, &app_path, current_pid);

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = (artifact_path, &app_path, current_pid);
        anyhow::bail!("Auto-update is not supported on this platform")
    }
}

/// Detect the current application path.
///
/// On macOS: walks up from the current exe to find the `.app` bundle.
/// On Windows: returns the directory containing the current exe.
fn detect_app_path() -> Result<PathBuf> {
    let exe = std::env::current_exe()?;

    #[cfg(target_os = "macos")]
    {
        // Walk up from e.g. /Applications/Codirigent.app/Contents/MacOS/codirigent
        // to find the .app bundle
        let mut path = exe.as_path();
        while let Some(parent) = path.parent() {
            if path
                .extension()
                .map(|e| e == "app")
                .unwrap_or(false)
            {
                return Ok(path.to_path_buf());
            }
            path = parent;
        }
        anyhow::bail!(
            "Could not find .app bundle from exe path: {}",
            exe.display()
        )
    }

    #[cfg(target_os = "windows")]
    {
        // Return the directory containing the exe
        exe.parent()
            .map(|p| p.to_path_buf())
            .ok_or_else(|| anyhow::anyhow!("Could not determine install directory"))
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        anyhow::bail!("Platform not supported")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_app_path_does_not_panic() {
        // On dev machines this won't find a .app bundle (macOS) or will return the
        // cargo target dir (Windows), but it should not panic.
        let result = detect_app_path();
        // On macOS in dev, this will error because we're not inside a .app bundle.
        // On Windows, it should succeed.
        #[cfg(target_os = "windows")]
        assert!(result.is_ok());
        // Just ensure no panic on any platform
        let _ = result;
    }
}
```

- [ ] **Step 2: Implement macOS apply logic**

Replace `crates/codirigent-updater/src/platform/macos.rs`:

```rust
//! macOS update application — mount DMG, swap .app bundle, relaunch.

use crate::state::cache_dir;
use anyhow::{Context, Result};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;
use tracing::info;

/// Generate the macOS update helper script content.
pub fn generate_update_script(
    dmg_path: &Path,
    current_app_path: &Path,
    pid: u32,
) -> String {
    format!(
        r#"#!/bin/bash
APP_PID={}

# Wait for the app to fully exit
while kill -0 "$APP_PID" 2>/dev/null; do sleep 0.5; done

DMG_PATH="{}"
CURRENT_APP_PATH="{}"

# Use a unique mount point to avoid collisions
MOUNT_DIR=$(mktemp -d /tmp/codirigent-mount.XXXXXX)

# Backup current app (abort if backup fails)
if ! cp -R "$CURRENT_APP_PATH" "$CURRENT_APP_PATH.bak"; then
  open "$CURRENT_APP_PATH"
  exit 1
fi

# Mount DMG and replace
if hdiutil attach "$DMG_PATH" -mountpoint "$MOUNT_DIR" -quiet; then
  rm -rf "$CURRENT_APP_PATH"
  if cp -R "$MOUNT_DIR/Codirigent.app" "$CURRENT_APP_PATH"; then
    rm -rf "$CURRENT_APP_PATH.bak"
  else
    rm -rf "$CURRENT_APP_PATH"
    mv "$CURRENT_APP_PATH.bak" "$CURRENT_APP_PATH"
  fi
  hdiutil detach "$MOUNT_DIR" -quiet
else
  rm -rf "$CURRENT_APP_PATH"
  mv "$CURRENT_APP_PATH.bak" "$CURRENT_APP_PATH"
fi

rmdir "$MOUNT_DIR" 2>/dev/null
open "$CURRENT_APP_PATH"
rm -f "$DMG_PATH"
"#,
        pid,
        dmg_path.display(),
        current_app_path.display()
    )
}

/// Apply the update on macOS by writing and launching a helper script.
pub fn apply_update(
    artifact_path: &Path,
    current_app_path: &Path,
    current_pid: u32,
) -> Result<()> {
    let cache = cache_dir().context("Could not determine cache directory")?;
    std::fs::create_dir_all(&cache)?;
    let script_path = cache.join("codirigent-update.sh");

    let script = generate_update_script(artifact_path, current_app_path, current_pid);
    std::fs::write(&script_path, &script)
        .with_context(|| format!("Failed to write update script to {}", script_path.display()))?;
    std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755))?;

    info!(script = %script_path.display(), "Launching update helper script");

    Command::new("bash")
        .arg(&script_path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .context("Failed to launch update helper script")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_generate_update_script_contains_pid() {
        let script = generate_update_script(
            &PathBuf::from("/tmp/test.dmg"),
            &PathBuf::from("/Applications/Codirigent.app"),
            12345,
        );
        assert!(script.contains("APP_PID=12345"));
    }

    #[test]
    fn test_generate_update_script_contains_paths() {
        let script = generate_update_script(
            &PathBuf::from("/tmp/test.dmg"),
            &PathBuf::from("/Applications/Codirigent.app"),
            1,
        );
        assert!(script.contains("/tmp/test.dmg"));
        assert!(script.contains("/Applications/Codirigent.app"));
    }

    #[test]
    fn test_generate_update_script_has_backup_logic() {
        let script = generate_update_script(
            &PathBuf::from("/tmp/test.dmg"),
            &PathBuf::from("/Applications/Codirigent.app"),
            1,
        );
        assert!(script.contains(".bak"));
        assert!(script.contains("mktemp -d"));
    }

    #[test]
    fn test_generate_update_script_has_relaunch() {
        let script = generate_update_script(
            &PathBuf::from("/tmp/test.dmg"),
            &PathBuf::from("/Applications/Codirigent.app"),
            1,
        );
        assert!(script.contains("open \"$CURRENT_APP_PATH\""));
    }
}
```

- [ ] **Step 3: Implement Windows apply logic**

Replace `crates/codirigent-updater/src/platform/windows.rs`:

```rust
//! Windows update application — run MSI installer via msiexec.

use crate::state::cache_dir;
use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;
use tracing::info;

/// Generate the Windows update helper batch script content.
pub fn generate_update_script(
    msi_path: &Path,
    install_path: &Path,
    pid: u32,
) -> String {
    format!(
        r#"@echo off
set APP_PID={}

REM Wait for the app to fully exit
:wait_loop
tasklist /FI "PID eq %APP_PID%" 2>nul | find "%APP_PID%" >nul
if not errorlevel 1 (
  timeout /t 1 /nobreak >nul
  goto wait_loop
)

msiexec /passive /i "{}"
del "{}"

REM Relaunch the app after MSI completes
start "" "{}\codirigent.exe"
"#,
        pid,
        msi_path.display(),
        msi_path.display(),
        install_path.display()
    )
}

/// Apply the update on Windows by writing and launching a helper batch script.
pub fn apply_update(
    artifact_path: &Path,
    install_path: &Path,
    current_pid: u32,
) -> Result<()> {
    let cache = cache_dir().context("Could not determine cache directory")?;
    std::fs::create_dir_all(&cache)?;
    let script_path = cache.join("codirigent-update.bat");

    let script = generate_update_script(artifact_path, install_path, current_pid);
    std::fs::write(&script_path, &script)
        .with_context(|| format!("Failed to write update script to {}", script_path.display()))?;

    info!(script = %script_path.display(), "Launching update helper script");

    Command::new("cmd")
        .args(["/C", "start", "/B", ""])
        .arg(&script_path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .context("Failed to launch update helper script")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_generate_update_script_contains_pid() {
        let script = generate_update_script(
            &PathBuf::from(r"C:\temp\update.msi"),
            &PathBuf::from(r"C:\Program Files\Codirigent"),
            12345,
        );
        assert!(script.contains("APP_PID=12345"));
    }

    #[test]
    fn test_generate_update_script_contains_msiexec() {
        let script = generate_update_script(
            &PathBuf::from(r"C:\temp\update.msi"),
            &PathBuf::from(r"C:\Program Files\Codirigent"),
            1,
        );
        assert!(script.contains("msiexec /passive /i"));
    }

    #[test]
    fn test_generate_update_script_has_relaunch() {
        let script = generate_update_script(
            &PathBuf::from(r"C:\temp\update.msi"),
            &PathBuf::from(r"C:\Program Files\Codirigent"),
            1,
        );
        assert!(script.contains(r"C:\Program Files\Codirigent\codirigent.exe"));
    }

    #[test]
    fn test_generate_update_script_has_wait_loop() {
        let script = generate_update_script(
            &PathBuf::from(r"C:\temp\update.msi"),
            &PathBuf::from(r"C:\Program Files\Codirigent"),
            1,
        );
        assert!(script.contains(":wait_loop"));
        assert!(script.contains("tasklist"));
    }
}
```

- [ ] **Step 4: Run all tests**

Run: `cd /Users/cyw/Desktop/github/Dirigent/.worktrees/auto-update && cargo test -p codirigent-updater`

Expected: All tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/codirigent-updater/src/platform/
git commit -m "feat: implement platform-specific update apply logic"
```

---

## Chunk 4: Service Orchestration + UI Integration

### Task 7: Implement UpdateService state machine

**Files:**
- Modify: `crates/codirigent-updater/src/service.rs`
- Modify: `crates/codirigent-updater/src/lib.rs`

**Context:** The `UpdateService` owns the state machine. It is constructed by the UI, runs a background check on startup, and exposes methods for the UI to call (start_download, apply_update, etc.). It publishes events on the EventBus.

- [ ] **Step 1: Implement the full UpdateService**

Replace `crates/codirigent-updater/src/service.rs` with the full implementation. Key parts:

```rust
//! Update state machine and orchestration.

use crate::checker::{self, UpdateInfo};
use crate::downloader;
use crate::state::{self, UpdatePersistentState, StagedUpdateState};
use anyhow::Result;
use codirigent_core::{CodirigentEvent, EventBus};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::watch;
use tracing::{error, info, warn};

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
    Downloading { percent: u8 },
    /// Download complete, ready to apply.
    Staged(StagedUpdate),
    /// Applying the update.
    Applying,
}

/// A downloaded update ready to apply.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StagedUpdate {
    /// The version of the staged update.
    pub version: Version,
    /// Path to the downloaded artifact.
    pub artifact_path: PathBuf,
    /// URL to the GitHub release page.
    pub release_url: String,
    /// Expected SHA256 hash of the artifact (for re-verification before apply).
    pub expected_sha256: String,
}

/// Check interval between update checks.
const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60); // 24 hours

/// Orchestrates update checking, downloading, and applying.
pub struct UpdateService {
    current_version: Version,
    event_bus: Arc<dyn EventBus>,
    state: Arc<Mutex<UpdateState>>,
    client: reqwest::Client,
    /// Cancellation token for in-progress downloads.
    download_cancel: Arc<Mutex<tokio_util::sync::CancellationToken>>,
}

impl UpdateService {
    /// Create a new UpdateService.
    pub fn new(current_version: &str, event_bus: Arc<dyn EventBus>) -> Result<Self> {
        let version = Version::parse(current_version)?;
        Ok(Self {
            current_version: version,
            event_bus,
            state: Arc::new(Mutex::new(UpdateState::Idle)),
            client: reqwest::Client::new(),
            download_cancel: Arc::new(Mutex::new(tokio_util::sync::CancellationToken::new())),
        })
    }

    /// Get the current update state.
    pub fn state(&self) -> UpdateState {
        self.state.lock().unwrap().clone()
    }

    /// Start background update checking. Call once at app startup.
    ///
    /// This spawns a tokio task that:
    /// 1. Checks for stale staged updates on startup
    /// 2. Restores staged updates from persistent state
    /// 3. Checks for updates immediately (if interval has elapsed)
    /// 4. Re-checks every 24 hours
    pub fn start_background_check(&self) {
        let version = self.current_version.clone();
        let event_bus = self.event_bus.clone();
        let state = self.state.clone();
        let client = self.client.clone();

        tokio::spawn(async move {
            // Load persistent state and handle startup conditions
            let mut persistent = state::load_state().unwrap_or_default();

            // Check for post-update (version changed)
            if let Some(ref last_version) = persistent.last_known_version {
                if last_version != &version.to_string() {
                    // Version changed — this is a post-update launch
                    info!(
                        old = %last_version,
                        new = %version,
                        "Detected version change — post-update launch"
                    );
                    persistent.staged_update = None;
                    persistent.last_known_version = Some(version.to_string());
                    let _ = state::save_state(&persistent);
                    // The UI will detect the version change and show the "Updated to" toast
                }
            } else {
                persistent.last_known_version = Some(version.to_string());
                let _ = state::save_state(&persistent);
            }

            // Check for stale staged update
            if let Some(ref staged) = persistent.staged_update {
                let artifact_exists = PathBuf::from(&staged.artifact_path).exists();
                let version_matches = persistent.last_known_version.as_deref()
                    == Some(&version.to_string());

                if !artifact_exists {
                    warn!("Staged artifact missing, clearing stale entry");
                    persistent.staged_update = None;
                    let _ = state::save_state(&persistent);
                } else if version_matches {
                    // Version matches but staged update exists — apply failed last time
                    warn!("Stale staged update detected (version matches), clearing");
                    let _ = std::fs::remove_file(&staged.artifact_path);
                    persistent.staged_update = None;
                    let _ = state::save_state(&persistent);
                } else {
                    // Restore staged state
                    let staged_update = StagedUpdate {
                        version: Version::parse(&staged.version).unwrap_or(version.clone()),
                        artifact_path: staged.artifact_path.clone(),
                        release_url: staged.release_url.clone(),
                    };
                    *state.lock().unwrap() = UpdateState::Staged(staged_update);
                    event_bus.publish(CodirigentEvent::UpdateReadyToApply);
                    return; // Don't check for updates if we already have one staged
                }
            }

            // Check if enough time has passed since last check
            let should_check = persistent
                .last_check_timestamp
                .map(|ts| {
                    let elapsed = chrono::Utc::now() - ts;
                    elapsed.num_seconds() >= CHECK_INTERVAL.as_secs() as i64
                })
                .unwrap_or(true); // Always check on first run

            if should_check {
                Self::do_check(&version, &client, &event_bus, &state).await;
            }

            // Schedule periodic checks
            let mut interval = tokio::time::interval(CHECK_INTERVAL);
            interval.tick().await; // Skip the first immediate tick
            loop {
                interval.tick().await;
                Self::do_check(&version, &client, &event_bus, &state).await;
            }
        });
    }

    async fn do_check(
        version: &Version,
        client: &reqwest::Client,
        event_bus: &Arc<dyn EventBus>,
        state: &Arc<Mutex<UpdateState>>,
    ) {
        *state.lock().unwrap() = UpdateState::Checking;

        match checker::check_for_update(version, client).await {
            Ok(Some(info)) => {
                event_bus.publish(CodirigentEvent::UpdateAvailable {
                    version: info.version.to_string(),
                    release_url: info.release_url.clone(),
                });
                *state.lock().unwrap() = UpdateState::UpdateAvailable(info);
            }
            Ok(None) => {
                *state.lock().unwrap() = UpdateState::Idle;
            }
            Err(e) => {
                warn!("Update check failed: {}", e);
                event_bus.publish(CodirigentEvent::UpdateFailed {
                    error: e.to_string(),
                });
                *state.lock().unwrap() = UpdateState::Idle;
            }
        }

        // Save check timestamp
        if let Ok(mut persistent) = state::load_state() {
            persistent.last_check_timestamp = Some(chrono::Utc::now());
            let _ = state::save_state(&persistent);
        }
    }

    /// Start downloading the update. Call when user clicks "Update".
    pub fn start_download(&self) {
        let state = self.state.clone();
        let event_bus = self.event_bus.clone();
        let client = self.client.clone();
        let version = self.current_version.clone();

        let update_info = {
            let current = state.lock().unwrap();
            match &*current {
                UpdateState::UpdateAvailable(info) => info.clone(),
                _ => return, // Can only download from UpdateAvailable state
            }
        };

        *state.lock().unwrap() = UpdateState::Downloading { percent: 0 };

        tokio::spawn(async move {
            let Some(dest_dir) = state::cache_dir() else {
                error!("Could not determine cache directory");
                *state.lock().unwrap() = UpdateState::UpdateAvailable(update_info);
                return;
            };

            let user_agent = format!("codirigent/{}", version);

            // Clean up any old staged artifacts
            if let Ok(persistent) = state::load_state() {
                if let Some(old_staged) = &persistent.staged_update {
                    let _ = std::fs::remove_file(&old_staged.artifact_path);
                }
            }

            let event_bus_progress = event_bus.clone();
            let state_progress = state.clone();

            match downloader::download_and_verify(
                &client,
                &update_info.asset_url,
                &update_info.checksum_url,
                &dest_dir,
                &user_agent,
                move |percent| {
                    *state_progress.lock().unwrap() =
                        UpdateState::Downloading { percent };
                    event_bus_progress.publish(CodirigentEvent::UpdateDownloadProgress {
                        percent,
                    });
                },
            )
            .await
            {
                Ok(artifact_path) => {
                    let staged = StagedUpdate {
                        version: update_info.version.clone(),
                        artifact_path: artifact_path.clone(),
                        release_url: update_info.release_url.clone(),
                    };

                    // Persist staged state
                    if let Ok(mut persistent) = state::load_state() {
                        persistent.staged_update = Some(StagedUpdateState {
                            version: staged.version.to_string(),
                            artifact_path,
                            release_url: staged.release_url.clone(),
                        });
                        let _ = state::save_state(&persistent);
                    }

                    *state.lock().unwrap() = UpdateState::Staged(staged);
                    event_bus.publish(CodirigentEvent::UpdateReadyToApply);
                }
                Err(e) => {
                    error!("Download failed: {}", e);
                    event_bus.publish(CodirigentEvent::UpdateFailed {
                        error: e.to_string(),
                    });
                    *state.lock().unwrap() = UpdateState::UpdateAvailable(update_info);
                }
            }
        });
    }

    /// Apply the staged update. Call when user clicks "Restart Now".
    ///
    /// This re-verifies the SHA256 checksum, writes the helper script, and
    /// launches it. The caller should quit the app after this returns Ok.
    pub fn apply(&self) -> Result<()> {
        let current_state = self.state.lock().unwrap().clone();
        let staged = match current_state {
            UpdateState::Staged(s) => s,
            _ => anyhow::bail!("No staged update to apply"),
        };

        // Re-verify SHA256 before applying (spec requirement)
        if !staged.artifact_path.exists() {
            anyhow::bail!("Staged artifact no longer exists");
        }
        if !downloader::verify_sha256(&staged.artifact_path, &staged.expected_sha256)? {
            anyhow::bail!("SHA256 re-verification failed — artifact may be corrupted");
        }

        *self.state.lock().unwrap() = UpdateState::Applying;

        let pid = std::process::id();
        crate::platform::apply_update(&staged.artifact_path, pid)?;

        Ok(())
    }

    /// Cancel an in-progress download.
    ///
    /// Signals the download task to stop via the cancellation token and
    /// transitions back to UpdateAvailable so the user can retry.
    pub fn cancel_download(&self) {
        self.download_cancel.lock().unwrap().cancel();
        // State will be reset to UpdateAvailable by the download task's
        // cancellation handler. If the task already completed, this is a no-op.
    }
}
```

- [ ] **Step 2: Update lib.rs exports**

Update `crates/codirigent-updater/src/lib.rs` to include the state module and re-exports:

```rust
pub mod checker;
pub mod downloader;
pub mod platform;
pub mod service;
pub mod state;

pub use checker::UpdateInfo;
pub use service::{StagedUpdate, UpdateService, UpdateState};
pub use state::{UpdatePersistentState, StagedUpdateState};
```

- [ ] **Step 3: Verify compilation**

Run: `cd /Users/cyw/Desktop/github/Dirigent/.worktrees/auto-update && cargo check -p codirigent-updater`

Expected: Compiles. Some warnings about unused code are acceptable at this stage.

- [ ] **Step 4: Run all updater tests**

Run: `cd /Users/cyw/Desktop/github/Dirigent/.worktrees/auto-update && cargo test -p codirigent-updater`

Expected: All tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/codirigent-updater/
git commit -m "feat: implement UpdateService state machine and orchestration"
```

---

### Task 8: Wire UpdateService into workspace view and add toast notification

**Files:**
- Modify: `crates/codirigent-ui/Cargo.toml` — add `codirigent-updater` dependency
- Modify: `crates/codirigent-ui/src/workspace/gpui.rs` — add update state fields to `WorkspaceView`, instantiate `UpdateService` in `new()`
- Create: `crates/codirigent-ui/src/workspace/toast_render.rs` — toast notification rendering
- Modify: `crates/codirigent-ui/src/workspace/render.rs` — call toast render in the main render function

**This is the UI integration task. The exact GPUI rendering code will depend on inspecting the existing render patterns in the workspace. The implementer should:**

- [ ] **Step 1: Add codirigent-updater dependency to codirigent-ui**

In `crates/codirigent-ui/Cargo.toml`, add under `[dependencies]`:

```toml
codirigent-updater.workspace = true
```

- [ ] **Step 2: Add update state fields to WorkspaceView**

In `crates/codirigent-ui/src/workspace/gpui.rs`, add these fields to the `WorkspaceView` struct (around line 150, in the sub-state groups section):

```rust
    /// Update service for auto-update checking and downloading.
    update_service: Option<Arc<codirigent_updater::UpdateService>>,
    /// Current update info from the checker.
    update_info: Option<codirigent_updater::UpdateInfo>,
    /// Whether the user dismissed the update toast this session.
    update_dismissed: bool,
    /// Download progress percentage (0-100) during download.
    update_download_progress: Option<u8>,
    /// Staged update ready to apply.
    staged_update: Option<codirigent_updater::StagedUpdate>,
    /// Whether this is the first launch after a successful update.
    post_update_version: Option<String>,
```

Initialize them all to `None`/`false` in the `WorkspaceView::new()` constructor.

- [ ] **Step 3: Detect post-update and instantiate UpdateService in WorkspaceView::new()**

In the `WorkspaceView::new()` function, after the event bus is available, add:

**IMPORTANT:** Post-update detection MUST happen before `start_background_check()` to avoid a race condition. The background task also updates `last_known_version`.

```rust
    // Detect post-update launch BEFORE starting the background check
    let post_update_version = {
        if let Ok(persistent) = codirigent_updater::state::load_state() {
            if let Some(ref last_ver) = persistent.last_known_version {
                if last_ver != env!("CARGO_PKG_VERSION") {
                    Some(env!("CARGO_PKG_VERSION").to_string())
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        }
    };

    let update_service = match codirigent_updater::UpdateService::new(
        env!("CARGO_PKG_VERSION"),
        event_bus.clone(),
    ) {
        Ok(svc) => {
            svc.start_background_check();
            Some(Arc::new(svc))
        }
        Err(e) => {
            tracing::warn!("Failed to initialize update service: {}", e);
            None
        }
    };
```

- [ ] **Step 4: Handle update events in the polling loop**

In the workspace view's event processing (where it processes `CodirigentEvent` variants from the EventBus), add handlers:

```rust
    CodirigentEvent::UpdateAvailable { version, release_url } => {
        if !self.update_dismissed {
            // The update_info is set from the service's state
            if let Some(svc) = &self.update_service {
                if let codirigent_updater::UpdateState::UpdateAvailable(info) = svc.state() {
                    self.update_info = Some(info);
                }
            }
            cx.notify();
        }
    }
    CodirigentEvent::UpdateDownloadProgress { percent } => {
        self.update_download_progress = Some(percent);
        cx.notify();
    }
    CodirigentEvent::UpdateReadyToApply => {
        if let Some(svc) = &self.update_service {
            if let codirigent_updater::UpdateState::Staged(staged) = svc.state() {
                self.staged_update = Some(staged);
                self.update_download_progress = None;
            }
        }
        cx.notify();
    }
    CodirigentEvent::UpdateFailed { error } => {
        tracing::warn!("Update failed: {}", error);
        self.update_download_progress = None;
        cx.notify();
    }
```

- [ ] **Step 5: Create toast rendering module**

Create `crates/codirigent-ui/src/workspace/toast_render.rs`. This module renders the toast notification in the bottom-right corner of the workspace. Follow the existing rendering patterns from `modal_render.rs` — use `div()`, theme colors, and GPUI's layout system.

The toast should show different content based on state:
- `update_info` is Some + no staged + no progress → "New version available (vX.Y.Z)" with [Update] button
- `update_download_progress` is Some → "Downloading... N%" with [Cancel] button
- `staged_update` is Some → "Update ready (vX.Y.Z)" with [Restart Now] and [Later] buttons
- `post_update_version` is Some → "Updated to vX.Y.Z" with [Release Notes] button

Button handlers:
- **[Update]**: call `self.update_service.as_ref().unwrap().start_download()`
- **[Cancel]**: call `self.update_service.as_ref().unwrap().cancel_download()`; set `update_download_progress = None`
- **[Restart Now]**: check for working sessions first. If any `SessionStatus::Working`, show confirmation. Then call `self.update_service.as_ref().unwrap().apply()`; if Ok, quit the app via `cx.quit()`
- **[Later]**: set `update_dismissed = true`; clear `staged_update` from local state (it persists on disk)
- **[Release Notes]**: open `release_url` in browser; set `post_update_version = None`
- **Dismiss (X or click outside)**: set `update_dismissed = true`

- [ ] **Step 6: Wire toast rendering into the main render function**

In `crates/codirigent-ui/src/workspace/render.rs`, add a call to render the toast overlay. It should be rendered last (on top of everything else) as an absolutely-positioned element in the bottom-right corner.

- [ ] **Step 7: Add `mod toast_render;` to workspace mod.rs**

In `crates/codirigent-ui/src/workspace/mod.rs`, add with the other `#[cfg(feature = "gpui-full")]` module declarations:

```rust
#[cfg(feature = "gpui-full")]
mod toast_render;
```

- [ ] **Step 8: Verify full workspace compiles**

Run: `cd /Users/cyw/Desktop/github/Dirigent/.worktrees/auto-update && cargo check --all --all-targets`

Expected: Compiles with no errors.

- [ ] **Step 9: Run all tests**

Run: `cd /Users/cyw/Desktop/github/Dirigent/.worktrees/auto-update && cargo test --all --all-targets`

Expected: All tests pass (existing + new).

- [ ] **Step 10: Commit**

```bash
git add crates/codirigent-ui/ crates/codirigent-updater/
git commit -m "feat: integrate auto-update toast notification into workspace UI"
```
