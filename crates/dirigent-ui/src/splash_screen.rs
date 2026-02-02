//! Splash screen component for Dirigent.
//!
//! Displays the Dirigent logo and branding during application startup.
//! The splash screen shows for a configurable duration before transitioning
//! to the main workspace.

use gpui::{
    div, hsla, px, App, AppContext, Context, Entity, FocusHandle, Focusable, InteractiveElement,
    IntoElement, ParentElement, Render, Styled, Window,
};
use std::time::Duration;

/// Brand colors used in the splash screen.
pub mod brand {
    use gpui::Hsla;

    /// Primary teal color (#4ECDC4)
    pub const TEAL: Hsla = Hsla {
        h: 176.0 / 360.0,
        s: 0.58,
        l: 0.55,
        a: 1.0,
    };

    /// Teal at 70% opacity
    pub const TEAL_70: Hsla = Hsla {
        h: 176.0 / 360.0,
        s: 0.58,
        l: 0.55,
        a: 0.7,
    };

    /// Teal at 40% opacity
    pub const TEAL_40: Hsla = Hsla {
        h: 176.0 / 360.0,
        s: 0.58,
        l: 0.55,
        a: 0.4,
    };

    /// Accent red/coral color (#FF6B6B)
    pub const CORAL: Hsla = Hsla {
        h: 0.0,
        s: 1.0,
        l: 0.71,
        a: 1.0,
    };

    /// Splash background color (#050508)
    pub const BACKGROUND: Hsla = Hsla {
        h: 240.0 / 360.0,
        s: 0.23,
        l: 0.025,
        a: 1.0,
    };

    /// Glow color (teal at 8% opacity)
    pub const GLOW: Hsla = Hsla {
        h: 176.0 / 360.0,
        s: 0.58,
        l: 0.55,
        a: 0.08,
    };

    /// Text color (white)
    pub const TEXT: Hsla = Hsla {
        h: 0.0,
        s: 0.0,
        l: 1.0,
        a: 1.0,
    };

    /// Muted text color (#666)
    pub const TEXT_MUTED: Hsla = Hsla {
        h: 0.0,
        s: 0.0,
        l: 0.4,
        a: 1.0,
    };

    /// Very muted text color (#333)
    pub const TEXT_VERY_MUTED: Hsla = Hsla {
        h: 0.0,
        s: 0.0,
        l: 0.2,
        a: 1.0,
    };
}

/// Splash screen view.
///
/// Displays the Dirigent logo, wordmark, and loading status during startup.
pub struct SplashScreen {
    /// Focus handle for the view.
    focus_handle: FocusHandle,
    /// Loading message to display.
    loading_message: String,
    /// Whether the splash is complete and ready to transition.
    is_complete: bool,
    /// Callback to invoke when splash is complete.
    on_complete: Option<Box<dyn FnOnce(&mut Context<Self>) + Send + 'static>>,
}

