//! Commands marked where they ran: each command run at a prompt marks the
//! prompt's line, so the scrollbar shows where each is in the scrollback (a
//! failed one in the error color, as VS Code marks them), and Previous and
//! Next Command (⌘↑ and ⌘↓, Ctrl+Shift+↑ and ↓ elsewhere) go from one to
//! the next, as Terminal.app's do.
//!
//! alacritty_terminal keeps nothing for a line but its cells, so the mark
//! lives in one, as a double line's size does (`crate::line_size`): an
//! underline color on the first cell of the prompt's first line, which
//! nothing draws (the prompt's first letter isn't underlined). It's put
//! there as the command starts (Enter at the prompt, the assistant's run,
//! and again at zsh's 133;C, once a transient prompt has drawn the line
//! again) and changed to done or failed by its exit (133;D). A line's
//! first cell is where its text starts, so the mark scrolls with it into
//! the scrollback and off its top, rides a rewrap, which never cuts a
//! line's first cell, and stays in the scrollback through a clear. A
//! program that writes over the prompt's line later takes it away, as
//! ConPTY's repaint after a resize does on Windows for the commands still
//! on the screen.

use alacritty_terminal::grid::{Dimensions, Grid, Row};
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::Cell;
use alacritty_terminal::vte::ansi::{Color, Rgb};

/// What a command's mark says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    Running,
    Done,
    /// It exited with a code other than 0.
    Failed,
}

impl Mark {
    /// Its color in the cell, apart from `line_size`'s marks.
    fn color(self) -> Color {
        let n = match self {
            Mark::Running => 1,
            Mark::Done => 2,
            Mark::Failed => 3,
        };
        Color::Spec(Rgb { r: 0xF1, g: 0x0C, b: 0xC0 + n })
    }

    fn of(color: Color) -> Option<Mark> {
        [Mark::Running, Mark::Done, Mark::Failed].into_iter().find(|m| m.color() == color)
    }

    /// The mark of a command that exited with `code`: failed for any but
    /// 0, done where the shell can't say (cmd).
    pub fn ended(code: Option<i32>) -> Mark {
        if code.is_some_and(|c| c != 0) { Mark::Failed } else { Mark::Done }
    }
}

/// The mark on `row`, on its first cell.
pub fn of_row(row: &Row<Cell>) -> Option<Mark> {
    (row.len() > 0).then(|| row[Column(0)].underline_color()).flatten().and_then(Mark::of)
}

/// The grid's line `abs` lines from the scrollback's top, while it's
/// there.
fn line_of(grid: &Grid<Cell>, abs: i64) -> Option<Line> {
    let line = abs - grid.history_size() as i64;
    (abs >= 0 && line < grid.screen_lines() as i64).then_some(Line(line as i32))
}

/// Marks line `abs` (from the scrollback's top), where a prompt began, as
/// its command running.
pub fn start(grid: &mut Grid<Cell>, abs: i64) {
    if let Some(line) = line_of(grid, abs).filter(|_| grid.columns() > 0) {
        grid[line][Column(0)].set_underline_color(Some(Mark::Running.color()));
    }
}

/// The command running nearest above line `abs` (the cursor's, from the
/// scrollback's top) ended with `code`: its mark says done or failed. A
/// mark that's ended already, met first, stops the look, so an older one
/// left running (the shell gone) isn't taken for it.
pub fn finish(grid: &mut Grid<Cell>, abs: i64, code: Option<i32>) {
    let top = -(grid.history_size() as i64);
    let mut line = (abs - grid.history_size() as i64).min(grid.screen_lines() as i64 - 1);
    while line >= top {
        match of_row(&grid[Line(line as i32)]) {
            Some(Mark::Running) => {
                grid[Line(line as i32)][Column(0)].set_underline_color(Some(Mark::ended(code).color()));
                return;
            }
            Some(_) => return,
            None => line -= 1,
        }
    }
}

