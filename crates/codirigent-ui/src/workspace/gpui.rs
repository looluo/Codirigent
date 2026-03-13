//! GPUI rendering implementation for Workspace.
//!
//! This module provides the GPUI View implementation for the workspace,
//! including rendering the grid of session panes with proper theming.
//!
//! # Architecture
//!
//! The `WorkspaceView` wraps a `Workspace` and provides:
//! - GPUI `Render` trait implementation for drawing the UI
//! - GPUI `Focusable` trait for keyboard focus management
//!
//! # Example
//!
//! ```ignore
//! use codirigent_ui::workspace::WorkspaceView;
//! use codirigent_ui::CodirigentApp;
//!
//! // In a window context:
//! let workspace = WorkspaceView::new(app, cx);
//! ```

use super::core::Workspace;
use super::editor_detection::detect_monospace_fonts;
use super::types::{
    CacheState, CliReaders, ModalState, PollingState, RenderLayoutSignature, SelectionState,
    TerminalResizeSignature, CELL_BORDER_WIDTH, FONT_SIZE_BASE_DEFAULT, HEADER_HEIGHT, REM_BASE,
    TERMINAL_CONTENT_PADDING,
};
use crate::clipboard_preview::ClipboardPreview;
use crate::empty_session::{EmptySessionEvent, EmptySessionPool};
use crate::input::{key_to_bytes, TerminalKeystroke, TerminalModifiers};
use crate::sidebar::{FileTreePanel, WorktreePanel};
use crate::task_board::TaskBoardPanel;
use crate::terminal_header::TerminalHeader;
use crate::terminal_view::TerminalView;
use crate::theme::CodirigentTheme;
use crate::toolbar::CustomLayoutPicker;
use codirigent_core::compaction::{CompactionConfig, CompactionService};
use codirigent_core::{
    CodexExecutionMode, CodirigentEvent, DefaultEventBus, EventBus, FileStorageService,
    GridPosition, ProcessMonitor, SessionId, SessionManager, SessionStatus, TaskManager,
    TaskManagerConfig,
};
use codirigent_detector::{InputDetector, NotificationManager};
use codirigent_filetree::FileTree;
use codirigent_session::clipboard_service::{ClipboardService, DefaultClipboardService};
use codirigent_session::DefaultSessionManager;
use gpui::{
    div, px, App, AppContext, Bounds, ClickEvent, Context, Entity, EntityInputHandler, FocusHandle,
    Focusable, InteractiveElement, IntoElement, KeyDownEvent, MouseButton, MouseMoveEvent,
    MouseUpEvent, ParentElement, Pixels, Render, StatefulInteractiveElement, Styled,
    UTF16Selection, Window,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracing::{info, warn};

fn session_project_name(session: &codirigent_core::Session) -> Option<String> {
    session
        .git_info
        .as_ref()
        .and_then(|git_info| git_info.repo_root.file_name())
        .or_else(|| session.working_directory.file_name())
        .and_then(|name| name.to_str())
        .map(str::to_owned)
}

fn resolved_task_title(
    task_id: &codirigent_core::TaskId,
    task_titles: Option<&HashMap<codirigent_core::TaskId, String>>,
) -> String {
    task_titles
        .and_then(|titles| titles.get(task_id))
        .cloned()
        .unwrap_or_else(|| task_id.0.to_string())
}

/// GPUI View wrapper for Workspace.
///
/// This is the main workspace view that renders the grid of session panes.
/// It wraps the core `Workspace` struct and provides GPUI rendering.
pub struct WorkspaceView {
    /// The underlying workspace state.
    pub(super) workspace: Workspace,
    /// Focus handle for keyboard navigation.
    focus_handle: FocusHandle,
    /// IME composition state: range of marked (composing) text.
    pub(super) ime_marked_range: Option<std::ops::Range<usize>>,
    /// Current IME pre-edit text shown during composition.
    pub(super) ime_preedit_text: Option<String>,
    /// Event bus for cross-module communication.
    pub(super) event_bus: Arc<DefaultEventBus>,
    /// Session manager for PTY and session lifecycle.
    pub(super) session_manager: Arc<Mutex<DefaultSessionManager>>,
    /// Input detector for monitoring session status.
    pub(super) detector: Arc<Mutex<InputDetector>>,
    /// Task manager for task lifecycle and assignment.
    pub(super) task_manager: Arc<Mutex<TaskManager>>,
    /// Terminal views for each session.
    pub(super) terminals: HashMap<SessionId, TerminalView>,
    /// Receivers for VTE PtyWrite events (DSR responses, etc.) per session.
    pub(super) pty_write_receivers:
        HashMap<SessionId, tokio::sync::mpsc::UnboundedReceiver<Vec<u8>>>,
    /// Next session ID counter (kept for UI session tracking).
    pub(super) next_session_id: u64,
    /// Custom layout picker modal state (extracted from deprecated SessionsToolbar).
    pub(super) custom_picker: CustomLayoutPicker,
    /// Title bar with window controls (minimize, maximize, close).
    pub(super) title_bar: crate::title_bar::TitleBar,
    /// Unified top bar component state.
    pub(super) top_bar: crate::top_bar::TopBar,
    /// Narrow icon rail (left edge).
    pub(super) icon_rail: crate::icon_rail::IconRail,
    /// Expandable drawer panel (next to icon rail).
    pub(super) drawer: crate::drawer::Drawer,
    /// Task board panel component state.
    pub(super) task_board: TaskBoardPanel,
    /// Empty session cells pool.
    pub(super) empty_cells: EmptySessionPool,
    /// Terminal headers by session ID.
    pub(super) terminal_headers: HashMap<SessionId, TerminalHeader>,
    /// Project and file tree state (file_tree, file_tree_model, project_root, worktree_panel, worktree_manager).
    pub(super) project: super::project_state::ProjectState,
    /// Clipboard state (smart_clipboard, clipboard_service, clipboard_preview, clipboard_preview_shown_at).
    pub(super) clipboard: super::clipboard_state::ClipboardState,
    /// Settings state (page, open, config_service).
    pub(super) settings: super::settings_state::SettingsState,
    /// Persistence services (storage, compaction).
    pub(super) persistence: super::persistence_state::PersistenceServices,

    // --- Grouped sub-state ---
    /// Modal dialog state (session action, task creation, profile deletion).
    pub(super) modals: ModalState,
    /// Selection and interaction state (session selection, menus, click tracking).
    pub(super) selection: SelectionState,
    /// Adaptive polling and timing state (output polling, resize throttle, git refresh).
    pub(super) polling: PollingState,
    /// Event-driven output dispatcher (replaces broad session scan).
    pub(super) output_dispatcher: super::output_dispatcher::OutputDispatcher,
    /// Receiver for the `SessionUpdate` mpsc channel from session manager.
    pub(super) update_rx: Option<tokio::sync::mpsc::Receiver<codirigent_core::SessionUpdate>>,
    /// Sender for the `SessionUpdate` mpsc channel (cloned to background tasks
    /// for OSC 133 / OSC 7 event emission).
    pub(super) update_tx: Option<tokio::sync::mpsc::Sender<codirigent_core::SessionUpdate>>,
    /// CLI session readers and process-tree detector (shared with background tasks).
    pub(super) cli_readers: Arc<Mutex<CliReaders>>,
    /// Cached detection results and memoized state.
    pub(super) cache: CacheState,
    /// Notification manager — enforces master toggle, per-type toggles, and cooldown.
    /// All desktop notifications must go through this instead of calling send_notification directly.
    pub(super) notification_manager: NotificationManager,
}

/// Returns `true` if the editor command refers to a terminal-based editor
/// (one that needs to run inside an existing terminal session).
impl WorkspaceView {
    /// Poll cadence while output is actively streaming.
    const ACTIVE_OUTPUT_POLL_INTERVAL_MS: u64 = 16;
    /// Poll cadence once output briefly goes idle.
    const LIGHT_IDLE_OUTPUT_POLL_INTERVAL_MS: u64 = 50;
    /// Poll cadence once output has been idle for a few seconds.
    const DEEP_IDLE_OUTPUT_POLL_INTERVAL_MS: u64 = 200;
    /// Number of active-rate idle polls before backing off to the light-idle interval.
    const LIGHT_IDLE_THRESHOLD_POLLS: u32 = 4;
    /// Number of consecutive idle polls before backing off to the deep-idle interval.
    const DEEP_IDLE_THRESHOLD_POLLS: u32 = 64;
    /// Maintenance cadence for non-output UI work (git refresh, clipboard, cleanup).
    const MAINTENANCE_POLL_INTERVAL_MS: u64 = 250;
    /// Debounce window for persisted app-state saves.
    const STATE_SAVE_DEBOUNCE: Duration = Duration::from_millis(200);

    /// Returns true when a computed target size is a transient collapse that
    /// should be ignored to avoid 1-column/1-row PTY resizes.
    fn should_skip_collapsed_resize(
        current_rows: u16,
        current_cols: u16,
        target_rows: u16,
        target_cols: u16,
    ) -> bool {
        let target_collapsed = target_rows <= 1 || target_cols <= 1;
        let current_usable = current_rows > 1 && current_cols > 1;
        target_collapsed && current_usable
    }

    fn session_is_shell_idle(&self, session_id: SessionId) -> bool {
        self.with_detector(|detector| {
            matches!(
                detector.get_status(session_id),
                Some(SessionStatus::Idle) | None
            )
        })
    }

    fn normalize_codex_execution_mode(command: &str) -> Option<CodexExecutionMode> {
        let tokens: Vec<&str> = command.split_whitespace().collect();
        let codex_index = tokens.iter().position(|token| *token == "codex")?;
        let args = &tokens[codex_index + 1..];

        let has_flag = |flag: &str| args.iter().any(|token| token.eq_ignore_ascii_case(flag));
        let option_value = |short: &str, long: &str| {
            args.windows(2).find_map(|window| {
                let [flag, value] = window else {
                    return None;
                };
                if flag.eq_ignore_ascii_case(short) || flag.eq_ignore_ascii_case(long) {
                    Some(*value)
                } else {
                    None
                }
            })
        };

        if has_flag("--dangerously-bypass-approvals-and-sandbox") || has_flag("--yolo") {
            return Some(CodexExecutionMode::Bypass);
        }

        if has_flag("--full-auto") {
            return Some(CodexExecutionMode::FullAuto);
        }

        let ask_policy = option_value("-a", "--ask-for-approval");
        let sandbox_mode = option_value("-s", "--sandbox");
        if ask_policy.is_some_and(|value| value.eq_ignore_ascii_case("never"))
            && sandbox_mode.is_some_and(|value| value.eq_ignore_ascii_case("danger-full-access"))
        {
            return Some(CodexExecutionMode::Bypass);
        }

        None
    }

    fn keystroke_is_text_input(event: &KeyDownEvent) -> bool {
        if event.keystroke.modifiers.control
            || event.keystroke.modifiers.alt
            || event.keystroke.modifiers.platform
            || event.keystroke.modifiers.function
        {
            return false;
        }

        let key: &str = event.keystroke.key.as_ref();

        // Special keys must go through the keydown → key_to_bytes path, not
        // the IME text-input path.  On macOS, GPUI may populate `key_char`
        // with control characters (e.g. "\r" for Enter, "\t" for Tab) which
        // would incorrectly classify them as text input and prevent the
        // keydown handler from sending the proper escape sequences.
        match key {
            "enter" | "backspace" | "delete" | "tab" | "escape" | "up" | "down" | "left"
            | "right" | "home" | "end" | "pageup" | "pagedown" | "insert" | "f1" | "f2" | "f3"
            | "f4" | "f5" | "f6" | "f7" | "f8" | "f9" | "f10" | "f11" | "f12" => {
                return false;
            }
            _ => {}
        }

        let is_plain_space = key == " " || key.eq_ignore_ascii_case("space");
        if is_plain_space {
            return true;
        }

        if event
            .keystroke
            .key_char
            .as_deref()
            .is_some_and(|text| !text.is_empty())
        {
            return true;
        }

        // On Windows IME layouts, pre-composition keystrokes can arrive with an
        // empty `key_char` before GPUI dispatches the eventual composition or
        // committed text via the input handler. Treat any plain printable key as
        // text input so it does not leak directly into the PTY and suppress the
        // IME preedit overlay.
        key.chars().count() == 1
    }

    pub(super) fn set_session_codex_execution_mode(
        &mut self,
        session_id: SessionId,
        mode: Option<CodexExecutionMode>,
        cx: &mut Context<Self>,
    ) {
        let mut changed = false;

        if let Ok(mgr) = self.session_manager.lock() {
            changed |= mgr
                .with_session_state_mut(session_id, |state| {
                    if state.session.codex_execution_mode != mode {
                        state.session.codex_execution_mode = mode;
                        true
                    } else {
                        false
                    }
                })
                .unwrap_or(false);
        }

        if let Some(session) = self.workspace.session_mut(session_id) {
            if session.codex_execution_mode != mode {
                session.codex_execution_mode = mode;
                changed = true;
            }
        }

        if changed {
            self.save_state_to_disk(cx);
        }
    }

    fn note_codex_command_submission(
        &mut self,
        session_id: SessionId,
        mode: Option<CodexExecutionMode>,
        cx: &mut Context<Self>,
    ) {
        let mut changed = false;
        let started_at = chrono::Utc::now();

        // Mark the pane as Codex immediately when the shell command is
        // submitted so background JSONL polling can report Working on the
        // same turn, even before the first hook signal arrives.
        self.clipboard
            .clipboard_service
            .set_session_cli_type(session_id, codirigent_core::CliType::CodexCli);

        if let Ok(mgr) = self.session_manager.lock() {
            changed |= mgr
                .with_session_state_mut(session_id, |state| {
                    let mut session_changed = false;
                    if state.session.codex_execution_mode != mode {
                        state.session.codex_execution_mode = mode;
                        session_changed = true;
                    }
                    if state.session.codex_started_at != Some(started_at) {
                        state.session.codex_started_at = Some(started_at);
                        session_changed = true;
                    }
                    if state.session.codex_session_id.take().is_some() {
                        session_changed = true;
                    }
                    session_changed
                })
                .unwrap_or(false);
        }

        if let Some(session) = self.workspace.session_mut(session_id) {
            if session.codex_execution_mode != mode {
                session.codex_execution_mode = mode;
                changed = true;
            }
            if session.codex_started_at != Some(started_at) {
                session.codex_started_at = Some(started_at);
                changed = true;
            }
            if session.codex_session_id.take().is_some() {
                changed = true;
            }
        }

        if changed {
            self.save_state_to_disk(cx);
        }
    }

    pub(super) fn capture_shell_text_input(&mut self, session_id: SessionId, text: &str) {
        if !self.session_is_shell_idle(session_id) {
            self.polling.shell_input_buffers.remove(&session_id);
            return;
        }

        if text.is_empty() {
            return;
        }

        let buffer = self
            .polling
            .shell_input_buffers
            .entry(session_id)
            .or_default();
        if buffer.len() >= 1024 {
            self.polling.shell_input_buffers.remove(&session_id);
            return;
        }
        buffer.push_str(text);
    }

    fn capture_shell_key_input(
        &mut self,
        session_id: SessionId,
        key: &str,
        cx: &mut Context<Self>,
    ) {
        if !self.session_is_shell_idle(session_id) {
            self.polling.shell_input_buffers.remove(&session_id);
            return;
        }

        match key {
            "backspace" => {
                if let Some(buffer) = self.polling.shell_input_buffers.get_mut(&session_id) {
                    buffer.pop();
                    if buffer.is_empty() {
                        self.polling.shell_input_buffers.remove(&session_id);
                    }
                }
            }
            "enter" => {
                let command = self.polling.shell_input_buffers.remove(&session_id);
                if let Some(command) = command {
                    let mode = Self::normalize_codex_execution_mode(command.trim());
                    if command.split_whitespace().any(|token| token == "codex") {
                        self.note_codex_command_submission(session_id, mode, cx);
                    }
                }
            }
            "escape" => {
                self.polling.shell_input_buffers.remove(&session_id);
            }
            _ => {}
        }
    }

    /// Create a new workspace view.
    ///
    /// # Arguments
    ///
    /// * `session_manager` - Session manager for PTY and session lifecycle
    /// * `detector` - Input detector for monitoring session status
    /// * `event_bus` - Event bus for cross-module communication
    /// * `theme` - Theme configuration
    /// * `cx` - GPUI context
    pub fn new(
        session_manager: Arc<Mutex<DefaultSessionManager>>,
        detector: Arc<Mutex<InputDetector>>,
        event_bus: Arc<DefaultEventBus>,
        theme: CodirigentTheme,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut workspace = Workspace::new();
        workspace.set_theme(theme);

        // Eagerly initialize the hook-signal run epoch so that signals
        // emitted between app start and the first scan are not dropped.
        super::impl_output_polling::init_app_start_ts();

        Self::start_output_polling(cx);
        Self::start_detector_maintenance_polling(cx);
        Self::start_maintenance_polling(cx);
        Self::start_modal_cursor_blink(cx);

        // Take the SessionUpdate receiver and clone the sender from the session
        // manager for the event-driven output dispatcher. The sender is cloned
        // to background tasks for OSC 133 / OSC 7 event emission.
        let (update_rx, update_tx) = match session_manager.lock() {
            Ok(mgr) => {
                let rx = mgr.take_update_receiver();
                let tx = Some(mgr.update_sender());
                (rx, tx)
            }
            Err(e) => {
                tracing::warn!("Failed to init SessionUpdate channel (mutex poisoned): {e}");
                (None, None)
            }
        };
        if update_rx.is_none() {
            tracing::error!(
                "Event-driven output pipeline unavailable; \
                 falling back to 1s legacy polling for all sessions"
            );
        }

        let (storage, task_manager) = Self::init_task_manager(event_bus.clone());
        let (file_tree, file_tree_model, project_root) = Self::init_file_tree();

        // Capture theme before workspace is moved
        let theme_for_clipboard = workspace.theme().clone();

        let mut view = Self {
            workspace,
            focus_handle: cx.focus_handle(),
            ime_marked_range: None,
            ime_preedit_text: None,
            event_bus,
            session_manager,
            detector,
            task_manager,
            terminals: HashMap::new(),
            pty_write_receivers: HashMap::new(),
            next_session_id: 1,
            custom_picker: CustomLayoutPicker::new(),
            title_bar: crate::title_bar::TitleBar::new(),
            top_bar: crate::top_bar::TopBar::new(),
            icon_rail: crate::icon_rail::IconRail::new(),
            drawer: crate::drawer::Drawer::new(),
            task_board: TaskBoardPanel::new(),
            empty_cells: EmptySessionPool::new(),
            terminal_headers: HashMap::new(),
            project: super::project_state::ProjectState {
                file_tree,
                file_tree_model,
                project_root: project_root.clone(),
                worktree_panel: WorktreePanel::new(),
                worktree_manager: None,
                root_cache: HashMap::new(),
            },
            clipboard: super::clipboard_state::ClipboardState {
                smart_clipboard: crate::platform::create_clipboard(),
                clipboard_service: DefaultClipboardService::new(
                    // Use a guaranteed user-writable directory for clipboard temp files.
                    // project_root is the CWD which may be C:\Program Files\... (unwritable)
                    // when launched from an installer shortcut.
                    dirs::data_local_dir()
                        .unwrap_or_else(std::env::temp_dir)
                        .join("Codirigent"),
                ),
                clipboard_preview: ClipboardPreview::new(theme_for_clipboard),
                clipboard_preview_shown_at: None,
                last_preview_image_signature: None,
            },
            settings: super::settings_state::SettingsState::new(),
            persistence: super::persistence_state::PersistenceServices {
                storage: storage.clone(),
                compaction: Arc::new(Mutex::new(CompactionService::new(
                    CompactionConfig::default(),
                ))),
            },
            modals: ModalState::new(),
            selection: SelectionState::new(),
            polling: PollingState::new(),
            output_dispatcher: super::output_dispatcher::OutputDispatcher::new(),
            update_rx,
            update_tx,
            cli_readers: Arc::new(Mutex::new(CliReaders::new())),
            cache: CacheState::new(),
            notification_manager: NotificationManager::new(Default::default()),
        };

        // Pre-detect editors and shells in the background so settings open instantly
        Self::start_detection_background(cx);
        view.start_settings_background_load(true, cx);

        if let Some(root) = project_root {
            view.set_project_root(root, cx);
        }

        view.refresh_derived_ui_state();
        view
    }

    /// Start adaptive output polling background task.
    ///
    /// Uses a frame-rate-friendly interval while output is active, then backs
    /// off once output has been quiet for a short period.
    fn start_output_polling(cx: &mut Context<Self>) {
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let mut poll_interval_ms: u64 = Self::ACTIVE_OUTPUT_POLL_INTERVAL_MS;
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(poll_interval_ms))
                    .await;
                let result = this.update(cx, |this, cx| {
                    this.poll_output(cx);
                    if this.polling.last_poll_had_output {
                        this.polling.idle_poll_count = 0;
                        poll_interval_ms = Self::ACTIVE_OUTPUT_POLL_INTERVAL_MS;
                    } else {
                        this.polling.idle_poll_count =
                            this.polling.idle_poll_count.saturating_add(1);
                        if this.polling.idle_poll_count > Self::DEEP_IDLE_THRESHOLD_POLLS {
                            poll_interval_ms = Self::DEEP_IDLE_OUTPUT_POLL_INTERVAL_MS;
                        } else if this.polling.idle_poll_count > Self::LIGHT_IDLE_THRESHOLD_POLLS {
                            poll_interval_ms = Self::LIGHT_IDLE_OUTPUT_POLL_INTERVAL_MS;
                        }
                    }
                });
                if result.is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    /// Start slower maintenance polling for non-output UI work.
    fn start_maintenance_polling(cx: &mut Context<Self>) {
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(Self::MAINTENANCE_POLL_INTERVAL_MS))
                .await;
            let result = this.update(cx, |this, cx| {
                this.poll_maintenance(cx);
            });
            if result.is_err() {
                break;
            }
        })
        .detach();
    }

    /// Start detector maintenance polling off the UI thread.
    fn start_detector_maintenance_polling(cx: &mut Context<Self>) {
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(Self::MAINTENANCE_POLL_INTERVAL_MS))
                .await;
            let result = this.update(cx, |this, cx| {
                this.spawn_background_detector_maintenance(cx);
            });
            if result.is_err() {
                break;
            }
        })
        .detach();
    }

    /// Blink modal text cursors at a steady cadence.
    fn start_modal_cursor_blink(cx: &mut Context<Self>) {
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(500))
                .await;
            let result = this.update(cx, |this, cx| {
                this.modals.cursor_blink_on = !this.modals.cursor_blink_on;
                if this.modals.task_creation.is_some() || this.modals.session_action.is_some() {
                    cx.notify();
                }
            });
            if result.is_err() {
                break;
            }
        })
        .detach();
    }

    /// Pre-detect installed editors and available shells on a background thread.
    ///
    /// These detections spawn multiple subprocesses (`where`, `pwsh.exe`, etc.)
    /// and would block the UI thread for hundreds of milliseconds if done synchronously.
    /// By running them here at startup, the results are cached and ready by the time
    /// the user opens settings.
    fn start_detection_background(cx: &mut Context<Self>) {
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let (editors, shells) = cx
                .background_executor()
                .spawn(async {
                    let editors = super::editor_detection::detect_installed_editors();
                    let shells = codirigent_session::detect_available_shells();
                    (editors, shells)
                })
                .await;
            let _ = this.update(cx, |this, _cx| {
                this.cache.detected_editors = Some(editors);
                this.cache.detected_shells = Some(shells);
            });
        })
        .detach();
    }

    /// Initialize task manager with file storage in platform-appropriate data directory.
    fn init_task_manager(
        event_bus: Arc<DefaultEventBus>,
    ) -> (
        Arc<dyn codirigent_core::StorageService>,
        Arc<Mutex<TaskManager>>,
    ) {
        let data_dir = dirs::data_dir()
            .map(|d| d.join("Codirigent"))
            .unwrap_or_else(|| std::env::temp_dir().join("codirigent-fallback"));
        let storage = Arc::new(FileStorageService::new(&data_dir).unwrap_or_else(|e| {
            warn!(
                "Failed to create file storage at {}: {}, using temp fallback",
                data_dir.display(),
                e
            );
            let temp_dir = std::env::temp_dir().join("codirigent-fallback");
            FileStorageService::new(&temp_dir).expect("Failed to create fallback storage")
        })) as Arc<dyn codirigent_core::StorageService>;

        let task_manager = Arc::new(Mutex::new(TaskManager::new(
            TaskManagerConfig::default(),
            storage.clone(),
            event_bus as Arc<dyn codirigent_core::EventBus>,
        )));

        (storage, task_manager)
    }

    /// Initialize file tree panel from the current working directory.
    fn init_file_tree() -> (FileTreePanel, Option<FileTree>, Option<PathBuf>) {
        let mut file_tree = FileTreePanel::new();
        let mut project_root = None;
        if let Ok(cwd) = std::env::current_dir() {
            file_tree.set_root(cwd.clone());
            project_root = Some(cwd.clone());
        }
        (file_tree, None, project_root)
    }

    /// Check if a session should be created at the given position.
    /// Returns true if this is not a duplicate click (same position within 100ms).
    pub(super) fn should_create_session_at(&mut self, position: GridPosition) -> bool {
        let now = Instant::now();

        // Check if this is a duplicate click
        if let Some((last_pos, last_time)) = self.selection.last_click_position {
            if last_pos == position && now.duration_since(last_time) < Duration::from_millis(100) {
                info!(?position, "Ignoring duplicate click within 100ms");
                return false;
            }
        }

        // Update last click position
        self.selection.last_click_position = Some((position, now));
        true
    }

    fn persisted_layout_mode(&self) -> codirigent_core::LayoutMode {
        match self.workspace.layout_state() {
            crate::layout::WorkspaceLayoutState::Grid(state) => state.profile().to_mode(),
            crate::layout::WorkspaceLayoutState::SplitTree(state) => {
                codirigent_core::LayoutMode::SplitTree {
                    root: state.tree().clone(),
                }
            }
        }
    }

    /// Mark layout-derived render caches as dirty after structural changes.
    pub(super) fn mark_layout_cache_dirty(&mut self) {
        self.cache.render_cell_info_dirty = true;
        self.cache.layout_generation = self.cache.layout_generation.saturating_add(1);
        self.cache.last_resize_signature = None;
        self.cache.pending_resize_signature = None;
    }

    fn current_resize_signature(
        &self,
        cell_width: f32,
        cell_height: f32,
    ) -> Option<TerminalResizeSignature> {
        Some(TerminalResizeSignature {
            layout_generation: self.cache.layout_generation,
            layout: self.cache.render_layout_signature?,
            cell_width,
            cell_height,
        })
    }

    fn render_focus_signature(&self) -> Option<SessionId> {
        Self::render_focus_signature_for_layout(
            self.workspace.layout_profile(),
            self.workspace.focused_session_id(),
        )
    }

    fn render_focus_signature_for_layout(
        layout_profile: crate::layout::LayoutProfile,
        focused_session_id: Option<SessionId>,
    ) -> Option<SessionId> {
        // Only single-pane mode swaps which session is visibly rendered when focus
        // changes. Multi-pane layouts already render every visible session, so
        // focus changes alone should not invalidate the cell-layout cache.
        if layout_profile == crate::layout::LayoutProfile::Single {
            focused_session_id
        } else {
            None
        }
    }

    /// Debounce persisted session/layout state writes off the UI thread.
    pub(super) fn save_state_to_disk(&mut self, cx: &mut Context<Self>) {
        self.polling.state_save_generation = self.polling.state_save_generation.saturating_add(1);
        let save_generation = self.polling.state_save_generation;
        self.polling.state_save_task = None;
        self.polling.state_save_task =
            Some(cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
                cx.background_executor()
                    .timer(Self::STATE_SAVE_DEBOUNCE)
                    .await;

                let save_inputs = match this.update(cx, |this, _cx| {
                    if this.polling.state_save_generation != save_generation {
                        return None;
                    }

                    Some((
                        this.persistence.storage.clone(),
                        this.session_manager.clone(),
                        this.persisted_layout_mode(),
                        this.workspace.pane_tab_groups(),
                        this.workspace.pane_stacks(),
                        this.cache.last_window_state.clone(),
                    ))
                }) {
                    Ok(Some(inputs)) => inputs,
                    Ok(None) | Err(_) => return,
                };

                let (storage, session_manager, layout, pane_tab_groups, pane_stacks, window_state) =
                    save_inputs;
                let result = cx
                    .background_executor()
                    .spawn(async move {
                        let sessions = session_manager
                            .lock()
                            .map(|manager| manager.list_sessions())
                            .unwrap_or_default();
                        let state = codirigent_core::AppState {
                            sessions,
                            layout,
                            pane_tab_groups,
                            pane_stacks,
                            updated_at: Some(chrono::Utc::now()),
                            window_bounds: window_state,
                        };
                        storage.save_state(&state)
                    })
                    .await;

                let _ = this.update(cx, |this, _cx| {
                    if this.polling.state_save_generation == save_generation {
                        this.polling.state_save_task = None;
                    }
                    if let Err(e) = result {
                        warn!("Failed to save state: {}", e);
                    }
                });
            }));
    }

    /// Cycle to next layout.
    pub fn next_layout(&mut self, cx: &mut Context<Self>) {
        self.workspace.next_layout();
        self.mark_layout_cache_dirty();
        self.sync_layout_derived_state();
        self.event_bus.publish(CodirigentEvent::LayoutChanged {
            mode: self.workspace.layout_profile().to_mode(),
        });
        cx.notify();
    }

    /// Toggle sidebar visibility.
    pub fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.workspace.toggle_sidebar();
        self.mark_layout_cache_dirty();
        self.sync_layout_derived_state();
        cx.notify();
    }

    /// Focus a session by number (1-9).
    pub fn focus_session_number(&mut self, number: usize, cx: &mut Context<Self>) {
        if self.workspace.focus_session_number(number) {
            if let Some(id) = self.workspace.focused_session_id() {
                self.event_bus
                    .publish(CodirigentEvent::SessionFocused { id });
            }
            self.sync_layout_derived_state();
            self.sync_file_tree_to_focused_session(cx);
            cx.notify();
        }
    }

    fn task_title_for_session(
        &self,
        session: &codirigent_core::Session,
        task_titles: Option<&HashMap<codirigent_core::TaskId, String>>,
    ) -> Option<String> {
        let task_id = session.current_task.as_ref()?;
        if let Some(task_titles) = task_titles {
            return Some(resolved_task_title(task_id, Some(task_titles)));
        }

        if let Ok(manager) = self.task_manager.lock() {
            return Some(
                manager
                    .get_task(task_id)
                    .map(|task| task.title.clone())
                    .unwrap_or_else(|| task_id.0.to_string()),
            );
        }

        Some(task_id.0.to_string())
    }

    fn sync_task_board_state(&mut self) -> HashMap<codirigent_core::TaskId, String> {
        let Ok(manager) = self.task_manager.lock() else {
            return HashMap::new();
        };

        let mut titles = HashMap::new();
        let all_tasks = manager.list_tasks();
        let counts =
            all_tasks
                .iter()
                .fold((0usize, 0usize, 0usize, 0usize), |(q, ip, r, d), task| {
                    titles.insert(task.id.clone(), task.title.clone());
                    match task.status {
                        codirigent_core::TaskStatus::Queued
                        | codirigent_core::TaskStatus::Blocked => (q + 1, ip, r, d),
                        codirigent_core::TaskStatus::Assigned
                        | codirigent_core::TaskStatus::Working => (q, ip + 1, r, d),
                        codirigent_core::TaskStatus::Verifying
                        | codirigent_core::TaskStatus::Review => (q, ip, r + 1, d),
                        codirigent_core::TaskStatus::Done => (q, ip, r, d + 1),
                    }
                });
        let running_items = all_tasks
            .iter()
            .filter(|t| {
                matches!(
                    t.status,
                    codirigent_core::TaskStatus::Assigned | codirigent_core::TaskStatus::Working
                )
            })
            .map(|t| self.core_task_to_ui_item(t))
            .collect();
        let queued_items = all_tasks
            .iter()
            .filter(|t| {
                matches!(
                    t.status,
                    codirigent_core::TaskStatus::Queued | codirigent_core::TaskStatus::Blocked
                )
            })
            .map(|t| self.core_task_to_ui_item(t))
            .collect();
        let review_items = all_tasks
            .iter()
            .filter(|t| {
                matches!(
                    t.status,
                    codirigent_core::TaskStatus::Verifying | codirigent_core::TaskStatus::Review
                )
            })
            .map(|t| self.core_task_to_ui_item(t))
            .collect();
        let done_items = all_tasks
            .iter()
            .filter(|t| t.status == codirigent_core::TaskStatus::Done)
            .map(|t| self.core_task_to_ui_item(t))
            .collect();
        let config = manager.assignment().config();
        let auto_assign_mode = crate::task_board::AutoAssignMode::from_config(
            config.auto_assign,
            config.confirm_before_assign,
        );
        let pending_assignments = manager
            .assignment()
            .pending_assignments()
            .iter()
            .map(|p| crate::task_board::PendingAssignmentSummary {
                task_id: p.task_id.to_string(),
                session_number: p.session_id.0,
                task_title: all_tasks
                    .iter()
                    .find(|t| t.id == p.task_id)
                    .map(|t| t.title.clone())
                    .unwrap_or_else(|| p.task_id.to_string()),
            })
            .collect();

        self.task_board
            .set_task_counts(counts.0, counts.1, counts.2, counts.3);
        self.task_board
            .set_snapshot(crate::task_board::TaskBoardSnapshot {
                running_items,
                queued_items,
                review_items,
                done_items,
                auto_assign_mode,
                pending_assignments,
            });

        titles
    }

    fn sync_all_session_headers(
        &mut self,
        task_titles: Option<&HashMap<codirigent_core::TaskId, String>>,
    ) {
        let sessions = self.workspace.sessions();
        let focused_id = self.workspace.focused_session_id();
        for session in sessions {
            let project_name = session_project_name(session);
            let git_branch = session.git_info.as_ref().map(|gi| gi.branch.clone());
            let git_dirty_count = session.git_info.as_ref().map(|gi| gi.dirty_count);
            let session_color = session
                .color
                .as_deref()
                .map(crate::sidebar::Color::from_hex)
                .unwrap_or_else(|| crate::sidebar::Color::from_hex("#6366f1"));
            let task = self.task_title_for_session(session, task_titles);
            if let Some(header) = self.terminal_headers.get_mut(&session.id) {
                if header.session_name != session.name {
                    header.session_name = session.name.clone();
                }
                if header.group_name != session.group {
                    header.group_name = session.group.clone();
                }
                header.status = session.status;
                header.context_usage = session.context_usage;
                header.is_focused = focused_id == Some(session.id);
                if header.project_name != project_name {
                    header.project_name = project_name;
                }
                if header.git_branch != git_branch {
                    header.git_branch = git_branch;
                }
                if header.git_dirty_count != git_dirty_count {
                    header.git_dirty_count = git_dirty_count;
                }
                if header.session_color != session_color {
                    header.session_color = session_color;
                }
                if header.task != task {
                    header.task = task;
                }
            }
        }
    }

    fn sync_empty_cells_state(&mut self) {
        let (rows, cols) = self.workspace.layout_profile().dimensions();
        let occupied: Vec<GridPosition> = self
            .workspace
            .layout_state()
            .as_grid()
            .map(|state| {
                state
                    .assignments()
                    .iter()
                    .enumerate()
                    .filter_map(|(index, session_id)| {
                        session_id.map(|_| GridPosition {
                            row: index as u32 / cols,
                            col: index as u32 % cols,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        self.empty_cells.setup_for_grid(rows, cols, &occupied);
    }

    pub(super) fn sync_layout_derived_state(&mut self) {
        self.sync_all_session_headers(None);
        self.sync_empty_cells_state();
    }

    pub(super) fn sync_task_derived_state(&mut self) {
        let task_titles = self.sync_task_board_state();
        self.sync_all_session_headers(Some(&task_titles));
    }

    /// Synchronize all derived UI state from canonical workspace/task state.
    ///
    /// This must only run from explicit mutation paths, never as a render fallback.
    pub(super) fn refresh_derived_ui_state(&mut self) {
        let task_titles = self.sync_task_board_state();
        self.sync_all_session_headers(Some(&task_titles));
        self.sync_empty_cells_state();
    }

    /// Sync a single session's terminal header from workspace state.
    ///
    /// This is a targeted delta update for the common case where only one
    /// session's status changed. Avoids the O(all sessions) cost of
    /// `refresh_derived_ui_state()` for each output poll.
    pub(super) fn sync_session_header(&mut self, session_id: SessionId) {
        let Some(session) = self.workspace.session(session_id) else {
            return;
        };
        let focused_id = self.workspace.focused_session_id();
        let project_name = session_project_name(session);
        let git_branch = session.git_info.as_ref().map(|gi| gi.branch.clone());
        let git_dirty_count = session.git_info.as_ref().map(|gi| gi.dirty_count);
        let session_color = session
            .color
            .as_deref()
            .map(crate::sidebar::Color::from_hex)
            .unwrap_or_else(|| crate::sidebar::Color::from_hex("#6366f1"));
        let task = self.task_title_for_session(session, None);

        if let Some(header) = self.terminal_headers.get_mut(&session_id) {
            header.status = session.status;
            header.context_usage = session.context_usage;
            header.is_focused = focused_id == Some(session_id);

            if header.session_name != session.name {
                header.session_name = session.name.clone();
            }
            if header.group_name != session.group {
                header.group_name = session.group.clone();
            }

            if header.project_name != project_name {
                header.project_name = project_name;
            }

            if header.git_branch != git_branch {
                header.git_branch = git_branch;
            }
            if header.git_dirty_count != git_dirty_count {
                header.git_dirty_count = git_dirty_count;
            }
            if header.session_color != session_color {
                header.session_color = session_color;
            }
            if header.task != task {
                header.task = task;
            }
        }
    }

    /// Get a terminal header for a session.
    pub fn get_terminal_header(&self, id: SessionId) -> Option<&TerminalHeader> {
        self.terminal_headers.get(&id)
    }

    /// Process pending events from all UI components.
    ///
    /// This method is called at the start of each render cycle to handle
    /// any pending events from task board, empty session cells, etc.
    fn process_ui_events(&mut self, cx: &mut Context<Self>) {
        // Process task board events
        for event in self.task_board.take_events() {
            self.handle_task_board_event(event, cx);
        }

        // Process empty session events
        for event in self.empty_cells.take_events() {
            self.handle_empty_session_event(event, cx);
        }
    }

    /// Process pending top bar events and translate to workspace actions.
    pub(super) fn process_top_bar_events(&mut self) {
        let events = self.top_bar.drain_events();
        for event in events {
            match event {
                crate::top_bar::TopBarEvent::LayoutSelected(layout_mode) => {
                    match layout_mode {
                        codirigent_core::LayoutMode::Grid { rows, cols } => {
                            let profile = match (rows, cols) {
                                (2, 2) => crate::layout::LayoutProfile::Grid2x2,
                                (4, 1) => crate::layout::LayoutProfile::Stack1x4,
                                (2, 3) => crate::layout::LayoutProfile::Grid2x3,
                                (3, 3) => crate::layout::LayoutProfile::Grid3x3,
                                _ => crate::layout::LayoutProfile::Custom { rows, cols },
                            };
                            self.workspace.set_layout(profile);
                        }
                        codirigent_core::LayoutMode::Single => {
                            self.workspace
                                .set_layout(crate::layout::LayoutProfile::Single);
                        }
                        codirigent_core::LayoutMode::SplitTree { root } => {
                            self.workspace.set_split_tree(root);
                        }
                        codirigent_core::LayoutMode::Custom { .. } => {
                            // Custom positional layouts not used from tabs
                        }
                    }
                    self.mark_layout_cache_dirty();
                    self.sync_layout_derived_state();
                }
                crate::top_bar::TopBarEvent::RightPanelToggled => {
                    // Will be wired in plan 05 (right task board)
                }
                crate::top_bar::TopBarEvent::CustomLayoutRequested => {
                    if self.custom_picker.is_open {
                        self.custom_picker.close();
                    } else {
                        let current_tree = if self.workspace.is_split_tree_mode() {
                            self.workspace
                                .layout_state()
                                .as_split_tree()
                                .map(|s| s.tree().clone())
                        } else {
                            None
                        };
                        let (rows, cols) = self.workspace.layout_profile().dimensions();
                        self.custom_picker.open_with_state(current_tree, rows, cols);
                    }
                }
                crate::top_bar::TopBarEvent::NewSessionRequested => {
                    // Future: delegate to create_session logic
                }
                crate::top_bar::TopBarEvent::BroadcastToggled(_) => {
                    // Broadcast feature removed
                }
            }
        }
    }

    /// Select a session (updates drawer context and grid focus).
    pub(super) fn select_session_with_cx(&mut self, session_id: SessionId, cx: &mut Context<Self>) {
        self.selection.selected_session_id = Some(session_id);
        self.drawer.set_selected_session(Some(session_id));
        self.workspace.focus_session(session_id);
        self.sync_layout_derived_state();
        self.sync_file_tree_to_focused_session(cx);
        // If the session showed ResponseReady, downgrade the cache to Idle
        // immediately so the badge clears without waiting for the next poll.
        if let Ok(mut readers) = self.cli_readers.lock() {
            if let Some(cached) = readers.cached_status.get_mut(&session_id) {
                if cached.status == codirigent_core::SessionStatus::ResponseReady {
                    cached.status = codirigent_core::SessionStatus::Idle;
                    cached.status_since = std::time::Instant::now();
                }
            }
        }
    }

    /// Process icon rail events (drawer toggling, settings).
    pub(super) fn process_icon_rail_events(&mut self) {
        let events = self.icon_rail.drain_events();
        for event in events {
            match event {
                crate::icon_rail::IconRailEvent::DrawerToggled(panel) => {
                    self.drawer.set_active_panel(panel);
                }
                crate::icon_rail::IconRailEvent::SettingsRequested => {
                    self.open_settings();
                }
            }
        }
    }

    /// Handle empty session cell events.
    fn handle_empty_session_event(&mut self, event: EmptySessionEvent, cx: &mut Context<Self>) {
        match event {
            EmptySessionEvent::CreateSessionClicked { position } => {
                info!(?position, "Create session at position");
                if self.should_create_session_at(position) {
                    let cols = self.workspace.layout_profile().dimensions().1;
                    let index = (position.row * cols + position.col) as usize;
                    self.create_session_in_pane(codirigent_core::PaneId::GridCell { index }, cx);
                }
            }
        }
        cx.notify();
    }

    /// Get a reference to the underlying workspace.
    ///
    /// Used by the render module to access workspace state.
    pub(super) fn workspace(&self) -> &Workspace {
        &self.workspace
    }

    /// Execute a closure with a locked session manager reference.
    ///
    /// This helper method reduces boilerplate for the common pattern of locking
    /// the session manager and unwrapping (since session manager lock should never fail).
    ///
    /// # Returns
    /// The result of executing the closure with the locked session manager.
    ///
    /// # Panics
    /// Panics if the session manager lock is poisoned (should never happen in normal operation).
    ///
    /// # Example
    /// ```ignore
    /// self.with_session_manager(|manager| {
    ///     manager.get_session(&session_id)
    /// })
    /// ```
    pub(super) fn with_session_manager<R>(
        &self,
        f: impl FnOnce(&mut DefaultSessionManager) -> R,
    ) -> R {
        let mut manager = self
            .session_manager
            .lock()
            .expect("session manager mutex poisoned");
        f(&mut manager)
    }

    /// Helper to acquire detector lock.
    ///
    /// # Panics
    /// Panics if the detector lock is poisoned (should never happen in normal operation).
    pub(super) fn with_detector<R>(&self, f: impl FnOnce(&mut InputDetector) -> R) -> R {
        let mut detector = self.detector.lock().expect("detector mutex poisoned");
        f(&mut detector)
    }

    /// Apply UI font size update to theme.
    ///
    /// Updates the theme's base, small, and large font sizes based on the given size.
    /// This helper eliminates duplication in the settings panel increment/decrement closures.
    ///
    /// # Parameters
    /// - `size`: The new base font size (10-24)
    pub(super) fn apply_ui_font_size(&mut self, size: f32) {
        let theme = self.workspace.theme_mut();
        theme.font_size_base = size;
        theme.font_size_small = (size - 2.0).max(8.0);
        theme.font_size_large = size + 2.0;
    }

    /// Apply terminal font size update to theme and all terminal views.
    ///
    /// Updates the theme's terminal font size and propagates the change to all
    /// active terminal views, including cell dimension recalculation.
    /// This helper eliminates duplication in the settings panel increment/decrement closures.
    ///
    /// # Parameters
    /// - `window`: GPUI window for accessing text system
    /// - `size`: The new terminal font size (8-24)
    pub(super) fn apply_terminal_font_size(&mut self, window: &mut Window, size: f32) {
        self.workspace.theme_mut().terminal_font_size = size;
        let family = self.workspace.theme().terminal_font_family.clone();
        let line_height = self.workspace.theme().terminal_line_height;
        let (w, h) = crate::terminal_view::compute_cell_dimensions(
            window.text_system(),
            &family,
            size,
            line_height,
        );
        for tv in self.terminals_mut().values_mut() {
            tv.set_font_size(size);
            tv.set_cell_dimensions(w, h);
        }
    }

    /// Apply a new terminal line height and propagate to all terminal views.
    pub(super) fn apply_terminal_line_height(&mut self, window: &mut Window, line_height: f32) {
        self.workspace.theme_mut().terminal_line_height = line_height;
        let family = self.workspace.theme().terminal_font_family.clone();
        let size = self.workspace.theme().terminal_font_size;
        let (w, h) = crate::terminal_view::compute_cell_dimensions(
            window.text_system(),
            &family,
            size,
            line_height,
        );
        for tv in self.terminals_mut().values_mut() {
            tv.set_cell_dimensions(w, h);
        }
    }

    /// Apply a new terminal font family and propagate to all terminal views.
    ///
    /// Updates the theme's terminal font family and propagates the change to all
    /// active terminal views, including cell dimension recalculation.
    pub(super) fn apply_terminal_font_family(&mut self, window: &mut Window, family: String) {
        self.workspace.theme_mut().terminal_font_family = family.clone();
        let size = self.workspace.theme().terminal_font_size;
        let line_height = self.workspace.theme().terminal_line_height;
        let (w, h) = crate::terminal_view::compute_cell_dimensions(
            window.text_system(),
            &family,
            size,
            line_height,
        );
        for tv in self.terminals_mut().values_mut() {
            tv.set_font_family(family.clone());
            tv.set_cell_dimensions(w, h);
        }
    }

    /// Get grid layout accounting for task board height.
    pub(super) fn grid_layout_with_task_board(&self) -> crate::layout::GridLayout {
        self.workspace.grid_layout()
    }

    /// Get a mutable reference to the terminals HashMap.
    ///
    /// Used by the render module for canvas-based rendering with content caching.
    pub(super) fn terminals_mut(&mut self) -> &mut HashMap<SessionId, TerminalView> {
        &mut self.terminals
    }

    /// Resize all terminals to fit their current grid cell bounds.
    ///
    /// This should be called when the window is resized or the layout changes,
    /// to ensure terminals have the correct character dimensions for their pixel bounds.
    /// Returns `true` if any terminal was actually resized.
    fn resize_terminals_to_grid(&mut self) -> bool {
        // Layout constants from types.rs: HEADER_HEIGHT, TERMINAL_CONTENT_PADDING, CELL_BORDER_WIDTH
        let mut resized_any = false;

        for info in &self.cache.render_cell_info {
            if let Some(terminal_view) = self.terminals.get_mut(&info.session_id) {
                // Subtract all chrome between the grid cell bounds and the
                // actual terminal canvas drawing area:
                //   - border: .border_1() on session cell (1px each side)
                //   - padding: canvas prepaint offsets by TERMINAL_CONTENT_PADDING
                //   - header: 32px header bar above terminal content
                let padding2 = TERMINAL_CONTENT_PADDING * 2.0;
                let available_width =
                    (info.bounds.size.width - CELL_BORDER_WIDTH - padding2).max(0.0);
                let available_height =
                    (info.bounds.size.height - CELL_BORDER_WIDTH - HEADER_HEIGHT - padding2)
                        .max(0.0);

                // Convert first so we can guard against transient layout collapses.
                // During some intermediate layout passes, bounds briefly report near-zero
                // sizes, which would otherwise force the PTY to 1 column/row and make
                // output wrap vertically until the next resize event.
                let (target_rows, target_cols) =
                    terminal_view.dimensions_from_pixels(available_width, available_height);
                let current_rows = terminal_view.rows();
                let current_cols = terminal_view.cols();

                if Self::should_skip_collapsed_resize(
                    current_rows,
                    current_cols,
                    target_rows,
                    target_cols,
                ) {
                    continue;
                }

                // Resize terminal emulator to fit the remaining space
                let did_resize = terminal_view.resize_to_fit(available_width, available_height);

                if did_resize {
                    resized_any = true;

                    // Propagate resize to actual PTY (ConPTY) so the shell
                    // knows the correct terminal dimensions
                    let rows = terminal_view.rows();
                    let cols = terminal_view.cols();
                    let last = self.cache.pty_sizes.get(&info.session_id);
                    if last != Some(&(rows, cols)) {
                        self.with_session_manager(|manager| {
                            if let Err(e) = manager.resize(info.session_id, rows, cols) {
                                warn!(
                                    "Failed to resize PTY for session {}: {}",
                                    info.session_id, e
                                );
                            }
                        });
                        self.cache.pty_sizes.insert(info.session_id, (rows, cols));
                    }
                }
            }
        }
        resized_any
    }

    /// Handle keyboard input for the focused session.
    fn handle_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Suppress unused-variable warning on macOS where the cfg-gated
        // Ctrl-shortcut block (which uses `window`) is compiled out.
        #[cfg(target_os = "macos")]
        let _ = &window;

        // Escape closes settings page if open
        if self.settings.open && event.keystroke.key == "escape" {
            self.close_settings(cx);
            cx.notify();
            return;
        }

        // Allow modals to capture input before sending to the terminal.
        if self.handle_modal_key_down(event, cx) {
            cx.stop_propagation();
            return;
        }

        // Don't send platform-modifier shortcuts to PTY (handled as GPUI actions).
        // On macOS, `platform` maps to Command key and GPUI's `cmd-v` bindings work
        // natively. On Windows/Linux, `platform` is false for Ctrl+key, so we handle
        // Ctrl shortcuts directly below.
        if event.keystroke.modifiers.platform {
            return;
        }

        // On Windows/Linux, GPUI's `cmd-<key>` keybindings expect `modifiers.platform`,
        // but Ctrl+key only sets `modifiers.control`. The action system never matches,
        // so we must handle Ctrl shortcuts directly here.
        #[cfg(not(target_os = "macos"))]
        if event.keystroke.modifiers.control {
            let key = event.keystroke.key.as_ref();
            match key {
                "v" => {
                    self.handle_paste(&crate::app::Paste, window, cx);
                    return;
                }
                "c" => {
                    self.handle_copy(&crate::app::Copy, window, cx);
                    return;
                }
                "n" => {
                    self.create_session(cx);
                    return;
                }
                "w" => {
                    self.close_focused_session(cx);
                    return;
                }
                "q" => {
                    cx.quit();
                    return;
                }
                "b" => {
                    self.toggle_task_board(cx);
                    return;
                }
                "e" => {
                    self.toggle_sidebar(cx);
                    return;
                }
                "k" => {
                    self.toggle_sidebar(cx);
                    return;
                }
                "p" if event.keystroke.modifiers.shift => {
                    self.open_task_creation_modal();
                    cx.notify();
                    return;
                }
                "\\" => {
                    self.next_layout(cx);
                    return;
                }
                "," => {
                    self.open_settings();
                    cx.notify();
                    return;
                }
                "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" => {
                    let num: usize = match key.parse::<usize>() {
                        Ok(n) => n,
                        Err(_) => return, // unreachable given match arm, but safe
                    };
                    self.focus_session_number(num, cx);
                    return;
                }
                _ => {} // Other Ctrl combos go to PTY (Ctrl+D, Ctrl+L, etc.)
            }
        }

        // Text input (including IME commits) is delivered through the
        // EntityInputHandler path via replace_text_in_range(). If we also
        // send printable keys from keydown, characters are duplicated.
        // GPUI can report plain Space with an empty key_char, so treat it
        // as text input as well to avoid inserting two spaces.
        if Self::keystroke_is_text_input(event) {
            return;
        }

        let key: &str = event.keystroke.key.as_ref();

        // Get focused session
        let Some(session_id) = self.workspace.focused_session_id() else {
            return;
        };

        self.capture_shell_key_input(session_id, key, cx);

        // Get terminal mode for proper escape sequence generation (immutable borrow)
        let term_mode = {
            let Some(terminal_view) = self.terminals.get(&session_id) else {
                return;
            };
            terminal_view.mode()
        };

        // Convert GPUI keystroke to terminal keystroke
        let modifiers = TerminalModifiers {
            shift: event.keystroke.modifiers.shift,
            control: event.keystroke.modifiers.control,
            alt: event.keystroke.modifiers.alt,
        };

        let mut keystroke =
            TerminalKeystroke::with_modifiers(event.keystroke.key.clone(), modifiers);

        // Use key_char for IME-composed characters (non-ASCII input like CJK, accented chars)
        if let Some(ref key_char) = event.keystroke.key_char {
            if !key_char.is_ascii() {
                keystroke.ime_key = Some(key_char.clone());
            }
        }

        // Convert to bytes
        if let Some(bytes) = key_to_bytes(&keystroke, term_mode) {
            let mut scrolled_to_bottom = false;
            // Auto-scroll to bottom when user types while scrolled up in scrollback.
            // This is standard terminal behavior: typing should return the view
            // to the cursor position.
            if let Some(terminal_view) = self.terminals.get_mut(&session_id) {
                scrolled_to_bottom = terminal_view.scroll_to_bottom_if_needed();
            }

            // Send to PTY
            self.with_session_manager(|manager| {
                if let Err(e) = manager.send_input(session_id, &bytes) {
                    warn!("Failed to send input to session {}: {}", session_id, e);
                }
            });

            if scrolled_to_bottom {
                cx.notify();
            }
        }
    }

    /// Render the main workspace content: title bar, top bar, sidebars, and grid.
    ///
    /// Returns early with a settings overlay if settings are open.
    fn render_main_workspace(
        &mut self,
        mut container: gpui::Stateful<gpui::Div>,
        window: &mut Window,
        cx: &mut Context<Self>,
        grid_gap: f32,
    ) -> gpui::Stateful<gpui::Div> {
        container = container.child(self.render_title_bar(window, cx));

        // Settings page overlay (replaces all content below title bar)
        if self.settings.open && self.settings.page.is_some() {
            // Delayed save: schedule a one-shot background flush instead of
            // blocking the render thread with synchronous file I/O.
            // Only one task at a time — if a task is already in flight it will
            // flush whatever is dirty when it fires (including later edits).
            self.schedule_settings_save(std::time::Duration::from_millis(500), cx);
            container = container.child(self.render_settings_overlay(cx));
            return container;
        }

        container = container.child(self.render_top_bar(cx));

        // Main content area (flex-row: icon rail + drawer + grid + right task board)
        let mut main_content = div()
            .id("main-content")
            .flex_1()
            .flex()
            .flex_row()
            .overflow_hidden()
            .min_h(px(0.0));

        main_content = main_content.child(self.render_icon_rail(cx));

        if self.drawer.is_open() {
            main_content = main_content.child(self.render_drawer(cx));
        }

        let grid_area = div()
            .id("grid-area")
            .flex_1()
            .flex()
            .flex_col()
            .overflow_hidden()
            .min_h(px(0.0))
            .child(
                div()
                    .id("session-grid-container")
                    .flex_1()
                    .flex()
                    .flex_col()
                    .p(px(grid_gap))
                    .overflow_hidden()
                    .min_h(px(0.0))
                    .child(self.render_grid_with_headers(window, cx)),
            );

        main_content = main_content.child(grid_area);

        if self.top_bar.is_right_panel_open() {
            main_content = main_content.child(self.render_right_task_board(cx));
        }

        container.child(main_content)
    }

    /// Render active modal dialogs (custom layout, session menu, action modal, task creation, context menu).
    fn render_active_modals(
        &mut self,
        mut container: gpui::Stateful<gpui::Div>,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        if let Some(modal) = self.render_custom_layout_modal(cx) {
            container = container.child(modal);
        }
        if let Some(menu) = self.render_session_menu(cx) {
            container = container.child(menu);
        }
        if let Some(modal) = self.render_session_action_modal(cx) {
            container = container.child(modal);
        }
        if let Some(modal) = self.render_task_creation_modal(cx) {
            container = container.child(modal);
        }
        if let Some(menu) = self.render_file_tree_context_menu(cx) {
            container = container.child(menu);
        }
        container
    }

    /// Render floating overlays (clipboard preview tooltip, profile deletion dialog).
    fn render_overlays(
        &mut self,
        mut container: gpui::Stateful<gpui::Div>,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        // Clipboard preview tooltip
        if self.clipboard.clipboard_preview.is_visible() {
            if let Some(preview) = self.clipboard.clipboard_preview.preview() {
                let theme = self.workspace.theme();
                let panel_bg: gpui::Hsla = theme.panel_background.into();
                let border_color: gpui::Hsla = theme.border.into();
                let fg: gpui::Hsla = theme.foreground.into();
                let muted: gpui::Hsla = theme.muted.into();

                let dims_text = ClipboardPreview::format_dimensions(
                    preview.original_width,
                    preview.original_height,
                );
                let size_text = preview.human_readable_size();
                let path_text = preview.image_path.display().to_string();

                container = container.child(
                    div()
                        .absolute()
                        .bottom(px(16.0))
                        .right(px(16.0))
                        .bg(panel_bg)
                        .border_1()
                        .border_color(border_color)
                        .rounded_md()
                        .p_2()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .max_w(px(200.0))
                        .child(div().text_xs().text_color(fg).child("Image in clipboard"))
                        .child(
                            div()
                                .text_xs()
                                .text_color(muted)
                                .child(format!("{} · {}", dims_text, size_text)),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(muted)
                                .truncate()
                                .child(path_text),
                        )
                        .child(div().text_xs().text_color(muted).child("Ctrl+V to paste")),
                );
            }
        }

        // Profile deletion confirmation dialog
        if let Some((tab_idx, profile_name)) = &self.modals.pending_profile_deletion {
            let theme = self.workspace.theme();
            let panel_bg: gpui::Hsla = theme.panel_background.into();
            let border_color: gpui::Hsla = theme.border.into();
            let fg: gpui::Hsla = theme.foreground.into();
            let muted: gpui::Hsla = theme.muted.into();
            let idx_for_confirm = *tab_idx;

            container = container.child(
                div()
                    .absolute()
                    .inset_0()
                    .bg(super::types::MODAL_BACKDROP)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .bg(panel_bg)
                            .border_1()
                            .border_color(border_color)
                            .rounded_lg()
                            .p_4()
                            .flex()
                            .flex_col()
                            .gap_3()
                            .w(px(380.0))
                            .child(
                                div()
                                    .text_base()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(fg)
                                    .child("Delete Layout Profile?"),
                            )
                            .child(
                                div().text_sm().text_color(muted).child(format!(
                                    "Are you sure you want to delete the layout profile '{}'? This action cannot be undone.",
                                    profile_name
                                )),
                            )
                            .child(
                                div()
                                    .flex()
                                    .gap_2()
                                    .justify_end()
                                    .child(
                                        div()
                                            .id("delete-profile-cancel")
                                            .px_4()
                                            .py_2()
                                            .rounded_md()
                                            .bg(gpui::Hsla::transparent_black())
                                            .border_1()
                                            .border_color(border_color)
                                            .text_sm()
                                            .text_color(fg)
                                            .cursor_pointer()
                                            .hover(|style| {
                                                style.bg(super::types::CANCEL_BUTTON_HOVER)
                                            })
                                            .on_click(cx.listener(
                                                move |this, _: &ClickEvent, _window, cx| {
                                                    this.modals.pending_profile_deletion = None;
                                                    cx.notify();
                                                },
                                            ))
                                            .child("Cancel"),
                                    )
                                    .child(
                                        div()
                                            .id("delete-profile-confirm")
                                            .px_4()
                                            .py_2()
                                            .rounded_md()
                                            .bg(super::types::DESTRUCTIVE_BUTTON_BG)
                                            .text_sm()
                                            .text_color(gpui::Hsla::white())
                                            .cursor_pointer()
                                            .hover(|style| {
                                                style.bg(super::types::DESTRUCTIVE_BUTTON_HOVER)
                                            })
                                            .on_click(cx.listener(
                                                move |this, _: &ClickEvent, _window, cx| {
                                                    this.modals.pending_profile_deletion = None;
                                                    this.top_bar.remove_tab(idx_for_confirm);
                                                    this.save_layout_profiles_to_settings(cx);
                                                    cx.notify();
                                                },
                                            ))
                                            .child("Delete"),
                                    ),
                            ),
                    ),
            );
        }

        container
    }

    fn handle_modal_key_down(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> bool {
        if self.handle_session_action_key_down(event, cx) {
            return true;
        }
        if self.handle_task_creation_key_down(event, cx) {
            return true;
        }
        if self.handle_custom_layout_key_down(event, cx) {
            return true;
        }
        false
    }

    /// Returns true when a modal/dialog should block terminal text input.
    pub(super) fn has_blocking_modal(&self) -> bool {
        self.custom_picker.is_open
            || self.modals.session_action.is_some()
            || self.modals.task_creation.is_some()
            || self.modals.pending_profile_deletion.is_some()
    }
}

impl Focusable for WorkspaceView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EntityInputHandler for WorkspaceView {
    fn text_for_range(
        &mut self,
        _range: std::ops::Range<usize>,
        adjusted_range: &mut Option<std::ops::Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        if let Some(text) = &self.ime_preedit_text {
            let len = text.encode_utf16().count();
            *adjusted_range = Some(0..len);
            Some(text.clone())
        } else {
            None
        }
    }

    fn selected_text_range(
        &mut self,
        _ignore_selection_if_not_focused: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        if let Some(range) = self.ime_marked_range.clone() {
            Some(UTF16Selection {
                range,
                reversed: false,
            })
        } else {
            Some(UTF16Selection {
                range: 0..0,
                reversed: false,
            })
        }
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<std::ops::Range<usize>> {
        self.ime_marked_range.clone()
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let had_ime_overlay = self.ime_marked_range.is_some() || self.ime_preedit_text.is_some();
        self.ime_marked_range = None;
        self.ime_preedit_text = None;
        if had_ime_overlay {
            cx.notify();
        }
    }

    fn replace_text_in_range(
        &mut self,
        _range: Option<std::ops::Range<usize>>,
        text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.has_blocking_modal() {
            // Modal text fields are handled via key events; do not leak input to PTY.
            return;
        }

        let had_ime_overlay = self.ime_marked_range.is_some() || self.ime_preedit_text.is_some();
        self.ime_marked_range = None;
        self.ime_preedit_text = None;
        let mut scrolled_to_bottom = false;
        if let Some(session_id) = self.workspace.focused_session_id() {
            // Typing while scrolled up should jump back to the cursor line,
            // matching native terminal behavior.
            if let Some(terminal_view) = self.terminals.get_mut(&session_id) {
                scrolled_to_bottom = terminal_view.scroll_to_bottom_if_needed();
            }

            self.capture_shell_text_input(session_id, text);
            let text_bytes = text.as_bytes().to_vec();

            self.with_session_manager(move |sm| {
                let _ = sm.send_input(session_id, &text_bytes);
            });
        }
        if had_ime_overlay || scrolled_to_bottom {
            cx.notify();
        }
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _range: Option<std::ops::Range<usize>>,
        text: &str,
        _mark_range: Option<std::ops::Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.has_blocking_modal() {
            let had_ime_overlay =
                self.ime_marked_range.is_some() || self.ime_preedit_text.is_some();
            self.ime_marked_range = None;
            self.ime_preedit_text = None;
            if had_ime_overlay {
                cx.notify();
            }
            return;
        }

        let previous_text = self.ime_preedit_text.clone();
        let previous_range = self.ime_marked_range.clone();
        let len = text.encode_utf16().count();
        if len == 0 {
            self.ime_marked_range = None;
            self.ime_preedit_text = None;
        } else {
            self.ime_marked_range = Some(0..len);
            self.ime_preedit_text = Some(text.to_string());
        }

        if self.ime_preedit_text != previous_text || self.ime_marked_range != previous_range {
            cx.notify();
        }
    }

    fn bounds_for_range(
        &mut self,
        _range: std::ops::Range<usize>,
        element_bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        Some(element_bounds)
    }

    fn character_index_for_point(
        &mut self,
        _point: gpui::Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        // TODO: implement proper character index for correct IME candidate window positioning
        Some(0)
    }
}

impl Render for WorkspaceView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Ensure Lucide icon font is loaded (no-op after first call)
        crate::icons::ensure_font_loaded(window);

        // Lazily detect monospace fonts on first render (text system is available here)
        if self.cache.monospace_fonts.is_none() {
            self.cache.monospace_fonts = Some(detect_monospace_fonts(window.text_system()));
        }

        let rem = REM_BASE * (self.workspace.theme().font_size_base / FONT_SIZE_BASE_DEFAULT);
        window.set_rem_size(gpui::px(rem));

        // Process any pending UI events first
        self.process_ui_events(cx);
        self.process_top_bar_events();
        self.process_icon_rail_events();

        // Update workspace bounds from window size
        // GPUI automatically re-renders when window resizes, so we update bounds here
        let window_size = window.viewport_size();
        let window_bounds =
            crate::layout::Bounds::from_size(window_size.width.into(), window_size.height.into());
        self.workspace.set_bounds(window_bounds);

        // Detect window move/resize and persist bounds (debounced)
        {
            let wb = window.bounds();
            let x = f32::from(wb.origin.x);
            let y = f32::from(wb.origin.y);
            let w = f32::from(wb.size.width);
            let h = f32::from(wb.size.height);
            let maximized = window.is_maximized();

            if w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0 {
                let current = (x, y, w, h, maximized);
                let prev = self
                    .cache
                    .last_window_state
                    .as_ref()
                    .map(|s| (s.x, s.y, s.width, s.height, s.is_maximized));
                let is_initial = prev.is_none();

                if prev != Some(current) {
                    self.cache.last_window_state = Some(codirigent_core::WindowState {
                        x,
                        y,
                        width: w,
                        height: h,
                        is_maximized: maximized,
                    });
                    if !is_initial {
                        self.save_state_to_disk(cx);
                    }
                }
            }
        }

        // Update sidebar width to match actual icon rail + drawer state
        // so grid_bounds() calculates correct cell dimensions
        let actual_sidebar_width = crate::icon_rail::IconRail::WIDTH
            + if self.drawer.is_open() {
                self.drawer.width()
            } else {
                0.0
            };
        self.workspace.set_sidebar_width(actual_sidebar_width);

        // Account for right panel width when open
        let right_panel_w = if self.top_bar.is_right_panel_open() {
            crate::layout::RIGHT_PANEL_WIDTH
        } else {
            0.0
        };
        self.workspace.set_right_panel_width(right_panel_w);
        let layout_signature = RenderLayoutSignature {
            bounds: window_bounds,
            sidebar_width: actual_sidebar_width,
            right_panel_width: right_panel_w,
            grid_gap: self.workspace.theme().grid_gap,
            focused_session_id: self.render_focus_signature(),
        };
        if self.cache.render_cell_info_dirty
            || self.cache.render_layout_signature != Some(layout_signature)
        {
            self.cache.render_cell_info = self.workspace.cell_info();
            self.cache.render_cell_info_dirty = false;
            self.cache.render_layout_signature = Some(layout_signature);
        }

        self.sync_terminal_dimensions_and_resize(window, cx);

        // Clone theme values before any mutable borrows
        let theme = self.workspace.theme();
        let bg: gpui::Hsla = theme.background.into();
        let grid_gap = theme.grid_gap;
        let ui_font_size = theme.font_size_base;

        // Build the main container with flex-col layout
        let mut container = div()
            .id("workspace-container")
            .size_full()
            .text_size(gpui::px(ui_font_size))
            .track_focus(&self.focus_handle(cx))
            // Register action handlers for keyboard shortcuts
            .on_action(cx.listener(Self::handle_new_session))
            .on_action(cx.listener(Self::handle_close_session))
            .on_action(cx.listener(Self::handle_next_layout))
            .on_action(cx.listener(Self::handle_toggle_sidebar))
            .on_action(cx.listener(Self::handle_focus_session1))
            .on_action(cx.listener(Self::handle_focus_session2))
            .on_action(cx.listener(Self::handle_focus_session3))
            .on_action(cx.listener(Self::handle_focus_session4))
            .on_action(cx.listener(Self::handle_focus_session5))
            .on_action(cx.listener(Self::handle_focus_session6))
            .on_action(cx.listener(Self::handle_focus_session7))
            .on_action(cx.listener(Self::handle_focus_session8))
            .on_action(cx.listener(Self::handle_focus_session9))
            .on_action(cx.listener(Self::handle_split_horizontal))
            .on_action(cx.listener(Self::handle_split_vertical))
            .on_action(cx.listener(Self::handle_close_pane))
            .on_action(cx.listener(Self::handle_paste))
            .on_action(cx.listener(Self::handle_copy))
            .on_action(cx.listener(Self::handle_open_settings))
            // Handle keyboard input for PTY
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                this.handle_key_down(event, window, cx);
            }))
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
                let Some(drag) = &mut this.selection.drag else {
                    return;
                };

                let pos =
                    crate::layout::Point::new(event.position.x.into(), event.position.y.into());
                drag.update_pointer(pos, &this.cache.render_cell_info);
                cx.notify();
            }))
            // Global mouse-up: catch drag releases anywhere in workspace
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _event: &MouseUpEvent, _window, cx| {
                    if let Some(drag) = this.selection.drag.take() {
                        if drag.active {
                            if let Some(target) = drag.target {
                                let changed = match target.kind {
                                    super::types::DragTargetKind::PaneBody => this
                                        .workspace
                                        .swap_sessions(drag.source_index, target.index),
                                    super::types::DragTargetKind::PaneHeader => this
                                        .cache
                                        .render_cell_info
                                        .iter()
                                        .find(|info| info.index == target.index)
                                        .cloned()
                                        .is_some_and(|info| {
                                            this.workspace.group_session_into_pane(
                                                drag.source_session_id,
                                                info.pane_id,
                                            )
                                        }),
                                };
                                if changed {
                                    this.mark_layout_cache_dirty();
                                    this.sync_layout_derived_state();
                                    this.save_state_to_disk(cx);
                                }
                            }
                        }
                        cx.notify();
                    }
                }),
            )
            .bg(bg)
            .flex()
            .flex_col();

        // Build the workspace: title bar ??main content ??modals ??overlays
        container = self.render_main_workspace(container, window, cx, grid_gap);
        container = self.render_active_modals(container, cx);
        container = self.render_overlays(container, cx);
        container
    }
}

