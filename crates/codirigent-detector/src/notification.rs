//! Desktop notification support for Codirigent.
//!
//! This module provides cross-platform desktop notification functionality
//! to alert users when a session requires input.
//!
//! # Platform Support
//!
//! - **macOS**: Uses `osascript` to display native notifications
//! - **Windows**: Logs notification intent (full toast support planned)
//!
//! # Example
//!
//! ```no_run
//! use codirigent_detector::notification::{send_notification, notify_input_required};
//! use codirigent_core::SessionId;
//!
//! // Send a generic notification
//! send_notification("Codirigent", "Hello from Codirigent!");
//!
//! // Notify that a session needs input
//! notify_input_required(SessionId(1), "Claude Code Session");
//! ```

use codirigent_core::config::NotificationSettings;
use codirigent_core::SessionId;
use std::collections::HashMap;
use std::time::Instant;
use tracing::{debug, info, warn};

use std::sync::mpsc;
use std::thread;

/// Types of notifications that can be sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NotificationType {
    /// Session is waiting for user input.
    InputRequired,
    /// A task completed successfully.
    TaskCompleted,
    /// A task failed.
    TaskFailed,
    /// Session needs permission for a tool.
    PermissionPrompt,
    /// Agent finished responding in a background session.
    ResponseReady,
    /// Session encountered an error.
    Error,
}

/// Manages notification dispatch with per-type filtering and per-session cooldown.
///
/// All notification calls should go through this manager instead of calling
/// the free functions directly. The manager checks:
/// 1. Master toggle (`desktop`) — if false, all notifications are suppressed
/// 2. Per-type toggle — each notification type can be individually disabled
/// 3. Per-session cooldown — suppresses repeated notifications within a time window
pub struct NotificationManager {
    settings: NotificationSettings,
    last_sent: HashMap<SessionId, Instant>,
}

impl NotificationManager {
    /// Create a new manager with the given settings.
    pub fn new(settings: NotificationSettings) -> Self {
        Self {
            settings,
            last_sent: HashMap::new(),
        }
    }

    /// Update settings at runtime (e.g., user changed config).
    pub fn update_settings(&mut self, settings: NotificationSettings) {
        self.settings = settings;
    }

    /// Send a notification if permitted by settings and cooldown.
    ///
    /// Returns `true` if the notification was sent, `false` if suppressed.
    pub fn notify(
        &mut self,
        kind: NotificationType,
        session_id: SessionId,
        session_name: &str,
        detail: Option<&str>,
    ) -> bool {
        if !self.settings.desktop {
            debug!("Notification suppressed: desktop notifications disabled");
            return false;
        }

        if !self.is_type_enabled(&kind) {
            debug!(?kind, "Notification suppressed: type disabled");
            return false;
        }

        if !self.cooldown_elapsed(session_id) {
            debug!(%session_id, "Notification suppressed: cooldown active");
            return false;
        }

        match kind {
            NotificationType::InputRequired => {
                notify_input_required(session_id, session_name);
            }
            NotificationType::TaskCompleted => {
                notify_task_completed(session_id, session_name, true);
            }
            NotificationType::TaskFailed => {
                notify_task_completed(session_id, session_name, false);
            }
            NotificationType::PermissionPrompt => {
                let body = match detail {
                    Some(tool) => format!("'{}' needs permission for {}", session_name, tool),
                    None => format!("'{}' needs your permission", session_name),
                };
                send_notification("Codirigent", &body);
            }
            NotificationType::ResponseReady => {
                let body = format!("'{}' finished responding", session_name);
                send_notification("Codirigent", &body);
            }
            NotificationType::Error => {
                let error_msg = detail.unwrap_or("Unknown error");
                notify_error(session_id, session_name, error_msg);
            }
        }

        self.last_sent.insert(session_id, Instant::now());
        true
    }

    fn is_type_enabled(&self, kind: &NotificationType) -> bool {
        match kind {
            NotificationType::InputRequired => self.settings.input_required,
            NotificationType::TaskCompleted => self.settings.task_completed,
            NotificationType::TaskFailed => self.settings.task_failed,
            NotificationType::PermissionPrompt => self.settings.permission_prompt,
            NotificationType::ResponseReady => self.settings.response_ready,
            NotificationType::Error => self.settings.error,
        }
    }

