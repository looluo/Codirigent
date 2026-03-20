use super::gpui::WorkspaceView;
use crate::terminal_view::SearchFocusControl;
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
        let focused_control = self
            .terminals
            .get(&session_id)
            .map(|terminal_view| terminal_view.search_focus_control())
            .unwrap_or(SearchFocusControl::Input);

        match key.as_str() {
            "escape" => {
                self.close_terminal_search(session_id, cx);
                return true;
            }
            "tab" => {
                if let Some(terminal_view) = self.terminals.get_mut(&session_id) {
                    terminal_view.cycle_search_focus_control(event.keystroke.modifiers.shift);
                }
                cx.notify();
                return true;
            }
            "left" => {
                if let Some(terminal_view) = self.terminals.get_mut(&session_id) {
                    terminal_view.cycle_search_focus_control(true);
                }
                cx.notify();
                return true;
            }
            "right" => {
                if let Some(terminal_view) = self.terminals.get_mut(&session_id) {
                    terminal_view.cycle_search_focus_control(false);
                }
                cx.notify();
                return true;
            }
            "enter" | "down" => {
                self.activate_terminal_search_control(
                    session_id,
                    focused_control,
                    !event.keystroke.modifiers.shift,
                    cx,
                );
                return true;
            }
            "up" => {
                self.activate_terminal_search_control(session_id, focused_control, false, cx);
                return true;
            }
            "space" | " " if focused_control != SearchFocusControl::Input => {
                self.activate_terminal_search_control(session_id, focused_control, true, cx);
                return true;
            }
            "backspace" => {
                if let Some(terminal_view) = self.terminals.get_mut(&session_id) {
                    terminal_view.set_search_focus_control(SearchFocusControl::Input);
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
        {
            return true;
        }

        if Self::keystroke_is_text_input(event) {
            if let Some(terminal_view) = self.terminals.get_mut(&session_id) {
                terminal_view.set_search_focus_control(SearchFocusControl::Input);
            }
            cx.notify();
            return false;
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
        let focused_control = search.focus_control;

        let input_display = if query.is_empty() {
            if focused_control == SearchFocusControl::Input {
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().w(px(8.0)).h(px(16.0)).rounded_sm().bg(accent))
                    .child(div().text_sm().text_color(muted).child("Type to search"))
                    .into_any_element()
            } else {
                div()
                    .text_sm()
                    .text_color(muted)
                    .child("Find in terminal")
                    .into_any_element()
            }
        } else if focused_control == SearchFocusControl::Input {
            div()
                .flex()
                .items_center()
                .gap_1()
                .child(div().text_sm().text_color(fg).child(query.clone()))
                .child(div().w(px(8.0)).h(px(16.0)).rounded_sm().bg(accent))
                .into_any_element()
        } else {
            div()
                .text_sm()
                .text_color(fg)
                .child(query.clone())
                .into_any_element()
        };

        let button = |label: &'static str,
                      control: SearchFocusControl,
                      session_id: SessionId,
                      forward: Option<bool>,
                      cx: &mut Context<Self>| {
            let hover_bg = accent.opacity(0.12);
            let is_focused = focused_control == control;
            let mut button = div()
                .id(SharedString::from(format!(
                    "terminal-search-button-{}-{label}",
                    session_id.0
                )))
                .px_2()
                .h(px(28.0))
                .rounded_md()
                .border_1()
                .border_color(if is_focused { accent } else { border_color })
                .bg(if is_focused {
                    accent.opacity(0.12)
                } else {
                    gpui::Hsla::transparent_black()
                })
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
                        if let Some(terminal_view) = this.terminals.get_mut(&session_id) {
                            terminal_view.set_search_focus_control(control);
                        }
                        window.focus(&this.focus_handle(cx));
                        cx.stop_propagation();
                        cx.notify();
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
                        .border_color(if focused_control == SearchFocusControl::Input {
                            accent
                        } else {
                            border_color
                        })
                        .rounded_md()
                        .flex()
                        .items_center()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                                this.select_session_with_cx(session_id, cx);
                                if let Some(terminal_view) = this.terminals.get_mut(&session_id) {
                                    terminal_view
                                        .set_search_focus_control(SearchFocusControl::Input);
                                }
                                window.focus(&this.focus_handle(cx));
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        )
                        .child(input_display),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(if total == 0 { muted } else { fg })
                        .child(format!("{current} of {total}")),
                )
                .child(button(
                    "Prev",
                    SearchFocusControl::Previous,
                    session_id,
                    Some(false),
                    cx,
                ))
                .child(button(
                    "Next",
                    SearchFocusControl::Next,
                    session_id,
                    Some(true),
                    cx,
                ))
                .child(button("X", SearchFocusControl::Close, session_id, None, cx))
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

    fn activate_terminal_search_control(
        &mut self,
        session_id: SessionId,
        control: SearchFocusControl,
        forward: bool,
        cx: &mut Context<Self>,
    ) {
        match control {
            SearchFocusControl::Input => {
                let action = self.terminals.get(&session_id).map(|terminal_view| {
                    if terminal_view.search_query().is_empty() {
                        SearchInputAction::None
                    } else if terminal_view.search().matches.is_empty() {
                        SearchInputAction::RunSearch
                    } else {
                        SearchInputAction::Navigate
                    }
                });

                match action {
                    Some(SearchInputAction::RunSearch) => {
                        self.schedule_terminal_search_with_delay(session_id, None, cx);
                    }
                    Some(SearchInputAction::Navigate) => {
                        self.navigate_terminal_search(session_id, forward, cx);
                    }
                    Some(SearchInputAction::None) | None => {}
                }
            }
            SearchFocusControl::Previous => self.navigate_terminal_search(session_id, false, cx),
            SearchFocusControl::Next => self.navigate_terminal_search(session_id, true, cx),
            SearchFocusControl::Close => self.close_terminal_search(session_id, cx),
        }
    }

    pub(super) fn schedule_terminal_search(
        &mut self,
        session_id: SessionId,
        cx: &mut Context<Self>,
    ) {
        self.schedule_terminal_search_with_delay(session_id, Some(Duration::from_millis(150)), cx);
    }

    fn schedule_terminal_search_with_delay(
        &mut self,
        session_id: SessionId,
        delay: Option<Duration>,
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
            if let Some(delay) = delay {
                cx.background_executor().timer(delay).await;
            }

            let search_inputs = match this.update(cx, |this, _cx| {
                let Some(terminal_view) = this.terminals.get(&session_id) else {
                    return None;
                };
                if !terminal_view.search().active
                    || terminal_view.search_generation() != generation
                    || terminal_view.search_query() != query
                {
                    return None;
                }

                Some((runtime.clone(), search_query.clone()))
            }) {
                Ok(Some(search_inputs)) => search_inputs,
                Ok(None) | Err(_) => return,
            };
            let (runtime, search_query) = search_inputs;

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
        let (total, query, runtime, current) = {
            let Some(terminal_view) = self.terminals.get(&session_id) else {
                return;
            };
            (
                terminal_view.search().matches.len(),
                terminal_view.search_query().to_string(),
                terminal_view.runtime_handle(),
                terminal_view.search().current_match,
            )
        };
        if total == 0 {
            return;
        }

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
            let Some(search_match) = self
                .terminals
                .get(&session_id)
                .and_then(|terminal_view| terminal_view.search().matches.get(index))
                .cloned()
            else {
                continue;
            };
            if runtime.match_still_matches(&query, &search_match) {
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

enum SearchInputAction {
    None,
    RunSearch,
    Navigate,
}
