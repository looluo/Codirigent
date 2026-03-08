//! Terminal view component for rendering terminal content.
//!
//! This module provides the `TerminalView` component that renders terminal cells,
//! cursor, and selection using GPUI. It integrates with alacritty_terminal's
//! `RenderableContent` to efficiently display terminal output.
//!
//! # Architecture
//!
//! The terminal view consists of:
//! - Cell rendering with proper color conversion
//! - Cursor rendering with multiple styles (block, beam, underline)
//! - Text selection support
//! - Scrollback buffer navigation
//!
//! # Performance
//!
//! The view is optimized for 120fps rendering through:
//! - Dirty state tracking (only render when content changes)
//! - Batched cell rendering (group cells by style)
//! - Efficient color conversion
//!
//! # Example
//!
//! ```rust,ignore
//! use codirigent_ui::{Terminal, TerminalView, CodirigentTheme};
//! use codirigent_core::SessionId;
//!
//! let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
//! let terminal = Terminal::new(24, 80, SessionId(1), tx);
//! let theme = CodirigentTheme::dark();
//! let mut view = TerminalView::new(terminal, theme);
//!
//! // Process output and render
//! view.terminal_mut().process_output(b"Hello, World!");
//! ```

use std::sync::Arc;

/// Ratio of font_size used as a conservative initial cell width estimate
/// before real font metrics arrive from `compute_cell_dimensions`.
/// Slightly underestimates typical monospace advance to avoid over-allocating cols.
const APPROX_CELL_WIDTH_RATIO: f32 = 0.55;

/// Ratio of font_size used as fallback cell width when GPUI font advance fails.
const FALLBACK_CELL_WIDTH_RATIO: f32 = 0.6;

/// Minimum initial cell width in pixels to stay sane at tiny font sizes.
const MIN_CELL_WIDTH_PX: f32 = 7.0;

use crate::terminal::Terminal;
use crate::terminal::TerminalSize;
use crate::terminal_colors::{convert_color, dim_color};
use crate::theme::{CodirigentTheme, Rgba};
use alacritty_terminal::grid::Scroll;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::Flags as CellFlags;
use alacritty_terminal::vte::ansi::{Color as TermColor, NamedColor};
use codirigent_core::SessionId;

/// A run of text with uniform style for efficient canvas painting.
#[derive(Debug, Clone)]
pub struct TextRunSegment {
    /// Concatenated characters in this run.
    pub text: String,
    /// Foreground color for the run.
    pub foreground: Rgba,
    /// Whether the run is bold.
    pub bold: bool,
    /// Whether the run is italic.
    pub italic: bool,
    /// Whether the run is underlined.
    pub underline: bool,
    /// Whether the run has strikethrough.
    pub strikethrough: bool,
    /// Row of the run.
    pub row: usize,
    /// Starting column of the run.
    pub start_col: usize,
    /// Number of cells in the run.
    pub cell_count: usize,
}

/// Pre-computed terminal content for canvas rendering.
#[derive(Debug)]
pub struct CachedTerminalContent {
    /// Pre-converted background rects with GPUI Hsla colors (avoids per-frame conversion).
    /// Wrapped in Arc for cheap per-frame cloning into canvas closures.
    pub bg_rects_hsla: Arc<Vec<(usize, usize, usize, gpui::Hsla)>>,
    /// Pre-converted text runs with GPUI Hsla foreground colors (avoids per-frame conversion).
    /// Wrapped in Arc for cheap per-frame cloning into canvas closures.
    pub text_runs_hsla: Arc<Vec<(TextRunSegment, gpui::Hsla)>>,
    /// Terminal rows at time of caching.
    pub rows: usize,
    /// Terminal columns at time of caching.
    pub cols: usize,
}

/// Cached content for a single visible terminal row.
#[derive(Debug, Clone, Default)]
pub(crate) struct CachedTerminalRow {
    pub(crate) bg_rects_hsla: Arc<Vec<(usize, usize, usize, gpui::Hsla)>>,
    pub(crate) text_runs_hsla: Arc<Vec<(TextRunSegment, gpui::Hsla)>>,
}

type ShapedTerminalRow = Arc<Vec<(usize, usize, gpui::ShapedLine)>>;

/// Cursor shape for rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CursorShape {
    /// Filled block cursor (default).
    #[default]
    Block,
    /// Hollow block (unfocused).
    HollowBlock,
    /// Vertical beam cursor.
    Beam,
    /// Horizontal underline cursor.
    Underline,
}

/// Selection state for text selection.
///
/// Tracks the start and end positions of the current selection,
/// if any.
#[derive(Debug, Clone, Default)]
pub struct Selection {
    /// Start position (grid line, column), if selection is active.
    pub start: Option<(i32, usize)>,
    /// End position (grid line, column), if selection is active.
    pub end: Option<(i32, usize)>,
}

impl Selection {
    /// Check if the selection is active (has both start and end).
    pub fn is_active(&self) -> bool {
        self.start.is_some() && self.end.is_some()
    }

    /// Clear the selection.
    pub fn clear(&mut self) {
        self.start = None;
        self.end = None;
    }

    /// Set the selection start position.
    pub fn set_start(&mut self, line: i32, col: usize) {
        self.start = Some((line, col));
    }

    /// Set the selection end position.
    pub fn set_end(&mut self, line: i32, col: usize) {
        self.end = Some((line, col));
    }

    /// Check if a cell position is within the selection.
    ///
    /// Returns `true` if the given (grid line, column) is selected.
    ///
    /// Comparison uses **lexicographic (row-major) order**: `(row, col)` tuples
    /// compare by row first, then column. This means the selection spans full
    /// rows between `start.row` and `end.row`, and only partial rows at the
    /// endpoints — which is standard terminal selection behaviour.
    pub fn contains(&self, line: i32, col: usize) -> bool {
        match (self.start, self.end) {
            (Some(start), Some(end)) => {
                // Normalize so start <= end
                let (start, end) = if start <= end {
                    (start, end)
                } else {
                    (end, start)
                };

                let pos = (line, col);
                pos >= start && pos <= end
            }
            _ => false,
        }
    }

