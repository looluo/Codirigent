//! Layout synchronization, focus transitions, and terminal resize helpers.

use super::WorkspaceView;
use crate::workspace::types::{
    CachedCellDims, TerminalResizeSignature, CELL_BORDER_WIDTH, HEADER_HEIGHT,
    TERMINAL_CONTENT_PADDING,
};
use codirigent_core::{CodirigentEvent, EventBus, SessionId, SessionManager};
use gpui::{Context, Window};
use std::time::{Duration, Instant};
use tracing::warn;

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

    /// Mark layout-derived render caches as dirty after structural changes.
    pub(in crate::workspace) fn mark_layout_cache_dirty(&mut self) {
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

    pub(super) fn render_focus_signature(&self) -> Option<SessionId> {
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

    /// Select a session (updates drawer context and grid focus).
    pub(in crate::workspace) fn select_session_with_cx(
        &mut self,
        session_id: SessionId,
        cx: &mut Context<Self>,
    ) {
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
                    cached.status_since = Instant::now();
                }
            }
        }
    }

    /// Resize all terminals to fit their current grid cell bounds.
    ///
    /// This should be called when the window is resized or the layout changes,
    /// to ensure terminals have the correct character dimensions for their pixel bounds.
    /// Returns `true` if any terminal was actually resized.
    fn resize_terminals_to_grid(&mut self) -> bool {
        // Layout constants from types.rs: HEADER_HEIGHT, TERMINAL_CONTENT_PADDING, CELL_BORDER_WIDTH
        let mut resized_any = false;

        for &info in &self.cache.render_cell_info {
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

    /// Sync terminal cell dimensions with font metrics, then throttle-trigger PTY resize.
    ///
    /// Uses a cache keyed on font family + size so font queries only run when
    /// terminal appearance settings change, not on every frame.
    ///
    /// Resize is debounced to <=10/sec to prevent PTY feedback loops during
    /// continuous window drag/resize.
    pub(super) fn sync_terminal_dimensions_and_resize(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
                self.cache.cached_cell_dims = Some(CachedCellDims {
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

#[cfg(test)]
mod tests {
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
}
