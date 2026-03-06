//! Terminal content rendering for WorkspaceView.
//!
//! Contains the canvas-based terminal rendering pipeline including:
//! - Text shaping and painting
//! - Background rectangle rendering
//! - Cursor rendering (block, hollow, beam, underline)
//! - IME pre-edit text overlay

use std::sync::Arc;

use super::gpui::WorkspaceView;
use crate::icons;
use crate::terminal_view::CursorShape;
use crate::theme::CodirigentTheme;
use codirigent_core::SessionId;
use gpui::{div, px, Entity, FocusHandle, IntoElement, ParentElement, Styled};
use std::cell::Cell;
use std::rc::Rc;

impl WorkspaceView {
    pub(super) fn render_terminal_content(
        &mut self,
        session_id: SessionId,
        theme: &CodirigentTheme,
        ime_context: Option<(Entity<WorkspaceView>, FocusHandle, bool, bool)>,
    ) -> (gpui::AnyElement, Rc<Cell<(f32, f32)>>) {
        // Shared cell for canvas origin (updated during prepaint)
        let canvas_origin: Rc<Cell<(f32, f32)>> = Rc::new(Cell::new((0.0, 0.0)));

        // IME pre-edit text should only be shown in the focused terminal pane.
        let ime_preedit_text = if matches!(ime_context.as_ref(), Some((_, _, true, true))) {
            self.ime_preedit_text.clone()
        } else {
            None
        };

        // Get the terminal view for this session
        let Some(terminal_view) = self.terminals_mut().get_mut(&session_id) else {
            // No terminal yet, show placeholder
            let terminal_bg: gpui::Hsla = theme.terminal_background.into();
            let terminal_fg: gpui::Hsla = theme.terminal_foreground.into();
            return (
                div()
                    .flex_1()
                    .bg(terminal_bg)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .text_base()
                            .text_color(terminal_fg)
                            .font_family(icons::LUCIDE_FONT_FAMILY)
                            .child(icons::terminal()),
                    )
                    .into_any_element(),
                canvas_origin,
            );
        };

        // Use cached theme colors (pre-converted in TerminalView constructor/set_theme)
        let terminal_bg = terminal_view.terminal_bg_hsla();
        let terminal_fg = terminal_view.terminal_fg_hsla();

        // Capture fallback dimensions (will be overridden by font metrics in prepaint)
        let fallback_cell_width = terminal_view.cell_width();
        let fallback_cell_height = terminal_view.cell_height();
        let font_size = terminal_view.font_size();
        let font_family_str = terminal_view.font_family().to_owned();
        let cursor_rect = terminal_view.cursor_rect();
        let needs_dimension_init = !terminal_view.dimensions_initialized();

        // Get cached content — Arc::clone is just a refcount bump (no deep copy)
        let content = terminal_view.cached_content();
        let bg_rects = content.bg_rects_hsla.clone();
        let text_runs = content.text_runs_hsla.clone();

        // Pre-convert cursor color (cursor position changes per-frame so not cacheable in content)
        let cursor_data = cursor_rect.map(|c| {
            let color: gpui::Hsla = c.color.into();
            (c, color)
        });

        let font_family: gpui::SharedString = font_family_str.into();

        // Clone Rc for capture into the canvas prepaint closure
        let canvas_origin_for_prepaint = Rc::clone(&canvas_origin);

        // Capture IME context for paint closure
        let ime_context_for_paint = ime_context.clone();

