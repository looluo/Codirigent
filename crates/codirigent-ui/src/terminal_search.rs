use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::Term;

const WIDE_SPACER_FLAGS: Flags = Flags::WIDE_CHAR_SPACER.union(Flags::LEADING_WIDE_CHAR_SPACER);

/// Literal search hit inside terminal grid coordinates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchMatch {
    /// Starting grid line for the match.
    pub grid_line: i32,
    /// Starting column (inclusive) on `grid_line`.
    pub start_col: usize,
    /// Ending grid line for the match.
    pub end_grid_line: i32,
    /// Ending column (exclusive) on `end_grid_line`.
    pub end_col: usize,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct SearchSnapshot {
    blocks: Vec<SearchBlock>,
}

#[derive(Debug, Clone, Default)]
struct SearchBlock {
    cells: Vec<SearchCell>,
}

#[derive(Debug, Clone, Copy)]
struct SearchCell {
    ch: char,
    grid_line: i32,
    start_col: usize,
    end_grid_line: i32,
    end_col: usize,
}

impl SearchMatch {
    /// Starting point of the match.
    pub fn start_point(&self) -> Point {
        Point::new(Line(self.grid_line), Column(self.start_col))
    }

    /// Exclusive end point of the match.
    pub fn end_point(&self) -> Point {
        Point::new(Line(self.end_grid_line), Column(self.end_col))
    }
}

pub(crate) fn snapshot<T>(term: &Term<T>) -> SearchSnapshot {
    let cols = term.columns();
    if cols == 0 {
        return SearchSnapshot::default();
    }

    let mut blocks = Vec::new();
    let mut cells = Vec::new();
    let last_column = Column(cols - 1);
    let top = term.topmost_line().0;
    let bottom = term.bottommost_line().0;

    for grid_line in top..=bottom {
        let line = Line(grid_line);
        for col in 0..cols {
            let cell = &term.grid()[line][Column(col)];
            if cell.flags.intersects(WIDE_SPACER_FLAGS) {
                continue;
            }

            let width = cell_width(cell.flags);
            let (end_grid_line, end_col) = exclusive_end_from_cell(grid_line, col, width, cols);
            cells.push(SearchCell {
                ch: cell.c,
                grid_line,
                start_col: col,
                end_grid_line,
                end_col,
            });
        }

        if !term.grid()[line][last_column]
            .flags
            .contains(Flags::WRAPLINE)
            && !cells.is_empty()
        {
            blocks.push(SearchBlock {
                cells: std::mem::take(&mut cells),
            });
        }
    }

    if !cells.is_empty() {
        blocks.push(SearchBlock { cells });
    }

    SearchSnapshot { blocks }
}

/// Search the terminal grid for literal, case-insensitive matches.
#[cfg(test)]
pub fn search<T>(term: &Term<T>, query: &str) -> Vec<SearchMatch> {
    search_snapshot(&snapshot(term), query)
}

pub(crate) fn search_snapshot(snapshot: &SearchSnapshot, query: &str) -> Vec<SearchMatch> {
    if query.is_empty() {
        return Vec::new();
    }

    let query_chars: Vec<char> = query.chars().flat_map(|ch| ch.to_lowercase()).collect();
    if query_chars.is_empty() {
        return Vec::new();
    }

    let mut matches = Vec::new();
    for block in &snapshot.blocks {
        matches.extend(search_block(block, &query_chars));
    }

    matches.reverse();
    matches
}

/// Verify that a stale search hit still contains the current query.
pub fn match_still_matches<T>(term: &Term<T>, query: &str, search_match: &SearchMatch) -> bool {
    if query.is_empty() {
        return false;
    }

    extract_match_text(term, search_match).to_lowercase() == query.to_lowercase()
}

