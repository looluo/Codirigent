//! macOS update application -- mount DMG, swap .app bundle, relaunch.

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