impl SplashScreen {
    /// Create a new splash screen with an automatic timer.
    ///
    /// Spawns a background timer that triggers the `on_complete` callback
    /// after the specified duration.
    ///
    /// # Arguments
    ///
    /// * `duration` - How long to show the splash screen
    /// * `on_complete` - Callback when splash duration is complete
    /// * `cx` - GPUI context
    pub fn new<F>(duration: Duration, on_complete: F, cx: &mut Context<Self>) -> Self
    where
        F: FnOnce(&mut Context<Self>) + Send + 'static,
    {
        // Spawn a timer that triggers completion after the duration.
        // Uses `async move |this, cx|` pattern for proper lifetime handling.
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(duration).await;
            this.update(cx, |this, cx| {
                this.complete(cx);
            })
        })
        .detach();

        Self {
            focus_handle: cx.focus_handle(),
            loading_message: "Loading modules...".to_string(),
            is_complete: false,
            on_complete: Some(Box::new(on_complete)),
        }
    }

    /// Update the loading message.
    pub fn set_loading_message(&mut self, message: impl Into<String>, cx: &mut Context<Self>) {
        self.loading_message = message.into();
        cx.notify();
    }

    /// Check if the splash screen is complete.
    pub fn is_complete(&self) -> bool {
        self.is_complete
    }

    /// Mark the splash as complete and trigger callback.
    fn complete(&mut self, cx: &mut Context<Self>) {
        self.is_complete = true;
        if let Some(callback) = self.on_complete.take() {
            callback(cx);
        }
    }

    /// Manually trigger completion (for external timer management).
    pub fn trigger_complete(&mut self, cx: &mut Context<Self>) {
        self.complete(cx);
    }

    /// Render a single grid cell of the logo.
    fn render_logo_cell(
        &self,
        color: gpui::Hsla,
        size: f32,
        radius: f32,
    ) -> impl IntoElement {
        div()
            .w(px(size))
            .h(px(size))
            .rounded(px(radius))
            .bg(color)
    }

    /// Render the 3x3 grid logo.
    fn render_logo(&self, scale: f32) -> impl IntoElement {
        let cell_size = 25.0 * scale;
        let gap = 7.0 * scale;
        let radius = 5.0 * scale;

        // Logo grid layout:
        // [100%] [70%]  [40%]
        // [70%]  [RED]  [70%]
        // [40%]  [70%]  [100%]

        div()
            .flex()
            .flex_col()
            .gap(px(gap))
            .child(
                // Row 1
                div()
                    .flex()
                    .flex_row()
                    .gap(px(gap))
                    .child(self.render_logo_cell(brand::TEAL, cell_size, radius))
                    .child(self.render_logo_cell(brand::TEAL_70, cell_size, radius))
                    .child(self.render_logo_cell(brand::TEAL_40, cell_size, radius)),
            )
            .child(
                // Row 2
                div()
                    .flex()
                    .flex_row()
                    .gap(px(gap))
                    .child(self.render_logo_cell(brand::TEAL_70, cell_size, radius))
                    .child(self.render_logo_cell(brand::CORAL, cell_size, radius))
                    .child(self.render_logo_cell(brand::TEAL_70, cell_size, radius)),
            )
            .child(
                // Row 3
                div()
                    .flex()
                    .flex_row()
                    .gap(px(gap))
                    .child(self.render_logo_cell(brand::TEAL_40, cell_size, radius))
                    .child(self.render_logo_cell(brand::TEAL_70, cell_size, radius))
                    .child(self.render_logo_cell(brand::TEAL, cell_size, radius)),
            )
    }

    /// Render the glow effect behind the logo.
    #[allow(dead_code)]
    fn render_glow(&self, size: f32) -> impl IntoElement {
        // Simulated radial gradient with concentric circles
        div()
            .absolute()
            .w(px(size))
            .h(px(size))
            .rounded_full()
            .bg(brand::GLOW)
    }
}

impl Focusable for SplashScreen {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for SplashScreen {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let version = env!("CARGO_PKG_VERSION");

        div()
            .size_full()
            .track_focus(&self.focus_handle(cx))
            .bg(brand::BACKGROUND)
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .relative()
            .child(
                // Glow effect container (behind logo)
                div()
                    .absolute()
                    .w(px(400.0))
                    .h(px(400.0))
                    .rounded_full()
                    .bg(hsla(176.0 / 360.0, 0.58, 0.55, 0.1)),
            )
            .child(
                // Main content
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_6()
                    .child(
                        // Logo
                        self.render_logo(1.0),
                    )
                    .child(
                        // Wordmark - using letter-spaced text via individual characters
                        div()
                            .text_size(px(28.0))
                            .text_color(brand::TEXT)
                            .flex()
                            .flex_row()
                            .gap(px(8.0))
                            .child("D")
                            .child("I")
                            .child("R")
                            .child("I")
                            .child("G")
                            .child("E")
                            .child("N")
                            .child("T"),
                    )
                    .child(
                        // Loading message
                        div()
                            .text_size(px(12.0))
                            .text_color(brand::TEXT_MUTED)
                            .child(self.loading_message.clone()),
                    ),
            )
            .child(
                // Version at bottom
                div()
                    .absolute()
                    .bottom_5()
                    .text_size(px(11.0))
                    .text_color(brand::TEXT_VERY_MUTED)
                    .child(format!("v{}", version)),
            )
    }
}

/// Create a splash screen entity.
///
/// # Arguments
///
/// * `duration` - How long to show the splash screen
/// * `on_complete` - Callback when splash is complete
/// * `cx` - Context that implements AppContext (typically from open_window callback)
pub fn create_splash_screen<C, F>(duration: Duration, on_complete: F, cx: &mut C) -> C::Result<Entity<SplashScreen>>
where
    C: AppContext,
    F: FnOnce(&mut Context<SplashScreen>) + Send + 'static,
{
    cx.new(|cx| SplashScreen::new(duration, on_complete, cx))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_brand_colors() {
        // Verify brand colors are defined correctly
        assert!(brand::TEAL.a == 1.0);
        assert!(brand::TEAL_70.a == 0.7);
        assert!(brand::TEAL_40.a == 0.4);
        assert!(brand::CORAL.a == 1.0);
        assert!(brand::BACKGROUND.a == 1.0);
    }

    #[test]
    fn test_brand_teal_hue() {
        // Teal should have hue around 176 degrees
        let expected_hue = 176.0 / 360.0;
        assert!((brand::TEAL.h - expected_hue).abs() < 0.01);
    }

    #[test]
    fn test_brand_coral_is_red_family() {
        // Coral should be in the red hue range (0 degrees)
        assert!(brand::CORAL.h < 0.1 || brand::CORAL.h > 0.9);
    }
}
