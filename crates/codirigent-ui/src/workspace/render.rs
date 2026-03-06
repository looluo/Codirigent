//! GPUI rendering components for WorkspaceView.
//!
//! This module coordinates rendering across component modules:
//! - [`terminal_render`] - Terminal canvas rendering (text, cursor, backgrounds)
//! - [`drawer_render`] - Drawer panels (sessions, files, worktrees)
//! - [`grid_render`] - Workspace grid and session cell layout
//! - [`icon_rail_render`] - Left sidebar icon rail
//! - [`task_board_render`] - Right task board panel
//! - [`top_bar_render`] - Top bar and session tabs
//! - [`modal_render`] - Action modals and dialogs
//! - [`icon_utils`] - Icon rendering utilities
//!
//! This file contains:
//! - Title bar rendering
//! - Terminal headers and empty cells
//! - Session menus

use super::gpui::WorkspaceView;
use crate::empty_session::EmptySessionRenderHints;
use crate::icons;
use crate::terminal_header::TerminalHeaderRenderHints;
use crate::title_bar::TitleBar;
use codirigent_core::SessionId;
use gpui::{
    div, px, ClickEvent, Context, FontWeight, InteractiveElement, IntoElement, MouseButton,
    ParentElement, SharedString, StatefulInteractiveElement, Styled, Window, WindowControlArea,
};
use std::borrow::Cow;
use tracing::info;

