//! Clipboard operations for WorkspaceView.
//!
//! This module contains all methods related to:
//! - Copy/paste handling
//! - Image clipboard formatting
//! - File path clipboard operations

use super::gpui::WorkspaceView;
use crate::app::{Copy, Paste};
use codirigent_core::{ClipboardContent, SessionManager};
use codirigent_session::clipboard_service::ClipboardService;
use gpui::{Context, Window};
use std::path::PathBuf;
use tracing::warn;

enum PreparedClipboardPaste {
    Text(String),
    Files(Vec<PathBuf>),
    ImagePath(String),
}

impl WorkspaceView {
    fn apply_prepared_clipboard_paste(
        &mut self,
        session_id: codirigent_core::SessionId,
        bracketed: bool,
        prepared: PreparedClipboardPaste,
        cx: &mut Context<Self>,
    ) {
        if !self.terminals.contains_key(&session_id) {
            return;
        }

        let mut did_change_viewport = false;
        let mut hide_preview = false;
        let bytes = match prepared {
            PreparedClipboardPaste::Text(text) => {
                if text.is_empty() {
                    return;
                }
                let sanitized = crate::clipboard::sanitize_paste(&text);
                crate::clipboard::prepare_paste(&sanitized, bracketed)
            }
            PreparedClipboardPaste::Files(paths) => {
                if paths.is_empty() {
                    return;
                }
                let text = paths
                    .iter()
                    .map(|p| self.project.format_path_for_terminal(p))
                    .collect::<Vec<_>>()
                    .join(" ");
                crate::clipboard::prepare_paste(&text, bracketed)
            }
            PreparedClipboardPaste::ImagePath(formatted_path) => {
                if formatted_path.is_empty() {
                    return;
                }
                hide_preview = true;
                let sanitized = crate::clipboard::sanitize_paste(&formatted_path);
                crate::clipboard::prepare_paste(&sanitized, bracketed)
            }
        };

        if let Some(tv) = self.terminals.get_mut(&session_id) {
            did_change_viewport = tv.scroll_to_bottom_if_needed();
        }

        self.with_session_manager(|manager| {
            if let Err(e) = manager.send_input(session_id, &bytes) {
                warn!("Failed to paste to session {}: {}", session_id, e);
            }
        });

        if hide_preview {
            self.clipboard.clipboard_preview.hide();
            self.clipboard.clipboard_preview_shown_at = None;
        }

        if did_change_viewport || hide_preview {
            cx.notify();
        }
    }

    pub(super) fn handle_paste(
        &mut self,
        _action: &Paste,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(session_id) = self.workspace.focused_session_id() else {
            return;
        };

        // Read bracketed paste mode from terminal
        let bracketed = self
            .terminals
            .get(&session_id)
            .map(|tv| tv.terminal().bracketed_paste_mode())
            .unwrap_or(false);
        let clipboard = self.clipboard.smart_clipboard.clone();
        let cli_type = self.clipboard.clipboard_service.get_session_cli_type(session_id);
        let base_dir = self
            .clipboard
            .clipboard_service
            .temp_dir()
            .parent()
            .unwrap_or_else(|| self.clipboard.clipboard_service.temp_dir())
            .to_path_buf();

        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    match clipboard.read_content() {
                        Ok(ClipboardContent::Text(text)) => Ok(Some(PreparedClipboardPaste::Text(text))),
                        Ok(ClipboardContent::Files(paths)) => Ok(Some(PreparedClipboardPaste::Files(paths))),
                        Ok(ClipboardContent::Image(image_data)) => {
                            let content_for_bg = ClipboardContent::Image(image_data);
                            let service =
                                codirigent_session::clipboard_service::DefaultClipboardService::new(
                                    &base_dir,
                                );
                            service
                                .format_for_cli(&content_for_bg, cli_type)
                                .map(PreparedClipboardPaste::ImagePath)
                                .map(Some)
                        }
                        Ok(ClipboardContent::Empty) => Ok(None),
                        Err(err) => Err(err),
                    }
                })
                .await;

            let _ = this.update(cx, |this, cx| match result {
                Ok(Some(prepared)) => {
                    this.apply_prepared_clipboard_paste(session_id, bracketed, prepared, cx);
                }
                Ok(None) => {}
                Err(e) => {
                    warn!("Failed to read clipboard: {}", e);
                }
            });
        })
        .detach();
    }

    /// Handle Copy action (Cmd+C / Ctrl+C).
    ///
    /// Dual behavior:
    /// - If a text selection is active in the focused terminal, copies the
    ///   selected text to the system clipboard and clears the selection.
    /// - If no selection is active, sends Ctrl+C (interrupt, `\x03`) to the PTY.
    pub(super) fn handle_copy(
        &mut self,
        _action: &Copy,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(session_id) = self.workspace.focused_session_id() else {
            return;
        };

        // Check if there's an active selection in the focused terminal
        let selected_text = self
            .terminals
            .get(&session_id)
            .and_then(|tv| tv.get_selected_text());

        if let Some(text) = selected_text {
            // Copy selected text to system clipboard
            if let Err(e) = self.clipboard.smart_clipboard.write_text(text) {
                warn!("Failed to copy selection to clipboard: {}", e);
            }
            // Clear the selection
            if let Some(tv) = self.terminals.get_mut(&session_id) {
                tv.clear_selection();
            }
        } else {
            // No selection: send Ctrl+C (interrupt) to the PTY
            self.with_session_manager(|manager| {
                if let Err(e) = manager.send_input(session_id, b"\x03") {
                    warn!("Failed to send interrupt to session {}: {}", session_id, e);
                }
            });
        }

        cx.notify();
    }
}