    fn cooldown_elapsed(&self, session_id: SessionId) -> bool {
        if self.settings.cooldown_seconds == 0 {
            return true;
        }
        match self.last_sent.get(&session_id) {
            Some(last) => last.elapsed().as_secs() >= self.settings.cooldown_seconds,
            None => true,
        }
    }
}

/// Commands sent from `NotificationHandle` to the background `NotificationActor`.
enum NotificationCommand {
    /// Send a desktop notification (subject to toggle/cooldown checks).
    Send {
        kind: NotificationType,
        session_id: SessionId,
        session_name: String,
        detail: Option<String>,
    },
    /// Update notification settings at runtime.
    UpdateSettings(NotificationSettings),
}

/// Background actor that owns notification state and performs blocking OS calls.
///
/// Runs on a dedicated `std::thread` — never on the UI thread or a tokio worker.
/// Receives commands via `std::sync::mpsc::Receiver`. Exits when the channel closes
/// (all `NotificationHandle` instances dropped).
struct NotificationActor {
    rx: mpsc::Receiver<NotificationCommand>,
    settings: NotificationSettings,
    last_sent: HashMap<SessionId, Instant>,
}

impl NotificationActor {
    fn run(mut self) {
        debug!("Notification actor started");
        while let Ok(cmd) = self.rx.recv() {
            let result =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.handle(cmd)));
            if let Err(e) = result {
                warn!("Notification actor recovered from panic: {:?}", e);
            }
        }
        debug!("Notification actor stopped: channel closed");
    }

    fn handle(&mut self, cmd: NotificationCommand) {
        match cmd {
            NotificationCommand::Send {
                kind,
                session_id,
                ref session_name,
                ref detail,
            } => {
                if !self.settings.desktop {
                    debug!("Notification suppressed: desktop notifications disabled");
                    return;
                }
                if !self.is_type_enabled(&kind) {
                    debug!(?kind, "Notification suppressed: type disabled");
                    return;
                }
                if !self.cooldown_elapsed(session_id) {
                    debug!(%session_id, "Notification suppressed: cooldown active");
                    return;
                }

                match kind {
                    NotificationType::InputRequired => {
                        notify_input_required(session_id, session_name);
                    }
                    NotificationType::TaskCompleted => {
                        notify_task_completed(session_id, session_name, true);
                    }
                    NotificationType::TaskFailed => {
                        notify_task_completed(session_id, session_name, false);
                    }
                    NotificationType::PermissionPrompt => {
                        let body = match detail.as_deref() {
                            Some(tool) => {
                                format!("'{}' needs permission for {}", session_name, tool)
                            }
                            None => format!("'{}' needs your permission", session_name),
                        };
                        send_notification("Codirigent", &body);
                    }
                    NotificationType::ResponseReady => {
                        let body = format!("'{}' finished responding", session_name);
                        send_notification("Codirigent", &body);
                    }
                    NotificationType::Error => {
                        let error_msg = detail.as_deref().unwrap_or("Unknown error");
                        notify_error(session_id, session_name, error_msg);
                    }
                }

                self.last_sent.insert(session_id, Instant::now());
            }
            NotificationCommand::UpdateSettings(settings) => {
                debug!("Notification actor: settings updated");
                self.settings = settings;
            }
        }
    }

    fn is_type_enabled(&self, kind: &NotificationType) -> bool {
        match kind {
            NotificationType::InputRequired => self.settings.input_required,
            NotificationType::TaskCompleted => self.settings.task_completed,
            NotificationType::TaskFailed => self.settings.task_failed,
            NotificationType::PermissionPrompt => self.settings.permission_prompt,
            NotificationType::ResponseReady => self.settings.response_ready,
            NotificationType::Error => self.settings.error,
        }
    }

    fn cooldown_elapsed(&self, session_id: SessionId) -> bool {
        if self.settings.cooldown_seconds == 0 {
            return true;
        }
        match self.last_sent.get(&session_id) {
            Some(last) => last.elapsed().as_secs() >= self.settings.cooldown_seconds,
            None => true,
        }
    }
}

