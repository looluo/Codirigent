//! GitHub Releases API polling and version comparison.
//!
//! The checker queries the GitHub Releases API for the latest stable release
//! of Codirigent, compares it against the currently running version using
//! semver, and returns an [`UpdateInfo`] when an upgrade is available.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

/// GitHub repository used for release checks.
const GITHUB_REPO: &str = "oso95/Codirigent";

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

// ---------------------------------------------------------------------------
// GitHub API response types (private)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
    assets: Vec<GitHubAsset>,
}

#[derive(Debug, Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

// ---------------------------------------------------------------------------
// Platform asset filter
// ---------------------------------------------------------------------------

/// Returns `(target_triple_substring, artifact_suffix)` for the current
/// platform, or `None` if auto-update is not supported on this OS/arch.
pub fn platform_asset_filter() -> Option<(&'static str, &'static str)> {
    match (std::env::consts::ARCH, std::env::consts::OS) {
        ("aarch64", "macos") => Some(("aarch64-apple-darwin", ".dmg")),
        ("x86_64", "macos") => Some(("x86_64-apple-darwin", ".dmg")),
        ("x86_64", "windows") => Some(("x86_64-pc-windows-msvc", ".msi")),
        ("aarch64", "windows") => Some(("aarch64-pc-windows-msvc", ".msi")),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Pure parsing / comparison logic
// ---------------------------------------------------------------------------

/// Parse a GitHub release JSON response and determine if an update is available.
///
/// This is a pure function (no I/O) that makes it easy to test version
/// comparison logic with synthetic payloads.
///
/// Returns:
/// - `Ok(Some(UpdateInfo))` if the release is newer than `current_version`
/// - `Ok(None)` if up to date, no matching platform asset, or no checksum file
/// - `Err` on JSON parse failure
pub fn parse_release(
    response_json: &str,
    current_version: &semver::Version,
) -> Result<Option<UpdateInfo>> {
    let release: GitHubRelease =
        serde_json::from_str(response_json).context("Failed to parse GitHub release JSON")?;

    // Strip optional leading 'v' from tag.
    let tag = release.tag_name.strip_prefix('v').unwrap_or(&release.tag_name);
    let remote_version: semver::Version =
        tag.parse().with_context(|| format!("Invalid semver in tag: {}", release.tag_name))?;

    // A pre-release current version (e.g. 0.3.0-alpha) is "ahead" of a stable
    // release (e.g. 0.2.0) if its major.minor.patch is greater. But a
    // pre-release *of the same version* (0.2.0-alpha) should be notified about
    // the stable release (0.2.0).
    //
    // semver crate ordering: 0.2.0-alpha < 0.2.0 < 0.3.0-alpha < 0.3.0
    // So `remote_version > *current_version` handles both cases correctly:
    //   - 0.2.0 > 0.2.0-alpha  →  true (notify)
    //   - 0.2.0 > 0.3.0-alpha  →  false (don't notify)
    if remote_version <= *current_version {
        debug!(
            current = %current_version,
            remote = %remote_version,
            "Already up to date"
        );
        return Ok(None);
    }

    // Find the platform artifact.
    let (triple, suffix) = match platform_asset_filter() {
        Some(filter) => filter,
        None => {
            debug!("No platform asset filter for this OS/arch — skipping");
            return Ok(None);
        }
    };

    let artifact = release.assets.iter().find(|a| {
        a.name.contains(triple) && a.name.ends_with(suffix)
    });

    let artifact = match artifact {
        Some(a) => a,
        None => {
            debug!(
                triple,
                suffix,
                "No matching platform artifact found in release assets"
            );
            return Ok(None);
        }
    };

    // Find the checksum file.
    let checksum = release
        .assets
        .iter()
        .find(|a| a.name == "checksums-sha256.txt");

    let checksum = match checksum {
        Some(c) => c,
        None => {
            debug!("No checksums-sha256.txt found in release assets");
            return Ok(None);
        }
    };

    Ok(Some(UpdateInfo {
        version: remote_version,
        release_url: release.html_url,
        asset_url: artifact.browser_download_url.clone(),
        checksum_url: checksum.browser_download_url.clone(),
    }))
}

// ---------------------------------------------------------------------------
// Async network check
// ---------------------------------------------------------------------------

/// Check the GitHub Releases API for the latest version.
///
/// Returns `Ok(Some(UpdateInfo))` when a newer release is found, `Ok(None)`
/// when up to date (or rate-limited / no releases), and `Err` on network or
/// parse failure.
pub async fn check_for_update(
    current_version: &semver::Version,
    client: &reqwest::Client,
) -> Result<Option<UpdateInfo>> {
    let url = format!(
        "https://api.github.com/repos/{}/releases/latest",
        GITHUB_REPO
    );

    let response = client
        .get(&url)
        .header("User-Agent", format!("codirigent/{current_version}"))
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .context("Failed to reach GitHub Releases API")?;

    let status = response.status();

    // 404 → repository has no releases yet.
    if status == reqwest::StatusCode::NOT_FOUND {
        debug!("No releases found (404)");
        return Ok(None);
    }

    // 403 / 429 → rate-limited; treat as "no update" and try again later.
    if status == reqwest::StatusCode::FORBIDDEN
        || status == reqwest::StatusCode::TOO_MANY_REQUESTS
    {
        warn!(
            status = status.as_u16(),
            "GitHub API rate limit hit — will retry later"
        );
        return Ok(None);
    }

    let body = response
        .error_for_status()
        .context("GitHub API returned an error")?
        .text()
        .await
        .context("Failed to read GitHub API response body")?;

    parse_release(&body, current_version)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to build a minimal GitHub release JSON payload.
    fn make_release_json(
        tag: &str,
        assets: &[(&str, &str)], // (name, url)
    ) -> String {
        let asset_entries: Vec<String> = assets
            .iter()
            .map(|(name, url)| {
                format!(
                    r#"{{"name": "{name}", "browser_download_url": "{url}"}}"#
                )
            })
            .collect();

        format!(
            r#"{{
                "tag_name": "{tag}",
                "html_url": "https://github.com/oso95/Codirigent/releases/tag/{tag}",
                "assets": [{assets}]
            }}"#,
            assets = asset_entries.join(",")
        )
    }

    /// Build release JSON with typical assets for the current platform.
    fn make_platform_release(tag: &str) -> String {
        let (triple, suffix) = platform_asset_filter()
            .unwrap_or(("aarch64-apple-darwin", ".dmg"));
        let artifact_name = format!("Codirigent-{triple}{suffix}");
        let artifact_url = format!("https://example.com/{artifact_name}");
        make_release_json(
            tag,
            &[
                (&artifact_name, &artifact_url),
                (
                    "checksums-sha256.txt",
                    "https://example.com/checksums-sha256.txt",
                ),
            ],
        )
    }

    #[test]
    fn newer_version_returns_update_info() {
        let current: semver::Version = "0.1.0".parse().unwrap();
        let json = make_platform_release("v0.2.0");
        let result = parse_release(&json, &current).expect("parse should succeed");
        assert!(result.is_some(), "should detect newer version");
        let info = result.unwrap();
        assert_eq!(info.version, "0.2.0".parse::<semver::Version>().unwrap());
    }

    #[test]
    fn same_version_returns_none() {
        let current: semver::Version = "0.2.0".parse().unwrap();
        let json = make_platform_release("v0.2.0");
        let result = parse_release(&json, &current).expect("parse should succeed");
        assert!(result.is_none(), "same version should return None");
    }

    #[test]
    fn older_version_returns_none() {
        let current: semver::Version = "0.3.0".parse().unwrap();
        let json = make_platform_release("v0.2.0");
        let result = parse_release(&json, &current).expect("parse should succeed");
        assert!(result.is_none(), "older version should return None");
    }

    #[test]
    fn prerelease_user_gets_notified_for_stable() {
        // User on 0.2.0-alpha should be notified about stable 0.2.0
        let current: semver::Version = "0.2.0-alpha".parse().unwrap();
        let json = make_platform_release("v0.2.0");
        let result = parse_release(&json, &current).expect("parse should succeed");
        assert!(
            result.is_some(),
            "prerelease user should be notified of stable release"
        );
    }

    #[test]
    fn prerelease_user_ahead_of_stable_gets_none() {
        // User on 0.3.0-alpha is ahead of stable 0.2.0
        let current: semver::Version = "0.3.0-alpha".parse().unwrap();
        let json = make_platform_release("v0.2.0");
        let result = parse_release(&json, &current).expect("parse should succeed");
        assert!(
            result.is_none(),
            "prerelease user ahead of stable should get None"
        );
    }

    #[test]
    fn missing_checksum_asset_returns_none() {
        let current: semver::Version = "0.1.0".parse().unwrap();
        let (triple, suffix) = platform_asset_filter()
            .unwrap_or(("aarch64-apple-darwin", ".dmg"));
        let artifact_name = format!("Codirigent-{triple}{suffix}");
        // Release with the artifact but NO checksums-sha256.txt
        let json = make_release_json(
            "v0.2.0",
            &[(&artifact_name, "https://example.com/artifact")],
        );
        let result = parse_release(&json, &current).expect("parse should succeed");
        assert!(result.is_none(), "missing checksum should return None");
    }

    #[test]
    fn tag_without_v_prefix_parses_correctly() {
        let current: semver::Version = "0.1.0".parse().unwrap();
        let json = make_platform_release("0.2.0"); // no 'v' prefix
        let result = parse_release(&json, &current).expect("parse should succeed");
        assert!(result.is_some(), "tag without v prefix should still parse");
        let info = result.unwrap();
        assert_eq!(info.version, "0.2.0".parse::<semver::Version>().unwrap());
    }

    #[test]
    fn invalid_json_returns_error() {
        let current: semver::Version = "0.1.0".parse().unwrap();
        let result = parse_release("not json at all", &current);
        assert!(result.is_err(), "invalid JSON should return Err");
    }

    #[test]
    fn platform_asset_filter_values() {
        // This test verifies the function returns a value on the current platform.
        // On CI or unsupported platforms, we just verify the function doesn't panic.
        let filter = platform_asset_filter();

        // On macOS (aarch64 or x86_64) or Windows, should be Some.
        if cfg!(target_os = "macos") {
            assert!(filter.is_some(), "macOS should have a platform filter");
            let (triple, suffix) = filter.unwrap();
            assert!(triple.contains("apple-darwin"));
            assert_eq!(suffix, ".dmg");
        } else if cfg!(target_os = "windows") {
            assert!(filter.is_some(), "Windows should have a platform filter");
            let (triple, suffix) = filter.unwrap();
            assert!(triple.contains("pc-windows-msvc"));
            assert_eq!(suffix, ".msi");
        }
    }

    #[test]
    fn no_matching_platform_artifact_returns_none() {
        let current: semver::Version = "0.1.0".parse().unwrap();
        // Only has a Linux artifact — no match on macOS or Windows.
        let json = make_release_json(
            "v0.2.0",
            &[
                (
                    "Codirigent-x86_64-unknown-linux-gnu.tar.gz",
                    "https://example.com/linux.tar.gz",
                ),
                (
                    "checksums-sha256.txt",
                    "https://example.com/checksums-sha256.txt",
                ),
            ],
        );
        let result = parse_release(&json, &current).expect("parse should succeed");
        // On macOS/Windows this returns None (no matching asset).
        // On Linux (unsupported platform) it also returns None (no filter).
        assert!(
            result.is_none(),
            "should return None when platform artifact is missing"
        );
    }
}
