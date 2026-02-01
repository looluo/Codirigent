//! Desktop notification support for Dirigent.
//!
//! This module provides cross-platform desktop notification functionality
//! to alert users when a session requires input.
//!
//! # Platform Support
//!
//! - **macOS**: Uses `osascript` to display native notifications
//! - **Linux**: Uses `notify-send` (requires libnotify)
//! - **Windows**: Logs notification intent (full toast support planned)
//!
//! # Example
//!
//! ```no_run
//! use dirigent_detector::notification::{send_notification, notify_input_required};
//! use dirigent_core::SessionId;
//!
//! // Send a generic notification
//! send_notification("Dirigent", "Hello from Dirigent!");
//!
//! // Notify that a session needs input
//! notify_input_required(SessionId(1), "Claude Code Session");
//! ```

use dirigent_core::SessionId;
use tracing::{debug, info, warn};

/// Default notification title for input required alerts.
pub const DEFAULT_TITLE: &str = "Dirigent - Input Required";

/// Send a desktop notification.
///
/// This function is platform-aware and uses the appropriate native
/// notification mechanism for each operating system.
///
/// # Arguments
///
/// * `title` - The notification title
/// * `body` - The notification body text
///
/// # Platform Behavior
///
/// - **macOS**: Uses AppleScript via `osascript`
/// - **Linux**: Uses `notify-send` command
/// - **Windows**: Currently logs the notification (toast support planned)
///
/// # Example
///
/// ```no_run
/// use dirigent_detector::notification::send_notification;
///
/// send_notification("Alert", "Something happened!");
/// ```
pub fn send_notification(title: &str, body: &str) {
    #[cfg(target_os = "macos")]
    send_macos_notification(title, body);

    #[cfg(target_os = "linux")]
    send_linux_notification(title, body);

    #[cfg(target_os = "windows")]
    send_windows_notification(title, body);
}

/// Notify that a session is waiting for user input.
///
/// This is a convenience function that formats an appropriate message
/// and sends a desktop notification.
///
/// # Arguments
///
/// * `session_id` - The ID of the session requiring input
/// * `session_name` - Human-readable name of the session
///
/// # Example
///
/// ```no_run
/// use dirigent_detector::notification::notify_input_required;
/// use dirigent_core::SessionId;
///
/// notify_input_required(SessionId(1), "Backend API");
/// ```
pub fn notify_input_required(session_id: SessionId, session_name: &str) {
    let body = format!("Session '{}' is waiting for input", session_name);
    info!(%session_id, %session_name, "Sending input required notification");
    send_notification(DEFAULT_TITLE, &body);
}

/// Notify that a session has completed a task.
///
/// # Arguments
///
/// * `session_id` - The ID of the session
/// * `session_name` - Human-readable name of the session
/// * `success` - Whether the task completed successfully
///
/// # Example
///
/// ```no_run
/// use dirigent_detector::notification::notify_task_completed;
/// use dirigent_core::SessionId;
///
/// notify_task_completed(SessionId(1), "Backend API", true);
/// ```
pub fn notify_task_completed(session_id: SessionId, session_name: &str, success: bool) {
    let title = if success {
        "Dirigent - Task Completed"
    } else {
        "Dirigent - Task Failed"
    };
    let body = format!(
        "Session '{}' {}",
        session_name,
        if success {
            "completed successfully"
        } else {
            "encountered an error"
        }
    );
    info!(%session_id, %session_name, success, "Sending task completion notification");
    send_notification(title, &body);
}

/// Notify that a session has an error.
///
/// # Arguments
///
/// * `session_id` - The ID of the session
/// * `session_name` - Human-readable name of the session
/// * `error_message` - Brief description of the error
///
/// # Example
///
/// ```no_run
/// use dirigent_detector::notification::notify_error;
/// use dirigent_core::SessionId;
///
/// notify_error(SessionId(1), "Backend API", "Build failed");
/// ```
pub fn notify_error(session_id: SessionId, session_name: &str, error_message: &str) {
    let title = "Dirigent - Error";
    let body = format!("Session '{}': {}", session_name, error_message);
    info!(%session_id, %session_name, %error_message, "Sending error notification");
    send_notification(title, &body);
}

/// macOS notification using osascript.
#[cfg(target_os = "macos")]
fn send_macos_notification(title: &str, body: &str) {
    use std::process::Command;

    // Escape quotes for AppleScript
    let escaped_title = title.replace('\\', "\\\\").replace('"', "\\\"");
    let escaped_body = body.replace('\\', "\\\\").replace('"', "\\\"");

    let script = format!(
        r#"display notification "{}" with title "{}""#,
        escaped_body, escaped_title
    );

    match Command::new("osascript").arg("-e").arg(&script).output() {
        Ok(output) => {
            if output.status.success() {
                debug!("macOS notification sent successfully");
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                warn!(error = %stderr, "osascript returned error");
            }
        }
        Err(e) => {
            warn!(error = %e, "Failed to send macOS notification");
        }
    }
}