    /// Get the normalized selection range (start <= end).
    ///
    /// Returns `None` if selection is not active.
    pub fn normalized(&self) -> Option<((i32, usize), (i32, usize))> {
        match (self.start, self.end) {
            (Some(start), Some(end)) => {
                if start <= end {
                    Some((start, end))
                } else {
                    Some((end, start))
                }
            }
            _ => None,
        }
    }
}

/// Terminal view component.
///
/// Renders terminal content to the screen, handling cells, cursor,
/// and selection display.
pub struct TerminalView {
    /// The underlying terminal emulator.
    terminal: Terminal,
    /// Theme for color resolution.
    theme: CodirigentTheme,
    /// Width of a single character cell in pixels.
    cell_width: f32,
    /// Height of a single character cell in pixels.
    cell_height: f32,
    /// Font size in pixels.
    font_size: f32,
    /// Font family name.
    font_family: String,
    /// Current text selection.
    selection: Selection,
    /// Cursor shape to render.
    cursor_shape: CursorShape,
    /// Whether the terminal view is focused.
    focused: bool,
    /// Cached terminal content for canvas rendering.
    cached_content: Option<CachedTerminalContent>,
    /// Per-row cached terminal content used for partial viewport rebuilds.
    cached_rows: Option<Vec<CachedTerminalRow>>,
    /// Font family used to build `cached_shaped_rows`.
    cached_shaped_font_family: Option<String>,
    /// Font size used to build `cached_shaped_rows`.
    cached_shaped_font_size: Option<f32>,
    /// Per-row shaped text derived from `cached_rows`.
    cached_shaped_rows: Option<Vec<ShapedTerminalRow>>,
    /// Whether the content needs to be recomputed.
    content_dirty: bool,
    /// Dirty viewport rows after a partial content rebuild.
    dirty_rows: Option<Vec<usize>>,
    /// Whether the next rebuild can reuse existing row caches.
    partial_rebuild_allowed: bool,
    /// Whether cell dimensions have been initialized from font metrics.
    dimensions_initialized: bool,
    /// Cached GPUI Hsla for terminal background (avoids per-frame conversion).
    cached_terminal_bg: gpui::Hsla,
    /// Cached GPUI Hsla for terminal foreground (avoids per-frame conversion).
    cached_terminal_fg: gpui::Hsla,
}

impl TerminalView {
    /// Create a new terminal view.
    pub fn new(mut terminal: Terminal, theme: CodirigentTheme) -> Self {
        let font_size = theme.terminal_font_size;
        let font_family = theme.terminal_font_family.clone();
        // Approximate cell dimensions until real font metrics arrive via
        // compute_cell_dimensions() on first render. Using conservative
        // ratios that slightly overestimate so the initial grid doesn't
        // allocate more rows/cols than will fit after correction.
        let cell_width = (font_size * APPROX_CELL_WIDTH_RATIO).max(MIN_CELL_WIDTH_PX);
        let cell_height = font_size.max(14.0);
        terminal.resize_with_cells(TerminalSize::new(
            terminal.rows(),
            terminal.cols(),
            cell_width,
            cell_height,
        ));

        let cached_terminal_bg: gpui::Hsla = theme.terminal_background.into();
        let cached_terminal_fg: gpui::Hsla = theme.terminal_foreground.into();

        Self {
            terminal,
            theme,
            cell_width,
            cell_height,
            font_size,
            font_family,
            selection: Selection::default(),
            cursor_shape: CursorShape::Block,
            focused: true,
            cached_content: None,
            cached_rows: None,
            cached_shaped_font_family: None,
            cached_shaped_font_size: None,
            cached_shaped_rows: None,
            content_dirty: true,
            dirty_rows: None,
            partial_rebuild_allowed: false,
            dimensions_initialized: false,
            cached_terminal_bg,
            cached_terminal_fg,
        }
    }

    /// Get a reference to the terminal.
    pub fn terminal(&self) -> &Terminal {
        &self.terminal
    }

    /// Get a mutable reference to the terminal.
    ///
    /// Marks content as dirty since callers typically modify terminal state.
    pub fn terminal_mut(&mut self) -> &mut Terminal {
        self.mark_output_dirty();
        &mut self.terminal
    }

    /// Get the theme.
    pub fn theme(&self) -> &CodirigentTheme {
        &self.theme
    }

    /// Set the theme.
    pub fn set_theme(&mut self, theme: CodirigentTheme) {
        self.cached_terminal_bg = theme.terminal_background.into();
        self.cached_terminal_fg = theme.terminal_foreground.into();
        self.theme = theme;
        self.invalidate_content_cache();
    }

    /// Get the cell width in pixels.
    pub fn cell_width(&self) -> f32 {
        self.cell_width
    }

    /// Get the cell height in pixels.
    pub fn cell_height(&self) -> f32 {
        self.cell_height
    }

    /// Set the cell dimensions.
    ///
    /// Marks the view as having initialized dimensions and triggers a
    /// terminal resize with the new cell metrics.
    pub fn set_cell_dimensions(&mut self, width: f32, height: f32) {
        self.cell_width = width;
        self.cell_height = height;
        self.dimensions_initialized = true;
        self.invalidate_content_cache();
        self.terminal.resize_with_cells(TerminalSize::new(
            self.terminal.rows(),
            self.terminal.cols(),
            width,
            height,
        ));
    }

    /// Check if cell dimensions have been initialized from font metrics.
    pub fn dimensions_initialized(&self) -> bool {
        self.dimensions_initialized
    }

    /// Get the font size.
    pub fn font_size(&self) -> f32 {
        self.font_size
    }

    /// Set the font size.
    pub fn set_font_size(&mut self, size: f32) {
        self.font_size = size;
        self.cached_shaped_font_size = None;
    }

    /// Get the font family.
    pub fn font_family(&self) -> &str {
        &self.font_family
    }

    /// Get cached terminal background color as GPUI Hsla.
    pub fn terminal_bg_hsla(&self) -> gpui::Hsla {
        self.cached_terminal_bg
    }

