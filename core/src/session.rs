//! A running shell: PTY + terminal emulator + shell-integration state.

use std::collections::VecDeque;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use alacritty_terminal::event::{Event as TermEvent, EventListener, WindowSize};
use alacritty_terminal::grid::{Dimensions, Grid, Row};
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::term::{Config as TermConfig, Term, TermMode};
use alacritty_terminal::vte::ansi::{CursorStyle, Handler, Processor, Rgb, StdSyncHandler};
use anyhow::{Context, Result};
use parking_lot::{Condvar, Mutex};
use portable_pty::{native_pty_system, ChildKiller, MasterPty, PtySize};
use unicode_width::UnicodeWidthChar;
use winit::event_loop::EventLoopProxy;

use crate::event::{SessionId, UserEvent};
use crate::graphics::{self, ImageStore, Place, Screen};
use crate::line_size::{self, LineSize};
use crate::model::{self, ModelStore};
/// The startup logo's picture, for [`Session::show_banner`].
pub use crate::graphics::Image;
use crate::osc::{FxRequest, Marker, Modes, OscScanner, Piece, Progress, SoundRequest};
use crate::pipe::{self, Pipe};
use crate::shell::{build_command, BashFlavor, ShellKind, ShellProfile};
use crate::{iterm, sixel};

/// The keys that clear what's half-typed at `kind`'s prompt, sent ahead of
/// a command Flow types. cmd's line editor and PowerShell's (PSReadLine,
/// whose Windows keys are its own there) take End, then Ctrl+Home to
/// delete back to the line's start. Esc alone would clear it too, but
/// ConPTY reads an Esc followed by the command's `[` as the start of a key's
/// sequence, so `[Console]::…` lost its first two letters to Esc [ C, the
/// cursor key. The Unix shells take Ctrl+E, Ctrl+U. BusyBox takes Ctrl+A,
/// Ctrl+K: its Ctrl+U draws the whole prompt again, and the prompt holds
/// the hook's mark that the last command is done, which would end the
/// command just sent before it ran, with nothing printed.
pub(crate) fn clear_line_keys(kind: ShellKind, flavor: BashFlavor) -> &'static [u8] {
    match (kind, flavor) {
        (ShellKind::Cmd | ShellKind::PowerShell, _) => b"\x1b[F\x1b[1;5H",
        (ShellKind::Bash, BashFlavor::BusyBox) => b"\x01\x0b",
        _ => b"\x05\x15",
    }
}

/// Terminal grid dimensions handed to alacritty.
#[derive(Clone, Copy, Debug)]
pub struct GridSize {
    pub cols: usize,
    pub rows: usize,
}

impl Dimensions for GridSize {
    fn total_lines(&self) -> usize {
        self.rows
    }
    fn screen_lines(&self) -> usize {
        self.rows
    }
    fn columns(&self) -> usize {
        self.cols
    }
}

type Sink = Arc<dyn Fn(UserEvent) + Send + Sync>;

/// Coalesces wakeups so a flood of output produces one redraw request.
#[derive(Clone)]
pub struct Waker {
    sink: Sink,
    pending: Arc<AtomicBool>,
}

impl Waker {
    pub fn new(proxy: EventLoopProxy<UserEvent>) -> Self {
        Self::from_fn(move |event| {
            let _ = proxy.send_event(event);
        })
    }
    pub fn from_fn(f: impl Fn(UserEvent) + Send + Sync + 'static) -> Self {
        Self { sink: Arc::new(f), pending: Arc::new(AtomicBool::new(false)) }
    }
    pub fn wake(&self) {
        if !self.pending.swap(true, Ordering::AcqRel) {
            (self.sink)(UserEvent::Wake);
        }
    }
    /// Called by the UI once it has handled the wake.
    pub fn clear(&self) {
        self.pending.store(false, Ordering::Release);
    }
    pub fn send(&self, event: UserEvent) {
        (self.sink)(event);
    }
}

/// What goes to the shell: keys, pastes, mouse reports and the answers to a
/// program's questions, in the order sent. A thread of the session's own
/// (`pty-write-<id>`) writes them, so when a program stops reading and the
/// pty's input fills, that thread waits, never the window's or the reader's.
/// The queue is bounded: past `MOTION_ROOM` mouse motion is dropped, and past
/// `ROOM` anything more, so a program that never reads can't make Flow hold
/// its input without end.
#[derive(Clone)]
pub struct ToShell(Arc<Outbox>);

/// Queued, mouse motion goes only while this much at most waits, so a program
/// that falls behind isn't sent a burst of stale moves once it reads again.
const MOTION_ROOM: usize = 4 * 1024;
/// Queued, nothing more goes once this much waits.
const ROOM: usize = 32 * 1024 * 1024;

struct Outbox {
    queue: Mutex<Queued>,
    ready: Condvar,
    room: usize,
    motion_room: usize,
    /// Written at once instead, with no thread (tests that read it back).
    direct: Option<Mutex<Box<dyn Write + Send>>>,
}

#[derive(Default)]
struct Queued {
    chunks: VecDeque<Vec<u8>>,
    /// Waiting, with what's being written now.
    bytes: usize,
    closed: bool,
}

impl ToShell {
    /// Writes to `writer` from a thread named `name`.
    fn spawn(writer: Box<dyn Write + Send>, name: String) -> ToShell {
        ToShell::spawn_with(writer, name, ROOM, MOTION_ROOM)
    }

    fn spawn_with(mut writer: Box<dyn Write + Send>, name: String, room: usize, motion_room: usize) -> ToShell {
        let outbox = Arc::new(Outbox { queue: Mutex::default(), ready: Condvar::new(), room, motion_room, direct: None });
        let theirs = outbox.clone();
        if let Err(err) = std::thread::Builder::new().name(name).spawn(move || theirs.drain(&mut *writer)) {
            log::warn!("no thread to write to the shell: {err}");
            outbox.queue.lock().closed = true;
        }
        ToShell(outbox)
    }

    /// Writes straight to `writer`, for tests that read it back at once.
    #[cfg(test)]
    fn direct(writer: Box<dyn Write + Send>) -> ToShell {
        ToShell(Arc::new(Outbox { queue: Mutex::default(), ready: Condvar::new(), room: ROOM, motion_room: MOTION_ROOM, direct: Some(Mutex::new(writer)) }))
    }

    /// Queues `bytes` for the shell and returns at once; false if there was
    /// no room for them.
    pub fn send(&self, bytes: &[u8]) -> bool {
        self.push(bytes, self.0.room)
    }

    /// Queues mouse motion, which is dropped while the shell is behind.
    pub fn send_motion(&self, bytes: &[u8]) -> bool {
        self.push(bytes, self.0.motion_room)
    }

    fn push(&self, bytes: &[u8], room: usize) -> bool {
        if bytes.is_empty() {
            return true;
        }
        if let Some(writer) = &self.0.direct {
            let mut w = writer.lock();
            return w.write_all(bytes).and_then(|_| w.flush()).is_ok();
        }
        let mut q = self.0.queue.lock();
        if q.closed || q.bytes + bytes.len() > room {
            log::debug!("{} bytes for the shell dropped, {} waiting", bytes.len(), q.bytes);
            return false;
        }
        q.bytes += bytes.len();
        q.chunks.push_back(bytes.to_vec());
        drop(q);
        self.0.ready.notify_one();
        true
    }

    /// Drops what's waiting and lets the thread end once it's done with what
    /// it's writing: the tab is closing.
    fn close(&self) {
        let mut q = self.0.queue.lock();
        q.closed = true;
        q.chunks.clear();
        drop(q);
        self.0.ready.notify_one();
    }
}

impl Outbox {
    fn drain(&self, writer: &mut dyn Write) {
        loop {
            let chunk = {
                let mut q = self.queue.lock();
                loop {
                    if q.closed {
                        return;
                    }
                    if let Some(chunk) = q.chunks.pop_front() {
                        break chunk;
                    }
                    self.ready.wait(&mut q);
                }
            };
            if let Err(err) = writer.write_all(&chunk).and_then(|_| writer.flush()) {
                log::debug!("pty write failed: {err}");
            }
            let mut q = self.queue.lock();
            q.bytes = q.bytes.saturating_sub(chunk.len());
        }
    }
}

/// The colors a tab answers programs' questions with (OSC 4, 10, 11, 12):
/// its own theme's, which may not be the one the window wears.
type TabInk = Arc<Mutex<crate::render::palette::Ink>>;

/// A tab's ink to begin with: the one the window wears.
fn tab_ink() -> TabInk {
    Arc::new(Mutex::new(crate::render::palette::ink()))
}

/// Receives callbacks from the emulator (replies to queries, title changes, ...).
#[derive(Clone)]
pub struct EventProxy {
    ink: TabInk,
    writer: ToShell,
    state: Arc<Mutex<ShellState>>,
    size: Arc<Mutex<WindowSize>>,
    waker: Waker,
}

/// DA1's answer, in the terminal and down the pipe: a VT220 (62) that draws
/// sixels (4) in ANSI colors (22).
const DA1: &str = "\x1b[?62;4;22c";
/// What alacritty answers DA1 with itself: a VT102, which Flow's answer
/// takes the place of.
const VT102: &str = "\x1b[?6c";
/// What alacritty answers DECRQM with for the sixel modes, which it
/// doesn't know: Flow answers them itself (`Marker::ModeQuery`).
const UNKNOWN_MODES: [&str; 2] = ["\x1b[?80;0$y", "\x1b[?8452;0$y"];

impl EventListener for EventProxy {
    fn send_event(&self, event: TermEvent) {
        match event {
            TermEvent::PtyWrite(text) => {
                if UNKNOWN_MODES.contains(&text.as_str()) {
                    return;
                }
                let text = if text == VT102 { DA1 } else { &text };
                self.writer.send(text.as_bytes());
            }
            TermEvent::Title(title) => {
                self.state.lock().title = title;
                self.waker.wake();
            }
            TermEvent::ResetTitle => {
                self.state.lock().title.clear();
                self.waker.wake();
            }
            TermEvent::ClipboardStore(_, text) => self.waker.send(UserEvent::Clipboard(text)),
            TermEvent::ColorRequest(index, reply) => {
                use crate::render::palette;
                let rgb = palette::answered(index, *self.ink.lock());
                let text = reply(Rgb { r: rgb[0], g: rgb[1], b: rgb[2] });
                self.writer.send(text.as_bytes());
            }
            TermEvent::TextAreaSizeRequest(reply) => {
                let text = reply(*self.size.lock());
                self.writer.send(text.as_bytes());
            }
            TermEvent::Bell => {
                self.state.lock().bell_at = Some(Instant::now());
                self.waker.wake();
            }
            TermEvent::Wakeup | TermEvent::Exit | TermEvent::ChildExit(_) => self.waker.wake(),
            _ => {}
        }
    }
}

pub struct Emulator {
    pub term: Term<EventProxy>,
    processor: Processor<StdSyncHandler>,
    banner: Option<Banner>,
    /// The image ids of the models placed here (`place_model`), whose cells
    /// are Flow's own as the logo's are, so kept through ConPTY's repaints
    /// too.
    models: Vec<u32>,
    /// A repaint ConPTY is about to send, with what it may blank of Flow's
    /// own cells.
    repaint: Option<Repaint>,
    /// A program has made a line double here (`crate::line_size`), so the
    /// emulator keeps such a line's right margin at its half
    /// ([`line_size::Margins`], which every letter goes through, for the
    /// Mac's spacing marks), and a resize looks after the lines' marks.
    sized: bool,
    /// DECSDM (`CSI ? 80 h`): sixels go at the screen's top left, the
    /// cursor staying and nothing scrolling (`osc::Modes`).
    sixel_display: bool,
    /// sixelScrollsRight (`CSI ? 8452 h`): the cursor goes right of a
    /// sixel's last row.
    sixel_right: bool,
    /// The output comes through ConPTY (Windows), which keeps the console's
    /// cursor itself and moves Flow's to it: a picture placed at the cursor
    /// rises over the lines above rather than reaching down past it
    /// ([`place_image`]).
    conpty: bool,
}

/// The startup logo, which Flow draws into the grid itself
/// ([`Session::show_banner`]), so on Windows ConPTY doesn't know it's there.
/// ConPTY leaves the rows above where it started alone until a program draws
/// over them on the alternate screen. From then on the whole screen is its
/// own, and the repaint after leaving the alternate screen, or after a
/// resize, blanks the logo's rows from the top. So when one of those repaints
/// is due, the logo's rows are noted, and those its frame (`?25l` … `?25h`)
/// leaves blank go back. A clear (`cls`) comes without one and takes the
/// logo away, as it should. A model's cells are kept the same way, cell by
/// cell, since it sits among a program's text (`docs/design/models-through-conpty.md`).
struct Banner {
    image: u32,
    placement: u32,
}

/// A repaint ConPTY is about to send.
struct Repaint {
    /// The logo's rows on screen, by line, when it became due.
    rows: Vec<(usize, Row<Cell>)>,
    /// The cells showing a model on the screen in use then, by line and
    /// column.
    cells: Vec<(usize, usize, Cell)>,
    /// Whether that screen was the alternate one.
    alternate: bool,
    /// Whether its frame has begun.
    begun: bool,
}

impl Emulator {
    fn new(term: Term<EventProxy>) -> Self {
        Self { term, processor: Processor::new(), banner: None, models: Vec::new(), repaint: None, sized: false, sixel_display: false, sixel_right: false, conpty: false }
    }

    /// Carries out the terminal's output: once a program has made a line
    /// double, with that line's right margin at its half, and on the Mac
    /// with each spacing mark on the letter before (`crate::combining`).
    fn run(&mut self, bytes: &[u8]) {
        self.processor.advance(&mut line_size::Margins::new(&mut self.term, self.sized), bytes);
    }

    /// Absolute line of the cursor (history lines + screen row).
    pub fn cursor_abs_line(&self) -> i64 {
        let grid = self.term.grid();
        grid.history_size() as i64 + grid.cursor.point.line.0 as i64
    }

    /// Resizes the grid as a session does once ConPTY has its size on
    /// Windows, else as the emulator does, for the tests.
    #[cfg(test)]
    fn resize(&mut self, grid: GridSize) {
        self.resize_for(grid, cfg!(windows).then_some(true));
    }

    /// Resizes the grid: as the emulator does where `conpty` is none (not
    /// on Windows), else as ConPTY resizes its own screen
    /// ([`resize_from_top`]), `conpty` saying whether ConPTY will repaint
    /// what's below the cursor, which it does only once it has its size. A
    /// new width would cut the double lines' marks off their ends or leave
    /// them short of them, so they're lifted through it
    /// (`crate::line_size`), on the screen in use and, under a program's
    /// alternate screen, on the main one, which is rewrapped out of sight.
    /// The rewrap goes by the screen's width, so the main screen's double
    /// lines are laid out for it first: one whose text flows rewraps with
    /// what wrapped from it by its own width, and the rest are cut as the
    /// alternate screen's lines are.
    fn resize_for(&mut self, grid: GridSize, conpty: Option<bool>) {
        let cols = grid.cols;
        let lifted = (self.sized && cols != self.term.columns()).then(|| {
            let alternate = self.term.mode().contains(TermMode::ALT_SCREEN).then(|| line_size::lift(self.term.grid_mut(), cols, false));
            let mut main = None;
            graphics::on_main_screen(&mut self.term, |grid| main = Some(line_size::lift(grid, cols, true)));
            (main, alternate)
        });
        match conpty {
            Some(repaints) => resize_from_top(&mut self.term, grid, !repaints),
            None => self.term.resize(grid),
        }
        if let Some((main, alternate)) = lifted {
            if let Some(lifted) = alternate {
                line_size::settle(self.term.grid_mut(), lifted);
            }
            if let Some(lifted) = main {
                graphics::on_main_screen(&mut self.term, |grid| line_size::settle(grid, lifted));
            }
        }
        // A narrower double line has fewer columns for the cursor.
        line_size::clamp_cursor(self.term.grid_mut());
        if cfg!(windows) {
            self.repaint_due();
        }
    }

    /// Flushes a synchronized update that never got its end marker.
    pub fn check_sync_timeout(&mut self) {
        if let Some(deadline) = self.processor.sync_timeout().sync_timeout() {
            if Instant::now() >= deadline {
                self.processor.stop_sync(&mut line_size::Margins::new(&mut self.term, self.sized));
            }
        }
    }

    /// What's typed at the prompt: from `start` (the line, counted from the
    /// top of the scrollback, and the column where the prompt ended) to the
    /// end of the cursor's line and of the lines that line wraps onto.
    /// Nothing if the cursor isn't at or below the prompt's end.
    pub fn input_from(&self, (start, col): (i64, usize)) -> String {
        let grid = self.term.grid();
        let (hist, rows, cols) = (grid.history_size() as i64, grid.screen_lines() as i64, grid.columns());
        let cursor = self.cursor_abs_line();
        if start < 0 || start > cursor || cursor - start > rows {
            return String::new();
        }
        let wraps = |abs: i64| line_size::wraps(&grid[Line((abs - hist) as i32)]);
        let mut end = cursor;
        while end + 1 - hist < rows && wraps(end) {
            end += 1;
        }
        let mut out = String::new();
        for abs in start..=end {
            let row = &grid[Line((abs - hist) as i32)];
            // A double line's text goes no further than its half.
            let shown = line_size::of_row(row).shown(cols);
            for c in if abs == start { col.min(shown) } else { 0 }..shown {
                let cell = &row[Column(c)];
                if cell.flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER) {
                    continue;
                }
                out.push(if cell.c == graphics::PLACEHOLDER { ' ' } else { cell.c });
                if let Some(zw) = cell.zerowidth() {
                    out.extend(zw.iter());
                }
            }
            if !wraps(abs) {
                // Trailing blanks are the row's, not the command's.
                out.truncate(out.trim_end().len());
                if abs < end {
                    out.push('\n');
                }
            }
        }
        out
    }

    /// How many letters the shell's cursor is to move along the line being
    /// typed at the prompt (less than none is left) for a click at `target`
    /// (a grid point, its line below 0 in the scrollback) to put it there.
    /// None unless the click is on the line the cursor is on, as the rows a
    /// wrap joins make it, and that's the line the prompt's input starts on
    /// (`start`, its end mark's place): the active line alone, never the
    /// scrollback's or what a program writes. A click on the prompt before
    /// the input goes to its start, one past the text to its end, and one on
    /// a wide letter's right half to that letter. A double line has none.
    pub fn letters_to(&self, (start, col): (i64, usize), target: Point) -> Option<isize> {
        let grid = self.term.grid();
        let (hist, rows, cols) = (grid.history_size() as i64, grid.screen_lines() as i64, grid.columns());
        let row = |abs: i64| &grid[Line((abs - hist) as i32)];
        let cursor = (self.cursor_abs_line(), grid.cursor.point.column.0);
        let (mut first, mut last) = (cursor.0, cursor.0);
        while first > 0 && line_size::wraps(row(first - 1)) {
            first -= 1;
        }
        while last + 1 < hist + rows && line_size::wraps(row(last)) {
            last += 1;
        }
        let at = hist + target.line.0 as i64;
        if !(first..=last).contains(&start) || !(first..=last).contains(&at) {
            return None;
        }
        if (first..=last).any(|abs| line_size::of_row(row(abs)) != LineSize::Single) {
            return None;
        }
        // Places along the line, a row after another.
        let place = |(abs, c): (i64, usize)| (abs - first) as usize * cols + c.min(cols - 1);
        let cell = |p: usize| &row(first + (p / cols) as i64)[Column(p % cols)];
        let spacer = |p: usize| cell(p).flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER);
        let (from, start) = (place(cursor), place((start, col)));
        let end = (first..=last).count() * cols;
        let text_end = (start..end).rev().find(|&p| !spacer(p) && (cell(p).c != ' ' || cell(p).zerowidth().is_some())).map_or(start, |p| p + 1);
        let mut to = place((at, target.column.0)).max(start).min(text_end.max(from));
        if to > start && cell(to.min(end - 1)).flags.contains(Flags::WIDE_CHAR_SPACER) {
            to -= 1;
        }
        let letters = |range: std::ops::Range<usize>| range.filter(|&p| !spacer(p)).count() as isize;
        Some(if to >= from { letters(from..to) } else { -letters(to..from) })
    }

    /// Where `place` (a line counted from the top of the scrollback, and a
    /// column) is in the text, as a rewrap keeps it: how many lines (the
    /// rows a wrap joins being one) it is before the cursor's, and how far
    /// into its own line it is. A resize rewraps the main screen (on
    /// Windows as ConPTY repaints it), which moves its rows, and the
    /// shell's prompt is drawn once, before: [`Emulator::place_from_text`]
    /// finds it again after. None below the cursor's line, and on the
    /// alternate screen, which holds no prompt.
    pub fn place_in_text(&self, (abs, col): (i64, usize)) -> Option<(usize, usize)> {
        if self.term.mode().contains(TermMode::ALT_SCREEN) {
            return None;
        }
        let grid = self.term.grid();
        let (hist, cols) = (grid.history_size() as i64, grid.columns());
        let cursor = self.cursor_abs_line();
        if !(0..=cursor).contains(&abs) {
            return None;
        }
        let row = |abs: i64| &grid[Line((abs - hist) as i32)];
        let mut first = abs;
        while first > 0 && line_size::wraps(row(first - 1)) {
            first -= 1;
        }
        let back = (first..cursor).filter(|&r| !line_size::wraps(row(r))).count();
        Some((back, (abs - first) as usize * cols + col))
    }

    /// The place [`Emulator::place_in_text`] gave, after a rewrap: the line
    /// `back` lines before the cursor's, and `into` that line's letters
    /// along its rows. None where it's gone off the top of the scrollback.
    pub fn place_from_text(&self, (back, into): (usize, usize)) -> Option<(i64, usize)> {
        let grid = self.term.grid();
        let (hist, rows, cols) = (grid.history_size() as i64, grid.screen_lines() as i64, grid.columns());
        let row = |abs: i64| &grid[Line((abs - hist) as i32)];
        let start = |mut abs: i64| {
            while abs > 0 && line_size::wraps(row(abs - 1)) {
                abs -= 1;
            }
            abs
        };
        let mut first = start(self.cursor_abs_line());
        for _ in 0..back {
            if first == 0 {
                return None;
            }
            first = start(first - 1);
        }
        let abs = (first + (into / cols) as i64).min(hist + rows - 1);
        Some((abs, into % cols))
    }

    /// Drops the scrollback and moves the cursor's line to the top of the
    /// screen, like Terminal's Clear to Start. Only the screen changes: the
    /// shell is not involved, so a half-typed command stays as it is.
    pub fn clear_to_cursor_line(&mut self) {
        let line = self.term.grid().cursor.point.line.0.max(0) as usize;
        if line > 0 && !self.term.mode().contains(TermMode::ALT_SCREEN) {
            // Scrolling the whole screen moves the lines above into history.
            Handler::scroll_up(&mut self.term, line);
            Handler::move_up(&mut self.term, line);
        }
        self.term.grid_mut().clear_history();
        self.term.selection = None;
    }

    /// Text of absolute lines `start..end`, joining soft-wrapped rows; a
    /// double line's text goes as far as its half (`crate::line_size`).
    pub fn text_between(&self, start: i64, end: i64) -> String {
        let grid = self.term.grid();
        let hist = grid.history_size() as i64;
        let rows = grid.screen_lines() as i64;
        let cols = grid.columns();
        let mut out = String::new();
        for abs in start.max(0)..end {
            let l = abs - hist;
            if l < -hist || l >= rows {
                continue;
            }
            let row = &grid[Line(l as i32)];
            let mut line = String::new();
            for c in 0..line_size::of_row(row).shown(cols) {
                let cell = &row[Column(c)];
                // A wide letter's second half, and the padding where one
                // didn't fit at a line's end.
                if cell.flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER) {
                    continue;
                }
                // An image's cells read as blanks.
                if cell.c == graphics::PLACEHOLDER {
                    line.push(' ');
                    continue;
                }
                line.push(cell.c);
                if let Some(zw) = cell.zerowidth() {
                    line.extend(zw.iter());
                }
            }
            if line_size::wraps(row) {
                out.push_str(&line);
            } else {
                out.push_str(line.trim_end());
                out.push('\n');
            }
        }
        out
    }

    /// The rows in view, as a screen reader reads them (`crate::access`):
    /// each row's letters as [`Emulator::text_between`] reads them, each
    /// with the marks combined on it, its column and its cells (a double
    /// line's twice as wide), and whether the row runs on into the next;
    /// and where the cursor is among them, while it shows in view. The
    /// cursor's row keeps the blanks before the cursor, which a prompt
    /// ends with, so the caret is where typing goes.
    pub(crate) fn rows_in_view(&self) -> (Vec<crate::access::Row>, Option<(usize, usize)>) {
        let grid = self.term.grid();
        let (rows, cols) = (grid.screen_lines(), grid.columns());
        let offset = grid.display_offset() as i32;
        let cursor = grid.cursor.point;
        let cursor_row = (self.term.mode().contains(TermMode::SHOW_CURSOR) && cursor.line.0 + offset < rows as i32).then(|| (cursor.line.0 + offset) as usize);
        let mut out = Vec::with_capacity(rows);
        let mut caret = None;
        for i in 0..rows {
            let row = &grid[Line(i as i32 - offset)];
            let size = line_size::of_row(row);
            let wide = if size.doubled() { 2 } else { 1 };
            let mut line = crate::access::Row { wraps: line_size::wraps(row), ..Default::default() };
            // The letters before the cursor's column, on its row.
            let mut before = None;
            for c in 0..size.shown(cols) {
                let cell = &row[Column(c)];
                if Some(i) == cursor_row && c == cursor.column.0 {
                    before = Some(line.letters.len());
                }
                if cell.flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER) {
                    continue;
                }
                let start = line.text.len();
                // An image's cells read as blanks.
                line.text.push(if cell.c == graphics::PLACEHOLDER { ' ' } else { cell.c });
                if let Some(zw) = cell.zerowidth().filter(|_| cell.c != graphics::PLACEHOLDER) {
                    line.text.extend(zw.iter());
                }
                let cells = if cell.flags.contains(Flags::WIDE_CHAR) { 2 } else { 1 };
                line.letters.push(crate::access::Letter { bytes: (line.text.len() - start).min(255) as u8, col: (c * wide) as u16, cells: cells * wide as u8 });
            }
            // Past the row's letters (a double line's hidden half), the
            // caret is at their end.
            let before = (Some(i) == cursor_row).then(|| before.unwrap_or(line.letters.len()));
            if let Some(before) = before {
                caret = Some((i, before));
            }
            // A row that runs on into the next keeps its blanks, which are
            // its line's; the others' go, after the cursor on its row.
            if !line.wraps {
                trim_blanks(&mut line, before.unwrap_or(0));
            }
            out.push(line);
        }
        (out, caret)
    }

    /// Moves the selection's end to `point`, on its `side`, as a drag does.
    /// True where that changed it (another cell, or the other half of one),
    /// which is when the highlight needs drawing again.
    pub fn drag_selection(&mut self, point: Point, side: alacritty_terminal::index::Side) -> bool {
        let Some(selection) = self.term.selection.as_mut() else { return false };
        let was = selection.clone();
        selection.update(point, side);
        *selection != was
    }

    /// The selection's text, for copying: a double line's as far as its half
    /// (`line_size::selection_to_string`).
    pub fn selection_to_string(&self) -> Option<String> {
        if self.sized {
            line_size::selection_to_string(&self.term)
        } else {
            self.term.selection_to_string()
        }
    }

    /// The last `max_lines` non-empty-terminated lines up to the cursor.
    pub fn recent_text(&self, max_lines: usize) -> String {
        let end = self.cursor_abs_line() + 1;
        let start = (end - max_lines as i64).max(0);
        let text = self.text_between(start, end);
        text.trim_matches('\n').to_string()
    }

    /// Notes what of Flow's own ConPTY is about to repaint over: the startup
    /// logo's rows on screen (see [`Banner`]), and on Windows the cells of
    /// the models shown on the screen in use. Once none of the logo's rows
    /// are left, it's gone for good.
    fn repaint_due(&mut self) {
        let alternate = self.term.mode().contains(TermMode::ALT_SCREEN);
        let grid = self.term.grid();
        let (lines, cols) = (grid.screen_lines(), grid.columns());
        // The logo is on the main screen; leaving the other brings a repaint.
        let logo = self.banner.as_ref().filter(|_| !alternate).map(|b| (b.image, b.placement));
        let rows: Vec<(usize, Row<Cell>)> = logo.map_or_else(Vec::new, |logo| {
            (0..lines)
                .filter_map(|l| {
                    let row = &grid[Line(l as i32)];
                    let shows = (0..cols).any(|c| graphics::read_placeholder(&row[Column(c)]).is_some_and(|m| (m.id, m.placement) == logo));
                    shows.then(|| (l, row.clone()))
                })
                .collect()
        });
        let mut cells = Vec::new();
        if self.conpty && !self.models.is_empty() {
            for l in 0..lines {
                let row = &grid[Line(l as i32)];
                for c in 0..cols {
                    if graphics::read_placeholder(&row[Column(c)]).is_some_and(|m| self.models.contains(&m.id)) {
                        cells.push((l, c, row[Column(c)].clone()));
                    }
                }
            }
        }
        if logo.is_some() && rows.is_empty() {
            self.banner = None;
        }
        self.repaint = (!rows.is_empty() || !cells.is_empty()).then_some(Repaint { rows, cells, alternate, begun: false });
    }

    /// Follows ConPTY's frames while a repaint is due: the first to begin is
    /// the repaint, and as it ends, the logo's rows it blanked go back, and
    /// the models' cells it blanked, of those still placed (one placed again
    /// or deleted meanwhile has another image or none), on the screen they
    /// were noted on.
    fn frame(&mut self, begins: bool) {
        let Some(repaint) = &mut self.repaint else { return };
        if begins || !repaint.begun {
            repaint.begun |= begins;
            return;
        }
        let Some(Repaint { rows, cells, alternate, .. }) = self.repaint.take() else { return };
        if self.term.mode().contains(TermMode::ALT_SCREEN) != alternate {
            return;
        }
        let models = &self.models;
        let grid = self.term.grid_mut();
        let (lines, cols) = (grid.screen_lines(), grid.columns());
        for (l, row) in rows {
            let line = Line(l as i32);
            if l < lines && row.len() == cols && (0..cols).all(|c| grid[line][Column(c)].c == ' ') {
                grid[line] = row;
            }
        }
        for (l, c, cell) in cells {
            let placed = graphics::read_placeholder(&cell).is_some_and(|m| models.contains(&m.id));
            if placed && l < lines && c < cols && grid[Line(l as i32)][Column(c)].c == ' ' {
                grid[Line(l as i32)][Column(c)] = cell;
            }
        }
    }
}

/// Outcome of a command Claude ran in the terminal.
#[derive(Debug, Clone)]
pub struct CommandResult {
    pub output: String,
    pub exit_code: Option<i32>,
    /// The command was still running when we stopped waiting.
    pub still_running: bool,
}

#[derive(Debug)]
pub struct Tracker {
    pub start_abs: i64,
    pub sent_at: Instant,
    pub result: Option<CommandResult>,
    /// The user typed something since the command was sent, and the last
    /// thing typed wasn't Enter. A Unix tty echoes it after the output.
    pub typed: bool,
}

/// What the command history hasn't taken yet from a tab, in the order it
/// happened.
#[derive(Debug, Clone, PartialEq)]
pub enum HistoryNote {
    /// A command started: Enter at a prompt with this on its line (or the
    /// assistant's command, sent whole), in this folder.
    Entered { command: String, cwd: Option<String>, at: SystemTime, assistant: bool },
    /// The shell's own word for the command (OSC 633;E, with the pane's
    /// nonce: no program's), which beats the grid's.
    Exact(String),
    /// What the screen showed at Enter names nothing, and the row it made
    /// goes: the shell's history left the line out (bash's HISTCONTROL or
    /// HISTIGNORE, as its hook says) and that wasn't the newest line it
    /// holds, or the hook, which says each line it runs, said nothing of
    /// this one, which ran nothing it saw (Enter alone, a comment). A
    /// program's output while the shell waited at its prompt (a
    /// background job's) could have drawn a prompt with words of its own
    /// there.
    LeftOut,
    /// The command finished, with its exit code if the shell said.
    Done(Option<i32>),
}

/// How many notes wait for the history at most: the oldest go first. The
/// app takes them as it draws, which a minimized window may not do for a
/// while.
const MAX_NOTES: usize = 256;

/// A command the shell finished: how it ended, how long it ran, and the
/// command itself where the shell's marks said what was run.
#[derive(Debug, Clone, PartialEq)]
pub struct Finished {
    pub exit: Option<i32>,
    pub secs: f32,
    pub command: Option<String>,
}

/// The last command that failed in a tab, with the end of what it printed:
/// what Ask why hands the assistant.
#[derive(Debug, Clone, PartialEq)]
pub struct Failure {
    pub command: String,
    pub exit: i32,
    pub output: String,
}

/// How much of a failed command's output Ask why keeps: its last lines, at
/// most so many bytes of them.
const FAILURE_LINES: usize = 200;
const FAILURE_BYTES: usize = 16_000;

/// Takes the blanks off a row's end, as copying does, but none of its first
/// `keep` letters: the cursor's row keeps those before the cursor.
fn trim_blanks(row: &mut crate::access::Row, keep: usize) {
    while row.letters.len() > keep && row.text.ends_with(' ') && row.letters.last().is_some_and(|l| l.bytes == 1) {
        row.letters.pop();
        row.text.pop();
    }
}

/// The end of `text`: its last [`FAILURE_LINES`] lines, and of them the last
/// [`FAILURE_BYTES`] bytes, cut where a letter starts.
fn tail_of(text: &str) -> String {
    let lines: Vec<&str> = text.trim_end().lines().collect();
    let kept = lines[lines.len().saturating_sub(FAILURE_LINES)..].join("\n");
    let mut cut = kept.len().saturating_sub(FAILURE_BYTES);
    while !kept.is_char_boundary(cut) {
        cut += 1;
    }
    kept[cut..].to_string()
}

/// A program's question about Flow's settings (OSC 7732), and where its
/// answer goes: into the terminal, or (`reply`) down Flow's pipe, whose
/// thread waits for it.
#[derive(Debug)]
pub struct SettingsAsk {
    pub request: String,
    pub reply: Option<crossbeam_channel::Sender<String>>,
}

/// The most settings questions a tab keeps waiting for the UI.
const MAX_SETTINGS_ASKS: usize = 16;

/// Keeps `ask` for the UI, unless as many wait already.
fn queue_settings(state: &Mutex<ShellState>, ask: SettingsAsk) {
    let mut st = state.lock();
    if st.settings.len() < MAX_SETTINGS_ASKS {
        st.settings.push(ask);
    }
}

#[derive(Debug, Default)]
pub struct ShellState {
    pub title: String,
    pub cwd: Option<String>,
    /// The shell has emitted integration markers at least once.
    pub integrated: bool,
    /// Sitting at a prompt waiting for input (best effort): from a prompt
    /// drawn (133;B) until a command is entered or starts (133;C). True
    /// before the shell says anything, false from its first mark until its
    /// first prompt.
    pub at_prompt: bool,
    pub last_exit: Option<i32>,
    pub tracker: Option<Tracker>,
    pub last_output_at: Option<Instant>,
    pub bell_at: Option<Instant>,
    /// Letters an old machine's start screen has typed since the app last
    /// looked, for a teletype's clatter.
    pub typed_out: u32,
    /// A start screen being typed shows the rest at once (efficient mode
    /// turned on while it types).
    pub hurry: bool,
    /// Lines it has ended since, for a teletype's carriage running back.
    pub typed_lines: u32,
    /// Lines a running command has written since the app last looked, for
    /// an old machine that printed them (a teletype's punch, a printer).
    pub output_lines: u32,
    /// Whether it began since, for a disk drive seeking as the machine
    /// starts (and again once the key its `{wait}` waits for comes).
    pub booting: bool,
    /// A start screen waits at its `{wait}` (`looks::KEY_WAIT`) for the
    /// tab's first key, which goes to it rather than to the shell.
    pub awaiting_key: bool,
    /// The tab is closing, so a start screen waiting for a key stops.
    pub closing: bool,
    /// The color a program washes the glass in while it keeps the alternate
    /// screen (OSC 7731's `fx=tint`).
    pub wash: Option<crate::osc::Wash>,
    /// When the command now running started. zsh says so; for the other
    /// shells it's Enter at a prompt with something typed (best effort).
    pub running_since: Option<Instant>,
    /// Something was typed at the prompt since it was drawn.
    pub input_pending: bool,
    /// When the last command ended (133;D), until the prompt after it is
    /// drawn (133;B) or another starts: the shell is drawing that prompt,
    /// which a theme reading git can take a moment over, running nothing
    /// though not at its prompt yet ([`ShellState::drawing_prompt`]).
    pub prompt_due: Option<Instant>,
    /// The last command that finished, until the UI takes it.
    pub finished: Option<Finished>,
    /// The last command that ended with a code, and its code, kept (not
    /// taken) for the tab sidebar's line, which says it until the next runs.
    pub ended: Option<(String, i32)>,
    /// The command running now, as typed at the prompt or as the shell's
    /// hook says it ran it (OSC 633;E with the pane's nonce), and the line
    /// its output starts on, counted from the top of the scrollback.
    pub command: Option<String>,
    pub output_from: Option<i64>,
    /// The last command that failed, until the next one starts.
    pub failed: Option<Failure>,
    /// What the running program last said about its progress (OSC 9;4), and when.
    pub progress: Option<(Progress, Instant)>,
    /// Effects programs asked for (OSC 7731), until the UI takes them. At
    /// most [`MAX_EFFECTS`]: the oldest go first.
    pub effects: Vec<AskedFx>,
    /// What programs asked about Flow's settings (OSC 7732), until the UI
    /// answers. At most [`MAX_SETTINGS_ASKS`]: past that they're dropped.
    pub settings: Vec<SettingsAsk>,
    /// Where typing starts at the prompt the shell last drew: the line
    /// (counted from the top of the scrollback) and column of its end mark.
    pub input_start: Option<(i64, usize)>,
    /// The line (counted from the top of the scrollback) where the prompt
    /// the shell last drew began (133;A), which a command run there marks
    /// (`crate::marks`).
    pub prompt_line: Option<i64>,
    /// What the command history hasn't taken yet.
    pub history: Vec<HistoryNote>,
    /// Sounds programs asked for, until the UI takes them. At most
    /// [`MAX_SOUNDS`]: the oldest go first.
    pub sounds: Vec<SoundRequest>,
    /// The pane's nonce, which only the shell's hook knows
    /// (`shell::NONCE`): a command line said without it (OSC 633;E) is a
    /// program's output that only looks like the hook's, and changes
    /// nothing. Empty takes none.
    nonce: String,
    /// The hook has said a command done (133;D) with the nonce, as zsh's,
    /// bash's and PowerShell's do from their first prompt on: from then on
    /// a "done" without it, and a prompt (133;A, 133;B) drawn while a
    /// command runs, is a program's output that only looks like the hook's
    /// ([`ShellState::forged`]). A pane whose hook never does (cmd, ash, a
    /// bash whose PROMPT_COMMAND is read-only, a zsh whose startup files
    /// `exec`ed another shell) takes every mark, as before, and into such a
    /// bash or zsh Flow types no letter past ASCII
    /// ([`ShellState::letters_may_be_keys`]).
    guarded: bool,
    /// The hook has said a command's start (133;C) with the nonce, as zsh's
    /// preexec and bash's PS0 say every one: a command runs from its 133;C.
    /// Else (PowerShell, which says none, and bash 3.2's DEBUG trap, which
    /// misses a line that starts with a subshell) from Enter at the prompt
    /// with something typed ([`ShellState::running_since`]).
    starts_said: bool,
    /// A 133;C has come since the hook's last "done".
    started: bool,
    /// `command` was read off the screen at Enter, and the hook hasn't
    /// said this line yet: until it does, what a program printed at the
    /// prompt (a background job's prompt with words of its own) may stand
    /// for it.
    command_read: bool,
    /// The hook has said a command line with the nonce (OSC 633;E), as
    /// zsh's preexec, bash's PS0 or DEBUG trap and PowerShell's prompt say
    /// each line they run: from then on, a line read off the screen that
    /// the hook says nothing of by its "done" ran nothing it saw (an empty
    /// line, a comment, one the shell couldn't read, or one bash 3.2's
    /// trap passes over), and names nothing.
    lines_said: bool,
    /// bash's line editor at this prompt may take each byte past ASCII as
    /// Meta and a key, as the hook said before it (`Marker::MetaKeys`):
    /// until the next "done", Flow types no letter past ASCII into it.
    meta_keys: bool,
}