impl WorkspaceView {
    /// Sync terminal cell dimensions with font metrics, then throttle-trigger PTY resize.
    ///
    /// Uses a cache keyed on font family + size so font queries only run when
    /// terminal appearance settings change, not on every frame.
    ///
    /// Resize is debounced to ≤10/sec to prevent PTY feedback loops during
    /// continuous window drag/resize.
    fn sync_terminal_dimensions_and_resize(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let font_family = &self.workspace.theme().terminal_font_family;
        let font_size = self.workspace.theme().terminal_font_size;
        let line_height = self.workspace.theme().terminal_line_height;
        let (real_w, real_h) = match &self.cache.cached_cell_dims {
            Some(cached)
                if cached.font_family == *font_family
                    && (cached.font_size - font_size).abs() < 0.01
                    && (cached.line_height - line_height).abs() < 0.001 =>
            {
                (cached.cell_width, cached.cell_height)
            }
            _ => {
                let (w, h) = crate::terminal_view::compute_cell_dimensions(
                    window.text_system(),
                    font_family,
                    font_size,
                    line_height,
                );
                self.cache.cached_cell_dims = Some(super::types::CachedCellDims {
                    font_family: font_family.clone(),
                    font_size,
                    line_height,
                    cell_width: w,
                    cell_height: h,
                });
                (w, h)
            }
        };
        for tv in self.terminals.values_mut() {
            if !tv.dimensions_initialized() {
                tv.set_cell_dimensions(real_w, real_h);
            }
        }

        let Some(resize_signature) = self.current_resize_signature(real_w, real_h) else {
            return;
        };
        if self.cache.last_resize_signature == Some(resize_signature) {
            return;
        }

        let now = Instant::now();
        if now.duration_since(self.polling.last_resize_time) > Duration::from_millis(100) {
            self.resize_terminals_to_grid();
            self.cache.last_resize_signature = Some(resize_signature);
            self.cache.pending_resize_signature = None;
            self.polling.last_resize_time = now;
            self.polling.pending_resize = false;
        } else {
            self.cache.pending_resize_signature = Some(resize_signature);
            if self.polling.pending_resize {
                return;
            }
            self.polling.pending_resize = true;
            cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                let _ = this.update(cx, |this, cx| {
                    let Some(signature) = this.cache.pending_resize_signature.take() else {
                        this.polling.pending_resize = false;
                        return;
                    };
                    let resized = this.resize_terminals_to_grid();
                    this.cache.last_resize_signature = Some(signature);
                    this.polling.last_resize_time = Instant::now();
                    this.polling.pending_resize = false;
                    if resized {
                        cx.notify();
                    }
                });
            })
            .detach();
        }
    }
}

