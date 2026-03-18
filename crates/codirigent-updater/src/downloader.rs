//! Artifact download and SHA256 checksum verification.
//!
//! Downloads release artifacts from GitHub with streaming progress callbacks,
//! then verifies integrity using SHA256 checksums from `checksums-sha256.txt`.

use anyhow::{bail, Context, Result};
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;
use tracing::{debug, info};

/// Parse a `sha256sum`-format checksums file and return the hash for `filename`.
///
/// The expected format is `<hex-hash>  <filename>` (two spaces between hash and
/// filename). Returns `None` if no matching line is found.
pub fn find_checksum(checksums_content: &str, filename: &str) -> Option<String> {
    for line in checksums_content.lines() {
        // sha256sum format: "<hash>  <filename>" (two spaces)
        if let Some((hash, name)) = line.split_once("  ") {
            if name.trim() == filename {
                return Some(hash.trim().to_string());
            }
        }
    }
    None
}

/// Read a file and verify its SHA256 hash against an expected hex string.
///
/// The comparison is case-insensitive. Returns `Ok(true)` on match,
/// `Ok(false)` on mismatch, or an error if the file cannot be read.
pub fn verify_sha256(file_path: &Path, expected_hex: &str) -> Result<bool> {
    let data = std::fs::read(file_path).with_context(|| {
        format!(
            "Failed to read file for SHA256 verification: {}",
            file_path.display()
        )
    })?;

    let mut hasher = Sha256::new();
    hasher.update(&data);
    let actual_hex = hex::encode(hasher.finalize());

    Ok(actual_hex.eq_ignore_ascii_case(expected_hex))
}

/// Download the checksums file content from the given URL.
pub async fn download_checksums(
    client: &reqwest::Client,
    url: &str,
    user_agent: &str,
) -> Result<String> {
    let response = client
        .get(url)
        .header("User-Agent", user_agent)
        .header("Accept", "application/octet-stream")
        .send()
        .await
        .context("Failed to download checksums file")?
        .error_for_status()
        .context("Checksums download returned an error status")?;

    response
        .text()
        .await
        .context("Failed to read checksums response body")
}

/// Download an artifact to `dest_dir` with streaming progress.
///
/// The filename is extracted from the URL path. The `on_progress` callback
/// receives the percentage (0..=100) as downloads proceed. A 10-minute timeout
/// is applied to the overall request.
pub async fn download_artifact<F>(
    client: &reqwest::Client,
    url: &str,
    dest_dir: &Path,
    user_agent: &str,
    on_progress: F,
) -> Result<PathBuf>
where
    F: Fn(u8),
{
    // Extract filename from URL.
    let filename = url
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .context("Could not extract filename from artifact URL")?;

    // Validate that the filename contains only safe characters to prevent
    // path traversal or other filesystem attacks.
    if !filename
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.' || c == '_')
    {
        bail!(
            "Unsafe characters in artifact filename '{}': only alphanumeric, hyphens, dots, and underscores are allowed",
            filename
        );
    }

    // Ensure destination directory exists.
    tokio::fs::create_dir_all(dest_dir).await.with_context(|| {
        format!(
            "Failed to create download directory: {}",
            dest_dir.display()
        )
    })?;

    let dest_path = dest_dir.join(filename);

    debug!(url, dest = %dest_path.display(), "Starting artifact download");

    let response = client
        .get(url)
        .header("User-Agent", user_agent)
        .header("Accept", "application/octet-stream")
        .timeout(std::time::Duration::from_secs(600)) // 10 minutes
        .send()
        .await
        .context("Failed to start artifact download")?
        .error_for_status()
        .context("Artifact download returned an error status")?;

    let total_size = response.content_length();
    let mut stream = response.bytes_stream();

    let mut file = tokio::fs::File::create(&dest_path)
        .await
        .with_context(|| format!("Failed to create file: {}", dest_path.display()))?;

    let mut downloaded: u64 = 0;
    let mut last_percent: u8 = 0;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("Error reading download stream")?;
        file.write_all(&chunk)
            .await
            .context("Failed to write chunk to file")?;

        downloaded += chunk.len() as u64;

        if let Some(total) = total_size {
            let percent = if total > 0 {
                ((downloaded as f64 / total as f64) * 100.0).min(100.0) as u8
            } else {
                0
            };
            if percent != last_percent {
                last_percent = percent;
                on_progress(percent);
            }
        }
    }

    file.flush()
        .await
        .context("Failed to flush downloaded file")?;

    // Ensure 100% is reported.
    if last_percent < 100 {
        on_progress(100);
    }

    info!(
        dest = %dest_path.display(),
        bytes = downloaded,
        "Artifact download complete"
    );

    Ok(dest_path)
}

