//! Icon rail (left sidebar) rendering for workspace.
//!
//! This module handles rendering of the left sidebar icon rail,
//! including navigation icons and layout controls.

use crate::icon_rail::DrawerPanel;
use crate::icons;
use crate::workspace::gpui::WorkspaceView;
use gpui::{div, px, Context, InteractiveElement, IntoElement, MouseButton, ParentElement, Styled};

/// Icon rail button size in pixels (width and height).
const BUTTON_SIZE: f32 = 40.0;
/// Icon font size within rail buttons.
const ICON_SIZE: f32 = 20.0;

/// Build a single icon rail button element.
///
/// All rail buttons share the same size, shape, and layout -- only the
/// icon glyph, element ID, and active state differ.
fn rail_button(
    id: &'static str,
    icon: impl IntoElement,
    is_active: bool,
    active_bg: gpui::Hsla,
    fg: gpui::Hsla,
    muted: gpui::Hsla,
) -> gpui::Stateful<gpui::Div> {
    let btn_bg = if is_active {
        active_bg
    } else {
        gpui::Hsla::transparent_black()
    };
    let btn_fg = if is_active { fg } else { muted };

    div()
        .id(id)
        .w(px(BUTTON_SIZE))
        .h(px(BUTTON_SIZE))
        .rounded_xl()
        .bg(btn_bg)
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .child(
            div()
                .text_size(px(ICON_SIZE))
                .text_color(btn_fg)
                .font_family(icons::LUCIDE_FONT_FAMILY)
                .child(icon),
        )
}

impl WorkspaceView {
    pub(super) fn render_icon_rail(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.workspace().theme();
        let rail_bg: gpui::Hsla = theme.icon_rail_background.into();
        let border_color: gpui::Hsla = theme.border.into();
        let active_bg: gpui::Hsla = theme.active.into();
        let muted: gpui::Hsla = theme.muted.into();
        let fg: gpui::Hsla = theme.foreground.into();
        let active_panel = self.icon_rail.active_panel();

        div()
            .id("icon-rail")
            .w(px(crate::icon_rail::IconRail::WIDTH))
            .h_full()
            .bg(rail_bg)
            .border_r_1()
            .border_color(border_color)
            .flex()
            .flex_col()
            .items_center()
            .py_4()
            .gap_2()
            // Sessions button
            .child(
                rail_button(
                    "rail-sessions",
                    icons::terminal(),
                    active_panel == Some(DrawerPanel::Sessions),
                    active_bg,
                    fg,
                    muted,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.icon_rail.toggle_panel(DrawerPanel::Sessions);
                        this.process_icon_rail_events();
                        cx.notify();
                    }),
                ),
            )
            // Files button
            .child(
                rail_button(
                    "rail-files",
                    icons::folder_tree(),
                    active_panel == Some(DrawerPanel::Files),
                    active_bg,
                    fg,
                    muted,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.icon_rail.toggle_panel(DrawerPanel::Files);
                        this.process_icon_rail_events();
                        cx.notify();
                    }),
                ),
            )
            // Worktrees button
            .child(
                rail_button(
                    "rail-worktrees",
                    icons::git_branch(),
                    active_panel == Some(DrawerPanel::Worktrees),
                    active_bg,
                    fg,
                    muted,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.icon_rail.toggle_panel(DrawerPanel::Worktrees);
                        this.process_icon_rail_events();
                        cx.notify();
                    }),
                ),
            )
            // Spacer
            .child(div().flex_1())
            // Settings button (bottom)
            .child(
                rail_button(
                    "rail-settings",
                    icons::settings(),
                    false,
                    active_bg,
                    fg,
                    muted,
                )
                .rounded_lg()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.open_settings();
                        cx.notify();
                    }),
                ),
            )
    }
}