/// Create a complete workspace view with all components wired up.
///
/// # Arguments
///
/// * `session_manager` - Session manager for PTY and session lifecycle
/// * `detector` - Input detector for monitoring session status
/// * `event_bus` - Event bus for cross-module communication
/// * `theme` - Theme configuration
/// * `cx` - App context (from window creation callback)
///
/// # Returns
///
/// A GPUI Entity containing the workspace.
pub fn create_workspace_view<C: AppContext>(
    session_manager: Arc<Mutex<DefaultSessionManager>>,
    detector: Arc<Mutex<InputDetector>>,
    event_bus: Arc<DefaultEventBus>,
    theme: CodirigentTheme,
    cx: &mut C,
) -> C::Result<Entity<WorkspaceView>> {
    cx.new(|cx| WorkspaceView::new(session_manager, detector, event_bus, theme, cx))
}

#[cfg(test)]
mod tests {
    //! GPUI View Testing Strategy
    //!
    //! # Why Limited Tests
    //!
    //! `WorkspaceView` is a GPUI view component that requires the GPUI runtime
    //! for rendering and interaction. Testing GPUI views requires:
    //! - GPUI test harness (`gpui::TestAppContext`)
    //! - Window creation for rendering tests
    //! - Focus simulation for interaction tests
    //!
    //! # Test Coverage Strategy
    //!
    //! 1. **Core Business Logic** - Fully tested in `workspace/tests.rs` (29 tests)
    //!    - Layout management, session handling, focus navigation
    //!    - Bounds calculation, cell info generation
    //!    - All non-GPUI logic has 100% test coverage
    //!
    //! 2. **GPUI Integration** - Deferred to integration tests
    //!    - Rendering correctness requires visual inspection or snapshot tests
    //!    - Action handlers require GPUI action dispatch simulation
    //!
    //! # Future: GPUI Test Infrastructure
    //!
    //! When GPUI test helpers are available, add tests for:
    //! - [ ] WorkspaceView renders without panic
    //! - [ ] Action handlers (NewSession, CloseSession, etc.) work correctly
    //! - [ ] Focus delegation to child components
    //! - [ ] Layout changes trigger re-render