/// Every mark from the scrollback's top down: its line (from the top) and
/// what it says. A glance at each line's first cell.
pub fn all(grid: &Grid<Cell>) -> Vec<(usize, Mark)> {
    let history = grid.history_size();
    (0..history + grid.screen_lines()).filter_map(|abs| of_row(&grid[Line(abs as i32 - history as i32)]).map(|m| (abs, m))).collect()
}

/// The mark Previous Command (`up`) or Next Command goes to from line
/// `from` (from the scrollback's top): the nearest before it, or after it.
pub fn beside(marks: &[(usize, Mark)], from: usize, up: bool) -> Option<usize> {
    if up { marks.iter().rev().map(|m| m.0).find(|&l| l < from) } else { marks.iter().map(|m| m.0).find(|&l| l > from) }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A grid of `rows` × 10 keeping `history` lines, its rows saying their
    /// numbers.
    fn grid(rows: usize, history: usize) -> Grid<Cell> {
        let mut g: Grid<Cell> = Grid::new(rows, 10, history);
        for r in 0..rows {
            g[Line(r as i32)][Column(0)].c = char::from(b'0' + r as u8);
        }
        g
    }

    /// Scrolls `n` lines into the scrollback, as output at the foot does.
    fn scroll(g: &mut Grid<Cell>, n: usize) {
        let rows = g.screen_lines() as i32;
        g.scroll_up(&(Line(0)..Line(rows)), n);
    }

    #[test]
    fn a_mark_goes_on_the_prompts_first_cell_and_ends_done_or_failed() {
        let mut g = grid(5, 20);
        start(&mut g, 1);
        assert_eq!(all(&g), [(1, Mark::Running)]);
        assert_eq!(g[Line(1)][Column(0)].c, '1', "its letter as it was");
        finish(&mut g, 3, Some(0));
        assert_eq!(all(&g), [(1, Mark::Done)]);
        start(&mut g, 3);
        finish(&mut g, 4, Some(2));
        assert_eq!(all(&g), [(1, Mark::Done), (3, Mark::Failed)]);
        // cmd says no code: done.
        assert_eq!(Mark::ended(None), Mark::Done);
    }

    #[test]
    fn a_mark_scrolls_with_its_line_into_the_scrollback_and_off_its_top() {
        let mut g = grid(4, 3);
        start(&mut g, 2);
        scroll(&mut g, 2);
        // Two lines up the scrollback, still line 2 from the top.
        assert_eq!((g.history_size(), all(&g)), (2, vec![(2, Mark::Running)]));
        // The command ends below it, scrolled further: it's found above.
        finish(&mut g, 5, Some(1));
        assert_eq!(all(&g), [(2, Mark::Failed)]);
        // The scrollback full (3 lines), each line in drops the oldest, and
        // the mark keeps to its line, counted from the new top.
        scroll(&mut g, 2);
        assert_eq!((g.history_size(), all(&g)), (3, vec![(1, Mark::Failed)]));
        scroll(&mut g, 2);
        assert!(all(&g).is_empty(), "gone off the top with its line");
    }

    #[test]
    fn an_ended_mark_stops_the_look_for_the_one_running() {
        let mut g = grid(6, 0);
        start(&mut g, 0);
        start(&mut g, 2);
        finish(&mut g, 3, Some(0));
        // Nothing running below the done one: the old one stays running.
        finish(&mut g, 5, Some(1));
        assert_eq!(all(&g), [(0, Mark::Running), (2, Mark::Done)]);
    }

    #[test]
    fn previous_and_next_go_to_the_nearest_mark_each_way() {
        let marks = [(3, Mark::Done), (10, Mark::Failed), (20, Mark::Running)];
        assert_eq!(beside(&marks, 15, true), Some(10));
        assert_eq!(beside(&marks, 10, true), Some(3));
        assert_eq!(beside(&marks, 3, true), None);
        assert_eq!(beside(&marks, 10, false), Some(20));
        assert_eq!(beside(&marks, 20, false), None);
        assert_eq!(beside(&[], 5, true), None);
    }
}
