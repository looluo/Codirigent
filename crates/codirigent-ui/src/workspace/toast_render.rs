//! Toast notification rendering for auto-update UI.
//!
//! Renders a small overlay in the bottom-right corner of the workspace
//! showing update status: available, downloading, ready to apply, or
//! post-update confirmation.

use super::gpui::WorkspaceView;
use gpui::{
    div, px, ClickEvent, Context, FontWeight, InteractiveElement, IntoElement, ParentElement,
    SharedString, StatefulInteractiveElement, Styled,
};

impl WorkspaceView {
    /// Render the auto-update toast notification.
    ///
    /// Returns `None` when there is nothing to show (all update state is
    /// `None` or the user has dismissed the toast).
    pub(super) fn render_update_toast(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        // Determine which toast variant to show (priority order).
        let variant = if let Some(ref staged) = self.staged_update {
            ToastVariant::ReadyToApply {
                version: staged.version.to_string(),
            }
        } else if let Some(percent) = self.update_download_progress {
            ToastVariant::Downloading { percent }
        } else if let Some(ref info) = self.update_info {
            if self.update_dismissed {
                return None;
            }
            ToastVariant::UpdateAvailable {
                version: info.version.to_string(),
            }
        } else if let Some(ref version) = self.post_update_version {
            ToastVariant::PostUpdate {
                version: version.clone(),
            }
        } else {
            return None;
        };

        let theme = self.workspace().theme();
        let panel_bg: gpui::Hsla = theme.panel_background.into();
        let border_color: gpui::Hsla = theme.border.into();
        let fg: gpui::Hsla = theme.foreground.into();
        let muted: gpui::Hsla = theme.muted.into();
        let primary: gpui::Hsla = theme.primary.into();

        let mut toast = div()
            .id("update-toast")
            .absolute()
            .bottom(px(16.0))
            .right(px(16.0))
            .bg(panel_bg)
            .border_1()
            .border_color(border_color)
            .rounded_lg()
            .shadow_lg()
            .p_3()
            .flex()
            .flex_col()
            .gap_2()
            .max_w(px(320.0))
            .min_w(px(240.0));

        match variant {
            ToastVariant::UpdateAvailable { version } => {
                toast = toast
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(fg)
                                    .child(SharedString::from(format!(
                                        "Update available (v{})",
                                        version
                                    ))),
                            )
                            .child(self.render_dismiss_button(muted, cx)),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child("A new version of Codirigent is available."),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .justify_end()
                            .child(self.render_toast_button(
                                "update-btn",
                                "Update",
                                primary,
                                gpui::Hsla::white(),
                                |this: &mut Self, _: &ClickEvent, _window, cx: &mut gpui::Context<Self>| {
                                    if let Some(svc) = &this.update_service {
                                        svc.start_download();
                                    }
                                    cx.notify();
                                },
                                cx,
                            )),
                    );
            }
            ToastVariant::Downloading { percent } => {
                toast = toast
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(fg)
                            .child(SharedString::from(format!("Downloading... {}%", percent))),
                    )
                    .child(self.render_progress_bar(percent, primary, border_color))
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .justify_end()
                            .child(self.render_toast_button(
                                "cancel-download-btn",
                                "Cancel",
                                border_color,
                                fg,
                                |this: &mut Self, _: &ClickEvent, _window, cx: &mut gpui::Context<Self>| {
                                    if let Some(svc) = &this.update_service {
                                        svc.cancel_download();
                                    }
                                    this.update_download_progress = None;
                                    // Restore update_info from service state
                                    if let Some(svc) = &this.update_service {
                                        if let codirigent_updater::UpdateState::UpdateAvailable(
                                            info,
                                        ) = svc.state()
                                        {
                                            this.update_info = Some(info);
                                        }
                                    }
                                    cx.notify();
                                },
                                cx,
                            )),
                    );
            }
            ToastVariant::ReadyToApply { version } => {
                toast = toast
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(fg)
                                    .child(SharedString::from(format!(
                                        "Update ready (v{})",
                                        version
                                    ))),
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child("Restart to apply the update."),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .justify_end()
                            .child(self.render_toast_button(
                                "later-btn",
                                "Later",
                                border_color,
                                fg,
                                |this: &mut Self, _: &ClickEvent, _window, cx: &mut gpui::Context<Self>| {
                                    this.update_dismissed = true;
                                    cx.notify();
                                },
                                cx,
                            ))
                            .child(self.render_toast_button(
                                "restart-btn",
                                "Restart Now",
                                primary,
                                gpui::Hsla::white(),
                                |this: &mut Self, _: &ClickEvent, _window, cx: &mut gpui::Context<Self>| {
                                    // TODO: Replace this with a confirmation modal dialog in a
                                    // future iteration so the user can choose to force-restart
                                    // even when sessions are actively working.
                                    let has_working = this.workspace.sessions().iter().any(|s| {
                                        s.status == codirigent_core::SessionStatus::Working
                                    });
                                    if has_working {
                                        tracing::warn!(
                                            "Update restart blocked: one or more sessions are actively working. \
                                             Please wait until sessions are idle and try again."
                                        );
                                        return;
                                    }

                                    if let Some(svc) = &this.update_service {
                                        match svc.apply() {
                                            Ok(()) => {
                                                cx.quit();
                                            }
                                            Err(e) => {
                                                tracing::error!(
                                                    "Failed to apply update: {}",
                                                    e
                                                );
                                            }
                                        }
                                    }
                                },
                                cx,
                            )),
                    );
            }
            ToastVariant::PostUpdate { version } => {
                let release_url = self
                    .update_service
                    .as_ref()
                    .and_then(|svc| match svc.state() {
                        codirigent_updater::UpdateState::Idle => None,
                        codirigent_updater::UpdateState::UpdateAvailable(info) => {
                            Some(info.release_url.clone())
                        }
                        _ => None,
                    });

                toast = toast
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(fg)
                                    .child(SharedString::from(format!(
                                        "Updated to v{}",
                                        version
                                    ))),
                            )
                            .child(self.render_dismiss_button(muted, cx)),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child("Codirigent has been updated successfully."),
                    );

                if release_url.is_some() {
                    toast = toast.child(
                        div()
                            .flex()
                            .gap_2()
                            .justify_end()
                            .child(self.render_toast_button(
                                "release-notes-btn",
                                "Release Notes",
                                border_color,
                                fg,
                                move |this: &mut Self, _: &ClickEvent, _window, cx: &mut gpui::Context<Self>| {
                                    // Try to open release URL in browser
                                    if let Some(svc) = &this.update_service {
                                        // Use a generic release page URL
                                        let url = format!(
                                            "https://github.com/oso95/Codirigent/releases/tag/v{}",
                                            this.post_update_version
                                                .as_deref()
                                                .unwrap_or(env!("CARGO_PKG_VERSION"))
                                        );
                                        let _ = svc; // suppress unused warning
                                        open_url_in_browser(&url);
                                    }
                                    this.post_update_version = None;
                                    cx.notify();
                                },
                                cx,
                            )),
                    );
                }
            }
        }

        Some(toast)
    }

    /// Render a small dismiss (X) button for the toast.
    fn render_dismiss_button(
        &self,
        muted: gpui::Hsla,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .id("dismiss-update-toast")
            .text_xs()
            .text_color(muted)
            .cursor_pointer()
            .hover(|style| style.text_color(muted.opacity(0.7)))
            .px_1()
            .rounded_sm()
            .child("\u{2715}") // Unicode X mark
            .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                this.update_dismissed = true;
                this.post_update_version = None;
                cx.notify();
            }))
    }

    /// Render a styled button for the toast.
    fn render_toast_button(
        &self,
        id: &str,
        label: &str,
        bg: gpui::Hsla,
        text: gpui::Hsla,
        on_click: impl Fn(&mut Self, &ClickEvent, &mut gpui::Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .id(SharedString::from(id.to_string()))
            .px_3()
            .py(px(4.0))
            .rounded_md()
            .bg(bg)
            .text_xs()
            .text_color(text)
            .cursor_pointer()
            .hover(|style| style.opacity(0.85))
            .on_click(cx.listener(on_click))
            .child(SharedString::from(label.to_string()))
    }

    /// Render a simple progress bar.
    fn render_progress_bar(
        &self,
        percent: u8,
        fill_color: gpui::Hsla,
        track_color: gpui::Hsla,
    ) -> impl IntoElement {
        let width_pct = (percent as f32).clamp(0.0, 100.0);
        div()
            .w_full()
            .h(px(4.0))
            .rounded_sm()
            .bg(track_color)
            .child(
                div()
                    .h_full()
                    .rounded_sm()
                    .bg(fill_color)
                    .w(gpui::relative(width_pct / 100.0)),
            )
    }
}

/// Toast content variants.
enum ToastVariant {
    UpdateAvailable { version: String },
    Downloading { percent: u8 },
    ReadyToApply { version: String },
    PostUpdate { version: String },
}

/// Open a URL in the platform default browser.
fn open_url_in_browser(url: &str) {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(url).spawn();
    }
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", url])
            .spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    }
}