    use std::collections::HashMap;

    #[test]
    fn test_core_workspace_is_tested_separately() {
        // Reminder: Core workspace logic has dedicated tests in workspace/tests.rs
        // Run `cargo test workspace::tests` to see all 29 tests pass
        use crate::workspace::Workspace;

        // Quick sanity check that we can create a workspace
        let ws = Workspace::new();
        assert!(ws.sessions().is_empty());
    }

    #[test]
    fn test_skip_collapsed_resize_when_current_is_usable() {
        assert!(super::WorkspaceView::should_skip_collapsed_resize(
            40, 120, 40, 1
        ));
        assert!(super::WorkspaceView::should_skip_collapsed_resize(
            40, 120, 1, 120
        ));
        assert!(super::WorkspaceView::should_skip_collapsed_resize(
            40, 120, 1, 1
        ));
    }

    #[test]
    fn test_do_not_skip_collapsed_resize_if_already_collapsed() {
        assert!(!super::WorkspaceView::should_skip_collapsed_resize(
            1, 1, 1, 1
        ));
        assert!(!super::WorkspaceView::should_skip_collapsed_resize(
            1, 80, 1, 1
        ));
    }

    #[test]
    fn test_do_not_skip_non_collapsed_resize() {
        assert!(!super::WorkspaceView::should_skip_collapsed_resize(
            40, 120, 30, 100
        ));
    }

