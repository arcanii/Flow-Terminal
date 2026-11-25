//! DEC's line sizes: a double-width line (DECDWL, `ESC # 6`), the top and
//! bottom halves of a double-height one (DECDHL, `ESC # 3` and `ESC # 4`),
//! and single again (DECSWL, `ESC # 5`), as teletext's double height is
//! written in a terminal.
//!
//! alacritty_terminal reads none of them and keeps nothing for a line but
//! its cells, so the size lives in the line itself, as images live in
//! placeholder cells: a mark in its last cell's underline color. A double
//! line shows only the first half of its columns, and its right margin is
//! there ([`Margins`]), so its own text never reaches that cell; nothing
//! draws an underline color on a blank cell, and the emulator counts such a
//! cell as blank, so the line's text and its length are as they were. The
//! mark scrolls, clears and goes into the scrollback with the line. A line
//! cleared whole (`clear`, or erased from its start down) is single again,
//! as on a DEC terminal, while an erase within it keeps its size, as a DEC
//! terminal's does. A single line made double loses what's past its half
//! ([`make`]), as on a DEC terminal.
//!
//! Text that wraps at a double line's half marks the line wrapped on that
//! same last cell (`WRAPLINE`), where the emulator marks a single line's
//! wrap, so selecting goes on across it; what reads the text (copying,
//! [`selection_to_string`], and the session's `text_between` and
//! `input_from`) reads a double line only as far as its half, and one that
//! wrapped there runs on into the next line's text, as a single line that
//! wrapped does. Its text flows from then on ([`flowing`]), which its mark
//! says too.
//!
//! A resize rewraps the main screen's lines, cutting cells off a narrowed
//! line and adding them to a widened one, so the marks ride through it on
//! the lines' first cells ([`lift`], [`settle`]), on both screens. The
//! rewrap counts cells by the screen's width, so a double line whose text
//! flows is laid out for it first: it and what wrapped from it rewrap as one
//! line whose first row holds as many letters as the double line shows at
//! the new width, its letters past that going on ahead of the next line's,
//! or the next line's coming back into it. A double line whose text never
//! flowed was laid out by its program (a title, the two halves of a double
//! height), so it's cut rather than given a line of its own, as the
//! alternate screen's lines are, and shows what it hid again when widened,
//! as in xterm; and a single line that wrapped onto a double one rewraps on
//! its own and goes on into it again after.
//!
//! A picture's cells on a double line are drawn doubled as its text is
//! (`ui::draw_terminal`), so a picture spanning rows of different sizes
//! doesn't line up across them; a program shows one double height as it
//! writes text so, the same cells on both halves.

use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::{Dimensions, Grid, GridCell, Row};
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::selection::{SelectionRange, SelectionType};
use alacritty_terminal::term::cell::{Cell, Flags, Hyperlink};
use alacritty_terminal::term::{Term, TermMode};
use alacritty_terminal::vte::ansi::cursor_icon::CursorIcon;
use alacritty_terminal::vte::ansi::{self, Attr, CharsetIndex, ClearMode, Color, CursorShape, CursorStyle, Handler, KeyboardModes, KeyboardModesApplyBehavior, LineClearMode, Mode, ModifyOtherKeys, NamedMode, PrivateMode, Rgb, ScpCharPath, ScpUpdateMode, StandardCharset, TabulationClearMode};
use unicode_width::UnicodeWidthChar;

/// How a line is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineSize {
    #[default]
    Single,
    /// Twice as wide.
    Wide,
    /// Twice as wide and the top half of twice as tall.
    Top,
    /// The bottom half of the same.
    Bottom,
}

impl LineSize {
    /// Drawn twice as wide (and maybe as tall).
    pub fn doubled(self) -> bool {
        self != LineSize::Single
    }

    /// How many of `cols` columns the line shows: half of them on a double
    /// line.
    pub fn shown(self, cols: usize) -> usize {
        if self.doubled() { (cols / 2).max(1) } else { cols }
    }

    /// Its mark's color, and another for a line whose text flows (see
    /// [`flowing`]).
    fn mark(self, flowing: bool) -> Option<Color> {
        let n = match self {
            LineSize::Single => return None,
            LineSize::Wide => 1,
            LineSize::Top => 2,
            LineSize::Bottom => 3,
        };
        Some(Color::Spec(Rgb { r: 0xF1, g: 0x0D + flowing as u8, b: 0xE0 + n }))
    }

    fn from_mark(color: Color) -> Option<(LineSize, bool)> {
        let sizes = [LineSize::Wide, LineSize::Top, LineSize::Bottom];
        sizes.into_iter().flat_map(|s| [(s, false), (s, true)]).find(|&(s, flowing)| s.mark(flowing) == Some(color))
    }
}

/// The mark on `row`'s last cell: its size, and whether its text flows.
fn marked(row: &Row<Cell>) -> Option<(LineSize, bool)> {
    let last = row.len().checked_sub(1)?;
    row[Column(last)].underline_color().and_then(LineSize::from_mark)
}

/// The size `row` is marked with, on its last cell. [`Margins`] keeps the
/// mark there through the edits that move a line's cells along it, so this
/// is a glance at one cell, made for each letter a program writes.
pub fn of_row(row: &Row<Cell>) -> LineSize {
    marked(row).map_or(LineSize::Single, |(size, _)| size)
}

/// Whether `row` is a double line whose text flows: text wrapped at its
/// half since it was made double, or it rewrapped with what wrapped from it
/// (see [`lift`]). Such a line rewraps when it's narrowed even if it no
/// longer wraps (a widening took in all that had wrapped from it), where a
/// line a program laid out, which never wrapped, is cut.
pub fn flowing(row: &Row<Cell>) -> bool {
    marked(row).is_some_and(|(_, flowing)| flowing)
}

/// The size of the line on screen row `row` as shown, scrolled back or not.
pub fn on_screen(grid: &Grid<Cell>, row: usize) -> LineSize {
    if row >= grid.screen_lines() {
        return LineSize::Single;
    }
    of_row(&grid[Line(row as i32 - grid.display_offset() as i32)])
}

/// Marks `row` with `size`, in its last cell; single takes the mark away,
/// and so does any other size from wherever it is. Its text doesn't flow
/// (see [`flowing`]) until it wraps.
pub fn set_row(row: &mut Row<Cell>, size: LineSize) {
    mark_row(row, size, false);
}

/// Marks `row` with `size`, its text flowing or not.
fn mark_row(row: &mut Row<Cell>, size: LineSize, flowing: bool) {
    let len = row.len();
    for c in 0..len {
        if row[Column(c)].underline_color().and_then(LineSize::from_mark).is_some() {
            row[Column(c)].set_underline_color(None);
        }
    }
    if let (Some(mark), Some(last)) = (size.mark(flowing), len.checked_sub(1)) {
        row[Column(last)].set_underline_color(Some(mark));
    }
}

/// Makes `row` `size`, as `ESC # 3` to `ESC # 6` make the cursor's line.
/// A single line made double loses the characters past its half (and a
/// wide one across it), as on DEC's terminals, so made single again it's
/// blank there; xterm only hides them, and shows them again then. A double
/// line made single no longer wraps at its half, and its padding (see
/// [`settle`]) is plain blanks again.
pub fn make(row: &mut Row<Cell>, size: LineSize) {
    let (was, len) = (of_row(row), row.len());
    if size.doubled() && !was.doubled() {
        let half = size.shown(len);
        let from = if row[Column(half - 1)].flags.contains(Flags::WIDE_CHAR) { half - 1 } else { half };
        for c in from..len {
            row[Column(c)] = Cell::default();
        }
    } else if was.doubled() && !size.doubled() {
        for c in 0..len {
            row[Column(c)].flags.remove(Flags::WRAPLINE | Flags::LEADING_WIDE_CHAR_SPACER);
        }
    }
    set_row(row, size);
}