fn extract_match_text<T>(term: &Term<T>, search_match: &SearchMatch) -> String {
    let mut point = search_match.start_point();
    let end = search_match.end_point();
    let mut text = String::new();

    while point < end {
        let cell = &term.grid()[point.line][point.column];
        if !cell.flags.intersects(WIDE_SPACER_FLAGS) {
            text.push(cell.c);
        }

        let step = cell_width(cell.flags);
        point = advance_point(term, point, step);
    }

    text
}

fn search_block(block: &SearchBlock, query_chars: &[char]) -> Vec<SearchMatch> {
    if block.cells.len() < query_chars.len() {
        return Vec::new();
    }

    let mut lowered = Vec::new();
    let mut lowered_to_cell = Vec::new();
    for (index, cell) in block.cells.iter().enumerate() {
        for ch in cell.ch.to_lowercase() {
            lowered.push(ch);
            lowered_to_cell.push(index);
        }
    }

    if lowered.len() < query_chars.len() {
        return Vec::new();
    }

    let mut matches = Vec::new();
    for start in 0..=lowered.len() - query_chars.len() {
        if lowered[start..start + query_chars.len()] != *query_chars {
            continue;
        }

        let start_cell = block.cells[lowered_to_cell[start]];
        let end_cell = block.cells[lowered_to_cell[start + query_chars.len() - 1]];
        matches.push(SearchMatch {
            grid_line: start_cell.grid_line,
            start_col: start_cell.start_col,
            end_grid_line: end_cell.end_grid_line,
            end_col: end_cell.end_col,
        });
    }

    matches
}

fn exclusive_end_from_cell(
    grid_line: i32,
    start_col: usize,
    width: usize,
    cols: usize,
) -> (i32, usize) {
    let mut line = grid_line;
    let mut col = start_col + width;

    while col > cols {
        line += 1;
        col -= cols;
    }

    if col == cols {
        line += 1;
        col = 0;
    }

    (line, col)
}

fn advance_point<T>(term: &Term<T>, point: Point, step: usize) -> Point {
    let cols = term.columns();
    if cols == 0 {
        return point;
    }

    let mut line = point.line.0;
    let mut col = point.column.0 + step;

    while col >= cols {
        col -= cols;
        line += 1;
    }

    Point::new(Line(line), Column(col))
}

fn cell_width(flags: Flags) -> usize {
    if flags.contains(Flags::WIDE_CHAR) {
        2
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::Terminal;
    use codirigent_core::SessionId;

    fn create_term(rows: u16, cols: u16) -> Terminal {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        Terminal::new(rows, cols, SessionId(1), tx)
    }

    #[test]
    fn finds_case_insensitive_matches() {
        let mut terminal = create_term(4, 20);
        terminal.process_output(b"Alpha\nbeta\nALPHA\n");

        let matches = search(terminal.term(), "alpha");

        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].grid_line, 2);
        assert_eq!(matches[1].grid_line, 0);
    }

    #[test]
    fn validates_stale_matches_against_current_grid_text() {
        let mut terminal = create_term(4, 20);
        terminal.process_output(b"hello world");

        let search_match = search(terminal.term(), "hello").pop().unwrap();
        assert!(match_still_matches(terminal.term(), "hello", &search_match));
        assert!(!match_still_matches(
            terminal.term(),
            "world",
            &search_match
        ));
    }

    #[test]
    fn does_not_match_across_hard_line_breaks() {
        let mut terminal = create_term(4, 20);
        terminal.process_output(b"ab\ncd\n");

        let matches = search(terminal.term(), "bc");

        assert!(matches.is_empty());
    }

    #[test]
    fn matches_across_wrapped_lines() {
        let mut terminal = create_term(4, 4);
        terminal.process_output(b"abcd");

        let matches = search(terminal.term(), "cd");

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].grid_line, 0);
        assert_eq!(matches[0].start_col, 2);
        assert_eq!(matches[0].end_grid_line, 1);
        assert_eq!(matches[0].end_col, 0);
    }
}