    #[test]
    fn test_render_focus_signature_tracks_focus_in_single_layout() {
        assert_eq!(
            super::WorkspaceView::render_focus_signature_for_layout(
                crate::layout::LayoutProfile::Single,
                Some(codirigent_core::SessionId(2)),
            ),
            Some(codirigent_core::SessionId(2))
        );
    }

    #[test]
    fn test_render_focus_signature_ignores_focus_outside_single_layout() {
        assert_eq!(
            super::WorkspaceView::render_focus_signature_for_layout(
                crate::layout::LayoutProfile::Grid2x2,
                Some(codirigent_core::SessionId(2)),
            ),
            None
        );
    }

    #[test]
    fn test_normalize_codex_execution_mode_detects_bypass_alias() {
        assert_eq!(
            super::WorkspaceView::normalize_codex_execution_mode("codex --yolo"),
            Some(codirigent_core::CodexExecutionMode::Bypass)
        );
    }

    #[test]
    fn test_normalize_codex_execution_mode_detects_full_auto() {
        assert_eq!(
            super::WorkspaceView::normalize_codex_execution_mode("codex resume abc --full-auto"),
            Some(codirigent_core::CodexExecutionMode::FullAuto)
        );
    }

    #[test]
    fn test_normalize_codex_execution_mode_detects_explicit_never_and_danger() {
        assert_eq!(
            super::WorkspaceView::normalize_codex_execution_mode(
                "codex -a never -s danger-full-access"
            ),
            Some(codirigent_core::CodexExecutionMode::Bypass)
        );
    }

