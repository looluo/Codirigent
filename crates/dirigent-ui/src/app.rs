//! Main application module for Dirigent.
//!
//! This module provides the core application structure, actions, and
//! window management for the Dirigent IDE.
//!
//! Requires the `gpui` feature to be enabled.

use dirigent_core::DefaultEventBus;
use dirigent_detector::{DetectorConfig, InputDetector};
use dirigent_session::DefaultSessionManager;
use gpui::{
    actions, div, px, size, App, Application, Bounds, Context, FontWeight, IntoElement, Render,
    TitlebarOptions, Window, WindowBounds, WindowOptions,
};
use std::sync::{Arc, Mutex};
use tracing::info;

use crate::theme::DirigentTheme;

// Application actions
actions!(
    dirigent,
    [
        NewSession,
        CloseSession,
        FocusSession1,
        FocusSession2,
        FocusSession3,
        FocusSession4,
        FocusSession5,
        FocusSession6,
        FocusSession7,
        FocusSession8,
        FocusSession9,
        NextLayout,
        ToggleSidebar,
        Quit,
    ]
);

/// Main Dirigent application state.
///
/// Holds references to the session manager, input detector, and event bus
/// that are shared across the application.
pub struct DirigentApp {
    /// Session manager for PTY and session lifecycle.
    pub session_manager: Arc<Mutex<DefaultSessionManager>>,
    /// Input detector for monitoring session status.
    pub detector: Arc<Mutex<InputDetector>>,
    /// Event bus for cross-module communication.
    pub event_bus: Arc<DefaultEventBus>,
    /// Application theme.
    pub theme: DirigentTheme,
}

impl DirigentApp {
    /// Create a new application instance with default configuration.
    ///
    /// Initializes the session manager, input detector, and event bus.
    pub fn new() -> Self {
        let event_bus = Arc::new(DefaultEventBus::new(64));

        let session_manager = Arc::new(Mutex::new(DefaultSessionManager::new(event_bus.clone())));

        let detector = Arc::new(Mutex::new(InputDetector::new(
            DetectorConfig::default(),
            event_bus.clone(),
        )));

        Self {
            session_manager,
            detector,
            event_bus,
            theme: DirigentTheme::dark(),
        }
    }

    /// Create a new application instance with a specific theme.
    pub fn with_theme(theme: DirigentTheme) -> Self {
        let mut app = Self::new();
        app.theme = theme;
        app
    }

    /// Run the GPUI application.
    ///
    /// This starts the application event loop and opens the main window.
    /// This method does not return until the application is closed.
    pub fn run(self) {
        info!("Starting Dirigent GPUI application...");

        Application::new().run(move |cx: &mut App| {
            // Register global actions
            Self::register_actions(cx);

            // Create the main window
            let theme = self.theme.clone();
            let bounds = Bounds::centered(None, size(px(1200.), px(800.)), cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        title: Some("Dirigent".into()),
                        appears_transparent: true,
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                |_window, cx| cx.new(|_cx| PlaceholderView::new(theme)),
            )
            .expect("Failed to open main window");
        });
    }

    /// Register global application actions.
    fn register_actions(cx: &mut App) {
        cx.on_action(|_: &Quit, cx| {
            info!("Quit action triggered");
            cx.quit();
        });

        // Placeholder handlers for other actions
        // These will be properly implemented when the workspace view is created
        cx.on_action(|_: &NewSession, _cx| {
            info!("NewSession action triggered");
        });

        cx.on_action(|_: &CloseSession, _cx| {
            info!("CloseSession action triggered");
        });

        cx.on_action(|_: &NextLayout, _cx| {
            info!("NextLayout action triggered");
        });

        cx.on_action(|_: &ToggleSidebar, _cx| {
            info!("ToggleSidebar action triggered");
        });

        // Session focus actions
        cx.on_action(|_: &FocusSession1, _cx| {
            info!("FocusSession1 action triggered");
        });
        cx.on_action(|_: &FocusSession2, _cx| {
            info!("FocusSession2 action triggered");
        });
        cx.on_action(|_: &FocusSession3, _cx| {
            info!("FocusSession3 action triggered");
        });
        cx.on_action(|_: &FocusSession4, _cx| {
            info!("FocusSession4 action triggered");
        });
        cx.on_action(|_: &FocusSession5, _cx| {
            info!("FocusSession5 action triggered");
        });
        cx.on_action(|_: &FocusSession6, _cx| {
            info!("FocusSession6 action triggered");
        });
        cx.on_action(|_: &FocusSession7, _cx| {
            info!("FocusSession7 action triggered");
        });
        cx.on_action(|_: &FocusSession8, _cx| {
            info!("FocusSession8 action triggered");
        });
        cx.on_action(|_: &FocusSession9, _cx| {
            info!("FocusSession9 action triggered");
        });
    }
}

impl Default for DirigentApp {
    fn default() -> Self {
        Self::new()
    }
}

/// Placeholder view until the Workspace is implemented.
///
/// Displays a simple loading screen with the Dirigent branding.
struct PlaceholderView {
    theme: DirigentTheme,
}

impl PlaceholderView {
    /// Create a new placeholder view with the given theme.
    fn new(theme: DirigentTheme) -> Self {
        Self { theme }
    }
}

impl Render for PlaceholderView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        // Convert theme colors to GPUI Hsla types
        let bg: gpui::Hsla = self.theme.background.into();
        let fg: gpui::Hsla = self.theme.foreground.into();
        let idle_color: gpui::Hsla = self.theme.session_idle.into();
        let border_color: gpui::Hsla = self.theme.border.into();

        div()
            .size_full()
            .bg(bg)
            .text_color(fg)
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .child(
                div()
                    .text_xl()
                    .font_weight(FontWeight::BOLD)
                    .child("Dirigent"),
            )
            .child(
                div()
                    .mt_2()
                    .text_color(idle_color)
                    .child("AI Coding Agent Orchestration IDE"),
            )
            .child(
                div()
                    .mt_4()
                    .text_sm()
                    .text_color(border_color)
                    .child("Press Cmd+N to create a new session"),
            )
    }
}

#[cfg(all(test, feature = "gpui-full"))]
mod tests {
    use super::*;

    #[test]
    fn test_dirigent_app_new() {
        let app = DirigentApp::new();
        assert!(Arc::strong_count(&app.event_bus) >= 1);
        assert!(Arc::strong_count(&app.session_manager) == 1);
        assert!(Arc::strong_count(&app.detector) == 1);
    }

    #[test]
    fn test_dirigent_app_default() {
        let app = DirigentApp::default();
        assert!(Arc::strong_count(&app.event_bus) >= 1);
    }

    #[test]
    fn test_dirigent_app_with_theme() {
        let app = DirigentApp::with_theme(DirigentTheme::light());
        // Light theme has different background color
        assert!(app.theme.background != DirigentTheme::dark().background);
    }

    #[test]
    fn test_placeholder_view_creation() {
        let theme = DirigentTheme::dark();
        let view = PlaceholderView::new(theme.clone());
        assert_eq!(view.theme.background, theme.background);
    }
}
