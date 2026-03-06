//! Reusable text input rendering helpers.

use gpui::{div, px, ParentElement, Styled};

/// Styling for a text input field.
#[derive(Debug, Clone, Copy)]
pub struct TextInputStyle {
    /// Input height in pixels.
    pub height: f32,
    /// Horizontal padding in pixels.
    pub padding_x: f32,
    /// Background color.
    pub bg: gpui::Hsla,
    /// Default border color.
    pub border: gpui::Hsla,
    /// Border color when focused.
    pub focus_border: gpui::Hsla,
    /// Border color when in error state.
    pub error_border: gpui::Hsla,
    /// Text color.
    pub text: gpui::Hsla,
}

/// Build a styled text input container.
///
/// Returns a plain `Div` (not `Stateful<Div>`) so callers can chain
/// `.cursor_pointer()` and other `Div`-only methods.
pub fn text_input(
    display_value: impl Into<String>,
    focused: bool,
    has_error: bool,
    style: &TextInputStyle,
) -> gpui::Div {
    let border = if focused {
        style.focus_border
    } else if has_error {
        style.error_border
    } else {
        style.border
    };

    // Note: .id() is not used here because it changes return type to Stateful<Div>,
    // which breaks callers that use .cursor_pointer() and other Div methods
    div()
        .h(px(style.height))
        .px(px(style.padding_x))
        .bg(style.bg)
        .border_1()
        .border_color(border)
        .rounded_md()
        .flex()
        .items_center()
        .child(
            div()
                .text_sm()
                .text_color(style.text)
                .child(display_value.into()),
        )
}