/// How many effects wait for the UI at most.
pub const MAX_EFFECTS: usize = 8;
/// How many sounds wait for the UI at most.
pub const MAX_SOUNDS: usize = 32;

/// An effect a program asked for, and where its screen was then, so it
/// still lands on its line if the screen scrolls before the next frame.
/// That holds while the scrollback has room: once it's full, the count of
/// lines above the screen stops growing. The alternate screen keeps no
/// scrollback, so there it's the screen's own rows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AskedFx {
    pub request: FxRequest,
    /// The line the screen's top row was, counted from the top of the
    /// scrollback.
    pub top: i64,
    /// Asked on the alternate screen: shown on the other, it's let go.
    pub alt: bool,
}

impl ShellState {
    /// Bytes on their way to the shell: at a prompt, Enter after something
    /// was typed (or recalled) starts a command.
    pub fn sent(&mut self, bytes: &[u8]) {
        if !self.integrated {
            return;
        }
        if bytes.contains(&b'\r') {
            let typed_now = bytes.iter().any(|&b| b != b'\r' && b != b'\n');
            if self.at_prompt && (self.input_pending || typed_now) {
                self.running_since = Some(Instant::now());
                self.failed = None;
            }
            self.at_prompt = false;
            self.input_pending = false;
        } else if self.at_prompt {
            self.input_pending = true;
        }
    }

    /// Keeps `note` for the command history.
    pub fn note(&mut self, note: HistoryNote) {
        if self.history.len() >= MAX_NOTES {
            self.history.remove(0);
        }
        self.history.push(note);
    }

    /// Whether the shell is drawing the prompt after a command that ended
    /// less than `within` ago: from its 133;D to its 133;B it runs nothing,
    /// yet it isn't at its prompt, so it counts as the user's
    /// ([`Session::taken`]) and may not have said its folder (bash and zsh
    /// say it between the two). Past `within` the prompt is given up on.
    pub fn drawing_prompt(&self, within: Duration) -> bool {
        self.prompt_due.is_some_and(|t| t.elapsed() < within)
    }

    /// The shell's "done" marker: the command running, if one was, is over.
    fn command_done(&mut self, code: Option<i32>, command: Option<String>) {
        self.last_exit = code;
        if let (Some(command), Some(code)) = (command.as_ref(), code) {
            self.ended = Some((command.clone(), code));
        }
        if let Some(start) = self.running_since.take() {
            self.finished = Some(Finished { exit: code, secs: start.elapsed().as_secs_f32(), command });
        }
        self.progress = None;
    }

    /// Whether the line editor at the shell's prompt may take a letter past
    /// ASCII as Meta and a key, not a letter, as far as Flow can tell
    /// (`by_nonce`: the pane's shell is bash or zsh, whose hooks say a
    /// command done with the nonce before every prompt, the first too):
    /// the shell has never shown its hook's prompt, so what reads the keys
    /// may be one the hook never reached (one its startup files `exec`ed);
    /// bash's hook said this prompt may (`meta_keys`); or the hook has
    /// never said a command done with the nonce (`guarded`): so it isn't
    /// running there, and a prompt Flow saw was a program's forgery (zsh's
    /// startup files `exec`ed bash), or bash's PROMPT_COMMAND is read-only
    /// and readline may have put a C locale's settings back for good.
    fn letters_may_be_keys(&self, by_nonce: bool) -> bool {
        !self.integrated || self.meta_keys || (by_nonce && !self.guarded)
    }

    /// Whether `marker` is a program's output that only looks like the
    /// shell's hook (a file being `cat`ed, a remote host's over ssh), which
    /// then changes nothing, and noting what the hook's own say of it. Only
    /// the hook knows the pane's nonce: a command line said without it is
    /// never the hook's, nor, once the hook has said a "done" with it, is a
    /// "done" without it, nor a prompt drawn while a command runs, which
    /// would otherwise have Enter read what the program drew after it as
    /// a command, for the history, Ask why and the notifications, and have
    /// Flow type into the program as if it were the shell.
    fn forged(&mut self, marker: &Marker) -> bool {
        let own = |nonce: &str| crate::flow_command::same_key(nonce, &self.nonce);
        match marker {
            Marker::CommandLine { nonce, .. } => !own(nonce),
            Marker::CommandStart { nonce } => {
                self.starts_said |= own(nonce);
                false
            }
            Marker::CommandDone { nonce, .. } => {
                if own(nonce) {
                    self.guarded = true;
                    false
                } else {
                    self.guarded
                }
            }
            Marker::PromptStart | Marker::PromptEnd => self.guarded && (self.started || (!self.starts_said && self.running_since.is_some())),
            _ => false,
        }
    }
}

/// Spots a progress figure in a program's output: a percentage on the line
/// the cursor is on, redrawn in place with a new value (as progress bars do),
/// so a percentage that is merely printed doesn't count.
#[derive(Debug, Default)]
pub struct ProgressGuess {
    line: i64,
    percent: Option<f32>,
    /// How many times the figure changed on that line.
    changes: u8,
}

impl ProgressGuess {
    /// The figure on the cursor's line `line` (absolute), once it has moved.
    pub fn update(&mut self, line: i64, text: &str) -> Option<f32> {
        let percent = percent_in(text);
        if line != self.line {
            *self = Self { line, percent, changes: 0 };
            return None;
        }
        if percent.is_some() && percent != self.percent {
            self.changes = self.changes.saturating_add(1);
        }
        self.percent = percent.or(self.percent);
        if self.changes >= 2 { self.percent } else { None }
    }
}

/// The last percentage in `text` (`45%`, `45.2 %`), 0..=100.
fn percent_in(text: &str) -> Option<f32> {
    let bytes = text.as_bytes();
    let mut found = None;
    for (i, _) in text.match_indices('%') {
        let mut j = i;
        while j > 0 && bytes[j - 1] == b' ' {
            j -= 1;
        }
        let end = j;
        while j > 0 && (bytes[j - 1].is_ascii_digit() || bytes[j - 1] == b'.') {
            j -= 1;
        }
        if let Ok(v) = text[j..end].trim_start_matches('.').parse::<f32>() {
            if (0.0..=100.0).contains(&v) {
                found = Some(v);
            }
        }
    }
    found
}

/// The pty, whose size waits on Windows until ConPTY has heard where its
/// cursor is. ConPTY asks that first, and Flow answers once it reads the
/// shell's output, after the startup logo or a start screen is in. Resized
/// before then (as a window is at launch, briefly to the screen's size and
/// back), its first paint runs down to the foot of a screen as tall as it
/// last saw and scrolls what Flow drew off the top. So until the reader has
/// had ConPTY's first output, only the latest size is kept, for then.
struct Pty {
    master: Box<dyn MasterPty + Send>,
    heard: bool,
    held: Option<PtySize>,
}

impl Pty {
    fn resize(&mut self, size: PtySize) {
        if cfg!(windows) && !self.heard {
            self.held = Some(size);
        } else {
            let _ = self.master.resize(size);
        }
    }

    /// The reader has had the shell's first output, and Flow has answered
    /// it: a size kept until now goes to the pty.
    fn heard(&mut self) {
        self.heard = true;
        if let Some(size) = self.held.take() {
            let _ = self.master.resize(size);
        }
    }
}

pub struct Session {
    pub id: SessionId,
    pub profile: ShellProfile,
    pub emu: Arc<Mutex<Emulator>>,
    pub state: Arc<Mutex<ShellState>>,
    writer: ToShell,
    ink: TabInk,
    /// The shell's process, under which the tab's programs run.
    pid: Option<u32>,
    pty: Arc<Mutex<Pty>>,
    killer: Box<dyn ChildKiller + Send + Sync>,
    size: Arc<Mutex<WindowSize>>,
    pub grid: GridSize,
    scrollback: usize,
    /// Images programs sent (the kitty graphics protocol), and where they show.
    pub images: Arc<Mutex<ImageStore>>,
    /// 3D models programs sent (the Ratty graphics protocol), and where they show.
    pub models: Arc<Mutex<ModelStore>>,
    /// A cell's size to a fraction of a pixel, for programs asking down the pipe.
    cell: Arc<Mutex<(f32, f32)>>,
    /// The pipe programs send images by where the terminal drops them
    /// (`crate::pipe`), open as long as the tab is.
    _pipe: Option<Box<dyn Pipe>>,
    /// The folder of what's first in this pane (`FLOW_FIRST`), gone with it.
    _first: Option<crate::apps::first::PaneFolder>,
    /// The shell's output, until [`Session::start`] begins reading it.
    unread: Option<(Box<dyn Read + Send>, Replies, Waker)>,
    /// The pane's own secret, which only programs started in it have
    /// (`flow_command::TAB_KEY`), so the `flow` command's asks for a tab
    /// are told from output that only looks like one.
    key: String,
}

impl Session {
    /// Starts the shell and reads what it writes.
    pub fn spawn(
        id: SessionId,
        profile: ShellProfile,
        grid: GridSize,
        cell: (f32, f32),
        scrollback: usize,
        cwd: Option<PathBuf>,
        waker: Waker,
    ) -> Result<Self> {
        let mut session = Self::launch(id, profile, grid, cell, scrollback, cwd, waker)?;
        session.start();
        Ok(session)
    }

    /// Starts the shell, leaving its output unread until [`Session::start`],
    /// so the grid can be set up first (the startup logo).
    pub fn launch(
        id: SessionId,
        profile: ShellProfile,
        grid: GridSize,
        cell: (f32, f32),
        scrollback: usize,
        cwd: Option<PathBuf>,
        waker: Waker,
    ) -> Result<Self> {
        let pty = native_pty_system();
        let pair = pty
            .openpty(PtySize {
                rows: grid.rows as u16,
                cols: grid.cols as u16,
                pixel_width: (grid.cols as f32 * cell.0) as u16,
                pixel_height: (grid.rows as f32 * cell.1) as u16,
            })
            .context("opening pseudo terminal")?;
        let reader = pair.master.try_clone_reader().context("cloning pty reader")?;
        let writer = ToShell::spawn(pair.master.take_writer().context("pty writer")?, format!("pty-write-{id}"));

        let nonce = crate::shell::new_nonce();
        let state = Arc::new(Mutex::new(ShellState { at_prompt: true, nonce: nonce.clone(), ..Default::default() }));
        let size = Arc::new(Mutex::new(WindowSize {
            num_lines: grid.rows as u16,
            num_cols: grid.cols as u16,
            cell_width: cell.0 as u16,
            cell_height: cell.1 as u16,
        }));
        let ink = tab_ink();
        let proxy = EventProxy {
            ink: ink.clone(),
            writer: writer.clone(),
            state: state.clone(),
            size: size.clone(),
            waker: waker.clone(),
        };
        let config = TermConfig { scrolling_history: scrollback, ..TermConfig::default() };
        let term = Term::new(config, &grid, proxy);
        let mut emu = Emulator::new(term);
        emu.conpty = cfg!(windows);
        let emu = Arc::new(Mutex::new(emu));

        let images = Arc::new(Mutex::new(ImageStore::default()));
        let models = Arc::new(Mutex::new(ModelStore::default()));
        let replies = Replies { writer: writer.clone(), size: size.clone(), images: images.clone(), models: models.clone() };
        let unread = Some((reader, replies, waker.clone()));

        // The pipe comes first, so the shell starts with its name.
        let cell = Arc::new(Mutex::new(cell));
        let link = Link {
            ink: ink.clone(),
            emu: emu.clone(),
            state: state.clone(),
            images: images.clone(),
            models: models.clone(),
            size: size.clone(),
            cell: cell.clone(),
            waker: waker.clone(),
        };
        let pipe = pipe::open(Arc::new(move |stream| serve_pipe(stream, &link)));
        // What `flowapp first` puts first in this pane, a folder of its own;
        // none for a snapshot's plain shells (`shell::make_shells_plain`),
        // which read nothing of the user's apps.
        let first = if crate::shell::plain_home().is_some() { None } else { crate::apps::first::open_pane(id, &profile) };
        let mut cmd = build_command(&profile, cwd.as_deref(), first.as_ref().map(|f| f.path()));
        if let Some(pipe) = &pipe {
            cmd.env(pipe::VAR, pipe.name());
        }
        let key = crate::flow_command::new_key();
        cmd.env(crate::flow_command::TAB_KEY, &key);
        // A nonce apart from the key, which opens tabs: the hook sends the
        // nonce back in the shell's output, which a log may keep (a shell
        // that tees what it prints to a file).
        crate::shell::give_nonce(&mut cmd, &profile, &nonce);
        let mut child = pair
            .slave
            .spawn_command(cmd)
            .with_context(|| format!("starting {}", profile.program.display()))?;
        drop(pair.slave);
        let killer = child.clone_killer();
        let pid = child.process_id();

        let exit_waker = waker;
        std::thread::Builder::new()
            .name(format!("pty-wait-{id}"))
            .spawn(move || {
                let _ = child.wait();
                exit_waker.send(UserEvent::ChildExited(id));
            })?;

        let pty = Arc::new(Mutex::new(Pty { master: pair.master, heard: false, held: None }));
        Ok(Self { id, profile, emu, state, writer, ink, pid, pty, killer, size, grid, scrollback, images, models, cell, _pipe: pipe, _first: first, unread, key })
    }

    /// The pane's own secret (`flow_command::TAB_KEY`).
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Begins reading the shell's output (once).
    pub fn start(&mut self) {
        if let Some((reader, replies, waker)) = self.unread.take() {
            spawn_reader(reader, self.emu.clone(), self.state.clone(), self.pty.clone(), replies, waker, None);
        }
    }

    /// Types `text` (an old machine's start screen) at `baud` bits a second,
    /// ten to a letter (0 shows it at once), then begins reading the
    /// shell's output, which waits until it's done.
    /// Its `{logo}` (`looks::LOGO_HERE`) shows `logo` four rows tall, its
    /// `{beside}` lines past it, and its `{disk}` (`looks::PICTURE_HERE`)
    /// shows `picture` in the middle of the screen.
    pub fn start_typing(&mut self, text: String, baud: u32, logo: Option<Image>, picture: Option<Image>) {
        if let Some((reader, replies, waker)) = self.unread.take() {
            spawn_reader(reader, self.emu.clone(), self.state.clone(), self.pty.clone(), replies, waker, Some(Typing { text, baud, logo, picture }));
        }
    }

    /// Sets the cursor the terminal shows unless a program picked its own,
    /// and goes back to when one resets it.
    pub fn set_cursor_style(&self, style: CursorStyle) {
        let options = TermConfig { scrolling_history: self.scrollback, default_cursor_style: style, ..TermConfig::default() };
        self.emu.lock().term.set_options(options);
    }

    /// Greets with a logo at the top left and `lines` beside it, the first in
    /// bold, as old workstations did, and leaves the cursor below them. Call
    /// it between [`Session::launch`] and [`Session::start`]: the shell's
    /// output follows on from there (on Windows, ConPTY asks where the cursor
    /// is when it starts). `api`, the graphics API's logo, goes at the end
    /// of the last line, one row tall, or its name where the line hasn't
    /// room.
    pub fn show_banner(&self, logo: Image, lines: &[String], api: Option<crate::logos::ApiLogo>) {
        const ROWS: u32 = 4;
        let (cw, ch) = {
            let size = self.size.lock();
            (size.cell_width.max(1) as f32, size.cell_height.max(1) as f32)
        };
        let cols = (ROWS as f32 * ch * logo.width as f32 / logo.height.max(1) as f32 / cw).round().max(1.0) as u32;
        let text_col = 1 + cols as usize + 2;
        if self.grid.rows < ROWS as usize + 2 || self.grid.cols < text_col + 20 {
            return;
        }
        let place = self.images.lock().show_own(logo, cols, ROWS);
        let mut emu = self.emu.lock();
        let e = &mut *emu;
        for row in 0..ROWS {
            for col in 0..cols {
                e.term.grid_mut()[Line(row as i32)][Column(1 + col as usize)] = graphics::placeholder_cell(place.id, place.placement, row, col);
            }
        }
        let room = self.grid.cols - text_col;
        let top = (ROWS as usize).saturating_sub(lines.len()) / 2;
        for (i, line) in lines.iter().enumerate() {
            let mut width = 0;
            let fitting: String = line.chars().take_while(|c| {
                width += c.width().unwrap_or(0);
                width <= room
            }).collect();
            let width: usize = fitting.chars().map(|c| c.width().unwrap_or(0)).sum();
            let bold = if i == 0 { "\x1b[1m" } else { "" };
            e.run(format!("\x1b[{};{}H{bold}{fitting}\x1b[0m", top + i + 1, text_col + 1).as_bytes());
            if let Some(api) = api.as_ref().filter(|_| i + 1 == lines.len()) {
                put_api_logo(e, &self.images, api, (cw, ch), (top + i, text_col + width), room - width);
            }
        }
        e.run(format!("\x1b[{};1H", ROWS + 2).as_bytes());
        // ConPTY never sees the logo, so it's kept through ConPTY's repaints.
        if cfg!(windows) {
            e.banner = Some(Banner { image: place.id, placement: place.placement });
        }
    }

    pub fn write(&self, bytes: &[u8]) {
        // A start screen waiting for a key takes it ("press any key").
        {
            let mut st = self.state.lock();
            if st.awaiting_key {
                st.awaiting_key = false;
                return;
            }
        }
        {
            // The emulator first, as the reader thread takes them.
            let mut emu = bytes.contains(&b'\r').then(|| self.emu.lock());
            let mut st = self.state.lock();
            if let (Some(emu), true, Some(start)) = (&mut emu, st.at_prompt, st.input_start) {
                let mut command = emu.input_from(start);
                // Typed in the same write as Enter (a paste where the shell
                // doesn't bracket pastes), so not echoed yet.
                let before = &bytes[..bytes.iter().position(|&b| b == b'\r').unwrap_or(0)];
                if let Ok(text) = std::str::from_utf8(before) {
                    if !text.chars().any(char::is_control) {
                        command.push_str(text);
                    }
                }
                if !command.trim().is_empty() {
                    // Its output starts on the line after the one it was
                    // typed on, for Ask why should it fail.
                    st.command = Some(command.trim().to_string());
                    st.command_read = true;
                    st.output_from = Some(emu.cursor_abs_line() + 1);
                    // Its prompt's line marked, for the scrollbar and
                    // Previous and Next Command.
                    if let Some(line) = st.prompt_line.filter(|_| !emu.term.mode().contains(TermMode::ALT_SCREEN)) {
                        crate::marks::start(emu.term.grid_mut(), line);
                    }
                    let cwd = st.cwd.clone();
                    st.note(HistoryNote::Entered { command, cwd, at: SystemTime::now(), assistant: false });
                }
            }
            drop(emu);
            st.sent(bytes);
            if let Some(tracker) = st.tracker.as_mut() {
                tracker.typed = bytes.last() != Some(&b'\r');
            }
        }
        self.report(bytes);
    }

    /// A click at `target` on the line being typed at the prompt
    /// ([`Emulator::letters_to`]) moves the shell's cursor there, by as
    /// many ← or → as there are letters between, as the keys would (in the
    /// shell's cursor-key mode). False where it isn't such a click: the
    /// shell isn't at its prompt, or can't say; a program holds the
    /// alternate screen; the click is on another line, or where the cursor
    /// is already.
    pub fn click_to_move(&self, target: Point) -> bool {
        let start = {
            let st = self.state.lock();
            match st.input_start {
                Some(start) if st.integrated && st.at_prompt => start,
                _ => return false,
            }
        };
        let (letters, app) = {
            let emu = self.emu.lock();
            let mode = *emu.term.mode();
            if mode.contains(TermMode::ALT_SCREEN) {
                return false;
            }
            (emu.letters_to(start, target), mode.contains(TermMode::APP_CURSOR))
        };
        let Some(n) = letters.filter(|&n| n != 0) else { return false };
        let key: &[u8] = match (n > 0, app) {
            (true, true) => b"\x1bOC",
            (true, false) => b"\x1b[C",
            (false, true) => b"\x1bOD",
            (false, false) => b"\x1b[D",
        };
        self.write(&key.repeat(n.unsigned_abs()));
        true
    }

    /// Bytes that aren't typing, such as a mouse report: a prompt doesn't
    /// take them as a command begun.
    pub fn report(&self, bytes: &[u8]) {
        self.go_on_from_start();
        self.writer.send(bytes);
    }

    /// Ends a start screen's wait for a key without taking one, for what
    /// goes to the shell that no one typed (the assistant's commands, the
    /// history's), so the shell's output isn't held up.
    pub(crate) fn go_on_from_start(&self) {
        self.state.lock().awaiting_key = false;
    }

    /// What programs asked about Flow's settings since the UI last looked.
    pub fn take_settings_asks(&self) -> Vec<SettingsAsk> {
        std::mem::take(&mut self.state.lock().settings)
    }

    /// Starts the tab's terminal afresh, as `flow-settings reset terminal`
    /// asks: a full reset (RIS) of its modes, colors, cursor and screen, its
    /// scrollback cleared, and its pictures, models and wash gone. The
    /// files its programs failed to read lately stay counted
    /// ([`ImageStore::reset`]), since a program holding the tab's key
    /// (`FLOW_TAB_KEY`, which a remote one may be handed) may ask for this.
    pub fn reset_terminal(&self) {
        self.play(b"\x1bc\x1b[3J");
        self.images.lock().reset();
        *self.models.lock() = ModelStore::default();
        self.state.lock().wash = None;
    }

    /// The shell's process, under which the tab's programs run.
    pub fn pid(&self) -> Option<u32> {
        self.pid
    }

    /// What Flow holds for this tab: its scrollback and screen (every row's
    /// cells), and its programs' pictures, decoded, with each one's serial
    /// and size for what the GPU holds of them.
    pub fn held(&self) -> (u64, u64, Vec<(u64, u32, u32)>) {
        let grid = {
            let emu = self.emu.lock();
            let grid = emu.term.grid();
            (grid.total_lines() * grid.columns() * std::mem::size_of::<Cell>()) as u64
        };
        let (pictures, images) = self.images.lock().held();
        (grid, pictures, images)
    }

    /// The colors programs in this tab are told when they ask: its own
    /// theme's.
    pub fn set_ink(&self, ink: crate::render::palette::Ink) {
        *self.ink.lock() = ink;
    }

    /// Mouse motion for a program that asked for it: dropped while the
    /// program is behind reading, as a stale move is worth nothing.
    pub fn report_motion(&self, bytes: &[u8]) {
        self.writer.send_motion(bytes);
    }

    pub fn resize(&mut self, grid: GridSize, cell: (f32, f32)) {
        // Programs size images by the cell's pixels, so a new font size counts too.
        let same_cell = {
            let size = self.size.lock();
            (size.cell_width, size.cell_height) == (cell.0 as u16, cell.1 as u16)
        };
        if grid.cols == self.grid.cols && grid.rows == self.grid.rows && same_cell {
            return;
        }
        self.grid = grid;
        *self.cell.lock() = cell;
        *self.size.lock() = WindowSize {
            num_lines: grid.rows as u16,
            num_cols: grid.cols as u16,
            cell_width: cell.0 as u16,
            cell_height: cell.1 as u16,
        };
        // ConPTY repaints after a resize only once it has its size, which
        // waits for its first output (`Pty`): until then nothing on the
        // screen is its to paint again, so none of it may go.
        let conpty = cfg!(windows).then(|| self.pty.lock().heard);
        let mut emu = self.emu.lock();
        // The prompt the shell drew last stays where its text goes in the
        // rewrap, for a click on its line, the command's text for the
        // history and the mark Enter puts on it: the shell draws it once.
        // The reader thread sets them only with the emulator, held here.
        let (input, prompt) = {
            let st = self.state.lock();
            (st.input_start, st.prompt_line)
        };
        let kept_input = input.map(|place| (place, emu.place_in_text(place)));
        let kept_prompt = prompt.map(|line| (line, emu.place_in_text((line, 0))));
        // Pictures are cut at a nearer edge rather than rewrapped, and
        // filled out again as it moves back.
        let old_cols = emu.term.columns();
        graphics::crop_pictures(&mut emu.term, grid.cols);
        emu.resize_for(grid, conpty);
        {
            let mut st = self.state.lock();
            if let Some((place, kept)) = kept_input {
                st.input_start = kept.map_or(Some(place), |kept| emu.place_from_text(kept));
            }
            if let Some((line, kept)) = kept_prompt {
                st.prompt_line = kept.map_or(Some(line), |kept| emu.place_from_text(kept).map(|(line, _)| line));
            }
        }
        graphics::regrow_pictures(&mut emu.term, old_cols, &self.images.lock());
        drop(emu);
        self.pty.lock().resize(PtySize {
            rows: grid.rows as u16,
            cols: grid.cols as u16,
            pixel_width: (grid.cols as f32 * cell.0) as u16,
            pixel_height: (grid.rows as f32 * cell.1) as u16,
        });
        if !same_cell {
            repicture_models(&self.models, &self.images, *self.size.lock());
        }
    }

    /// A press of `button` over the terminal, at `cell` (its row on the
    /// screen as shown, scrolled back or not, and column) and `within` it
    /// (0 to 1 each way), cells being `cell_size` pixels: if a model whose
    /// program asked for clicks (`clicks=1`) is there and the press lands
    /// on it, the program hears where. False otherwise, for the press to go
    /// on as any other.
    pub fn click_model(&self, cell: (usize, usize), within: (f32, f32), cell_size: (f32, f32), button: model::Button) -> bool {
        let (cell, within) = on_double_line(self.line_size_at(cell.0), cell, within);
        // The emulator only to read the cell: the ray cast needn't hold up
        // the shell's output.
        let Some(mark) = picture_cell(&self.emu.lock(), cell) else { return false };
        match model_click(mark, (&self.models, &self.images), within, cell_size, button) {
            Some(answer) => {
                self.report(answer.as_bytes());
                true
            }
            None => false,
        }
    }

    /// A press at `cell` on Flow's own turning model (the Graphics
    /// workstation's startup logo) turns it back to how the logo stands,
    /// to rest there a while (`model::Shown::align`). False where there's
    /// none.
    pub fn align_logo(&self, cell: (usize, usize), within: (f32, f32)) -> bool {
        let (cell, _) = on_double_line(self.line_size_at(cell.0), cell, within);
        let Some(logo) = own_model_at(&self.emu.lock(), &self.images, cell) else { return false };
        logo.align(Instant::now());
        true
    }

    /// Takes `bytes` as though the program in the tab wrote them: for
    /// snapshots, which show what a program would.
    pub fn play(&self, bytes: &[u8]) {
        let replies = Replies { writer: self.writer.clone(), size: self.size.clone(), images: self.images.clone(), models: self.models.clone() };
        let mut e = self.emu.lock();
        let mut scanner = OscScanner::default();
        for piece in scanner.feed(bytes) {
            match piece {
                Piece::Data(a, b) => advance(&mut e, &bytes[a..b]),
                Piece::Drop(..) => {}
                Piece::Marker(marker) => handle_marker(&mut e, &self.state, &replies, marker),
            }
        }
    }

    /// The size of the line on screen row `row` as shown, scrolled back or
    /// not (`crate::line_size`).
    pub fn line_size_at(&self, row: usize) -> LineSize {
        line_size::on_screen(self.emu.lock().term.grid(), row)
    }

    /// The line the cursor is on (absolute) and its text.
    pub fn cursor_line(&self) -> (i64, String) {
        let emu = self.emu.lock();
        let line = emu.cursor_abs_line();
        (line, emu.text_between(line, line + 1))
    }

    /// The keys that clear what's half-typed at the prompt.
    fn clear_line(&self) -> &'static [u8] {
        clear_line_keys(self.profile.kind, self.profile.flavor)
    }

    /// Whether the shell waits at its prompt, as its marks say: None where
    /// it sends none, so nobody can tell.
    pub fn at_prompt(&self) -> Option<bool> {
        let st = self.state.lock();
        st.integrated.then_some(st.at_prompt)
    }

    /// What runs in the pane, as its shell says, for closing it: the
    /// command started at its prompt (empty where the shell didn't say
    /// which), or none while it waits at its prompt or draws the next one.
    /// A shell that says nothing (never integrated) counts as waiting, so
    /// closing it never asks.
    pub fn busy_with(&self) -> Option<String> {
        let st = self.state.lock();
        (st.integrated && !st.at_prompt && st.prompt_due.is_none()).then(|| st.command.clone().unwrap_or_default())
    }

    /// Whether the shell is drawing the prompt after a command that ended
    /// less than `within` ago ([`ShellState::drawing_prompt`]).
    pub fn drawing_prompt(&self, within: Duration) -> bool {
        self.state.lock().drawing_prompt(within)
    }

    /// Whether letters past ASCII typed into this pane now go to a line
    /// editor that takes them as letters, as far as Flow knows: its shell
    /// waits at the prompt its hook drew, and, in bash and zsh, the hook
    /// runs before each prompt, and bash's hasn't said this one may take
    /// them as keys ([`ShellState::letters_may_be_keys`]). Else what reads
    /// the keys may be anything: a program, a shell started inside the
    /// pane's or over ssh, one that the startup files `exec`ed, which Flow's
    /// hook never reached.
    pub fn types_letters(&self) -> bool {
        let st = self.state.lock();
        st.at_prompt && !st.letters_may_be_keys(self.weighs_letters())
    }

    /// Whether letters past ASCII are weighed for this pane's shell
    /// ([`Session::letter_as_keys`]): bash's every flavor but BusyBox's ash,
    /// and zsh's, whose hooks each say a command done with the nonce before
    /// every prompt.
    fn weighs_letters(&self) -> bool {
        matches!(self.profile.kind, ShellKind::Bash | ShellKind::Zsh) && self.profile.flavor != BashFlavor::BusyBox
    }

    /// A letter past ASCII in `text`, a whole command, that this pane's
    /// shell may take as keys, not a letter, typed at its prompt now, and
    /// which can't be written otherwise: Flow then types none of it. In a
    /// C or POSIX locale readline takes each byte from 0x80 up as Meta and
    /// a key: Ē's second byte is M-C-r, revert-line, which throws away
    /// what came before it, quotes and all. So into bash where its hook
    /// said this prompt may (its locale has just changed into one that
    /// isn't UTF-8, `Marker::MetaKeys`), or hasn't run before it
    /// ([`ShellState::letters_may_be_keys`]). zsh's line editor never takes
    /// them as keys (with glibc in a locale that isn't UTF-8 it makes each
    /// byte a `?`, a letter lost, as glibc's C locale has none past ASCII
    /// to keep), but a zsh whose hook never said a command
    /// done with the nonce may have handed the pane to another shell (its
    /// startup files `exec`ed bash), whatever prompt a program printed, so
    /// into one of those neither. The others aren't readline, nor does
    /// Flow start them where there's a bash to hand the pane to.
    pub fn letter_as_keys(&self, text: &str) -> Option<char> {
        let at_risk = self.weighs_letters() && self.state.lock().letters_may_be_keys(true);
        text.chars().find(|c| !c.is_ascii()).filter(|_| at_risk)
    }

    /// Puts `command` at the prompt as a paste, without running it, in place
    /// of anything half-typed there. Its control characters but the line
    /// breaks are left out (`shell::without_keys`), which the line editor
    /// would take as keys where the shell doesn't bracket pastes (bash's
    /// Ctrl+O runs the line, with no Enter of the user's), and the
    /// terminal's driver even mid-paste (Ctrl+C), as one from the history
    /// may be a program's; but a tab inside a bracketed paste, a letter to
    /// the end of it, which a paste can't end early (`input::paste_bytes`
    /// leaves out Esc), stays, as `awk -F'<tab>'` needs. Where the shell
    /// doesn't bracket pastes, its lines are joined by spaces too, or each
    /// would run as it came.
    pub fn put_at_prompt(&self, command: &str) {
        let bracketed = self.brackets_pastes();
        let command = crate::shell::without_keys(command, bracketed);
        let text = if bracketed { command } else { command.replace("\r\n", " ").replace(['\r', '\n'], " ") };
        {
            // Typed, though a paste's line breaks aren't Enter: the shell
            // waits at its prompt still.
            let mut st = self.state.lock();
            if st.at_prompt {
                st.input_pending = true;
                drop(st);
                self.report(self.clear_line());
            }
        }
        self.report(&crate::input::paste_bytes(&text, bracketed));
    }

    /// Whether the shell takes a paste bracketed, so its lines come as one
    /// piece of text rather than each run as it comes.
    pub fn brackets_pastes(&self) -> bool {
        self.emu.lock().term.mode().contains(TermMode::BRACKETED_PASTE)
    }

    /// `commands`' lines as one, run one after another as the shell runs
    /// them (`; `, or cmd's ` & `).
    pub fn one_line(&self, commands: &str) -> String {
        commands.lines().map(str::trim_end).filter(|l| !l.trim().is_empty()).collect::<Vec<_>>().join(match self.profile.kind {
            ShellKind::Cmd => " & ",
            _ => "; ",
        })
    }

    /// Runs `commands` at the prompt as if typed there and Enter pressed
    /// (the assistant panel's Run), in place of anything half-typed: one
    /// line as typed, several as a paste where the shell brackets pastes,
    /// else as one line. It goes into the history as the user's, since a
    /// shell that doesn't send the line it runs (cmd) would leave the grid
    /// read before the shell echoed it.
    pub fn run_typed(&self, commands: &str) {
        let lines: Vec<&str> = commands.lines().map(str::trim_end).filter(|l| !l.trim().is_empty()).collect();
        let bracketed = lines.len() > 1 && self.brackets_pastes();
        let text = if lines.len() > 1 && !bracketed { self.one_line(commands) } else { lines.join("\n") };
        {
            let mut st = self.state.lock();
            let cwd = st.cwd.clone();
            st.note(HistoryNote::Entered { command: self.one_line(commands), cwd, at: SystemTime::now(), assistant: false });
            if st.at_prompt {
                st.running_since = Some(Instant::now());
            }
            st.at_prompt = false;
            st.input_pending = false;
        }
        let mut bytes = self.clear_line().to_vec();
        if bracketed {
            bytes.extend(crate::input::paste_bytes(&text, true));
        } else {
            bytes.extend(text.as_bytes());
        }
        bytes.push(b'\r');
        self.report(&bytes);
    }

    /// Shows the rest of a start screen being typed at once.
    pub fn hurry(&self) {
        self.state.lock().hurry = true;
    }

    /// Types `command` at the prompt and starts tracking its output.
    pub fn run_command(&self, command: &str) {
        self.go_on_from_start();
        // Clear anything half-typed at the prompt first.
        let clear = self.clear_line();
        let start_abs = {
            let mut emu = self.emu.lock();
            // Make sure the user sees what is about to run.
            emu.term.scroll_display(alacritty_terminal::grid::Scroll::Bottom);
            // Its prompt's line marked, as the user's commands' are.
            let st = self.state.lock();
            if let Some(line) = st.prompt_line.filter(|_| st.at_prompt && !emu.term.mode().contains(TermMode::ALT_SCREEN)) {
                crate::marks::start(emu.term.grid_mut(), line);
            }
            drop(st);
            emu.cursor_abs_line()
        };
        {
            let mut st = self.state.lock();
            st.tracker = Some(Tracker { start_abs, sent_at: Instant::now(), result: None, typed: false });
            st.at_prompt = false;
            st.prompt_due = None;
            st.input_pending = false;
            st.running_since = Some(Instant::now());
        }
        let one_line: String = command.lines().map(str::trim_end).collect::<Vec<_>>().join(match self.profile.kind {
            ShellKind::Cmd => " & ",
            _ => "; ",
        });
        {
            let mut st = self.state.lock();
            let cwd = st.cwd.clone();
            st.note(HistoryNote::Entered { command: one_line.clone(), cwd, at: SystemTime::now(), assistant: true });
        }
        self.writer.send(&[clear, one_line.as_bytes(), b"\r"].concat());
    }

    /// Polls the command Claude started. Returns the result once it is done,
    /// or once `timeout` has passed (with `still_running` set). `timeout`
    /// counts from when `run_command` sent the command, so every poll passes
    /// the same whole timeout, not the time left.
    pub fn poll_command(&self, timeout: Duration) -> Option<CommandResult> {
        let emu = self.emu.lock();
        let mut st = self.state.lock();
        let integrated = st.integrated;
        let last_output_at = st.last_output_at;
        let tracker = st.tracker.as_mut()?;
        if let Some(result) = tracker.result.take() {
            st.tracker = None;
            return Some(result);
        }
        let elapsed = tracker.sent_at.elapsed();
        let idle = last_output_at.map(|t| t.elapsed()).unwrap_or(elapsed);
        let start = tracker.start_abs;
        let cursor = emu.cursor_abs_line();
        let finished_heuristically = !integrated
            && elapsed > Duration::from_millis(800)
            && idle > Duration::from_millis(1200)
            && looks_like_prompt(&emu.text_between(cursor, cursor + 1));
        if finished_heuristically || elapsed >= timeout {
            let output = emu.text_between(start, if finished_heuristically { cursor } else { cursor + 1 });
            st.tracker = None;
            return Some(CommandResult {
                output,
                exit_code: None,
                still_running: !finished_heuristically,
            });
        }
        None
    }

    /// Whether the user has the shell: running something, or with something
    /// typed at its prompt.
    pub fn taken(&self) -> bool {
        let (busy, start) = {
            let st = self.state.lock();
            (st.integrated && !st.at_prompt, st.input_start.filter(|_| st.at_prompt))
        };
        busy || start.is_some_and(|start| !self.emu.lock().input_from(start).trim().is_empty())
    }

    /// What a command waits at, where its output has been still a moment
    /// and the cursor's line is a question: what's asked, and the line.
    pub fn question(&self) -> Option<(Question, String)> {
        if self.state.lock().last_output_at.is_some_and(|t| t.elapsed() < QUESTION_STILL) {
            return None;
        }
        let emu = self.emu.lock();
        let cursor = emu.cursor_abs_line();
        let line = emu.text_between(cursor, cursor + 1);
        question(&line).map(|q| (q, line.trim().to_string()))
    }

    pub fn cancel_tracking(&self) {
        self.state.lock().tracker = None;
    }

    pub fn cwd(&self) -> Option<String> {
        self.state.lock().cwd.clone()
    }

    pub fn kill(&mut self) {
        let _ = self.killer.kill();
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.state.lock().closing = true;
        self.kill();
        self.writer.close();
    }
}

