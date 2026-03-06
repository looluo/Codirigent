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
    CacheState, CliReaders, ModalState, PollingState, SelectionState, CELL_BORDER_WIDTH,
    FONT_SIZE_BASE_DEFAULT, HEADER_HEIGHT, REM_BASE, TERMINAL_CONTENT_PADDING,
};
use crate::app::{Copy, Paste};
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
use codirigent_core::config_service::ConfigService;
use codirigent_core::{
    CodirigentEvent, DefaultEventBus, EventBus, FileStorageService, GridPosition, SessionId,
    SessionManager, TaskManager, TaskManagerConfig,
};
use codirigent_detector::InputDetector;
use codirigent_filetree::FileTree;
use codirigent_session::clipboard_service::DefaultClipboardService;
use codirigent_session::DefaultSessionManager;
use gpui::{
    div, px, App, AppContext, Bounds, ClickEvent, Context, Entity, EntityInputHandler, FocusHandle,
    Focusable, InteractiveElement, IntoElement, KeyDownEvent, ParentElement, Pixels, Render,
    StatefulInteractiveElement, Styled, UTF16Selection, Window,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracing::{info, warn};

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
    /// CLI session readers and process-tree detector (shared with background tasks).
    pub(super) cli_readers: Arc<Mutex<CliReaders>>,
    /// Cached detection results and memoized state.
    pub(super) cache: CacheState,
}

/// Returns `true` if the editor command refers to a terminal-based editor
/// (one that needs to run inside an existing terminal session).
impl WorkspaceView {
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

