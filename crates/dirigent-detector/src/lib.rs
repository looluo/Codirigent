//! Dirigent Detector
//!
//! Process monitoring and status detection crate providing platform-specific
//! process state monitoring, output pattern matching, and CLI-specific
//! heuristics for detecting session status in Dirigent.
//!
//! # Overview
//!
//! This crate provides the foundation for detecting when AI CLI sessions
//! (Claude Code, Codex CLI, Gemini CLI) are waiting for user input. It uses
//! a combination of:
//!
//! - Platform-specific process monitoring (via `/proc` on Linux, `libproc` on
//!   macOS, and Win32 APIs on Windows)
//! - Output pattern matching to detect common input prompts
//! - Timing heuristics to identify idle processes
//!
//! # Platform Support
//!
//! The crate supports three platforms:
//!
//! - **Linux**: Uses the `/proc` filesystem via the `procfs` crate
//! - **macOS**: Uses `libproc` for BSD process information
//! - **Windows**: Uses Win32 APIs (ToolHelp32, process status)
//!
//! # Example
//!
//! ```no_run
//! use dirigent_detector::{NativeMonitor, PlatformMonitor, ProcessState};
//!
//! let monitor = NativeMonitor::new();
//! let pid = std::process::id();
//!
//! // Get process state
//! let state = monitor.get_process_state(pid).unwrap();
//! println!("Current process state: {}", state);
//!
//! // Get detailed process info
//! let info = monitor.get_process_info(pid).unwrap();
//! println!("Process: {} (PID: {})", info.command.unwrap_or_default(), info.pid);
//! ```

#![warn(missing_docs)]
#![warn(clippy::all)]

pub mod platform;

// Re-export main types for convenience
pub use platform::{PlatformMonitor, ProcessInfo, ProcessState};

// Re-export the native monitor for the current platform
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
pub use platform::NativeMonitor;

// Re-export the factory function
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
pub use platform::create_native_monitor;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_state_reexport() {
        // Verify that ProcessState is accessible from the crate root
        let state = ProcessState::Running;
        assert_eq!(format!("{}", state), "Running");
    }

    #[test]
    fn test_process_info_reexport() {
        // Verify that ProcessInfo is accessible from the crate root
        let info = ProcessInfo::new(1234, ProcessState::Running);
        assert_eq!(info.pid, 1234);
    }

    #[test]
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    fn test_native_monitor_reexport() {
        // Verify that NativeMonitor is accessible from the crate root
        let monitor = NativeMonitor::new();
        let pid = std::process::id();
        let result = monitor.get_process_state(pid);
        assert!(result.is_ok());
    }

    #[test]
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    fn test_create_native_monitor_reexport() {
        // Verify that create_native_monitor is accessible from the crate root
        let monitor = create_native_monitor();
        let pid = std::process::id();
        let result = monitor.get_process_state(pid);
        assert!(result.is_ok());
    }

    #[test]
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    fn test_platform_monitor_trait_reexport() {
        // Verify that PlatformMonitor trait is usable with NativeMonitor
        fn use_monitor(monitor: &dyn PlatformMonitor) -> bool {
            let pid = std::process::id();
            monitor.get_process_state(pid).is_ok()
        }

        let monitor = NativeMonitor::new();
        assert!(use_monitor(&monitor));
    }
}
