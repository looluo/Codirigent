use super::gpui::WorkspaceView;
use crate::theme::CodirigentTheme;
use codirigent_core::SessionId;
use gpui::{
    div, px, ClickEvent, Context, Focusable, InteractiveElement, IntoElement, MouseButton,
    MouseDownEvent, ParentElement, SharedString, StatefulInteractiveElement, Styled,
};
use std::time::Duration;

impl WorkspaceView {
    pub(super) fn open_terminal_search(&mut self, cx: &mut Context<Self>) {
        if self.has_blocking_modal() || self.settings.open {
            return;
        }

        let Some(session_id) = self.workspace.focused_session_id() else {
            return;
        };

        if let Some(terminal_view) = self.terminals.get_mut(&session_id) {
            terminal_view.open_search();
            if terminal_view.search_query().is_empty() {
                terminal_view.clear_search_matches();
                cx.notify();
                return;
            }
        }

        self.schedule_terminal_search(session_id, cx);
        cx.notify();
    }

    pub(super) fn handle_terminal_search_key_down(
        &mut self,
        event: &gpui::KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(session_id) = self.focused_search_session_id() else {
            return false;
        };

        let key = event.keystroke.key.to_lowercase();
        match key.as_str() {
            "escape" => {
                self.close_terminal_search(session_id, cx);
                return true;
            }
            "enter" | "down" => {
                self.navigate_terminal_search(session_id, !event.keystroke.modifiers.shift, cx);
                return true;
            }
            "up" => {
                self.navigate_terminal_search(session_id, false, cx);
                return true;
            }
            "backspace" => {
                if let Some(terminal_view) = self.terminals.get_mut(&session_id) {
                    terminal_view.pop_search_char();
                    if terminal_view.search_query().is_empty() {
                        terminal_view.clear_search_matches();
                        cx.notify();
                        return true;
                    }
                }
                self.schedule_terminal_search(session_id, cx);
                cx.notify();
                return true;
            }
            _ => {}
        }

        if event.keystroke.modifiers.control
            || event.keystroke.modifiers.alt
            || event.keystroke.modifiers.platform
            || event.keystroke.modifiers.function
            || Self::keystroke_is_text_input(event)
        {
            return true;
        }

        true
    }