/// Whether `row` goes on into the next one: the emulator marks it on the
/// last cell, a double line's as a single line's.
pub fn wraps(row: &Row<Cell>) -> bool {
    row.last().is_some_and(|cell| cell.flags.contains(Flags::WRAPLINE))
}

/// The selection's text, as the emulator's `Term::selection_to_string`
/// reads it, but with each double line read only as far as its half, where
/// its text ends and the cells past it are hidden; one that wrapped there
/// runs on into the next line's text, as a single line that wrapped does.
/// The emulator reads a wrapped line to the screen's edge, which on a double
/// line would put the hidden half's blanks in the middle of the text. A
/// block selection, which Flow doesn't make, is the emulator's own.
pub fn selection_to_string<L>(term: &Term<L>) -> Option<String> {
    let selection = term.selection.as_ref()?;
    let SelectionRange { start, end, .. } = selection.to_range(term)?;
    let grid = term.grid();
    let doubled = |line: Line| of_row(&grid[line]).doubled();
    if selection.ty == SelectionType::Block || !(start.line.0..=end.line.0).any(|l| doubled(Line(l))) {
        return term.selection_to_string();
    }
    let last = term.last_column();
    let half = Column(LineSize::Wide.shown(term.columns()) - 1);
    let mut text = String::new();
    let mut line = start.line;
    loop {
        let from = if line == start.line { start.column } else { Column(0) };
        if doubled(line) {
            let to = if line == end.line { end.column.min(half) } else { half };
            if from <= to {
                text += &term.bounds_to_string(Point::new(line, from), Point::new(line, to));
            }
        } else {
            // Single lines up to the next double one, read as the emulator
            // reads them, to the last column, short of a wide letter's
            // spacer there, which it reads as the letter only at the
            // selection's end.
            let mut run = line;
            while run < end.line && !doubled(Line(run.0 + 1)) {
                run = Line(run.0 + 1);
            }
            let spacer = grid[run][last].flags.contains(Flags::LEADING_WIDE_CHAR_SPACER);
            let to = if run == end.line { end.column } else if spacer { Column(last.0.saturating_sub(1)) } else { last };
            text += &term.bounds_to_string(Point::new(line, from), Point::new(run, to));
            line = run;
        }
        if line == end.line {
            break;
        }
        if !wraps(&grid[line]) {
            text.push('\n');
        }
        line = Line(line.0 + 1);
    }
    if selection.ty == SelectionType::Lines {
        text.push('\n');
    }
    Some(text)
}

/// What a lifted mark's first cell is linked to while a resize runs: which
/// of [`Lifted`]'s lines it is.
const LIFTED: &str = "flow:line-size;";

/// What a blank ending the text of a line that goes on into a double one
/// stands as while the line is rewrapped on its own (see [`lift`]), since
/// the rewrap drops blanks at the end of a line and these are the text's
/// own: a noncharacter, which no program writes.
const END: char = '\u{FDD0}';

/// A double line whose mark [`lift`] took off.
struct Lift {
    size: LineSize,
    /// Its text flowed ([`flowing`]).
    flowing: bool,
    /// Its first cell's own link, which the lifted mark stands in for.
    link: Option<Hyperlink>,
    /// It wrapped at its half and was kept out of the rewrap, to wrap there
    /// again after.
    wrapped: bool,
    /// The line before it wrapped onto it (a single line, or a double one
    /// that's narrowed), to go on into it again after from the single line
    /// it rewraps into; true if that line's text ended in a blank, kept as
    /// [`END`].
    joined: Option<bool>,
    /// How it rewraps, if it does.
    flow: Option<Flow>,
}

/// How a double line whose text flows is laid out for the emulator's
/// rewrap, which counts cells by the screen's width: as the first row of
/// one line with what wrapped from it, holding as many cells as the screen
/// is wide once rewrapped, of which only the double line's own are kept.
#[derive(Clone, Copy)]
enum Flow {
    /// Narrower: it keeps its first `kept` columns (and padding after them
    /// where a wide letter no longer fits), then blanks to the new width,
    /// and the rest of its letters go on ahead of the next line's, or onto
    /// a line of their own if it didn't wrap.
    Narrower { kept: usize },
    /// Wider: its `len` columns, then blanks as far as `from`, past which
    /// the rewrap brings back what it now has room for from the next line.
    Wider { len: usize, from: usize },
}

/// Marks [`lift`] took off a grid's lines, and how they go back.
pub struct Lifted {
    lines: Vec<Lift>,
    /// The columns a double line showed before.
    half: usize,
    /// The double line the cursor was on (which of `lines`), and where its
    /// next letter would have gone along it: past its last column with a
    /// wrap due.
    cursor: Option<(usize, usize)>,
}

/// Whether line `line` of `grid` is a double line that rewraps when the
/// grid narrows ([`lift`]): it wrapped at its half, or its text flows
/// ([`flowing`]). A picture on it is cut at its new half, not the new
/// width, since past that its cells go on to the next line.
pub fn flows(grid: &Grid<Cell>, line: Line) -> bool {
    let row = &grid[line];
    of_row(row).doubled() && (wraps(row) || flowing(row))
}

