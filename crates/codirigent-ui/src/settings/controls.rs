//! Reusable form controls for settings panels.
//!
//! Provides toggle, dropdown, number stepper, text input, path picker,
//! and section header components that return `impl IntoElement`.

use crate::theme::CodirigentTheme;
use gpui::{div, px, IntoElement, ParentElement, Styled};

/// Render a section header with title and optional "Reset" link.
pub fn settings_section_header(
    title: &str,
    theme: &CodirigentTheme,
    on_reset: bool,
) -> impl IntoElement {
    let fg: gpui::Hsla = theme.foreground.into();
    let accent: gpui::Hsla = theme.primary.into();
    let title = title.to_string();

    let mut row = div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .pb_1()
        .mb_2()
        .border_b_1()
        .border_color(gpui::Hsla { a: 0.15, ..fg })
        .child(
            div()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(fg)
                .child(title),
        );

    if on_reset {
        row = row.child(
            div()
                .text_size(px(theme.font_size_small))
                .text_color(accent)
                .cursor_pointer()
                .child("Reset to defaults"),
        );
    }

    row
}

/// Render a setting row with label, description, and a control element.
pub fn setting_row(
    label: &str,
    description: &str,
    theme: &CodirigentTheme,
    control: impl IntoElement,
) -> impl IntoElement {
    let fg: gpui::Hsla = theme.foreground.into();
    let muted: gpui::Hsla = theme.muted.into();
    let label = label.to_string();
    let description = description.to_string();

    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .py_2()
        .gap_4()
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .child(div().text_color(fg).child(label))
                .child(
                    div()
                        .text_size(px(theme.font_size_small))
                        .text_color(muted)
                        .child(description),
                ),
        )
        .child(control)
}

/// Render a toggle switch with sliding circle indicator.
pub fn setting_toggle(value: bool, theme: &CodirigentTheme) -> impl IntoElement {
    let accent: gpui::Hsla = theme.primary.into();
    let muted: gpui::Hsla = theme.muted.into();
    let track_bg = if value {
        accent
    } else {
        gpui::Hsla { a: 0.3, ..muted }
    };
    let circle_color = gpui::Hsla {
        h: 0.0,
        s: 0.0,
        l: 1.0,
        a: 1.0,
    };

    let circle = div()
        .w(px(16.0))
        .h(px(16.0))
        .rounded(px(8.0))
        .bg(circle_color);

    let mut track = div()
        .w(px(36.0))
        .h(px(20.0))
        .rounded(px(10.0))
        .bg(track_bg)
        .flex()
        .flex_row()
        .items_center()
        .px(px(2.0))
        .cursor_pointer();

    if value {
        // Push circle to the right with a flex spacer
        track = track.child(div().flex_1()).child(circle);
    } else {
        track = track.child(circle);
    }

    track
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_controls_compile() {
        // Validates that control functions compile with proper GPUI types.
        let theme = CodirigentTheme::dark();
        let _ = settings_section_header("Test", &theme, true);
        let _ = setting_row("Label", "Description", &theme, div());
        let _ = setting_toggle(true, &theme);
    }
}