    pub(super) fn render_terminal_search_overlay(
        &mut self,
        session_id: SessionId,
        theme: &CodirigentTheme,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let terminal_view = self.terminals.get(&session_id)?;
        let search = terminal_view.search();
        if !search.active {
            return None;
        }

        let panel_bg: gpui::Hsla = theme.panel_background.into();
        let border_color: gpui::Hsla = theme.border.into();
        let fg: gpui::Hsla = theme.foreground.into();
        let muted: gpui::Hsla = theme.muted.into();
        let accent: gpui::Hsla = theme.primary.into();
        let input_bg: gpui::Hsla = theme.background.into();

        let total = search.matches.len();
        let current = search.current_match.map(|index| index + 1).unwrap_or(0);
        let query = search.query.clone();

        let input_display = if query.is_empty() {
            div()
                .text_sm()
                .text_color(muted)
                .child("Find in terminal")
                .into_any_element()
        } else {
            div()
                .text_sm()
                .text_color(fg)
                .child(format!("{}|", query))
                .into_any_element()
        };

        let button = |label: &'static str,
                      session_id: SessionId,
                      forward: Option<bool>,
                      cx: &mut Context<Self>| {
            let hover_bg = accent.opacity(0.12);
            let mut button = div()
                .id(SharedString::from(format!(
                    "terminal-search-button-{}-{label}",
                    session_id.0
                )))
                .px_2()
                .h(px(28.0))
                .rounded_md()
                .border_1()
                .border_color(border_color)
                .flex()
                .items_center()
                .justify_center()
                .text_xs()
                .text_color(fg)
                .cursor_pointer()
                .hover(move |style| style.bg(hover_bg))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                        this.select_session_with_cx(session_id, cx);
                        window.focus(&this.focus_handle(cx));
                        cx.stop_propagation();
                    }),
                )
                .child(label);

            button = match forward {
                Some(forward) => {
                    button.on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                        this.navigate_terminal_search(session_id, forward, cx);
                    }))
                }
                None => button.on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                    this.close_terminal_search(session_id, cx);
                })),
            };

            button
        };

        Some(
            div()
                .occlude()
                .absolute()
                .top(px(10.0))
                .right(px(14.0))
                .w(px(300.0))
                .bg(panel_bg)
                .border_1()
                .border_color(border_color)
                .rounded_lg()
                .px_2()
                .py_2()
                .shadow_md()
                .flex()
                .items_center()
                .gap_2()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                        this.select_session_with_cx(session_id, cx);
                        window.focus(&this.focus_handle(cx));
                        cx.stop_propagation();
                    }),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                        this.select_session_with_cx(session_id, cx);
                        window.focus(&this.focus_handle(cx));
                        cx.stop_propagation();
                    }),
                )
                .child(
                    div()
                        .id(SharedString::from(format!(
                            "terminal-search-input-{}",
                            session_id.0
                        )))
                        .flex_1()
                        .h(px(32.0))
                        .px_2()
                        .bg(input_bg)
                        .border_1()
                        .border_color(accent.opacity(0.5))
                        .rounded_md()
                        .flex()
                        .items_center()
                        .child(input_display),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(if total == 0 { muted } else { fg })
                        .child(format!("{current} of {total}")),
                )
                .child(button("Prev", session_id, Some(false), cx))
                .child(button("Next", session_id, Some(true), cx))
                .child(button("X", session_id, None, cx))
                .into_any_element(),
        )
    }

    pub(super) fn focused_search_session_id(&self) -> Option<SessionId> {
        let session_id = self.workspace.focused_session_id()?;
        self.terminals
            .get(&session_id)
            .and_then(|terminal_view| terminal_view.search().active.then_some(session_id))
    }

    pub(super) fn close_terminal_search(&mut self, session_id: SessionId, cx: &mut Context<Self>) {
        if let Some(terminal_view) = self.terminals.get_mut(&session_id) {
            terminal_view.close_search();
            cx.notify();
        }
    }

    pub(super) fn schedule_terminal_search(
        &mut self,
        session_id: SessionId,
        cx: &mut Context<Self>,
    ) {
        let Some(terminal_view) = self.terminals.get(&session_id) else {
            return;
        };

        let query = terminal_view.search_query().to_string();
        let search_query = query.clone();
        let generation = terminal_view.search_generation();
        let runtime = terminal_view.runtime_handle();

        if query.is_empty() {
            if let Some(terminal_view) = self.terminals.get_mut(&session_id) {
                terminal_view.clear_search_matches();
            }
            return;
        }

        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(150))
                .await;

            let matches = cx
                .background_executor()
                .spawn(async move { runtime.search(&search_query) })
                .await;

            let _ = this.update(cx, |this, cx| {
                let Some(terminal_view) = this.terminals.get_mut(&session_id) else {
                    return;
                };
                if !terminal_view.search().active
                    || terminal_view.search_generation() != generation
                    || terminal_view.search_query() != query
                {
                    return;
                }

                terminal_view.set_search_matches(matches);
                if !terminal_view.search().matches.is_empty() {
                    terminal_view.scroll_to_search_match(0);
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn navigate_terminal_search(
        &mut self,
        session_id: SessionId,
        forward: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(terminal_view) = self.terminals.get(&session_id) else {
            return;
        };
        let total = terminal_view.search().matches.len();
        if total == 0 {
            return;
        }

        let query = terminal_view.search_query().to_string();
        let runtime = terminal_view.runtime_handle();
        let current = terminal_view.search().current_match;
        let matches = terminal_view.search().matches.clone();

        let base = match current {
            Some(index) => index,
            None if forward => total - 1,
            None => 0,
        };

        for offset in 1..=total {
            let index = if forward {
                (base + offset) % total
            } else {
                (base + total - (offset % total)) % total
            };
            let search_match = &matches[index];
            if runtime.match_still_matches(&query, search_match) {
                if let Some(terminal_view) = self.terminals.get_mut(&session_id) {
                    terminal_view.scroll_to_search_match(index);
                }
                cx.notify();
                return;
            }
        }

        if let Some(terminal_view) = self.terminals.get_mut(&session_id) {
            terminal_view.set_current_search_match(None);
            cx.notify();
        }
    }
}