/// Before the grid is resized to `cols` columns: takes each line's mark off
/// its last cell, which a narrowing would cut off (or wrap onto a line of
/// its own), and keeps it on the line's first cell, which stays first, as a
/// link (the cell's own link kept aside) for [`settle`] to put back once the
/// grid is resized. `reflow` says whether the grid is rewrapped (the main
/// screen) or cut (the alternate one).
///
/// A double line that wrapped at its half rewraps with what wrapped from it
/// as one line by the double line's width (on the main screen; it's cut
/// with the rest of the alternate one): it keeps its wrap, and is laid out
/// for the rewrap, which counts cells by the screen's width, as a row that
/// takes as many cells as the screen is wide. Narrower, its letters past its
/// new half (and a wide letter across it) go after that many, and so on
/// ahead of the next line's text; wider, it's short of the screen's width by
/// the letters it now has room for, which the rewrap takes back into it from
/// the next line, and a line left empty by that goes. One whose text flows
/// ([`flowing`]) but which no longer wraps, as when a widening took in all
/// that had wrapped from it, gives what a narrowing leaves no room for a
/// line of its own, as a single line does; and so does one that wrapped
/// onto another double line, which takes in nothing from it when it
/// widens, that line of its own going on into the other again once
/// settled. Which line was double and what it kept ride on its first
/// cell's link.
///
/// Any other double line is kept out of the rewrap. A double line whose
/// text never flowed was laid out by its program, as a title or the two
/// halves of a double height, so a narrowing cuts its cells past the new
/// width rather than give them a line of their own (as the emulator cuts
/// the alternate screen's lines). A single line that wrapped onto a double
/// one is rewrapped on its own, the double line beginning a row whatever
/// the width, and goes on into it again once settled; a blank ending its
/// text is kept through the rewrap as [`END`].
///
/// The cursor on a double line is kept on it within the new width, where
/// the rewrap would carry it onto a line of its own; [`settle`] puts it
/// where its next letter goes.
pub fn lift(grid: &mut Grid<Cell>, cols: usize, reflow: bool) -> Lifted {
    let cols = cols.max(1);
    let half = LineSize::Wide.shown(grid.columns());
    let narrower = cols < grid.columns();
    let reflow = reflow && cols != grid.columns();
    let (cursor_line, at) = (grid.cursor.point.line, grid.cursor.point.column.0 + grid.cursor.input_needs_wrap as usize);
    let mut lines = Vec::new();
    let mut cursor = None;
    let mut joined = None;
    let (hist, rows) = (grid.history_size() as i32, grid.screen_lines() as i32);
    for l in -hist..rows {
        let before_double = l + 1 < rows && of_row(&grid[Line(l + 1)]).doubled();
        let row = &mut grid[Line(l)];
        let (size, wrapped, flowed) = (of_row(row), wraps(row), flowing(row));
        let join = joined.take();
        if !size.doubled() {
            if wrapped && before_double {
                joined = Some(end_before_double(row));
            }
            continue;
        }
        set_row(row, LineSize::Single);
        // With what wrapped from it if that's single. One that wrapped onto
        // another double line, or didn't wrap, has nothing to take back
        // when it widens; narrowed, what no longer fits goes on a line of
        // its own, which goes on into the double line it wrapped onto.
        let onto_single = wrapped && !before_double;
        let flow = reflow && (onto_single || (narrower && (wrapped || flowed)));
        if flow && wrapped && before_double {
            joined = Some(keep_end(row, half));
        }
        let flow = flow.then(|| lay_out(row, half, cols, onto_single));
        if flow.is_none() {
            let last = row.len() - 1;
            row[Column(last)].flags.remove(Flags::WRAPLINE);
            for c in cols..row.len() {
                row[Column(c)] = Cell::default();
            }
        }
        if Line(l) == cursor_line {
            cursor = Some((lines.len(), at));
        }
        let first = &mut row[Column(0)];
        lines.push(Lift { size, flowing: flowed, link: first.hyperlink(), wrapped, joined: join, flow });
        first.set_hyperlink(Some(Hyperlink::new(Some("flow-line-size"), format!("{LIFTED}{}", lines.len() - 1))));
    }
    if cursor.is_some() {
        let cursor = &mut grid.cursor;
        cursor.input_needs_wrap = false;
        cursor.point.column = cursor.point.column.min(Column(cols - 1));
    }
    Lifted { lines, half, cursor }
}

/// The columns of `row` up to `upto` that hold its text, short of the
/// padding at their end where a wide letter didn't fit, which the rewrap
/// leaves out at a line's end.
fn text_end(row: &Row<Cell>, upto: usize) -> usize {
    let mut end = upto.min(row.len());
    while end > 0 && row[Column(end - 1)].flags.contains(Flags::LEADING_WIDE_CHAR_SPACER) {
        end -= 1;
    }
    end
}

/// Lays out a double line whose text flows, with its old `half`, for a
/// rewrap to `cols` columns (see [`lift`]), with the wrap on its last cell
/// if it `wrapped`.
fn lay_out(row: &mut Row<Cell>, half: usize, cols: usize, wrapped: bool) -> Flow {
    let new_half = LineSize::Wide.shown(cols);
    let len = text_end(row, half);
    let text: Vec<Cell> = (0..len).map(|c| row[Column(c)].clone()).collect();
    let (mut cells, flow) = if cols < row.len() {
        // Cut at the new half, or before a wide letter across it, which
        // leaves padding in its place, as writing it there would.
        let mut kept = len.min(new_half);
        if kept < len && kept > 0 && text[kept - 1].flags.contains(Flags::WIDE_CHAR) {
            kept -= 1;
        }
        let mut cells = text[..kept].to_vec();
        if kept < len && kept < new_half {
            let mut pad = Cell::default();
            pad.flags.insert(Flags::LEADING_WIDE_CHAR_SPACER);
            cells.push(pad);
        }
        cells.resize(cols, Cell::default());
        cells.extend_from_slice(&text[kept..]);
        (cells, Flow::Narrower { kept })
    } else {
        let from = cols - new_half.saturating_sub(len);
        let mut cells = text;
        cells.resize(from, Cell::default());
        (cells, Flow::Wider { len, from })
    };
    if let Some(last) = cells.last_mut().filter(|_| wrapped) {
        last.flags.insert(Flags::WRAPLINE);
    }
    let occupied = cells.len();
    *row = Row::from_vec(cells, occupied);
    flow
}

/// Ends a single line that wrapped onto a double one, for the rewrap to take
/// it on its own: no wrap, its padding plain blanks, and a blank ending its
/// text kept as [`END`] (returning whether it was), which the rewrap would
/// otherwise drop.
fn end_before_double(row: &mut Row<Cell>) -> bool {
    let last = row.len() - 1;
    row[Column(last)].flags.remove(Flags::WRAPLINE);
    let end = text_end(row, row.len());
    for c in end..row.len() {
        row[Column(c)].flags.remove(Flags::LEADING_WIDE_CHAR_SPACER);
    }
    keep_end(row, end)
}

/// Keeps a blank ending the text of `row`'s first `upto` columns as [`END`]
/// through a rewrap, which would drop it at the end of a line; returns
/// whether it did.
fn keep_end(row: &mut Row<Cell>, upto: usize) -> bool {
    match text_end(row, upto).checked_sub(1).map(|c| &mut row[Column(c)]) {
        Some(cell) if cell.is_empty() => {
            cell.c = END;
            true
        },
        _ => false,
    }
}

/// Puts the wrap of the single line `row` back onto the double line after
/// it, once rewrapped (see [`lift`]): its text ends a row now, so the cells
/// after it are marked as the emulator's padding at the end of a wrapped
/// row (which it leaves where a wide letter didn't fit), which what reads
/// the text skips, and it runs straight on into the double line's. `ended`
/// is whether a blank ending its text was kept as [`END`].
fn rejoin(row: &mut Row<Cell>, ended: bool) {
    let cols = row.len();
    let shown = of_row(row).shown(cols);
    let end = (0..shown).rev().find(|&c| !row[Column(c)].is_empty()).map_or(0, |c| c + 1);
    if ended && end > 0 && row[Column(end - 1)].c == END {
        row[Column(end - 1)].c = ' ';
    }
    for c in end..shown {
        row[Column(c)].flags.insert(Flags::LEADING_WIDE_CHAR_SPACER);
    }
    row[Column(cols - 1)].flags.insert(Flags::WRAPLINE);
}

/// After a wider rewrap: moves the cells the rewrap brought back into a
/// double line laid out by [`lay_out`] from `from` to just after its `len`
/// columns, blanks the rest (its hidden half), and keeps its wrap on its
/// last cell if it still goes on.
fn take_back(row: &mut Row<Cell>, len: usize, from: usize) {
    let cols = row.len();
    let wrapped = wraps(row);
    let cells = &mut row[..];
    let taken = cols.saturating_sub(from);
    for k in 0..taken {
        cells.swap(len + k, from + k);
    }
    for cell in &mut cells[len + taken..] {
        *cell = Cell::default();
    }
    for cell in cells.iter_mut() {
        cell.flags.remove(Flags::WRAPLINE);
    }
    if wrapped {
        cells[cols - 1].flags.insert(Flags::WRAPLINE);
    }
}

