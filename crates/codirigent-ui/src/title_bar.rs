//! Title bar component.
//!
//! Provides a custom title bar with logo and drag area.
//! Window controls (minimize, maximize, close) are handled natively by GPUI's
//! `WindowControlArea` — no custom state machine is needed.

/// Title bar component state.
#[derive(Debug)]
pub struct TitleBar {
    /// Title bar height.
    height: f32,
}

impl Default for TitleBar {
    fn default() -> Self {
        Self::new()
    }
}

impl TitleBar {
    /// Default title bar height.
    pub const DEFAULT_HEIGHT: f32 = 32.0;
    /// Logo text.
    pub const LOGO_TEXT: &'static str = "CODIRIGENT";

    /// Create a new title bar.
    pub fn new() -> Self {
        Self {
            height: Self::DEFAULT_HEIGHT,
        }
    }

    /// Get the title bar height.
    pub fn height(&self) -> f32 {
        self.height
    }

    /// Set the title bar height.
    pub fn set_height(&mut self, height: f32) {
        self.height = height.max(24.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_title_bar_new() {
        let bar = TitleBar::new();
        assert_eq!(bar.height(), TitleBar::DEFAULT_HEIGHT);
    }

    #[test]
    fn test_title_bar_default() {
        let _bar = TitleBar::default();
    }

    #[test]
    fn test_set_height() {
        let mut bar = TitleBar::new();
        bar.set_height(40.0);
        assert_eq!(bar.height(), 40.0);

        // Minimum enforced
        bar.set_height(10.0);
        assert!(bar.height() >= 24.0);
    }

    #[test]
    fn test_logo_text() {
        assert_eq!(TitleBar::LOGO_TEXT, "CODIRIGENT");
    }
}
