//! Windows-specific process monitoring using Win32 APIs.
//!
//! This module uses the `windows` crate to access Windows process information
//! through Win32 APIs. It provides access to process state, parent-child
//! relationships, and basic process detection.
//!
//! Note: Windows does not have the Unix concept of foreground process groups.
//! Input detection on Windows uses different heuristics, primarily based on
//! timing and output patterns.

use super::{PlatformMonitor, ProcessInfo, ProcessState};
use anyhow::{Context, Result};
use tracing::debug;
use windows::Win32::Foundation::{CloseHandle, HANDLE, STILL_ACTIVE};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{
    GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
};

/// Windows process monitor using Win32 APIs.
///
/// This monitor uses Windows ToolHelp API for process enumeration and
/// standard Win32 process functions for state queries.
#[derive(Debug, Default)]
pub struct WindowsMonitor;

impl WindowsMonitor {
    /// Create a new Windows process monitor.
    pub fn new() -> Self {
        Self
    }

    /// Helper to safely close a Windows handle.
    ///
    /// This is a no-op if the handle is invalid.
    fn close_handle_safe(handle: HANDLE) {
        if !handle.is_invalid() {
            // SAFETY: We only close handles that were successfully opened
            let _ = unsafe { CloseHandle(handle) };
        }
    }

    /// Find a process entry in the system snapshot by PID.
    fn find_process_entry(pid: u32) -> Result<PROCESSENTRY32W> {
        // SAFETY: CreateToolhelp32Snapshot is safe to call
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }
            .context("Failed to create process snapshot")?;

        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };

        // SAFETY: Process32FirstW is safe with a valid snapshot and properly sized entry
        let first_result = unsafe { Process32FirstW(snapshot, &mut entry) };

        if first_result.is_err() {
            Self::close_handle_safe(snapshot);
            anyhow::bail!("Failed to get first process entry");
        }

        loop {
            if entry.th32ProcessID == pid {
                Self::close_handle_safe(snapshot);
                return Ok(entry);
            }

            // SAFETY: Process32NextW is safe with valid snapshot and entry
            if unsafe { Process32NextW(snapshot, &mut entry) }.is_err() {
                break;
            }
        }

        Self::close_handle_safe(snapshot);
        anyhow::bail!("Process not found: {}", pid)
    }
}

impl PlatformMonitor for WindowsMonitor {
    fn get_process_state(&self, pid: u32) -> Result<ProcessState> {
        // SAFETY: OpenProcess with PROCESS_QUERY_LIMITED_INFORMATION is safe
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }
            .context("Failed to open process")?;

        let mut exit_code: u32 = 0;
        // SAFETY: GetExitCodeProcess is safe with a valid handle
        let result = unsafe { GetExitCodeProcess(handle, &mut exit_code) };

        Self::close_handle_safe(handle);

        result.context("Failed to get exit code")?;

        let state = if exit_code == STILL_ACTIVE.0 as u32 {
            ProcessState::Running
        } else {
            ProcessState::Terminated
        };