/// The cell `q` cells along the line that goes on from `line` (a cell of a
/// double line's half, or of a single line's width, short of the padding
/// at a wrapped row's end), or the last of it.
fn along(grid: &Grid<Cell>, mut line: Line, mut q: usize) -> Point {
    let cols = grid.columns();
    loop {
        let row = &grid[line];
        let shown = of_row(row).shown(cols);
        let len = if wraps(row) { text_end(row, shown) } else { shown };
        if q < len || !wraps(row) || line.0 + 1 >= grid.screen_lines() as i32 {
            return Point::new(line, Column(q.min(shown - 1)));
        }
        q -= len;
        line += 1;
    }
}

/// Puts the marks [`lift`] took back on their lines' last cells, and the
/// first cells' own links back.
///
/// A double line that rewrapped with what wrapped from it keeps the cells
/// it holds of its own: narrower, its first row's first cells (the rest of
/// that row is blanks); wider, its own and those the rewrap brought back,
/// which are moved up to them. It wraps if the rewrap left it so.
///
/// One kept out of the rewrap that had wrapped wraps again. Widened, it
/// shows blank cells after its text, which a rewrap would have filled from
/// the next line (the alternate screen's, which the emulator cuts). They're
/// marked as the emulator's padding at the end of a wrapped row, so what
/// reads the text skips them, and it still runs straight on into the next
/// line's, as xterm, which rewraps nothing, leaves out cells nothing was
/// written in. Writing in one makes it a letter like any other.
///
/// A single line that wrapped onto a double one goes on into it again
/// ([`rejoin`]).
///
/// The cursor on a double line goes where its next letter would have gone,
/// as the emulator moves it through a rewrap: along the line, onto what
/// wrapped from it if the double line no longer holds that letter; or, on
/// one kept out of the rewrap, within its new half, or on its last column
/// with a wrap due if that's just past it. The cursor the rewrap brought
/// back into a double line with its letter goes along with that letter.
pub fn settle(grid: &mut Grid<Cell>, lifted: Lifted) {
    let cols = grid.columns();
    let half = LineSize::Wide.shown(cols);
    let mut left = lifted.lines.len();
    // Blanks kept as `END` before double lines yet to be found.
    let mut ends = lifted.lines.iter().filter(|lift| lift.joined == Some(true)).count();
    // The cursor's double line: where it is, and how many of its letters it
    // kept if it rewrapped.
    let mut cursor_on = None;
    let (hist, rows) = (grid.history_size() as i32, grid.screen_lines() as i32);
    for l in -hist..rows {
        if left == 0 {
            break;
        }
        let row = &mut grid[Line(l)];
        // On the first cell, where the rewrap leaves it, but looked for
        // along the line to be sure no link of Flow's is left behind.
        let mut found = None;
        for c in 0..row.len() {
            let Some(link) = row[Column(c)].hyperlink() else { continue };
            let Some(i) = link.uri().strip_prefix(LIFTED).and_then(|i| i.parse::<usize>().ok()) else { continue };
            row[Column(c)].set_hyperlink(lifted.lines.get(i).and_then(|lift| lift.link.clone()));
            found = found.or(lifted.lines.get(i).map(|lift| (i, lift)));
            left = left.saturating_sub(1);
        }
        let Some((i, lift)) = found else { continue };
        let kept = match lift.flow {
            Some(Flow::Narrower { kept }) => Some(kept),
            Some(Flow::Wider { len, from }) => {
                take_back(row, len, from);
                Some(half)
            },
            None => {
                if lift.wrapped {
                    for c in (lifted.half..half).rev() {
                        let cell = &mut row[Column(c)];
                        if cell.c != ' ' || cell.zerowidth().is_some() || cell.flags.intersects(Flags::WIDE_CHAR | Flags::WIDE_CHAR_SPACER) {
                            break;
                        }
                        cell.flags.insert(Flags::LEADING_WIDE_CHAR_SPACER);
                    }
                    row[Column(cols - 1)].flags.insert(Flags::WRAPLINE);
                }
                None
            },
        };
        mark_row(row, lift.size, lift.flowing || lift.flow.is_some());
        if let Some(Flow::Wider { len, from }) = lift.flow {
            let cursor = &mut grid.cursor.point;
            if cursor.line == l && cursor.column.0 >= from {
                cursor.column = Column(len + cursor.column.0 - from);
            }
        }
        if let (Some(ended), true) = (lift.joined, l > -hist) {
            rejoin(&mut grid[Line(l - 1)], ended);
        }
        ends = ends.saturating_sub((lift.joined == Some(true)) as usize);
        if lifted.cursor.is_some_and(|(at, _)| at == i) {
            cursor_on = Some((l, kept));
        }
    }
    // A double line can be gone where its line before isn't (ConPTY's
    // resize keeps nothing below the cursor), which leaves that line's END.
    if ends > 0 {
        for l in -hist..rows {
            for cell in &mut grid[Line(l)] {
                if cell.c == END {
                    cell.c = ' ';
                }
            }
        }
    }
    let (Some((line, kept)), Some((_, at))) = (cursor_on, lifted.cursor) else { return };
    // Along the line if it goes on (a narrowing may have given its letters
    // a line of their own), else on the double line, as one kept out. The
    // cursor stays on the screen: a rewrap that pushed the line it's on up
    // into the scrollback has left it where it could.
    let point = match kept {
        Some(kept) if at >= kept && wraps(&grid[Line(line)]) => (along(grid, Line(line + 1), at - kept), false),
        _ if at < half => (Point::new(Line(line), Column(at)), false),
        _ => (Point::new(Line(line), Column(half - 1)), at == half),
    };
    if point.0.line >= 0 {
        (grid.cursor.point, grid.cursor.input_needs_wrap) = point;
    }
}

/// Puts a cursor that's past the half of a double line on its last column,
/// as a DEC terminal keeps it within the line's columns. Returns how many
/// columns the cursor's line has if it's a double line: a DEC terminal's
/// right margin is at its half.
pub fn clamp_cursor(grid: &mut Grid<Cell>) -> Option<usize> {
    let half = grid.columns() / 2;
    if half == 0 || !of_row(&grid[grid.cursor.point.line]).doubled() {
        return None;
    }
    if grid.cursor.point.column.0 >= half {
        grid.cursor.point.column = Column(half - 1);
        grid.cursor.input_needs_wrap = false;
    }
    Some(half)
}

/// The emulator with a double line's right margin at its half, as on a DEC
/// terminal or xterm, where alacritty_terminal has it at the screen's edge
/// on every line: text wraps there (or, with autowrap off, overwrites the
/// last column), the cursor stops there, and inserting, deleting and
/// erasing characters work within it, which keeps the line's mark on its
/// last cell. An erase within the line (`CSI K`, or `CSI J` from past its
/// start) puts back the mark it cleared. The line's wrap is kept on that
/// cell too, and ends as a single line's does when blanks come in at its
/// end. Everything else, and everything on a single line, is the emulator's
/// own, but for a spacing mark the Mac's shells measure as no width, which
/// goes on the letter before ([`crate::combining`]) on any line. Until a
/// program has made a line double (`double` false), that's all it does.
pub struct Margins<'a, L: EventListener> {
    term: &'a mut Term<L>,
    double: bool,
}

impl<'a, L: EventListener> Margins<'a, L> {
    /// The emulator as it is to `term`'s output: with double lines' margins
    /// where `double` says a program has made one.
    pub fn new(term: &'a mut Term<L>, double: bool) -> Self {
        Self { term, double }
    }

    /// The cursor's line's mark: its size, and whether its text flows.
    fn mark(&self) -> Option<(LineSize, bool)> {
        let grid = self.term.grid();
        marked(&grid[grid.cursor.point.line]).filter(|_| self.double)
    }