/// Linux notification using notify-send.
#[cfg(target_os = "linux")]
fn send_linux_notification(title: &str, body: &str) {
    use std::process::Command;

    match Command::new("notify-send")
        .arg("--app-name=Dirigent")
        .arg(title)
        .arg(body)
        .output()
    {
        Ok(output) => {
            if output.status.success() {
                debug!("Linux notification sent successfully");
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                warn!(error = %stderr, "notify-send returned error");
            }
        }
        Err(e) => {
            // notify-send might not be installed
            warn!(error = %e, "Failed to send Linux notification (is notify-send installed?)");
        }
    }
}

/// Windows notification (placeholder).
///
/// Full Windows toast notification support requires more complex setup
/// with the Windows notification APIs. For now, we log the notification.
#[cfg(target_os = "windows")]
fn send_windows_notification(title: &str, body: &str) {
    // Windows toast notifications require more complex setup
    // For Phase 1, we log the notification intent
    info!(
        %title,
        %body,
        "Windows notification requested (toast API not yet implemented)"
    );
    // TODO: Implement using windows-rs toast API
    // This would require adding features to the windows dependency:
    // - Win32_UI_Shell
    // - Win32_UI_WindowsAndMessaging
}

/// Check if notifications are supported on this platform.
///
/// # Returns
///
/// `true` if the platform supports notifications and the necessary
/// tools are available.
///
/// # Example
///
/// ```
/// use dirigent_detector::notification::notifications_supported;
///
/// if notifications_supported() {
///     println!("Notifications are available");
/// }
/// ```
#[allow(unreachable_code)]
pub fn notifications_supported() -> bool {
    #[cfg(target_os = "macos")]
    {
        // osascript is always available on macOS
        return true;
    }

    #[cfg(target_os = "linux")]
    {
        // Check if notify-send is available
        use std::process::Command;
        return Command::new("which")
            .arg("notify-send")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
    }

    #[cfg(target_os = "windows")]
    {
        // Windows toast API not yet implemented
        return false;
    }

    // Fallback for other platforms
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_title() {
        assert_eq!(DEFAULT_TITLE, "Dirigent - Input Required");
    }

    #[test]
    fn test_notify_input_required_does_not_panic() {
        // Just verify it doesn't panic
        notify_input_required(SessionId(1), "Test Session");
    }

    #[test]
    fn test_notify_task_completed_success() {
        // Just verify it doesn't panic
        notify_task_completed(SessionId(1), "Test Session", true);
    }

    #[test]
    fn test_notify_task_completed_failure() {
        // Just verify it doesn't panic
        notify_task_completed(SessionId(1), "Test Session", false);
    }

    #[test]
    fn test_notify_error_does_not_panic() {
        // Just verify it doesn't panic
        notify_error(SessionId(1), "Test Session", "Test error");
    }

    #[test]
    fn test_send_notification_does_not_panic() {
        // Just verify it doesn't panic
        send_notification("Test Title", "Test Body");
    }

    #[test]
    fn test_send_notification_with_special_chars() {
        // Test with quotes and backslashes
        send_notification("Test \"Title\"", "Body with 'quotes' and \\backslash");
    }

    #[test]
    fn test_send_notification_empty_strings() {
        // Test with empty strings
        send_notification("", "");
    }

    #[test]
    fn test_send_notification_long_text() {
        // Test with long text
        let long_title = "A".repeat(100);
        let long_body = "B".repeat(500);
        send_notification(&long_title, &long_body);
    }

    #[test]
    fn test_send_notification_unicode() {
        // Test with unicode characters
        send_notification("Test", "Hello World!");
    }

    #[test]
    fn test_notifications_supported_returns_bool() {
        let result = notifications_supported();
        // Just verify it returns a boolean and doesn't panic
        let _ = result;
    }

    #[test]
    fn test_notify_input_required_with_empty_name() {
        notify_input_required(SessionId(0), "");
    }

    #[test]
    fn test_notify_input_required_with_special_chars() {
        notify_input_required(SessionId(1), "Session 'with' \"quotes\"");
    }

    #[test]
    fn test_notify_task_completed_with_special_name() {
        notify_task_completed(SessionId(1), "Test & Session <1>", true);
    }

    #[test]
    fn test_notify_error_with_multiline_message() {
        notify_error(SessionId(1), "Test", "Error\nwith\nmultiple\nlines");
    }

    // Platform-specific tests
    #[cfg(target_os = "macos")]
    mod macos_tests {
        use super::*;

        #[test]
        fn test_macos_notification_escaping() {
            // Test that escaping doesn't cause issues
            send_notification(
                "Title with \"quotes\"",
                "Body with \\backslash and \"quotes\"",
            );
        }

        #[test]
        fn test_notifications_supported_macos() {
            // macOS always has osascript
            assert!(notifications_supported());
        }
    }

    #[cfg(target_os = "linux")]
    mod linux_tests {
        use super::*;

        #[test]
        fn test_linux_notification_basic() {
            // This might fail if notify-send is not installed
            send_notification("Test", "Test notification");
        }

        #[test]
        fn test_notifications_supported_linux() {
            // This depends on whether notify-send is installed
            let _ = notifications_supported();
        }
    }

    #[cfg(target_os = "windows")]
    mod windows_tests {
        use super::*;

        #[test]
        fn test_windows_notification_placeholder() {
            // Windows just logs for now
            send_notification("Test", "Test notification");
        }

        #[test]
        fn test_notifications_supported_windows() {
            // Windows toast API not yet implemented
            assert!(!notifications_supported());
        }
    }
}