/// Non-blocking handle for sending notifications from any thread.
///
/// Wraps an `mpsc::Sender` to a background `NotificationActor`. All methods
/// take `&self` and return immediately — the actual notification dispatch
/// (including blocking OS calls) happens on the actor's dedicated thread.
///
/// The actor thread exits automatically when all `NotificationHandle` clones
/// are dropped.
#[derive(Clone)]
pub struct NotificationHandle {
    tx: mpsc::Sender<NotificationCommand>,
}

impl NotificationHandle {
    /// Spawn the background notification actor and return a handle.
    ///
    /// The actor runs on a dedicated OS thread named `"notification-actor"`.
    /// It processes commands sequentially, applying toggle/cooldown checks
    /// before making blocking platform notification calls.
    pub fn new(settings: NotificationSettings) -> Self {
        let (tx, rx) = mpsc::channel();
        let actor = NotificationActor {
            rx,
            settings,
            last_sent: HashMap::new(),
        };
        thread::Builder::new()
            .name("notification-actor".into())
            .spawn(move || actor.run())
            .expect("failed to spawn notification actor thread");
        Self { tx }
    }

    /// Queue a notification for background dispatch.
    ///
    /// Returns immediately. The actor will check master toggle, per-type
    /// toggles, and per-session cooldown before sending.
    pub fn send(
        &self,
        kind: NotificationType,
        session_id: SessionId,
        session_name: &str,
        detail: Option<&str>,
    ) {
        if let Err(e) = self.tx.send(NotificationCommand::Send {
            kind,
            session_id,
            session_name: session_name.to_owned(),
            detail: detail.map(|s| s.to_owned()),
        }) {
            warn!("Notification actor unreachable: {}", e);
        }
    }

    /// Update notification settings on the actor (non-blocking).
    pub fn update_settings(&self, settings: NotificationSettings) {
        if let Err(e) = self.tx.send(NotificationCommand::UpdateSettings(settings)) {
            warn!("Notification actor unreachable (settings update): {}", e);
        }
    }
}