/// Resizes `term` as a Windows session does once ConPTY has its size, for
/// the tests.
#[cfg(test)]
fn resize_as_conpty<L: EventListener>(term: &mut Term<L>, size: GridSize) {
    resize_from_top(term, size, false);
}

/// Resizes `term` the way ConPTY resizes its own screen, which it repaints
/// whole after every resize. ConPTY keeps the top row where it is: lines that
/// wrap onto more rows grow downward, and only rows that no longer fit go off
/// the top. alacritty keeps the bottom row instead, so a line that wraps near
/// the top pushed a copy into the scrollback that ConPTY then painted again,
/// and growing wider or taller pulled scrollback onto the screen, where the
/// repaint wrote over it. So the screen down to the cursor is rewrapped on its
/// own and put back from the top, and the scrollback is rewrapped behind it
/// without coming down. The alternate screen has no scrollback to lose.
///
/// ConPTY repaints only once it has its size, after its first output
/// (`Pty`), so before that (`keep_below`) the rows below the cursor are
/// kept too, as many as the new screen has room for under those down to
/// the cursor: a start screen or the startup logo typed in, which the
/// window being sized to the screen and back as Windows first shows it
/// would otherwise take away. The rows left below either way are blank.
fn resize_from_top<L: EventListener>(term: &mut Term<L>, size: GridSize, keep_below: bool) {
    let (old_rows, old_cols) = (term.screen_lines(), term.columns());
    if term.mode().contains(TermMode::ALT_SCREEN) || (old_rows, old_cols) == (size.rows, size.cols) {
        term.resize(size);
        return;
    }
    // Rows shift under a selection in ways Term::resize can't follow here.
    term.selection = None;
    let grid = term.grid_mut();
    let cursor = grid.cursor.clone();
    let used = if keep_below { old_rows } else { cursor.point.line.0 as usize + 1 };

    // The screen down to the cursor (or whole), rewrapped on its own. Below
    // the cursor there's nothing ConPTY won't repaint, once it has its size.
    let mut screen: Grid<Cell> = Grid::new(used, old_cols, used * old_cols);
    for l in 0..used as i32 {
        screen[Line(l)] = grid[Line(l)].clone();
    }
    screen.cursor = cursor.clone();
    screen.resize(true, used, size.cols);
    let pushed = screen.history_size();
    let to_cursor = pushed + screen.cursor.point.line.0 as usize + 1;
    // Kept below the cursor, the rows the new screen has room for.
    let rows = if keep_below { (pushed + used).min(to_cursor.max(size.rows)) } else { to_cursor };
    let row = |k: usize| screen[Line(k as i32 - pushed as i32)].clone();

    // The scrollback on its own: its last line ends where the screen begins,
    // and a single blank row with the cursor on it marks that end through the
    // rewrap. Whatever the resize pulls down from the scrollback lands above
    // the cursor, and goes straight back.
    // Its last cell, which on a double line laid out for the rewrap
    // (`line_size::lift`) needn't be the screen's last column.
    if let Some(cell) = (grid.history_size() > 0).then(|| grid[Line(-1)].last_mut()).flatten() {
        cell.flags.remove(Flags::WRAPLINE);
    }
    grid.cursor.point = Point::new(Line(0), Column(0));
    grid.resize(false, 1, old_cols);
    grid[Line(0)].reset(&Cell::default());
    term.resize(size);
    let grid = term.grid_mut();
    let region = Line(0)..Line(size.rows as i32);
    let pulled = grid.cursor.point.line.0 as usize;
    if pulled > 0 {
        grid.scroll_up(&region, pulled);
    }

    // The screen's rows from the top; those that don't fit scroll off into
    // the scrollback, as they do in ConPTY.
    let mut next = 0;
    while rows - next > size.rows {
        let off = (rows - next - size.rows).min(size.rows);
        for j in 0..off {
            grid[Line(j as i32)] = row(next + j);
        }
        grid.scroll_up(&region, off);
        next += off;
    }
    for j in 0..rows - next {
        grid[Line(j as i32)] = row(next + j);
    }
    // Below them, blank rows: not in the colors the cursor last wrote in,
    // which the scroll above fills the rows it brings in with (a start
    // screen's white page left a white row at the foot).
    for j in rows - next..size.rows {
        grid[Line(j as i32)].reset(&Cell::default());
    }
    grid.cursor = cursor;
    grid.cursor.point = Point::new(Line((to_cursor - 1 - next) as i32), screen.cursor.point.column);
    grid.cursor.input_needs_wrap = screen.cursor.input_needs_wrap;
}

/// What a command waiting at the cursor's line asks of the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Question {
    /// A password or a passphrase.
    Password,
    /// Yes or no, a choice, or a key to go on.
    Answer,
}

/// How long a command's output has to be still before its last line is
/// taken for a question.
pub const QUESTION_STILL: Duration = Duration::from_millis(1500);

/// What `line` asks, where it's a question a program waits at: a
/// password's prompt, or yes or no (`[Y/n]`, `(y/n)`, `(yes/no)`), a key
/// to go on, or a "Continue?".
pub fn question(line: &str) -> Option<Question> {
    if is_password_prompt(line) {
        return Some(Question::Password);
    }
    let l = line.trim_end().to_lowercase();
    if l.is_empty() || l.len() > 300 {
        return None;
    }
    const ASKS: &[&str] = &[
        "[y/n]", "(y/n)", "[yes/no]", "(yes/no)", "[y/n/q]", "(y/n/q)", "([y]/n)", "[y,n", "(y/n/a)", "[ynq]", "press any key",
        "press enter to continue", "press return to continue", "continue?", "proceed?", "overwrite?", "are you sure",
    ];
    // PowerShell's choices (`-Confirm`, `Remove-Item` of a folder with
    // children) end `[?] Help (default is "Y"):`, which a narrow pane
    // wraps, so its end is all the cursor's line may hold.
    let powershell = l.contains("(default is \"") && l.ends_with("\"):");
    (powershell || ASKS.iter().any(|a| l.contains(a))).then_some(Question::Answer)
}

/// Whether a line is a prompt for a password or a passphrase, as sudo, su,
/// doas, ssh and git ask: it ends with a colon and names one.
pub fn is_password_prompt(line: &str) -> bool {
    let line = line.trim_end().to_lowercase();
    line.len() < 200 && line.ends_with(':') && ["password", "passphrase", "passcode", "パスワード"].iter().any(|w| line.contains(w))
}

fn looks_like_prompt(line: &str) -> bool {
    let line = line.trim_end();
    line.ends_with('>') || line.ends_with('$') || line.ends_with('#') || line.ends_with('%') || line.ends_with('❯')
}

/// What the reader thread needs to answer programs and keep their images.
struct Replies {
    writer: ToShell,
    size: Arc<Mutex<WindowSize>>,
    images: Arc<Mutex<ImageStore>>,
    models: Arc<Mutex<ModelStore>>,
}

impl Replies {
    fn send(&self, text: &str) {
        self.writer.send(text.as_bytes());
    }
}

/// An old machine's start screen to type before the shell's output: its
/// text, its speed, and the logo its `{logo}` shows.
struct Typing {
    text: String,
    baud: u32,
    logo: Option<Image>,
    picture: Option<Image>,
}

fn spawn_reader(
    mut reader: Box<dyn Read + Send>,
    emu: Arc<Mutex<Emulator>>,
    state: Arc<Mutex<ShellState>>,
    pty: Arc<Mutex<Pty>>,
    replies: Replies,
    waker: Waker,
    typing: Option<Typing>,
) {
    std::thread::Builder::new()
        .name("pty-reader".into())
        .spawn(move || {
            if let Some(typing) = typing {
                type_out(&emu, &state, &replies, &waker, typing);
            }
            let mut scanner = OscScanner::default();
            let mut buf = vec![0u8; 64 * 1024];
            let mut heard = false;
            loop {
                let n = match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => n,
                    Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => break,
                };
                let pieces = scanner.feed(&buf[..n]);
                {
                    let mut guard = emu.lock();
                    let e = &mut *guard;
                    for piece in pieces {
                        match piece {
                            Piece::Data(a, b) => advance(e, &buf[a..b]),
                            Piece::Drop(..) => {}
                            Piece::Marker(marker) => handle_marker(e, &state, &replies, marker),
                        }
                    }
                }
                // ConPTY's question, where the cursor is, came first and is
                // answered (see `Pty`).
                if !heard {
                    heard = true;
                    pty.lock().heard();
                }
                {
                    let mut state = state.lock();
                    state.last_output_at = Some(Instant::now());
                    if !state.at_prompt {
                        state.output_lines += buf[..n].iter().filter(|&&b| b == b'\n').count() as u32;
                    }
                }
                waker.wake();
            }
            waker.wake();
        })
        .expect("spawn pty reader");
}

/// Writes `text` into the terminal as a line at `baud` would bring it, ten
/// bits to a letter, a frame's worth at a time. Its escapes are read as a
/// program's are, so a start screen can make a line double; where it says
/// (`looks::LOGO_HERE`), `logo` goes in, and the lines that follow it
/// (`looks::BESIDE_LOGO`) start past it.
fn type_out(emu: &Mutex<Emulator>, state: &Mutex<ShellState>, replies: &Replies, waker: &Waker, typing: Typing) {
    let Typing { text, baud, mut logo, mut picture } = typing;
    let chars: Vec<char> = text.chars().collect();
    let per_second = baud as f32 / 10.0;
    // When typing began, or went on after a key, and from which letter.
    let (mut started, mut from) = (Instant::now(), 0);
    let mut scanner = OscScanner::default();
    let mut shown = 0;
    // How many columns the logo, once in, takes with the gap after it.
    let mut past_logo = 0;
    state.lock().booting = true;
    while shown < chars.len() {
        // A `{wait}` waits for the tab's first key, the cursor hidden as
        // the machine's was, before the rest.
        if chars[shown] == crate::looks::KEY_WAIT {
            emu.lock().run(b"\x1b[?25l");
            state.lock().awaiting_key = true;
            waker.wake();
            loop {
                let st = state.lock();
                if !st.awaiting_key || st.closing {
                    break;
                }
                drop(st);
                std::thread::sleep(Duration::from_millis(16));
            }
            emu.lock().run(b"\x1b[?25h");
            {
                let mut st = state.lock();
                st.awaiting_key = false;
                // The machine sounds again as it goes on, as a home
                // computer beeped at its menu.
                st.booting = true;
            }
            shown += 1;
            (started, from) = (Instant::now(), shown);
            continue;
        }
        // Up to the next `{wait}` at most.
        let end = chars[shown..].iter().position(|&c| c == crate::looks::KEY_WAIT).map_or(chars.len(), |at| shown + at);
        let due = if baud == 0 || state.lock().hurry { end } else { (from + (started.elapsed().as_secs_f32() * per_second) as usize + 1).min(end) };
        if due > shown {
            let piece: String = chars[shown..due].iter().collect();
            {
                let mut guard = emu.lock();
                // The text between the logo's marks as a program's output.
                let marks = [crate::looks::LOGO_HERE, crate::looks::BESIDE_LOGO, crate::looks::LOGO_CENTERED, crate::looks::PICTURE_HERE];
                let mut rest = piece.as_str();
                loop {
                    let at = rest.find(marks);
                    let run = &rest[..at.unwrap_or(rest.len())];
                    let bytes = run.as_bytes();
                    for part in scanner.feed(bytes) {
                        match part {
                            Piece::Data(a, b) => advance(&mut guard, &bytes[a..b]),
                            Piece::Drop(..) => {}
                            Piece::Marker(marker) => handle_marker(&mut guard, state, replies, marker),
                        }
                    }
                    let Some(at) = at else { break };
                    let mark = rest[at..].chars().next().unwrap_or(' ');
                    if mark == crate::looks::LOGO_HERE {
                        past_logo = logo.take().map_or(0, |logo| place_logo(&mut guard, replies, logo, false));
                    } else if mark == crate::looks::LOGO_CENTERED {
                        if let Some(logo) = logo.take() {
                            place_logo(&mut guard, replies, logo, true);
                        }
                    } else if mark == crate::looks::PICTURE_HERE {
                        if let Some(picture) = picture.take() {
                            place_picture(&mut guard, replies, picture);
                        }
                    } else if past_logo > 0 {
                        guard.run(format!("\x1b[{past_logo}C").as_bytes());
                    }
                    rest = &rest[at + mark.len_utf8()..];
                }
            }
            {
                let mut state = state.lock();
                state.typed_out += piece.chars().filter(|c| !c.is_whitespace() && !c.is_control()).count() as u32;
                state.typed_lines += piece.matches('\n').count() as u32;
            }
            waker.wake();
            shown = due;
        }
        if shown < chars.len() && chars[shown] != crate::looks::KEY_WAIT {
            std::thread::sleep(Duration::from_millis(16));
        }
    }
}

/// The most rows a start screen's picture (`{disk}`) takes: 13 of the 32
/// rows of the Flow notanAmiga's screen, with room round it for its words.
pub const PICTURE_ROWS: u32 = 13;

/// Puts `picture` in the middle of the screen's width from the cursor's
/// line down, [`PICTURE_ROWS`] tall or as tall as leaves six rows for the
/// words around it and fits the width, the screen scrolling first if it
/// must; none in a screen too small for a quarter of that. It fills its
/// cells exactly, so no sliver of them is left without the page's color.
/// The cursor is left at the start of its last row.
fn place_picture(e: &mut Emulator, replies: &Replies, picture: Image) {
    let size = *replies.size.lock();
    let (cw, ch) = (size.cell_width.max(1) as f32, size.cell_height.max(1) as f32);
    let (columns, lines) = (e.term.grid().columns(), e.term.grid().screen_lines());
    let wide = ch * picture.width as f32 / picture.height.max(1) as f32 / cw;
    let rows = PICTURE_ROWS.min(lines.saturating_sub(6) as u32).min((columns as f32 / wide) as u32);
    if rows < PICTURE_ROWS / 4 {
        return;
    }
    let cols = ((rows as f32 * wide).round() as u32).clamp(1, columns as u32);
    e.run(format!("\r{}\x1b[{}A", "\n".repeat(rows as usize - 1), rows - 1).as_bytes());
    let place = replies.images.lock().show_own_filling(picture, cols, rows);
    let top = e.term.grid().cursor.point.line;
    let left = (columns - cols as usize) / 2;
    for row in 0..rows {
        for col in 0..cols {
            e.term.grid_mut()[top + row as i32][Column(left + col as usize)] = graphics::placeholder_cell(place.id, place.placement, row, col);
        }
    }
    e.run(format!("\x1b[{}B", rows - 1).as_bytes());
}

/// The graphics API's logo after the banner's last line, which ends at
/// `(row, col)` of the screen with `room` columns left: a space and the
/// logo one row tall (`ApiLogo::in_row`); or, where that hasn't room, its
/// name as text.
fn put_api_logo(e: &mut Emulator, images: &Mutex<ImageStore>, api: &crate::logos::ApiLogo, cell: (f32, f32), (row, col): (usize, usize), room: usize) {
    let (image, cols) = api.in_row(cell);
    let name: usize = api.name.chars().map(|c| c.width().unwrap_or(0)).sum();
    if 1 + cols as usize > room {
        if name < room {
            e.run(format!("\x1b[{};{}H {}", row + 1, col + 1, api.name).as_bytes());
        }
        return;
    }
    let place = images.lock().show_own(image, cols, 1);
    for c in 0..cols {
        e.term.grid_mut()[Line(row as i32)][Column(col + 1 + c as usize)] = graphics::placeholder_cell(place.id, place.placement, 0, c);
    }
}

/// Puts `logo` in at the start of the cursor's line, or in its middle
/// where `centered`, four rows tall, the screen scrolling first if there
/// aren't four left; the cursor stays. How many columns lines beside it
/// skip: the logo's, one before and two after (none beside a centered one).
fn place_logo(e: &mut Emulator, replies: &Replies, logo: Image, centered: bool) -> usize {
    const ROWS: u32 = 4;
    let size = *replies.size.lock();
    let (cw, ch) = (size.cell_width.max(1) as f32, size.cell_height.max(1) as f32);
    let cols = (ROWS as f32 * ch * logo.width as f32 / logo.height.max(1) as f32 / cw).round().max(1.0) as u32;
    let columns = e.term.grid().columns();
    if columns < cols as usize + if centered { 0 } else { 24 } {
        return 0;
    }
    e.run(format!("\r{}\x1b[{}A", "\n".repeat(ROWS as usize - 1), ROWS - 1).as_bytes());
    let place = replies.images.lock().show_own(logo, cols, ROWS);
    let top = e.term.grid().cursor.point.line;
    let left = if centered { (columns - cols as usize) / 2 } else { 1 };
    for row in 0..ROWS {
        for col in 0..cols {
            e.term.grid_mut()[top + row as i32][Column(left + col as usize)] = graphics::placeholder_cell(place.id, place.placement, row, col);
        }
    }
    if centered {
        0
    } else {
        1 + cols as usize + 2
    }
}

/// Feeds the terminal's output to the emulator. With a startup logo to keep
/// ([`Banner`]), or through ConPTY models, it also follows ConPTY: leaving
/// the alternate screen (`?1049l`) brings a repaint, and each frame begins
/// with `?25l` and ends with `?25h`.
fn advance(e: &mut Emulator, bytes: &[u8]) {
    let keeps_own_cells = e.banner.is_some() || (e.conpty && !e.models.is_empty());
    if !keeps_own_cells {
        e.run(bytes);
        return;
    }
    const MARKS: [&[u8]; 3] = [b"\x1b[?1049l", b"\x1b[?25l", b"\x1b[?25h"];
    let (mut from, mut at) = (0, 0);
    while let Some(i) = bytes[at..].iter().position(|&b| b == 0x1b) {
        at += i;
        let Some(k) = MARKS.iter().position(|m| bytes[at..].starts_with(m)) else {
            at += 1;
            continue;
        };
        at += MARKS[k].len();
        e.run(&bytes[from..at]);
        from = at;
        match k {
            0 => e.repaint_due(),
            1 => e.frame(true),
            _ => e.frame(false),
        }
    }
    e.run(&bytes[from..]);
}

fn handle_marker(emu: &mut Emulator, state: &Mutex<ShellState>, replies: &Replies, marker: Marker) {
    // Graphics, queries and effects come from any program, not the shell's hooks.
    let marker = match marker {
        Marker::Graphics(body) => {
            let ctx = at_cursor(emu, *replies.size.lock());
            let response = replies.images.lock().handle(&body, ctx);
            if let Some(place) = response.place {
                place_image(emu, place, if place.stay { Then::Stay } else { Then::Beside });
            }
            if let Some(free) = response.clear_screen {
                let shown = shown_images(&emu.term);
                replies.images.lock().delete_shown(&shown, free);
            }
            if let Some(reply) = response.reply {
                replies.send(&reply);
            }
            return;
        }
        Marker::VersionQuery => {
            replies.send(&version_answer());
            return;
        }
        Marker::CellSizeQuery => {
            replies.send(&cell_size_answer(*replies.size.lock()));
            return;
        }
        Marker::Fx(request) => {
            queue_effect(emu, state, request);
            return;
        }
        Marker::Sound(request) => {
            queue_sound(state, request);
            return;
        }
        Marker::Tint(wash) => {
            state.lock().wash = wash;
            return;
        }
        Marker::Settings(request) => {
            queue_settings(state, SettingsAsk { request, reply: None });
            return;
        }
        Marker::LineSize(size) => {
            emu.sized |= size.doubled();
            set_cursor_line_size(&mut emu.term, size);
            return;
        }
        Marker::Model(body) => {
            let size = *replies.size.lock();
            if let Some(reply) = handle_model(emu, state, (&replies.models, &replies.images), size, &body) {
                replies.send(reply);
            }
            return;
        }
        Marker::Sixel { params, data } => {
            // At its own size: with DECSDM at the screen's top left, the
            // cursor staying and nothing scrolling; else at the cursor, which
            // goes below it at the left as xterm leaves it (scrolling), or
            // right of its last row with sixelScrollsRight.
            if let Some(image) = sixel::decode(&params, &data) {
                let then = match (emu.sixel_display, emu.sixel_right) {
                    (true, _) => Then::Top,
                    (false, true) => Then::Beside,
                    (false, false) => Then::Below,
                };
                show_picture(emu, &replies.images, *replies.size.lock(), image, ((0, 0), false), then);
            }
            return;
        }
        Marker::ITermImage { args, data } => {
            show_iterm(emu, replies, &args, &data);
            return;
        }
        Marker::ITermFileStart(args) => {
            replies.images.lock().start_file(args);
            return;
        }
        Marker::ITermFilePart(part) => {
            replies.images.lock().file_part(&part);
            return;
        }
        Marker::ITermFileEnd => {
            let file = replies.images.lock().end_file();
            if let Some((args, data)) = file {
                show_iterm(emu, replies, &args, &data);
            }
            return;
        }
        Marker::Modes(modes) => {
            emu.sixel_display = modes.sixel_display.unwrap_or(emu.sixel_display);
            emu.sixel_right = modes.sixel_right.unwrap_or(emu.sixel_right);
            // The alternate screen left takes its pictures' cells with it;
            // a full reset, the main screen's and its scrollback's too.
            if modes.alternate == Some(false) {
                replies.images.lock().let_go_unseen(Screen::Alternate, &Default::default());
            }
            if modes == Modes::RESET {
                let_go_unseen(emu, &replies.images, Screen::Main);
            }
            return;
        }
        Marker::ModeQuery(mode) => {
            let set = if mode == 80 { emu.sixel_display } else { emu.sixel_right };
            replies.send(&format!("\x1b[?{mode};{}$y", if set { 1 } else { 2 }));
            return;
        }
        Marker::Erased { scrollback } => {
            // Pictures the screen no longer shows; the main screen's only
            // as its scrollback goes too (`clear`), which is then cheap to
            // read, as 2J alone moves them into it.
            if emu.term.mode().contains(TermMode::ALT_SCREEN) {
                let_go_unseen(emu, &replies.images, Screen::Alternate);
            } else if scrollback {
                let_go_unseen(emu, &replies.images, Screen::Main);
            }
            return;
        }
        Marker::GraphicsQuery { item, action } => {
            replies.send(&graphics_answer(item, action, *replies.size.lock()));
            return;
        }
        // The emulator answers these in the terminal.
        Marker::AreaSizeQuery | Marker::DeviceQuery | Marker::ColorQuery { .. } => return,
        // bash's hook says its prompt may take letters past ASCII as keys:
        // until the next "done". Anything may print it, which only has
        // Flow type less, so it counts for nothing else, not even as the
        // shell's first mark.
        Marker::MetaKeys => {
            state.lock().meta_keys = true;
            return;
        }
        other => other,
    };
    let mut st = state.lock();
    // A mark only the shell's hook could have sent without its nonce is a
    // program's output that only looks like the hook's: it changes
    // nothing, and the line read off the screen at Enter stands.
    if st.forged(&marker) {
        return;
    }
    // A tab counts as at its prompt (as it does before its shell says
    // anything) only once one is drawn, at 133;B: bash says its last
    // command's end and its folder (133;D, 633;P) from PROMPT_COMMAND a
    // moment before its prompt, and a command sent then would go to the
    // shell ahead of the prompt, leaving `at_prompt` true all through it.
    if !st.integrated {
        st.at_prompt = false;
    }
    st.integrated = true;
    let main = !emu.term.mode().contains(TermMode::ALT_SCREEN);
    match marker {
        Marker::PromptStart => st.prompt_line = main.then(|| emu.cursor_abs_line()),
        Marker::PromptEnd => {
            st.at_prompt = true;
            st.input_pending = false;
            st.prompt_due = None;
            st.input_start = Some((emu.cursor_abs_line(), emu.term.grid().cursor.point.column.0));
        }
        // The shell's own word for the line, which beats the screen's: an
        // empty one (bash 3.2's with its history emptied) is no command.
        Marker::CommandLine { line, newest: false, .. } => {
            st.command = Some(line.trim().to_string()).filter(|c| !c.is_empty());
            st.command_read = false;
            st.lines_said = true;
            st.note(HistoryNote::Exact(line));
        }
        // Only the newest line of bash's history, which may not be the
        // line run: where the history left that out (HISTCONTROL's
        // ignorespace, HISTIGNORE), it's the line before. It is the line
        // where erasedups took an older copy of it out, or ignoredups left
        // out a copy of it, so what the screen showed at Enter stands
        // where it's that line, the user's own. Any other, which a
        // background job's output may have put on the screen since the
        // prompt was drawn (a prompt with words of its own), names
        // nothing: no row in the history, no command for the notification
        // or Ask why. Only what the screen showed is weighed: after the
        // shell's own word for a line, the newest that follows is that of
        // a later line of the same paste, which bash reads one by one, no
        // prompt between them, and the first line's word stands.
        Marker::CommandLine { line, newest: true, .. } => {
            if st.command_read && st.command.as_deref().is_some_and(|c| c != line.trim()) {
                st.command = None;
                st.note(HistoryNote::LeftOut);
            }
            st.command_read = false;
            st.lines_said = true;
        }
        Marker::CommandStart { .. } => {
            // Marked at Enter already; again now, since a transient prompt
            // (zsh's) draws the line again in between, taking the mark.
            if let Some(line) = st.prompt_line.filter(|_| main) {
                crate::marks::start(emu.term.grid_mut(), line);
            }
            st.started = true;
            st.at_prompt = false;
            st.prompt_due = None;
            st.running_since.get_or_insert_with(Instant::now);
            st.output_from.get_or_insert(emu.cursor_abs_line());
            st.failed = None;
        }
        Marker::Progress(progress) => st.progress = progress.map(|p| (p, Instant::now())),
        Marker::CommandDone { code, .. } => {
            st.started = false;
            st.prompt_due = Some(Instant::now());
            st.meta_keys = false;
            // A line read off the screen that the hook, which says each
            // line it runs, said nothing of ran nothing it saw: an empty
            // line, a blank or a comment, one the shell couldn't read (a
            // syntax error), bash 3.2's that begins in a subshell, or one
            // too long to say. So what the screen showed names nothing: a
            // background job's output may have drawn a prompt with words
            // of its own there, and Enter alone would have them named.
            if st.command_read && st.lines_said {
                st.command = None;
                st.note(HistoryNote::LeftOut);
            }
            st.command_read = false;
            if main {
                let cursor = emu.cursor_abs_line();
                crate::marks::finish(emu.term.grid_mut(), cursor, code);
            }
            let (command, from) = (st.command.take(), st.output_from.take());
            st.command_done(code, command.clone());
            st.note(HistoryNote::Done(code));
            let cursor = emu.cursor_abs_line();
            // A command of the user's that failed (not one the assistant
            // ran, which it sees itself, nor one stopped with Ctrl+C), kept
            // for Ask why with the end of what it printed. Output that
            // doesn't end in a newline leaves the cursor after it, so that
            // line counts too.
            let theirs = st.tracker.as_ref().is_none_or(|t| t.result.is_some());
            st.failed = match (code, command, from) {
                (Some(exit), Some(command), Some(from)) if exit != 0 && exit != 130 && theirs => {
                    let end = if emu.term.grid().cursor.point.column.0 > 0 { cursor + 1 } else { cursor };
                    Some(Failure { command, exit, output: tail_of(&emu.text_between(from, end.max(from))) })
                }
                _ => None,
            };
            if let Some(tracker) = st.tracker.as_mut() {
                // Commands are typed without redrawing the prompt, so the first
                // "done" marker after sending belongs to our command.
                if tracker.result.is_none() {
                    // In bash and ash, output that doesn't end in a newline
                    // (`printf foo`) leaves the cursor after it, so that line
                    // counts too. Not when it may hold keys the user typed
                    // meanwhile, which a Unix tty echoes there.
                    let after_text = emu.term.grid().cursor.point.column.0 > 0;
                    let end = if after_text && !tracker.typed { cursor + 1 } else { cursor };
                    let output = emu.text_between(tracker.start_abs, end.max(tracker.start_abs + 1));
                    tracker.result = Some(CommandResult { output, exit_code: code, still_running: false });
                }
            }
        }
        Marker::Cwd(path) => {
            if !path.is_empty() {
                st.cwd = Some(path);
            }
        }
        Marker::MetaKeys
        | Marker::Graphics(_)
        | Marker::Model(_)
        | Marker::VersionQuery
        | Marker::CellSizeQuery
        | Marker::AreaSizeQuery
        | Marker::DeviceQuery
        | Marker::ColorQuery { .. }
        | Marker::Fx(_)
        | Marker::Sound(_)
        | Marker::Tint(_)
        | Marker::Settings(_)
        | Marker::LineSize(_)
        | Marker::Sixel { .. }
        | Marker::ITermImage { .. }
        | Marker::ITermFileStart(_)
        | Marker::ITermFilePart(_)
        | Marker::ITermFileEnd
        | Marker::Modes(_)
        | Marker::ModeQuery(_)
        | Marker::Erased { .. }
        | Marker::GraphicsQuery { .. } => {}
    }
}

/// Shows an iTerm2 file, `data` its base64, as its `args` ask: the cursor
/// after its last row, right of it, as iTerm2 leaves it, or staying.
fn show_iterm(emu: &mut Emulator, replies: &Replies, args: &iterm::Args, data: &[u8]) {
    let image = match iterm::decode(data) {
        Ok(image) => image,
        Err(err) => {
            // Its name is the program's, escaped so it writes no lines or
            // escapes of its own into the log.
            log::info!("iTerm2 image {} not shown: {err}", args.name.as_deref().unwrap_or("without a name").escape_debug());
            return;
        }
    };
    let size = *replies.size.lock();
    let screen = (emu.term.columns() as u32, emu.term.screen_lines() as u32);
    let (cols, rows, fills) = iterm::cells(args, (image.width, image.height), (size.cell_width as u32, size.cell_height as u32), screen);
    show_picture(emu, &replies.images, size, image, ((cols, rows), fills), if args.stay { Then::Stay } else { Then::Beside });
}

/// Shows a picture a program sent another way than kitty's (a sixel, an
/// iTerm2 file), which nothing names again: over `cells` (0 and 0 its own
/// size), fitted, or stretched to fill them with `fills`, at the cursor or,
/// for [`Then::Top`], at the screen's top left, as wide as the screen, and
/// for [`Then::Above`] in lines of its own. Then pictures no cell shows any
/// more go, rather than wait for the quota: on the alternate screen those
/// this one covered, and on the main one, once they hold half the quota,
/// those its scrollback hasn't either.
fn show_picture(emu: &mut Emulator, images: &Mutex<ImageStore>, size: WindowSize, image: Image, (cells, fills): ((u32, u32), bool), then: Then) {
    let screen = if emu.term.mode().contains(TermMode::ALT_SCREEN) { Screen::Alternate } else { Screen::Main };
    let mut ctx = at_cursor(emu, size);
    match then {
        Then::Top => ctx.cols_left = line_size::of_row(&emu.term.grid()[Line(0)]).shown(emu.term.columns()) as u32,
        Then::Above(_) => ctx.cols_left = emu.term.columns() as u32,
        _ => {}
    }
    let Some(place) = images.lock().show(image, cells, fills, ctx, screen) else { return };
    place_image(emu, place, then);
    if screen == Screen::Alternate || images.lock().sweep_due() {
        let_go_unseen(emu, images, screen);
    }
}

/// Lets go of the pictures Flow placed for programs on `screen` (which is
/// the one in use) that none of its cells show, nor, on the main screen,
/// its scrollback's.
fn let_go_unseen(emu: &Emulator, images: &Mutex<ImageStore>, screen: Screen) {
    if !images.lock().holds_shown(screen) {
        return;
    }
    let grid = emu.term.grid();
    let from = if screen == Screen::Main { -(grid.history_size() as i32) } else { 0 };
    let mut seen = std::collections::HashSet::new();
    for line in from..grid.screen_lines() as i32 {
        let row = &grid[Line(line)];
        for column in 0..grid.columns() {
            if let Some(mark) = graphics::read_placeholder(&row[Column(column)]) {
                seen.insert(mark.id);
            }
        }
    }
    images.lock().let_go_unseen(screen, &seen);
}

/// What placing a picture at the cursor needs: a cell's pixels, and the
/// columns right of the cursor (on a double line, half of them).
fn at_cursor(emu: &Emulator, size: WindowSize) -> graphics::Context {
    let grid = emu.term.grid();
    let columns = line_size::of_row(&grid[grid.cursor.point.line]).shown(grid.columns());
    graphics::Context { cell: (size.cell_width as u32, size.cell_height as u32), cols_left: columns.saturating_sub(grid.cursor.point.column.0) as u32 }
}

/// Makes the cursor's line `size`, as `ESC # 3` to `ESC # 6` ask
/// (`line_size::make`: a single line made double loses its right half). A
/// cursor past a double line's half goes to its right margin, as on a DEC
/// terminal.
fn set_cursor_line_size<L: EventListener>(term: &mut Term<L>, size: LineSize) {
    let grid = term.grid_mut();
    let line = grid.cursor.point.line;
    line_size::make(&mut grid[line], size);
    line_size::clamp_cursor(grid);
}

/// A Ratty graphics command (`crate::model`), from any program as images
/// are; returns the answer to send back, if it asks for one.
fn handle_model(e: &mut Emulator, state: &Mutex<ShellState>, stores: (&Mutex<ModelStore>, &Mutex<ImageStore>), size: WindowSize, body: &[u8]) -> Option<&'static str> {
    let (models, images) = stores;
    match model::parse(body)? {
        model::Command::Support => return Some(model::SUPPORT),
        model::Command::Register { id, format, source: model::Source::Payload { more, data } } => {
            if let Err(err) = models.lock().receive(id, format, more, data) {
                log::warn!("3D model: {err}");
            }
        }
        // Read only once the user says yes; a relative path is from the
        // shell's folder. One on another computer isn't asked about: the
        // card shows only a path, and on Windows a yes would hand that
        // computer the user's sign-in (`link::on_this_computer`).
        model::Command::Register { id, format, source: model::Source::Path(path) } => {
            let path = PathBuf::from(path);
            let path = match state.lock().cwd.as_deref() {
                Some(cwd) if path.is_relative() => PathBuf::from(cwd).join(path),
                _ => path,
            };
            if !crate::link::on_this_computer(&path) {
                log::warn!("3D model: {} isn't on this computer, so it isn't asked about", path.display());
                return None;
            }
            models.lock().ask(id, format, path);
        }
        model::Command::Place { id, row, col, cols, rows, style } => place_model(e, stores, size, id, (row, col), (cols, rows), style),
        model::Command::Update { id, update } => models.lock().update(id, &update),
        model::Command::Delete(id) => {
            let gone = models.lock().delete(id);
            for s in &gone {
                clear_model(e, images, s);
            }
        }
    }
    None
}