    /// The cursor's line's columns if it's a double line, the cursor put
    /// within them.
    fn half(&mut self) -> Option<usize> {
        if !self.double {
            return None;
        }
        clamp_cursor(self.term.grid_mut())
    }

    /// On to the start of the next line, scrolling at the bottom, as
    /// autowrap goes, with the line marked as wrapped on its last cell, as
    /// the emulator marks a single line, so that its text and the next
    /// line's read as one. Its text flows ([`flowing`]) from then on: a
    /// resize lays the two out for its rewrap, which goes by the screen's
    /// width, as one line by the double line's ([`lift`]).
    fn wrap(&mut self) {
        let grid = self.term.grid_mut();
        let (line, last) = (grid.cursor.point.line, grid.columns() - 1);
        let size = of_row(&grid[line]);
        mark_row(&mut grid[line], size, true);
        grid[line][Column(last)].flags.insert(Flags::WRAPLINE);
        Handler::linefeed(&mut *self.term);
        Handler::carriage_return(&mut *self.term);
    }

    /// Ends the cursor's line's wrap: an edit has left blanks at the end of
    /// its columns, so its text no longer runs on into the next line's, as
    /// the emulator's edits that reach a single line's end take its wrap.
    fn unwrap(&mut self) {
        let grid = self.term.grid_mut();
        let (line, last) = (grid.cursor.point.line, grid.columns() - 1);
        grid[line][Column(last)].flags.remove(Flags::WRAPLINE);
    }

    /// A blank on the cursor's cell, marked as the padding the emulator
    /// leaves at the end of a single line where a wide letter didn't fit,
    /// which reading the text skips; written as the emulator writes it there,
    /// over half a wide letter too.
    fn pad(&mut self) {
        let insert = self.term.mode().contains(TermMode::INSERT);
        if insert {
            Handler::unset_mode(&mut *self.term, Mode::Named(NamedMode::Insert));
        }
        self.term.grid_mut().cursor.template.flags.insert(Flags::LEADING_WIDE_CHAR_SPACER);
        Handler::input(&mut *self.term, ' ');
        self.term.grid_mut().cursor.template.flags.remove(Flags::LEADING_WIDE_CHAR_SPACER);
        if insert {
            Handler::set_mode(&mut *self.term, Mode::Named(NamedMode::Insert));
        }
    }

    /// Padding before the cursor's cell (see [`settle`] and [`rejoin`]) is
    /// blanks the letter written there now follows. The emulator's own
    /// padding is only ever on a line's last cell, after the cursor.
    fn follow_padding(&mut self) {
        let (row, col, _) = self.row();
        for cell in row[..col].iter_mut().rev().take_while(|cell| cell.flags.contains(Flags::LEADING_WIDE_CHAR_SPACER)) {
            cell.flags.remove(Flags::LEADING_WIDE_CHAR_SPACER);
        }
    }

    /// Puts back `mark` on the cursor's line, which an erase has taken off.
    fn keep(&mut self, mark: Option<(LineSize, bool)>) {
        if let Some((size, flowing)) = mark {
            let grid = self.term.grid_mut();
            let line = grid.cursor.point.line;
            mark_row(&mut grid[line], size, flowing);
        }
    }

    /// The cursor's line's cells, its column, and the color erased cells take.
    fn row(&mut self) -> (&mut [Cell], usize, Color) {
        let grid = self.term.grid_mut();
        let (point, bg) = (grid.cursor.point, grid.cursor.template.bg);
        (&mut grid[point.line][..], point.column.0, bg)
    }
}

/// Handler methods passed on to the emulator as they are, and those after
/// which the cursor may be past a double line's half (moved there, or with
/// another line moved under it), which put it back on the line's last column.
macro_rules! pass {
    ($($name:ident($($arg:ident: $ty:ty),*);)*) => {
        $(fn $name(&mut self, $($arg: $ty),*) {
            Handler::$name(&mut *self.term, $($arg),*)
        })*
    };
}
macro_rules! moving {
    ($($name:ident($($arg:ident: $ty:ty),*);)*) => {
        $(fn $name(&mut self, $($arg: $ty),*) {
            Handler::$name(&mut *self.term, $($arg),*);
            if self.double {
                clamp_cursor(self.term.grid_mut());
            }
        })*
    };
}