        debug!(pid, ?state, exit_code, "Got Windows process state");
        Ok(state)
    }

    fn get_process_info(&self, pid: u32) -> Result<ProcessInfo> {
        let entry = Self::find_process_entry(pid)?;

        // Get the actual state
        let state = self.get_process_state(pid).unwrap_or(ProcessState::Unknown);

        // Extract the command name from the wide string
        let command = {
            let end = entry
                .szExeFile
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(entry.szExeFile.len());
            String::from_utf16_lossy(&entry.szExeFile[..end])
        };

        Ok(ProcessInfo {
            pid,
            state,
            ppid: Some(entry.th32ParentProcessID),
            command: if command.is_empty() {
                None
            } else {
                Some(command)
            },
        })
    }

    fn get_child_processes(&self, pid: u32) -> Result<Vec<u32>> {
        let mut children = Vec::new();

        // SAFETY: CreateToolhelp32Snapshot is safe to call
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }
            .context("Failed to create process snapshot")?;

        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };

        // SAFETY: Process32FirstW is safe with properly initialized entry
        if unsafe { Process32FirstW(snapshot, &mut entry) }.is_ok() {
            loop {
                if entry.th32ParentProcessID == pid {
                    children.push(entry.th32ProcessID);
                }

                // SAFETY: Process32NextW is safe with valid snapshot
                if unsafe { Process32NextW(snapshot, &mut entry) }.is_err() {
                    break;
                }
            }
        }

        Self::close_handle_safe(snapshot);
        debug!(pid, child_count = children.len(), "Got child processes");
        Ok(children)
    }

    fn get_foreground_pgid(&self, _tty_fd: i32) -> Result<i32> {
        // Windows doesn't have process groups like Unix
        // Return -1 to indicate this concept doesn't apply
        debug!("get_foreground_pgid called on Windows - returning -1");
        Ok(-1)
    }

    fn is_likely_waiting_for_input(&self, pid: u32, _tty_fd: i32) -> Result<bool> {
        // On Windows, we can't directly detect if a process is waiting for input
        // the way we can on Unix (using foreground process groups).
        //
        // The actual input detection on Windows is handled at a higher level
        // using timing heuristics and output pattern matching.
        //
        // Here we just check if the process is still running.
        let state = self.get_process_state(pid)?;

        // If the process is running, it *might* be waiting for input
        // The higher-level detection logic will make the final determination
        let might_be_waiting = state == ProcessState::Running;

        debug!(
            pid,
            ?state,
            might_be_waiting,
            "Windows input detection (basic check)"
        );

        // Return false by default - actual detection happens in pattern matching
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_windows_monitor_new() {
        let monitor = WindowsMonitor::new();
        let _ = format!("{:?}", monitor);
    }

    #[test]
    fn test_windows_monitor_default() {
        let monitor = WindowsMonitor;
        let _ = format!("{:?}", monitor);
    }

    #[test]
    fn test_get_current_process_state() {
        let monitor = WindowsMonitor::new();
        let pid = std::process::id();
        let result = monitor.get_process_state(pid);
        assert!(result.is_ok(), "Failed to get state: {:?}", result.err());
        // Current process should be Running
        assert_eq!(result.unwrap(), ProcessState::Running);
    }

    #[test]
    fn test_get_process_state_nonexistent() {
        let monitor = WindowsMonitor::new();
        // Use a very high PID that's unlikely to exist
        let result = monitor.get_process_state(999_999_999);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_current_process_info() {
        let monitor = WindowsMonitor::new();
        let pid = std::process::id();
        let result = monitor.get_process_info(pid);
        assert!(result.is_ok(), "Failed to get info: {:?}", result.err());

        let info = result.unwrap();
        assert_eq!(info.pid, pid);
        assert!(info.ppid.is_some());
        assert!(info.command.is_some());
    }

    #[test]
    fn test_get_process_info_nonexistent() {
        let monitor = WindowsMonitor::new();
        let result = monitor.get_process_info(999_999_999);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_child_processes_current() {
        let monitor = WindowsMonitor::new();
        let pid = std::process::id();
        let result = monitor.get_child_processes(pid);
        assert!(result.is_ok(), "Failed to get children: {:?}", result.err());
        // Current test process probably doesn't have children, but should not error
    }

    #[test]
    fn test_get_foreground_pgid_returns_negative_one() {
        let monitor = WindowsMonitor::new();
        // On Windows, this should always return -1
        let result = monitor.get_foreground_pgid(0);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), -1);
    }

    #[test]
    fn test_is_likely_waiting_for_input_current_process() {
        let monitor = WindowsMonitor::new();
        let pid = std::process::id();
        // On Windows, this always returns false (detection happens elsewhere)
        let result = monitor.is_likely_waiting_for_input(pid, -1);
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[test]
    fn test_is_likely_waiting_for_input_nonexistent() {
        let monitor = WindowsMonitor::new();
        let result = monitor.is_likely_waiting_for_input(999_999_999, -1);
        assert!(result.is_err());
    }

    #[test]
    fn test_monitor_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<WindowsMonitor>();
    }

    #[test]
    fn test_find_process_entry_current() {
        let pid = std::process::id();
        let result = WindowsMonitor::find_process_entry(pid);
        assert!(result.is_ok());
        let entry = result.unwrap();
        assert_eq!(entry.th32ProcessID, pid);
    }

    #[test]
    fn test_find_process_entry_nonexistent() {
        let result = WindowsMonitor::find_process_entry(999_999_999);
        assert!(result.is_err());
    }
}