/// Shows object `id` centered on `at` (a row and column on the screen, from
/// 0) over `span` cells, as Ratty does: a picture of its own, written as
/// placeholder cells without moving the cursor. Placed again, it moves.
fn place_model(
    e: &mut Emulator,
    (models, images): (&Mutex<ModelStore>, &Mutex<ImageStore>),
    size: WindowSize,
    id: u32,
    (row, col): (u32, u32),
    (cols, rows): (u32, u32),
    style: model::Style,
) {
    let (cols, rows) = (cols.clamp(1, model::MAX_SPAN), rows.clamp(1, model::MAX_SPAN));
    let mut models = models.lock();
    if let Some(old) = models.shown.remove(&id) {
        clear_model(e, images, &old);
    }
    let (w, h) = model_picture((cols, rows), size);
    let pixels = u64::from(w) * u64::from(h);
    let covered: u64 = models.shown.values().map(|s| s.pixels).sum();
    if models.shown.len() >= model::MAX_SHOWN || covered + pixels > model::MAX_PIXELS {
        log::warn!("3D model {id}: too many shown at once to place another");
        return;
    }
    let Some(slot) = models.slot(id) else { return };
    let shown = Arc::new(model::Shown::new(slot, style, f32::from(size.cell_height.max(1))));
    let picture = Image::model(w, h, shown.clone());
    let place = images.lock().show_own(picture, cols, rows);
    // The anchor is the middle cell, or the one past the middle.
    let top = row.saturating_sub((rows - 1).div_ceil(2)) as usize;
    let left = col.saturating_sub((cols - 1).div_ceil(2)) as usize;
    let (screen_rows, screen_cols) = (e.term.screen_lines(), e.term.columns());
    for r in 0..(rows as usize).min(screen_rows.saturating_sub(top)) {
        // A double line shows half its columns; past them, its last cell
        // holds its size.
        let row = &mut e.term.grid_mut()[Line((top + r) as i32)];
        let shown = line_size::of_row(row).shown(screen_cols);
        for c in 0..(cols as usize).min(shown.saturating_sub(left)) {
            row[Column(left + c)] = graphics::placeholder_cell(place.id, place.placement, r as u32, c as u32);
        }
    }
    let top = e.term.grid().history_size() as i64 + top as i64;
    models.shown.insert(id, model::Showing { image: place.id, top, left, cols, rows, pixels, shown });
    e.models.push(place.id);
}

/// The pixels of a model's picture over `cols` × `rows` cells: whole pixels
/// a cell, as programs sizing their pictures by `CSI 16 t` count them.
fn model_picture((cols, rows): (u32, u32), size: WindowSize) -> (u32, u32) {
    let (cw, ch) = (u32::from(size.cell_width).max(1), u32::from(size.cell_height).max(1));
    ((cols * cw).min(4096), (rows * ch).min(4096))
}

/// After a font size change, gives each placed model a picture the cells'
/// new size under the same image id (so its cells stay as they are), which
/// the renderer draws afresh rather than scaling the old one; one that
/// would cover more than a tab may keeps its old picture, scaled.
fn repicture_models(models: &Mutex<ModelStore>, images: &Mutex<ImageStore>, size: WindowSize) {
    let mut models = models.lock();
    let mut covered: u64 = models.shown.values().map(|s| s.pixels).sum();
    for s in models.shown.values_mut() {
        s.shown.set_cell_height(f32::from(size.cell_height.max(1)));
        let (w, h) = model_picture((s.cols, s.rows), size);
        let pixels = u64::from(w) * u64::from(h);
        if covered - s.pixels + pixels <= model::MAX_PIXELS && images.lock().resize_model(s.image, (w, h)) {
            covered = covered - s.pixels + pixels;
            s.pixels = pixels;
        }
    }
}

/// Where a point `within` cell (`row`, `col`) of the grid lands among the
/// cells of a line of `size` as they're drawn: a double line's are twice as
/// wide, and each half of a double-height one shows half of each.
fn on_double_line(size: LineSize, (row, col): (usize, usize), within: (f32, f32)) -> ((usize, usize), (f32, f32)) {
    let x = (col as f32 + within.0) / 2.0;
    let y = match size {
        LineSize::Single => return ((row, col), within),
        LineSize::Wide => within.1,
        LineSize::Top => within.1 / 2.0,
        LineSize::Bottom => 0.5 + within.1 / 2.0,
    };
    ((row, x as usize), (x.fract(), y))
}

/// The picture cell at `(row, col)` on the screen as shown, if it's one.
fn picture_cell(e: &Emulator, (row, col): (usize, usize)) -> Option<graphics::Mark> {
    let grid = e.term.grid();
    if row >= e.term.screen_lines() || col >= e.term.columns() {
        return None;
    }
    let line = Line(row as i32 - grid.display_offset() as i32);
    graphics::read_placeholder(&grid[line][Column(col)])
}

/// Flow's own turning model shown at `cell`, if one is: its startup logo.
fn own_model_at(e: &Emulator, images: &Mutex<ImageStore>, cell: (usize, usize)) -> Option<Arc<model::Shown>> {
    let mark = picture_cell(e, cell)?;
    let (picture, _) = images.lock().lookup(mark.id, mark.placement)?;
    picture.model.clone().filter(|m| m.flows_own && m.style().animate)
}

/// The answer to a press of `button` `within` picture cell `mark` over a
/// model that asked for clicks, if it lands on the model
/// (`Session::click_model`).
fn model_click(
    mark: graphics::Mark,
    (models, images): (&Mutex<ModelStore>, &Mutex<ImageStore>),
    within: (f32, f32),
    cell_size: (f32, f32),
    button: model::Button,
) -> Option<String> {
    let (Some(r), Some(c)) = (mark.row, mark.col) else { return None };
    let (id, shown) = models.lock().shown.iter().find(|(_, s)| s.image == mark.id).map(|(&id, s)| (id, s.shown.clone()))?;
    if !shown.style().clicks {
        return None;
    }
    let (picture, placement) = images.lock().lookup(mark.id, mark.placement)?;
    let at = ((c as f32 + within.0) * cell_size.0, (r as f32 + within.1) * cell_size.1);
    let point = graphics::picture_point(&placement, cell_size, at)?;
    let hit = shown.hit((picture.width as f32, picture.height as f32), point, Instant::now())?;
    Some(model::click_answer(id, button, &hit))
}

/// Blanks the cells that still show `s`, and lets its picture go.
fn clear_model(e: &mut Emulator, images: &Mutex<ImageStore>, s: &model::Showing) {
    let hist = e.term.grid().history_size() as i64;
    let (rows, cols) = (e.term.screen_lines() as i64, e.term.columns());
    for r in 0..s.rows as i64 {
        let line = s.top + r - hist;
        if line < -hist || line >= rows {
            continue;
        }
        for c in s.left..(s.left + s.cols as usize).min(cols) {
            let cell = &mut e.term.grid_mut()[Line(line as i32)][Column(c)];
            if graphics::read_placeholder(cell).is_some_and(|m| m.id == s.image) {
                *cell = Cell::default();
            }
        }
    }
    e.models.retain(|&id| id != s.image);
    images.lock().forget_own(s.image);
}

/// Keeps a program's effect for the UI with where its screen is now. Called
/// with the emulator locked, as the reader thread does, before the state.
fn queue_effect(emu: &Emulator, state: &Mutex<ShellState>, request: FxRequest) {
    let top = emu.term.grid().history_size() as i64;
    let alt = emu.term.mode().contains(TermMode::ALT_SCREEN);
    let mut st = state.lock();
    if st.effects.len() >= MAX_EFFECTS {
        st.effects.remove(0);
    }
    st.effects.push(AskedFx { request, top, alt });
}

/// Keeps a program's sound for the UI.
fn queue_sound(state: &Mutex<ShellState>, request: SoundRequest) {
    let mut st = state.lock();
    if st.sounds.len() >= MAX_SOUNDS {
        st.sounds.remove(0);
    }
    st.sounds.push(request);
}

/// XTVERSION's answer: Flow and its version.
fn version_answer() -> String {
    format!("\x1bP>|Flow {}\x1b\\", crate::version::VERSION)
}

/// The answer to `CSI 16 t`: a cell's size in whole pixels.
fn cell_size_answer(size: WindowSize) -> String {
    format!("\x1b[6;{};{}t", size.cell_height, size.cell_width)
}

/// XTSMGRAPHICS's answer, `CSI ? Pi ; Ps ; Pv S` with Ps 0 for done, 1 for
/// an item Flow hasn't and 2 for an action it hasn't: sixel's color
/// registers (item 1), fixed, so setting or resetting them answers what
/// they are; and the largest sixel picture shown whole (item 2), the text
/// area in whole pixels as xterm's is, the most a picture may be when the
/// most is asked. ReGIS (item 3) isn't here.
fn graphics_answer(item: u16, action: u16, size: WindowSize) -> String {
    let most = graphics::MAX_SIDE;
    match (item, action) {
        (1, 1..=4) => format!("\x1b[?1;0;{}S", sixel::REGISTERS),
        (2, 1..=3) => {
            let width = (u32::from(size.num_cols) * u32::from(size.cell_width)).min(most);
            let height = (u32::from(size.num_lines) * u32::from(size.cell_height)).min(most);
            format!("\x1b[?2;0;{width};{height}S")
        }
        (2, 4) => format!("\x1b[?2;0;{most};{most}S"),
        (1 | 2, _) => format!("\x1b[?{item};2;0S"),
        _ => format!("\x1b[?{item};1;0S"),
    }
}

/// The answers to OSC 4, 10, 11 and 12 asking for `colors`, one for each in
/// turn, as the emulator gives them in the terminal (`EventProxy`'s
/// `ColorRequest`): from the tab's ink, as it's shown (its phosphor's shade
/// on a monochrome screen, `palette::answered`), each part's byte written twice
/// (`rgb:0808/0c0c/1616`), and ended as the question was.
fn color_answers(colors: &[usize], bell: bool, ink: crate::render::palette::Ink) -> String {
    use crate::render::palette;
    let end = if bell { "\x07" } else { "\x1b\\" };
    let mut answers = String::new();
    for &index in colors {
        let prefix = match index {
            0..=255 => format!("4;{index}"),
            _ => (index - 256 + 10).to_string(),
        };
        let [r, g, b] = palette::answered(index, ink);
        answers.push_str(&format!("\x1b]{prefix};rgb:{r:02x}{r:02x}/{g:02x}{g:02x}/{b:02x}{b:02x}{end}"));
    }
    answers
}

/// What a program connected to the tab's pipe works on.
struct Link {
    ink: TabInk,
    emu: Arc<Mutex<Emulator>>,
    state: Arc<Mutex<ShellState>>,
    images: Arc<Mutex<ImageStore>>,
    models: Arc<Mutex<ModelStore>>,
    size: Arc<Mutex<WindowSize>>,
    cell: Arc<Mutex<(f32, f32)>>,
    waker: Waker,
}

/// Reads what a program sends down the tab's pipe (`crate::pipe`) until it
/// hangs up, and answers it there. Only images, models, effects and sounds
/// count, with the questions a program asks before them (which terminal,
/// the pixels, the colors, DA1); anything else is passed over.
fn serve_pipe(mut stream: Box<dyn pipe::Stream>, link: &Link) {
    let mut scanner = OscScanner::default();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        };
        let mut answers = String::new();
        let mut changed = false;
        for piece in scanner.feed(&buf[..n]) {
            let Piece::Marker(marker) = piece else { continue };
            let size = *link.size.lock();
            match marker {
                Marker::Graphics(body) => {
                    // The pipe has no cursor to place a picture at: the
                    // program writes the placeholder cells that show it.
                    let ctx = graphics::Context { cell: (size.cell_width as u32, size.cell_height as u32), cols_left: size.num_cols as u32 };
                    let response = link.images.lock().handle(&body, ctx);
                    if let Some(free) = response.clear_screen {
                        let shown = shown_images(&link.emu.lock().term);
                        link.images.lock().delete_shown(&shown, free);
                    }
                    answers.extend(response.reply);
                    changed = true;
                }
                Marker::VersionQuery => answers.push_str(&version_answer()),
                Marker::CellSizeQuery => answers.push_str(&cell_size_answer(size)),
                Marker::AreaSizeQuery => {
                    // ConPTY reports no pixels, and whole ones for a cell
                    // would leave hairlines between pictures made to fit it.
                    let (cw, ch) = *link.cell.lock();
                    let (w, h) = ((size.num_cols as f32 * cw).round(), (size.num_lines as f32 * ch).round());
                    answers.push_str(&format!("\x1b[4;{h};{w}t"));
                }
                Marker::DeviceQuery => answers.push_str(DA1),
                Marker::GraphicsQuery { item, action } => answers.push_str(&graphics_answer(item, action, size)),
                // A program that sets its own colors asks what the
                // background is, to suit it.
                Marker::ColorQuery { colors, bell } => answers.push_str(&color_answers(&colors, bell, *link.ink.lock())),
                // A game sends its effects and sounds with its pictures.
                Marker::Fx(request) => {
                    queue_effect(&link.emu.lock(), &link.state, request);
                    changed = true;
                }
                Marker::Sound(request) => {
                    queue_sound(&link.state, request);
                    changed = true;
                }
                Marker::Tint(wash) => {
                    link.state.lock().wash = wash;
                    changed = true;
                }
                // Answered by the UI, which this thread waits for, a few
                // seconds at most.
                Marker::Settings(request) => {
                    let (reply, answer) = crossbeam_channel::bounded(1);
                    queue_settings(&link.state, SettingsAsk { request, reply: Some(reply) });
                    link.waker.wake();
                    if let Ok(answer) = answer.recv_timeout(Duration::from_secs(3)) {
                        answers.push_str(&answer);
                    }
                }
                Marker::Model(body) => {
                    // A placed model's cells are Flow's own, which ConPTY
                    // doesn't know of, so what the program wrote just before
                    // placing it, which ConPTY paints a moment later, would
                    // paint over them.
                    if body.starts_with(b"p;") {
                        wait_for_paint(&link.state);
                    }
                    answers.extend(handle_model(&mut link.emu.lock(), &link.state, (&link.models, &link.images), size, &body));
                    changed = true;
                }
                // ConPTY drops a sixel written to the terminal, so one comes
                // here, and goes where the program's text has brought the
                // cursor once ConPTY has painted that text.
                Marker::Sixel { params, data } => {
                    if let Some(image) = sixel::decode(&params, &data) {
                        wait_for_paint(&link.state);
                        show_piped_sixel(link, image);
                        changed = true;
                    }
                }
                // How those go, DECSDM and sixelScrollsRight, as the program
                // may set them here with its sixels (ConPTY passes them on
                // to the terminal too), and DECRQM asking.
                Marker::Modes(modes) => {
                    let mut emu = link.emu.lock();
                    emu.sixel_display = modes.sixel_display.unwrap_or(emu.sixel_display);
                    emu.sixel_right = modes.sixel_right.unwrap_or(emu.sixel_right);
                }
                Marker::ModeQuery(mode) => {
                    let emu = link.emu.lock();
                    let set = if mode == 80 { emu.sixel_display } else { emu.sixel_right };
                    answers.push_str(&format!("\x1b[?{mode};{}$y", if set { 1 } else { 2 }));
                }
                // iTerm2's images among the rest: ConPTY passes them on to the
                // terminal, where they go at the cursor.
                _ => {}
            }
        }
        if !answers.is_empty() && stream.write_all(answers.as_bytes()).and_then(|_| stream.flush()).is_err() {
            break;
        }
        if changed {
            link.waker.wake();
        }
    }
}

/// How long ConPTY's output has to have stopped before a model sent down
/// the pipe is placed, and how long the placement waits at most.
const PAINT_QUIET: Duration = Duration::from_millis(40);
const PAINT_WAIT: Duration = Duration::from_millis(250);