    #[test]
    fn test_session_project_name_prefers_git_repo_root_name() {
        let mut session = codirigent_core::Session::new(
            codirigent_core::SessionId(1),
            "Session 1".to_string(),
            std::path::PathBuf::from("/workspace/subdir"),
        );
        session.git_info = Some(codirigent_core::GitRepoInfo {
            repo_root: std::path::PathBuf::from("/workspace/project-root"),
            branch: "main".to_string(),
            dirty_count: 0,
            has_staged: false,
            head_sha: None,
            unstaged_files: Vec::new(),
            staged_files: Vec::new(),
        });

        assert_eq!(
            super::session_project_name(&session),
            Some("project-root".to_string())
        );
    }

    #[test]
    fn test_session_project_name_falls_back_to_working_directory_name() {
        let session = codirigent_core::Session::new(
            codirigent_core::SessionId(1),
            "Session 1".to_string(),
            std::path::PathBuf::from("/workspace/focused-pane"),
        );

        assert_eq!(
            super::session_project_name(&session),
            Some("focused-pane".to_string())
        );
    }

    #[test]
    fn test_resolved_task_title_prefers_cached_title_and_falls_back_to_id() {
        let task_id = codirigent_core::TaskId::from("task-123");
        let mut titles = HashMap::new();
        titles.insert(task_id.clone(), "Review parser".to_string());

        assert_eq!(
            super::resolved_task_title(&task_id, Some(&titles)),
            "Review parser".to_string()
        );
        assert_eq!(
            super::resolved_task_title(&codirigent_core::TaskId::from("task-456"), Some(&titles)),
            "task-456".to_string()
        );
        assert_eq!(
            super::resolved_task_title(&task_id, None),
            "task-123".to_string()
        );
    }

    #[test]
    fn test_keystroke_is_text_input_for_plain_printable_without_key_char() {
        let event = gpui::KeyDownEvent {
            keystroke: gpui::Keystroke {
                modifiers: gpui::Modifiers::default(),
                key: "a".to_string(),
                key_char: None,
            },
            is_held: false,
        };

        assert!(super::WorkspaceView::keystroke_is_text_input(&event));
    }

    #[test]
    fn test_keystroke_is_not_text_input_for_named_terminal_key() {
        let event = gpui::KeyDownEvent {
            keystroke: gpui::Keystroke {
                modifiers: gpui::Modifiers::default(),
                key: "enter".to_string(),
                key_char: None,
            },
            is_held: false,
        };

        assert!(!super::WorkspaceView::keystroke_is_text_input(&event));
    }
}
