//! macOS update application -- mount DMG, swap .app bundle, relaunch.
//!
//! Generates a bash helper script that:
//! 1. Waits for the current process to exit
//! 2. Backs up the existing .app bundle
//! 3. Mounts the DMG and copies the new .app
//! 4. Restores the backup on failure
//! 5. Relaunches the application

use anyhow::{Context, Result};
use std::path::Path;
use tracing::info;

/// Generate a bash script that performs the macOS update.
///
/// The script waits for the running process (`pid`) to exit, mounts the DMG,
/// swaps the .app bundle with a backup/restore safety net, and relaunches.
pub fn generate_update_script(dmg_path: &Path, current_app_path: &Path, pid: u32) -> String {
    let dmg = dmg_path.display();
    let app = current_app_path.display();

    // Derive the .app name from the path (e.g. "Codirigent.app")
    let app_name = current_app_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Codirigent.app");

    format!(
        r#"#!/bin/bash
set -euo pipefail

APP_PID={pid}
DMG_PATH="{dmg}"
APP_PATH="{app}"
APP_NAME="{app_name}"
APP_PARENT="$(dirname "$APP_PATH")"
BACKUP_PATH="${{APP_PARENT}}/.codirigent-update-backup"

# --- Wait for the application to exit ---
echo "Waiting for PID $APP_PID to exit..."
while kill -0 "$APP_PID" 2>/dev/null; do
    sleep 0.5
done
echo "Process $APP_PID has exited."

# --- Create a unique mount point ---
MOUNT_POINT="$(mktemp -d /tmp/codirigent-mount.XXXXXX)"

cleanup() {{
    # Unmount the DMG if mounted
    if [ -d "$MOUNT_POINT" ]; then
        hdiutil detach "$MOUNT_POINT" -quiet 2>/dev/null || true
        rmdir "$MOUNT_POINT" 2>/dev/null || true
    fi
    # Clean up the DMG file
    rm -f "$DMG_PATH"
}}
trap cleanup EXIT

# --- Back up the current .app ---
echo "Backing up $APP_PATH to $BACKUP_PATH..."
if ! cp -Rp "$APP_PATH" "$BACKUP_PATH"; then
    echo "ERROR: Failed to create backup. Aborting update."
    open "$APP_PATH"
    exit 1
fi

# --- Mount the DMG ---
echo "Mounting $DMG_PATH..."
if ! hdiutil attach "$DMG_PATH" -mountpoint "$MOUNT_POINT" -nobrowse -quiet; then
    echo "ERROR: Failed to mount DMG. Restoring backup..."
    rm -rf "$APP_PATH"
    mv "$BACKUP_PATH" "$APP_PATH"
    open "$APP_PATH"
    exit 1
fi

# --- Copy the new .app ---
echo "Installing new version..."
if ! (rm -rf "$APP_PATH" && cp -Rp "$MOUNT_POINT/$APP_NAME" "$APP_PATH"); then
    echo "ERROR: Failed to copy new app. Restoring backup..."
    rm -rf "$APP_PATH"
    mv "$BACKUP_PATH" "$APP_PATH"
    open "$APP_PATH"
    exit 1
fi

# --- Success: remove backup ---
rm -rf "$BACKUP_PATH"

echo "Update applied successfully."

# --- Relaunch ---
open "$APP_PATH"
"#
    )
}

/// Apply the update on macOS by writing and launching a helper script.
///
/// Writes the update script to the cache directory and launches it as a
/// detached process. The script will wait for the current app to exit before
/// performing the swap.
pub fn apply_update(artifact_path: &Path, current_app_path: &Path, current_pid: u32) -> Result<()> {
    let cache = crate::state::cache_dir().context("Could not determine cache directory")?;
    std::fs::create_dir_all(&cache)
        .with_context(|| format!("Failed to create cache directory: {}", cache.display()))?;

    let script_path = cache.join("codirigent-update.sh");
    let script = generate_update_script(artifact_path, current_app_path, current_pid);

    std::fs::write(&script_path, &script)
        .with_context(|| format!("Failed to write update script: {}", script_path.display()))?;

    // Make the script executable.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o755);
        std::fs::set_permissions(&script_path, perms)
            .with_context(|| format!("Failed to set permissions on {}", script_path.display()))?;
    }

    info!(script = %script_path.display(), "Launching update script");

    std::process::Command::new("bash")
        .arg(&script_path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .context("Failed to launch update script")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn script_contains_pid() {
        let script = generate_update_script(
            &PathBuf::from("/tmp/Codirigent-0.2.0.dmg"),
            &PathBuf::from("/Applications/Codirigent.app"),
            12345,
        );
        assert!(
            script.contains("APP_PID=12345"),
            "Script should contain the PID"
        );
    }

    #[test]
    fn script_contains_correct_paths() {
        let dmg = PathBuf::from("/tmp/downloads/Codirigent-0.2.0.dmg");
        let app = PathBuf::from("/Applications/Codirigent.app");
        let script = generate_update_script(&dmg, &app, 99999);

        assert!(
            script.contains("/tmp/downloads/Codirigent-0.2.0.dmg"),
            "Script should contain the DMG path"
        );
        assert!(
            script.contains("/Applications/Codirigent.app"),
            "Script should contain the app path"
        );
    }

    #[test]
    fn script_has_backup_restore_logic() {
        let script = generate_update_script(
            &PathBuf::from("/tmp/Codirigent.dmg"),
            &PathBuf::from("/Applications/Codirigent.app"),
            1000,
        );

        assert!(
            script.contains("BACKUP_PATH"),
            "Script should define a backup path"
        );
        assert!(
            script.contains("cp -Rp \"$APP_PATH\" \"$BACKUP_PATH\""),
            "Script should back up the current app"
        );
        assert!(
            script.contains("mv \"$BACKUP_PATH\" \"$APP_PATH\""),
            "Script should restore backup on failure"
        );
    }

    #[test]
    fn script_has_relaunch_command() {
        let script = generate_update_script(
            &PathBuf::from("/tmp/Codirigent.dmg"),
            &PathBuf::from("/Applications/Codirigent.app"),
            1000,
        );

        assert!(
            script.contains("open \"$APP_PATH\""),
            "Script should relaunch the app with 'open'"
        );
    }

    #[test]
    fn script_mounts_dmg_with_unique_mount_point() {
        let script = generate_update_script(
            &PathBuf::from("/tmp/Codirigent.dmg"),
            &PathBuf::from("/Applications/Codirigent.app"),
            1000,
        );

        assert!(
            script.contains("mktemp -d /tmp/codirigent-mount.XXXXXX"),
            "Script should create unique mount point with mktemp"
        );
        assert!(
            script.contains("hdiutil attach"),
            "Script should mount the DMG"
        );
        assert!(
            script.contains("hdiutil detach"),
            "Script should unmount the DMG on cleanup"
        );
    }

    #[test]
    fn script_waits_for_process_exit() {
        let script = generate_update_script(
            &PathBuf::from("/tmp/Codirigent.dmg"),
            &PathBuf::from("/Applications/Codirigent.app"),
            42,
        );

        assert!(
            script.contains("kill -0 \"$APP_PID\""),
            "Script should poll for process exit using kill -0"
        );
    }

    #[test]
    fn script_cleans_up_dmg() {
        let script = generate_update_script(
            &PathBuf::from("/tmp/Codirigent.dmg"),
            &PathBuf::from("/Applications/Codirigent.app"),
            1000,
        );

        assert!(
            script.contains("rm -f \"$DMG_PATH\""),
            "Script should clean up the DMG file"
        );
    }
}