/// Download an artifact and verify it against the checksums file.
///
/// Returns both the artifact path and the expected SHA256 hash on success.
/// The caller can store the hash in [`StagedUpdate`] for re-verification
/// before applying the update.
///
/// If the checksum does not match, the downloaded artifact is deleted and
/// an error is returned.
pub async fn download_and_verify<F>(
    client: &reqwest::Client,
    asset_url: &str,
    checksum_url: &str,
    dest_dir: &Path,
    user_agent: &str,
    on_progress: F,
) -> Result<(PathBuf, String)>
where
    F: Fn(u8),
{
    // 1. Download checksums file.
    let checksums_content = download_checksums(client, checksum_url, user_agent).await?;

    // 2. Download the artifact.
    let artifact_path =
        download_artifact(client, asset_url, dest_dir, user_agent, on_progress).await?;

    // 3. Extract the filename to look up its expected hash.
    let filename = artifact_path
        .file_name()
        .and_then(|n| n.to_str())
        .context("Artifact path has no filename")?;

    let expected_hash = find_checksum(&checksums_content, filename)
        .with_context(|| format!("No checksum found for '{}' in checksums file", filename))?;

    // 4. Verify.
    let valid = verify_sha256(&artifact_path, &expected_hash)
        .with_context(|| format!("SHA256 verification failed for {}", artifact_path.display()))?;

    if !valid {
        // Delete the corrupt artifact.
        let _ = std::fs::remove_file(&artifact_path);
        bail!(
            "SHA256 checksum mismatch for '{}': expected {}",
            filename,
            expected_hash
        );
    }

    info!(
        artifact = %artifact_path.display(),
        sha256 = %expected_hash,
        "Artifact verified successfully"
    );

    Ok((artifact_path, expected_hash))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    // -----------------------------------------------------------------------
    // find_checksum tests
    // -----------------------------------------------------------------------

    #[test]
    fn find_checksum_standard_format() {
        let content = "\
abc123def456  Codirigent-aarch64-apple-darwin.dmg
789abc012def  Codirigent-x86_64-pc-windows-msvc.msi
fedcba987654  checksums-sha256.txt
";
        let hash = find_checksum(content, "Codirigent-aarch64-apple-darwin.dmg");
        assert_eq!(hash, Some("abc123def456".to_string()));

        let hash2 = find_checksum(content, "Codirigent-x86_64-pc-windows-msvc.msi");
        assert_eq!(hash2, Some("789abc012def".to_string()));
    }

    #[test]
    fn find_checksum_not_found() {
        let content = "abc123def456  some-other-file.dmg\n";
        let hash = find_checksum(content, "nonexistent.dmg");
        assert_eq!(hash, None);
    }

    #[test]
    fn find_checksum_empty_content() {
        let hash = find_checksum("", "anything.dmg");
        assert_eq!(hash, None);
    }

    #[test]
    fn find_checksum_no_double_space() {
        // Lines without double-space separator should not match.
        let content = "abc123 single-space-file.dmg\n";
        let hash = find_checksum(content, "single-space-file.dmg");
        assert_eq!(hash, None);
    }

    // -----------------------------------------------------------------------
    // verify_sha256 tests
    // -----------------------------------------------------------------------

    #[test]
    fn verify_sha256_correct_hash() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file_path = dir.path().join("test.bin");
        let data = b"hello world";

        std::fs::write(&file_path, data).expect("write test file");

        // Known SHA256 of "hello world"
        let expected = "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";
        let result = verify_sha256(&file_path, expected).expect("verify should not error");
        assert!(result, "hash should match for correct content");
    }

    #[test]
    fn verify_sha256_incorrect_hash() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file_path = dir.path().join("test.bin");
        std::fs::write(&file_path, b"hello world").expect("write test file");

        let wrong_hash = "0000000000000000000000000000000000000000000000000000000000000000";
        let result = verify_sha256(&file_path, wrong_hash).expect("verify should not error");
        assert!(!result, "hash should NOT match for wrong hash");
    }

    #[test]
    fn verify_sha256_case_insensitive() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file_path = dir.path().join("test.bin");
        std::fs::write(&file_path, b"hello world").expect("write test file");

        // Same hash as above but in UPPERCASE
        let expected = "B94D27B9934D3E08A52E52D7DA7DABFAC484EFE37A5380EE9088F7ACE2EFCDE9";
        let result = verify_sha256(&file_path, expected).expect("verify should not error");
        assert!(result, "hash comparison should be case-insensitive");
    }

    #[test]
    fn verify_sha256_missing_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file_path = dir.path().join("nonexistent.bin");

        let result = verify_sha256(&file_path, "abc123");
        assert!(result.is_err(), "missing file should return an error");
    }

    #[test]
    fn verify_sha256_empty_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file_path = dir.path().join("empty.bin");
        std::fs::File::create(&file_path).expect("create empty file");

        // SHA256 of empty input
        let expected = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        let result = verify_sha256(&file_path, expected).expect("verify should not error");
        assert!(result, "empty file hash should match");
    }

    #[test]
    fn verify_sha256_large_content() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file_path = dir.path().join("large.bin");

        // Write 1MB of data
        let mut file = std::fs::File::create(&file_path).expect("create file");
        let chunk = vec![0xABu8; 1024];
        for _ in 0..1024 {
            file.write_all(&chunk).expect("write chunk");
        }
        drop(file);

        // Compute expected hash
        let data = std::fs::read(&file_path).expect("read");
        let mut hasher = Sha256::new();
        hasher.update(&data);
        let expected = hex::encode(hasher.finalize());

        let result = verify_sha256(&file_path, &expected).expect("verify should not error");
        assert!(result, "large file hash should match");
    }
}