    /// Get cached terminal foreground color as GPUI Hsla.
    pub fn terminal_fg_hsla(&self) -> gpui::Hsla {
        self.cached_terminal_fg
    }

    /// Set the font family.
    pub fn set_font_family(&mut self, family: String) {
        self.font_family = family;
        self.cached_shaped_font_family = None;
    }

    /// Get the current selection.
    pub fn selection(&self) -> &Selection {
        &self.selection
    }

    /// Get mutable access to the selection.
    pub fn selection_mut(&mut self) -> &mut Selection {
        &mut self.selection
    }

    /// Set the cursor shape.
    pub fn set_cursor_shape(&mut self, shape: CursorShape) {
        self.cursor_shape = shape;
    }

    /// Get the cursor shape.
    pub fn cursor_shape(&self) -> CursorShape {
        self.cursor_shape
    }

    /// Set the focused state.
    pub fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }

    /// Check if the view is focused.
    pub fn is_focused(&self) -> bool {
        self.focused
    }

    /// Get the session ID.
    pub fn session_id(&self) -> SessionId {
        self.terminal.session_id()
    }

    /// Scroll up by the specified number of lines (show older content).
    ///
    /// Positive `Scroll::Delta` increases `display_offset`, moving the
    /// viewport up into the scrollback buffer.
    pub fn scroll_up(&mut self, lines: usize) {
        self.invalidate_content_cache();
        self.terminal
            .term_mut()
            .scroll_display(Scroll::Delta(lines as i32));
    }

    /// Scroll down by the specified number of lines (show newer content).
    ///
    /// Negative `Scroll::Delta` decreases `display_offset`, moving the
    /// viewport down toward the most recent output.
    pub fn scroll_down(&mut self, lines: usize) {
        self.invalidate_content_cache();
        self.terminal
            .term_mut()
            .scroll_display(Scroll::Delta(-(lines as i32)));
    }

    /// Scroll to the bottom (most recent output).
    pub fn scroll_to_bottom(&mut self) {
        let _ = self.scroll_to_bottom_if_needed();
    }

    /// Scroll to the bottom only when the viewport is currently in scrollback.
    ///
    /// Returns `true` if the viewport changed.
    pub fn scroll_to_bottom_if_needed(&mut self) -> bool {
        if !self.is_scrolled_back() {
            return false;
        }

        self.invalidate_content_cache();
        self.terminal.term_mut().scroll_display(Scroll::Bottom);
        true
    }

    /// Check whether the viewport is showing scrollback instead of the live prompt.
    pub fn is_scrolled_back(&self) -> bool {
        self.terminal.term().renderable_content().display_offset != 0
    }

    fn viewport_row_to_grid_line(&self, row: usize) -> i32 {
        row as i32 - self.terminal.term().grid().display_offset() as i32
    }

    /// Clear the terminal screen while preserving the current line (prompt).
    ///
    /// This clears the scrollback and visible content while keeping
    /// the current line (typically the shell prompt) at the top.
    pub fn clear(&mut self) {
        self.invalidate_content_cache();
        self.terminal.clear();
    }

    /// Get cursor rendering information.
    ///
    /// Returns `None` if the cursor is not visible or is scrolled off-screen.
    /// Uses viewport-relative coordinates from `renderable_content()` so the
    /// cursor position is correct when the terminal is scrolled.
    pub fn cursor_rect(&self) -> Option<CursorRect> {
        if !self.terminal.cursor_visible() {
            return None;
        }

        let (x, y) = self.cursor_pos_in_viewport()?;

        let shape = if self.focused {
            self.cursor_shape
        } else {
            CursorShape::HollowBlock
        };

        Some(CursorRect {
            x,
            y,
            width: self.cell_width,
            height: self.cell_height,
            color: self.theme.terminal_cursor,
            shape,
        })
    }

    /// Returns the cursor (x, y) position for IME preedit anchoring.
    ///
    /// Unlike `cursor_rect`, this ignores `\e[?25l` cursor-hide mode so the
    /// preedit overlay always tracks the real cursor location — even during
    /// Claude Code / Ink redraw cycles. The hide/show sequence completes
    /// within a single PTY poll batch, so by the time GPUI renders the next
    /// frame the cursor is already back at the input row.
    pub fn ime_anchor_pos(&self) -> Option<(f32, f32)> {
        self.cursor_pos_in_viewport()
    }

    /// Computes viewport-relative (x, y) pixel position of the cursor.
    /// Returns `None` if the cursor is scrolled off-screen.
    fn cursor_pos_in_viewport(&self) -> Option<(f32, f32)> {
        let content = self.terminal.term().renderable_content();
        let display_offset = content.display_offset;
        let cursor_point = content.cursor.point;

        // Convert grid-relative cursor line to viewport-relative row.
        let viewport_line = cursor_point.line.0 + display_offset as i32;
        let rows = self.terminal.rows() as usize;
        if viewport_line < 0 || viewport_line as usize >= rows {
            return None;
        }

        let row = viewport_line as usize;
        let col = cursor_point.column.0;
        Some((col as f32 * self.cell_width, row as f32 * self.cell_height))
    }

    /// Calculate pixel dimensions for the current terminal size.
    #[cfg(test)]
    pub(crate) fn pixel_size(&self) -> (f32, f32) {
        let width = self.terminal.cols() as f32 * self.cell_width;
        let height = self.terminal.rows() as f32 * self.cell_height;
        (width, height)
    }

    /// Calculate terminal dimensions from pixel size.
    pub fn dimensions_from_pixels(&self, width: f32, height: f32) -> (u16, u16) {
        let cols = (width / self.cell_width).floor() as u16;
        let rows = (height / self.cell_height).floor() as u16;
        (rows.max(1), cols.max(1))
    }

    /// Resize the terminal to fit within the given pixel dimensions.
    ///
    /// Returns `true` if the terminal was actually resized, `false` if
    /// dimensions were already at the target size (no-op).
    pub fn resize_to_fit(&mut self, width: f32, height: f32) -> bool {
        let (rows, cols) = self.dimensions_from_pixels(width, height);
        if rows == self.terminal.rows() && cols == self.terminal.cols() {
            return false;
        }
        self.invalidate_content_cache();
        self.terminal.resize(rows, cols);
        true
    }

    /// Start a new text selection at the given cell position.
    ///
    /// Converts the viewport row into a stable grid line, clears any previous
    /// end, and marks dirty.
    pub fn start_selection(&mut self, row: usize, col: usize) {
        self.selection
            .set_start(self.viewport_row_to_grid_line(row), col);
        self.selection.end = None;
        self.invalidate_content_cache();
    }

    /// Update the selection end position during a drag.
    ///
    /// Converts the viewport row into a stable grid line and marks dirty for
    /// re-rendering.
    pub fn update_selection(&mut self, row: usize, col: usize) {
        self.selection
            .set_end(self.viewport_row_to_grid_line(row), col);
        self.invalidate_content_cache();
    }

    /// Clear the current selection.
    pub fn clear_selection(&mut self) {
        self.selection.clear();
        self.invalidate_content_cache();
    }

    /// Get the currently selected text, if any.
    ///
    /// Returns `None` if no selection is active. Extracts text from the
    /// terminal grid between the normalized selection start and end.
    pub fn get_selected_text(&self) -> Option<String> {
        let (start, end) = self.selection.normalized()?;
        let text = crate::clipboard::copy_selection(self.terminal.term(), start, end);
        if text.is_empty() {
            None
        } else {
            Some(text)
        }
    }

    /// Mark content as dirty, forcing recomputation on next access.
    pub fn mark_dirty(&mut self) {
        self.invalidate_content_cache();
    }

    /// Check if content is dirty (needs recomputation).
    pub fn is_dirty(&self) -> bool {
        self.content_dirty
    }

    fn ensure_row_caches(&mut self) {
        if !self.content_dirty && self.cached_rows.is_some() {
            return;
        }

        let rows = self.terminal.rows() as usize;
        let cols = self.terminal.cols() as usize;
        let damage = if self.partial_rebuild_allowed
            && self
                .cached_rows
                .as_ref()
                .is_some_and(|cached_rows| cached_rows.len() == rows)
        {
            let term = self.terminal.term_mut();
            let damage = match term.damage() {
                alacritty_terminal::term::TermDamage::Full => None,
                alacritty_terminal::term::TermDamage::Partial(lines) => {
                    Some(lines.map(|line| line.line).collect::<Vec<_>>())
                }
            };
            term.reset_damage();
            damage
        } else {
            self.terminal.term_mut().reset_damage();
            None
        };

        if let Some(dirty_rows) = damage {
            let rebuilt_rows = dirty_rows
                .iter()
                .copied()
                .filter(|row| *row < rows)
                .map(|row| (row, self.build_row_cache(row, cols)))
                .collect::<Vec<_>>();
            if let Some(cached_rows) = self.cached_rows.as_mut() {
                for (row, rebuilt_row) in rebuilt_rows {
                    cached_rows[row] = rebuilt_row;
                }
                self.dirty_rows = Some(dirty_rows);
                self.cached_content = None;
                self.content_dirty = false;
                return;
            }
        }

        self.cached_rows = Some(
            (0..rows)
                .map(|row| self.build_row_cache(row, cols))
                .collect(),
        );
        self.cached_content = None;
        self.cached_shaped_font_family = None;
        self.cached_shaped_font_size = None;
        self.cached_shaped_rows = None;
        self.dirty_rows = None;
        self.content_dirty = false;
        self.partial_rebuild_allowed = false;
    }

    /// Get cached terminal content, flattening row caches only when requested.
    ///
    /// Rendering uses row caches directly; this flattened view is retained for
    /// tests and any future callers that need a contiguous snapshot.
    pub fn cached_content(&mut self) -> &CachedTerminalContent {
        self.ensure_row_caches();
        if self.cached_content.is_none() {
            let rows = self.terminal.rows() as usize;
            let cols = self.terminal.cols() as usize;
            let content = self
                .cached_rows
                .as_ref()
                .map(|cached_rows| Self::flatten_cached_rows(cached_rows, rows, cols))
                .unwrap_or_else(|| CachedTerminalContent {
                    bg_rects_hsla: Arc::default(),
                    text_runs_hsla: Arc::default(),
                    rows,
                    cols,
                });
            self.cached_content = Some(content);
        }
        self.cached_content
            .as_ref()
            .expect("BUG: cached_content must be Some after rebuild")
    }

    /// Get per-row cached terminal content for rendering.
    pub(crate) fn render_rows(&mut self) -> Vec<CachedTerminalRow> {
        self.ensure_row_caches();
        self.cached_rows.clone().unwrap_or_default()
    }

    /// Get per-row shaped text, rebuilding only dirty rows when content changes.
    pub(crate) fn shaped_rows(
        &mut self,
        text_system: &gpui::WindowTextSystem,
    ) -> Vec<ShapedTerminalRow> {
        let font_family = self.font_family.clone();
        let font_size = self.font_size;
        self.ensure_row_caches();

        let font_changed = self.cached_shaped_font_family.as_ref() != Some(&font_family)
            || self
                .cached_shaped_font_size
                .map_or(true, |size| (size - font_size).abs() > 0.01);
        let row_shapes_need_full_rebuild = font_changed
            || self.cached_shaped_rows.is_none()
            || self
                .cached_rows
                .as_ref()
                .zip(self.cached_shaped_rows.as_ref())
                .map_or(true, |(rows, row_shapes)| rows.len() != row_shapes.len());

        if row_shapes_need_full_rebuild {
            let row_shapes = self
                .cached_rows
                .as_ref()
                .map(|rows| {
                    rows.iter()
                        .map(|row| {
                            shape_text_runs(
                                text_system,
                                row.text_runs_hsla.as_ref(),
                                &font_family,
                                font_size,
                            )
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            self.cached_shaped_font_family = Some(font_family);
            self.cached_shaped_font_size = Some(font_size);
            self.cached_shaped_rows = Some(row_shapes);
            self.dirty_rows = None;
        } else if let Some(dirty_rows) = self.dirty_rows.take() {
            if let Some(row_shapes) = self.cached_shaped_rows.as_mut() {
                if let Some(rows) = self.cached_rows.as_ref() {
                    for row in dirty_rows {
                        if row >= rows.len() || row >= row_shapes.len() {
                            continue;
                        }
                        row_shapes[row] = shape_text_runs(
                            text_system,
                            rows[row].text_runs_hsla.as_ref(),
                            &font_family,
                            font_size,
                        );
                    }
                }
            }
        }
        self.cached_shaped_rows.clone().unwrap_or_default()
    }

    fn build_row_cache(&self, row: usize, cols: usize) -> CachedTerminalRow {
        let display_offset = self.terminal.term().grid().display_offset();
        let grid_line = Line(row as i32) - display_offset;
        let grid = self.terminal.term().grid();

        let mut text_runs: Vec<TextRunSegment> = Vec::new();
        let mut background_rects: Vec<(usize, usize, usize, Rgba)> = Vec::new();
        let mut current_run: Option<TextRunSegment> = None;

        for col in 0..cols {
            let cell = &grid[grid_line][Column(col)];
            let c = cell.c;

            if cell.flags.contains(CellFlags::WIDE_CHAR_SPACER) {
                let bg = convert_color(cell.bg, &self.theme);
                if bg != self.theme.terminal_background {
                    let merged = background_rects.last_mut().and_then(
                        |last: &mut (usize, usize, usize, Rgba)| {
                            if last.0 == row && last.2 == col && last.3 == bg {
                                last.2 = col + 1;
                                Some(())
                            } else {
                                None
                            }
                        },
                    );
                    if merged.is_none() {
                        background_rects.push((row, col, col + 1, bg));
                    }
                }
                continue;
            }

            if c == ' ' && cell.bg == TermColor::Named(NamedColor::Background) {
                continue;
            }

            let mut foreground = convert_color(cell.fg, &self.theme);
            let mut background = convert_color(cell.bg, &self.theme);

            if cell.flags.contains(CellFlags::INVERSE) {
                std::mem::swap(&mut foreground, &mut background);
            }
            if self.selection.contains(grid_line.0, col) {
                foreground = self.theme.terminal_selection_fg;
                background = self.theme.terminal_selection_bg;
            }
            if cell.flags.contains(CellFlags::DIM) {
                foreground = dim_color(foreground);
            }

            let same_style = current_run.as_ref().is_some_and(|run| {
                run.row == row
                    && run.foreground == foreground
                    && run.bold == cell.flags.contains(CellFlags::BOLD)
                    && run.italic == cell.flags.contains(CellFlags::ITALIC)
                    && run.underline == cell.flags.contains(CellFlags::UNDERLINE)
                    && run.strikethrough == cell.flags.contains(CellFlags::STRIKEOUT)
                    && run.start_col + run.cell_count == col
            });

            if same_style {
                let run = current_run
                    .as_mut()
                    .expect("BUG: current_run must be Some when same_style is true");
                run.text.push(c);
                run.cell_count += 1;
            } else {
                if let Some(run) = current_run.take() {
                    text_runs.push(run);
                }
                current_run = Some(TextRunSegment {
                    text: String::from(c),
                    foreground,
                    bold: cell.flags.contains(CellFlags::BOLD),
                    italic: cell.flags.contains(CellFlags::ITALIC),
                    underline: cell.flags.contains(CellFlags::UNDERLINE),
                    strikethrough: cell.flags.contains(CellFlags::STRIKEOUT),
                    row,
                    start_col: col,
                    cell_count: 1,
                });
            }

            if background != self.theme.terminal_background {
                let merged = background_rects.last_mut().and_then(
                    |last: &mut (usize, usize, usize, Rgba)| {
                        if last.0 == row && last.2 == col && last.3 == background {
                            last.2 = col + 1;
                            Some(())
                        } else {
                            None
                        }
                    },
                );
                if merged.is_none() {
                    background_rects.push((row, col, col + 1, background));
                }
            }
        }

        if let Some(run) = current_run.take() {
            text_runs.push(run);
        }

        CachedTerminalRow {
            bg_rects_hsla: Arc::new(
                background_rects
                    .into_iter()
                    .map(|(r, start, end, color)| (r, start, end, color.into()))
                    .collect(),
            ),
            text_runs_hsla: Arc::new(
                text_runs
                    .into_iter()
                    .map(|run| {
                        let fg: gpui::Hsla = run.foreground.into();
                        (run, fg)
                    })
                    .collect(),
            ),
        }
    }

    fn flatten_cached_rows(
        cached_rows: &[CachedTerminalRow],
        rows: usize,
        cols: usize,
    ) -> CachedTerminalContent {
        let bg_capacity = cached_rows.iter().map(|row| row.bg_rects_hsla.len()).sum();
        let text_capacity = cached_rows.iter().map(|row| row.text_runs_hsla.len()).sum();

        let mut bg_rects = Vec::with_capacity(bg_capacity);
        let mut text_runs = Vec::with_capacity(text_capacity);
        for row in cached_rows {
            bg_rects.extend(row.bg_rects_hsla.iter().copied());
            text_runs.extend(row.text_runs_hsla.iter().cloned());
        }

        CachedTerminalContent {
            bg_rects_hsla: Arc::new(bg_rects),
            text_runs_hsla: Arc::new(text_runs),
            rows,
            cols,
        }
    }

    /// Convert pixel coordinates to terminal cell position.
    pub fn pixel_to_cell(&self, x: f32, y: f32) -> Option<(usize, usize)> {
        if x < 0.0 || y < 0.0 {
            return None;
        }

        let col = (x / self.cell_width).floor() as usize;
        let row = (y / self.cell_height).floor() as usize;

        let max_row = self.terminal.rows() as usize;
        let max_col = self.terminal.cols() as usize;

        if row < max_row && col < max_col {
            Some((row, col))
        } else {
            None
        }
    }

    /// Convert pixel coordinates to a clamped terminal cell position.
    ///
    /// Unlike `pixel_to_cell`, this never returns `None` — coordinates are
    /// clamped to the viewport bounds. The returned `scroll_dir` indicates
    /// whether the position was above (`-1`), within (`0`), or below (`1`)
    /// the viewport, allowing callers to trigger auto-scroll during selection.
    pub fn pixel_to_cell_clamped(&self, x: f32, y: f32) -> (usize, usize, i32) {
        let max_row = self.terminal.rows() as usize;
        let max_col = self.terminal.cols() as usize;

        if max_row == 0 || max_col == 0 {
            return (0, 0, 0);
        }

        let scroll_dir = if y < 0.0 {
            -1
        } else if y >= (max_row as f32) * self.cell_height {
            1
        } else {
            0
        };

        let row = if y < 0.0 {
            0
        } else {
            let r = (y / self.cell_height).floor() as usize;
            r.min(max_row - 1)
        };

        let col = if x < 0.0 {
            0
        } else {
            let c = (x / self.cell_width).floor() as usize;
            c.min(max_col - 1)
        };

        (row, col, scroll_dir)
    }

    fn mark_output_dirty(&mut self) {
        self.content_dirty = true;
        self.partial_rebuild_allowed = self.cached_rows.is_some();
        self.cached_content = None;
    }

    fn invalidate_content_cache(&mut self) {
        self.content_dirty = true;
        self.partial_rebuild_allowed = false;
        self.cached_content = None;
        self.cached_rows = None;
        self.cached_shaped_font_family = None;
        self.cached_shaped_font_size = None;
        self.cached_shaped_rows = None;
        self.dirty_rows = None;
    }
}

fn shape_text_runs(
    text_system: &gpui::WindowTextSystem,
    text_runs: &[(TextRunSegment, gpui::Hsla)],
    font_family: &str,
    font_size: f32,
) -> Arc<Vec<(usize, usize, gpui::ShapedLine)>> {
    use gpui::{px, Font, FontFeatures, FontStyle, FontWeight, TextRun};

    let font_size_px = px(font_size);
    let font_family: gpui::SharedString = font_family.to_owned().into();
    let mut shaped_runs = Vec::with_capacity(text_runs.len());

    for (run, fg_color) in text_runs.iter() {
        let weight = if run.bold {
            FontWeight::BOLD
        } else {
            FontWeight::NORMAL
        };
        let style = if run.italic {
            FontStyle::Italic
        } else {
            FontStyle::Normal
        };

        let font = Font {
            family: font_family.clone(),
            features: FontFeatures::default(),
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
        let text_run = TextRun {
            len: text.len(),
            font,
            color: *fg_color,
            background_color: None,
            underline,
            strikethrough,
        };

        let shaped = text_system.shape_line(text, font_size_px, &[text_run], None);
        shaped_runs.push((run.row, run.start_col, shaped));
    }

    Arc::new(shaped_runs)
}

/// Compute cell dimensions from actual font metrics using the text system.
///
/// Uses `text_system.advance('m')` to get the actual character width and
/// `ascent + |descent|` to get the line height. GPUI's ascent already includes
/// room for accented characters, so no leading factor is needed.
///
/// Returns `(cell_width, cell_height)` in pixels.
pub fn compute_cell_dimensions(
    text_system: &gpui::TextSystem,
    font_family: &str,
    font_size: f32,
    line_height: f32,
) -> (f32, f32) {
    use gpui::{px, Font, FontFeatures, FontStyle, FontWeight};

    let font = Font {
        family: font_family.to_owned().into(),
        features: FontFeatures::default(),
        fallbacks: None,
        weight: FontWeight::NORMAL,
        style: FontStyle::Normal,
    };

    let font_id = text_system.resolve_font(&font);
    let font_size_px = px(font_size);

    let cell_width = text_system
        .advance(font_id, font_size_px, 'm')
        .map(|adv| f32::from(adv.width))
        .unwrap_or(font_size * FALLBACK_CELL_WIDTH_RATIO);

    // GPUI's ascent already includes room for accented characters, so natural
    // ascent + |descent| gives correct terminal row height without extra leading.
    // (The old 1.3x factor on font_size caused visible double-spacing.)
    let ascent: f32 = text_system.ascent(font_id, font_size_px).into();
    let descent: f32 = text_system.descent(font_id, font_size_px).into();
    let cell_height = (ascent + descent.abs()) * line_height.max(1.0);

    (cell_width, cell_height)
}

/// Cursor rendering information.
#[derive(Debug, Clone, Copy)]
pub struct CursorRect {
    /// X position in pixels.
    pub x: f32,
    /// Y position in pixels.
    pub y: f32,
    /// Width in pixels.
    pub width: f32,
    /// Height in pixels.
    pub height: f32,
    /// Cursor color.
    pub color: Rgba,
    /// Cursor shape.
    pub shape: CursorShape,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal_colors::brighten_color;

    fn create_test_view() -> TerminalView {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let terminal = Terminal::new(24, 80, SessionId(1), tx);
        let theme = CodirigentTheme::dark();
        TerminalView::new(terminal, theme)
    }

    #[test]
    fn test_selection_new() {
        let selection = Selection::default();
        assert!(!selection.is_active());
        assert_eq!(selection.start, None);
        assert_eq!(selection.end, None);
    }

    #[test]
    fn test_selection_set_and_clear() {
        let mut selection = Selection::default();
        selection.set_start(5, 10);
        selection.set_end(10, 20);
        assert!(selection.is_active());
        selection.clear();
        assert!(!selection.is_active());
    }

    #[test]
    fn test_selection_contains() {
        let mut selection = Selection::default();
        selection.set_start(5, 0);
        selection.set_end(10, 80);

        assert!(selection.contains(5, 0));
        assert!(selection.contains(7, 40));
        assert!(selection.contains(10, 80));
        assert!(!selection.contains(4, 0));
        assert!(!selection.contains(11, 0));
    }

    #[test]
    fn test_selection_contains_reversed() {
        let mut selection = Selection::default();
        selection.set_start(10, 80);
        selection.set_end(5, 0);
        assert!(selection.contains(5, 0));
        assert!(selection.contains(7, 40));
    }

    /// Verifies lexicographic row-major ordering: the column at the end of
    /// the start-row IS included, but a column that is AFTER the end-col on
    /// the end-row is NOT included.
    #[test]
    fn test_selection_contains_row_major_boundary() {
        let mut selection = Selection::default();
        // Select from (2, 5) to (4, 3) in forward order
        selection.set_start(2, 5);
        selection.set_end(4, 3);

        // Exact endpoints are included
        assert!(selection.contains(2, 5));
        assert!(selection.contains(4, 3));

        // Middle row is fully included
        assert!(selection.contains(3, 0));
        assert!(selection.contains(3, 79));

        // On start-row, columns BEFORE start-col are NOT selected
        assert!(!selection.contains(2, 4));
        // On end-row, columns AFTER end-col are NOT selected
        assert!(!selection.contains(4, 4));

        // Rows outside the range are not selected
        assert!(!selection.contains(1, 99));
        assert!(!selection.contains(5, 0));

        // Single-cell selection
        let mut sel2 = Selection::default();
        sel2.set_start(3, 7);
        sel2.set_end(3, 7);
        assert!(sel2.contains(3, 7));
        assert!(!sel2.contains(3, 6));
        assert!(!sel2.contains(3, 8));
    }

    #[test]
    fn test_selection_normalized() {
        let mut selection = Selection::default();
        selection.set_start(10, 80);
        selection.set_end(5, 0);

        let normalized = selection.normalized();
        assert!(normalized.is_some());
        let ((sr, sc), (er, ec)) = normalized.unwrap();
        assert_eq!((sr, sc), (5, 0));
        assert_eq!((er, ec), (10, 80));
    }

    #[test]
    fn test_terminal_view_creation() {
        let view = create_test_view();
        assert_eq!(view.terminal().rows(), 24);
        assert_eq!(view.terminal().cols(), 80);
        assert!(view.is_focused());
    }

    #[test]
    fn test_terminal_view_cell_dimensions() {
        let mut view = create_test_view();
        view.set_cell_dimensions(10.0, 20.0);
        assert_eq!(view.cell_width(), 10.0);
        assert_eq!(view.cell_height(), 20.0);
    }

    #[test]
    fn test_terminal_view_font_size() {
        let mut view = create_test_view();
        view.set_font_size(16.0);
        assert_eq!(view.font_size(), 16.0);
    }

    #[test]
    fn test_terminal_view_cursor_shape() {
        let mut view = create_test_view();
        assert_eq!(view.cursor_shape(), CursorShape::Block);
        view.set_cursor_shape(CursorShape::Beam);
        assert_eq!(view.cursor_shape(), CursorShape::Beam);
    }

    #[test]
    fn test_terminal_view_focused() {
        let mut view = create_test_view();
        assert!(view.is_focused());
        view.set_focused(false);
        assert!(!view.is_focused());
    }

    #[test]
    fn test_pixel_size() {
        let view = create_test_view();
        let (width, height) = view.pixel_size();
        assert_eq!(width, view.cell_width() * view.terminal().cols() as f32);
        assert_eq!(height, view.cell_height() * view.terminal().rows() as f32);
    }

    #[test]
    fn test_dimensions_from_pixels() {
        let view = create_test_view();
        let (rows, cols) = view.dimensions_from_pixels(800.0, 600.0);
        assert_eq!(cols, (800.0 / view.cell_width()).floor() as u16);
        assert_eq!(rows, (600.0 / view.cell_height()).floor() as u16);
    }

    #[test]
    fn test_pixel_to_cell() {
        let view = create_test_view();
        assert_eq!(view.pixel_to_cell(40.0, 32.0), Some((2, 5)));
        assert_eq!(view.pixel_to_cell(-1.0, 0.0), None);
        assert_eq!(view.pixel_to_cell(1000.0, 1000.0), None);
    }

    #[test]
    fn test_pixel_to_cell_clamped() {
        let view = create_test_view();
        assert_eq!(view.pixel_to_cell_clamped(-1.0, -1.0), (0, 0, -1));
        assert_eq!(view.pixel_to_cell_clamped(40.0, 32.0), (2, 5, 0));
        assert_eq!(
            view.pixel_to_cell_clamped(10_000.0, 10_000.0),
            (
                view.terminal().rows() as usize - 1,
                view.terminal().cols() as usize - 1,
                1,
            )
        );
    }

    #[test]
    fn test_cursor_rect_unfocused() {
        let mut view = create_test_view();
        view.set_focused(false);
        let cursor = view.cursor_rect();
        assert!(cursor.is_some());
        assert_eq!(cursor.unwrap().shape, CursorShape::HollowBlock);
    }

    #[test]
    fn test_cursor_rect_focused() {
        let view = create_test_view();
        let cursor = view.cursor_rect();
        assert!(cursor.is_some());
        assert_eq!(cursor.unwrap().shape, CursorShape::Block);
    }

    #[test]
    fn test_cached_content_empty() {
        let mut view = create_test_view();
        let content = view.cached_content();
        assert!(
            content.text_runs_hsla.is_empty() && content.bg_rects_hsla.is_empty(),
            "Expected empty terminal to produce no text runs or background rects"
        );
    }

    #[test]
    fn test_cached_content_with_content() {
        let mut view = create_test_view();
        view.terminal_mut().process_output(b"Hello");
        let content = view.cached_content();
        assert!(
            !content.text_runs_hsla.is_empty(),
            "Expected non-empty text runs after writing 'Hello'"
        );
        let has_h = content
            .text_runs_hsla
            .iter()
            .any(|(run, _)| run.text.contains('H'));
        assert!(has_h, "Expected a text run containing 'H'");
    }

    #[test]
    fn test_resize_to_fit() {
        let mut view = create_test_view();
        view.resize_to_fit(400.0, 200.0);
        assert_eq!(
            view.terminal().cols(),
            (400.0 / view.cell_width()).floor() as u16
        );
        assert_eq!(
            view.terminal().rows(),
            (200.0 / view.cell_height()).floor() as u16
        );
    }

    #[test]
    fn test_color_functions() {
        let original = Rgba::rgb(100, 100, 100);
        let dimmed = dim_color(original);
        let brightened = brighten_color(original);
        assert_eq!(dimmed.r, 70);
        assert_eq!(brightened.r, 120);
    }

    #[test]
    fn test_cached_content_consecutive_rows() {
        let mut view = create_test_view();
        // Simulate multi-line output with Windows-style \r\n endings
        // This mimics what ConPTY sends for a simple dir/ls listing
        view.terminal_mut().process_output(
            b"file1.txt\x1b[K\r\nfile2.txt\x1b[K\r\nfile3.txt\x1b[K\r\nfile4.txt\x1b[K\r\n",
        );
        let content = view.cached_content();

        // Collect unique sorted row indices from text runs
        let mut rows: Vec<usize> = content
            .text_runs_hsla
            .iter()
            .map(|(run, _)| run.row)
            .collect();
        rows.sort();
        rows.dedup();

        assert!(
            rows.len() >= 4,
            "Expected at least 4 content rows, got {:?}",
            rows
        );

        // Verify rows are consecutive (no gaps)
        for i in 1..rows.len() {
            assert_eq!(
                rows[i],
                rows[i - 1] + 1,
                "Rows must be consecutive but found gap: {:?}",
                rows
            );
        }
    }

    #[test]
    fn test_start_selection() {
        let mut view = create_test_view();
        view.start_selection(5, 10);
        assert!(view.selection().start.is_some());
        assert_eq!(view.selection().start.unwrap(), (5, 10));
        assert!(view.selection().end.is_none());
    }

    #[test]
    fn test_start_selection_uses_grid_line_when_scrolled_back() {
        let mut view = create_test_view();

        let mut output = String::new();
        for i in 0..40 {
            output.push_str(&format!("row{i:02}\r\n"));
        }
        view.terminal_mut().process_output(output.as_bytes());
        view.scroll_up(3);

        view.start_selection(0, 1);
        assert_eq!(view.selection().start.unwrap(), (-3, 1));
    }

    #[test]
    fn test_update_selection() {
        let mut view = create_test_view();
        view.start_selection(5, 10);
        view.update_selection(10, 20);
        assert!(view.selection().is_active());
        assert_eq!(view.selection().end.unwrap(), (10, 20));
    }

    #[test]
    fn test_clear_selection() {
        let mut view = create_test_view();
        view.start_selection(5, 10);
        view.update_selection(10, 20);
        assert!(view.selection().is_active());
        view.clear_selection();
        assert!(!view.selection().is_active());
    }

    #[test]
    fn test_selection_stays_active_after_update() {
        let mut view = create_test_view();
        view.start_selection(5, 10);
        view.update_selection(10, 20);
        // Selection stays active until cleared — no separate end_selection needed
        assert!(view.selection().is_active());
    }

    #[test]
    fn test_get_selected_text_no_selection() {
        let view = create_test_view();
        assert!(view.get_selected_text().is_none());
    }

    #[test]
    fn test_get_selected_text_with_content() {
        let mut view = create_test_view();
        view.terminal_mut().process_output(b"Hello, World!");
        view.start_selection(0, 0);
        view.update_selection(0, 4);
        let text = view.get_selected_text();
        assert!(text.is_some());
        assert_eq!(text.unwrap(), "Hello");
    }

    #[test]
    fn test_get_selected_text_empty_region() {
        let view = create_test_view();
        // No content rendered, so even with selection, text should be empty/None
        let mut view = view;
        view.start_selection(0, 0);
        view.update_selection(0, 0);
        // Single cell with no content = None (empty string filtered)
        let text = view.get_selected_text();
        assert!(text.is_none());
    }

    #[test]
    fn test_get_selected_text_remains_bound_to_buffer_while_scrolling() {
        let mut view = create_test_view();

        let mut output = String::new();
        for i in 0..40 {
            output.push_str(&format!("row{i:02}\r\n"));
        }
        view.terminal_mut().process_output(output.as_bytes());
        view.scroll_up(5);

        view.start_selection(0, 0);
        view.update_selection(0, 4);
        let before = view.get_selected_text();
        assert_eq!(before.as_deref(), Some("row12"));

        view.scroll_up(2);
        let after_scroll_up = view.get_selected_text();
        assert_eq!(after_scroll_up, before);

        view.scroll_down(1);
        let after_scroll_down = view.get_selected_text();
        assert_eq!(after_scroll_down, before);
    }

    #[test]
    fn test_cached_content_row_indices_with_ansi() {
        let mut view = create_test_view();
        // More realistic ConPTY output with ANSI sequences
        view.terminal_mut().process_output(
            b"\x1b[?25l\x1b[2J\x1b[H\
            Row0 text here\x1b[K\r\n\
            Row1 text here\x1b[K\r\n\
            Row2 text here\x1b[K\r\n\
            Row3 text here\x1b[K\r\n\
            Row4 text here\x1b[K\r\n\
            \x1b[?25h",
        );
        let content = view.cached_content();

        let mut rows: Vec<usize> = content
            .text_runs_hsla
            .iter()
            .map(|(run, _)| run.row)
            .collect();
        rows.sort();
        rows.dedup();

        assert!(
            rows.len() >= 5,
            "Expected at least 5 content rows, got {:?}",
            rows
        );

        for i in 1..rows.len() {
            assert_eq!(
                rows[i],
                rows[i - 1] + 1,
                "Rows must be consecutive with ANSI output: {:?}",
                rows
            );
        }
    }

    #[test]
    fn test_cached_content_wide_char_background_rect() {
        let mut view = create_test_view();
        // '中' (U+4E2D) is a CJK character that occupies 2 columns.
        // \x1b[41m sets red background; \x1b[0m resets.
        view.terminal_mut()
            .process_output("\x1b[41m中\x1b[0m".as_bytes());
        let content = view.cached_content();
        // The background rect must span both columns of the wide char (start=0, end=2).
        let has_two_col_rect = content
            .bg_rects_hsla
            .iter()
            .any(|(_, start, end, _)| *end - *start >= 2);
        assert!(
            has_two_col_rect,
            "Expected a 2-column-wide background rect for wide char '中', got: {:?}",
            content
                .bg_rects_hsla
                .iter()
                .map(|(r, s, e, _)| (r, s, e))
                .collect::<Vec<_>>()
        );
    }
}