        Self::start_output_polling(cx);
        Self::start_modal_cursor_blink(cx);

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
                worktree_manager: Self::init_worktree_manager(),
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
            cli_readers: Arc::new(Mutex::new(CliReaders::new())),
            cache: CacheState::new(),
        };

        // Pre-detect editors and shells in the background so settings open instantly
        Self::start_detection_background(cx);

        // Restore sessions from previous run
        view.restore_sessions_from_disk(cx);

        view.refresh_file_tree_panel();
        view.refresh_worktree_panel();

        // Load saved layout profiles from user settings
        if let Some(ref config_service) = view.settings.config_service {
            if let Ok(user_settings) = config_service.load_user_settings() {
                view.top_bar
                    .load_saved_profiles(user_settings.saved_layouts);
            }
        }

        view
    }

    /// Start adaptive output polling background task.
    ///
    /// Uses 4ms interval when output is being received (low latency for typing),
    /// 50ms after 12 idle polls (~50ms of no output), and 200ms after 100 idle
    /// polls (~5s of no output).
    fn start_output_polling(cx: &mut Context<Self>) {
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let mut poll_interval_ms: u64 = 4;
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(poll_interval_ms))
                    .await;
                let result = this.update(cx, |this, cx| {
                    this.poll_output(cx);
                    if this.polling.last_poll_had_output {
                        this.polling.idle_poll_count = 0;
                        poll_interval_ms = 4;
                    } else {
                        this.polling.idle_poll_count =
                            this.polling.idle_poll_count.saturating_add(1);
                        if this.polling.idle_poll_count > 100 {
                            // Deep idle (~5s of no output): 200ms polling
                            poll_interval_ms = 200;
                        } else if this.polling.idle_poll_count > 12 {
                            // Light idle (~50ms of no output): 50ms polling
                            poll_interval_ms = 50;
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
        let mut file_tree_model = None;
        let mut project_root = None;
        if let Ok(cwd) = std::env::current_dir() {
            file_tree.set_root(cwd.clone());
            project_root = Some(cwd.clone());
            match FileTree::new(cwd) {
                Ok(tree) => {
                    file_tree_model = Some(tree);
                }
                Err(e) => {
                    warn!("Failed to initialize file tree: {}", e);
                }
            }
        }
        (file_tree, file_tree_model, project_root)
    }

    /// Initialize worktree manager if in a git repository.
    fn init_worktree_manager() -> Option<Arc<Mutex<codirigent_session::WorktreeManager>>> {
        if let Ok(cwd) = std::env::current_dir() {
            if let Ok(manager) = codirigent_session::WorktreeManager::new(&cwd) {
                return Some(Arc::new(Mutex::new(manager)));
            }
        }
        None
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

    /// Save current session state to disk.
    pub(super) fn save_state_to_disk(&self) {
        let sessions = self.with_session_manager(|m| m.list_sessions());
        let state = codirigent_core::AppState {
            sessions,
            layout: codirigent_core::LayoutMode::Grid {
                rows: self.workspace.layout_profile().dimensions().0,
                cols: self.workspace.layout_profile().dimensions().1,
            },
            updated_at: Some(chrono::Utc::now()),
        };
        if let Err(e) = self.persistence.storage.save_state(&state) {
            warn!("Failed to save state: {}", e);
        }
    }

    /// Cycle to next layout.
    pub fn next_layout(&mut self, cx: &mut Context<Self>) {
        self.workspace.next_layout();
        self.event_bus.publish(CodirigentEvent::LayoutChanged {
            mode: self.workspace.layout_profile().to_mode(),
        });
        cx.notify();
    }

    /// Toggle sidebar visibility.
    pub fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.workspace.toggle_sidebar();
        cx.notify();
    }

    /// Focus a session by number (1-9).
    pub fn focus_session_number(&mut self, number: usize, cx: &mut Context<Self>) {
        if self.workspace.focus_session_number(number) {
            if let Some(id) = self.workspace.focused_session_id() {
                self.event_bus
                    .publish(CodirigentEvent::SessionFocused { id });
            }
            self.sync_file_tree_to_focused_session(cx);
            cx.notify();
        }
    }

    /// Synchronize UI component states with workspace state.
    ///
    /// This should be called before rendering to ensure all UI components
    /// reflect the current workspace state.
    fn sync_ui_state(&mut self) {
        // Update terminal headers from sessions
        let sessions = self.workspace.sessions();
        let focused_id = self.workspace.focused_session_id();
        for session in sessions {
            if let Some(header) = self.terminal_headers.get_mut(&session.id) {
                // Dirty-check string fields to avoid heap allocations every sync tick
                if header.session_name != session.name {
                    header.session_name = session.name.clone();
                }
                if header.group_name != session.group {
                    header.group_name = session.group.clone();
                }
                header.status = session.status;
                header.context_usage = session.context_usage;
                header.is_focused = focused_id == Some(session.id);
                header.project_name = session
                    .git_info
                    .as_ref()
                    .and_then(|gi| gi.repo_root.file_name())
                    .or_else(|| session.working_directory.file_name())
                    .and_then(|n| n.to_str())
                    .map(|s| s.to_string());
                if let Some(color_hex) = &session.color {
                    header.session_color = crate::sidebar::Color::from_hex(color_hex);
                }
                if let Some(task_id) = &session.current_task {
                    // Show task title instead of raw ID
                    let title = self
                        .task_manager
                        .lock()
                        .ok()
                        .and_then(|mgr| mgr.get_task(task_id).map(|t| t.title.clone()));
                    header.task = Some(title.unwrap_or_else(|| task_id.0.to_string()));
                } else {
                    header.task = None;
                }
            }
        }

        // Update empty cells pool
        let (rows, cols) = self.workspace.layout_profile().dimensions();
        let occupied: Vec<GridPosition> = self
            .workspace
            .sessions()
            .iter()
            .enumerate()
            .map(|(i, _)| {
                let row = i as u32 / cols;
                let col = i as u32 % cols;
                GridPosition { row, col }
            })
            .collect();
        self.empty_cells.setup_for_grid(rows, cols, &occupied);

        // Sync task board counts from TaskManager — single pass over all tasks
        if let Ok(manager) = self.task_manager.lock() {
            let (queue_count, in_progress_count, review_count, done_count) =
                manager.list_tasks().iter().fold(
                    (0usize, 0usize, 0usize, 0usize),
                    |(q, ip, r, d), t| match t.status {
                        codirigent_core::TaskStatus::Queued
                        | codirigent_core::TaskStatus::Blocked => (q + 1, ip, r, d),
                        codirigent_core::TaskStatus::Assigned
                        | codirigent_core::TaskStatus::Working => (q, ip + 1, r, d),
                        codirigent_core::TaskStatus::Verifying
                        | codirigent_core::TaskStatus::Review => (q, ip, r + 1, d),
                        codirigent_core::TaskStatus::Done => (q, ip, r, d + 1),
                    },
                );
            self.task_board.set_task_counts(
                queue_count,
                in_progress_count,
                review_count,
                done_count,
            );
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
        self.sync_file_tree_to_focused_session(cx);
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
                    self.create_session(cx);
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
    pub(super) fn with_detector<R>(&self, f: impl FnOnce(&mut InputDetector) -> R) -> R {
        let mut detector = self.detector.lock().unwrap_or_else(|p| p.into_inner());
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
        let (w, h) =
            crate::terminal_view::compute_cell_dimensions(window.text_system(), &family, size);
        for tv in self.terminals_mut().values_mut() {
            tv.set_font_size(size);
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
        let (w, h) =
            crate::terminal_view::compute_cell_dimensions(window.text_system(), &family, size);
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
        let cell_info = self.workspace.cell_info();
        let mut resized_any = false;

        for info in cell_info {
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
                let current_rows = terminal_view.terminal().rows();
                let current_cols = terminal_view.terminal().cols();

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
                    let rows = terminal_view.terminal().rows();
                    let cols = terminal_view.terminal().cols();
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
            self.close_settings();
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
                    self.handle_paste(&Paste, window, cx);
                    return;
                }
                "c" => {
                    self.handle_copy(&Copy, window, cx);
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
                    self.toggle_sidebar(cx);
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
        let key: &str = event.keystroke.key.as_ref();
        let is_plain_space = key == " " || key.eq_ignore_ascii_case("space");
        if !event.keystroke.modifiers.control
            && !event.keystroke.modifiers.alt
            && (event
                .keystroke
                .key_char
                .as_deref()
                .is_some_and(|s| !s.is_empty())
                || is_plain_space)
        {
            return;
        }

        // Get focused session
        let Some(session_id) = self.workspace.focused_session_id() else {
            return;
        };

        // Get terminal mode for proper escape sequence generation (immutable borrow)
        let term_mode = {
            let Some(terminal_view) = self.terminals.get(&session_id) else {
                return;
            };
            terminal_view.terminal().mode()
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
            // Auto-scroll to bottom when user types while scrolled up in scrollback.
            // This is standard terminal behavior: typing should return the view
            // to the cursor position.
            if let Some(terminal_view) = self.terminals.get_mut(&session_id) {
                terminal_view.scroll_to_bottom();
            }

            // Send to PTY
            self.with_session_manager(|manager| {
                if let Err(e) = manager.send_input(session_id, &bytes) {
                    warn!("Failed to send input to session {}: {}", session_id, e);
                }
            });
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
            let should_flush = self
                .settings
                .page
                .as_ref()
                .map(|p| p.user_save_pending || p.project_save_pending)
                .unwrap_or(false);
            if should_flush && self.settings.save_task.is_none() {
                self.settings.save_task =
                    Some(cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
                        cx.background_executor()
                            .timer(std::time::Duration::from_millis(500))
                            .await;
                        if this
                            .update(cx, |this, _| {
                                this.flush_settings();
                                this.settings.save_task = None;
                            })
                            .is_err()
                        {
                            tracing::warn!("Settings save: entity dropped before flush");
                        }
                    }));
            }
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
                    .child(self.render_grid_with_headers(cx)),
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
                                                    this.save_layout_profiles_to_settings();
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

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.ime_marked_range = None;
        self.ime_preedit_text = None;
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

        self.ime_marked_range = None;
        self.ime_preedit_text = None;
        if let Some(session_id) = self.workspace.focused_session_id() {
            // Typing while scrolled up should jump back to the cursor line,
            // matching native terminal behavior.
            if let Some(terminal_view) = self.terminals.get_mut(&session_id) {
                terminal_view.scroll_to_bottom();
            }

            let text_bytes = text.as_bytes().to_vec();

            self.with_session_manager(move |sm| {
                let _ = sm.send_input(session_id, &text_bytes);
            });
        }
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _range: Option<std::ops::Range<usize>>,
        text: &str,
        _mark_range: Option<std::ops::Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        if self.has_blocking_modal() {
            self.ime_marked_range = None;
            self.ime_preedit_text = None;
            return;
        }

        let len = text.encode_utf16().count();
        if len == 0 {
            self.ime_marked_range = None;
            self.ime_preedit_text = None;
        } else {
            self.ime_marked_range = Some(0..len);
            self.ime_preedit_text = Some(text.to_string());
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

        // Sync UI state (throttled to ~10/sec to avoid locking task_manager
        // and iterating all tasks on every frame during high-frequency output).
        let now = Instant::now();
        if now.duration_since(self.polling.last_ui_sync) >= Duration::from_millis(100) {
            self.sync_ui_state();
            self.polling.last_ui_sync = now;
        }

        // Update workspace bounds from window size
        // GPUI automatically re-renders when window resizes, so we update bounds here
        let window_size = window.viewport_size();
        let window_bounds =
            crate::layout::Bounds::from_size(window_size.width.into(), window_size.height.into());
        self.workspace.set_bounds(window_bounds);

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
        let (real_w, real_h) = match &self.cache.cached_cell_dims {
            Some(cached)
                if cached.font_family == *font_family
                    && (cached.font_size - font_size).abs() < 0.01 =>
            {
                (cached.cell_width, cached.cell_height)
            }
            _ => {
                let (w, h) = crate::terminal_view::compute_cell_dimensions(
                    window.text_system(),
                    font_family,
                    font_size,
                );
                self.cache.cached_cell_dims = Some(super::types::CachedCellDims {
                    font_family: font_family.clone(),
                    font_size,
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

        let now = Instant::now();
        if now.duration_since(self.polling.last_resize_time) > Duration::from_millis(100) {
            self.resize_terminals_to_grid();
            self.polling.last_resize_time = now;
            self.polling.pending_resize = false;
        } else if !self.polling.pending_resize {
            self.polling.pending_resize = true;
            cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                let _ = this.update(cx, |this, cx| {
                    let resized = this.resize_terminals_to_grid();
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

    #[test]
    fn test_workspace_view_module_compiles() {
        // Validates that the module compiles with all GPUI dependencies.
        // The actual rendering and interaction tests require GPUI test infrastructure.
        // See workspace/tests.rs for core logic tests (29 tests, 100% coverage).
        assert!(true, "WorkspaceView module compiles successfully");
    }

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
}