/// Default notification title for input required alerts.
pub const DEFAULT_TITLE: &str = "Codirigent - Input Required";

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
/// - **Windows**: Currently logs the notification (toast support planned)
///
/// # Example
///
/// ```no_run
/// use codirigent_detector::notification::send_notification;
///
/// send_notification("Alert", "Something happened!");
/// ```
pub fn send_notification(title: &str, body: &str) {
    // Skip real OS notifications during test builds to avoid
    // spamming the desktop when running cargo test.
    if cfg!(test) {
        debug!("Test mode: skipping notification '{}': {}", title, body);
        return;
    }

    #[cfg(target_os = "macos")]
    send_macos_notification(title, body);

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
/// use codirigent_detector::notification::notify_input_required;
/// use codirigent_core::SessionId;
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
/// use codirigent_detector::notification::notify_task_completed;
/// use codirigent_core::SessionId;
///
/// notify_task_completed(SessionId(1), "Backend API", true);
/// ```
pub fn notify_task_completed(session_id: SessionId, session_name: &str, success: bool) {
    let title = if success {
        "Codirigent - Task Completed"
    } else {
        "Codirigent - Task Failed"
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
/// use codirigent_detector::notification::notify_error;
/// use codirigent_core::SessionId;
///
/// notify_error(SessionId(1), "Backend API", "Build failed");
/// ```
pub fn notify_error(session_id: SessionId, session_name: &str, error_message: &str) {
    let title = "Codirigent - Error";
    let body = format!("Session '{}': {}", session_name, error_message);
    info!(%session_id, %session_name, %error_message, "Sending error notification");
    send_notification(title, &body);
}

/// Sanitize a string for safe use in AppleScript.
///
/// Escapes backslashes and double quotes, and replaces control characters
/// (newlines, carriage returns, tabs) with spaces to prevent injection attacks.
#[cfg(target_os = "macos")]
fn sanitize_for_applescript(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace(['\n', '\r', '\t'], " ")
}

/// macOS notification using osascript.
#[cfg(target_os = "macos")]
fn send_macos_notification(title: &str, body: &str) {
    use std::process::Command;

    // Sanitize strings for AppleScript (escape special chars, remove control chars)
    let escaped_title = sanitize_for_applescript(title);
    let escaped_body = sanitize_for_applescript(body);

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

/// Windows toast notification using winrt-notification.
///
/// Shows a Windows toast notification using the PowerShell App ID,
/// which allows notifications without registering a custom app ID.
#[cfg(target_os = "windows")]
fn send_windows_notification(title: &str, body: &str) {
    use winrt_notification::{Duration, Sound, Toast};

    match Toast::new(Toast::POWERSHELL_APP_ID)
        .title(title)
        .text1(body)
        .sound(Some(Sound::Default))
        .duration(Duration::Short)
        .show()
    {
        Ok(()) => {
            debug!(
                %title,
                "Windows toast notification sent successfully"
            );
        }
        Err(e) => {
            warn!(
                %title,
                error = ?e,
                "Failed to send Windows toast notification"
            );
        }
    }
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
/// use codirigent_detector::notification::notifications_supported;
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

    #[cfg(target_os = "windows")]
    {
        // Windows toast notifications are supported on Windows 10+
        return true;
    }

    // Fallback for other platforms
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_title() {
        assert_eq!(DEFAULT_TITLE, "Codirigent - Input Required");
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

    #[test]
    #[cfg(target_os = "macos")]
    fn test_sanitize_for_applescript_basic() {
        let result = sanitize_for_applescript("hello");
        assert_eq!(result, "hello");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_sanitize_for_applescript_quotes() {
        let result = sanitize_for_applescript("hello \"world\"");
        assert_eq!(result, "hello \\\"world\\\"");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_sanitize_for_applescript_backslash() {
        let result = sanitize_for_applescript("path\\to\\file");
        assert_eq!(result, "path\\\\to\\\\file");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_sanitize_for_applescript_newline() {
        let result = sanitize_for_applescript("line1\nline2");
        assert_eq!(result, "line1 line2");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_sanitize_for_applescript_carriage_return() {
        let result = sanitize_for_applescript("line1\rline2");
        assert_eq!(result, "line1 line2");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_sanitize_for_applescript_tab() {
        let result = sanitize_for_applescript("col1\tcol2");
        assert_eq!(result, "col1 col2");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_sanitize_for_applescript_all_control_chars() {
        let result = sanitize_for_applescript("a\nb\rc\td");
        assert_eq!(result, "a b c d");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_sanitize_for_applescript_combined() {
        let result = sanitize_for_applescript("\"hello\"\nworld\\path");
        assert_eq!(result, "\\\"hello\\\" world\\\\path");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_sanitize_for_applescript_empty() {
        let result = sanitize_for_applescript("");
        assert_eq!(result, "");
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
            // Windows toast API is supported via winrt_notification
            assert!(notifications_supported());
        }
    }

    // NotificationManager tests

    #[test]
    fn test_notification_manager_sends_when_enabled() {
        let settings = NotificationSettings::default();
        let mut manager = NotificationManager::new(settings);
        let sent = manager.notify(
            NotificationType::InputRequired,
            SessionId(1),
            "Test Session",
            None,
        );
        assert!(sent);
    }

    #[test]
    fn test_notification_manager_master_toggle_off() {
        let settings = NotificationSettings {
            desktop: false,
            ..Default::default()
        };
        let mut manager = NotificationManager::new(settings);
        let sent = manager.notify(
            NotificationType::InputRequired,
            SessionId(1),
            "Test Session",
            None,
        );
        assert!(!sent);
    }

    #[test]
    fn test_notification_manager_per_type_toggle_off() {
        let settings = NotificationSettings {
            input_required: false,
            ..Default::default()
        };
        let mut manager = NotificationManager::new(settings);
        let sent = manager.notify(NotificationType::InputRequired, SessionId(1), "Test", None);
        assert!(!sent);

        let mut manager2 = NotificationManager::new(NotificationSettings {
            input_required: false,
            cooldown_seconds: 0,
            ..Default::default()
        });
        let sent = manager2.notify(NotificationType::TaskCompleted, SessionId(1), "Test", None);
        assert!(sent);
    }

    #[test]
    fn test_notification_manager_cooldown_suppresses() {
        let settings = NotificationSettings {
            cooldown_seconds: 60,
            ..Default::default()
        };
        let mut manager = NotificationManager::new(settings);

        let sent1 = manager.notify(NotificationType::InputRequired, SessionId(1), "Test", None);
        assert!(sent1);

        let sent2 = manager.notify(NotificationType::TaskCompleted, SessionId(1), "Test", None);
        assert!(!sent2);
    }

    #[test]
    fn test_notification_manager_cooldown_per_session() {
        let settings = NotificationSettings {
            cooldown_seconds: 60,
            ..Default::default()
        };
        let mut manager = NotificationManager::new(settings);

        let sent1 = manager.notify(
            NotificationType::InputRequired,
            SessionId(1),
            "Session A",
            None,
        );
        assert!(sent1);

        let sent2 = manager.notify(
            NotificationType::InputRequired,
            SessionId(2),
            "Session B",
            None,
        );
        assert!(sent2);
    }

    #[test]
    fn test_notification_manager_zero_cooldown() {
        let settings = NotificationSettings {
            cooldown_seconds: 0,
            ..Default::default()
        };
        let mut manager = NotificationManager::new(settings);

        let sent1 = manager.notify(NotificationType::InputRequired, SessionId(1), "Test", None);
        assert!(sent1);

        let sent2 = manager.notify(NotificationType::InputRequired, SessionId(1), "Test", None);
        assert!(sent2);
    }

    #[test]
    fn test_notification_manager_update_settings() {
        let settings = NotificationSettings::default();
        let mut manager = NotificationManager::new(settings);

        let sent = manager.notify(NotificationType::InputRequired, SessionId(1), "Test", None);
        assert!(sent);

        manager.update_settings(NotificationSettings {
            desktop: false,
            ..Default::default()
        });

        let sent = manager.notify(NotificationType::InputRequired, SessionId(2), "Test", None);
        assert!(!sent);
    }

    #[test]
    fn test_notification_manager_permission_prompt() {
        let settings = NotificationSettings {
            cooldown_seconds: 0,
            ..Default::default()
        };
        let mut manager = NotificationManager::new(settings);
        let sent = manager.notify(
            NotificationType::PermissionPrompt,
            SessionId(1),
            "Test",
            Some("bash"),
        );
        assert!(sent);
    }

    #[test]
    fn test_notification_manager_task_failed_toggle() {
        let settings = NotificationSettings {
            task_failed: false,
            cooldown_seconds: 0,
            ..Default::default()
        };
        let mut manager = NotificationManager::new(settings);
        let sent = manager.notify(NotificationType::TaskFailed, SessionId(1), "Test", None);
        assert!(!sent);
    }

    #[test]
    fn test_notification_manager_error_toggle() {
        let settings = NotificationSettings {
            error: false,
            cooldown_seconds: 0,
            ..Default::default()
        };
        let mut manager = NotificationManager::new(settings);
        let sent = manager.notify(
            NotificationType::Error,
            SessionId(1),
            "Test",
            Some("Build failed"),
        );
        assert!(!sent);
    }

    // ── NotificationActor tests ──

    #[test]
    fn test_actor_processes_send_command() {
        let (tx, rx) = mpsc::channel();
        let actor = NotificationActor {
            rx,
            settings: NotificationSettings::default(),
            last_sent: HashMap::new(),
        };
        tx.send(NotificationCommand::Send {
            kind: NotificationType::InputRequired,
            session_id: SessionId(1),
            session_name: "Test".to_owned(),
            detail: None,
        })
        .unwrap();
        drop(tx);
        actor.run();
    }

    #[test]
    fn test_actor_cooldown_suppresses_second_notification() {
        let (_tx, rx) = mpsc::channel();
        let settings = NotificationSettings {
            cooldown_seconds: 60,
            ..Default::default()
        };
        let mut actor = NotificationActor {
            rx,
            settings,
            last_sent: HashMap::new(),
        };

        let cmd1 = NotificationCommand::Send {
            kind: NotificationType::InputRequired,
            session_id: SessionId(1),
            session_name: "Test".to_owned(),
            detail: None,
        };
        actor.handle(cmd1);
        assert!(actor.last_sent.contains_key(&SessionId(1)));

        let prev_time = *actor.last_sent.get(&SessionId(1)).unwrap();
        let cmd2 = NotificationCommand::Send {
            kind: NotificationType::InputRequired,
            session_id: SessionId(1),
            session_name: "Test".to_owned(),
            detail: None,
        };
        actor.handle(cmd2);
        assert_eq!(*actor.last_sent.get(&SessionId(1)).unwrap(), prev_time);
    }

    #[test]
    fn test_actor_per_session_cooldown_independent() {
        let (_tx, rx) = mpsc::channel();
        let settings = NotificationSettings {
            cooldown_seconds: 60,
            ..Default::default()
        };
        let mut actor = NotificationActor {
            rx,
            settings,
            last_sent: HashMap::new(),
        };

        let cmd1 = NotificationCommand::Send {
            kind: NotificationType::InputRequired,
            session_id: SessionId(1),
            session_name: "A".to_owned(),
            detail: None,
        };
        actor.handle(cmd1);

        let cmd2 = NotificationCommand::Send {
            kind: NotificationType::InputRequired,
            session_id: SessionId(2),
            session_name: "B".to_owned(),
            detail: None,
        };
        actor.handle(cmd2);

        assert!(actor.last_sent.contains_key(&SessionId(1)));
        assert!(actor.last_sent.contains_key(&SessionId(2)));
    }

    #[test]
    fn test_actor_update_settings_disables_notifications() {
        let (_tx, rx) = mpsc::channel();
        let mut actor = NotificationActor {
            rx,
            settings: NotificationSettings::default(),
            last_sent: HashMap::new(),
        };

        actor.handle(NotificationCommand::UpdateSettings(NotificationSettings {
            desktop: false,
            ..Default::default()
        }));

        actor.handle(NotificationCommand::Send {
            kind: NotificationType::InputRequired,
            session_id: SessionId(1),
            session_name: "Test".to_owned(),
            detail: None,
        });
        assert!(actor.last_sent.is_empty());
    }

    #[test]
    fn test_actor_type_toggle_respected() {
        let (_tx, rx) = mpsc::channel();
        let mut actor = NotificationActor {
            rx,
            settings: NotificationSettings {
                input_required: false,
                cooldown_seconds: 0,
                ..Default::default()
            },
            last_sent: HashMap::new(),
        };

        actor.handle(NotificationCommand::Send {
            kind: NotificationType::InputRequired,
            session_id: SessionId(1),
            session_name: "Test".to_owned(),
            detail: None,
        });
        assert!(actor.last_sent.is_empty());

        actor.handle(NotificationCommand::Send {
            kind: NotificationType::TaskCompleted,
            session_id: SessionId(1),
            session_name: "Test".to_owned(),
            detail: None,
        });
        assert!(actor.last_sent.contains_key(&SessionId(1)));
    }

    // ── NotificationHandle tests ──

    #[test]
    fn test_notification_handle_send_does_not_panic() {
        let handle = NotificationHandle::new(NotificationSettings::default());
        handle.send(NotificationType::InputRequired, SessionId(1), "Test", None);
    }

    #[test]
    fn test_notification_handle_update_settings() {
        let handle = NotificationHandle::new(NotificationSettings::default());
        handle.update_settings(NotificationSettings {
            desktop: false,
            ..Default::default()
        });
    }

    #[test]
    fn test_notification_handle_is_clone() {
        let handle = NotificationHandle::new(NotificationSettings::default());
        let _clone = handle.clone();
    }

    #[test]
    fn test_notification_handle_send_after_clone_drop() {
        let handle = NotificationHandle::new(NotificationSettings::default());
        drop(handle.clone());
        handle.send(NotificationType::InputRequired, SessionId(1), "Test", None);
    }
}
