use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Boundary, Column, Direction, Line, Point, Side};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::search::{Match, RegexSearch};
use alacritty_terminal::term::Term;

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

/// Search the terminal grid for literal, case-insensitive matches.
pub fn search<T>(term: &Term<T>, query: &str) -> Vec<SearchMatch> {
    if query.is_empty() {
        return Vec::new();
    }

    let mut regex = match RegexSearch::new(&literal_case_insensitive_pattern(query)) {
        Ok(regex) => regex,
        Err(_) => return Vec::new(),
    };

    let mut matches = Vec::new();
    let mut origin = Point::new(term.bottommost_line(), term.last_column());
    let top_left = Point::new(term.topmost_line(), Column(0));

    while let Some(regex_match) =
        term.search_next(&mut regex, origin, Direction::Left, Side::Right, None)
    {
        if let Some(search_match) = from_regex_match(term, &regex_match) {
            let start = search_match.start_point();
            matches.push(search_match);

            if start <= top_left {
                break;
            }

            origin = start.sub(term, Boundary::Grid, 1);
        } else {
            break;
        }
    }

    matches
}

/// Verify that a stale search hit still contains the current query.
pub fn match_still_matches<T>(term: &Term<T>, query: &str, search_match: &SearchMatch) -> bool {
    if query.is_empty() {
        return false;
    }

    extract_match_text(term, search_match).to_lowercase() == query.to_lowercase()
}

fn from_regex_match<T>(term: &Term<T>, regex_match: &Match) -> Option<SearchMatch> {
    let start = *regex_match.start();
    let inclusive_end = *regex_match.end();
    let (end_grid_line, end_col) = exclusive_end(term, inclusive_end)?;

    Some(SearchMatch {
        grid_line: start.line.0,
        start_col: start.column.0,
        end_grid_line,
        end_col,
    })
}

fn extract_match_text<T>(term: &Term<T>, search_match: &SearchMatch) -> String {
    let mut point = search_match.start_point();
    let end = search_match.end_point();
    let mut text = String::new();

    while point < end {
        let cell = &term.grid()[point.line][point.column];
        if !cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
            text.push(cell.c);
        }

        let step = cell_width(cell.flags);
        point = advance_point(term, point, step);
    }

    text
}

fn exclusive_end<T>(term: &Term<T>, inclusive_end: Point) -> Option<(i32, usize)> {
    let cell = &term.grid()[inclusive_end.line][inclusive_end.column];
    let width = cell_width(cell.flags);
    let cols = term.columns();
    if cols == 0 {
        return None;
    }

    let mut line = inclusive_end.line.0;
    let mut col = inclusive_end.column.0 + width;

    while col > cols {
        line += 1;
        col -= cols;
    }

    if col == cols && line < term.bottommost_line().0 {
        line += 1;
        col = 0;
    }

    Some((line, col))
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

fn literal_case_insensitive_pattern(query: &str) -> String {
    format!("(?i:{})", escape_regex_literal(query))
}

fn escape_regex_literal(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\\' | '.' | '+' | '*' | '?' | '(' | ')' | '|' | '[' | ']' | '{' | '}' | '^' | '$'
            | '#' | '&' | '-' | '~' => {
                escaped.push('\\');
                escaped.push(ch);
            }
            _ => escaped.push(ch),
        }
    }
    escaped
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
}
