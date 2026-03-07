//! Clipboard state management for WorkspaceView.

use crate::clipboard_preview::ClipboardPreview;
use crate::smart_clipboard::SmartClipboardProvider;
use codirigent_session::clipboard_service::DefaultClipboardService;
use std::sync::Arc;

/// Groups all clipboard-related state for the workspace.
pub(super) struct ClipboardState {
    /// Smart clipboard provider for cross-platform clipboard access.
    pub(super) smart_clipboard: Arc<dyn SmartClipboardProvider>,
    /// Clipboard polling service.
    pub(super) clipboard_service: DefaultClipboardService,
    /// Preview panel for clipboard content.
    pub(super) clipboard_preview: ClipboardPreview,
    /// Timestamp when clipboard preview was last shown (for auto-hide).
    pub(super) clipboard_preview_shown_at: Option<std::time::Instant>,
    /// Signature of the last clipboard image that triggered a preview.
    /// Used to suppress repeated preview popups for the same image when the
    /// platform clipboard sequence changes without a genuinely new image.
    pub(super) last_preview_image_signature: Option<u64>,
}
