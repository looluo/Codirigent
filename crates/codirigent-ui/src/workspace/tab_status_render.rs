//! Tab status indicator rendering for session tabs.
//!
//! Provides three configurable styles (dot, badge, glow) for showing
//! session status on tab pills. Animation policy: only NeedsAttention
//! and ResponseReady pulse, and only on background (non-active) tabs.
//!
//! Note: Tab animation rules differ from `StatusIndicator::animated`
//! (which flags Working and NeedsAttention for the pane header). This
//! module defines its own animation policy.

use crate::sidebar::Color;
use codirigent_core::SessionStatus;
use gpui::Hsla;

/// Decoration produced by the tab status renderer.
///
/// The caller uses this to apply the status indicator to each tab:
/// - `child`: a dot/badge element to prepend or append to the tab name
/// - `tab_bg` / `tab_border`: background tint for glow style
/// - `should_pulse`: whether this tab should animate (pulse opacity)
pub(super) struct TabStatusDecoration {
    /// Optional child element (dot/badge circle). None for glow style.
    pub child: Option<gpui::AnyElement>,
    /// Optional background color for the tab container (glow style).
    pub tab_bg: Option<Hsla>,
    /// Optional border color for the tab container (glow style).
    pub tab_border: Option<Hsla>,
    /// Whether this tab should pulse (NeedsAttention/ResponseReady on background tabs).
    pub should_pulse: bool,
}

/// Map a `SessionStatus` to its indicator color as HSLA.
fn status_color(status: SessionStatus) -> Hsla {
    let color = match status {
        SessionStatus::Idle => Color::from_hex("#52525b"),
        SessionStatus::Working => Color::from_hex("#f59e0b"),
        SessionStatus::NeedsAttention => Color::from_hex("#f43f5e"),
        SessionStatus::ResponseReady => Color::from_hex("#22c55e"),
        SessionStatus::Error => Color::from_hex("#ef4444"),
    };
    color.into()
}

/// Whether the given status should pulse on a background tab.
fn should_pulse_status(status: SessionStatus, is_active: bool) -> bool {
    if is_active {
        return false;
    }
    matches!(
        status,
        SessionStatus::NeedsAttention | SessionStatus::ResponseReady
    )
}

/// Render tab status decoration for the given style.
///
/// `style` is one of "dot", "badge", or "glow". Unknown values fall back to "dot".
/// `is_active` is true for the currently visible tab in the pane.
pub(super) fn render_tab_status(
    style: &str,
    status: SessionStatus,
    is_active: bool,
) -> TabStatusDecoration {
    let color = status_color(status);
    let pulse = should_pulse_status(status, is_active);

    match style {
        "glow" => TabStatusDecoration {
            child: None,
            tab_bg: Some(color.opacity(0.15)),
            tab_border: Some(color.opacity(0.25)),
            should_pulse: pulse,
        },
        // "dot", "badge", and any unknown value all produce a dot child.
        // The caller decides placement (prepend for "dot", append for "badge").
        _ => {
            use gpui::{div, px, IntoElement, Styled};
            let dot = div()
                .w(px(8.0))
                .h(px(8.0))
                .rounded_full()
                .bg(color)
                .flex_shrink_0()
                .into_any_element();
            TabStatusDecoration {
                child: Some(dot),
                tab_bg: None,
                tab_border: None,
                should_pulse: pulse,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── status_color tests ──────────────────────────────────────────

    #[test]
    fn test_status_color_idle() {
        let color = status_color(SessionStatus::Idle);
        assert!(color.a > 0.0);
    }

    #[test]
    fn test_status_color_all_variants_are_distinct() {
        let colors: Vec<Hsla> = [
            SessionStatus::Idle,
            SessionStatus::Working,
            SessionStatus::NeedsAttention,
            SessionStatus::ResponseReady,
            SessionStatus::Error,
        ]
        .iter()
        .map(|s| status_color(*s))
        .collect();

        for i in 0..colors.len() - 1 {
            assert_ne!(
                (colors[i].h, colors[i].s),
                (colors[i + 1].h, colors[i + 1].s),
                "colors for variants {} and {} should differ",
                i,
                i + 1
            );
        }
    }

    // ── should_pulse_status tests ───────────────────────────────────

    #[test]
    fn test_pulse_needs_attention_background() {
        assert!(should_pulse_status(SessionStatus::NeedsAttention, false));
    }

    #[test]
    fn test_pulse_response_ready_background() {
        assert!(should_pulse_status(SessionStatus::ResponseReady, false));
    }

    #[test]
    fn test_no_pulse_needs_attention_active() {
        assert!(!should_pulse_status(SessionStatus::NeedsAttention, true));
    }

    #[test]
    fn test_no_pulse_response_ready_active() {
        assert!(!should_pulse_status(SessionStatus::ResponseReady, true));
    }

    #[test]
    fn test_no_pulse_idle() {
        assert!(!should_pulse_status(SessionStatus::Idle, false));
        assert!(!should_pulse_status(SessionStatus::Idle, true));
    }

    #[test]
    fn test_no_pulse_working() {
        assert!(!should_pulse_status(SessionStatus::Working, false));
        assert!(!should_pulse_status(SessionStatus::Working, true));
    }

    #[test]
    fn test_no_pulse_error() {
        assert!(!should_pulse_status(SessionStatus::Error, false));
        assert!(!should_pulse_status(SessionStatus::Error, true));
    }

    // ── render_tab_status tests ─────────────────────────────────────
    //
    // Glow tests are safe (no GPUI element creation). Dot/badge tests
    // create GPUI elements — if they fail without a context, the pure
    // function tests above still cover the core logic.

    #[test]
    fn test_glow_style_has_bg_no_child() {
        let dec = render_tab_status("glow", SessionStatus::Working, false);
        assert!(dec.child.is_none());
        assert!(dec.tab_bg.is_some());
        assert!(dec.tab_border.is_some());
    }

    #[test]
    fn test_glow_pulse_on_needs_attention_background() {
        let dec = render_tab_status("glow", SessionStatus::NeedsAttention, false);
        assert!(dec.should_pulse);
    }

    #[test]
    fn test_glow_no_pulse_on_active_tab() {
        let dec = render_tab_status("glow", SessionStatus::NeedsAttention, true);
        assert!(!dec.should_pulse);
    }

    #[test]
    fn test_glow_no_pulse_idle() {
        let dec = render_tab_status("glow", SessionStatus::Idle, false);
        assert!(!dec.should_pulse);
    }

    #[test]
    fn test_dot_style_has_child_no_bg() {
        let dec = render_tab_status("dot", SessionStatus::Working, false);
        assert!(dec.child.is_some());
        assert!(dec.tab_bg.is_none());
        assert!(dec.tab_border.is_none());
    }

    #[test]
    fn test_badge_style_has_child_no_bg() {
        let dec = render_tab_status("badge", SessionStatus::Idle, true);
        assert!(dec.child.is_some());
        assert!(dec.tab_bg.is_none());
        assert!(dec.tab_border.is_none());
    }

    #[test]
    fn test_unknown_style_falls_back_to_dot() {
        let dec = render_tab_status("unknown", SessionStatus::Idle, false);
        assert!(dec.child.is_some());
        assert!(dec.tab_bg.is_none());
    }

    #[test]
    fn test_dot_pulse_on_response_ready_background() {
        let dec = render_tab_status("dot", SessionStatus::ResponseReady, false);
        assert!(dec.should_pulse);
    }
}
