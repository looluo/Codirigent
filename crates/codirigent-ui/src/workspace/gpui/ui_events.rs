//! GPUI event-processing helpers.

use super::WorkspaceView;
use crate::empty_session::EmptySessionEvent;
use codirigent_core::{GridPosition, LayoutMode};
use gpui::Context;
use std::time::{Duration, Instant};
use tracing::info;

impl WorkspaceView {
    /// Check if a session should be created at the given position.
    /// Returns true if this is not a duplicate click (same position within 100ms).
    fn should_create_session_at(&mut self, position: GridPosition) -> bool {
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

    /// Process pending events from all UI components.
    ///
    /// This method is called at the start of each render cycle to handle
    /// any pending events from task board, empty session cells, etc.
    pub(super) fn process_ui_events(&mut self, cx: &mut Context<Self>) {
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
    pub(in crate::workspace) fn process_top_bar_events(&mut self) {
        let events = self.top_bar.drain_events();
        for event in events {
            match event {
                crate::top_bar::TopBarEvent::LayoutSelected(layout_mode) => {
                    match layout_mode {
                        LayoutMode::Grid { rows, cols } => {
                            let profile = match (rows, cols) {
                                (2, 2) => crate::layout::LayoutProfile::Grid2x2,
                                (4, 1) => crate::layout::LayoutProfile::Stack1x4,
                                (2, 3) => crate::layout::LayoutProfile::Grid2x3,
                                (3, 3) => crate::layout::LayoutProfile::Grid3x3,
                                _ => crate::layout::LayoutProfile::Custom { rows, cols },
                            };
                            self.workspace.set_layout(profile);
                        }
                        LayoutMode::Single => {
                            self.workspace
                                .set_layout(crate::layout::LayoutProfile::Single);
                        }
                        LayoutMode::SplitTree { root } => {
                            self.workspace.set_split_tree(root);
                        }
                        LayoutMode::Custom { .. } => {
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

    /// Process icon rail events (drawer toggling, settings).
    pub(in crate::workspace) fn process_icon_rail_events(&mut self) {
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
}