        // Build canvas element that paints directly
        let terminal_canvas = gpui::canvas(
            // Prepaint: shape text lines for each row's text runs
            move |bounds, window: &mut gpui::Window, _cx: &mut gpui::App| {
                // Store origin as f32 for arithmetic (Pixels doesn't support Add in gpui 0.2.1)
                let origin_x: f32 = bounds.origin.x.into();
                let origin_y: f32 = bounds.origin.y.into();
                let padding = super::types::TERMINAL_CONTENT_PADDING;
                let ox = origin_x + padding;
                let oy = origin_y + padding;

                // Store origin for mouse coordinate translation
                canvas_origin_for_prepaint.set((ox, oy));

                // Compute cell dimensions from font metrics (Zed pattern)
                // This ensures proper character spacing by using the actual 'm' advance width
                let (cell_width, cell_height) = if needs_dimension_init {
                    crate::terminal_view::compute_cell_dimensions(
                        window.text_system(),
                        &font_family,
                        font_size,
                    )
                } else {
                    (fallback_cell_width, fallback_cell_height)
                };

                // Shape text for each run (prepaint phase)
                let mut shaped_runs: Vec<(usize, usize, gpui::ShapedLine)> =
                    Vec::with_capacity(text_runs.len());
                let font_size_px = px(font_size);

                for (run, fg_color) in &text_runs {
                    let weight = if run.bold {
                        gpui::FontWeight::BOLD
                    } else {
                        gpui::FontWeight::NORMAL
                    };
                    let style = if run.italic {
                        gpui::FontStyle::Italic
                    } else {
                        gpui::FontStyle::Normal
                    };

                    let font = gpui::Font {
                        family: font_family.clone(),
                        features: gpui::FontFeatures::default(),
                        fallbacks: None,
                        weight,
                        style,
                    };

                    let underline = if run.underline {
                        Some(gpui::UnderlineStyle {
                            thickness: px(1.0),
                            color: Some(*fg_color),
                            wavy: false,
                        })
                    } else {
                        None
                    };

                    let strikethrough = if run.strikethrough {
                        Some(gpui::StrikethroughStyle {
                            thickness: px(1.0),
                            color: Some(*fg_color),
                        })
                    } else {
                        None
                    };

                    let text: gpui::SharedString = run.text.clone().into();
                    let text_run = gpui::TextRun {
                        len: text.len(),
                        font,
                        color: *fg_color,
                        background_color: None,
                        underline,
                        strikethrough,
                    };

                    let shaped =
                        window
                            .text_system()
                            .shape_line(text, font_size_px, &[text_run], None);

                    shaped_runs.push((run.row, run.start_col, shaped));
                }

                (
                    ox,
                    oy,
                    bg_rects,
                    shaped_runs,
                    {
                        if let (Some(preedit), Some((cursor, _))) =
                            (ime_preedit_text.as_ref(), cursor_data.as_ref())
                        {
                            if preedit.is_empty() {
                                None
                            } else {
                                let font = gpui::Font {
                                    family: font_family.clone(),
                                    features: gpui::FontFeatures::default(),
                                    fallbacks: None,
                                    weight: gpui::FontWeight::NORMAL,
                                    style: gpui::FontStyle::Normal,
                                };
                                let preedit_text: gpui::SharedString = preedit.clone().into();
                                let preedit_run = gpui::TextRun {
                                    len: preedit_text.len(),
                                    font,
                                    color: terminal_fg,
                                    background_color: None,
                                    underline: Some(gpui::UnderlineStyle {
                                        thickness: px(1.0),
                                        color: Some(terminal_fg),
                                        wavy: false,
                                    }),
                                    strikethrough: None,
                                };
                                let shaped = window.text_system().shape_line(
                                    preedit_text,
                                    font_size_px,
                                    &[preedit_run],
                                    None,
                                );
                                Some((cursor.x, cursor.y, shaped))
                            }
                        } else {
                            None
                        }
                    },
                    cursor_data,
                    cell_width,
                    cell_height,
                )
            },
            // Paint: draw backgrounds, text, and cursor
            move |bounds: gpui::Bounds<gpui::Pixels>,
                  prepaint_data: (
                f32,
                f32,
                Arc<Vec<(usize, usize, usize, gpui::Hsla)>>,
                Vec<(usize, usize, gpui::ShapedLine)>,
                Option<(f32, f32, gpui::ShapedLine)>,
                Option<(crate::terminal_view::CursorRect, gpui::Hsla)>,
                f32,
                f32,
            ),
                  window: &mut gpui::Window,
                  cx: &mut gpui::App| {
                let (ox, oy, bg_rects, shaped_runs, ime_preedit, cursor_data, cell_w, cell_h) =
                    prepaint_data;

                // Register input handler for IME if context is provided and it's the focused pane
                if let Some((ref entity, ref focus_handle, is_focused, input_enabled)) =
                    ime_context_for_paint
                {
                    if is_focused && input_enabled {
                        window.handle_input(
                            focus_handle,
                            gpui::ElementInputHandler::new(bounds, entity.clone()),
                            cx,
                        );
                    }
                }

                // 1. Paint background rectangles
                for (row, start_col, end_col, bg_color) in &bg_rects {
                    let rect_x = ox + *start_col as f32 * cell_w;
                    let rect_y = oy + *row as f32 * cell_h;
                    let rect_w = (*end_col - *start_col) as f32 * cell_w;
                    let rect_bounds = gpui::Bounds {
                        origin: gpui::Point {
                            x: px(rect_x),
                            y: px(rect_y),
                        },
                        size: gpui::Size {
                            width: px(rect_w),
                            height: px(cell_h),
                        },
                    };
                    window.paint_quad(gpui::fill(rect_bounds, *bg_color));
                }

                // 2. Paint shaped text runs
                for (row, start_col, shaped_line) in &shaped_runs {
                    let text_x = ox + *start_col as f32 * cell_w;
                    let text_y = oy + *row as f32 * cell_h;
                    let text_origin = gpui::Point {
                        x: px(text_x),
                        y: px(text_y),
                    };
                    let _ = shaped_line.paint(text_origin, px(cell_h), window, cx);
                }

                // 3. Paint IME pre-edit text at the cursor position.
                if let Some((preedit_x, preedit_y, preedit_line)) = &ime_preedit {
                    let preedit_origin = gpui::Point {
                        x: px(ox + *preedit_x),
                        y: px(oy + *preedit_y),
                    };
                    let _ = preedit_line.paint(preedit_origin, px(cell_h), window, cx);
                }

                // 4. Paint cursor
                if let Some((cursor, cursor_color)) = &cursor_data {
                    let cx_pos = ox + cursor.x;
                    let cy_pos = oy + cursor.y;

                    match cursor.shape {
                        CursorShape::Block => {
                            let cursor_bounds = gpui::Bounds {
                                origin: gpui::Point {
                                    x: px(cx_pos),
                                    y: px(cy_pos),
                                },
                                size: gpui::Size {
                                    width: px(cell_w),
                                    height: px(cell_h),
                                },
                            };
                            window.paint_quad(gpui::fill(cursor_bounds, cursor_color.opacity(0.7)));
                        }
                        CursorShape::HollowBlock => {
                            let cursor_bounds = gpui::Bounds {
                                origin: gpui::Point {
                                    x: px(cx_pos),
                                    y: px(cy_pos),
                                },
                                size: gpui::Size {
                                    width: px(cell_w),
                                    height: px(cell_h),
                                },
                            };
                            window.paint_quad(gpui::quad(
                                cursor_bounds,
                                px(0.0),
                                gpui::transparent_black(),
                                px(1.0),
                                *cursor_color,
                                gpui::BorderStyle::default(),
                            ));
                        }
                        CursorShape::Beam => {
                            let cursor_bounds = gpui::Bounds {
                                origin: gpui::Point {
                                    x: px(cx_pos),
                                    y: px(cy_pos),
                                },
                                size: gpui::Size {
                                    width: px(2.0),
                                    height: px(cell_h),
                                },
                            };
                            window.paint_quad(gpui::fill(cursor_bounds, *cursor_color));
                        }
                        CursorShape::Underline => {
                            let cursor_bounds = gpui::Bounds {
                                origin: gpui::Point {
                                    x: px(cx_pos),
                                    y: px(cy_pos + cell_h - 2.0),
                                },
                                size: gpui::Size {
                                    width: px(cell_w),
                                    height: px(2.0),
                                },
                            };
                            window.paint_quad(gpui::fill(cursor_bounds, *cursor_color));
                        }
                    }
                }
            },
        )
        .size_full();

        // Wrap canvas in a container with terminal background
        // Note: size_full() instead of flex_1() because parent has explicit dimensions
        let element = div()
            .size_full()
            .bg(terminal_bg)
            .overflow_hidden()
            .child(terminal_canvas)
            .into_any_element();

        (element, canvas_origin)
    }
}