impl WorkspaceView {
    /// Render the title bar with window controls (minimize, maximize, close).
    ///
    /// This is a 32px bar with the logo on the left and native window controls
    /// on the right. The entire bar is a drag region for moving the window.
    pub(super) fn render_title_bar(
        &mut self,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = self.workspace().theme();
        let bg: gpui::Hsla = theme.panel_background.into();
        let border_color: gpui::Hsla = theme.border.into();
        let fg: gpui::Hsla = theme.foreground.into();

        // The entire bar is a drag region. Caption buttons use .occlude() +
        // their own WindowControlArea to carve out non-drag zones.
        let mut bar = div()
            .id("title-bar")
            .h(px(self.title_bar.height()))
            .w_full()
            .bg(bg)
            .border_b_1()
            .border_color(border_color)
            .flex()
            .items_center()
            .px_3()
            .gap_2()
            .window_control_area(WindowControlArea::Drag);

        // macOS: Native traffic lights are rendered by the OS.
        // Reserve left padding so content doesn't overlap them, and handle
        // double-click to trigger the system zoom behavior (following Zed's approach).
        #[cfg(target_os = "macos")]
        {
            const TRAFFIC_LIGHT_PADDING: f32 = 71.0;

            bar = if window.is_fullscreen() {
                bar.pl_2()
            } else {
                bar.pl(px(TRAFFIC_LIGHT_PADDING))
            };

            bar = bar.on_click(|event: &ClickEvent, window, _cx| {
                if event.click_count() == 2 {
                    window.titlebar_double_click();
                }
            });
        }

        // Windows: GPUI 0.2.1 has a stale mouse_hit_test issue in WM_NCHITTEST,
        // so WindowControlArea::Drag alone doesn't reliably initiate drags.
        // Work around by sending WM_NCLBUTTONDOWN(HTCAPTION) on mouse-down.
        #[cfg(target_os = "windows")]
        {
            use raw_window_handle::HasWindowHandle;
            let raw_handle = window.window_handle().ok().map(|h| match h.as_raw() {
                raw_window_handle::RawWindowHandle::Win32(win32) => win32.hwnd.get() as isize,
                _ => 0,
            });
            if let Some(hwnd) = raw_handle {
                bar = bar.on_mouse_down(MouseButton::Left, move |_event, _window, _cx| {
                    crate::platform_drag::begin_title_bar_drag(hwnd);
                });
            }
        }

        // Logo (3x3 grid matching logo-primary-dark.svg)
        bar = bar.child(div().flex_shrink_0().ml_2().child(self.render_logo_small()));
        bar = bar.child(
            div()
                .text_sm()
                .font_weight(FontWeight::BOLD)
                .text_color(fg)
                .ml_2()
                .child(TitleBar::LOGO_TEXT),
        );

        // Spacer — fills remaining space so window controls stay on the right
        bar = bar.child(div().flex_1());

        // Window controls (Windows/Linux)
        // Uses native Segoe icon fonts and WindowControlArea for OS-level handling.
        #[cfg(not(target_os = "macos"))]
        {
            /// Windows-standard close-button hover red (matches OS titlebar).
            const CLOSE_BUTTON_RED: gpui::Rgba = gpui::Rgba {
                r: 232.0 / 255.0,
                g: 17.0 / 255.0,
                b: 32.0 / 255.0,
                a: 1.0,
            };

            let icon_font = "Segoe MDL2 Assets";
            let close_hover_bg: gpui::Hsla = CLOSE_BUTTON_RED.into();

            let maximize_icon = if window.is_maximized() {
                "\u{e923}" // Restore
            } else {
                "\u{e922}" // Maximize
            };

            let controls = div()
                .flex()
                .flex_row()
                .items_center()
                .font_family(icon_font)
                // Minimize
                .child(
                    div()
                        .id("window-minimize")
                        .w(px(36.0))
                        .h_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(10.0))
                        .text_color(fg)
                        .occlude()
                        .hover(|style| style.bg(border_color.opacity(0.2)))
                        .active(|style| style.bg(border_color.opacity(0.3)))
                        .window_control_area(WindowControlArea::Min)
                        .child("\u{e921}"),
                )
                // Maximize / Restore
                .child(
                    div()
                        .id("window-maximize")
                        .w(px(36.0))
                        .h_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(10.0))
                        .text_color(fg)
                        .occlude()
                        .hover(|style| style.bg(border_color.opacity(0.2)))
                        .active(|style| style.bg(border_color.opacity(0.3)))
                        .window_control_area(WindowControlArea::Max)
                        .child(maximize_icon),
                )
                // Close
                .child(
                    div()
                        .id("window-close")
                        .w(px(36.0))
                        .h_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(10.0))
                        .text_color(fg)
                        .occlude()
                        .hover(|style| style.bg(close_hover_bg).text_color(gpui::Hsla::white()))
                        .active(|style| {
                            style
                                .bg(close_hover_bg.opacity(0.8))
                                .text_color(gpui::Hsla::white().opacity(0.8))
                        })
                        .window_control_area(WindowControlArea::Close)
                        .child("\u{e8bb}"),
                );
            bar = bar.child(controls);
        }

        bar
    }

    /// Render the unified top bar (replaces separate TitleBar + Toolbar).
    ///
    /// A single 48px bar containing: logo, layout tabs, broadcast toggle,
    pub(super) fn render_terminal_header(
        &self,
        session_id: SessionId,
        hints: &TerminalHeaderRenderHints,
    ) -> impl IntoElement {
        let theme = self.workspace().theme();
        let bg: gpui::Hsla = theme.panel_background.into();
        let border_color: gpui::Hsla = theme.border.into();
        let fg: gpui::Hsla = theme.foreground.into();

        let header_border = if hints.is_focused {
            let primary: gpui::Hsla = theme.primary.into();
            primary
        } else {
            border_color
        };

        let mut header = div()
            .id(SharedString::from(format!(
                "terminal-header-{}",
                session_id.0
            )))
            .h(px(hints.height))
            .w_full()
            .bg(bg)
            .border_b_1()
            .border_color(header_border)
            .flex()
            .items_center()
            .px_2()
            .gap_2();

        // Color indicator bar
        let color_indicator: gpui::Hsla = hints.color_indicator.into();
        header = header.child(
            div()
                .w(px(3.0))
                .h(px(16.0))
                .rounded_sm()
                .bg(color_indicator),
        );

        // Status dot
        let status_color: gpui::Hsla = hints.status.color.into();
        header = header.child(div().w(px(8.0)).h(px(8.0)).rounded_full().bg(status_color));

        // Session name
        header = header.child(
            div()
                .text_xs()
                .font_weight(FontWeight::MEDIUM)
                .text_color(fg)
                .overflow_hidden()
                .text_ellipsis()
                .child(hints.name.clone()),
        );

        // Project/directory name (after session name)
        if let Some(project) = &hints.project_name {
            let muted_fg = gpui::Hsla {
                h: 0.0,
                s: 0.0,
                l: 0.5,
                a: 0.7,
            };
            header = header.child(
                div()
                    .text_xs()
                    .text_color(muted_fg)
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(project.clone()),
            );
        }

        // Git branch badge (after session name)
        if let Some(branch) = &hints.git_branch {
            let git_muted = gpui::Hsla {
                h: 0.0,
                s: 0.0,
                l: 0.6,
                a: 0.8,
            };
            let branch_label: Cow<str> = if branch.chars().count() > 16 {
                let truncated: String = branch.chars().take(13).collect();
                Cow::Owned(format!("{}...", truncated))
            } else {
                Cow::Borrowed(branch.as_str())
            };
            let mut git_badge = div()
                .px(px(4.0))
                .py_px()
                .rounded_sm()
                .bg(gpui::Hsla {
                    h: 0.0,
                    s: 0.0,
                    l: 1.0,
                    a: 0.06,
                })
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap_1()
                .child(
                    div()
                        .text_xs()
                        .text_color(git_muted)
                        .child(icons::git_branch()),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(git_muted)
                        .child(branch_label.into_owned()),
                );

            if let Some(count) = hints.git_dirty_count {
                if count > 0 {
                    git_badge = git_badge.child(
                        div()
                            .text_xs()
                            .text_color(gpui::Hsla {
                                h: 0.1,
                                s: 0.8,
                                l: 0.6,
                                a: 1.0,
                            })
                            .child(format!("+{}", count)),
                    );
                }
            }

            header = header.child(git_badge);
        }

        // Group badge (if session is in a group)
        if let Some(group) = &hints.group_name {
            let group_color: gpui::Hsla = hints.color_indicator.into();
            let badge_bg = gpui::Hsla {
                h: group_color.h,
                s: group_color.s,
                l: group_color.l,
                a: 0.15,
            };
            let group_label: Cow<str> = if group.chars().count() > 12 {
                let truncated: String = group.chars().take(10).collect();
                Cow::Owned(format!("{}...", truncated))
            } else {
                Cow::Borrowed(group.as_str())
            };
            header = header.child(
                div()
                    .px(px(5.0))
                    .py_px()
                    .rounded_sm()
                    .bg(badge_bg)
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap_1()
                    .child(div().w(px(6.0)).h(px(6.0)).rounded_full().bg(group_color))
                    .child(
                        div()
                            .text_xs()
                            .text_color(group_color)
                            .child(group_label.into_owned()),
                    ),
            );
        }

        // Spacer
        header = header.child(div().flex_1());

        // Task badge (if any)
        if let Some(task) = &hints.task {
            let task_bg: gpui::Hsla = task.bg_color.into();
            let task_color: gpui::Hsla = task.text_color.into();
            header = header.child(
                div()
                    .px_2()
                    .py_px()
                    .rounded_sm()
                    .bg(task_bg)
                    .text_xs()
                    .text_color(task_color)
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(task.display_text.clone()),
            );
        }

        // Context usage (if any)
        if let Some(context) = &hints.context {
            let context_color: gpui::Hsla = context.color.into();
            header = header.child(
                div()
                    .text_xs()
                    .text_color(context_color)
                    .child(context.text().to_string()),
            );
        }

        header
    }

    /// Render an empty session cell.
    ///
    /// Returns a GPUI element representing an empty grid slot with a dashed border
    /// and a plus icon.
    pub(super) fn render_empty_cell(
        &self,
        hints: &EmptySessionRenderHints,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let bg: gpui::Hsla = hints.background.into();
        let border_color: gpui::Hsla = hints.border.color.into();
        let icon_color: gpui::Hsla = hints.icon_color.into();
        let text_color: gpui::Hsla = hints.text_color.into();
        let position = hints.position;

        div()
            .id(SharedString::from(format!(
                "empty-cell-{}-{}",
                position.row, position.col
            )))
            .flex_1()
            .bg(bg)
            .border_1()
            .border_color(border_color)
            .rounded_lg()
            .border_dashed()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_2()
            .cursor_pointer()
            .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                info!(?position, "Empty cell clicked");
                this.empty_cells.click(position);
                cx.notify();
            }))
            .child(div().text_xl().text_color(icon_color).child(hints.icon))
            .child(div().text_xs().text_color(text_color).child(hints.message))
    }

    /// This is the updated grid renderer that uses TerminalHeader for sessions
    /// Render terminal header inline (returns Stateful<Div> for type consistency).
    fn render_terminal_header_inline(
        &self,
        session_id: SessionId,
        hints: &TerminalHeaderRenderHints,
    ) -> gpui::Stateful<gpui::Div> {
        let theme = self.workspace().theme();
        let bg: gpui::Hsla = theme.panel_background.into();
        let border_color: gpui::Hsla = theme.border.into();
        let fg: gpui::Hsla = theme.foreground.into();

        let header_border = if hints.is_focused {
            let primary: gpui::Hsla = theme.primary.into();
            primary
        } else {
            border_color
        };

        let mut header = div()
            .id(SharedString::from(format!(
                "terminal-header-{}",
                session_id.0
            )))
            .h(px(hints.height))
            .w_full()
            .bg(bg)
            .border_b_1()
            .border_color(header_border)
            .flex()
            .items_center()
            .px_2()
            .gap_2();

        // Color indicator bar
        let color_indicator: gpui::Hsla = hints.color_indicator.into();
        header = header.child(
            div()
                .w(px(3.0))
                .h(px(16.0))
                .rounded_sm()
                .bg(color_indicator),
        );

        // Status dot
        let status_color: gpui::Hsla = hints.status.color.into();
        header = header.child(div().w(px(8.0)).h(px(8.0)).rounded_full().bg(status_color));

        // Session name
        header = header.child(
            div()
                .text_xs()
                .font_weight(FontWeight::MEDIUM)
                .text_color(fg)
                .overflow_hidden()
                .text_ellipsis()
                .child(hints.name.clone()),
        );

        // Project/directory name (after session name)
        if let Some(project) = &hints.project_name {
            let muted_fg = gpui::Hsla {
                h: 0.0,
                s: 0.0,
                l: 0.5,
                a: 0.7,
            };
            header = header.child(
                div()
                    .text_xs()
                    .text_color(muted_fg)
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(project.clone()),
            );
        }

        // Git branch badge (after session name)
        if let Some(branch) = &hints.git_branch {
            let git_muted = gpui::Hsla {
                h: 0.0,
                s: 0.0,
                l: 0.6,
                a: 0.8,
            };
            let branch_label: Cow<str> = if branch.chars().count() > 16 {
                let truncated: String = branch.chars().take(13).collect();
                Cow::Owned(format!("{}...", truncated))
            } else {
                Cow::Borrowed(branch.as_str())
            };
            let mut git_badge = div()
                .px(px(4.0))
                .py_px()
                .rounded_sm()
                .bg(gpui::Hsla {
                    h: 0.0,
                    s: 0.0,
                    l: 1.0,
                    a: 0.06,
                })
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap_1()
                .child(
                    div()
                        .text_xs()
                        .text_color(git_muted)
                        .child(icons::git_branch()),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(git_muted)
                        .child(branch_label.into_owned()),
                );

            if let Some(count) = hints.git_dirty_count {
                if count > 0 {
                    git_badge = git_badge.child(
                        div()
                            .text_xs()
                            .text_color(gpui::Hsla {
                                h: 0.1,
                                s: 0.8,
                                l: 0.6,
                                a: 1.0,
                            })
                            .child(format!("+{}", count)),
                    );
                }
            }

            header = header.child(git_badge);
        }

        // Group badge (if session is in a group)
        if let Some(group) = &hints.group_name {
            let group_color: gpui::Hsla = hints.color_indicator.into();
            let badge_bg = gpui::Hsla {
                h: group_color.h,
                s: group_color.s,
                l: group_color.l,
                a: 0.15,
            };
            let group_label: Cow<str> = if group.chars().count() > 12 {
                let truncated: String = group.chars().take(10).collect();
                Cow::Owned(format!("{}...", truncated))
            } else {
                Cow::Borrowed(group.as_str())
            };
            header = header.child(
                div()
                    .px(px(5.0))
                    .py_px()
                    .rounded_sm()
                    .bg(badge_bg)
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap_1()
                    .child(div().w(px(6.0)).h(px(6.0)).rounded_full().bg(group_color))
                    .child(
                        div()
                            .text_xs()
                            .text_color(group_color)
                            .child(group_label.into_owned()),
                    ),
            );
        }

        // Spacer
        header = header.child(div().flex_1());

        // Task badge (if any)
        if let Some(task) = &hints.task {
            let task_bg: gpui::Hsla = task.bg_color.into();
            let task_color: gpui::Hsla = task.text_color.into();
            header = header.child(
                div()
                    .px_2()
                    .py_px()
                    .rounded_sm()
                    .bg(task_bg)
                    .text_xs()
                    .text_color(task_color)
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(task.display_text.clone()),
            );
        }

        // Context usage (if any)
        if let Some(context) = &hints.context {
            let context_color: gpui::Hsla = context.color.into();
            header = header.child(
                div()
                    .text_xs()
                    .text_color(context_color)
                    .child(context.text().to_string()),
            );
        }

        header
    }

    /// Render empty cell inline with pre-computed colors (returns Stateful<Div>).
    pub(super) fn render_empty_cell_inline_with_colors(
        &mut self,
        position: codirigent_core::GridPosition,
        panel_bg: gpui::Hsla,
        border_color: gpui::Hsla,
        muted: gpui::Hsla,
        cell_height: f32,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        div()
            .id(SharedString::from(format!(
                "empty-cell-{}-{}",
                position.row, position.col
            )))
            .w_full()
            .h(px(cell_height))
            .bg(panel_bg)
            .border_1()
            .border_color(border_color)
            .rounded_lg()
            .border_dashed()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_2()
            .cursor_pointer()
            .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                info!(?position, "Empty cell clicked");
                this.empty_cells.click(position);
                cx.notify();
            }))
            .child(
                div()
                    .text_xl()
                    .text_color(muted)
                    .font_family(icons::LUCIDE_FONT_FAMILY)
                    .child(icons::circle_plus()),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child("Idle - Ready for next task"),
            )
    }

    /// Render empty cell inline (returns Stateful<Div>).
    fn render_empty_cell_inline(
        &mut self,
        position: codirigent_core::GridPosition,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let theme = self.workspace().theme().clone();
        let panel_bg: gpui::Hsla = theme.panel_background.into();
        let border_color: gpui::Hsla = theme.border.into();
        let muted: gpui::Hsla = theme.muted.into();

        let layout = self.grid_layout_with_task_board();
        let cell_height = layout.cell_size().height;

        self.render_empty_cell_inline_with_colors(
            position,
            panel_bg,
            border_color,
            muted,
            cell_height,
            cx,
        )
    }

    /// Render the custom layout picker modal.
    ///
    /// Displays a modal overlay with tabs for Grid and Split modes.
    /// Grid mode provides rows/columns inputs; Split mode provides an
    pub(super) fn render_session_menu(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement> {
        let session_id = self.selection.session_menu_open?;

        let theme = self.workspace().theme().clone();
        let panel_bg: gpui::Hsla = theme.panel_background.into();
        let border_color: gpui::Hsla = theme.border.into();
        let fg: gpui::Hsla = theme.foreground.into();
        let muted: gpui::Hsla = theme.muted.into();
        let hover_bg: gpui::Hsla = theme.active.into();
        let destructive = gpui::Hsla {
            h: 0.0,
            s: 0.7,
            l: 0.55,
            a: 1.0,
        };

        // Check if this session has a group
        let session_group = self
            .workspace()
            .session(session_id)
            .and_then(|s| s.group.clone());
        let has_group = session_group.is_some();

        // Collect existing group names (deduplicated, sorted)
        let existing_groups: Vec<String> = {
            let mut groups: Vec<String> = self
                .workspace()
                .sessions()
                .iter()
                .filter_map(|s| s.group.clone())
                .filter(|g| !g.is_empty())
                .collect();
            groups.sort();
            groups.dedup();
            groups
        };

        // Compute vertical position based on session's index in the list
        let sessions = self.workspace().sessions().to_vec();
        let mut row_index = 0usize;
        for (i, s) in sessions.iter().enumerate() {
            if s.id == session_id {
                row_index = i;
                break;
            }
        }
        let top_offset = crate::title_bar::TitleBar::DEFAULT_HEIGHT
            + crate::top_bar::TopBar::HEIGHT
            + 40.0   // drawer header
            + (row_index as f32) * 36.0;

        // Transparent click-away backdrop (no dark overlay)
        let backdrop = div()
            .id("session-menu-backdrop")
            .absolute()
            .inset_0()
            .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                this.close_session_menu(cx);
            }));

        // Build dropdown menu
        let mut dropdown = div()
            .w(px(180.0))
            .bg(panel_bg)
            .border_1()
            .border_color(border_color)
            .rounded_md()
            .overflow_hidden()
            .shadow_lg()
            .flex()
            .flex_col()
            .py_1();

        // Rename
        dropdown = dropdown.child(self.render_menu_item(
            "Rename",
            session_id,
            SessionMenuAction::Rename,
            &theme,
            hover_bg,
            fg,
            cx,
        ));

        // Separator before groups section
        dropdown = dropdown.child(div().h(px(1.0)).mx_2().my_1().bg(border_color));

        // Existing groups as direct-click options
        if !existing_groups.is_empty() {
            dropdown = dropdown.child(
                div().px_3().pt_1().pb(px(2.0)).child(
                    div()
                        .text_xs()
                        .text_color(muted.opacity(0.6))
                        .child("GROUPS"),
                ),
            );
            for group_name in &existing_groups {
                let is_current = session_group.as_deref() == Some(group_name.as_str());
                let label = if is_current {
                    format!("{} \u{2713}", group_name)
                } else {
                    group_name.clone()
                };
                dropdown = dropdown.child(self.render_menu_item(
                    &label,
                    session_id,
                    SessionMenuAction::AssignToGroup(group_name.clone()),
                    &theme,
                    hover_bg,
                    if is_current { muted } else { fg },
                    cx,
                ));
            }
        }

        // New Group...
        dropdown = dropdown.child(self.render_menu_item(
            "New Group\u{2026}",
            session_id,
            SessionMenuAction::NewGroup,
            &theme,
            hover_bg,
            fg,
            cx,
        ));

        // Remove Group (only if session has a group)
        if has_group {
            dropdown = dropdown.child(self.render_menu_item(
                "Remove Group",
                session_id,
                SessionMenuAction::RemoveGroup,
                &theme,
                hover_bg,
                fg,
                cx,
            ));
        }

        // Separator before destructive action
        dropdown = dropdown.child(div().h(px(1.0)).mx_2().my_1().bg(border_color));

        // End Session (destructive, clearly labeled)
        dropdown = dropdown.child(self.render_menu_item(
            "End Session",
            session_id,
            SessionMenuAction::EndSession,
            &theme,
            hover_bg,
            destructive,
            cx,
        ));

        // Position dropdown to the right of the drawer, aligned with the row
        let left_offset = crate::icon_rail::IconRail::WIDTH + self.drawer.width() - 8.0;

        Some(
            div()
                .id("session-menu-container")
                .absolute()
                .inset_0()
                .child(backdrop)
                .child(
                    div()
                        .absolute()
                        .left(px(left_offset))
                        .top(px(top_offset))
                        .child(dropdown),
                ),
        )
    }

    // Drawer, file tree, session rows, and group headers are in drawer_render.rs
}

/// Session menu actions.
#[derive(Debug, Clone)]
pub enum SessionMenuAction {
    Rename,
    AssignToGroup(String),
    NewGroup,
    RemoveGroup,
    EndSession,
}