impl<L: EventListener> Handler for Margins<'_, L> {
    fn input(&mut self, c: char) {
        if crate::combining::joins_letter_before(c) {
            return crate::combining::join(self.term.grid_mut(), c);
        }
        if !self.double {
            return Handler::input(&mut *self.term, c);
        }
        let width = c.width().unwrap_or(0);
        // Control characters and combining marks are the emulator's: a mark
        // goes on the letter before, which a wrap due leaves under the cursor.
        // So is a letter on a single line, which may follow padding where
        // the line goes on into a double one (see `rejoin`).
        let Some(half) = self.half().filter(|_| width > 0) else {
            if width > 0 && !self.term.grid().cursor.input_needs_wrap {
                self.follow_padding();
            }
            return Handler::input(&mut *self.term, c);
        };
        if width > half {
            return;
        }
        let autowrap = self.term.mode().contains(TermMode::LINE_WRAP);
        let cursor = &self.term.grid().cursor;
        let (col, due) = (cursor.point.column.0, cursor.input_needs_wrap);
        // A letter already on the last column, or a wide one whose second
        // half wouldn't fit: on to the next line (the wide one leaving
        // padding where it didn't fit), or without autowrap, onto the last
        // column again (a wide one is dropped), as alacritty_terminal does at
        // the end of a single line.
        if due || col + width > half {
            if autowrap {
                if !due {
                    self.pad();
                }
                self.wrap();
                return self.input(c);
            }
            if col + width > half {
                self.term.grid_mut().cursor.input_needs_wrap = true;
                return;
            }
        }
        // Insert mode pushes the letters after the cursor along within the
        // line's columns, losing those pushed past its last, not the mark.
        let insert = self.term.mode().contains(TermMode::INSERT);
        if insert {
            let (row, col, _) = self.row();
            for k in (col..half - width).rev() {
                row.swap(k + width, k);
            }
            Handler::unset_mode(&mut *self.term, Mode::Named(NamedMode::Insert));
        }
        self.follow_padding();
        self.term.grid_mut().cursor.input_needs_wrap = false;
        Handler::input(&mut *self.term, c);
        if insert {
            Handler::set_mode(&mut *self.term, Mode::Named(NamedMode::Insert));
        }
        // The emulator moves on past the last column; the cursor stays on it
        // with a wrap due, as at the end of a single line.
        let cursor = &mut self.term.grid_mut().cursor;
        if cursor.point.column.0 >= half {
            cursor.point.column = Column(half - 1);
            cursor.input_needs_wrap = true;
        }
    }

    /// ICH: the letters from the cursor on move right within the line's
    /// columns.
    fn insert_blank(&mut self, count: usize) {
        let Some(half) = self.half() else { return Handler::insert_blank(&mut *self.term, count) };
        let (row, col, bg) = self.row();
        let count = count.min(half - col);
        for k in (col..half - count).rev() {
            row.swap(k + count, k);
        }
        for cell in &mut row[col..col + count] {
            *cell = bg.into();
        }
    }

    /// DCH: the letters after those deleted move left, and blanks come in
    /// at the line's last column rather than the screen's.
    fn delete_chars(&mut self, count: usize) {
        let Some(half) = self.half() else { return Handler::delete_chars(&mut *self.term, count) };
        let (row, col, bg) = self.row();
        let count = count.min(half - col);
        for k in col..half - count {
            row.swap(k, k + count);
        }
        for cell in &mut row[half - count..half] {
            *cell = bg.into();
        }
        self.unwrap();
    }

    /// ECH, as far as the line's last column.
    fn erase_chars(&mut self, count: usize) {
        let Some(half) = self.half() else { return Handler::erase_chars(&mut *self.term, count) };
        let left = half - self.term.grid().cursor.point.column.0;
        Handler::erase_chars(&mut *self.term, count.min(left));
        if count >= left {
            self.unwrap();
        }
    }

    /// EL: DEC's terminals keep a line's size through an erase within it.
    fn clear_line(&mut self, mode: LineClearMode) {
        let mark = self.mark();
        // Erased from its start to its last column, it no longer wraps (an
        // erase to its end or of all of it clears the last cell's wrap).
        let whole = matches!(mode, LineClearMode::Left) && self.half().is_some_and(|half| self.term.grid().cursor.point.column.0 + 1 >= half);
        Handler::clear_line(&mut *self.term, mode);
        self.keep(mark);
        if whole {
            self.unwrap();
        }
    }

    /// ED: and through one from the cursor down that leaves some of it.
    fn clear_screen(&mut self, mode: ClearMode) {
        let partly = matches!(mode, ClearMode::Below) && self.term.grid().cursor.point.column.0 > 0;
        let mark = if partly { self.mark() } else { None };
        Handler::clear_screen(&mut *self.term, mode);
        self.keep(mark);
    }

    /// HT, to the tab stops within the line's columns, and at most its last.
    fn put_tab(&mut self, count: u16) {
        let Some(half) = self.half() else { return Handler::put_tab(&mut *self.term, count) };
        // After a letter on the last column, a tab is a line break.
        if self.term.grid().cursor.input_needs_wrap {
            if self.term.mode().contains(TermMode::LINE_WRAP) {
                self.wrap();
            }
            return;
        }
        for _ in 0..count {
            if self.term.grid().cursor.point.column.0 + 1 >= half {
                break;
            }
            Handler::put_tab(&mut *self.term, 1);
        }
        clamp_cursor(self.term.grid_mut());
    }

    moving! {
        goto(line: i32, col: usize);
        goto_line(line: i32);
        goto_col(col: usize);
        move_up(lines: usize);
        move_down(lines: usize);
        move_forward(cols: usize);
        move_forward_tabs(count: u16);
        linefeed();
        newline();
        reverse_index();
        scroll_up(lines: usize);
        scroll_down(lines: usize);
        insert_blank_lines(lines: usize);
        delete_lines(lines: usize);
        restore_cursor_position();
        set_scrolling_region(top: usize, bottom: Option<usize>);
        set_private_mode(mode: PrivateMode);
        unset_private_mode(mode: PrivateMode);
        reset_state();
    }

    pass! {
        set_title(title: Option<String>);
        set_cursor_style(style: Option<CursorStyle>);
        set_cursor_shape(shape: CursorShape);
        identify_terminal(intermediate: Option<char>);
        device_status(arg: usize);
        move_backward(cols: usize);
        move_down_and_cr(rows: usize);
        move_up_and_cr(rows: usize);
        backspace();
        carriage_return();
        bell();
        substitute();
        set_horizontal_tabstop();
        move_backward_tabs(count: u16);
        save_cursor_position();
        clear_tabs(mode: TabulationClearMode);
        set_tabs(interval: u16);
        terminal_attribute(attr: Attr);
        set_mode(mode: Mode);
        unset_mode(mode: Mode);
        report_mode(mode: Mode);
        report_private_mode(mode: PrivateMode);
        set_keypad_application_mode();
        unset_keypad_application_mode();
        set_active_charset(index: CharsetIndex);
        configure_charset(index: CharsetIndex, charset: StandardCharset);
        set_color(index: usize, color: Rgb);
        dynamic_color_sequence(prefix: String, index: usize, terminator: &str);
        reset_color(index: usize);
        clipboard_store(clipboard: u8, base64: &[u8]);
        clipboard_load(clipboard: u8, terminator: &str);
        decaln();
        push_title();
        pop_title();
        text_area_size_pixels();
        text_area_size_chars();
        set_hyperlink(link: Option<ansi::Hyperlink>);
        set_mouse_cursor_icon(icon: CursorIcon);
        report_keyboard_mode();
        push_keyboard_mode(mode: KeyboardModes);
        pop_keyboard_modes(to_pop: u16);
        set_keyboard_mode(mode: KeyboardModes, behavior: KeyboardModesApplyBehavior);
        set_modify_other_keys(mode: ModifyOtherKeys);
        report_modify_other_keys();
        set_scp(char_path: ScpCharPath, update_mode: ScpUpdateMode);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mark_lives_in_the_last_cell() {
        let mut row = Row::<Cell>::new(10);
        assert_eq!(of_row(&row), LineSize::Single);
        set_row(&mut row, LineSize::Top);
        assert_eq!(of_row(&row), LineSize::Top);
        assert!(row[Column(9)].underline_color().is_some() && row[Column(9)].c == ' ');
        // Anywhere else it doesn't count, and setting a size takes it away
        // from there too.
        let last = row[Column(9)].clone();
        row[Column(6)] = last;
        row[Column(9)] = Cell::default();
        assert_eq!(of_row(&row), LineSize::Single);
        set_row(&mut row, LineSize::Wide);
        assert_eq!(of_row(&row), LineSize::Wide);
        assert!(row[Column(6)].underline_color().is_none());
        set_row(&mut row, LineSize::Single);
        assert!((0..10).all(|c| row[Column(c)].extra.is_none()));
        // An underline color of a program's own isn't a mark.
        row[Column(9)].set_underline_color(Some(Color::Spec(Rgb { r: 0xF1, g: 0x0D, b: 0xE7 })));
        assert_eq!(of_row(&row), LineSize::Single);
    }

    #[test]
    fn on_single_lines_the_margins_change_nothing() {
        use alacritty_terminal::event::VoidListener;
        use alacritty_terminal::term::test::TermSize;
        use alacritty_terminal::term::Config;
        use alacritty_terminal::vte::ansi::{Processor, StdSyncHandler};
        // Text, colors, links, a title, moves, tabs, the line edits, insert
        // mode, a repeat, autowrap on and off, a scrolling region, the
        // alternate screen and a reverse index.
        let bytes = "plain\r\n\x1b[1;31mred\x1b[0m \x1b]8;;https://example.com\x1b\\link\x1b]8;;\x1b\\\x1b]2;title\x07\
            \x1b[3;5Hat\x1b[2@\x1b[P\x1b[3X\x1b[1K\r\n\t\ttabs\x1b[3I!\x1b[Z\x1b[4hins\x1b[4l\x1b[2b \
            \x1b[?7lno wrap past the edge here\x1b[?7h\r\nwrap past the edge of it 中文\x1b[2;4r\x1b[S\x1b[r\
            \x1b7\x1b[?1049halt\x1b[?1049l\x1b8\x1bM\x1bMup\x1b[8;30H\x1b[Ldown\x1b[0J";
        let screen = || Term::new(Config { scrolling_history: 100, ..Config::default() }, &TermSize::new(20, 8), VoidListener);
        let (mut plain, mut margined) = (screen(), screen());
        Processor::<StdSyncHandler>::new().advance(&mut plain, bytes.as_bytes());
        Processor::<StdSyncHandler>::new().advance(&mut Margins::new(&mut margined, true), bytes.as_bytes());
        let (a, b) = (plain.grid(), margined.grid());
        assert_eq!((a.history_size(), a.cursor.point, *plain.mode()), (b.history_size(), b.cursor.point, *margined.mode()));
        let rows = -(a.history_size() as i32)..a.screen_lines() as i32;
        assert!(rows.clone().all(|l| a[Line(l)] == b[Line(l)]));
        let text: Vec<String> = rows.map(|l| (0..20).map(|c| a[Line(l)][Column(c)].c).collect::<String>().trim_end().to_string()).collect();
        assert!(["up", "plain", "tabs", "own"].iter().all(|w| text.iter().any(|t| t.contains(w))), "{text:?}");
    }

    #[test]
    fn marks_ride_a_rewrap_on_their_lines_first_cells_and_come_back_last() {
        let mut grid: Grid<Cell> = Grid::new(4, 10, 100);
        for (c, ch) in "ab".chars().enumerate() {
            grid[Line(1)][Column(c)].c = ch;
        }
        let link = Hyperlink::new(Some("mine"), "https://example.com".into());
        grid[Line(1)][Column(0)].set_hyperlink(Some(link.clone()));
        set_row(&mut grid[Line(1)], LineSize::Top);
        set_row(&mut grid[Line(2)], LineSize::Bottom);
        let lifted = lift(&mut grid, 6, true);
        assert_eq!(of_row(&grid[Line(1)]), LineSize::Single);
        grid.resize(true, 4, 6);
        settle(&mut grid, lifted);
        // Narrowed, the marks would have wrapped onto lines of their own.
        assert_eq!(grid.history_size(), 0);
        assert_eq!((of_row(&grid[Line(1)]), of_row(&grid[Line(2)]), of_row(&grid[Line(3)])), (LineSize::Top, LineSize::Bottom, LineSize::Single));
        assert!(grid[Line(1)][Column(5)].underline_color().is_some(), "on the new last cell");
        assert_eq!(grid[Line(1)][Column(0)].hyperlink(), Some(link), "the first cell's own link back");
        assert_eq!(grid[Line(2)][Column(0)].hyperlink(), None);
        // And widened again.
        let lifted = lift(&mut grid, 12, true);
        grid.resize(true, 4, 12);
        settle(&mut grid, lifted);
        assert!(grid[Line(2)][Column(11)].underline_color().is_some());
    }

    /// Writes `text` from the start of `line`, a letter to a cell.
    fn write(grid: &mut Grid<Cell>, line: i32, text: &str) {
        for (c, ch) in text.chars().enumerate() {
            grid[Line(line)][Column(c)].c = ch;
        }
    }

    /// The letters on `line` up to `upto`, the blanks after them left out.
    fn letters(grid: &Grid<Cell>, line: i32, upto: usize) -> String {
        (0..upto).map(|c| grid[Line(line)][Column(c)].c).collect::<String>().trim_end().to_string()
    }

    #[test]
    fn a_single_line_made_double_loses_its_right_half_and_a_double_one_made_single_its_wrap() {
        let mut row = Row::<Cell>::new(10);
        for (c, ch) in "0123".chars().chain(['\u{4e2d}', ' ']).chain("xyzw".chars()).enumerate() {
            row[Column(c)].c = ch;
        }
        row[Column(4)].flags.insert(Flags::WIDE_CHAR);
        row[Column(5)].flags.insert(Flags::WIDE_CHAR_SPACER);
        row[Column(9)].flags.insert(Flags::WRAPLINE);
        // Past its half, and the wide letter across it, are gone, the line's
        // wrap with them.
        make(&mut row, LineSize::Wide);
        assert_eq!(((0..10).map(|c| row[Column(c)].c).collect::<String>(), of_row(&row), wraps(&row)), ("0123      ".into(), LineSize::Wide, false));
        assert!((4..9).all(|c| row[Column(c)] == Cell::default()));
        // From one double size to another, nothing's lost: wrapped at its
        // half after a wide letter that didn't fit, it stays so.
        row[Column(4)].flags.insert(Flags::LEADING_WIDE_CHAR_SPACER);
        row[Column(9)].flags.insert(Flags::WRAPLINE);
        make(&mut row, LineSize::Top);
        assert_eq!((of_row(&row), wraps(&row)), (LineSize::Top, true));
        assert!(row[Column(4)].flags.contains(Flags::LEADING_WIDE_CHAR_SPACER));
        // Single again: blanks past its old half, and no wrap.
        make(&mut row, LineSize::Single);
        assert_eq!((of_row(&row), wraps(&row)), (LineSize::Single, false));
        assert!((0..10).all(|c| row[Column(c)].flags.is_empty() && row[Column(c)].extra.is_none()));
    }

    /// Resizes `grid` to `cols` columns as `Emulator::resize` does, the
    /// marks lifted through it: rewrapped like the main screen, or cut like
    /// the alternate one.
    fn resize(grid: &mut Grid<Cell>, cols: usize, reflow: bool) {
        let lifted = lift(grid, cols, reflow);
        let rows = grid.screen_lines();
        grid.resize(reflow, rows, cols);
        settle(grid, lifted);
    }

    /// Each row from the top of the scrollback down to the cursor's: its
    /// letters as far as it shows them (a wide letter once, padding as `·`,
    /// the blanks after them left out), after `2:` if it's double, and then
    /// `+` if it wraps.
    fn rows(grid: &Grid<Cell>) -> Vec<String> {
        let cols = grid.columns();
        (-(grid.history_size() as i32)..=grid.cursor.point.line.0)
            .map(|l| {
                let row = &grid[Line(l)];
                let size = of_row(row);
                let cells = (0..size.shown(cols)).map(|c| &row[Column(c)]).filter(|cell| !cell.flags.contains(Flags::WIDE_CHAR_SPACER));
                let text: String = cells.map(|cell| if cell.flags.contains(Flags::LEADING_WIDE_CHAR_SPACER) { '·' } else { cell.c }).collect();
                format!("{}{}{}", if size.doubled() { "2:" } else { "" }, text.trim_end(), if wraps(row) { "+" } else { "" })
            })
            .collect()
    }

    /// A double line wrapped at its half onto a single line that wrapped at
    /// the screen's edge, ten columns wide, the cursor after its text.
    fn wrapped_double_line() -> Grid<Cell> {
        let mut grid: Grid<Cell> = Grid::new(6, 10, 100);
        write(&mut grid, 0, "abcde");
        set_row(&mut grid[Line(0)], LineSize::Wide);
        grid[Line(0)][Column(9)].flags.insert(Flags::WRAPLINE);
        write(&mut grid, 1, "fghijklmno");
        grid[Line(1)][Column(9)].flags.insert(Flags::WRAPLINE);
        write(&mut grid, 2, "pq");
        grid.cursor.point = Point::new(Line(2), Column(2));
        grid
    }

    #[test]
    fn a_wrapped_double_line_rewraps_with_what_wrapped_from_it_by_its_half() {
        let mut grid = wrapped_double_line();
        let before = rows(&grid);
        assert_eq!(before, ["2:abcde+", "fghijklmno+", "pq"]);
        // Narrowed, it keeps the three letters it now shows, and the rest go
        // on ahead of what wrapped from it, which takes a line more; the
        // cursor stays after the text.
        resize(&mut grid, 6, true);
        assert_eq!(rows(&grid), ["2:abc+", "defghi+", "jklmno+", "pq"]);
        assert_eq!((of_row(&grid[Line(-1)]), grid.cursor.point.column), (LineSize::Wide, Column(2)));
        // Widened, it takes back as many as it now shows, and a line goes.
        resize(&mut grid, 14, true);
        assert_eq!(rows(&grid), ["2:abcdefg+", "hijklmnopq"]);
        assert_eq!(grid.cursor.point.column, Column(10));
        resize(&mut grid, 10, true);
        assert_eq!(rows(&grid), before);
        // Wide enough for all of it, it no longer wraps, but its text flows:
        // narrowed again, what no longer fits goes on a line of its own, as a
        // single line's does, and the cursor goes along with it.
        resize(&mut grid, 40, true);
        assert_eq!((rows(&grid), grid.cursor.point.column), (vec!["2:abcdefghijklmnopq".to_string()], Column(17)));
        assert!(flowing(&grid[grid.cursor.point.line]));
        resize(&mut grid, 10, true);
        assert_eq!((rows(&grid), grid.cursor.point.column), (before, Column(2)));
    }

    #[test]
    fn on_the_alternate_screen_a_double_line_is_cut_and_keeps_its_wrap() {
        let mut grid = wrapped_double_line();
        // Narrowed, the double line keeps its letters (three of them shown),
        // pushes none onto a line of its own and still wraps, and the line
        // after is cut, as the emulator cuts every line there.
        resize(&mut grid, 6, false);
        assert_eq!((rows(&grid), letters(&grid, 0, 6)), (vec!["2:abc+".to_string(), "fghijk".into(), "pq".into()], "abcde".into()));
        // Widened, it takes nothing from the next line, and the cells it now
        // shows after its text are padding.
        resize(&mut grid, 14, false);
        assert_eq!(rows(&grid), ["2:abcde··+", "fghijk", "pq"]);
        assert!(grid[Line(0)][Column(13)].underline_color().is_some());
        assert!(!grid[Line(0)][Column(7)].flags.contains(Flags::LEADING_WIDE_CHAR_SPACER), "only in its half");
    }

    #[test]
    fn a_wide_letter_across_a_double_lines_new_half_goes_on_whole() {
        let mut grid: Grid<Cell> = Grid::new(6, 20, 100);
        // Two wide letters among its ten columns, each two of them.
        write(&mut grid, 0, "ab\u{4e2d} \u{6587} cdef");
        for c in [2, 4] {
            grid[Line(0)][Column(c)].flags.insert(Flags::WIDE_CHAR);
            grid[Line(0)][Column(c + 1)].flags.insert(Flags::WIDE_CHAR_SPACER);
        }
        set_row(&mut grid[Line(0)], LineSize::Wide);
        grid[Line(0)][Column(19)].flags.insert(Flags::WRAPLINE);
        write(&mut grid, 1, "ghij");
        grid.cursor.point = Point::new(Line(1), Column(4));
        let before = rows(&grid);
        assert_eq!(before, ["2:ab中文cdef+", "ghij"]);
        // Three columns: the first wide letter would cross the half, so it
        // goes on whole, leaving padding, as writing it there would.
        resize(&mut grid, 7, true);
        assert_eq!(rows(&grid), ["2:ab·+", "中文cde+", "fghij"]);
        // Four: it comes back, and the second wide letter starts the next line.
        resize(&mut grid, 9, true);
        assert_eq!(rows(&grid), ["2:ab中+", "文cdefghi+", "j"]);
        resize(&mut grid, 20, true);
        assert_eq!(rows(&grid), before);
        // Two columns, then three: no room for the wide letter it would take
        // back, so it ends with padding again.
        resize(&mut grid, 5, true);
        assert_eq!(rows(&grid), ["2:ab+", "中文c+", "defgh+", "ij"]);
        resize(&mut grid, 7, true);
        assert_eq!(rows(&grid), ["2:ab·+", "中文cde+", "fghij"]);
        resize(&mut grid, 20, true);
        assert_eq!(rows(&grid), before);
    }

    #[test]
    fn a_single_line_wrapped_onto_a_double_one_goes_on_into_it_through_a_resize() {
        let mut grid: Grid<Cell> = Grid::new(6, 20, 100);
        // Its text ends in a blank, which is its own.
        write(&mut grid, 0, "0123456789abcdefghi ");
        grid[Line(0)][Column(19)].flags.insert(Flags::WRAPLINE);
        write(&mut grid, 1, "KLMNOPQRST");
        set_row(&mut grid[Line(1)], LineSize::Wide);
        grid[Line(1)][Column(19)].flags.insert(Flags::WRAPLINE);
        write(&mut grid, 2, "UVWXYZ");
        grid.cursor.point = Point::new(Line(2), Column(6));
        let before = rows(&grid);
        // It rewraps on its own, and the double line begins a row whatever
        // the width: the cells after its text are padding, and it still
        // runs on into the double line's.
        resize(&mut grid, 12, true);
        assert_eq!(rows(&grid), ["0123456789ab+", "cdefghi ····+", "2:KLMNOP+", "QRSTUVWXYZ"]);
        resize(&mut grid, 30, true);
        assert_eq!(rows(&grid), ["0123456789abcdefghi ··········+", "2:KLMNOPQRSTUVWXY+", "Z"]);
        resize(&mut grid, 20, true);
        assert_eq!(rows(&grid), before);
        let hist = grid.history_size() as i32;
        assert!((-hist..6).all(|l| (0..20).all(|c| grid[Line(l)][Column(c)].c != END)), "its blank back");
        // And cut on the alternate screen.
        resize(&mut grid, 12, false);
        assert_eq!(rows(&grid)[hist as usize..], ["0123456789ab+", "2:KLMNOP+", "UVWXYZ"]);
    }

    #[test]
    fn a_double_line_wrapped_onto_another_gives_what_no_longer_fits_a_line_of_its_own() {
        let mut grid: Grid<Cell> = Grid::new(6, 20, 100);
        // A double line wrapped onto a single line that wrapped onto another
        // double line.
        write(&mut grid, 0, "abcdefghij");
        set_row(&mut grid[Line(0)], LineSize::Wide);
        grid[Line(0)][Column(19)].flags.insert(Flags::WRAPLINE);
        write(&mut grid, 1, "klmnopqrstuvwxyz0123");
        grid[Line(1)][Column(19)].flags.insert(Flags::WRAPLINE);
        write(&mut grid, 2, "45");
        set_row(&mut grid[Line(2)], LineSize::Wide);
        grid.cursor.point = Point::new(Line(2), Column(2));
        let before = rows(&grid);
        assert_eq!(before, ["2:abcdefghij+", "klmnopqrstuvwxyz0123+", "2:45"]);
        resize(&mut grid, 12, true);
        assert_eq!(rows(&grid), ["2:abcdef+", "ghijklmnopqr+", "stuvwxyz0123+", "2:45"]);
        // Wide enough to take in all of the single line, the first double
        // line wraps straight onto the second, which it can take nothing
        // from; narrowed, what no longer fits goes on a line of its own,
        // which goes on into the second.
        resize(&mut grid, 60, true);
        assert_eq!(rows(&grid), ["2:abcdefghijklmnopqrstuvwxyz0123+", "2:45"]);
        resize(&mut grid, 70, true);
        assert_eq!(rows(&grid), ["2:abcdefghijklmnopqrstuvwxyz0123·····+", "2:45"]);
        resize(&mut grid, 20, true);
        assert_eq!((rows(&grid), grid.cursor.point.column), (before, Column(2)));
    }

    #[test]
    fn a_double_line_shows_half_its_columns() {
        assert_eq!((LineSize::Single.shown(81), LineSize::Wide.shown(81), LineSize::Bottom.shown(1)), (81, 40, 1));
    }
}
