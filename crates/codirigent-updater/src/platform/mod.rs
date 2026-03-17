//! Platform-specific update application.
//!
//! Dispatches to macOS or Windows implementations via `#[cfg(target_os)]`.
//! Both modules are always compiled so that their unit tests (which only test
//! script generation, not execution) run on every platform.

pub mod macos;
pub mod windows;

use anyhow::Result;
use std::path::Path;

/// Apply a staged update. Platform-specific.
pub fn apply_update(
    artifact_path: &Path,
    current_pid: u32,
) -> Result<()> {
    #[cfg(target_os = "macos")]
    return macos::apply_update(artifact_path, &detect_app_path()?, current_pid);

    #[cfg(target_os = "windows")]
    return windows::apply_update(artifact_path, &detect_app_path()?, current_pid);

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = (artifact_path, current_pid);
        anyhow::bail!("Auto-update is not supported on this platform")
    }
}

/// Detect the current application path.
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn detect_app_path() -> Result<std::path::PathBuf> {
    let exe = std::env::current_exe()?;

    #[cfg(target_os = "macos")]
    {
        let mut path = exe.as_path();
        while let Some(parent) = path.parent() {
            if path.extension().map(|e| e == "app").unwrap_or(false) {
                return Ok(path.to_path_buf());
            }
            path = parent;
        }
        anyhow::bail!("Could not find .app bundle from exe path: {}", exe.display())
    }

    #[cfg(target_os = "windows")]
    {
        exe.parent()
            .map(|p| p.to_path_buf())
            .ok_or_else(|| anyhow::anyhow!("Could not determine install directory"))
    }
}
