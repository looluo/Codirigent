//! Windows update application -- run MSI installer via msiexec.

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
