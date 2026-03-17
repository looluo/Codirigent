//! Windows update application -- run MSI installer via msiexec.
//!
//! Generates a batch helper script that:
//! 1. Waits for the current process to exit
//! 2. Runs `msiexec /passive /i` on the MSI
//! 3. Cleans up the MSI file
//! 4. Relaunches the application

use anyhow::{Context, Result};
use std::path::Path;
use tracing::info;

/// Generate a batch script that performs the Windows update.
///
/// The script waits for the running process (`pid`) to exit using `tasklist`,
/// runs the MSI installer in passive mode, then relaunches the application.
pub fn generate_update_script(msi_path: &Path, install_path: &Path, pid: u32) -> String {
    let msi = msi_path.display();
    let install = install_path.display();

    format!(
        r#"@echo off
setlocal

set "APP_PID={pid}"
set "MSI_PATH={msi}"
set "INSTALL_PATH={install}"

REM --- Wait for the application to exit ---
echo Waiting for PID %APP_PID% to exit...
:wait_loop
tasklist /FI "PID eq %APP_PID%" 2>NUL | find /I "%APP_PID%" >NUL
if not errorlevel 1 (
    timeout /t 1 /nobreak >NUL
    goto wait_loop
)
echo Process %APP_PID% has exited.

REM --- Run the MSI installer ---
echo Installing update...
msiexec /passive /i "%MSI_PATH%"
if errorlevel 1 (
    echo ERROR: MSI installation failed with exit code %ERRORLEVEL%.
    del /f "%MSI_PATH%" 2>NUL
    exit /b 1
)

REM --- Clean up MSI ---
del /f "%MSI_PATH%" 2>NUL

echo Update applied successfully.

REM --- Relaunch ---
start "" "%INSTALL_PATH%\codirigent.exe"
"#
    )
}

/// Apply the update on Windows by writing and launching a helper batch script.
///
/// Writes the update script to the cache directory and launches it as a
/// detached process. The script will wait for the current app to exit before
/// running the MSI installer.
pub fn apply_update(
    artifact_path: &Path,
    install_path: &Path,
    current_pid: u32,
) -> Result<()> {
    let cache = crate::state::cache_dir().context("Could not determine cache directory")?;
    std::fs::create_dir_all(&cache)
        .with_context(|| format!("Failed to create cache directory: {}", cache.display()))?;

    let script_path = cache.join("codirigent-update.bat");
    let script = generate_update_script(artifact_path, install_path, current_pid);

    std::fs::write(&script_path, &script)
        .with_context(|| format!("Failed to write update script: {}", script_path.display()))?;

    info!(script = %script_path.display(), "Launching update script");

    std::process::Command::new("cmd")
        .args(["/C", "start", "/B", ""])
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
            &PathBuf::from("C:\\Users\\user\\Downloads\\Codirigent-0.2.0.msi"),
            &PathBuf::from("C:\\Program Files\\Codirigent"),
            12345,
        );
        assert!(
            script.contains("APP_PID=12345"),
            "Script should contain the PID"
        );
    }

    #[test]
    fn script_contains_correct_paths() {
        let msi = PathBuf::from("C:\\temp\\Codirigent-0.2.0.msi");
        let install = PathBuf::from("C:\\Program Files\\Codirigent");
        let script = generate_update_script(&msi, &install, 99999);

        assert!(
            script.contains("C:\\temp\\Codirigent-0.2.0.msi"),
            "Script should contain the MSI path"
        );
        assert!(
            script.contains("C:\\Program Files\\Codirigent"),
            "Script should contain the install path"
        );
    }

    #[test]
    fn script_has_wait_loop() {
        let script = generate_update_script(
            &PathBuf::from("C:\\temp\\update.msi"),
            &PathBuf::from("C:\\Program Files\\Codirigent"),
            1000,
        );

        assert!(
            script.contains("tasklist"),
            "Script should use tasklist to poll for process exit"
        );
        assert!(
            script.contains(":wait_loop"),
            "Script should have a wait loop label"
        );
        assert!(
            script.contains("goto wait_loop"),
            "Script should loop back to wait"
        );
    }

    #[test]
    fn script_runs_msiexec() {
        let script = generate_update_script(
            &PathBuf::from("C:\\temp\\update.msi"),
            &PathBuf::from("C:\\Program Files\\Codirigent"),
            1000,
        );

        assert!(
            script.contains("msiexec /passive /i"),
            "Script should run msiexec in passive mode"
        );
    }

    #[test]
    fn script_has_relaunch_command() {
        let script = generate_update_script(
            &PathBuf::from("C:\\temp\\update.msi"),
            &PathBuf::from("C:\\Program Files\\Codirigent"),
            1000,
        );

        assert!(
            script.contains(r#"start "" "%INSTALL_PATH%\codirigent.exe""#),
            "Script should relaunch the app"
        );
    }

    #[test]
    fn script_cleans_up_msi() {
        let script = generate_update_script(
            &PathBuf::from("C:\\temp\\update.msi"),
            &PathBuf::from("C:\\Program Files\\Codirigent"),
            1000,
        );

        assert!(
            script.contains(r#"del /f "%MSI_PATH%""#),
            "Script should delete the MSI after install"
        );
    }
}