/// Waits for ConPTY to paint what a program wrote before it placed a model
/// down the pipe: until ConPTY's output has stopped for [`PAINT_QUIET`],
/// which it may not have begun yet, so no sooner than that from now, and
/// [`PAINT_WAIT`] at most.
fn wait_for_paint(state: &Mutex<ShellState>) {
    let start = Instant::now();
    while start.elapsed() < PAINT_WAIT {
        let quiet = state.lock().last_output_at.is_none_or(|t| t.elapsed() >= PAINT_QUIET);
        if quiet && start.elapsed() >= PAINT_QUIET {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// The images the screen shows now, as (image, placement) from its
/// placeholder cells: the screen in use, main or alternate, without the
/// scrollback.
fn shown_images<T>(term: &Term<T>) -> std::collections::HashSet<(u32, u32)> {
    let grid = term.grid();
    let mut shown = std::collections::HashSet::new();
    for line in 0..grid.screen_lines() {
        let row = &grid[Line(line as i32)];
        for column in 0..grid.columns() {
            if let Some(mark) = graphics::read_placeholder(&row[Column(column)]) {
                shown.insert((mark.id, mark.placement));
            }
        }
    }
    shown
}

/// Where the cursor goes once a picture is in.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Then {
    /// After its last row, right of it: kitty's and iTerm2's.
    Beside,
    /// Back where it was: kitty's `C=1`, WezTerm's `doNotMoveCursor=1`.
    Stay,
    /// To the start of the line below it, as xterm leaves it after a sixel.
    Below,
    /// The picture at the screen's top left, not the cursor's, as far as
    /// the screen goes, the cursor staying and nothing scrolling: a sixel
    /// in DECSDM's display mode.
    Top,
    /// The picture in lines of its own above the screen's line given, the
    /// lines above it scrolling up, nothing from there down moving: a sixel
    /// sent down the pipe, through ConPTY ([`rise_above`]).
    Above(i32),
}

/// Writes an image into the grid at the cursor as placeholder cells, row by
/// row (scrolling as text would), and leaves the cursor as `then` says; or,
/// for [`Then::Top`], into the screen's top rows, as a start screen's logo
/// is written, moving nothing.
fn place_image(emu: &mut Emulator, place: Place, then: Then) {
    let e = emu;
    if then == Then::Top {
        let (columns, grid) = (e.term.columns(), e.term.grid_mut());
        for row in 0..(place.rows as usize).min(grid.screen_lines()) {
            let line = Line(row as i32);
            let shown = line_size::of_row(&grid[line]).shown(columns);
            for col in 0..(place.cols as usize).min(shown) {
                grid[line][Column(col)] = graphics::placeholder_cell(place.id, place.placement, row as u32, col as u32);
            }
        }
        return;
    }
    if let Then::Above(line) = then {
        rise_above(e, place, line);
        return;
    }
    // ConPTY never learns a picture's room: the console's cursor stays on
    // the row the picture began on, and what comes next is drawn from
    // there, by absolute moves, over the picture's second row. So through
    // ConPTY, on the main screen, the picture rises from the cursor's row
    // instead, and the next line comes below it.
    if e.conpty && then == Then::Beside && place.rows > 1 && !e.term.mode().contains(TermMode::ALT_SCREEN) {
        rise_into_place(e, place);
        return;
    }
    if then == Then::Stay {
        e.run(b"\x1b7");
    }
    let column = e.term.grid().cursor.point.column.0;
    let cols = (place.cols as usize).min(e.term.columns().saturating_sub(column));
    for row in 0..place.rows {
        if row > 0 {
            e.run(b"\n");
        }
        // Within the columns a double line shows.
        let line = e.term.grid().cursor.point.line;
        let shown = line_size::of_row(&e.term.grid()[line]).shown(e.term.columns());
        for col in 0..cols.min(shown.saturating_sub(column)) {
            e.term.grid_mut()[line][Column(column + col)] = graphics::placeholder_cell(place.id, place.placement, row, col as u32);
        }
    }
    match then {
        Then::Stay => e.run(b"\x1b8"),
        Then::Beside => e.run(format!("\x1b[{cols}C").as_bytes()),
        Then::Below => e.run(b"\r\n"),
        Then::Top | Then::Above(_) => {}
    }
}

/// [`place_image`] through ConPTY: the lines above scroll up by all but one
/// of the picture's rows (into the scrollback, as output would take them),
/// and the picture goes in from where the cursor's line went to the
/// cursor's row, its top in the scrollback where the screen has too few
/// rows above. The cursor goes right of its last row, the row ConPTY has
/// it on, so the console and Flow agree on every row from there down.
fn rise_into_place(e: &mut Emulator, place: Place) {
    let point = e.term.grid().cursor.point;
    let (line, column) = (point.line.0, point.column.0);
    let columns = e.term.columns();
    let cols = (place.cols as usize).min(columns.saturating_sub(column));
    let up = (place.rows as usize - 1).min(e.term.screen_lines().saturating_sub(1));
    e.run(format!("\x1b[{up}S").as_bytes());
    let grid = e.term.grid_mut();
    let lines = -(grid.history_size() as i32)..grid.screen_lines() as i32;
    for row in 0..place.rows {
        let at = line - up as i32 + row as i32;
        if !lines.contains(&at) {
            continue;
        }
        // Within the columns a double line shows.
        let shown = line_size::of_row(&grid[Line(at)]).shown(columns);
        for col in 0..cols.min(shown.saturating_sub(column)) {
            grid[Line(at)][Column(column + col)] = graphics::placeholder_cell(place.id, place.placement, row, col as u32);
        }
    }
    e.run(format!("\x1b[{cols}C").as_bytes());
}

/// [`place_image`] above the screen's line `at`, for a sixel sent down the
/// pipe through ConPTY: the screen's lines above it scroll up by the
/// picture's rows (into the scrollback, as output would take them), and the
/// picture goes in the rows they leave, from the left, its top in the
/// scrollback where the screen has too few rows above. Nothing from `at`
/// down moves, the cursor included, so ConPTY, which never learns the
/// picture is there, and Flow agree on every row from there down.
fn rise_above(e: &mut Emulator, place: Place, at: i32) {
    let grid = e.term.grid_mut();
    let (lines, columns) = (grid.screen_lines(), grid.columns());
    let at = at.clamp(0, lines as i32 - 1);
    let kept: Vec<Row<Cell>> = (at..lines as i32).map(|line| grid[Line(line)].clone()).collect();
    // The whole screen up, a screen less a line at a time, since a
    // screen's worth at once would clear it rather than scroll it, then the
    // lines from `at` down put back.
    let mut left = place.rows as usize;
    while left > 0 {
        let step = left.min(lines.saturating_sub(1).max(1));
        grid.scroll_up(&(Line(0)..Line(lines as i32)), step);
        left -= step;
    }
    for (row, line) in kept.into_iter().zip(at..) {
        grid[Line(line)] = row;
    }
    let top = -(grid.history_size() as i32);
    let cols = (place.cols as usize).min(columns);
    for row in 0..place.rows {
        let line = at - place.rows as i32 + row as i32;
        if line < top {
            continue;
        }
        for col in 0..columns {
            grid[Line(line)][Column(col)] = if col < cols { graphics::placeholder_cell(place.id, place.placement, row, col as u32) } else { Cell::default() };
        }
    }
}

/// Shows a sixel a program sent down the tab's pipe, since ConPTY drops one
/// written to the terminal, where a terminal would show it, as far as
/// ConPTY allows: it never learns the picture is there and keeps its cursor
/// where the program's text left it, drawing on from there, so nothing from
/// the cursor's line down may move. In DECSDM's display mode it goes at
/// the screen's top left, as from the terminal, and on the alternate screen
/// at the cursor, which stays (a full-screen program puts its cursor where
/// it draws next). On the main screen it goes in lines of its own above the
/// cursor's line, so that line comes below it, as xterm leaves a sixel's
/// cursor; above the prompt where the shell is back at one, as it is once a
/// sixel program's output sent here (`img2sixel a.png > \\.\pipe\…`) has
/// come, the shell drawing its prompt meanwhile; and with sixelScrollsRight,
/// or the cursor mid-line, from the cursor, rising over the lines above as
/// an iTerm2 picture does (`rise_into_place`).
fn show_piped_sixel(link: &Link, image: Image) {
    let mut emu = link.emu.lock();
    let size = *link.size.lock();
    let then = if emu.sixel_display {
        Then::Top
    } else if emu.term.mode().contains(TermMode::ALT_SCREEN) {
        Then::Stay
    } else {
        let cursor = emu.term.grid().cursor.point;
        let history = emu.term.grid().history_size() as i64;
        // At a prompt only with the cursor on the line typing starts on: a
        // line typed ahead while another ran has `at_prompt` true all
        // through it in PowerShell, cmd and BusyBox, which don't say when a
        // command starts (133;C).
        let st = link.state.lock();
        let prompt = st
            .input_start
            .filter(|&(line, _)| st.at_prompt && line == history + cursor.line.0 as i64)
            .map(|(line, _)| st.prompt_line.filter(|&first| first <= line).unwrap_or(line) - history)
            .filter(|&line| line >= 0)
            .map(|line| line as i32);
        drop(st);
        match prompt {
            Some(line) => Then::Above(line),
            None if emu.sixel_right || cursor.column.0 > 0 => Then::Beside,
            None => Then::Above(cursor.line.0),
        }
    };
    show_picture(&mut emu, &link.images, size, image, ((0, 0), false), then);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_question_a_command_waits_at_is_told_from_other_lines() {
        for asked in ["Do you want to continue? [Y/n] ", "Overwrite file? (y/n)", "Proceed ([y]/n)? ", "Press any key to continue . . .",
            "Are you sure you want to continue connecting (yes/no/[fingerprint])? ", "Remove 3 packages? [y/N]",
            r#"[Y] Yes  [A] Yes to All  [N] No  [L] No to All  [S] Suspend  [?] Help (default is "Y"): "#, r#"elp (default is "N"):"#,
            "Are you sure (Y/N)? ", "[Y,N]?"] {
            assert_eq!(question(asked), Some(Question::Answer), "{asked}");
        }
        assert_eq!(question("[sudo] password for bob: "), Some(Question::Password));
        for line in ["bob@host ~ % ", "Reading package lists... Done", "y/n is a fraction", r#"  -o  Where it goes (default is "out")."#, ""] {
            assert_eq!(question(line), None, "{line}");
        }
    }

    #[test]
    fn a_password_prompt_is_told_from_other_lines() {
        for prompt in [
            "[sudo] password for bob: ",
            "Password:",
            "bob@build.local's password: ",
            "Enter passphrase for key '/Users/bob/.ssh/id_ed25519': ",
            "doas (bob@host) password: ",
            "Password for 'https://github.com':",
            "[sudo] bob のパスワード:",
        ] {
            assert!(is_password_prompt(prompt), "{prompt}");
        }
        for line in ["bob@host ~ % ", "Changing password for bob.", "password: hunter2", "Sorry, try again.", "grep password notes.txt", ""] {
            assert!(!is_password_prompt(line), "{line}");
        }
    }

    /// The reader thread's work on `bytes`, without a PTY.
    #[test]
    fn a_failed_command_is_kept_with_the_end_of_its_output_until_the_next_starts() {
        let writer = ToShell::direct(Box::new(Vec::new()));
        let state = Arc::new(Mutex::new(ShellState { nonce: "5eed".into(), ..ShellState::default() }));
        let size = Arc::new(Mutex::new(WindowSize { num_lines: 8, num_cols: 40, cell_width: 10, cell_height: 20 }));
        let proxy = EventProxy { ink: tab_ink(), writer: writer.clone(), state: state.clone(), size: size.clone(), waker: Waker::from_fn(|_| {}) };
        let term = Term::new(TermConfig { scrolling_history: 100, ..TermConfig::default() }, &GridSize { cols: 40, rows: 8 }, proxy);
        let mut emu = Emulator::new(term);
        let replies = Replies { writer, size, images: Arc::new(Mutex::new(ImageStore::default())), models: Arc::new(Mutex::new(ModelStore::default())) };
        // A prompt, the line as the shell ran it, its output, then how it ended.
        let run = |emu: &mut Emulator, command: &str, output: &str, code: i32| {
            feed(emu, &state, &replies, format!("\x1b]133;A\x07$ \x1b]133;B\x07{command}\r\n\x1b]633;E;{command};5eed\x07\x1b]133;C;5eed\x07{output}\x1b]133;D;{code};5eed\x07").as_bytes());
        };
        run(&mut emu, "ls nothere", "ls: nothere: No such file or directory\r\n", 1);
        let failed = state.lock().failed.clone().expect("kept");
        assert_eq!((failed.command.as_str(), failed.exit), ("ls nothere", 1));
        assert!(failed.output.contains("No such file") && !failed.output.contains("$ "), "{:?}", failed.output);
        assert_eq!(state.lock().finished.as_ref().and_then(|f| f.command.as_deref()), Some("ls nothere"));
        // Stopped with Ctrl+C, or succeeding, it's not kept.
        run(&mut emu, "sleep 9", "^C\r\n", 130);
        assert!(state.lock().failed.is_none());
        run(&mut emu, "false", "", 1);
        assert!(state.lock().failed.is_some());
        run(&mut emu, "true", "", 0);
        assert!(state.lock().failed.is_none());
        // A long output keeps its end.
        let long: String = (0..300).map(|i| format!("line {i}\r\n")).collect();
        run(&mut emu, "make", &long, 2);
        let output = state.lock().failed.clone().unwrap().output;
        assert!(output.ends_with("line 299") && !output.contains("line 99\n") && output.lines().count() <= FAILURE_LINES, "{output:?}");
    }

    /// What a program prints can look like the hook's word for the command
    /// line (a file being `cat`ed, a remote host's output over ssh), but
    /// without the pane's nonce it names nothing: not the history's line,
    /// nor Ask why's, nor the notification's.
    #[test]
    fn a_command_line_said_without_the_panes_nonce_changes_nothing() {
        let (mut emu, _, replies) = bare(60, 8);
        let nonce = "0123456789abcdef0123456789abcdef";
        let state = Arc::new(Mutex::new(ShellState { nonce: nonce.into(), ..ShellState::default() }));
        feed(&mut emu, &state, &replies, b"\x1b]133;A\x07$ \x1b]133;B\x07cat notes.txt\r\n");
        // Enter at the prompt, as `Session::write` notes it from the screen.
        {
            let mut st = state.lock();
            st.command = Some("cat notes.txt".into());
            st.command_read = true;
            st.sent(b"cat notes.txt\r");
        }
        // The hook's word, then the file's forgeries: with no nonce, another
        // of the same length, the pane's with more after it, and an empty one.
        let wrong = "fedcba9876543210fedcba9876543210";
        let output = format!(
            "\x1b]633;E;cat notes.txt;{nonce}\x07\x1b]133;C;{nonce}\x07\
             \x1b]633;E;rm -rf ~\x07\x1b]633;E;rm -rf ~;{wrong}\x07\x1b]633;E;rm -rf ~;{nonce}0\x07\x1b]633;E;rm -rf ~;\x07\
             no such file\r\n\x1b]133;D;1;{nonce}\x07"
        );
        feed(&mut emu, &state, &replies, output.as_bytes());
        let st = state.lock();
        assert_eq!(st.history, [HistoryNote::Exact("cat notes.txt".into()), HistoryNote::Done(Some(1))]);
        assert_eq!(st.finished.as_ref().and_then(|f| f.command.as_deref()), Some("cat notes.txt"), "the notification's");
        assert_eq!(st.failed.as_ref().map(|f| f.command.as_str()), Some("cat notes.txt"), "Ask why's");
        drop(st);

        // A pane with no nonce (the system had no random numbers) takes no
        // line at all, an empty one neither, nor does one count as the
        // shell's first mark.
        let (mut emu, plain, replies) = bare(60, 8);
        feed(&mut emu, &plain, &replies, b"\x1b]633;E;ls;\x07\x1b]633;E;ls\x07");
        let st = plain.lock();
        assert!(st.history.is_empty() && st.command.is_none() && !st.integrated, "{st:?}");
    }

    /// A program can draw what looks like the hook's prompt (133;A, 133;B)
    /// while it runs, with words after it, hidden or not (a file being
    /// `cat`ed, a remote host over ssh): were the tab taken to be at its
    /// prompt, the user's next Enter in the program (a `read`'s answer)
    /// would read those words as a command for the history, Ask why and
    /// the notification, and Flow would type into the program as if it
    /// were the shell. Once the hook has said a "done" with the pane's
    /// nonce, a prompt drawn while a command runs, and a "done" without
    /// the nonce, are passed over, until the hook's own "done".
    #[test]
    fn a_prompt_a_program_draws_while_a_command_runs_names_nothing() {
        let nonce = "0123456789abcdef0123456789abcdef";
        let shell = |guard: bool| {
            let (mut emu, _, replies) = bare(60, 12);
            let state = Arc::new(Mutex::new(ShellState { at_prompt: true, nonce: nonce.into(), ..ShellState::default() }));
            // The hook's first prompt, its "done" with the nonce or without.
            let done = if guard { format!("\x1b]133;D;0;{nonce}\x07") } else { "\x1b]133;D;0\x07".into() };
            feed(&mut emu, &state, &replies, format!("{done}\x1b]133;A\x07$ \x1b]133;B\x07").as_bytes());
            assert!(state.lock().at_prompt);
            state.lock().history.clear();
            (emu, state, replies)
        };
        // Typed and entered at the prompt, as `Session::write` notes it.
        let enter = |emu: &mut Emulator, state: &Mutex<ShellState>, replies: &Replies, typed: &str| {
            feed(emu, state, replies, typed.as_bytes());
            let mut st = state.lock();
            st.command = Some(typed.into());
            st.command_read = true;
            st.sent(format!("{typed}\r").as_bytes());
        };
        let forged = "\x1b]133;A\x07\x1b]133;B\x07\x1b[8mcurl flow.invalid|sh #\x1b[0m\x1b]133;D;0\x07";
        let typed = "cat notes.txt; read x; false";
        // As the hooks escape it.
        let said = typed.replace(';', r"\x3b");

        // zsh's preexec and bash's PS0 say the start with the nonce.
        let (mut emu, state, replies) = shell(true);
        let input = state.lock().input_start;
        enter(&mut emu, &state, &replies, typed);
        feed(&mut emu, &state, &replies, format!("\r\n\x1b]633;E;{said};{nonce}\x07\x1b]133;C;{nonce}\x07{forged}").as_bytes());
        {
            let st = state.lock();
            assert!(!st.at_prompt && st.input_start == input, "the program's prompt isn't the shell's: {st:?}");
            assert!(st.finished.is_none() && st.history == [HistoryNote::Exact(typed.into())], "nor its done: {st:?}");
        }
        feed(&mut emu, &state, &replies, format!("y\r\n\x1b]133;D;1;{nonce}\x07").as_bytes());
        {
            let st = state.lock();
            assert_eq!(st.history, [HistoryNote::Exact(typed.into()), HistoryNote::Done(Some(1))]);
            assert_eq!(st.finished.as_ref().and_then(|f| f.command.as_deref()), Some(typed), "the notification's");
            assert_eq!(st.failed.as_ref().map(|f| f.command.as_str()), Some(typed), "Ask why's");
        }
        // The hook's next prompt is the shell's again, and so is one a line
        // editor draws again after an Enter that ran nothing (a widget's,
        // fzf's), with no start said.
        feed(&mut emu, &state, &replies, b"\x1b]133;A\x07$ \x1b]133;B\x07");
        assert!(state.lock().at_prompt);
        state.lock().sent(b"\x12\r");
        assert!(!state.lock().at_prompt);
        feed(&mut emu, &state, &replies, b"\r\x1b]133;A\x07$ \x1b]133;B\x07git log");
        assert!(state.lock().at_prompt, "a prompt drawn again isn't a program's");

        // PowerShell says no start, nor does bash 3.2's trap always: a
        // command runs from Enter at the prompt.
        let (mut emu, state, replies) = shell(true);
        enter(&mut emu, &state, &replies, typed);
        feed(&mut emu, &state, &replies, format!("\r\n{forged}").as_bytes());
        assert!(!state.lock().at_prompt && state.lock().history.is_empty(), "{:?}", state.lock());
        feed(&mut emu, &state, &replies, format!("y\r\n\x1b]633;E;{said};{nonce}\x07\x1b]133;D;1;{nonce}\x07\x1b]133;A\x07$ \x1b]133;B\x07").as_bytes());
        let st = state.lock();
        assert_eq!(st.history, [HistoryNote::Exact(typed.into()), HistoryNote::Done(Some(1))]);
        assert!(st.at_prompt && st.finished.as_ref().and_then(|f| f.command.as_deref()) == Some(typed), "{st:?}");
        drop(st);

        // A hook that never says a "done" with the nonce (cmd's, ash's, a
        // bash whose PROMPT_COMMAND is read-only) has its marks taken as
        // they come, as before.
        let (mut emu, state, replies) = shell(false);
        enter(&mut emu, &state, &replies, typed);
        feed(&mut emu, &state, &replies, format!("\r\n\x1b]133;C\x07{forged}").as_bytes());
        assert!(state.lock().at_prompt && state.lock().history == [HistoryNote::Done(Some(0))]);
    }

    /// What's typed, echoed, then Enter, read off the screen as
    /// `Session::write` reads it.
    fn enter(emu: &mut Emulator, state: &Mutex<ShellState>, replies: &Replies, typed: &str) {
        feed(emu, state, replies, typed.as_bytes());
        let mut st = state.lock();
        st.sent(typed.as_bytes());
        let command = emu.input_from(st.input_start.unwrap());
        if !command.trim().is_empty() {
            st.command = Some(command.trim().to_string());
            st.command_read = true;
            st.output_from = Some(emu.cursor_abs_line() + 1);
            st.note(HistoryNote::Entered { command, cwd: None, at: SystemTime::now(), assistant: false });
        }
        st.sent(b"\r");
    }

    /// The commands the history noted as entered.
    fn entered(notes: &[HistoryNote]) -> Vec<String> {
        notes.iter().filter_map(|n| match n {
            HistoryNote::Entered { command, .. } => Some(command.clone()),
            _ => None,
        }).collect()
    }

    /// bash's history leaves some lines out (HISTCONTROL's ignorespace and
    /// ignoredups, HISTIGNORE), and for those its hook says only the newest
    /// line the history holds. A background job's output while the shell
    /// waits at its prompt can draw a prompt with words of its own after
    /// it, which Enter reads off the screen as the command: unless that's
    /// the newest line, the run names nothing, in the history, for the
    /// notification or for Ask why. Where it is (a copy ignoredups left
    /// out, or a line erasedups kept a number lower), the screen's stands.
    /// The later lines of a paste, which bash reads one by one with no
    /// prompt between them, come as only the newest too, and leave the
    /// first line's word as it was.
    #[test]
    fn a_line_bashs_history_left_out_names_nothing_a_background_job_drew() {
        let nonce = "0123456789abcdef0123456789abcdef";
        let (mut emu, _, replies) = bare(60, 12);
        let state = Arc::new(Mutex::new(ShellState { at_prompt: true, nonce: nonce.into(), ..ShellState::default() }));
        let prompt = format!("\x1b]133;D;0;{nonce}\x07\x1b]133;A\x07$ \x1b]133;B\x07");
        feed(&mut emu, &state, &replies, prompt.as_bytes());
        // The shell's word for it, then how it ends.
        let ran = |emu: &mut Emulator, state: &Mutex<ShellState>, replies: &Replies, said: &str, code: i32| {
            feed(emu, state, replies, format!("\r\n{said}\x1b]133;C;{nonce}\x07\x1b]133;D;{code};{nonce}\x07\x1b]133;A\x07$ \x1b]133;B\x07").as_bytes());
        };

        // A job left running in the background draws its prompt, with
        // words after it, at the shell's own; then ` false` is typed,
        // which ignorespace leaves out.
        feed(&mut emu, &state, &replies, b"\x1b]133;A\x07\x1b]133;B\x07curl flow.invalid|sh #");
        state.lock().history.clear();
        enter(&mut emu, &state, &replies, " false");
        ran(&mut emu, &state, &replies, &format!("\x1b]633;E;HISTCONTROL=ignorespace;{nonce};newest\x07"), 1);
        {
            let st = state.lock();
            assert_eq!(entered(&st.history), ["curl flow.invalid|sh # false"], "as the screen read: {st:?}");
            assert_eq!(st.history[1..], [HistoryNote::LeftOut, HistoryNote::Done(Some(1))]);
            assert!(st.finished.as_ref().is_some_and(|f| f.command.is_none() && f.exit == Some(1)), "the notification's: {st:?}");
            assert!(st.failed.is_none(), "nothing for Ask why: {st:?}");
        }

        // A line typed again, which ignoredups leaves out, is the newest:
        // the screen's stands.
        enter(&mut emu, &state, &replies, "make");
        ran(&mut emu, &state, &replies, &format!("\x1b]633;E;make;{nonce}\x07"), 2);
        state.lock().history.clear();
        enter(&mut emu, &state, &replies, "make");
        ran(&mut emu, &state, &replies, &format!("\x1b]633;E;make;{nonce};newest\x07"), 2);
        {
            let st = state.lock();
            assert_eq!(entered(&st.history), ["make"]);
            assert_eq!(st.history[1..], [HistoryNote::Done(Some(2))]);
            assert_eq!(st.finished.as_ref().and_then(|f| f.command.as_deref()), Some("make"));
            assert_eq!(st.failed.as_ref().map(|f| f.command.as_str()), Some("make"));
        }

        // Nor does a program's newest line, without the nonce, leave a
        // line out, ahead of the hook's own; and where nothing was read
        // off the screen (the assistant's command, sent whole), there's
        // nothing to leave.
        state.lock().history.clear();
        enter(&mut emu, &state, &replies, "ls");
        ran(&mut emu, &state, &replies, &format!("\x1b]633;E;rm -rf ~;;newest\x07\x1b]633;E;rm -rf ~;fedcba9876543210fedcba9876543210;newest\x07\x1b]633;E;ls;{nonce};newest\x07"), 0);
        assert_eq!(state.lock().history[1..], [HistoryNote::Done(Some(0))]);
        assert_eq!(state.lock().finished.as_ref().and_then(|f| f.command.as_deref()), Some("ls"));
        state.lock().history.clear();
        state.lock().sent(b"\r");
        ran(&mut emu, &state, &replies, &format!("\x1b]633;E;ls;{nonce};newest\x07"), 0);
        assert_eq!(state.lock().history, [HistoryNote::Done(Some(0))]);

        // A paste of two lines: the first's own word, then the second's as
        // only the newest, then its end.
        state.lock().history.clear();
        enter(&mut emu, &state, &replies, "echo flow-a; false");
        let said = format!("\x1b]633;E;echo flow-a;{nonce}\x07\x1b]133;C;{nonce}\x07flow-a\r\n\x1b]633;E;false;{nonce};newest\x07");
        ran(&mut emu, &state, &replies, &said, 1);
        let st = state.lock();
        assert_eq!(st.history[1..], [HistoryNote::Exact("echo flow-a".into()), HistoryNote::Done(Some(1))], "{st:?}");
        assert_eq!(st.finished.as_ref().and_then(|f| f.command.as_deref()), Some("echo flow-a"));
        assert_eq!(st.failed.as_ref().map(|f| f.command.as_str()), Some("echo flow-a"));
    }

    /// A line that runs nothing (Enter alone, blanks, a comment, a line
    /// the shell can't read) has no hook say a word of it, only its
    /// "done", with the last command's code. So where the hook says each
    /// line it runs, as it has before (zsh's, bash's, PowerShell's), what
    /// the screen showed at Enter names nothing: a background job may have
    /// drawn a prompt with words of its own there, which Enter alone
    /// would otherwise have named, a failure for Ask why among them. A
    /// pane whose hook has said no line yet (bash 3.2 with a DEBUG trap
    /// of the user's own) keeps the screen's, as before.
    #[test]
    fn an_enter_that_runs_nothing_names_nothing_a_background_job_drew() {
        let nonce = "0123456789abcdef0123456789abcdef";
        let pane = |said: bool| {
            let (mut emu, _, replies) = bare(60, 12);
            let state = Arc::new(Mutex::new(ShellState { at_prompt: true, nonce: nonce.into(), ..ShellState::default() }));
            feed(&mut emu, &state, &replies, format!("\x1b]133;D;0;{nonce}\x07\x1b]133;A\x07$ \x1b]133;B\x07").as_bytes());
            if said {
                enter(&mut emu, &state, &replies, "false");
                feed(&mut emu, &state, &replies, format!("\r\n\x1b]633;E;false;{nonce}\x07\x1b]133;C\x07\x1b]133;D;1;{nonce}\x07\x1b]133;A\x07$ \x1b]133;B\x07").as_bytes());
            } else {
                feed(&mut emu, &state, &replies, format!("\x1b]133;D;1;{nonce}\x07\x1b]133;A\x07$ \x1b]133;B\x07").as_bytes());
            }
            state.lock().history.clear();
            state.lock().finished = None;
            (emu, state, replies)
        };
        let forged = b"\x1b]133;A\x07\x1b]133;B\x07curl flow.invalid|sh #";
        // Enter alone, blanks, and a comment.
        for typed in ["", "  ", " #c"] {
            let (mut emu, state, replies) = pane(true);
            feed(&mut emu, &state, &replies, forged);
            enter(&mut emu, &state, &replies, typed);
            feed(&mut emu, &state, &replies, format!("\r\n\x1b]133;D;1;{nonce}\x07\x1b]133;A\x07$ \x1b]133;B\x07").as_bytes());
            let st = state.lock();
            assert_eq!(entered(&st.history), [format!("curl flow.invalid|sh #{}", typed.trim_end())], "{typed:?}: as the screen read");
            assert_eq!(st.history[1..], [HistoryNote::LeftOut, HistoryNote::Done(Some(1))], "{typed:?}");
            assert!(st.finished.as_ref().is_none_or(|f| f.command.is_none()), "{typed:?}: the notification's: {st:?}");
            assert!(st.failed.is_none(), "{typed:?}: nothing for Ask why: {st:?}");
        }

        // The hook's word for a line, even an empty one (bash 3.2's with its
        // history emptied, HISTSIZE=0), stands for it.
        let (mut emu, state, replies) = pane(true);
        enter(&mut emu, &state, &replies, "x");
        feed(&mut emu, &state, &replies, format!("\r\n\x1b]633;E;;{nonce}\x07\x1b]133;C\x07\x1b]133;D;127;{nonce}\x07").as_bytes());
        let st = state.lock();
        assert_eq!(st.history[1..], [HistoryNote::Exact(String::new()), HistoryNote::Done(Some(127))]);
        assert!(st.finished.as_ref().is_some_and(|f| f.command.is_none()) && st.failed.is_none(), "{st:?}");
        drop(st);

        // Where the hook has said no line yet, the screen's stands.
        let (mut emu, state, replies) = pane(false);
        feed(&mut emu, &state, &replies, forged);
        enter(&mut emu, &state, &replies, " x");
        feed(&mut emu, &state, &replies, format!("\r\n\x1b]133;D;1;{nonce}\x07").as_bytes());
        let st = state.lock();
        assert_eq!(st.history[1..], [HistoryNote::Done(Some(1))], "{st:?}");
        assert_eq!(st.failed.as_ref().map(|f| f.command.as_str()), Some("curl flow.invalid|sh # x"));
    }

    /// bash's hook says, before a prompt, that its line editor may take
    /// each byte past ASCII as Meta and a key there (its locale has just
    /// changed into one that isn't UTF-8): until that line is done, a
    /// letter past ASCII isn't typed into it, nor at all into a shell
    /// whose hook never showed its prompt, nor into a bash or zsh whose
    /// hook has never said a "done" with the nonce, which each says before
    /// every prompt it runs at, the first too.
    #[test]
    fn a_prompt_bash_says_may_take_letters_as_keys_is_typed_none_until_its_done() {
        let nonce = "0123456789abcdef0123456789abcdef";
        let (mut emu, _, replies) = bare(60, 12);
        let state = Arc::new(Mutex::new(ShellState { at_prompt: true, nonce: nonce.into(), ..ShellState::default() }));
        let keys = |state: &Mutex<ShellState>| {
            let st = state.lock();
            (st.letters_may_be_keys(true), st.letters_may_be_keys(false))
        };
        assert_eq!(keys(&state), (true, true), "before the shell's first mark");
        let prompt = |meta: bool| format!("\x1b]133;D;0;{nonce}\x07{}\x1b]133;A\x07$ \x1b]133;B\x07", if meta { "\x1b]633;P;FlowMetaKeys=1\x07" } else { "" });
        feed(&mut emu, &state, &replies, prompt(true).as_bytes());
        assert!(state.lock().meta_keys && state.lock().at_prompt);
        assert_eq!(keys(&state), (true, true));
        feed(&mut emu, &state, &replies, prompt(false).as_bytes());
        assert!(!state.lock().meta_keys, "for that prompt alone");
        assert_eq!(keys(&state), (false, false));
        // Anything may say it (it only has Flow type less), and a "done"
        // only the hook's; nor is it a shell's first mark.
        feed(&mut emu, &state, &replies, b"\x1b]633;P;FlowMetaKeys=1\x07\x1b]133;D;0\x07");
        assert!(state.lock().meta_keys);
        let (mut emu, fresh, replies) = bare(60, 12);
        feed(&mut emu, &fresh, &replies, b"\x1b]633;P;FlowMetaKeys=1\x07");
        assert!(fresh.lock().meta_keys && !fresh.lock().integrated);

        // A bash whose PROMPT_COMMAND is read-only draws the hook's prompt,
        // but runs nothing before it, so never says a "done" with the
        // nonce: its readline may have put a C locale's settings back. And a
        // prompt any program can print (a zsh whose startup files `exec`ed
        // bash, `cat` of a file holding the marks) is no zsh hook's either.
        let (mut emu, _, replies) = bare(60, 12);
        let state = Arc::new(Mutex::new(ShellState { at_prompt: true, nonce: nonce.into(), ..ShellState::default() }));
        feed(&mut emu, &state, &replies, b"\x1b]133;A\x07$ \x1b]133;B\x07");
        assert!(state.lock().integrated && state.lock().at_prompt);
        assert_eq!(keys(&state), (true, false), "bash's and zsh's, whose hooks say a done before every prompt; not another shell's");
        // Nor does a "done" without the nonce, a program's, stand for one.
        feed(&mut emu, &state, &replies, b"\x1b]133;D;0\x07\x1b]133;A\x07$ \x1b]133;B\x07");
        assert_eq!(keys(&state), (true, false));
    }

    #[test]
    fn on_the_mac_a_spacing_mark_goes_on_the_letter_before_as_the_shells_measure_it() {
        // "मेरा नाम": म with its sign े (no width anywhere), र with its sign ा
        // (a cell of its own to unicode-width and glibc, none to the Mac's
        // wcwidth), a space, न with ा, and म.
        let (mut emu, state, replies) = bare(20, 2);
        feed(&mut emu, &state, &replies, "\u{092E}\u{0947}\u{0930}\u{093E} \u{0928}\u{093E}\u{092E}".as_bytes());
        let grid = emu.term.grid();
        let cell = |col: usize| {
            let cell = &grid[Line(0)][Column(col)];
            format!("{}{}", cell.c, cell.zerowidth().unwrap_or(&[]).iter().collect::<String>())
        };
        let cells: Vec<String> = (0..grid.cursor.point.column.0).map(cell).collect();
        if cfg!(target_os = "macos") {
            assert_eq!(cells, ["\u{092E}\u{0947}", "\u{0930}\u{093E}", " ", "\u{0928}\u{093E}", "\u{092E}"]);
        } else {
            assert_eq!(cells, ["\u{092E}\u{0947}", "\u{0930}", "\u{093E}", " ", "\u{0928}", "\u{093E}", "\u{092E}"]);
        }
        // The text reads back the same either way.
        assert_eq!(emu.recent_text(1).trim_end(), "\u{092E}\u{0947}\u{0930}\u{093E} \u{0928}\u{093E}\u{092E}");
    }

    #[test]
    fn a_tab_is_at_its_prompt_only_once_one_is_drawn_and_until_a_command_starts() {
        let (mut emu, _, replies) = bare(40, 8);
        // As a session starts: at its prompt until the shell says anything.
        let state = Arc::new(Mutex::new(ShellState { at_prompt: true, ..ShellState::default() }));
        // bash's PROMPT_COMMAND says the last command's end and the folder a
        // moment before its prompt: not at one yet.
        feed(&mut emu, &state, &replies, b"\x1b]133;D;0\x07\x1b]633;P;Cwd=/home\x07");
        assert!(state.lock().integrated && !state.lock().at_prompt);
        feed(&mut emu, &state, &replies, b"\x1b]133;A\x07$ \x1b]133;B\x07");
        assert!(state.lock().at_prompt);
        // A command typed ahead while another ran: entered (as `sent` has
        // it), the earlier one's prompt drawn, then the queued line starting
        // (133;C, from bash's PS0) takes the tab off its prompt.
        state.lock().sent(b"sleep 9\r");
        assert!(!state.lock().at_prompt);
        feed(&mut emu, &state, &replies, b"\r\n\x1b]133;D;0\x07\x1b]133;A\x07$ \x1b]133;B\x07");
        assert!(state.lock().at_prompt, "the earlier command's prompt");
        feed(&mut emu, &state, &replies, b"\x1b]633;E;sleep 9\x07\x1b]133;C\x07");
        assert!(!state.lock().at_prompt, "the queued line runs");
    }

    #[test]
    fn a_shell_is_drawing_its_prompt_from_a_commands_end_until_the_prompt_shows() {
        let (mut emu, _, replies) = bare(40, 8);
        let state = Arc::new(Mutex::new(ShellState { at_prompt: true, ..ShellState::default() }));
        let within = Duration::from_secs(3);
        let drawing = || state.lock().drawing_prompt(within);
        assert!(!drawing(), "a shell that's said nothing");
        feed(&mut emu, &state, &replies, b"\x1b]133;A\x07$ \x1b]133;B\x07");
        assert!(!drawing(), "at its prompt");
        feed(&mut emu, &state, &replies, b"true\r\n\x1b]133;C\x07");
        assert!(!drawing(), "running");
        // Its end and folder said, the prompt (which a theme may take a
        // moment over) not yet drawn: neither running nor at its prompt.
        feed(&mut emu, &state, &replies, b"\x1b]133;D;0\x07\x1b]633;P;Cwd=/home\x07");
        assert!(drawing() && !state.lock().at_prompt);
        feed(&mut emu, &state, &replies, b"\x1b]133;A\x07$ ");
        assert!(drawing(), "its start is drawn, not its end");
        feed(&mut emu, &state, &replies, b"\x1b]133;B\x07");
        assert!(!drawing() && state.lock().at_prompt);
        // A command starting next ends it too, and one whose prompt never
        // comes is given up on.
        feed(&mut emu, &state, &replies, b"\x1b]133;D;0\x07\x1b]133;C\x07");
        assert!(!drawing());
        feed(&mut emu, &state, &replies, b"\x1b]133;D;0\x07");
        assert!(drawing());
        if let Some(long_ago) = Instant::now().checked_sub(within) {
            state.lock().prompt_due = Some(long_ago);
            assert!(!drawing());
        }
    }

    #[test]
    fn each_command_marks_the_line_its_prompt_began_on_done_or_failed() {
        use crate::marks::Mark;
        let writer = ToShell::direct(Box::new(Vec::new()));
        let state = Arc::new(Mutex::new(ShellState::default()));
        let size = Arc::new(Mutex::new(WindowSize { num_lines: 8, num_cols: 40, cell_width: 10, cell_height: 20 }));
        let proxy = EventProxy { ink: tab_ink(), writer: writer.clone(), state: state.clone(), size: size.clone(), waker: Waker::from_fn(|_| {}) };
        let term = Term::new(TermConfig { scrolling_history: 100, ..TermConfig::default() }, &GridSize { cols: 40, rows: 8 }, proxy);
        let mut emu = Emulator::new(term);
        let replies = Replies { writer, size, images: Arc::new(Mutex::new(ImageStore::default())), models: Arc::new(Mutex::new(ModelStore::default())) };
        // A two-line prompt, as some themes draw: the mark goes on its first.
        let run = |emu: &mut Emulator, command: &str, output: &str, code: i32| {
            feed(emu, &state, &replies, format!("\x1b]133;A\x07~/src\r\n$ \x1b]133;B\x07{command}\r\n\x1b]133;C\x07{output}\x1b]133;D;{code}\x07").as_bytes());
        };
        run(&mut emu, "ls", "a\r\nb\r\n", 0);
        run(&mut emu, "false", "", 1);
        let long: String = (0..20).map(|i| format!("{i}\r\n")).collect();
        run(&mut emu, "seq 20", &long, 0);
        let marks = crate::marks::all(emu.term.grid());
        assert_eq!(marks, [(0, Mark::Done), (4, Mark::Failed), (6, Mark::Done)], "the prompts' first lines, into the scrollback");
        let text = |line: usize| emu.text_between(line as i64, line as i64 + 1);
        assert!(marks.iter().all(|&(l, _)| text(l).starts_with("~/src")), "{:?}", marks.iter().map(|&(l, _)| text(l)).collect::<Vec<_>>());
        // A prompt with nothing run leaves none; a command still running is.
        feed(&mut emu, &state, &replies, b"\x1b]133;A\x07~/src\r\n$ \x1b]133;B\x07\r\n");
        feed(&mut emu, &state, &replies, b"\x1b]133;A\x07~/src\r\n$ \x1b]133;B\x07sleep 9\r\n\x1b]133;C\x07");
        let marks = crate::marks::all(emu.term.grid());
        assert_eq!((marks.len(), marks.last().map(|m| m.1)), (4, Some(Mark::Running)));
        // A full-screen program's own 133;A marks nothing on its screen.
        feed(&mut emu, &state, &replies, b"\x1b]133;D;0\x07\x1b[?1049h\x1b]133;A\x07x\x1b]133;C\x07\x1b[?1049l");
        assert_eq!(crate::marks::all(emu.term.grid()).len(), 4);
    }

    #[test]
    fn the_tail_of_a_long_output_is_cut_where_a_letter_starts() {
        let text = "あ".repeat(10_000);
        let tail = tail_of(&text);
        assert!(tail.len() <= FAILURE_BYTES && tail.chars().all(|c| c == 'あ'));
    }

    fn feed(e: &mut Emulator, state: &Mutex<ShellState>, replies: &Replies, bytes: &[u8]) {
        let mut scanner = OscScanner::default();
        for piece in scanner.feed(bytes) {
            match piece {
                Piece::Data(a, b) => advance(e, &bytes[a..b]),
                Piece::Drop(..) => {}
                Piece::Marker(marker) => handle_marker(e, state, replies, marker),
            }
        }
    }

    #[test]
    fn a_model_is_placed_on_its_center_cell_moves_when_placed_again_and_clears() {
        use base64::Engine as _;
        let writer = ToShell::direct(Box::new(Vec::new()));
        let state = Arc::new(Mutex::new(ShellState::default()));
        let size = Arc::new(Mutex::new(WindowSize { num_lines: 6, num_cols: 20, cell_width: 10, cell_height: 20 }));
        let proxy = EventProxy { ink: tab_ink(), writer: writer.clone(), state: state.clone(), size: size.clone(), waker: Waker::from_fn(|_| {}) };
        let term = Term::new(TermConfig { scrolling_history: 100, ..TermConfig::default() }, &GridSize { cols: 20, rows: 6 }, proxy);
        let mut emu = Emulator::new(term);
        let (images, models) = (Arc::new(Mutex::new(ImageStore::default())), Arc::new(Mutex::new(ModelStore::default())));
        let replies = Replies { writer, size, images: images.clone(), models: models.clone() };
        let square = base64::engine::general_purpose::STANDARD.encode(model::tests::SQUARE);
        let send = |emu: &mut Emulator, cmd: &str| feed(emu, &state, &replies, format!("\x1b_ratty;g;{cmd}\x1b\\").as_bytes());
        // What a cell shows: the image and the row and column in it.
        let at = |emu: &Emulator, line: i32, col: usize| graphics::read_placeholder(&emu.term.grid()[Line(line)][Column(col)]).map(|m| (m.id, m.row, m.col));

        feed(&mut emu, &state, &replies, b"hello");
        send(&mut emu, &format!("r;id=5;fmt=obj;source=payload;more=0;{square}"));
        send(&mut emu, "p;id=5;row=2;col=10;w=4;h=3;ry=30");
        // Centered on row 2, column 10: rows 1 to 3, columns 8 to 11.
        let image = at(&emu, 1, 8).expect("the top-left cell").0;
        assert_eq!((at(&emu, 1, 8), at(&emu, 3, 11)), (Some((image, Some(0), Some(0))), Some((image, Some(2), Some(3)))));
        assert_eq!((at(&emu, 0, 8), at(&emu, 1, 7), at(&emu, 4, 8), at(&emu, 1, 12)), (None, None, None, None));
        assert_eq!(emu.term.grid().cursor.point.column.0, 5, "the cursor stays where the text left it");
        let (picture, _) = images.lock().lookup(image, 0).unwrap();
        assert_eq!((picture.width, picture.height), (40, 60));
        let shown = picture.model.clone().expect("a model's picture");
        assert_eq!(shown.style().rotation, [0.0, 30.0, 0.0]);
        assert!(shown.mesh().is_some(), "registered before it was placed");

        send(&mut emu, "u;id=5;ry=90;animate=1");
        assert_eq!((shown.style().rotation[1], shown.animating(), shown.version()), (90.0, true, 1));

        // Placed again: it moves, and the cells it left are blank.
        send(&mut emu, "p;id=5;row=0;col=0;w=2;h=2");
        assert_eq!((at(&emu, 1, 8), at(&emu, 3, 11)), (None, None));
        let moved = at(&emu, 0, 0).expect("at the top-left corner").0;
        assert_ne!(moved, image);
        assert!(images.lock().lookup(image, 0).is_none(), "the old picture is let go");
        assert_eq!(at(&emu, 1, 1).map(|(_, r, c)| (r, c)), Some((Some(1), Some(1))), "pushed back onto the screen at its edge, as Ratty does");
        // Past what a tab may cover, a placement is turned down.
        send(&mut emu, "p;id=6;row=3;col=10;w=200;h=200");
        send(&mut emu, "p;id=7;row=3;col=10;w=200;h=200");
        send(&mut emu, "p;id=8;row=3;col=10;w=200;h=200");
        // 2000 × 4000 pixels each: the first fits beside the small one, the
        // next two don't.
        let shown: Vec<u32> = { let mut ids: Vec<u32> = models.lock().shown.keys().copied().collect(); ids.sort(); ids };
        assert_eq!(shown, [5, 6]);

        send(&mut emu, "d");
        assert_eq!(at(&emu, 0, 0), None);
        assert!(models.lock().shown.is_empty());
        let size = *replies.size.lock();
        assert_eq!(handle_model(&mut emu, &state, (&models, &images), size, b"s"), Some(model::SUPPORT));
    }

    #[test]
    fn a_model_named_by_a_path_on_another_computer_is_never_asked_about() {
        let writer = ToShell::direct(Box::new(Vec::new()));
        let state = Arc::new(Mutex::new(ShellState::default()));
        let size = Arc::new(Mutex::new(WindowSize { num_lines: 6, num_cols: 20, cell_width: 10, cell_height: 20 }));
        let proxy = EventProxy { ink: tab_ink(), writer: writer.clone(), state: state.clone(), size: size.clone(), waker: Waker::from_fn(|_| {}) };
        let term = Term::new(TermConfig { scrolling_history: 100, ..TermConfig::default() }, &GridSize { cols: 20, rows: 6 }, proxy);
        let mut emu = Emulator::new(term);
        let (images, models) = (Arc::new(Mutex::new(ImageStore::default())), Arc::new(Mutex::new(ModelStore::default())));
        let replies = Replies { writer, size, images, models: models.clone() };
        let send = |emu: &mut Emulator, cmd: &str| feed(emu, &state, &replies, format!("\x1b_ratty;g;{cmd}\x1b\\").as_bytes());
        let asked = || models.lock().take_asks().into_iter().map(|a| a.path).collect::<Vec<_>>();
        // Never asked about: on Windows that's \\evil.example\share, where
        // a yes would hand it the user's sign-in.
        send(&mut emu, "r;id=1;fmt=obj;path=//evil.example/share/m.obj");
        send(&mut emu, "r;id=2;fmt=glb;path=\\\\evil.example\\share\\m.glb");
        assert!(asked().is_empty());
        // Nor from a folder a program said the shell is in.
        state.lock().cwd = Some("//evil.example/share".into());
        send(&mut emu, "r;id=3;fmt=obj;path=m.obj");
        assert!(asked().is_empty());
        // A file here is asked about, as ever.
        let here = if cfg!(windows) { "C:/models/m.obj" } else { "/tmp/models/m.obj" };
        send(&mut emu, &format!("r;id=4;fmt=obj;path={here}"));
        assert_eq!(asked(), [PathBuf::from(here)]);
    }

    #[test]
    fn a_click_on_a_model_that_asked_for_clicks_is_answered_with_where_it_landed() {
        use base64::Engine as _;
        let writer = ToShell::direct(Box::new(Vec::new()));
        let state = Arc::new(Mutex::new(ShellState::default()));
        let size = Arc::new(Mutex::new(WindowSize { num_lines: 6, num_cols: 20, cell_width: 10, cell_height: 20 }));
        let proxy = EventProxy { ink: tab_ink(), writer: writer.clone(), state: state.clone(), size: size.clone(), waker: Waker::from_fn(|_| {}) };
        let term = Term::new(TermConfig { scrolling_history: 100, ..TermConfig::default() }, &GridSize { cols: 20, rows: 6 }, proxy);
        let mut emu = Emulator::new(term);
        let (images, models) = (Arc::new(Mutex::new(ImageStore::default())), Arc::new(Mutex::new(ModelStore::default())));
        let replies = Replies { writer, size, images: images.clone(), models: models.clone() };
        let square = base64::engine::general_purpose::STANDARD.encode(model::tests::SQUARE);
        let send = |emu: &mut Emulator, cmd: &str| feed(emu, &state, &replies, format!("\x1b_ratty;g;{cmd}\x1b\\").as_bytes());
        send(&mut emu, &format!("r;id=5;fmt=obj;source=payload;more=0;{square}"));
        // Over rows 0 to 3 and columns 1 to 4: a picture 40 × 80, the square
        // 36 pixels a side in its middle.
        send(&mut emu, "p;id=5;row=2;col=3;w=4;h=4");
        let click = |emu: &Emulator, cell, within| model_click(picture_cell(emu, cell)?, (&models, &images), within, (10.0, 20.0), model::Button::Left);
        assert_eq!(click(&emu, (2, 3), (0.5, 0.5)), None, "it didn't ask for clicks");
        send(&mut emu, "u;id=5;clicks=1");
        // The picture's middle is the top left corner of cell (2, 3).
        let answer = click(&emu, (2, 3), (0.0, 0.0)).expect("on the square");
        assert_eq!(answer, "\x1b_ratty;g;click;id=5;button=left;x=0.0000;y=0.0000;z=0.0000\x1b\\");
        // 9 pixels right and 9 up of it: a quarter of the way to the
        // square's right edge and to its top.
        let answer = click(&emu, (1, 3), (0.9, 0.55)).expect("on the square");
        assert_eq!(answer, "\x1b_ratty;g;click;id=5;button=left;x=0.2500;y=0.2500;z=0.0000\x1b\\");
        assert_eq!(click(&emu, (0, 1), (0.1, 0.1)), None, "in the picture but off the square");
        assert_eq!(click(&emu, (0, 0), (0.5, 0.5)), None, "not on the model");
        assert_eq!(click(&emu, (9, 0), (0.5, 0.5)), None, "off the screen");
    }

    #[test]
    fn a_model_comes_back_cell_by_cell_when_conpty_repaints_over_it() {
        use base64::Engine as _;
        let writer = ToShell::direct(Box::new(Vec::new()));
        let state = Arc::new(Mutex::new(ShellState::default()));
        let size = Arc::new(Mutex::new(WindowSize { num_lines: 6, num_cols: 20, cell_width: 10, cell_height: 20 }));
        let proxy = EventProxy { ink: tab_ink(), writer: writer.clone(), state: state.clone(), size: size.clone(), waker: Waker::from_fn(|_| {}) };
        let term = Term::new(TermConfig { scrolling_history: 100, ..TermConfig::default() }, &GridSize { cols: 20, rows: 6 }, proxy);
        let mut emu = Emulator::new(term);
        emu.conpty = true;
        let (images, models) = (Arc::new(Mutex::new(ImageStore::default())), Arc::new(Mutex::new(ModelStore::default())));
        let replies = Replies { writer, size, images, models: models.clone() };
        let square = base64::engine::general_purpose::STANDARD.encode(model::tests::SQUARE);
        let send = |emu: &mut Emulator, cmd: &str| feed(emu, &state, &replies, format!("\x1b_ratty;g;{cmd}\x1b\\").as_bytes());
        // Which cells show a model, and the text there is.
        let model_cells = |emu: &Emulator| {
            let grid = emu.term.grid();
            (0..6).flat_map(|l| (0..20).map(move |c| (l, c))).filter(|&(l, c)| graphics::read_placeholder(&grid[Line(l)][Column(c)]).is_some()).collect::<Vec<_>>()
        };
        let line = |emu: &Emulator, l: i64| emu.text_between(l, l + 1).trim_end().to_string();
        // ConPTY's repaint of the main screen as it knows it: its text, and
        // nothing where the model is, which it never saw.
        let repaint = "\x1b[?25l\x1b[Hhello\x1b[K\r\n\x1b[K\r\n\x1b[K\x1b[3;11Hbeside\x1b[K\r\n\x1b[K\r\n\x1b[K\r\n\x1b[K\x1b[4;1H\x1b[?25h";
        feed(&mut emu, &state, &replies, b"hello\x1b[3;11Hbeside\x1b[4;1H");
        send(&mut emu, &format!("r;id=5;fmt=obj;source=payload;more=0;{square}"));
        // Rows 1 to 3, columns 1 to 4: on the line with "beside" after it.
        send(&mut emu, "p;id=5;row=2;col=3;w=4;h=3");
        let placed = model_cells(&emu);
        assert_eq!(placed.len(), 12);

        // A full-screen program, then ConPTY repaints the main screen.
        feed(&mut emu, &state, &replies, b"\x1b[?1049h\x1b[?25l\x1b[H#####\x1b[?25h");
        feed(&mut emu, &state, &replies, format!("\x1b[?1049l{repaint}").as_bytes());
        assert_eq!(model_cells(&emu), placed, "every cell back where it was");
        assert!(line(&emu, 0) == "hello" && line(&emu, 2).ends_with("beside"), "and the text ConPTY knows: {:?}", line(&emu, 2));

        // A resize's repaint, and one that writes over a cell of the model.
        emu.repaint_due();
        feed(&mut emu, &state, &replies, repaint.replace("\x1b[4;1H", "\x1b[2;2HX\x1b[4;1H").as_bytes());
        assert_eq!(model_cells(&emu).len(), 11, "the text written wins");
        assert!(line(&emu, 1).contains('X'));

        // Placed again while a repaint is due: the old copy doesn't come back.
        send(&mut emu, "p;id=5;row=2;col=3;w=4;h=3");
        emu.repaint_due();
        send(&mut emu, "p;id=5;row=4;col=16;w=2;h=2");
        feed(&mut emu, &state, &replies, repaint.as_bytes());
        assert!(model_cells(&emu).iter().all(|&(l, c)| l >= 3 && c >= 15), "{:?}", model_cells(&emu));

        // Deleted while one is due: nothing comes back.
        emu.repaint_due();
        send(&mut emu, "d;id=5");
        feed(&mut emu, &state, &replies, repaint.as_bytes());
        assert!(model_cells(&emu).is_empty());

        // On the alternate screen too, which a game's models are on.
        feed(&mut emu, &state, &replies, b"\x1b[?1049h\x1b[H@ game");
        send(&mut emu, "p;id=5;row=3;col=6;w=2;h=2");
        let game = model_cells(&emu);
        emu.repaint_due();
        feed(&mut emu, &state, &replies, b"\x1b[?25l\x1b[H@ game\x1b[K\r\n\x1b[K\r\n\x1b[K\r\n\x1b[K\x1b[?25h");
        assert_eq!(model_cells(&emu), game);
        // Not put back on the other screen, whose cells were never noted.
        emu.repaint_due();
        feed(&mut emu, &state, &replies, b"\x1b[?1049l\x1b[?25l\x1b[H\x1b[K\x1b[?25h");
        assert!(model_cells(&emu).is_empty());
    }

    #[test]
    fn a_model_is_kept_through_repaints_only_where_conpty_sits_between() {
        use base64::Engine as _;
        let writer = ToShell::direct(Box::new(Vec::new()));
        let state = Arc::new(Mutex::new(ShellState::default()));
        let size = Arc::new(Mutex::new(WindowSize { num_lines: 6, num_cols: 20, cell_width: 10, cell_height: 20 }));
        let proxy = EventProxy { ink: tab_ink(), writer: writer.clone(), state: state.clone(), size: size.clone(), waker: Waker::from_fn(|_| {}) };
        let term = Term::new(TermConfig { scrolling_history: 100, ..TermConfig::default() }, &GridSize { cols: 20, rows: 6 }, proxy);
        let mut emu = Emulator::new(term);
        let (images, models) = (Arc::new(Mutex::new(ImageStore::default())), Arc::new(Mutex::new(ModelStore::default())));
        let replies = Replies { writer, size, images, models };
        let square = base64::engine::general_purpose::STANDARD.encode(model::tests::SQUARE);
        feed(&mut emu, &state, &replies, format!("\x1b_ratty;g;r;id=5;fmt=obj;source=payload;more=0;{square}\x1b\\").as_bytes());
        feed(&mut emu, &state, &replies, b"\x1b_ratty;g;p;id=5;row=2;col=3;w=4;h=3\x1b\\");
        // A terminal's own program repaints nothing it didn't draw: what
        // blanks the screen there is the program's, and stays.
        emu.repaint_due();
        assert!(emu.repaint.is_none());
        feed(&mut emu, &state, &replies, b"\x1b[?25l\x1b[H\x1b[J\x1b[?25h");
        let grid = emu.term.grid();
        assert!((0..6).all(|l| (0..20).all(|c| graphics::read_placeholder(&grid[Line(l)][Column(c)]).is_none())));
    }

    #[test]
    fn a_font_change_draws_each_models_picture_again_at_the_new_size() {
        use base64::Engine as _;
        let writer = ToShell::direct(Box::new(Vec::new()));
        let state = Arc::new(Mutex::new(ShellState::default()));
        let size = Arc::new(Mutex::new(WindowSize { num_lines: 6, num_cols: 20, cell_width: 10, cell_height: 20 }));
        let proxy = EventProxy { ink: tab_ink(), writer: writer.clone(), state: state.clone(), size: size.clone(), waker: Waker::from_fn(|_| {}) };
        let term = Term::new(TermConfig { scrolling_history: 100, ..TermConfig::default() }, &GridSize { cols: 20, rows: 6 }, proxy);
        let mut emu = Emulator::new(term);
        let (images, models) = (Arc::new(Mutex::new(ImageStore::default())), Arc::new(Mutex::new(ModelStore::default())));
        let replies = Replies { writer, size, images: images.clone(), models: models.clone() };
        let square = base64::engine::general_purpose::STANDARD.encode(model::tests::SQUARE);
        feed(&mut emu, &state, &replies, format!("\x1b_ratty;g;r;id=5;fmt=obj;source=payload;more=0;{square}\x1b\\").as_bytes());
        feed(&mut emu, &state, &replies, b"\x1b_ratty;g;p;id=5;row=2;col=3;w=4;h=3\x1b\\");
        let image = models.lock().shown[&5].image;
        let (before, _) = images.lock().lookup(image, 0).unwrap();
        assert_eq!((before.width, before.height), (40, 60));
        let bigger = WindowSize { num_lines: 6, num_cols: 20, cell_width: 12, cell_height: 25 };
        repicture_models(&models, &images, bigger);
        let (after, placement) = images.lock().lookup(image, 0).unwrap();
        assert_eq!((after.width, after.height, placement.src), (48, 75, [0, 0, 48, 75]), "the same id, a picture the new cells' size");
        assert!(Arc::ptr_eq(before.model.as_ref().unwrap(), after.model.as_ref().unwrap()), "of the same placement");
        assert_eq!(models.lock().shown[&5].pixels, 48 * 75);
        // Past what a tab may cover, it keeps its picture, scaled.
        let huge = WindowSize { num_lines: 6, num_cols: 20, cell_width: 1000, cell_height: 1000 };
        repicture_models(&models, &images, huge);
        assert_eq!(images.lock().lookup(image, 0).unwrap().0.width, 48);
    }

    #[test]
    fn effects_wait_for_the_ui_with_where_the_screen_was() {
        let writer = ToShell::direct(Box::new(Vec::new()));
        let state = Arc::new(Mutex::new(ShellState::default()));
        let size = Arc::new(Mutex::new(WindowSize { num_lines: 6, num_cols: 20, cell_width: 10, cell_height: 20 }));
        let proxy = EventProxy { ink: tab_ink(), writer: writer.clone(), state: state.clone(), size: size.clone(), waker: Waker::from_fn(|_| {}) };
        let term = Term::new(TermConfig { scrolling_history: 100, ..TermConfig::default() }, &GridSize { cols: 20, rows: 6 }, proxy);
        let mut emu = Emulator::new(term);
        let replies = Replies { writer, size, images: Arc::new(Mutex::new(ImageStore::default())), models: Arc::default() };
        // Ten lines and the cursor's empty one on a six-line screen: five
        // went up into the scrollback.
        feed(&mut emu, &state, &replies, "line\r\n".repeat(10).as_bytes());
        feed(&mut emu, &state, &replies, b"\x1b]7731;fx=burst;at=3,2\x07");
        let asked = state.lock().effects[0];
        assert_eq!((asked.request.at, asked.top, asked.alt), ((3, 2), 5, false));
        // On the alternate screen, which keeps no scrollback.
        feed(&mut emu, &state, &replies, b"\x1b[?1049h\x1b]7731;fx=ripple;at=1,1\x07\x1b[?1049l");
        let asked = state.lock().effects[1];
        assert_eq!((asked.top, asked.alt), (0, true));
        // Any program may ask: it doesn't make the shell count as integrated.
        assert!(!state.lock().integrated);
        // A flood keeps only the latest few.
        for i in 0..12 {
            feed(&mut emu, &state, &replies, format!("\x1b]7731;fx=ripple;at={i},0\x07").as_bytes());
        }
        let effects = std::mem::take(&mut state.lock().effects);
        assert_eq!(effects.len(), MAX_EFFECTS);
        assert_eq!(effects.last().map(|asked| asked.request.at), Some((11, 0)));
    }

    #[test]
    fn deleting_every_picture_takes_only_those_on_screen() {
        let writer = ToShell::direct(Box::new(Vec::new()));
        let state = Arc::new(Mutex::new(ShellState::default()));
        let size = Arc::new(Mutex::new(WindowSize { num_lines: 6, num_cols: 20, cell_width: 10, cell_height: 20 }));
        let proxy = EventProxy { ink: tab_ink(), writer: writer.clone(), state: state.clone(), size: size.clone(), waker: Waker::from_fn(|_| {}) };
        let term = Term::new(TermConfig { scrolling_history: 100, ..TermConfig::default() }, &GridSize { cols: 20, rows: 6 }, proxy);
        let mut emu = Emulator::new(term);
        let images = Arc::new(Mutex::new(ImageStore::default()));
        let replies = Replies { writer, size, images: images.clone(), models: Arc::default() };
        let picture = |id: u32| format!("\x1b_Ga=T,f=24,s=1,v=1,c=2,r=1,i={id},q=2;AAAA\x1b\\");
        let delete_all = "\x1b_Ga=d,d=A,q=2\x1b\\";
        let shown = |id| images.lock().lookup(id, 0).is_some();

        // One picture scrolls into the scrollback; one stays on the screen.
        let main = format!("{}{}{}", picture(1), "\r\n".repeat(10), picture(2));
        feed(&mut emu, &state, &replies, main.as_bytes());
        // A game on the alternate screen shows its own and, leaving, deletes
        // every picture: only its own go.
        let game = format!("\x1b[?1049h{}{delete_all}\x1b[?1049l", picture(3));
        feed(&mut emu, &state, &replies, game.as_bytes());
        assert!(!shown(3));
        assert!(shown(1) && shown(2));
        // On the main screen, the one there goes and the scrollback's stays.
        feed(&mut emu, &state, &replies, delete_all.as_bytes());
        assert!(!shown(2) && shown(1));
    }

    /// A sixel `w` × `h` pixels, all red, `h` a multiple of six.
    fn red_sixel(w: u32, h: u32) -> String {
        let bands = vec![format!("!{w}~"); h.div_ceil(6) as usize].join("-");
        format!("\x1bPq\"1;1;{w};{h}#1;2;100;0;0{bands}\x1b\\")
    }

    /// The row and column of the picture cell at `line`, `col`, if it's one.
    fn picture_at(e: &Emulator, line: i32, col: usize) -> Option<(Option<u32>, Option<u32>)> {
        graphics::read_placeholder(&e.term.grid()[Line(line)][Column(col)]).map(|m| (m.row, m.col))
    }

    fn cursor(e: &Emulator) -> (i32, usize) {
        let point = e.term.grid().cursor.point;
        (point.line.0, point.column.0)
    }

    #[test]
    fn a_sixel_is_placed_at_the_cursor_at_its_own_size_and_the_cursor_goes_below_it_at_the_left() {
        let (mut e, state, replies) = bare(20, 6);
        feed(&mut e, &state, &replies, format!("ab{}", red_sixel(25, 30)).as_bytes());
        // 25 × 30 pixels in cells of 10 × 20: three columns and two rows,
        // from the cursor's column.
        assert_eq!((picture_at(&e, 0, 2), picture_at(&e, 1, 4)), (Some((Some(0), Some(0))), Some((Some(1), Some(2)))));
        assert_eq!((picture_at(&e, 0, 5), picture_at(&e, 2, 2), e.term.grid()[Line(0)][Column(1)].c), (None, None, 'b'));
        assert_eq!(cursor(&e), (2, 0), "the line below it, at the left, as xterm leaves it");
        let mark = graphics::read_placeholder(&e.term.grid()[Line(0)][Column(2)]).unwrap();
        let (image, placement) = replies.images.lock().lookup(mark.id, mark.placement).unwrap();
        assert_eq!((image.width, image.height, placement.natural, &image.pixels[..4]), (25, 30, true, &[255, 0, 0, 255][..]));
        // At the screen's foot it scrolls up as text would, the cursor
        // below it still.
        feed(&mut e, &state, &replies, format!("$\r\n\r\n\r\n{}", red_sixel(10, 60)).as_bytes());
        assert_eq!((picture_at(&e, 2, 0), picture_at(&e, 4, 0), cursor(&e)), (Some((Some(0), Some(0))), Some((Some(2), Some(0))), (5, 0)));
        assert_eq!(e.term.grid()[Line(-1)][Column(0)].c, '$', "the line before it gone up");
        // One wider than the room right of the cursor shrinks to fit it,
        // and the line below it scrolls it up.
        feed(&mut e, &state, &replies, format!("{}{}", " ".repeat(16), red_sixel(80, 12)).as_bytes());
        assert_eq!((picture_at(&e, 4, 16), picture_at(&e, 4, 19), cursor(&e)), (Some((Some(0), Some(0))), Some((Some(0), Some(3))), (5, 0)));
    }

    #[test]
    fn an_iterm2_png_or_jpeg_is_placed_at_the_cursor_and_leaves_it_after_its_last_row() {
        let (mut e, state, replies) = bare(20, 6);
        let file = |args: &str, bytes: &[u8]| format!("\x1b]1337;File={args}:{}\x07", String::from_utf8(iterm::tests::b64(bytes)).unwrap());
        feed(&mut e, &state, &replies, format!("ab{}", file("inline=1", &iterm::tests::png(30, 40, [0, 255, 0, 255]))).as_bytes());
        // 30 × 40 pixels: three columns and two rows, the cursor after the
        // last row's last cell, as iTerm2 leaves it.
        assert_eq!((picture_at(&e, 0, 2), picture_at(&e, 1, 4), picture_at(&e, 0, 5)), (Some((Some(0), Some(0))), Some((Some(1), Some(2))), None));
        assert_eq!(cursor(&e), (1, 5));
        let held = |replies: &Replies| replies.images.lock().held().1.len();
        let mark = graphics::read_placeholder(&e.term.grid()[Line(0)][Column(2)]).unwrap();
        let (image, placement) = replies.images.lock().lookup(mark.id, mark.placement).unwrap();
        assert_eq!((image.width, image.height, placement.natural, &image.pixels[..4]), (30, 40, true, &[0, 255, 0, 255][..]));
        // A JPEG asked to be six cells wide: 60 × 30 pixels, its shape kept,
        // so two rows.
        feed(&mut e, &state, &replies, file("inline=1;width=6", &iterm::tests::jpeg(16, 8, [255, 255, 255])).as_bytes());
        assert_eq!((picture_at(&e, 1, 5), picture_at(&e, 2, 10), picture_at(&e, 1, 11)), (Some((Some(0), Some(0))), Some((Some(1), Some(5))), None));
        assert_eq!((cursor(&e), held(&replies)), ((2, 11), 2));
        // Asked to leave the cursor, it does.
        feed(&mut e, &state, &replies, file("inline=1;doNotMoveCursor=1", &iterm::tests::png(10, 20, [0; 4])).as_bytes());
        assert_eq!((picture_at(&e, 2, 11), cursor(&e)), (Some((Some(0), Some(0))), (2, 11)));
        // A GIF that won't decode isn't shown, and a download isn't either: nothing kept,
        // nothing written, the cursor where it was.
        feed(&mut e, &state, &replies, file("inline=1", b"GIF89a\x01\x00\x01\x00").as_bytes());
        feed(&mut e, &state, &replies, file("name=YS5wbmc=", &iterm::tests::png(10, 20, [0; 4])).as_bytes());
        assert_eq!((held(&replies), cursor(&e), e.term.grid()[Line(2)][Column(12)].c), (3, (2, 11), ' '));
    }

    #[test]
    fn in_sixel_display_mode_a_sixel_goes_at_the_top_left_and_nothing_moves() {
        let heard = Arc::new(Mutex::new(Vec::new()));
        let (mut e, state, replies) = answering(&heard);
        // Cells of 16 × 38 on 20 × 6: the cursor on the bottom row.
        feed(&mut e, &state, &replies, b"top\r\n\r\n\r\n\r\n\r\n$ ");
        feed(&mut e, &state, &replies, format!("\x1b[?80h{}", red_sixel(40, 120)).as_bytes());
        // 40 × 120 pixels: three columns and four rows from the top left,
        // over the text there, the cursor where it was, nothing scrolled.
        assert_eq!((picture_at(&e, 0, 0), picture_at(&e, 3, 2), picture_at(&e, 0, 3), picture_at(&e, 4, 0)), (Some((Some(0), Some(0))), Some((Some(3), Some(2))), None, None));
        assert_eq!((cursor(&e), e.term.grid().history_size()), ((5, 2), 0));
        // As wide as the screen, not the room right of the cursor; taller
        // than the screen, it's cut at its foot.
        feed(&mut e, &state, &replies, format!("\x1b[1;19H{}", red_sixel(400, 300)).as_bytes());
        assert_eq!((picture_at(&e, 5, 19), cursor(&e), e.term.grid().history_size()), (Some((Some(5), Some(19))), (0, 18), 0));
        // DECRQM says so, and reset it scrolls again, the cursor below it.
        feed(&mut e, &state, &replies, b"\x1b[?80$p\x1b[?8452$p\x1b[?80l\x1b[?80$p");
        assert_eq!(String::from_utf8(heard.lock().clone()).unwrap(), "\x1b[?80;1$y\x1b[?8452;2$y\x1b[?80;2$y", "alacritty's own 'not known' kept back");
        feed(&mut e, &state, &replies, format!("\x1b[6;1H{}", red_sixel(16, 36)).as_bytes());
        assert_eq!((picture_at(&e, 4, 0), cursor(&e), e.term.grid().history_size()), (Some((Some(0), Some(0))), (5, 0), 1));
    }

    #[test]
    fn with_sixel_scrolls_right_the_cursor_goes_right_of_a_sixel_until_a_full_reset() {
        let heard = Arc::new(Mutex::new(Vec::new()));
        let (mut e, state, replies) = answering(&heard);
        feed(&mut e, &state, &replies, format!("ab\x1b[?8452h{}", red_sixel(40, 60)).as_bytes());
        // Three columns and two rows: the cursor after the last row's last cell.
        assert_eq!((picture_at(&e, 1, 4), cursor(&e)), (Some((Some(1), Some(2))), (1, 5)));
        feed(&mut e, &state, &replies, b"\x1b[?8452$p\x1bc\x1b[?8452$p\x1b[?80$p");
        assert_eq!(String::from_utf8(heard.lock().clone()).unwrap(), "\x1b[?8452;1$y\x1b[?8452;2$y\x1b[?80;2$y");
        feed(&mut e, &state, &replies, red_sixel(16, 36).as_bytes());
        assert_eq!(cursor(&e), (1, 0), "below it again, at the left");
    }

    #[test]
    fn through_conpty_an_iterm2_picture_rises_from_the_cursors_row_so_the_next_line_comes_below_it() {
        let (mut e, state, replies) = bare(20, 6);
        e.conpty = true;
        let file = |bytes: &[u8]| format!("\x1b]1337;File=inline=1:{}\x07", String::from_utf8(iterm::tests::b64(bytes)).unwrap());
        let whole = |e: &Emulator, lines: std::ops::Range<i32>| lines.flat_map(|l| (2..5).map(move |c| (l, c))).all(|(l, c)| picture_at(e, l, c).is_some());
        let text = |e: &Emulator, line: i32| (0..e.term.columns()).map(|c| e.term.grid()[Line(line)][Column(c)].c).collect::<String>().trim_end().to_string();
        // A command, and its output from the third row.
        feed(&mut e, &state, &replies, b"top\r\n$ cat pic\r\nab");
        feed(&mut e, &state, &replies, file(&iterm::tests::png(30, 60, [0, 255, 0, 255])).as_bytes());
        // 30 × 60 pixels: three columns and three rows, the last on the
        // cursor's row, the lines above two rows up and the first two in the
        // scrollback, the cursor right of the last row.
        assert_eq!((text(&e, -2), text(&e, -1), &text(&e, 0)[..2]), ("top".into(), "$ cat pic".into(), "ab"));
        assert_eq!((picture_at(&e, 0, 2), picture_at(&e, 2, 4), cursor(&e)), (Some((Some(0), Some(0))), Some((Some(2), Some(2))), (2, 5)));
        // ConPTY has its cursor on that row, and moves to the next for the
        // prompt: below the picture, which stays whole.
        feed(&mut e, &state, &replies, b"\x1b[4;1H$ ");
        assert_eq!((whole(&e, 0..3), text(&e, 3), cursor(&e)), (true, "$".into(), (3, 2)));
        // Too few rows above for a taller one: its top is in the scrollback.
        feed(&mut e, &state, &replies, file(&iterm::tests::png(30, 100, [0, 0, 255, 255])).as_bytes());
        assert_eq!((picture_at(&e, -1, 2), picture_at(&e, 3, 2), cursor(&e)), (Some((Some(0), Some(0))), Some((Some(4), Some(0))), (3, 5)));
        // On the alternate screen, where a program lays its pictures out
        // itself, one goes down from the cursor as anywhere else.
        feed(&mut e, &state, &replies, format!("\x1b[?1049h\x1b[2;3H{}", file(&iterm::tests::png(30, 60, [0; 4]))).as_bytes());
        assert_eq!((picture_at(&e, 1, 2), picture_at(&e, 3, 4), cursor(&e)), (Some((Some(0), Some(0))), Some((Some(2), Some(2))), (3, 5)));
    }

    #[test]
    fn an_iterm2_file_in_parts_is_shown_once_its_last_part_is_in() {
        let (mut e, state, replies) = bare(20, 6);
        let base64 = String::from_utf8(iterm::tests::b64(&iterm::tests::png(30, 40, [0, 255, 0, 255]))).unwrap();
        let (a, b) = base64.split_at(7);
        let parts = format!("ab\x1b]1337;MultipartFile=inline=1;size=99\x07\x1b]1337;FilePart={a}\x07\x1b]1337;FilePart={b}\x1b\\");
        feed(&mut e, &state, &replies, parts.as_bytes());
        assert!(picture_at(&e, 0, 2).is_none() && replies.images.lock().held().1.is_empty(), "not until its end");
        feed(&mut e, &state, &replies, b"\x1b]1337;FileEnd\x07");
        // As a `File=` would be: three columns and two rows, the cursor after them.
        assert_eq!((picture_at(&e, 0, 2), picture_at(&e, 1, 4), cursor(&e)), (Some((Some(0), Some(0))), Some((Some(1), Some(2))), (1, 5)));
        assert_eq!(e.text_between(0, 1).trim(), "ab", "none of it reached the screen as text");
        // A download in parts shows nothing.
        feed(&mut e, &state, &replies, format!("\x1b]1337;MultipartFile=size=99\x07\x1b]1337;FilePart={base64}\x07\x1b]1337;FileEnd\x07").as_bytes());
        assert_eq!((replies.images.lock().held().1.len(), cursor(&e)), (1, (1, 5)));
    }

    #[test]
    fn an_animated_gif_as_iterm2_sends_it_is_placed_once_with_its_frames() {
        let (mut e, state, replies) = bare(20, 6);
        let gif = crate::frames::tests::gif(&[(&[0; 8], [0, 0, 4, 2], 5, gif::DisposalMethod::Keep, None), (&[2; 8], [0, 0, 4, 2], 5, gif::DisposalMethod::Keep, None)], gif::Repeat::Infinite);
        let file = format!("\x1b]1337;File=inline=1;width=2:{}\x07", String::from_utf8(iterm::tests::b64(&gif)).unwrap());
        feed(&mut e, &state, &replies, file.as_bytes());
        let mark = graphics::read_placeholder(&e.term.grid()[Line(0)][Column(1)]).unwrap();
        assert_eq!(mark.col, Some(1));
        let (image, _) = replies.images.lock().lookup(mark.id, mark.placement).unwrap();
        let frames = image.frames.as_ref().expect("moving");
        let (frame, next) = image.frame_at(Instant::now()).unwrap();
        assert!(frames.each().any(|f| Arc::ptr_eq(f, frame)) && next.is_some());
        assert_eq!(replies.images.lock().held().1, [(frames.reel(), 4, 2)], "one texture for all its frames");
    }

    #[test]
    fn a_kitty_animation_sent_frame_by_frame_plays_in_its_cells_from_the_terminal_and_down_the_pipe() {
        use crate::frames::State;
        let pixels = |color: [u8; 4]| graphics::tests::b64(&color.repeat(4));
        // Red, then a blue frame, the red given a gap, and played round.
        let sent = |id: u32, placed: &str| {
            let (red, blue) = (pixels([255, 0, 0, 255]), pixels([0, 0, 255, 255]));
            format!("\x1b_Ga=T,{placed}f=32,s=2,v=2,i={id},q=1;{red}\x1b\\\x1b_Ga=f,i={id};{blue}\x1b\\\x1b_Ga=a,i={id},r=1,z=500\x1b\\\x1b_Ga=a,i={id},s=3\x1b\\")
        };
        let heard = Arc::new(Mutex::new(Vec::new()));
        let (mut e, state, replies) = answering(&heard);
        feed(&mut e, &state, &replies, sent(5, "").as_bytes());
        let mark = graphics::read_placeholder(&e.term.grid()[Line(0)][Column(0)]).unwrap();
        let (image, _) = replies.images.lock().lookup(mark.id, mark.placement).unwrap();
        let frames = image.frames.as_ref().expect("moving");
        assert_eq!((mark.id, frames.len(), frames.state()), (5, 2, State::Running));
        assert!(image.frame_at(Instant::now()).unwrap().1.is_some(), "a frame is due");
        // The frame is answered with its number; the picture asked for
        // quiet, and playing is answered only when it goes wrong.
        assert_eq!(String::from_utf8(heard.lock().clone()).unwrap(), "\x1b_Gi=5,r=2;OK\x1b\\");
        // Down the pipe the same, a virtual placement's cells the program's,
        // and the answers come back up it.
        let link = pipe_link(&Arc::default());
        let composed = sent(6, "U=1,") + "\x1b_Ga=c,i=6,r=2,c=1,w=1,h=1\x1b\\";
        assert_eq!(piped(&link, composed.as_bytes()), "\x1b_Gi=6,r=2;OK\x1b\\\x1b_Gi=6;OK\x1b\\");
        let (image, _) = link.images.lock().lookup(6, 0).unwrap();
        let frames = image.frames.as_ref().expect("moving");
        assert_eq!((frames.len(), frames.state(), &frames.frame(0).unwrap().pixels[..8]), (2, State::Running, &[0, 0, 255, 255, 255, 0, 0, 255][..]));
    }

    #[test]
    fn pictures_a_program_covers_or_leaves_behind_are_let_go_but_those_still_shown_stay() {
        let (mut e, state, replies) = bare(20, 6);
        let held = |replies: &Replies| replies.images.lock().held().1.len();
        // A sixel in the scrollback and one on the main screen.
        feed(&mut e, &state, &replies, format!("{}{}\r\n\r\n\r\n\r\n\r\n{}", red_sixel(10, 18), "x\r\n".repeat(6), red_sixel(10, 18)).as_bytes());
        assert_eq!(held(&replies), 2);
        // A game on the alternate screen draws a sixel over the last, frame
        // after frame, at the same place: only the one shown is kept.
        feed(&mut e, &state, &replies, b"\x1b[?1049h");
        for _ in 0..5 {
            feed(&mut e, &state, &replies, format!("\x1b[H{}", red_sixel(30, 36)).as_bytes());
        }
        assert_eq!(held(&replies), 3);
        // Erased, its last one goes; and those it shows as it leaves too.
        feed(&mut e, &state, &replies, format!("\x1b[2J\x1b[H{}", red_sixel(30, 36)).as_bytes());
        feed(&mut e, &state, &replies, b"\x1b[2J");
        assert_eq!(held(&replies), 2);
        feed(&mut e, &state, &replies, format!("\x1b[H{}\x1b[?1049l", red_sixel(30, 36)).as_bytes());
        assert_eq!(held(&replies), 2, "the main screen's, in its scrollback too, stay");
        // `clear` takes the screen and the scrollback away, and them with it.
        feed(&mut e, &state, &replies, b"\x1b[H\x1b[2J\x1b[3J");
        assert_eq!(held(&replies), 0);
        // A full reset, the same, but Flow's own logo and kitty's stay.
        feed(&mut e, &state, &replies, format!("{}\x1b_Ga=T,f=24,s=1,v=1,i=7,q=2;AAAA\x1b\\", red_sixel(10, 18)).as_bytes());
        replies.images.lock().show_own(Image::new(1, 1, vec![0; 4]), 1, 1);
        feed(&mut e, &state, &replies, b"\x1bc");
        assert_eq!(held(&replies), 2);
    }

    /// An emulator `cols` by `rows` with its state and replies, as the reader
    /// thread has them.
    fn bare(cols: usize, rows: usize) -> (Emulator, Arc<Mutex<ShellState>>, Replies) {
        let writer = ToShell::direct(Box::new(Vec::new()));
        let state = Arc::new(Mutex::new(ShellState::default()));
        let size = Arc::new(Mutex::new(WindowSize { num_lines: rows as u16, num_cols: cols as u16, cell_width: 10, cell_height: 20 }));
        let proxy = EventProxy { ink: tab_ink(), writer: writer.clone(), state: state.clone(), size: size.clone(), waker: Waker::from_fn(|_| {}) };
        let term = Term::new(TermConfig { scrolling_history: 100, ..TermConfig::default() }, &GridSize { cols, rows }, proxy);
        let replies = Replies { writer, size, images: Arc::default(), models: Arc::default() };
        (Emulator::new(term), state, replies)
    }

    #[test]
    fn a_click_on_the_line_being_typed_moves_the_cursor_by_its_letters_and_nowhere_else() {
        let (mut e, state, replies) = bare(20, 6);
        feed(&mut e, &state, &replies, b"earlier output\r\n$ ");
        let start = (e.cursor_abs_line(), e.term.grid().cursor.point.column.0);
        feed(&mut e, &state, &replies, b"echo hello");
        let at = |line: i32, col: usize| Point::new(Line(line), Column(col));
        // "$ echo hello", the cursor after it at 12.
        assert_eq!(e.letters_to(start, at(1, 5)), Some(-7), "onto the o of echo");
        assert_eq!(e.letters_to(start, at(1, 0)), Some(-10), "on the prompt: the input's start");
        assert_eq!(e.letters_to(start, at(1, 18)), Some(0), "past the text: its end, where the cursor is");
        assert_eq!(e.letters_to(start, at(0, 3)), None, "a line above");
        assert_eq!(e.letters_to(start, at(2, 3)), None, "a line below");
        // The cursor moved back, as the shell echoes ←: a click right of it.
        feed(&mut e, &state, &replies, b"\x1b[4D");
        assert_eq!(e.letters_to(start, at(1, 11)), Some(3));
        assert_eq!(e.letters_to(start, at(1, 19)), Some(4), "no further than the text");
        // Wrapped, the line's rows are one: from its second row to its first.
        let (mut e, state, replies) = bare(20, 6);
        feed(&mut e, &state, &replies, b"$ ");
        let start = (e.cursor_abs_line(), 2);
        feed(&mut e, &state, &replies, b"echo abcdefghijklmnopqrstu");
        assert_eq!(e.term.grid().cursor.point, at(1, 8));
        assert_eq!(e.letters_to(start, at(0, 4)), Some(-24));
        assert_eq!(e.letters_to(start, at(1, 2)), Some(-6));
        // A wide letter is one letter, its right half taken for it.
        let (mut e, state, replies) = bare(20, 6);
        feed(&mut e, &state, &replies, b"$ ");
        let start = (e.cursor_abs_line(), 2);
        feed(&mut e, &state, &replies, "a漢b".as_bytes());
        assert_eq!(e.letters_to(start, at(0, 4)), Some(-2), "onto 漢's right half: before 漢");
        assert_eq!(e.letters_to(start, at(0, 5)), Some(-1));
        // A command's second line, which no wrap joins, isn't the line the
        // input starts on.
        feed(&mut e, &state, &replies, b"\r\n> more");
        assert_eq!(e.letters_to(start, at(1, 3)), None);
        assert_eq!(e.letters_to(start, at(0, 3)), None);
    }

    #[test]
    fn a_place_on_the_prompts_line_is_found_again_after_a_rewrap_moves_its_rows() {
        let (mut e, state, replies) = bare(20, 8);
        // A line of 18 letters, which 10 columns wrap, then the prompt.
        feed(&mut e, &state, &replies, b"abcdefghijklmnopqr\r\n$ ");
        let start = (e.cursor_abs_line(), 2);
        feed(&mut e, &state, &replies, b"echo abcdefghijklmn");
        assert_eq!(start, (1, 2));
        let kept = e.place_in_text(start).unwrap();
        let line_above = e.place_in_text((0, 5)).unwrap();
        assert_eq!((kept, line_above), ((0, 2), (1, 5)));
        e.resize(GridSize { cols: 10, rows: 8 });
        // The line above takes two rows now, and the input's start is on the
        // third, where the cursor's line begins.
        assert_eq!(e.place_from_text(kept), Some((2, 2)));
        assert_eq!(e.place_from_text(line_above), Some((0, 5)));
        let found = e.place_from_text(kept).unwrap();
        let cursor_row = e.term.grid().cursor.point.line;
        let b = Point::new(cursor_row, Column(3));
        assert!(e.letters_to(found, b).is_some(), "a click on the input's line after it");
        assert_eq!(e.place_in_text((e.cursor_abs_line() + 1, 0)), None, "below the cursor");
    }

    /// The size of absolute line `abs` (counted from the top of the scrollback).
    fn size_of(e: &Emulator, abs: i64) -> LineSize {
        let grid = e.term.grid();
        line_size::of_row(&grid[Line((abs - grid.history_size() as i64) as i32)])
    }

    #[test]
    fn clearing_the_line_leaves_a_command_beginning_with_a_bracket_whole() {
        use alacritty_terminal::vte::{Parser, Perform};
        /// What a terminal's input parser makes of the bytes: the letters
        /// typed, and the keys' sequences it read.
        #[derive(Default)]
        struct Read {
            letters: String,
            keys: Vec<char>,
        }
        impl Perform for Read {
            fn print(&mut self, c: char) {
                self.letters.push(c);
            }
            fn csi_dispatch(&mut self, _: &alacritty_terminal::vte::Params, _: &[u8], _: bool, action: char) {
                self.keys.push(action);
            }
        }
        let shells = [
            (ShellKind::Cmd, BashFlavor::Native),
            (ShellKind::PowerShell, BashFlavor::Native),
            (ShellKind::Bash, BashFlavor::Native),
            (ShellKind::Bash, BashFlavor::GitBash),
            (ShellKind::Bash, BashFlavor::BusyBox),
            (ShellKind::Zsh, BashFlavor::Native),
        ];
        for (kind, flavor) in shells {
            for command in ["[Console]::WriteLine('ok')", "[x] & echo done", "ls"] {
                let mut read = Read::default();
                Parser::new().advance(&mut read, &[clear_line_keys(kind, flavor), command.as_bytes()].concat());
                assert_eq!(read.letters, command, "{kind:?} {flavor:?}: the command arrives whole");
            }
        }
        // End, then Ctrl+Home, for the two whose Windows line editors take them.
        let mut read = Read::default();
        Parser::new().advance(&mut read, clear_line_keys(ShellKind::PowerShell, BashFlavor::Native));
        assert_eq!(read.keys, ['F', 'H']);
        // BusyBox's line editor draws its prompt again for Ctrl+U, so it's
        // cleared with Ctrl+A and Ctrl+K, which don't; bash's keeps Ctrl+U.
        assert_eq!(clear_line_keys(ShellKind::Bash, BashFlavor::BusyBox), b"\x01\x0b");
        assert_eq!(clear_line_keys(ShellKind::Bash, BashFlavor::GitBash), b"\x05\x15");
    }

    #[test]
    fn a_drag_changes_the_selection_only_as_it_reaches_another_cell_or_side() {
        use alacritty_terminal::index::Side;
        use alacritty_terminal::selection::{Selection, SelectionType};
        let (mut e, _, _) = bare(20, 6);
        let at = |line, col| Point::new(Line(line), Column(col));
        // With nothing selected, a drag has nothing to move.
        assert!(!e.drag_selection(at(0, 3), Side::Left));
        e.term.selection = Some(Selection::new(SelectionType::Simple, at(0, 0), Side::Left));
        assert!(e.drag_selection(at(0, 3), Side::Left), "another cell");
        assert!(!e.drag_selection(at(0, 3), Side::Left), "the same place, as the pointer moves within the half");
        assert!(e.drag_selection(at(0, 3), Side::Right), "the cell's other half");
        assert!(e.drag_selection(at(2, 3), Side::Right), "another line");
    }

    #[test]
    fn double_lines_keep_their_size_into_the_scrollback_and_copy_as_their_text() {
        let (mut e, state, replies) = bare(20, 6);
        feed(&mut e, &state, &replies, b"\x1b#3Big\r\n\x1b#4Big\r\n\x1b#6Wide\r\nplain\r\n");
        assert_eq!((0..4).map(|l| size_of(&e, l)).collect::<Vec<_>>(), [LineSize::Top, LineSize::Bottom, LineSize::Wide, LineSize::Single]);
        // Scrolled up into the scrollback, they keep it; new lines are single.
        feed(&mut e, &state, &replies, "more\r\n".repeat(8).as_bytes());
        assert!(e.term.grid().history_size() >= 4);
        assert_eq!((0..5).map(|l| size_of(&e, l)).collect::<Vec<_>>(), [LineSize::Top, LineSize::Bottom, LineSize::Wide, LineSize::Single, LineSize::Single]);
        // The marks are in none of their text: the assistant's, or a copy.
        assert_eq!(e.text_between(0, 4), "Big\nBig\nWide\nplain\n");
        use alacritty_terminal::index::Side;
        use alacritty_terminal::selection::{Selection, SelectionType};
        let top = -(e.term.grid().history_size() as i32);
        let mut selection = Selection::new(SelectionType::Simple, Point::new(Line(top), Column(0)), Side::Left);
        selection.update(Point::new(Line(top + 3), Column(19)), Side::Right);
        e.term.selection = Some(selection);
        assert_eq!(e.selection_to_string().as_deref(), Some("Big\nBig\nWide\nplain"), "no blanks out to the mark");
        // Back to single, and a clear leaves the screen single (its lines
        // go up into the scrollback, as they are).
        feed(&mut e, &state, &replies, b"\x1b#6\x1b#5x");
        assert_eq!(size_of(&e, e.cursor_abs_line()), LineSize::Single);
        feed(&mut e, &state, &replies, b"\x1b[3;1H\x1b#3\x1b[2J");
        assert!((0..6).all(|r| line_size::on_screen(e.term.grid(), r) == LineSize::Single));
        assert_eq!(size_of(&e, 0), LineSize::Top);
    }

    #[test]
    fn an_erase_within_a_double_line_keeps_its_size_and_one_from_its_start_down_doesnt() {
        let (mut e, state, replies) = bare(20, 6);
        let line = |e: &Emulator| size_of(e, e.cursor_abs_line());
        feed(&mut e, &state, &replies, b"\x1b#6Title\x1b[K");
        assert_eq!(line(&e), LineSize::Wide, "erased to its end");
        feed(&mut e, &state, &replies, b"\x1b[2K\x1b[1K\x1b[0J");
        assert_eq!(line(&e), LineSize::Wide, "erased whole, then from the cursor down past text");
        feed(&mut e, &state, &replies, b"\r\x1b[J");
        assert_eq!(line(&e), LineSize::Single, "erased from its start down");
        // A cursor past a double line's half goes to its right margin.
        feed(&mut e, &state, &replies, b"\r\n0123456789abcde\x1b#3");
        assert_eq!((line(&e), e.term.grid().cursor.point.column.0), (LineSize::Top, 9));
    }

    #[test]
    fn a_resize_keeps_the_double_lines_and_adds_none() {
        let (mut e, state, replies) = bare(20, 6);
        feed(&mut e, &state, &replies, b"\x1b#3Big\r\n\x1b#4Big\r\n$ ");
        e.resize(GridSize { cols: 12, rows: 6 });
        assert_eq!((size_of(&e, 0), size_of(&e, 1), size_of(&e, 2)), (LineSize::Top, LineSize::Bottom, LineSize::Single));
        assert_eq!((e.term.grid().history_size(), e.text_between(0, 3)), (0, "Big\nBig\n$\n".into()), "no line of its own for a mark");
        e.resize(GridSize { cols: 30, rows: 4 });
        assert_eq!((size_of(&e, 0), size_of(&e, 1)), (LineSize::Top, LineSize::Bottom));
    }

    #[test]
    fn a_page_typed_before_conpty_has_its_size_keeps_every_row_through_the_windows_launch() {
        // A start screen's white page (the Flow notanAmiga's), typed in
        // before ConPTY's first output, with the window sized to the screen
        // and back as Windows first shows it, just after the clear.
        let white = "\x1b[0m\x1b[48;2;255;255;255m";
        // Each row as W where it's white, the cursor, and the cursor's line.
        let page = |conpty_repaints: Option<bool>| {
            let (mut e, state, replies) = bare(40, 12);
            feed(&mut e, &state, &replies, format!("{white}\x1b[2J\x1b[H").as_bytes());
            if conpty_repaints.is_some() {
                e.resize_for(GridSize { cols: 60, rows: 20 }, conpty_repaints);
            }
            let rest = format!("{}{white}{}Press any key", format!("{white}\r\n").repeat(8), " ".repeat(13));
            feed(&mut e, &state, &replies, rest.as_bytes());
            if conpty_repaints.is_some() {
                e.resize_for(GridSize { cols: 40, rows: 12 }, conpty_repaints);
            }
            let grid = e.term.grid();
            let white = |c: &Cell| matches!(c.bg, alacritty_terminal::vte::ansi::Color::Spec(Rgb { r: 255, g: 255, b: 255 }));
            let rows: Vec<String> = (0..12).map(|l| (0..40).map(|c| if white(&grid[Line(l)][Column(c)]) { 'W' } else { '.' }).collect()).collect();
            let at = e.cursor_abs_line();
            (rows, grid.cursor.point, e.text_between(at, at + 1))
        };
        let typed = page(None);
        assert!(typed.0.iter().all(|r| *r == "W".repeat(40)) && typed.2.trim() == "Press any key", "{typed:#?}");
        assert_eq!(page(Some(false)), typed);
        // ConPTY's way drops what's below the cursor for a repaint, which
        // doesn't come before ConPTY has its size: the page went back to the
        // screen's own color there, and a white row came in at its foot.
        assert_ne!(page(Some(true)).0, typed.0);
    }

    /// The cursor's line, column and whether a wrap is due.
    fn cursor_of(e: &Emulator) -> (i32, usize, bool) {
        let cursor = &e.term.grid().cursor;
        (cursor.point.line.0, cursor.point.column.0, cursor.input_needs_wrap)
    }

    #[test]
    fn text_on_a_double_line_wraps_at_its_half_or_without_autowrap_stays_on_its_last_column() {
        // Twenty columns: a double line has ten.
        let (mut e, state, replies) = bare(20, 6);
        feed(&mut e, &state, &replies, b"\x1b#6abcdefghijkl");
        assert_eq!(e.text_between(0, 2), "abcdefghijkl\n", "one line, wrapped");
        assert_eq!((size_of(&e, 0), size_of(&e, 1), cursor_of(&e)), (LineSize::Wide, LineSize::Single, (1, 2, false)));
        // Ten fill it: the cursor stays on its last column with a wrap due,
        // so a line break then leaves no empty line, and a combining mark
        // goes on the last letter.
        feed(&mut e, &state, &replies, b"\r\n\x1b#30123456789\xcc\x81");
        assert_eq!(cursor_of(&e), (2, 9, true));
        assert_eq!(e.text_between(2, 3), "0123456789\u{301}\n");
        feed(&mut e, &state, &replies, b"\r\nx");
        assert_eq!(cursor_of(&e), (3, 1, false));
        // Without autowrap, its last column is written over.
        feed(&mut e, &state, &replies, b"\r\x1b#4\x1b[?7labcdefghijkl\x1b[?7h");
        assert_eq!((e.text_between(3, 4), cursor_of(&e)), ("abcdefghil\n".into(), (3, 9, true)));
        // A wide letter with no room for its second half goes on to the next
        // line, leaving padding that reads as nothing, and nowhere without
        // autowrap.
        feed(&mut e, &state, &replies, b"\r\n\x1b#6123456789\xe4\xb8\xad");
        assert_eq!((e.text_between(4, 6), cursor_of(&e)), ("123456789\u{4e2d}\n".into(), (5, 2, false)));
        assert!(e.term.grid()[Line(4)][Column(9)].flags.contains(Flags::LEADING_WIDE_CHAR_SPACER));
        feed(&mut e, &state, &replies, b"\x1b[5;10H\x1b[?7l\xe4\xb8\xad\x1b[?7h");
        assert_eq!(e.text_between(4, 6), "123456789\u{4e2d}\n");
        // None of it reached a line's mark.
        assert_eq!((0..5).map(|l| size_of(&e, l)).collect::<Vec<_>>(), [LineSize::Wide, LineSize::Single, LineSize::Top, LineSize::Bottom, LineSize::Wide]);
    }

    #[test]
    fn the_cursor_stops_at_a_double_lines_half_however_it_moves() {
        let (mut e, state, replies) = bare(20, 6);
        let col = |e: &mut Emulator, bytes: &[u8]| {
            feed(e, &state, &replies, bytes);
            e.term.grid().cursor.point.column.0
        };
        feed(&mut e, &state, &replies, b"\x1b#6");
        // CUF, CUP, CHA, HPA, HPR, HT and CHT.
        let moves: [&[u8]; 7] = [b"\x1b[30C", b"\x1b[1;40H", b"\x1b[50G", b"\x1b[15`", b"\x1b[20a", b"\t\t\t", b"\x1b[3I"];
        assert!(moves.iter().all(|m| col(&mut e, &[b"\r".as_slice(), m].concat()) == 9));
        assert_eq!(col(&mut e, b"\r\t"), 8, "a tab stop within it");
        // Onto it from past its half on a single line, up or down: its last
        // column; the single line keeps its width.
        assert_eq!(col(&mut e, b"\x1b[2;15H"), 14);
        assert_eq!((col(&mut e, b"\x1b[A"), e.term.grid().cursor.point.line.0), (9, 0));
        feed(&mut e, &state, &replies, b"\x1b[3;1H\x1b#3\x1b[2;15H");
        assert_eq!((col(&mut e, b"\n"), e.term.grid().cursor.point.line.0), (9, 2));
        assert_eq!(col(&mut e, b"\x1b[2;15H\x1b7\x1b[H\x1b8\x1b[B"), 9, "and restored there, then moved down");
        // A narrower grid narrows a double line too.
        e.resize(GridSize { cols: 12, rows: 6 });
        assert_eq!(cursor_of(&e), (2, 5, false));
    }

    #[test]
    fn inserting_deleting_and_erasing_characters_on_a_double_line_keep_within_its_half_and_its_mark() {
        let (mut e, state, replies) = bare(20, 4);
        let line = |e: &Emulator| (e.text_between(0, 1), size_of(e, 0));
        let wide = |text: &str| (format!("{text}\n"), LineSize::Wide);
        // ICH: the last two letters are pushed off its tenth column, not the mark off the screen's.
        feed(&mut e, &state, &replies, b"\x1b#6abcdefghij\x1b[1;3H\x1b[2@");
        assert_eq!(line(&e), wide("ab  cdefgh"));
        // DCH: blanks come in at its tenth column.
        feed(&mut e, &state, &replies, b"\x1b[3P");
        assert_eq!(line(&e), wide("abdefgh"));
        assert!((7..19).all(|c| e.term.grid()[Line(0)][Column(c)].c == ' '));
        // ECH: as far as its last column.
        feed(&mut e, &state, &replies, b"\x1b[1;6H\x1b[99X");
        assert_eq!(line(&e), wide("abdef"));
        // Insert mode pushes letters along within it too.
        feed(&mut e, &state, &replies, b"\x1b[1;1H\x1b[4hXY\x1b[4l");
        assert_eq!(line(&e), wide("XYabdef"));
        feed(&mut e, &state, &replies, b"\x1b[1;1H\x1b[4h12345\x1b[4l");
        assert_eq!(line(&e), wide("12345XYabd"));
        // A repeat wraps at its half.
        feed(&mut e, &state, &replies, b"\x1b[1;8HZ\x1b[5b");
        assert_eq!((e.text_between(0, 2), size_of(&e, 0)), ("12345XYZZZZZZ\n".into(), LineSize::Wide));
        assert_eq!(e.text_between(1, 2), "ZZZ\n");
    }

    #[test]
    fn a_resize_under_the_alternate_screen_keeps_the_main_screens_double_lines() {
        let (mut e, state, replies) = bare(20, 6);
        feed(&mut e, &state, &replies, b"\x1b#3Big\r\n\x1b#4Big\r\n$ \x1b[?1049h\x1b#6game");
        e.resize(GridSize { cols: 12, rows: 6 });
        assert_eq!(size_of(&e, 2), LineSize::Wide, "the game's own, on the line it started at");
        feed(&mut e, &state, &replies, b"\x1b[?1049l");
        assert_eq!((size_of(&e, 0), size_of(&e, 1), size_of(&e, 2)), (LineSize::Top, LineSize::Bottom, LineSize::Single));
        assert_eq!((e.term.grid().history_size(), e.text_between(0, 3)), (0, "Big\nBig\n$\n".into()));
        // Widened under it, the marks go on the new last cells.
        feed(&mut e, &state, &replies, b"\x1b[?1049h");
        e.resize(GridSize { cols: 30, rows: 6 });
        feed(&mut e, &state, &replies, b"\x1b[?1049l");
        assert_eq!((size_of(&e, 0), size_of(&e, 1)), (LineSize::Top, LineSize::Bottom));
        assert!(e.term.grid()[Line(1)][Column(29)].underline_color().is_some());
    }

    /// What copying a selection of `ty` from `from` to `to` (screen lines
    /// and columns) gives.
    fn copied(e: &mut Emulator, ty: alacritty_terminal::selection::SelectionType, from: (i32, usize), to: (i32, usize)) -> Option<String> {
        use alacritty_terminal::index::Side;
        use alacritty_terminal::selection::Selection;
        let mut selection = Selection::new(ty, Point::new(Line(from.0), Column(from.1)), Side::Left);
        selection.update(Point::new(Line(to.0), Column(to.1)), Side::Right);
        e.term.selection = Some(selection);
        e.selection_to_string()
    }

    #[test]
    fn a_double_line_that_wrapped_reads_as_one_line_copied_or_typed() {
        use alacritty_terminal::selection::SelectionType::{Lines, Simple};
        // Twenty columns: the double line's ten wrap onto the next line,
        // as a single line of ten does.
        let (mut e, state, replies) = bare(20, 6);
        feed(&mut e, &state, &replies, b"\x1b#6abcdefghijkl\r\nnext");
        let (mut single, state1, replies1) = bare(10, 6);
        feed(&mut single, &state1, &replies1, b"abcdefghijkl\r\nnext");
        assert_eq!(e.text_between(0, 3), "abcdefghijkl\nnext\n");
        assert_eq!(single.text_between(0, 3), e.text_between(0, 3));
        let both = |e: &mut Emulator, single: &mut Emulator, ty: alacritty_terminal::selection::SelectionType, from: (i32, usize), to: (i32, usize)| {
            let last = |e: &Emulator, line: i32| if line_size::of_row(&e.term.grid()[Line(line)]).doubled() { 9 } else { e.term.columns() - 1 };
            let (a, b) = (copied(e, ty, from, (to.0, to.1.min(last(e, to.0)))), copied(single, ty, from, (to.0, to.1.min(last(single, to.0)))));
            assert_eq!(a, b, "{ty:?} from {from:?} to {to:?}");
            a.unwrap_or_default()
        };
        // Copied across the wrap, from within it, to its end, and by line
        // (from the line it wrapped onto, as a triple click there).
        assert_eq!(both(&mut e, &mut single, Simple, (0, 0), (2, 19)), "abcdefghijkl\nnext");
        assert_eq!(both(&mut e, &mut single, Simple, (0, 3), (1, 0)), "defghijk");
        assert_eq!(both(&mut e, &mut single, Simple, (0, 3), (0, 19)), "defghij");
        assert_eq!(both(&mut e, &mut single, Lines, (1, 0), (1, 0)), "abcdefghijkl\n");
        // A double line that doesn't wrap copies as its text and a line
        // break, whatever was past its half before it was made double.
        feed(&mut e, &state, &replies, b"\r\n0123456789ABCDEFGHI\r\x1b#3abc\r\nplain");
        assert_eq!(copied(&mut e, Simple, (3, 0), (4, 19)).as_deref(), Some("abc3456789\nplain"));
        // What's typed at a prompt on a double line, wrapped at its half, is
        // the history's as one line.
        let (mut e, state, replies) = bare(20, 6);
        feed(&mut e, &state, &replies, b"\x1b#6$ ");
        let start = (e.cursor_abs_line(), e.term.grid().cursor.point.column.0);
        feed(&mut e, &state, &replies, b"echo hello world");
        assert_eq!((e.input_from(start), e.text_between(0, 2)), ("echo hello world".into(), "$ echo hello world\n".into()));
        // Deleting a letter brings a blank in at its half, so it no longer
        // runs on, as a single line's end doesn't after a deletion.
        feed(&mut e, &state, &replies, b"\x1b[1;3H\x1b[P");
        assert_eq!(e.text_between(0, 2), "$ cho hel\nlo world\n");
    }

    #[test]
    fn a_line_made_double_loses_its_right_half_so_single_again_it_shows_blanks() {
        let (mut e, state, replies) = bare(20, 6);
        let cells = |e: &Emulator, line: i32| (0..20).map(|c| e.term.grid()[Line(line)][Column(c)].c).collect::<String>();
        // A full line made double: the cursor goes to its half, and what was
        // past it is gone.
        feed(&mut e, &state, &replies, b"0123456789abcdefghij\x1b#6");
        assert_eq!((e.text_between(0, 1), size_of(&e, 0), cursor_of(&e)), ("0123456789\n".into(), LineSize::Wide, (0, 9, false)));
        feed(&mut e, &state, &replies, b"\x1b#5");
        assert_eq!((cells(&e, 0), size_of(&e, 0)), ("0123456789          ".into(), LineSize::Single));
        // A line that wrapped at the screen's edge no longer runs on into the
        // next once it's double; from one double size to another nothing
        // more is lost, and its wrap at the half is kept, until it's single.
        feed(&mut e, &state, &replies, b"\r\nABCDEFGHIJKLMNOPQRSTUV\x1b[2;1H\x1b#3");
        assert_eq!((e.text_between(1, 3), size_of(&e, 1)), ("ABCDEFGHIJ\nUV\n".into(), LineSize::Top));
        feed(&mut e, &state, &replies, b"\x1b[2;10H*wrapped\x1b[2;1H\x1b#6");
        assert_eq!((e.text_between(1, 3), size_of(&e, 1)), ("ABCDEFGHI*wrapped\n".into(), LineSize::Wide));
        feed(&mut e, &state, &replies, b"\x1b#5");
        assert_eq!((e.text_between(1, 3), cells(&e, 1)), ("ABCDEFGHI*\nwrapped\n".into(), "ABCDEFGHI*          ".into()));
        // A wide letter across the half goes with it.
        feed(&mut e, &state, &replies, b"\x1b[5;1H012345678\xe4\xb8\xad\x1b#6");
        assert_eq!(e.text_between(4, 5), "012345678\n");
    }

    /// The text of each line from the top of the scrollback down to the
    /// cursor's, as `text_between` reads it: one that wraps without a line
    /// break.
    fn lines_of(e: &Emulator) -> Vec<String> {
        (0..=e.cursor_abs_line()).map(|l| e.text_between(l, l + 1)).collect()
    }

    // Windows resizes as ConPTY does, keeping the top row rather than the
    // bottom one, which this counts on
    // (`a_wrapped_double_line_rewraps_through_conptys_resize_too`).
    #[cfg(not(windows))]
    #[test]
    fn a_wrapped_double_line_and_what_wrapped_from_it_rewrap_as_one_line() {
        use alacritty_terminal::selection::SelectionType::Simple;
        let (mut e, state, replies) = bare(20, 6);
        // The double line's ten, then the rest wrapped at the screen's edge.
        feed(&mut e, &state, &replies, b"\x1b#6abcdefghijklmnopqrstuvwxyz0123456\r\n$ ");
        let all = "abcdefghijklmnopqrstuvwxyz0123456\n$\n";
        let text = |e: &Emulator| e.text_between(0, e.cursor_abs_line() + 1);
        let before = lines_of(&e);
        assert_eq!((before.clone(), text(&e)), (vec!["abcdefghij".to_string(), "klmnopqrstuvwxyz0123".into(), "456\n".into(), "$\n".into()], all.into()));
        // Narrowed, it shows six letters and the rest go on ahead of what
        // wrapped from it, which takes a line more (the top line going into
        // the scrollback, as the emulator keeps the bottom one); it reads,
        // and copies, as it did, and the cursor is where it was.
        e.resize(GridSize { cols: 12, rows: 6 });
        assert_eq!((lines_of(&e), size_of(&e, 0), e.term.grid().history_size()), (vec!["abcdef".to_string(), "ghijklmnopqr".into(), "stuvwxyz0123".into(), "456\n".into(), "$\n".into()], LineSize::Wide, 1));
        assert_eq!((text(&e), cursor_of(&e)), (all.into(), (3, 2, false)));
        assert_eq!(copied(&mut e, Simple, (-1, 0), (2, 11)).as_deref(), Some("abcdefghijklmnopqrstuvwxyz0123456"));
        // Widened back, it's as it was; further, it takes back as many as it
        // shows, and the line left empty goes.
        e.resize(GridSize { cols: 20, rows: 6 });
        assert_eq!(lines_of(&e), before);
        e.resize(GridSize { cols: 30, rows: 6 });
        assert_eq!((lines_of(&e), size_of(&e, 0)), (vec!["abcdefghijklmno".to_string(), "pqrstuvwxyz0123456\n".into(), "$\n".into()], LineSize::Wide));
        assert_eq!(copied(&mut e, Simple, (0, 0), (1, 29)).as_deref(), Some("abcdefghijklmnopqrstuvwxyz0123456"));
        // Narrowed past what it showed, nothing is cut.
        e.resize(GridSize { cols: 8, rows: 6 });
        assert_eq!(lines_of(&e), ["abcd", "efghijkl", "mnopqrst", "uvwxyz01", "23456\n", "$\n"]);
        e.resize(GridSize { cols: 20, rows: 6 });
        assert_eq!((lines_of(&e), text(&e)), (before, all.into()));
        // The cursor is where the next letter goes.
        feed(&mut e, &state, &replies, b"x");
        assert_eq!(e.text_between(e.cursor_abs_line(), e.cursor_abs_line() + 1), "$ x\n");
    }

    #[test]
    fn a_wrapped_double_line_runs_on_through_a_resize_on_either_screen() {
        let (mut e, state, replies) = bare(20, 6);
        feed(&mut e, &state, &replies, b"\x1b#6abcdefghijkl\r\n$ \x1b[?1049h\x1b[H\x1b#3ABCDEFGHIJKL");
        // The alternate screen is cut, not rewrapped, the cells its double
        // line now shows after its text padding; the main one is rewrapped
        // out of sight, its double line taking back what wrapped from it.
        e.resize(GridSize { cols: 30, rows: 6 });
        assert_eq!((e.text_between(0, 2), size_of(&e, 0)), ("ABCDEFGHIJKL\n".into(), LineSize::Top));
        feed(&mut e, &state, &replies, b"\x1b[?1049l");
        assert_eq!((lines_of(&e), size_of(&e, 0)), (vec!["abcdefghijkl\n".to_string(), "$\n".into()], LineSize::Wide));
        // Narrowed under it, the main screen's double line no longer holds
        // all its text, which goes on a line of its own again, while the
        // alternate screen's is cut.
        feed(&mut e, &state, &replies, b"\x1b[?1049h\x1b[H\x1b#6ABCDEFGHIJKLMNOPQR");
        e.resize(GridSize { cols: 12, rows: 6 });
        assert_eq!(e.text_between(0, 2), "ABCDEFPQR\n");
        feed(&mut e, &state, &replies, b"\x1b[?1049l");
        assert_eq!(lines_of(&e), ["abcdef", "ghijkl\n", "$\n"]);
    }

    #[test]
    fn a_resize_keeps_the_cursor_on_a_double_line_where_its_next_letter_goes() {
        let (mut e, state, replies) = bare(21, 6);
        // Ten letters fill the double line's ten, with a wrap due.
        feed(&mut e, &state, &replies, b"\x1b#60123456789");
        assert_eq!(cursor_of(&e), (0, 9, true));
        // The same ten columns, a column narrower: still due.
        e.resize(GridSize { cols: 20, rows: 6 });
        assert_eq!(cursor_of(&e), (0, 9, true));
        // Widened, the next letter goes in the columns it now has.
        e.resize(GridSize { cols: 30, rows: 6 });
        assert_eq!(cursor_of(&e), (0, 10, false));
        // Narrowed past it, it stops at the new last column, and no line
        // is added for it: nothing wrapped there, so it's a line its
        // program laid out, cut rather than rewrapped.
        e.resize(GridSize { cols: 8, rows: 6 });
        assert_eq!((cursor_of(&e), e.term.grid().history_size()), ((0, 3, false), 0));
        feed(&mut e, &state, &replies, b"X");
        assert_eq!((e.text_between(0, 2), cursor_of(&e)), ("012X\n\n".into(), (0, 3, true)));
        // A single line that wrapped onto a double one still runs on into it
        // once rewrapped, the double line beginning a row of its own.
        let (mut e, state, replies) = bare(20, 6);
        feed(&mut e, &state, &replies, b"0123456789abcdefghijKL\x1b#6");
        assert_eq!(e.text_between(0, 2), "0123456789abcdefghijKL\n");
        e.resize(GridSize { cols: 12, rows: 6 });
        assert_eq!((lines_of(&e), size_of(&e, 2)), (vec!["0123456789ab".to_string(), "cdefghij".into(), "KL\n".into()], LineSize::Wide));
        assert_eq!((e.cursor_abs_line(), e.term.grid().cursor.point.column.0), (2, 2));
        e.resize(GridSize { cols: 20, rows: 6 });
        assert_eq!((lines_of(&e), size_of(&e, 1)), (vec!["0123456789abcdefghij".to_string(), "KL\n".into()], LineSize::Wide));
    }

    // Windows resizes as ConPTY does, keeping nothing below the cursor, which
    // ConPTY paints again, so there what wrapped from the double line goes
    // (`a_wrapped_double_line_rewraps_through_conptys_resize_too`).
    #[cfg(not(windows))]
    #[test]
    fn the_cursor_goes_with_its_letter_on_a_wrapped_double_line_through_a_resize() {
        // On a double line that wrapped, the cursor goes with the letter it's
        // on, onto what wrapped from it once the double line no longer holds
        // it, and back.
        let (mut e, state, replies) = bare(20, 6);
        feed(&mut e, &state, &replies, b"\x1b#6$ echo hello world\x1b[1;9H");
        e.resize(GridSize { cols: 12, rows: 6 });
        assert_eq!((lines_of(&e), cursor_of(&e)), (vec!["$ echo".to_string(), " hello world\n".into()], (1, 2, false)));
        feed(&mut e, &state, &replies, b"E");
        e.resize(GridSize { cols: 30, rows: 6 });
        assert_eq!((lines_of(&e), cursor_of(&e)), (vec!["$ echo hEllo wo".to_string()], (0, 9, false)));
        assert_eq!(e.text_between(0, 2), "$ echo hEllo world\n");
        // At the end of what's typed, it stays after the last letter.
        feed(&mut e, &state, &replies, b"\x1b[2;4H");
        e.resize(GridSize { cols: 11, rows: 6 });
        assert_eq!((lines_of(&e), cursor_of(&e)), (vec!["$ ech".to_string(), "o hEllo wor".into(), "ld\n".into()], (1, 2, false)));
    }

    #[test]
    fn a_command_typed_on_a_wrapped_double_line_reads_the_same_after_a_resize() {
        let (mut e, state, replies) = bare(20, 6);
        feed(&mut e, &state, &replies, b"\x1b#6$ ");
        let start = (e.cursor_abs_line(), e.term.grid().cursor.point.column.0);
        feed(&mut e, &state, &replies, b"echo hello world and more");
        for cols in [12, 30, 8, 21, 20] {
            e.resize(GridSize { cols, rows: 6 });
            assert_eq!((e.input_from(start), size_of(&e, 0)), ("echo hello world and more".into(), LineSize::Wide), "at {cols} columns");
        }
        // And typing goes on after it.
        feed(&mut e, &state, &replies, b"!");
        assert_eq!(e.input_from(start), "echo hello world and more!");
    }

    #[test]
    fn a_double_lines_text_flows_once_it_wraps_at_its_half_until_its_made_again() {
        let (mut e, state, replies) = bare(20, 6);
        let flows = |e: &Emulator| (0..2).map(|l| line_size::flowing(&e.term.grid()[Line(l)])).collect::<Vec<_>>();
        // A title doesn't; text that wraps at the half does, and still does
        // after an erase within it.
        feed(&mut e, &state, &replies, b"\x1b#6Title\r\n\x1b#6abcdefghijkl\x1b[1;3H\x1b[K\x1b[2;3H\x1b[1K");
        assert_eq!((flows(&e), line_size::wraps(&e.term.grid()[Line(1)])), (vec![false, true], true));
        // Made double again, it starts over.
        feed(&mut e, &state, &replies, b"\x1b#3");
        assert_eq!((flows(&e), size_of(&e, 1)), (vec![false, false], LineSize::Top));
    }

    #[test]
    fn pictures_on_a_double_line_keep_within_its_half() {
        use base64::Engine as _;
        let writer = ToShell::direct(Box::new(Vec::new()));
        let state = Arc::new(Mutex::new(ShellState::default()));
        let size = Arc::new(Mutex::new(WindowSize { num_lines: 6, num_cols: 20, cell_width: 10, cell_height: 20 }));
        let proxy = EventProxy { ink: tab_ink(), writer: writer.clone(), state: state.clone(), size: size.clone(), waker: Waker::from_fn(|_| {}) };
        let term = Term::new(TermConfig { scrolling_history: 100, ..TermConfig::default() }, &GridSize { cols: 20, rows: 6 }, proxy);
        let mut e = Emulator::new(term);
        let (images, models) = (Arc::new(Mutex::new(ImageStore::default())), Arc::new(Mutex::new(ModelStore::default())));
        let replies = Replies { writer, size, images: images.clone(), models };
        let pictured = |e: &Emulator, line: i32| (0..e.term.columns()).filter(|&c| graphics::read_placeholder(&e.term.grid()[Line(line)][Column(c)]).is_some()).collect::<Vec<_>>();
        // Eight columns asked for at its sixth: shrunk to the five left in
        // its ten, and the cursor at its last column after them.
        feed(&mut e, &state, &replies, b"\x1b#6\x1b[1;6H\x1b_Ga=T,f=24,s=1,v=1,c=8,r=1,i=1,q=2;AAAA\x1b\\");
        assert_eq!(pictured(&e, 0), [5, 6, 7, 8, 9]);
        assert_eq!((size_of(&e, 0), cursor_of(&e)), (LineSize::Wide, (0, 9, false)));
        // Cut by a narrowing, as `Session::resize` has it, and filled out
        // again as far as its half.
        feed(&mut e, &state, &replies, b"\r");
        let resize = |e: &mut Emulator, cols: usize| {
            let old = e.term.columns();
            graphics::crop_pictures(&mut e.term, cols);
            e.resize(GridSize { cols, rows: 6 });
            graphics::regrow_pictures(&mut e.term, old, &images.lock());
        };
        resize(&mut e, 8);
        assert_eq!(pictured(&e, 0), [5, 6, 7]);
        resize(&mut e, 20);
        assert_eq!((pictured(&e, 0), size_of(&e, 0)), (vec![5, 6, 7, 8, 9], LineSize::Wide));
        // A model over it covers only its ten.
        let square = base64::engine::general_purpose::STANDARD.encode(model::tests::SQUARE);
        feed(&mut e, &state, &replies, format!("\x1b_ratty;g;r;id=5;fmt=obj;source=payload;more=0;{square}\x1b\\").as_bytes());
        feed(&mut e, &state, &replies, b"\x1b_ratty;g;p;id=5;row=1;col=10;w=20;h=3\x1b\\");
        assert_eq!(pictured(&e, 0), (0..10).collect::<Vec<_>>());
        assert_eq!((pictured(&e, 1).len(), size_of(&e, 0)), (20, LineSize::Wide));
    }

    #[test]
    fn a_picture_cut_on_a_wrapped_double_line_grows_back_as_it_widens() {
        let (mut e, state, replies) = bare(20, 6);
        let images = replies.images.clone();
        let pictured = |e: &Emulator| (0..e.term.columns()).filter(|&c| graphics::read_placeholder(&e.term.grid()[Line(0)][Column(c)]).is_some()).collect::<Vec<_>>();
        let resize = |e: &mut Emulator, cols: usize| {
            let old = e.term.columns();
            graphics::crop_pictures(&mut e.term, cols);
            e.resize(GridSize { cols, rows: 6 });
            graphics::regrow_pictures(&mut e.term, old, &images.lock());
        };
        // A picture in the last five of a double line's ten columns, and
        // text wrapped on from it. Its cells read as blanks.
        feed(&mut e, &state, &replies, b"\x1b#6abcde\x1b_Ga=T,f=24,s=1,v=1,c=5,r=1,i=1,q=2;AAAA\x1b\\\r\nxyz");
        e.term.grid_mut()[Line(0)][Column(19)].flags.insert(Flags::WRAPLINE);
        let text = "abcde     xyz\n";
        assert_eq!((pictured(&e), e.text_between(0, 2)), (vec![5, 6, 7, 8, 9], text.into()));
        // Narrowed, it's cut at the double line's new half rather than
        // carried onto what wrapped from it, the blanks in its place going
        // on ahead of that, so the text reads the same.
        resize(&mut e, 12);
        assert_eq!((pictured(&e), lines_of(&e), e.text_between(0, 2)), (vec![5], vec!["abcde ".to_string(), "    xyz\n".into()], text.into()));
        // Widened, it takes the blanks back and the picture grows into them
        // again, as on a single line; wider still, it takes the text too.
        resize(&mut e, 20);
        assert_eq!((pictured(&e), lines_of(&e)), (vec![5, 6, 7, 8, 9], vec!["abcde     ".to_string(), "xyz\n".into()]));
        resize(&mut e, 30);
        assert_eq!((pictured(&e), e.text_between(0, 1), size_of(&e, 0)), (vec![5, 6, 7, 8, 9], text.into(), LineSize::Wide));
    }

    #[test]
    fn a_click_on_a_double_line_lands_in_its_cells_as_drawn() {
        assert_eq!(on_double_line(LineSize::Single, (2, 7), (0.25, 0.5)), ((2, 7), (0.25, 0.5)));
        assert_eq!(on_double_line(LineSize::Wide, (2, 7), (0.5, 0.5)), ((2, 3), (0.75, 0.5)));
        assert_eq!(on_double_line(LineSize::Top, (2, 6), (0.0, 0.5)), ((2, 3), (0.0, 0.25)));
        assert_eq!(on_double_line(LineSize::Bottom, (2, 6), (0.0, 0.5)), ((2, 3), (0.0, 0.75)));
    }

    #[test]
    fn a_start_screen_makes_its_lines_double_as_it_types() {
        let (e, state, replies) = bare(20, 6);
        let emu = Mutex::new(e);
        type_out(&emu, &state, &replies, &Waker::from_fn(|_| {}), typing("\x1b#3\x1b[38;2;255;255;0mHi\x1b[0m\r\n\x1b#4Hi\r\n", 0, None));
        let e = emu.lock();
        assert_eq!((size_of(&e, 0), size_of(&e, 1), size_of(&e, 2)), (LineSize::Top, LineSize::Bottom, LineSize::Single));
        assert_eq!(e.text_between(0, 2), "Hi\nHi\n");
    }

    /// A start screen to type: `text` at `baud`, with `logo`.
    fn typing(text: &str, baud: u32, logo: Option<Image>) -> Typing {
        Typing { text: text.into(), baud, logo, picture: None }
    }

    #[test]
    fn a_start_screens_paper_fills_the_screen_and_its_lines_margins() {
        use alacritty_terminal::vte::ansi::Color;
        // A page of white, as `{paper-white}{clear}` writes it, then a
        // centered line on it.
        let (e, state, replies) = bare(20, 6);
        let emu = Mutex::new(e);
        let white = "\x1b[48;2;255;255;255m";
        let text = format!("\x1b[0m{white}\x1b[2J\x1b[H\x1b[0m{white}\r\n  x\x1b[0m{white}\r\n");
        type_out(&emu, &state, &replies, &Waker::from_fn(|_| {}), typing(&text, 0, None));
        // Every cell is on the paper, the cleared ones, the margin before
        // the line's letter and the letter's own.
        let e = emu.lock();
        let paper = Color::Spec(Rgb { r: 255, g: 255, b: 255 });
        assert!((0..6).all(|line| (0..20).all(|col| e.term.grid()[Line(line)][Column(col)].bg == paper)));
        let top = e.term.grid().history_size() as i64;
        assert_eq!(e.text_between(top + 1, top + 2), "  x\n");
    }

    #[test]
    fn a_start_screens_picture_goes_in_the_middle_as_tall_as_the_screen_has_room() {
        use crate::looks::PICTURE_HERE;
        // On the notanAmiga's 80 × 32, 13 rows of a square picture in
        // cells half as wide as they're tall, under the line before it, the
        // cursor on its last row for the words after it.
        let at = |cols: usize, rows: usize| {
            let (e, state, replies) = bare(cols, rows);
            let emu = Mutex::new(e);
            let text = format!("top\r\n{PICTURE_HERE}\r\nunder\r\n");
            type_out(&emu, &state, &replies, &Waker::from_fn(|_| {}), Typing { text, baud: 0, logo: None, picture: Some(Image::new(130, 131, vec![255; 130 * 131 * 4])) });
            let e = emu.lock();
            let shown: Vec<(i32, usize)> = (0..rows as i32).flat_map(|line| (0..cols).map(move |col| (line, col))).filter(|&(line, col)| graphics::read_placeholder(&e.term.grid()[Line(line)][Column(col)]).is_some()).collect();
            let under = (0..rows as i32).find(|&line| e.text_between(line as i64, line as i64 + 1).starts_with("under"));
            (shown.first().copied(), shown.last().copied(), shown.len(), under)
        };
        // Cells 10 by 20 pixels: 13 rows of 20 are 260 pixels, 258 wide.
        assert_eq!(at(80, 32), (Some((1, 27)), Some((13, 52)), 13 * 26, Some(14)));
        // A shorter screen leaves six rows for the words; a narrower one
        // makes it as tall as its width allows; too small, none.
        assert_eq!(at(80, 16).2, 10 * 20);
        assert_eq!(at(20, 32).2, 10 * 20);
        assert_eq!(at(80, 8), (None, None, 0, Some(2)));
    }

    #[test]
    fn a_start_screen_waits_for_a_key_with_its_cursor_hidden_then_goes_on() {
        use crate::looks::KEY_WAIT;
        let (e, state, replies) = bare(20, 6);
        let emu = Arc::new(Mutex::new(e));
        let text = format!("PRESS ANY KEY\r\n{KEY_WAIT}\x1b[2J\x1b[HREADY\r\n");
        let typing = {
            let (emu, state) = (emu.clone(), state.clone());
            std::thread::spawn(move || type_out(&emu, &state, &replies, &Waker::from_fn(|_| {}), typing(&text, 0, None)))
        };
        let waiting = |want: bool| {
            let until = Instant::now() + Duration::from_secs(5);
            while state.lock().awaiting_key != want {
                assert!(Instant::now() < until, "the start screen never waited");
                std::thread::sleep(Duration::from_millis(5));
            }
        };
        waiting(true);
        // It stops at the wait, the cursor hidden as the machine's was.
        {
            let e = emu.lock();
            assert_eq!(e.text_between(0, 1), "PRESS ANY KEY\n");
            assert!(!e.term.mode().contains(TermMode::SHOW_CURSOR));
        }
        state.lock().booting = false;
        // The key (`Session::write` takes it), and the rest is typed.
        state.lock().awaiting_key = false;
        typing.join().unwrap();
        let e = emu.lock();
        let top = e.term.grid().history_size() as i64;
        assert_eq!(e.text_between(top, top + 1), "READY\n");
        assert!(e.term.mode().contains(TermMode::SHOW_CURSOR));
        // The machine sounds again as it goes on.
        assert!(state.lock().booting);
    }

    #[test]
    fn a_start_screen_waiting_for_a_key_stops_when_its_tab_closes() {
        let (e, state, replies) = bare(20, 6);
        let emu = Arc::new(Mutex::new(e));
        let text = format!("{}never\r\n", crate::looks::KEY_WAIT);
        let typing = {
            let (emu, state) = (emu.clone(), state.clone());
            std::thread::spawn(move || type_out(&emu, &state, &replies, &Waker::from_fn(|_| {}), typing(&text, 0, None)))
        };
        while !state.lock().awaiting_key {
            std::thread::sleep(Duration::from_millis(5));
        }
        state.lock().closing = true;
        typing.join().unwrap();
    }

    #[test]
    fn a_start_screen_puts_its_logo_in_where_it_says_with_its_lines_beside() {
        use crate::looks::{BESIDE_LOGO, LOGO_HERE};
        let text = format!("Probing\r\n{LOGO_HERE}{BESIDE_LOGO}Banner\r\n{BESIDE_LOGO}ROM\r\n\r\n\r\nok boot\r\n");
        let typed = |logo: Option<Image>| {
            let (e, state, replies) = bare(60, 8);
            let emu = Mutex::new(e);
            type_out(&emu, &state, &replies, &Waker::from_fn(|_| {}), typing(&text, 0, logo));
            let e = emu.into_inner();
            let row = |r: i32| (0..60).map(|c| e.term.grid()[Line(r)][Column(c)].c).collect::<Vec<char>>();
            (row(1), row(2), row(5), e.term.grid()[Line(1)][Column(1)].c, e.term.grid()[Line(4)][Column(8)].c)
        };
        // A square logo four rows of 20 pixels tall is eight 10-pixel cells
        // wide, at the second column; its lines start three columns past it.
        let (banner, rom, ok, corner, last) = typed(Some(Image::new(4, 4, vec![255; 64])));
        assert_eq!((corner, last), (graphics::PLACEHOLDER, graphics::PLACEHOLDER));
        let text = |cells: &[char]| cells.iter().collect::<String>();
        assert_eq!((text(&banner[11..17]), text(&rom[11..14]), text(&ok).trim_end()), ("Banner".into(), "ROM".into(), "ok boot"));
        // Without one, the lines start at the left.
        let (banner, rom, ..) = typed(None);
        assert!(text(&banner).starts_with("Banner") && text(&rom).starts_with("ROM"));
    }

    #[test]
    fn a_start_screen_puts_a_centered_logo_in_the_middle_of_its_line() {
        use crate::looks::LOGO_CENTERED;
        // Seen on a screen too narrow for lines beside it, as the cyan home
        // computer's 32 columns are.
        let (e, state, replies) = bare(32, 8);
        let emu = Mutex::new(e);
        let text = format!("Title\r\n{LOGO_CENTERED}\r\n\r\n\r\n\r\nName\r\n");
        type_out(&emu, &state, &replies, &Waker::from_fn(|_| {}), typing(&text, 0, Some(Image::new(4, 4, vec![255; 64]))));
        let e = emu.into_inner();
        let shown = |row: i32| (0..32).filter(|&c| e.term.grid()[Line(row)][Column(c)].c == graphics::PLACEHOLDER).collect::<Vec<usize>>();
        // Eight columns wide, twelve either side, on the four rows under the title.
        assert_eq!(shown(1), (12..20).collect::<Vec<usize>>());
        assert!((2..5).all(|row| shown(row) == shown(1)) && shown(0).is_empty() && shown(5).is_empty());
        let row = |r: i32| (0..32).map(|c| e.term.grid()[Line(r)][Column(c)].c).collect::<String>();
        assert_eq!((row(0).trim_end(), row(5).trim_end()), ("Title", "Name"));
    }

    /// A program at the other end of a tab's pipe: what it sends, and what
    /// comes back to it.
    struct Program {
        sends: std::io::Cursor<Vec<u8>>,
        hears: Arc<Mutex<Vec<u8>>>,
    }

    impl Read for Program {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.sends.read(buf)
        }
    }

    impl Write for Program {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.hears.lock().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn the_pipe_keeps_pictures_and_answers_down_itself() {
        let writer = ToShell::direct(Box::new(Vec::new()));
        let state = Arc::new(Mutex::new(ShellState::default()));
        let size = Arc::new(Mutex::new(WindowSize { num_lines: 6, num_cols: 20, cell_width: 16, cell_height: 38 }));
        let proxy = EventProxy { ink: tab_ink(), writer, state: state.clone(), size: size.clone(), waker: Waker::from_fn(|_| {}) };
        let term = Term::new(TermConfig::default(), &GridSize { cols: 20, rows: 6 }, proxy);
        let emu = Arc::new(Mutex::new(Emulator::new(term)));
        let images = Arc::new(Mutex::new(ImageStore::default()));
        let woken = Arc::new(AtomicBool::new(false));
        let waker = Waker::from_fn({
            let woken = woken.clone();
            move |_| woken.store(true, Ordering::Release)
        });
        let link = Link { ink: tab_ink(), emu: emu.clone(), state: state.clone(), images: images.clone(), models: Arc::default(), size, cell: Arc::new(Mutex::new((16.54, 38.0))), waker };
        // A picture, the questions a program asks first, a picture placed at
        // the cursor (there's none down the pipe), a shell's marker, an
        // effect for the glass, and DA1.
        let sends = b"\x1b_Ga=T,U=1,f=24,s=1,v=1,i=1048577,p=1,c=2,r=1,q=2;AAAA\x1b\\\x1b[14t\x1b[16t\x1b[>0q\
            \x1b_Ga=q,i=31,s=1,v=1,f=24;AAAA\x1b\\\x1b_Ga=T,f=24,s=1,v=1,i=9,q=2;AAAA\x1b\\\x1b]133;A\x07\x1b]7731;fx=burst;at=4,2\x07\x1b[c";
        let hears = Arc::new(Mutex::new(Vec::new()));
        serve_pipe(Box::new(Program { sends: std::io::Cursor::new(sends.to_vec()), hears: hears.clone() }), &link);
        let answers = String::from_utf8(hears.lock().clone()).unwrap();
        // The text area to the pixel: 20 cells of 16.54.
        let expected = format!("\x1b[4;228;331t\x1b[6;38;16t\x1bP>|Flow {}\x1b\\\x1b_Gi=31;OK\x1b\\\x1b[?62;4;22c", crate::version::VERSION);
        assert_eq!(answers, expected);
        assert!(images.lock().lookup(1048577, 0).is_some() && woken.load(Ordering::Acquire));
        // The effect waits for the UI as one from the terminal would; the
        // shell's marker means nothing down the pipe.
        let st = state.lock();
        assert_eq!((st.effects.len(), st.effects[0].request.at, st.integrated), (1, (4, 2), false));
        drop(st);
        // Nothing goes on screen down the pipe.
        assert!(shown_images(&emu.lock().term).is_empty());
        assert!(emu.lock().text_between(0, 6).trim().is_empty());
    }

    /// A tab's pipe as a program finds it, with nothing on screen: an
    /// emulator of 20 by 6 cells of 16.54 by 38 pixels whose own answers go
    /// to `heard`, and what it works on.
    fn pipe_link(heard: &Arc<Mutex<Vec<u8>>>) -> Link {
        let writer = ToShell::direct(Box::new(Program { sends: std::io::Cursor::new(Vec::new()), hears: heard.clone() }));
        let state = Arc::new(Mutex::new(ShellState::default()));
        let size = Arc::new(Mutex::new(WindowSize { num_lines: 6, num_cols: 20, cell_width: 16, cell_height: 38 }));
        let proxy = EventProxy { ink: tab_ink(), writer, state: state.clone(), size: size.clone(), waker: Waker::from_fn(|_| {}) };
        let term = Term::new(TermConfig::default(), &GridSize { cols: 20, rows: 6 }, proxy);
        let emu = Arc::new(Mutex::new(Emulator::new(term)));
        Link { ink: tab_ink(), emu, state, images: Arc::default(), models: Arc::default(), size, cell: Arc::new(Mutex::new((16.54, 38.0))), waker: Waker::from_fn(|_| {}) }
    }

    /// What the pipe answers a program that sends `sends` and hangs up.
    fn piped(link: &Link, sends: &[u8]) -> String {
        let hears = Arc::new(Mutex::new(Vec::new()));
        serve_pipe(Box::new(Program { sends: std::io::Cursor::new(sends.to_vec()), hears: hears.clone() }), link);
        let answers = hears.lock().clone();
        String::from_utf8(answers).unwrap()
    }

    #[test]
    fn a_sixel_down_the_pipe_goes_where_conptys_cursor_is_and_moves_nothing_below() {
        let heard = Arc::new(Mutex::new(Vec::new()));
        let link = pipe_link(&heard);
        link.emu.lock().conpty = true;
        let text = |line: i32| {
            let e = link.emu.lock();
            (0..e.term.columns()).map(|c| e.term.grid()[Line(line)][Column(c)].c).filter(|&c| c != '\u{10EEEE}').collect::<String>().trim_end().to_string()
        };
        let at = |line: i32, col: usize| picture_at(&link.emu.lock(), line, col);
        let cursor_at = || cursor(&link.emu.lock());
        // 32 × 72 pixels: two columns and two rows of 16 × 38.
        let sixel = red_sixel(32, 72);
        // A program's line, then a sixel at the start of the next: in lines
        // of its own above the cursor's, the lines above scrolled up into
        // the scrollback, the cursor where ConPTY has it, and the program's
        // next line there, below the picture.
        advance(&mut link.emu.lock(), b"$ show\r\none\r\n");
        assert_eq!(piped(&link, sixel.as_bytes()), "");
        assert_eq!((at(0, 0), at(1, 1), at(0, 2), cursor_at()), (Some((Some(0), Some(0))), Some((Some(1), Some(1))), None, (2, 0)));
        assert_eq!((text(-2), text(-1)), ("$ show".into(), "one".into()));
        advance(&mut link.emu.lock(), b"two\r\n~\r\n$ ");
        assert_eq!((text(2), text(4), cursor_at()), ("two".into(), "$".into(), (4, 2)));
        // A sixel program's output sent here (`img2sixel a.png > pipe`)
        // comes once the shell has drawn its prompt again: above the
        // prompt's first line, which, with the cursor, stays where it was.
        // (Only with the cursor on the line typing starts on: readline draws
        // a prompt's last line again as Flow clears it to type a command.)
        let history = link.emu.lock().term.grid().history_size() as i64;
        {
            let mut st = link.state.lock();
            (st.at_prompt, st.prompt_line, st.input_start) = (true, Some(history + 3), Some((history + 4, 2)));
        }
        piped(&link, sixel.as_bytes());
        assert_eq!((text(0), at(1, 0), at(2, 1)), ("two".into(), Some((Some(0), Some(0))), Some((Some(1), Some(1)))));
        assert_eq!((text(3), text(4), cursor_at()), ("~".into(), "$".into(), (4, 2)));
        // Mid-line, a command running: from the cursor, rising over the line
        // above as an iTerm2 picture does, the cursor right of its last row.
        link.state.lock().at_prompt = false;
        advance(&mut link.emu.lock(), b"\r\nsee ");
        piped(&link, sixel.as_bytes());
        assert_eq!((text(4), at(4, 4), at(5, 5), cursor_at()), ("see".into(), Some((Some(0), Some(0))), Some((Some(1), Some(1))), (5, 6)));
        // DECSDM sent here too, and asked of: at the screen's top left,
        // nothing moving.
        assert_eq!(piped(&link, format!("\x1b[?80h\x1b[?80$p{sixel}").as_bytes()), "\x1b[?80;1$y");
        assert_eq!((at(0, 0), at(1, 1), cursor_at()), (Some((Some(0), Some(0))), Some((Some(1), Some(1))), (5, 6)));
        // On the alternate screen, at the cursor, which stays.
        assert_eq!(piped(&link, b"\x1b[?80l\x1b[?80$p"), "\x1b[?80;2$y");
        advance(&mut link.emu.lock(), b"\x1b[?1049h\x1b[3;4H");
        piped(&link, sixel.as_bytes());
        assert_eq!((at(2, 3), at(3, 4), cursor_at()), (Some((Some(0), Some(0))), Some((Some(1), Some(1))), (2, 3)));
    }

    #[test]
    fn the_pipe_answers_the_colors_asked_as_the_terminal_does() {
        // The background by BEL and by ST, the text's, two of the palette's,
        // the background and the cursor in one question, and eight of the
        // palette's in one, which the emulator, keeping 16 parameters, leaves
        // unanswered.
        let asked = b"\x1b]11;?\x07\x1b]11;?\x1b\\\x1b]10;?\x07\x1b]4;1;?;250;?\x1b\\\x1b]11;?;?\x07\x1b]4;0;?;1;?;2;?;3;?;4;?;5;?;6;?;7;?\x07";
        let heard = Arc::new(Mutex::new(Vec::new()));
        let link = pipe_link(&heard);
        advance(&mut link.emu.lock(), asked);
        let terminal = String::from_utf8(heard.lock().clone()).unwrap();
        assert_eq!(terminal.matches("\x1b]").count(), 7, "{terminal:?}");
        // Down the pipe the same answers come, in the same order, before
        // DA1's, which comes last.
        assert_eq!(piped(&link, &[&asked[..], b"\x1b[c"].concat()), format!("{terminal}\x1b[?62;4;22c"));
        // Flow's glass, each part's byte written twice.
        assert!(terminal.starts_with("\x1b]11;rgb:0808/0c0c/1616\x07\x1b]11;rgb:0808/0c0c/1616\x1b\\\x1b]10;rgb:e4e4/e9e9/f2f2\x07"), "{terminal:?}");
    }

    #[test]
    fn the_sdk_reads_what_the_pipe_answers_to_its_questions() {
        use flow_sdk::detect::{parse, Query};
        let link = pipe_link(&Arc::default());
        // Every question, as `Query::ask` sends them down the pipe on Windows.
        let answers = piped(&link, &Query::everything().bytes());
        assert!(flow_sdk::pipe::answered_da1(answers.as_bytes()));
        let caps = parse(answers.as_bytes());
        assert!(caps.pictures && caps.models && caps.flow, "{answers:?}");
        assert_eq!((caps.cell, caps.area), (Some((16, 38)), Some((331, 228))));
        let background = crate::render::palette::ink().background;
        assert_eq!(caps.background, Some(background));
        // Flow Mines' question, the background alone, which the SDK asks
        // with the text area's size: on Flow's dark glass it leaves the
        // opened ground clear.
        let caps = parse(piped(&link, &Query { area: true, ..Query::new().background() }.bytes()).as_bytes());
        assert_eq!((caps.background, caps.light_background()), (Some(background), Some(false)));
    }

    /// An emulator 20 by 6 with cells 16 by 38, as [`pipe_link`]'s, whose
    /// answers in the terminal go to `heard`, and its state and replies.
    fn answering(heard: &Arc<Mutex<Vec<u8>>>) -> (Emulator, Arc<Mutex<ShellState>>, Replies) {
        let writer = ToShell::direct(Box::new(Program { sends: std::io::Cursor::new(Vec::new()), hears: heard.clone() }));
        let state = Arc::new(Mutex::new(ShellState::default()));
        let size = Arc::new(Mutex::new(WindowSize { num_lines: 6, num_cols: 20, cell_width: 16, cell_height: 38 }));
        let proxy = EventProxy { ink: tab_ink(), writer: writer.clone(), state: state.clone(), size: size.clone(), waker: Waker::from_fn(|_| {}) };
        let term = Term::new(TermConfig::default(), &GridSize { cols: 20, rows: 6 }, proxy);
        (Emulator::new(term), state, Replies { writer, size, images: Arc::default(), models: Arc::default() })
    }

    #[test]
    fn da1_says_flow_draws_sixels_in_the_terminal_and_down_the_pipe() {
        let heard = Arc::new(Mutex::new(Vec::new()));
        let (mut e, state, replies) = answering(&heard);
        feed(&mut e, &state, &replies, b"\x1b[c\x1b[0c\x1b[>c");
        let terminal = String::from_utf8(heard.lock().clone()).unwrap();
        assert!(terminal.starts_with("\x1b[?62;4;22c\x1b[?62;4;22c\x1b[>0;"), "DA2's is alacritty's still: {terminal:?}");
        assert!(flow_sdk::pipe::answered_da1(terminal.as_bytes()), "the SDK still reads it as DA1");
        assert_eq!(piped(&pipe_link(&Arc::default()), b"\x1b[c"), "\x1b[?62;4;22c");
    }

    #[test]
    fn xtsmgraphics_says_what_sixel_pictures_may_hold_in_the_terminal_and_down_the_pipe() {
        let heard = Arc::new(Mutex::new(Vec::new()));
        let (mut e, state, replies) = answering(&heard);
        feed(&mut e, &state, &replies, b"top\x1b[?1;1S\x1b[?2;1S\x1b[?2;4S\x1b[?1;3;16S\x1b[?2;5S\x1b[?3;1S");
        // The registers, the text area of 20 × 6 cells of 16 × 38, the most
        // a picture may be, the registers again (they can't be set), an
        // action there isn't, and ReGIS, which isn't here.
        let terminal = String::from_utf8(heard.lock().clone()).unwrap();
        assert_eq!(terminal, "\x1b[?1;0;1024S\x1b[?2;0;320;228S\x1b[?2;0;10000;10000S\x1b[?1;0;1024S\x1b[?2;2;0S\x1b[?3;1;0S");
        assert_eq!(e.text_between(0, 1).trim(), "top", "none of them scrolled the screen");
        let huge = WindowSize { num_lines: 500, num_cols: 1000, cell_width: 16, cell_height: 38 };
        assert_eq!(graphics_answer(2, 1, huge), "\x1b[?2;0;10000;10000S", "no larger than a picture may be");
        // Down the pipe the same. A sixel there is shown (as ConPTY drops one
        // written to the terminal), and iTerm2's pictures, which ConPTY
        // passes on to the terminal, are passed over. (The cursor a line
        // down, as the sixel goes in above its line.)
        let link = pipe_link(&Arc::default());
        advance(&mut link.emu.lock(), b"\r\n");
        let pictures = format!("{}\x1b]1337;File=inline=1:{}\x07", red_sixel(6, 6), String::from_utf8(iterm::tests::b64(&iterm::tests::png(2, 2, [0; 4]))).unwrap());
        assert_eq!(piped(&link, &[pictures.as_bytes(), b"\x1b[?2;1S\x1b[c"].concat()), "\x1b[?2;0;320;228S\x1b[?62;4;22c");
        assert_eq!(link.images.lock().held().1.iter().map(|&(_, w, h)| (w, h)).collect::<Vec<_>>(), [(6, 6)]);
        assert_eq!(shown_images(&link.emu.lock().term).len(), 1);
    }

    /// An emulator 30 by 12 with the startup logo as `Session::show_banner`
    /// leaves it on Windows: the picture's cells and a line beside it on the
    /// top four rows, and a prompt below.
    fn with_logo() -> Emulator {
        let writer = ToShell::direct(Box::new(Vec::new()));
        let state = Arc::new(Mutex::new(ShellState::default()));
        let size = Arc::new(Mutex::new(WindowSize { num_lines: 12, num_cols: 30, cell_width: 10, cell_height: 20 }));
        let proxy = EventProxy { ink: tab_ink(), writer, state, size, waker: Waker::from_fn(|_| {}) };
        let term = Term::new(TermConfig { scrolling_history: 100, ..TermConfig::default() }, &GridSize { cols: 30, rows: 12 }, proxy);
        let mut e = Emulator::new(term);
        for row in 0..4 {
            for col in 0..3 {
                e.term.grid_mut()[Line(row as i32)][Column(1 + col as usize)] = graphics::placeholder_cell(7, 1, row, col);
            }
        }
        e.processor.advance(&mut e.term, b"\x1b[2;6HFlow 0.1.19\x1b[6;1H$ ");
        e.banner = Some(Banner { image: 7, placement: 1 });
        e
    }

    #[test]
    fn the_graphics_apis_logo_ends_the_banners_last_line_or_its_name_where_theres_no_room() {
        let images = Mutex::new(ImageStore::default());
        let cell = (10.0, 20.0);
        let text = |e: &Emulator| e.text_between(0, 1);
        // After the line: the startup logo's own cells are at its left.
        let marks = |e: &Emulator| (10..30).filter_map(|c| graphics::read_placeholder(&e.term.grid()[Line(0)][Column(c)]).map(|m| (c, m.col))).collect::<Vec<_>>();
        // Vulkan's wordmark, after "zsh ·" and a space, in whole cells.
        let mut e = with_logo();
        e.run(b"\x1b[1;6Hzsh \xc2\xb7");
        let vulkan = crate::logos::api("Vulkan").unwrap();
        let (_, cols) = vulkan.in_row(cell);
        put_api_logo(&mut e, &images, &vulkan, cell, (0, 10), 20);
        let placed = marks(&e);
        assert_eq!(placed.len(), cols as usize);
        assert_eq!(placed.first().map(|&(c, col)| (c, col)), Some((11, Some(0))), "a space after the line");
        // Metal's icon alone, without its name.
        let mut e = with_logo();
        let metal = crate::logos::api("Metal").unwrap();
        let (_, cols) = metal.in_row(cell);
        put_api_logo(&mut e, &images, &metal, cell, (0, 10), 20);
        assert_eq!(marks(&e).len(), cols as usize);
        assert!(!text(&e).contains("Metal"), "{:?}", text(&e));
        // No room for the logo: the name, as before.
        let mut e = with_logo();
        put_api_logo(&mut e, &images, &crate::logos::api("Vulkan").unwrap(), cell, (0, 10), 7);
        assert!(marks(&e).is_empty());
        assert!(text(&e).trim_end().ends_with("Vulkan"), "{:?}", text(&e));
    }

    #[test]
    fn a_press_on_the_turning_startup_logo_finds_it_and_on_a_programs_model_doesnt() {
        let mut e = with_logo();
        let images = Mutex::new(ImageStore::default());
        let logo = images.lock().show_own(crate::logos::sgi(64, 20.0).expect("the logo"), 3, 4);
        // The same cube turning, but a program's.
        let slot = Arc::new(model::Slot::default());
        slot.set(crate::logos::sgi_cube().expect("the cube"));
        let turning = model::Style { animate: true, ..model::Style::default() };
        let program = images.lock().show_own(Image::model(64, 64, Arc::new(model::Shown::new(slot, turning, 20.0))), 3, 4);
        for (row, col) in [(0, 1), (3, 3)] {
            e.term.grid_mut()[Line(row)][Column(col)] = graphics::placeholder_cell(logo.id, logo.placement, row as u32, col as u32 - 1);
        }
        e.term.grid_mut()[Line(6)][Column(1)] = graphics::placeholder_cell(program.id, program.placement, 0, 0);
        assert!(own_model_at(&e, &images, (0, 1)).is_some_and(|m| m.flows_own));
        assert!(own_model_at(&e, &images, (3, 3)).is_some());
        assert!(own_model_at(&e, &images, (6, 1)).is_none(), "a program's model is the program's");
        assert!(own_model_at(&e, &images, (1, 10)).is_none(), "its line beside it");
        assert!(own_model_at(&e, &images, (5, 0)).is_none());
    }

    /// How many of the top rows show the logo, and whether its line is there.
    fn logo_shown(e: &Emulator) -> (usize, bool) {
        let grid = e.term.grid();
        let rows = (0..4).filter(|&l| (0..30).any(|c| graphics::read_placeholder(&grid[Line(l)][Column(c)]).is_some())).count();
        (rows, e.text_between(0, 12).contains("Flow 0.1.19"))
    }

    // ConPTY's bytes, as it sent them for a program that drew at the top of
    // the alternate screen, then left it: its frames begin with `?25l` and
    // end with `?25h`, and the main screen is repainted from the top.
    const FULL_SCREEN: &str = "\x1b[?25l\x1b[H#####\x1b[K\r\n\x1b[K\x1b[1;6H\x1b[?25h";
    fn left_full_screen() -> String {
        format!("\x1b[?1049l\x1b[?25l\x1b[H{}$ \x1b[K\r\n$ \x1b[K\x1b[7;3H\x1b[?25h", "\x1b[K\r\n".repeat(5))
    }

    #[test]
    fn the_startup_logo_comes_back_when_conpty_repaints_over_it() {
        let mut e = with_logo();
        advance(&mut e, format!("\x1b[?1049h{FULL_SCREEN}").as_bytes());
        advance(&mut e, left_full_screen().as_bytes());
        assert_eq!(logo_shown(&e), (4, true));
        assert_eq!(e.term.grid().cursor.point, Point::new(Line(6), Column(2)));
        // From then on ConPTY repaints from the top after each resize too.
        e.repaint_due();
        advance(&mut e, format!("\x1b[?25l\x1b[H{}$ \x1b[K\r\n$ \x1b[K\x1b[7;3H\x1b[?25h", "\x1b[K\r\n".repeat(5)).as_bytes());
        assert_eq!(logo_shown(&e), (4, true));
        // A full-screen program that leaves the top alone changes nothing.
        advance(&mut e, b"\x1b[?1049h\x1b[?25l\x1b[9;1H###\x1b[?25h\x1b[?1049l\x1b[?25l\x1b[7;3H\x1b[?25h");
        assert_eq!(logo_shown(&e), (4, true));
    }

    #[test]
    fn clearing_the_screen_still_takes_the_startup_logo_away() {
        // What ConPTY sends for cls: the screen blanked from the top, the
        // scrollback cleared, and the screen blanked again.
        let blank = "\x1b[K\r\n".repeat(11);
        let cls = format!("\x1b[?25l\x1b[H{blank}\x1b[K\x1b[H\x1b[?25h\x1b[3J\x1b[?25l\x1b[H{blank}\x1b[K\x1b[H\x1b[?25h$ ");
        let mut e = with_logo();
        advance(&mut e, cls.as_bytes());
        assert_eq!(logo_shown(&e), (0, false));
        // Even when a resize's repaint was due and the clear's first frame is
        // taken for it.
        let mut e = with_logo();
        e.repaint_due();
        advance(&mut e, cls.as_bytes());
        assert_eq!(logo_shown(&e), (0, false));
        // And it stays away.
        advance(&mut e, format!("\x1b[?1049h{FULL_SCREEN}").as_bytes());
        advance(&mut e, left_full_screen().as_bytes());
        assert_eq!(logo_shown(&e), (0, false));
        assert!(e.banner.is_none());
    }

    fn prompt() -> ShellState {
        ShellState { integrated: true, at_prompt: true, ..ShellState::default() }
    }

    #[test]
    fn a_command_runs_from_enter_after_typing_until_the_shell_says_done() {
        let mut st = prompt();
        st.sent(b"ls");
        st.sent(b"\r");
        assert!(st.running_since.is_some() && !st.at_prompt);
        st.command_done(Some(2), None);
        let done = st.finished.take().expect("a finished command");
        assert_eq!(done.exit, Some(2));
        assert!(done.secs >= 0.0 && done.secs < 1.0);

        // A bare Enter at a prompt runs nothing, so its "done" reports none.
        st.at_prompt = true;
        st.sent(b"\r");
        st.command_done(Some(2), None);
        assert_eq!(st.finished, None);

        // A pasted line with its own Enter counts; without integration nothing does.
        st.at_prompt = true;
        st.sent(b"make\r");
        assert!(st.running_since.is_some());
        let mut plain = ShellState { at_prompt: true, ..ShellState::default() };
        plain.sent(b"ls\r");
        assert!(plain.running_since.is_none());
    }

    #[test]
    fn progress_is_guessed_only_from_a_figure_redrawn_in_place() {
        assert_eq!(percent_in("Downloading  45% [====>   ]"), Some(45.0));
        assert_eq!(percent_in("12.5 % of 3 files, 100%"), Some(100.0));
        assert_eq!(percent_in("discount: 250% off"), None);
        assert_eq!(percent_in("no figure here"), None);

        let mut guess = ProgressGuess::default();
        assert_eq!(guess.update(7, "  10% [=>    ]"), None);
        assert_eq!(guess.update(7, "  20% [==>   ]"), None);
        assert_eq!(guess.update(7, "  35% [===>  ]"), Some(35.0));
        // A new line starts over: printed percentages don't make a bar.
        assert_eq!(guess.update(8, "  40% done"), None);
        assert_eq!(guess.update(9, "  50% done"), None);
    }

    /// A terminal of `cols` × `rows` that has been sent `bytes`.
    fn term_with(cols: usize, rows: usize, bytes: &[u8]) -> Term<alacritty_terminal::event::VoidListener> {
        let mut term = Term::new(TermConfig::default(), &GridSize { cols, rows }, alacritty_terminal::event::VoidListener);
        Processor::<StdSyncHandler>::new().advance(&mut term, bytes);
        term
    }

    /// Every row from the top of the scrollback, without trailing blanks.
    fn rows_of<L>(term: &Term<L>) -> Vec<String> {
        let grid = term.grid();
        (-(grid.history_size() as i32)..grid.screen_lines() as i32)
            .map(|l| (0..grid.columns()).map(|c| grid[Line(l)][Column(c)].c).collect::<String>().trim_end().to_string())
            .collect()
    }

    /// The numbers of the rows that start one of [`numbered`]'s lines.
    fn numbers_in<L>(term: &Term<L>) -> Vec<usize> {
        rows_of(term).iter().filter_map(|r| r.strip_prefix("line-")?.get(..3)?.parse().ok()).collect()
    }

    /// `count` lines `line-001-xxx…`, each `width` columns long.
    fn numbered(count: usize, width: usize) -> String {
        (1..=count).map(|i| format!("line-{i:03}-{}\r\n", "x".repeat(width - 9))).collect()
    }

    #[test]
    fn a_line_that_wraps_near_the_top_grows_down_instead_of_into_the_scrollback() {
        let long = format!("$ {}", "a".repeat(93));
        let mut term = term_with(100, 30, format!("{long}\r\nok\r\n$ ").as_bytes());
        resize_as_conpty(&mut term, GridSize { cols: 90, rows: 30 });

        assert_eq!(term.grid().history_size(), 0);
        let rows = rows_of(&term);
        assert_eq!(rows[..4], [&long[..90], &long[90..], "ok", "$"]);
        assert_eq!(term.grid().cursor.point, Point::new(Line(3), Column(2)));
    }

    #[test]
    fn growing_wider_or_taller_leaves_the_scrollback_where_it_is() {
        // At 90 columns each 94-column line takes two rows; the screen starts
        // with the end of line 26.
        let mut term = term_with(90, 30, numbered(40, 94).as_bytes());
        assert_eq!(rows_of(&term)[term.grid().history_size()], "xxxx");

        resize_as_conpty(&mut term, GridSize { cols: 100, rows: 30 });
        assert_eq!(numbers_in(&term), (1..=40).collect::<Vec<_>>());
        // Lines 1 to 25 take a row each now, and so does the start of line 26,
        // whose end stays at the top of the screen with lines 27 to 40 below.
        assert_eq!(term.grid().history_size(), 26);
        assert_eq!(rows_of(&term)[26], "xxxx");
        assert_eq!(term.grid().cursor.point, Point::new(Line(15), Column(0)));

        resize_as_conpty(&mut term, GridSize { cols: 100, rows: 40 });
        assert_eq!(numbers_in(&term), (1..=40).collect::<Vec<_>>());
        assert_eq!(term.grid().history_size(), 26);
        assert_eq!(term.grid().cursor.point, Point::new(Line(15), Column(0)));
    }

    #[test]
    fn rows_that_no_longer_fit_go_into_the_scrollback_in_order() {
        // A full screen at 100 columns: lines 12 to 40, and the cursor below.
        let mut term = term_with(100, 30, numbered(40, 94).as_bytes());
        assert_eq!(term.grid().history_size(), 11);

        resize_as_conpty(&mut term, GridSize { cols: 90, rows: 30 });
        assert_eq!(numbers_in(&term), (1..=40).collect::<Vec<_>>());
        // Every line now takes two rows, and the cursor stays at the bottom.
        assert_eq!(term.grid().history_size(), 2 * 40 + 1 - 30);
        assert_eq!(rows_of(&term)[2 * 40 - 1], "xxxx");
        assert_eq!(term.grid().cursor.point, Point::new(Line(29), Column(0)));
    }

    #[test]
    fn a_wrapped_double_line_rewraps_through_conptys_resize_too() {
        // Below lines gone into the scrollback, as `Emulator::resize` has it
        // on Windows.
        let (mut e, state, replies) = bare(20, 6);
        let lines: String = (1..=8).map(|i| format!("{i}\r\n")).collect();
        feed(&mut e, &state, &replies, format!("{lines}\x1b#6abcdefghijklmnopqrstuvwxyz0123456\r\n$ ").as_bytes());
        let resize = |e: &mut Emulator, cols: usize| {
            let lifted = line_size::lift(e.term.grid_mut(), cols, true);
            resize_as_conpty(&mut e.term, GridSize { cols, rows: 6 });
            line_size::settle(e.term.grid_mut(), lifted);
        };
        let from_it = |e: &Emulator| e.text_between(8, e.cursor_abs_line() + 1);
        let all = "abcdefghijklmnopqrstuvwxyz0123456\n$\n";
        resize(&mut e, 12);
        assert_eq!((from_it(&e), size_of(&e, 8), e.text_between(8, 9)), (all.into(), LineSize::Wide, "abcdef".into()));
        resize(&mut e, 30);
        assert_eq!((from_it(&e), size_of(&e, 8), e.text_between(8, 9)), (all.into(), LineSize::Wide, "abcdefghijklmno".into()));
        assert_eq!(e.text_between(7, 8), "8\n");
        // A double line at the scrollback's end, what wrapped from it at the
        // screen's top: ConPTY's rewrap parts them there, as it does a single
        // line, but no letter is lost.
        let (mut e, state, replies) = bare(20, 6);
        feed(&mut e, &state, &replies, b"\x1b#6abcdefghijklmnopqrstuvwxyz0123456\r\na\r\nb\r\nc\r\nd");
        assert_eq!((e.term.grid().history_size(), size_of(&e, 0)), (1, LineSize::Wide));
        for cols in [12, 20, 30, 20] {
            resize(&mut e, cols);
            let text = e.text_between(0, e.cursor_abs_line() + 1).replace('\n', "");
            assert_eq!((text, size_of(&e, 0)), ("abcdefghijklmnopqrstuvwxyz0123456abcd".into(), LineSize::Wide), "at {cols} columns");
        }
        // Nothing below the cursor is kept, for ConPTY paints it again: a
        // double line there goes, and the line before it keeps its own text.
        let (mut e, state, replies) = bare(20, 6);
        feed(&mut e, &state, &replies, b"0123456789abcdefghi KL\x1b#6\x1b[1;5H");
        resize(&mut e, 12);
        let grid = e.term.grid();
        assert!((-(grid.history_size() as i32)..6).all(|l| (0..12).all(|c| grid[Line(l)][Column(c)].c != '\u{FDD0}')));
        assert_eq!(e.text_between(0, 2), "0123456789abcdefghi\n");
    }

    #[test]
    fn a_pictures_rows_stay_whole_through_conptys_rewrap_too() {
        graphics::tests::picture_through_a_rewrap(|term, cols| resize_as_conpty(term, GridSize { cols, rows: 10 }));
    }

    /// A shell that has stopped reading: each write waits until it's let go.
    struct Stuck {
        open: Arc<(Mutex<bool>, Condvar)>,
        heard: Arc<Mutex<Vec<u8>>>,
    }

    impl Write for Stuck {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            let (lock, opened) = &*self.open;
            let mut open = lock.lock();
            while !*open {
                opened.wait(&mut open);
            }
            self.heard.lock().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn a_shell_that_stops_reading_never_holds_up_whoever_writes_to_it() {
        let open = Arc::new((Mutex::new(false), Condvar::new()));
        let heard = Arc::new(Mutex::new(Vec::new()));
        let to = ToShell::spawn_with(Box::new(Stuck { open: open.clone(), heard: heard.clone() }), "pty-write-test".into(), 1024, 16);
        let started = Instant::now();
        assert!(to.send(b"k1"));
        // Behind, a move goes while there's room for it, the rest are
        // dropped; keys still go, up to the room, and past it none.
        let moves = (0..100).filter(|_| to.send_motion(b"\x1b[<35;1;1M")).count();
        assert!(to.send(b"k2"));
        assert!(!to.send(&[b'x'; 2000]));
        assert!(to.send(b"k3"));
        assert!(started.elapsed() < Duration::from_millis(500), "each send returns at once");
        assert_eq!(moves, 1);
        // Reading again, the program hears what was kept, in order.
        *open.0.lock() = true;
        open.1.notify_all();
        let until = Instant::now() + Duration::from_secs(5);
        while !heard.lock().ends_with(b"k3") && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(heard.lock().as_slice(), b"k1\x1b[<35;1;1Mk2k3");
    }

    #[cfg(unix)]
    #[test]
    fn a_program_that_never_reads_its_pty_never_holds_up_the_window() {
        let pair = native_pty_system().openpty(PtySize { rows: 24, cols: 80, pixel_width: 0, pixel_height: 0 }).unwrap();
        // Raw, as a program that takes the mouse makes its terminal: a pty in
        // its usual lines would throw away what doesn't fit rather than wait.
        let mut sleep = portable_pty::CommandBuilder::new("sh");
        sleep.args(["-c", "stty raw -echo; printf ready; sleep 5"]);
        let mut child = pair.slave.spawn_command(sleep).unwrap();
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader().unwrap();
        let mut said = Vec::new();
        while !said.ends_with(b"ready") {
            let mut buf = [0u8; 64];
            let n = reader.read(&mut buf).unwrap();
            assert!(n > 0, "the program ended first");
            said.extend_from_slice(&buf[..n]);
        }
        let to = ToShell::spawn(pair.master.take_writer().unwrap(), "pty-write-test".into());
        let started = Instant::now();
        // Far more than a pty holds for a program (a few kilobytes), which
        // written straight from the window's thread would have stopped it.
        for _ in 0..256 {
            assert!(to.send(&[b'a'; 1024]));
        }
        assert!(started.elapsed() < Duration::from_millis(500), "each send returns at once");
        std::thread::sleep(Duration::from_millis(200));
        assert!(to.0.queue.lock().bytes > 0, "the pty filled, so the writer waits on it");
        to.close();
        let _ = child.kill();
    }

    #[test]
    fn a_settings_question_left_unanswered_down_the_pipe_still_lets_da1_after_it_end_the_read() {
        // The SDK puts DA1 after its request, since it can't read the pipe
        // against a clock. With as many questions waiting as the UI keeps,
        // this one is dropped, and DA1's answer comes back alone.
        let link = pipe_link(&Arc::default());
        for _ in 0..MAX_SETTINGS_ASKS {
            queue_settings(&link.state, SettingsAsk { request: String::new(), reply: None });
        }
        let asked = [flow_sdk::settings::request(r#"{"do":"list"}"#), flow_sdk::pipe::DA1.to_vec()].concat();
        let answers = piped(&link, &asked);
        assert!(flow_sdk::pipe::answered_da1(answers.as_bytes()), "{answers:?}");
        assert_eq!(flow_sdk::settings::answer(answers.as_bytes()), None);
    }

    #[test]
    fn a_settings_question_down_the_pipe_waits_for_the_ui_and_its_answer_comes_back() {
        let link = pipe_link(&Arc::default());
        let state = link.state.clone();
        // The UI, taking the question and answering it a moment later.
        let ui = std::thread::spawn(move || {
            for _ in 0..300 {
                if let Some(ask) = state.lock().settings.pop() {
                    assert_eq!(ask.request, "eyJkbyI6Imxpc3QifQ==");
                    ask.reply.expect("down the pipe").send("\x1b]7732;eyJvayI6dHJ1ZX0=\x07".into()).unwrap();
                    return;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            panic!("never asked");
        });
        let answers = piped(&link, &flow_sdk::settings::request(r#"{"do":"list"}"#));
        ui.join().unwrap();
        assert_eq!(flow_sdk::settings::answer(answers.as_bytes()).as_deref(), Some(r#"{"ok":true}"#));
    }

    #[test]
    fn a_settings_question_the_ui_leaves_unanswered_down_the_pipe_ends_its_wait_at_once_with_nothing() {
        // As Flow does with a request without its pane's own key: the UI
        // takes it and drops its reply, so only DA1's answer comes back,
        // not after the pipe's three seconds.
        let link = pipe_link(&Arc::default());
        let state = link.state.clone();
        let ui = std::thread::spawn(move || {
            for _ in 0..300 {
                if let Some(ask) = state.lock().settings.pop() {
                    drop(ask.reply);
                    return;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            panic!("never asked");
        });
        let started = Instant::now();
        let asked = [flow_sdk::settings::request(r#"{"do":"list"}"#), flow_sdk::pipe::DA1.to_vec()].concat();
        let answers = piped(&link, &asked);
        ui.join().unwrap();
        assert!(started.elapsed() < Duration::from_secs(2), "{:?}", started.elapsed());
        assert!(flow_sdk::pipe::answered_da1(answers.as_bytes()), "{answers:?}");
        assert_eq!(flow_sdk::settings::answer(answers.as_bytes()), None);
    }

    /// What a screen reader is given of the screen: each row's letters,
    /// where they are, whether it wraps, and the caret after a prompt's blank.
    #[test]
    fn the_rows_in_view_read_as_their_letters_with_the_cursor_among_them() {
        let writer = ToShell::direct(Box::new(Vec::new()));
        let state = Arc::new(Mutex::new(ShellState::default()));
        let size = Arc::new(Mutex::new(WindowSize { num_lines: 5, num_cols: 10, cell_width: 10, cell_height: 20 }));
        let proxy = EventProxy { ink: tab_ink(), writer: writer.clone(), state: state.clone(), size: size.clone(), waker: Waker::from_fn(|_| {}) };
        let term = Term::new(TermConfig { scrolling_history: 100, ..TermConfig::default() }, &GridSize { cols: 10, rows: 5 }, proxy);
        let mut e = Emulator::new(term);
        let replies = Replies { writer, size, images: Arc::new(Mutex::new(ImageStore::default())), models: Arc::new(Mutex::new(ModelStore::default())) };
        // A line that wraps, a wide letter and one with a mark on it, a
        // double-width line, then a prompt with the cursor after its blank.
        feed(&mut e, &state, &replies, "abcdefghijKL\r\n日e\u{301}\r\n\x1b#6wide\r\n$ ".as_bytes());
        let (rows, cursor) = e.rows_in_view();
        let read: Vec<(&str, bool)> = rows.iter().map(|r| (r.text.as_str(), r.wraps)).collect();
        assert_eq!(read, [("abcdefghij", true), ("KL", false), ("日e\u{301}", false), ("wide", false), ("$ ", false)]);
        let at = |r: usize| rows[r].letters.iter().map(|l| (l.bytes, l.col, l.cells)).collect::<Vec<_>>();
        assert_eq!(at(2), [(3, 0, 2), (3, 2, 1)]);
        assert_eq!(at(3), [(1, 0, 2), (1, 2, 2), (1, 4, 2), (1, 6, 2)]);
        assert_eq!(cursor, Some((4, 2)));
        // Scrolled back, the cursor is out of view.
        feed(&mut e, &state, &replies, b"\r\nmore\r\n");
        e.term.scroll_display(alacritty_terminal::grid::Scroll::Delta(2));
        let (rows, cursor) = e.rows_in_view();
        assert_eq!((rows[0].text.as_str(), cursor), ("abcdefghij", None));
    }
}
