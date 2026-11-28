//! UI state and drawing: title bar, terminal grid, assistant panel, dialogs.

use std::cell::{Cell, RefCell};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::vte::ansi::CursorShape;
use unicode_width::UnicodeWidthChar;

use crate::ai::tools::Risk;
use crate::ai::Provider;
use crate::ai::usage::fmt_tokens;
use crate::platform::{keys, UpdateState};
use crate::ai::tools::StepStatus;
use crate::i18n::{self, t, tf};
use crate::render::palette::{self, FOREGROUND};
use crate::render::{rgba, Canvas, Rgba, Scaled, ICON_IMAGE};
use crate::graphics;
use crate::line_size::{self, LineSize};
use crate::box_drawing;
use crate::mosaic;
use crate::history::{Filter, Found};
use crate::session::Session;
use crate::settings::{self, ColorPicker, Control, SettingsMenu};
use crate::shell::ShellProfile;
use crate::text::{self, Style};
use crate::APP_NAME;

pub const ACCENT: [u8; 3] = [0x7C, 0xE7, 0xFF];
pub const ACCENT2: [u8; 3] = [0xB7, 0x8C, 0xFF];
const DIM: [u8; 3] = [0x8A, 0x95, 0xAB];
const CODE: [u8; 3] = [0xFF, 0xD3, 0x8A];
const GOOD: [u8; 3] = [0x7E, 0xE0, 0x9F];
const WARN: [u8; 3] = [0xFF, 0xC8, 0x6B];
const BAD: [u8; 3] = [0xFF, 0x7A, 0x8E];
/// Caps Lock's notice: an orange none of the others is near, to be seen at
/// a glance.
const ORANGE: [u8; 3] = [0xFF, 0x9A, 0x3C];
const PANEL_TINT: [u8; 3] = [0x0D, 0x13, 0x22];
/// Height of the title bar in points. On macOS the traffic lights are
/// centered in it.
pub const TITLE_HEIGHT: f32 = 40.0;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }
    pub fn contains(&self, p: (f32, f32)) -> bool {
        p.0 >= self.x && p.0 < self.x + self.w && p.1 >= self.y && p.1 < self.y + self.h
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    Tab(usize),
    /// The × in a tab.
    CloseTab(usize),
    /// The title bar's three lines, left of the tabs: the tab sidebar.
    TabSidebar,
    /// The sidebar's card, where nothing else is.
    SidebarCard,
    /// A row of the sidebar, by its place among its rows.
    SidebarRow(usize),
    /// A group's or a split tab's fold, the ▸ or ▾ before its name.
    SidebarFold(usize),
    /// A row's ×, which closes its tab or its group.
    SidebarClose(usize),
    /// A row's …, which opens its menu.
    SidebarMore(usize),
    /// The sidebar's foot: a new tab, and a new group of the tab in front.
    SidebarNewTab,
    SidebarNewGroup,
    /// An item of a row's menu, by its place.
    SidebarMenuItem(usize),
    /// The pin at its foot: pinned beside the terminal, or popping out.
    SidebarPin,
    /// Its right edge, which a drag makes wider or narrower.
    SidebarEdge,
    /// The + after the tabs: a tab at once, the default shell in Settings'
    /// theme.
    NewTab,
    /// The … beside it: the new-tab picker.
    NewTabChoose,
    /// A shell, or a theme, in the new-tab picker.
    TabPickerShell(usize),
    TabPickerTheme(usize),
    /// The picker's Open.
    TabPickerOpen,
    /// The picker's Save as theme…, which names what's worn now.
    TabPickerSave,
    ToggleAi,
    ToggleMode,
    /// The title bar's e-mode (efficient mode).
    ToggleEfficient,
    /// The title bar's on-top, which keeps the window on top of every other.
    ToggleOnTop,
    /// The title bar's clock of a recording under way, which stops it.
    StopRecording,
    KeyStatus,
    OpenSettings,
    Minimize,
    Maximize,
    Close,
    Approve,
    Edit,
    Skip,
    Stop,
    /// The task offered at the foot of the panel: Carry on and Not now.
    ResumeOn,
    ResumeLater,
    /// The panel's Attach, which opens the system's file panel.
    Attach,
    /// A model in `/model`'s list: its place in `provider::model_list`.
    ModelPick(usize),
    /// A file's chip under the input: its ✕, and (one that looks like a
    /// secret) the chip itself, which confirms it may go.
    AttachRemove(usize),
    AttachConfirm(usize),
    /// The app name in the title bar: opens About.
    About,
    AboutCard,
    AboutClose,
    AboutUpdate,
    AboutRestart,
    AboutLegal,
    /// The link to this version's release page.
    AboutReleaseNotes,
    /// About's Keys, between Release notes and Resources: help's page of
    /// every key as it's bound.
    AboutKeys,
    /// About's Resources, beside Release notes: the table of what each tab
    /// uses, and the card it's drawn on, and its close.
    AboutResources,
    /// About's credits, under its tagline: the maker's name, which opens
    /// their page on GitHub.
    AboutCredits,
    ResourcesCard,
    ResourcesClose,
    /// Copy under a block of code in the assistant's reply, or at the end
    /// of a line of nothing but a span of code: the entry and the block.
    CodeCopy(usize, usize),
    /// Run under a block of shell commands: puts them at the prompt.
    CodeRun(usize, usize),
    /// Copy on the assistant's reply under the pointer: the entry.
    ReplyCopy(usize),
    /// Copy on the card of a command the assistant ran or waits to run:
    /// the entry.
    CardCopy(usize),
    /// Undo on the card of a file the assistant wrote: the entry.
    CardUndo(usize),
    /// The settings card itself (clicks there don't close it).
    SettingsCard,
    SettingsClose,
    /// Settings' Collapse all and Expand all, beside its ✕.
    SettingsCollapseAll,
    SettingsExpandAll,
    /// Settings' search field, at the end of the row of tabs.
    SettingsSearch,
    /// In the row of tabs under Settings' title: the open tab whose look it
    /// changes, or the look new tabs open with.
    SettingsOnTab(usize),
    SettingsOnNewTabs,
    SettingRow(usize),
    /// Rect is the slider track, so a click maps straight to a value.
    SettingSlider(usize),
    SettingToggle(usize),
    SettingChoice(usize, usize),
    SettingAction(usize, usize),
    /// A text row's field: a click there starts typing in it.
    SettingText(usize),
    /// A place in a text row's field: the row, and the line and character a
    /// click there puts the caret at.
    SettingCaret(usize, usize, usize),
    /// The color picker over the settings, and its parts.
    PickerCard,
    PickerField,
    PickerHue,
    PickerDone,
    PickerCancel,
    /// The box for a CRT preset's or a theme's name, and its buttons.
    NameCard,
    NameSave,
    /// The line between two of a split tab's panes, by its place in
    /// `Split::dividers`, and whether it stands upright (moves across).
    Divider(usize, bool),
    /// A pane's pill saying how many lines are below while it's scrolled
    /// back, which jumps back to the bottom, by the pane's place in its tab.
    ScrollBottom(usize),
    /// The close box in a pane's window frame (a theme's, `ui::Frame`), by
    /// the pane's place in its tab: it closes the pane, or its tab where it's
    /// the only one, as ⌘W does.
    FrameClose(usize),
    /// The depth gadgets at a window frame's title bar's right, which bring
    /// Flow's window in front of every other and send it behind them.
    FrameFront,
    FrameBack,
    /// The sizing gadget in a window frame's bottom right corner, which
    /// drags the window's corner where it can (`frame_sizes`).
    FrameSize,
    /// The ✕ at a pane's top right while the pointer is over it, on a split
    /// tab's panes that wear no frame (which has its own close box), by its
    /// place in its tab: it closes the pane.
    PaneClose(usize),
    /// A pane's scrollbar (`scrollbar::Bar::zone`), by its place in its tab.
    Scrollbar(usize),
    /// The theme gallery over Settings: its card, its ✕, a package, a
    /// theme in it, one of its buttons, and Check now where it has none.
    GalleryCard,
    GalleryClose,
    GalleryPackage(usize),
    GalleryTheme(usize),
    GalleryAct(usize),
    GalleryCheck,
    NameCancel,
    /// The question whether to read a model's file, and its answers.
    ModelCard,
    ModelAllow,
    ModelDeny,
    /// An outside app's card ([`McpCard`]): itself, its yes and no, and
    /// one of the kinds it may be allowed, switched.
    McpCard,
    McpAllow,
    McpDeny,
    McpKind(u8),
    /// An MCP server's question ([`ServerAsk`]): itself, one of its fields,
    /// an option of one of its choices, and Send (or Open) and Decline.
    AskCard,
    AskField(u8),
    AskOption(u8, u8),
    AskSend,
    AskDecline,
    /// A call that waits for the user in the browser: made again now, or
    /// given up.
    CardRetry,
    CardGiveUp,
    /// The title bar's sign that an update waits: opens About.
    UpdatePill,
    UpdateAskCard,
    UpdateAskYes,
    UpdateAskNo,
    /// The question whether to restart into the Flow an update just put in
    /// place, and its answers.
    RestartAskCard,
    RestartAskNow,
    RestartAskLater,
    /// The command history's search card, a command found in it, and a filter.
    HistoryCard,
    HistoryItem(usize),
    HistoryFilter(usize),
    HistoryClose,
    /// Ask why, at the terminal's foot after a command failed.
    AskWhy,
    /// The command palette's card, and an item found in it (a click outside
    /// closes it).
    PaletteCard,
    PaletteItem(usize),
    /// The find bar over the terminal, its ↑ (an older match), ↓ (a newer)
    /// and ✕.
    FindBar,
    FindPrev,
    FindNext,
    FindClose,
    /// The find bar's `.*`: what's typed is a pattern, or text as written.
    FindPattern,
    /// A page of help, its ✕, its ← once a link was followed, and a link
    /// on it (its place among the page's links).
    HelpCard,
    HelpClose,
    HelpBack,
    HelpLink(usize),
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Layout {
    pub width: f32,
    pub height: f32,
    pub scale: f32,
    pub title: Rect,
    pub term: Rect,
    pub panel: Option<Rect>,
    pub cols: usize,
    pub rows: usize,
    pub cell_w: f32,
    pub cell_h: f32,
    /// A shell window's frame round a pane wearing an old machine's look
    /// that had one: `term` is the room it leaves the text.
    pub frame: Option<Frame>,
}

/// A shell window's frame round a pane, as the Flow notanAmiga CLI's
/// desktop drew one: an outline `line` wide round `outer`, the pane's
/// rectangle, a title bar `bar` tall along its top, and a sizing gadget in
/// its bottom right corner.
///
/// The bar and its gadgets are counted as the machine drew them, in its
/// lines (a tenth of the bar, [`Frame::unit`]) down and its pixels (half a
/// line) across, which at a scale of 1 and the font's 16 pixels are the
/// screen's own: a bar of 20 pixels, its white lines two of the machine's
/// lines thick and as far apart, gadgets 22 pixels wide.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    pub outer: Rect,
    pub line: f32,
    pub bar: f32,
}

/// A piece of a gadget's picture, as the machine drew it: its edges in the
/// machine's pixels across and lines down from the gadget's top left, and
/// what it's painted with.
type Piece = (f32, f32, f32, f32, Pen);

/// What a frame's pieces are painted with: the text's color (the
/// machine's white), the background's (its blue), or black.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Pen {
    White,
    Blue,
    Black,
}

/// The depth gadget at the title bar's right that brings the window in
/// front of the others: a box drawn in outline, and a black one, the
/// window, in front of it, lower and to the right.
const TO_FRONT: [Piece; 6] = [
    (0.0, 0.0, 22.0, 10.0, Pen::White),
    (2.0, 1.0, 16.0, 2.0, Pen::Blue),
    (2.0, 6.0, 16.0, 7.0, Pen::Blue),
    (2.0, 1.0, 4.0, 7.0, Pen::Blue),
    (14.0, 1.0, 16.0, 7.0, Pen::Blue),
    (6.0, 3.0, 20.0, 9.0, Pen::Black),
];

/// The one beside it that sends the window behind them: the black box
/// behind, the one in outline in front of it.
const TO_BACK: [Piece; 7] = [
    (0.0, 0.0, 22.0, 10.0, Pen::White),
    (2.0, 1.0, 16.0, 7.0, Pen::Black),
    (6.0, 2.0, 20.0, 9.0, Pen::White),
    (6.0, 2.0, 20.0, 3.0, Pen::Blue),
    (6.0, 8.0, 20.0, 9.0, Pen::Blue),
    (6.0, 2.0, 8.0, 9.0, Pen::Blue),
    (18.0, 2.0, 20.0, 9.0, Pen::Blue),
];

/// The sizing gadget in the bottom right corner: a small box and a larger
/// one overlapping it, both in outline.
const SIZING: [Piece; 9] = [
    (0.0, 0.0, 16.0, 9.0, Pen::White),
    (2.0, 1.0, 8.0, 2.0, Pen::Blue),
    (2.0, 3.0, 8.0, 4.0, Pen::Blue),
    (2.0, 1.0, 4.0, 4.0, Pen::Blue),
    (6.0, 1.0, 8.0, 4.0, Pen::Blue),
    (6.0, 3.0, 14.0, 4.0, Pen::Blue),
    (6.0, 7.0, 14.0, 8.0, Pen::Blue),
    (6.0, 3.0, 8.0, 8.0, Pen::Blue),
    (12.0, 3.0, 14.0, 8.0, Pen::Blue),
];

/// A gadget's width and the gap beside each, in the machine's pixels.
const GADGET_W: f32 = 22.0;
const GADGET_GAP: f32 = 2.0;
/// The sizing gadget's width in pixels and height in lines.
const SIZING_W: f32 = 16.0;
const SIZING_H: f32 = 9.0;
/// The white end the lines stop at, before the gadgets, in pixels.
const LINES_END: f32 = 3.0;

impl Frame {
    /// The frame round `outer` for text of `cell` at `scale`, on whole
    /// pixels, and the room it leaves the text: inside the outline and a
    /// gap of half a cell, below the title bar, which is a cell tall and
    /// the outline more, and above the sizing gadget.
    pub fn round(outer: Rect, cell: (f32, f32), scale: f32) -> (Frame, Rect) {
        let outer = Rect::new(outer.x.round(), outer.y.round(), outer.w.round(), outer.h.round());
        let line = (2.0 * scale).round().max(1.0);
        let gap = (cell.0 * 0.5).max(3.0 * scale).round();
        let bar = (cell.1 + 2.0 * line).round();
        let frame = Frame { outer, line, bar };
        let (x, y) = (outer.x + line + gap, outer.y + bar + gap);
        // The text ends above the sizing gadget, a line clear of it, so the
        // gadget never covers a letter in the corner.
        let foot = (line + gap).max((outer.y + outer.h) - frame.size_box().y + line);
        let room = Rect::new(x, y, (outer.w - 2.0 * (line + gap)).max(cell.0), (outer.y + outer.h - foot - y).max(cell.1));
        (frame, room)
    }

    /// One of the machine's lines: a tenth of the title bar. Its pixels
    /// across are half of one.
    pub fn unit(&self) -> f32 {
        self.bar / 10.0
    }

    /// A rectangle from its edges, each rounded to a whole pixel, so
    /// pieces that share an edge meet.
    fn edges(x0: f32, y0: f32, x1: f32, y1: f32) -> Rect {
        let (x0, y0) = (x0.round(), y0.round());
        Rect::new(x0, y0, x1.round() - x0, y1.round() - y0)
    }

    /// A piece of a gadget whose top left is at `at`, on the screen.
    fn piece(&self, at: (f32, f32), p: &Piece) -> Rect {
        let (px, u) = (self.unit() / 2.0, self.unit());
        Frame::edges(at.0 + p.0 * px, at.1 + p.1 * u, at.0 + p.2 * px, at.1 + p.3 * u)
    }

    /// The three white lines across the title bar right of the name, each
    /// as tall as the bar's fifth, as far apart: the bar's top, its middle
    /// and its foot, as the machine's were. Each is its top and foot.
    pub fn stripes(&self) -> [(f32, f32); 3] {
        let at = |k: f32| (self.outer.y + self.bar * k / 5.0).round();
        [(at(0.0), at(1.0)), (at(2.0), at(3.0)), (at(4.0), at(5.0))]
    }

    /// The close box at the title bar's left, where the windows of the
    /// frame's era had theirs: square, as tall as the bar, inside the
    /// outline.
    pub fn close_box(&self) -> Rect {
        Rect::new(self.outer.x + self.line, self.outer.y, self.bar, self.bar)
    }

    /// Where the name and the lines start, a gap right of the close box.
    fn after_close(&self) -> f32 {
        let close = self.close_box();
        (close.x + close.w + GADGET_GAP * self.unit() / 2.0).round()
    }

    /// The depth gadgets at the title bar's right, as the machine had them,
    /// the one that brings the window to the front and the one that sends
    /// it to the back, each a little wider than the bar is tall, a gap
    /// apart and from the outline. None where the bar hasn't room for them
    /// beside the close box.
    pub fn gadgets(&self) -> Option<(Rect, Rect)> {
        let px = self.unit() / 2.0;
        let right = self.outer.x + self.outer.w - self.line - GADGET_GAP * px;
        let back = Frame::edges(right - GADGET_W * px, self.outer.y, right, self.outer.y + self.bar);
        let left = right - (2.0 * GADGET_W + GADGET_GAP) * px;
        let front = Frame::edges(left, self.outer.y, left + GADGET_W * px, self.outer.y + self.bar);
        // Room for the lines' end and a bar's width of them besides.
        (front.x - (GADGET_GAP + LINES_END) * px - self.bar >= self.after_close()).then_some((front, back))
    }

    /// Where the lines end: the white end a gap left of the gadgets, or the
    /// outline where there are none. Its left edge, and its right.
    fn lines_end(&self) -> (f32, f32) {
        let px = self.unit() / 2.0;
        match self.gadgets() {
            Some((front, _)) => {
                let right = (front.x - GADGET_GAP * px).round();
                ((right - LINES_END * px).round(), right)
            }
            None => {
                let right = self.outer.x + self.outer.w - self.line;
                (right, right)
            }
        }
    }

    /// The sizing gadget in the bottom right corner, over the outline.
    pub fn size_box(&self) -> Rect {
        let (right, foot) = (self.outer.x + self.outer.w, self.outer.y + self.outer.h);
        let (px, u) = (self.unit() / 2.0, self.unit());
        Frame::edges(right - SIZING_W * px, foot - SIZING_H * u, right, foot)
    }
}

/// What the terminal shares the window with besides the title bar: the
/// assistant's panel at its foot (`panel`), a wide margin round it, as on an
/// 8-bit home computer's screen (`border`, an old machine's look,
/// `looks.rs`), and the tab sidebar pinned at its left (`left`, the room it
/// takes, in pixels).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Around {
    pub panel: bool,
    pub border: bool,
    pub left: f32,
}

impl Around {
    #[cfg(test)]
    pub fn new(panel: bool, border: bool) -> Self {
        Around { panel, border, left: 0.0 }
    }
}

impl Layout {
    /// The window's layout, `around` as it says: the title bar along the
    /// top, the whole width, and the rest laid out as a window `left`
    /// narrower would be, moved right by it.
    pub fn compute(width: f32, height: f32, scale: f32, cell: (f32, f32), around: Around) -> Self {
        let s = scale;
        let (panel_open, border) = (around.panel, around.border);
        let left = around.left.clamp(0.0, width * 0.5);
        let inner = width - left;
        let title_h = TITLE_HEIGHT * s;
        let pad = if border { (inner * 0.06).max(14.0 * s) } else { 14.0 * s };
        let title = Rect::new(0.0, 0.0, width, title_h);
        let top = title_h + if border { (height * 0.06).max(2.0 * s) } else { 2.0 * s };
        let panel = panel_open.then(|| {
            // At most what leaves the terminal room, which in a short window
            // is less than the panel's least.
            let most = (height - title_h - 140.0 * s).max(120.0 * s);
            let h = (height * 0.42).clamp((190.0 * s).min(most), most);
            Rect::new(left + pad * 0.7, height - h - pad * 0.7, inner - pad * 1.4, h)
        });
        let bottom = match panel {
            Some(p) => p.y - 8.0 * s,
            None if border => height - (height * 0.06).max(pad),
            None => height - pad,
        };
        let term = Rect::new(left + pad, top, (inner - 2.0 * pad).max(cell.0), (bottom - top).max(cell.1));
        let cols = ((term.w / cell.0).floor() as usize).max(10);
        let rows = ((term.h / cell.1).floor() as usize).max(3);
        Self { width, height, scale, title, term, panel, cols, rows, cell_w: cell.0, cell_h: cell.1, frame: None }
    }

    /// The same window with `rect` as its terminal, a pane's, its grid what
    /// that holds of the cell.
    pub fn within(&self, rect: Rect) -> Layout {
        self.within_cell(rect, (self.cell_w, self.cell_h))
    }

    /// As [`Layout::within`], with a cell of its own: a pane whose theme's
    /// font isn't the one in front's.
    pub fn within_cell(&self, rect: Rect, (cell_w, cell_h): (f32, f32)) -> Layout {
        let cols = ((rect.w / cell_w).floor() as usize).max(1);
        let rows = ((rect.h / cell_h).floor() as usize).max(1);
        Layout { term: rect, cols, rows, cell_w, cell_h, frame: None, ..*self }
    }

    /// Terminal cell under a window position (clamped to the grid).
    pub fn cell_at(&self, p: (f32, f32)) -> (usize, usize, bool) {
        let x = ((p.0 - self.term.x) / self.cell_w).max(0.0);
        let y = ((p.1 - self.term.y) / self.cell_h).max(0.0);
        let col = (x as usize).min(self.cols.saturating_sub(1));
        let row = (y as usize).min(self.rows.saturating_sub(1));
        let right_half = x.fract() > 0.5;
        (col, row, right_half)
    }

    /// [`Layout::cell_at`] on a line of `size`: a double line's cells are
    /// twice as wide, and only the first half of its columns show.
    pub fn cell_on_line(&self, p: (f32, f32), size: LineSize) -> (usize, usize, bool) {
        let (col, row, right) = self.cell_at(p);
        if !size.doubled() {
            return (col, row, right);
        }
        let x = ((p.0 - self.term.x) / (self.cell_w * 2.0)).max(0.0);
        ((x as usize).min(size.shown(self.cols) - 1), row, x.fract() > 0.5)
    }

    /// The smallest window, in whole pixels, whose terminal [`compute`]
    /// makes `cols` × `rows` cells (at least 10 × 3, as it does), the
    /// panel and a border as given. Found by trying sizes rather than worked
    /// back from the margins, since a border's grow with the window and the
    /// panel's height with the window's: the terminal's width follows the
    /// window's alone, and its height the window's and the border's margin,
    /// which the width sets, so the width comes first. Each grows with the
    /// window, so halving the range finds the first size that holds them.
    ///
    /// [`compute`]: Layout::compute
    #[cfg(test)]
    pub fn size_for(cols: usize, rows: usize, scale: f32, cell: (f32, f32), around: Around) -> (u32, u32) {
        Self::size_for_part(|l| (l.cols, l.rows), cols, rows, scale, cell, around)
    }

    /// As [`Layout::size_for`], for a part of the terminal: `part` gives the
    /// grid it has in a window's layout (a split tab's pane in front), which
    /// grows with the window as the whole does.
    pub fn size_for_part(part: impl Fn(&Layout) -> (usize, usize), cols: usize, rows: usize, scale: f32, cell: (f32, f32), around: Around) -> (u32, u32) {
        let at = |w: u32, h: u32| part(&Layout::compute(w as f32, h as f32, scale, cell, around));
        let width = smallest(|w| at(w, Self::LARGEST).0 >= cols);
        (width, smallest(|h| at(width, h).1 >= rows))
    }

    /// The largest window side [`Layout::size_for`] tries, in pixels: more
    /// than any screen has.
    const LARGEST: u32 = 1 << 15;

    /// The least whole text size, in points, at which a window of `window`
    /// pixels holds no more than `cols` × `rows`, `cell` being the cell at
    /// `size`: the window can't be smaller, so a grid it holds too much of
    /// needs larger letters. A cell grows with the text in proportion, near
    /// enough (its metrics are the font's, scaled), and the terminal's room
    /// in a given window doesn't change with it.
    #[cfg(test)]
    pub fn text_size_within((cols, rows): (usize, usize), window: (u32, u32), scale: f32, cell: (f32, f32), around: Around, size: f32) -> f32 {
        Self::text_size_within_part(|l| l.term, (cols, rows), window, scale, cell, around, size)
    }

    /// As [`Layout::text_size_within`], for a part of the terminal: `part`
    /// gives its room in a window's layout.
    pub fn text_size_within_part(part: impl Fn(&Layout) -> Rect, (cols, rows): (usize, usize), window: (u32, u32), scale: f32, cell: (f32, f32), around: Around, size: f32) -> f32 {
        let term = part(&Layout::compute(window.0 as f32, window.1 as f32, scale, cell, around));
        // Cells wider than the room shared among one column more, and taller
        // than among one row more.
        let wide = size * term.w / ((cols + 1) as f32 * cell.0);
        let tall = size * term.h / ((rows + 1) as f32 * cell.1);
        wide.max(tall).floor() + 1.0
    }
}

/// The least of 1 to [`Layout::LARGEST`] for which `holds` is true, `holds`
/// being false below some size and true from it on; the largest where it
/// never is.
fn smallest(holds: impl Fn(u32) -> bool) -> u32 {
    let (mut lo, mut hi) = (1, Layout::LARGEST);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if holds(mid) {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    lo
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardStatus {
    Waiting,
    Queued,
    Running,
    /// A server's call waits for the user to finish in the browser.
    InBrowser,
    Done { ok: bool },
    Skipped,
}

#[derive(Debug, Clone)]
pub struct ToolCard {
    pub title: String,
    /// The command line, for a command's card, which its Copy copies (the
    /// title says too whether you edited it).
    pub command: Option<String>,
    pub detail: String,
    pub risk: Option<Risk>,
    pub status: CardStatus,
    pub result: String,
    pub started: Option<Instant>,
    /// A plan to work on its own, whose buttons are Let it work and Ask
    /// each time.
    pub plan: bool,
    /// A file the assistant wrote, which its Undo puts back as it was.
    pub undo: Option<std::path::PathBuf>,
}

#[derive(Debug, Clone)]
pub enum Entry {
    User(String),
    Assistant(String),
    Thinking { text: String, started: Instant, ended: Option<Instant> },
    Tool(ToolCard),
    Search(String),
    Notice(String),
    Error(String),
    /// The files that went with the user's message before it, by name.
    Files(Vec<String>),
    /// A task left unfinished, offered to carry on: its id in the journal,
    /// the user's first words, the progress saved and what comes next,
    /// whether it waits for a restart, and the minutes left to work on its
    /// own, where it was let.
    Resume { task: i64, ask: String, summary: String, next: String, restart: bool, own: Option<u64>, steps: Option<(usize, usize)> },
}

pub struct AiPanel {
    pub open: bool,
    pub focus: bool,
    /// The message being typed, its caret and what's selected
    /// (`docs/design/assistant-input.md`).
    pub input: crate::panel_input::Input,
    pub entries: Vec<Entry>,
    pub busy: bool,
    pub auto_mode: bool,
    /// Rows scrolled up from the bottom.
    pub scroll: usize,
    /// The messages sent, which ↑ and ↓ bring back.
    pub history: crate::panel_input::Recall,
    /// Card index awaiting approval, if any.
    pub awaiting: Option<usize>,
    pub editing: Option<usize>,
    /// Text dragged over in the conversation, from where the drag began to
    /// where it is: each a row of the conversation's layout and a
    /// character in it.
    pub selection: Option<(Place, Place)>,
    /// The conversation as the last frame laid it out, for the mouse and
    /// Copy, which can't measure text themselves.
    pub laid: RefCell<PanelLaid>,
    /// The input as the last frame laid it out, for the keys and the mouse.
    pub input_laid: RefCell<crate::panel_input::Laid>,
    /// `/model`'s list while it's open over the input: the row chosen in
    /// `provider::model_list`.
    pub model_pick: Option<usize>,
    /// The provider in use's own list of models, as it last came
    /// (`ai::models`), and whether it's being asked for.
    pub live: Option<crate::ai::models::Live>,
    pub models_asking: bool,
    /// The task's steps the assistant set, pinned under the header while
    /// there are some.
    pub steps: Vec<(String, StepStatus)>,
    /// Files to go with the next message, a chip each under the input
    /// (`crate::ai::files`), and those still being read, by name.
    pub files: Vec<crate::ai::files::Attached>,
    pub reading: Vec<String>,
}

/// A row of the assistant's conversation and a character in it.
pub type Place = (usize, usize);

/// The assistant's conversation as it was last drawn: each row's text, and
/// where the rows in view and their characters are.
#[derive(Debug, Default)]
pub struct PanelLaid {
    /// The width it was laid out to, in cells: a selection from another
    /// width names other rows.
    pub cols: usize,
    pub rows: Vec<String>,
    /// How each row goes on from the one above, where the wrap carried it
    /// down, so a long line copies as one.
    pub wraps: Vec<Option<text::Wrapped>>,
    /// The conversation's part of the panel.
    pub area: Rect,
    /// The first row in view, the top of it, and a row's height.
    pub start: usize,
    pub top: f32,
    pub row_h: f32,
    /// Each row in view's characters' left edges, then its end.
    pub edges: Vec<Vec<f32>>,
}

impl PanelLaid {
    /// The place under `p`, or with `clamp` the nearest one in view, for a
    /// drag that's left the conversation.
    pub fn place_at(&self, p: (f32, f32), clamp: bool) -> Option<Place> {
        if self.edges.is_empty() || (!clamp && !self.area.contains(p)) {
            return None;
        }
        let i = ((p.1 - self.top) / self.row_h).floor().clamp(0.0, (self.edges.len() - 1) as f32) as usize;
        let edges = &self.edges[i];
        let char = edges.windows(2).position(|w| p.0 < (w[0] + w[1]) * 0.5).unwrap_or(edges.len().saturating_sub(1));
        Some((self.start + i, char))
    }

    /// The text between two places, a row's text to a line.
    /// The words from place to place, a line of the text's own to a line:
    /// a row the wrap carried down goes on from the one above as it was
    /// written, so a long command copies as one and runs as one.
    pub fn text_between(&self, a: Place, b: Place) -> String {
        let (from, to) = if a <= b { (a, b) } else { (b, a) };
        let mut text = String::new();
        for (row, line) in self.rows.iter().enumerate().take(to.0 + 1).skip(from.0) {
            let chars: Vec<char> = line.chars().collect();
            let wrap = self.wraps.get(row).cloned().flatten();
            // Not the room the wrap put before it.
            let lead = wrap.as_ref().map_or(0, |w| w.indent);
            let first = if row == from.0 { from.1 } else { 0 }.max(lead).min(chars.len());
            let last = if row == to.0 { to.1.min(chars.len()) } else { chars.len() };
            if row > from.0 {
                match wrap {
                    Some(w) => text.push_str(&w.spaces),
                    None => {
                        text.truncate(text.trim_end_matches([' ', '\t']).len());
                        text.push('\n');
                    }
                }
            }
            text.extend(&chars[first..last.max(first)]);
        }
        text.truncate(text.trim_end_matches([' ', '\t']).len());
        text
    }
}

impl AiPanel {
    pub fn new(auto_mode: bool) -> Self {
        Self {
            open: false,
            focus: false,
            input: crate::panel_input::Input::default(),
            entries: Vec::new(),
            busy: false,
            auto_mode,
            scroll: 0,
            history: crate::panel_input::Recall::default(),
            awaiting: None,
            editing: None,
            selection: None,
            laid: RefCell::new(PanelLaid::default()),
            input_laid: RefCell::new(crate::panel_input::Laid::default()),
            model_pick: None,
            live: None,
            models_asking: false,
            steps: Vec::new(),
            files: Vec::new(),
            reading: Vec::new(),
        }
    }

    /// What's selected in the conversation, if anything.
    pub fn selected_text(&self) -> Option<String> {
        let (a, b) = self.selection?;
        let laid = self.laid.borrow();
        Some(laid.text_between(a, b)).filter(|text| !text.trim().is_empty())
    }

    /// `provider`'s own list of models, where it's the one kept.
    pub fn live_for(&self, provider: crate::ai::provider::Provider) -> &[crate::ai::models::Listed] {
        self.live.as_ref().filter(|l| l.provider == provider).map_or(&[], |l| l.models.as_slice())
    }

    /// Puts `s` in the input in place of what's there, the caret at its
    /// end.
    pub fn set_input(&mut self, s: String) {
        self.input.set(&s);
    }

    /// Appends streamed assistant text, merging with the last text entry.
    pub fn push_text(&mut self, t: &str) {
        self.end_thinking();
        if let Some(Entry::Assistant(s)) = self.entries.last_mut() {
            s.push_str(t);
        } else {
            self.entries.push(Entry::Assistant(t.to_string()));
        }
    }

    pub fn push_thinking(&mut self, t: &str) {
        if let Some(Entry::Thinking { text, ended: None, .. }) = self.entries.last_mut() {
            text.push_str(t);
        } else {
            self.entries.push(Entry::Thinking { text: t.to_string(), started: Instant::now(), ended: None });
        }
    }

    pub fn end_thinking(&mut self) {
        for e in self.entries.iter_mut().rev() {
            if let Entry::Thinking { ended: ended @ None, .. } = e {
                *ended = Some(Instant::now());
            }
        }
    }

    pub fn card_mut(&mut self, index: usize) -> Option<&mut ToolCard> {
        match self.entries.get_mut(index) {
            Some(Entry::Tool(card)) => Some(card),
            _ => None,
        }
    }
}

pub struct KeyDialog {
    pub input: String,
    pub error: Option<String>,
    pub checking: bool,
    /// The MCP server whose bearer token it takes (`crate::mcp::client`),
    /// where it isn't the provider's API key.
    pub server: Option<String>,
}

impl KeyDialog {
    /// The box for the provider's API key.
    pub fn api_key() -> Self {
        KeyDialog { input: String::new(), error: None, checking: false, server: None }
    }

    /// The box for MCP server `server`'s token.
    pub fn server_token(server: &str) -> Self {
        KeyDialog { server: Some(server.to_string()), ..KeyDialog::api_key() }
    }

    /// Its title, and the line under the box: how it stands, or its keys.
    pub fn title_and_status(&self, p: Provider) -> (String, String) {
        let keys = |mac, other| t(keys(mac, other)).to_string();
        match &self.server {
            Some(server) => (
                tf("key_dialog.server_title", &[("server", server)]),
                self.error.clone().unwrap_or_else(|| keys("key_dialog.server_keys_mac", "key_dialog.server_keys")),
            ),
            None if self.checking => (tf("key_dialog.title", &[("name", &p.name())]), tf("key_dialog.checking", &[("company", &p.company())])),
            None => (tf("key_dialog.title", &[("name", &p.name())]), self.error.clone().unwrap_or_else(|| keys("key_dialog.keys_mac", "key_dialog.keys"))),
        }
    }
}

/// An update the user started has put a new Flow in place of this one
/// (`UpdateState::Installed`, the apt tab's or install.sh's), and About
/// isn't open to say so: whether to restart into it now.
pub struct RestartAsk<'a> {
    /// The new Flow, as "0.1.37.3 (build 540)".
    pub version: &'a str,
    /// A program still runs in some tab, which restarting ends.
    pub busy: bool,
}

/// An outside app waits for the user's yes in a card ([`crate::mcp`]): to use
/// Flow at all, the first time it calls, or to run a command.
pub struct McpCard {
    /// The app as its MCP client names it.
    pub app: String,
    /// The program that started `flow-mcp`, as the system says; empty where
    /// it didn't.
    pub host: String,
    pub ask: McpAsk,
}

pub enum McpAsk {
    /// What it may do, each kind ([`crate::mcp::tools::Kind::ALL`]) as the
    /// user leaves its switch; commands, switched on, ask each time.
    Use { kinds: [bool; 4] },
    /// A command to run in its own pane, and how risky it is.
    Command { command: String, risk: crate::ai::tools::Risk },
}

/// An MCP server asks the user something (`crate::mcp::client::elicit`), a
/// call needs a link opened first, or a prompt its arguments: a card over
/// everything, naming the server.
pub struct ServerAsk {
    pub server: String,
    /// For a prompt's arguments, the prompt's name.
    pub prompt: Option<String>,
    pub message: String,
    pub body: AskBody,
    /// What has the keys: a field by its place, then Send and Decline.
    pub focus: usize,
    /// Each field's problem, once Send found one.
    pub problems: Vec<Option<String>>,
}

pub enum AskBody {
    /// A form: its fields, each one's answer so far, its text as it's
    /// typed (for the fields typed in), and the option each choice is at.
    Form { form: crate::mcp::client::elicit::Form, answers: Vec<crate::mcp::client::elicit::Answer>, texts: Vec<crate::settings::TextField>, picks: Vec<usize> },
    /// A link to open in the browser.
    Link(crate::mcp::client::elicit::LinkAsk),
}

impl ServerAsk {
    pub fn fields(&self) -> usize {
        match &self.body {
            AskBody::Form { form, .. } => form.fields.len(),
            AskBody::Link(_) => 0,
        }
    }

    /// What takes the keys in turn: each field, then Send and Decline.
    pub fn items(&self) -> usize {
        self.fields() + 2
    }

    pub fn title(&self) -> String {
        match (&self.body, &self.prompt) {
            (_, Some(prompt)) => tf("server_ask.prompt_title", &[("server", &self.server), ("prompt", prompt)]),
            (AskBody::Link(_), None) => tf("server_ask.link_title", &[("server", &self.server)]),
            (AskBody::Form { .. }, None) => tf("server_ask.title", &[("server", &self.server)]),
        }
    }

    /// Its Send's words (Open for a link, Run for a prompt) and Decline's.
    pub fn button_words(&self) -> (&'static str, &'static str) {
        match (&self.body, &self.prompt) {
            (_, Some(_)) => (t("server_ask.run_prompt"), t("server_ask.cancel")),
            (AskBody::Link(_), None) => (t("server_ask.open"), t("server_ask.decline")),
            (AskBody::Form { .. }, None) => (t("server_ask.send"), t("server_ask.decline")),
        }
    }
}

/// A program asks to show a 3D model from a file (`crate::model::Ask`),
/// which waits for the user's yes: Flow would read a file the program chose.
pub struct ModelAsk {
    /// The tab it's in, as its title bar names it, and its session.
    pub tab: String,
    pub session: crate::event::SessionId,
    pub ask: crate::model::Ask,
}

/// The command history's search, over the terminal while it's open.
pub struct HistoryPanel {
    pub query: String,
    pub filter: Filter,
    pub found: Vec<Found>,
    pub selected: usize,
    /// The first command shown, so the list moves only as far as the
    /// selection takes it.
    pub top: Cell<usize>,
    /// The folder and the shell of the tab in front, which two filters mean.
    pub cwd: Option<String>,
    pub shell: &'static str,
    pub shell_label: String,
    /// Forgetting everything shown was asked once, then: once more forgets.
    pub forget_all_at: Option<Instant>,
    /// Settings turned the history off.
    pub off: bool,
}

impl HistoryPanel {
    pub fn selected(&self) -> Option<&Found> {
        self.found.get(self.selected)
    }
}

/// What an input method is still composing, such as Japanese before it's
/// converted: drawn where it will go, until it's committed as typed text.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Preedit {
    pub text: String,
    /// What the input method marks in `text`, as byte offsets: the part
    /// being converted, or its caret when both ends are the same.
    pub cursor: Option<(usize, usize)>,
}

impl Preedit {
    /// Each character, and whether it's in the part being converted.
    pub fn marked(&self) -> Vec<(char, bool)> {
        let (a, b) = self.cursor.unwrap_or((0, 0));
        self.text.char_indices().map(|(i, ch)| (ch, (a..b).contains(&i))).collect()
    }

    /// How many characters come before the input method's caret. None while
    /// a part is being converted, which shows no caret.
    pub fn caret(&self) -> Option<usize> {
        match self.cursor {
            Some((a, b)) if a != b => None,
            Some((_, b)) => Some(self.text.char_indices().take_while(|&(i, _)| i < b).count()),
            None => Some(self.text.chars().count()),
        }
    }
}

pub struct SettingsView<'a> {
    pub menu: &'a SettingsMenu,
    pub entries: &'a [settings::Entry],
    /// The open tabs' names, for the row under the title, and the one it's
    /// on (none: on new tabs').
    pub tabs: &'a [String],
    pub on: Option<usize>,
}

pub struct AboutView {
    /// Seconds since it opened (drives the entrance).
    pub age: f32,
    pub can_update: bool,
    /// The rolling cube's picture (`about_cube.rs`), [`ABOUT_CUBE`] big.
    pub cube: Option<std::sync::Arc<crate::graphics::Image>>,
    /// Where the updater Flow shows stands (Linux's), for a line and the
    /// button; None where the updater has its own windows.
    pub update: Option<UpdateState>,
    /// The command to install a release by hand was just copied, which
    /// its button says for a moment.
    pub copied: bool,
    /// The window system, after the version, where there's a choice of
    /// them (`Platform::windowing`: Linux's Wayland or X11).
    pub windowing: Option<&'static str>,
    /// Its logo, drawn before its name (`logos::windowing`).
    pub windowing_logo: Option<std::sync::Arc<crate::graphics::Image>>,
}

/// About's version line: Flow's version, and the window system after it
/// where there's a choice of them, so it's plain which a window is on.
pub fn about_version(windowing: Option<&str>) -> String {
    let version = crate::version::display();
    match windowing {
        Some(windowing) => tf("about.version_on", &[("version", &version), ("windowing", &windowing)]),
        None => tf("about.version", &[("version", &version)]),
    }
}

/// The About card's cube's picture, in points.
pub const ABOUT_CUBE: (f32, f32) = (320.0, 200.0);

/// Everything the frame needs that isn't owned by the renderer.
/// What a tab uses, for the card under it and a row of About's Resources.
#[derive(Debug, Clone, PartialEq)]
pub struct TabUse {
    pub label: String,
    /// Its programs, once read twice; none until then, or where the OS
    /// can't say.
    pub programs: Option<crate::resources::Programs>,
    /// Its shell runs in WSL, whose programs Windows can't see.
    pub wsl: bool,
    /// What Flow holds for it: its rows' cells, its pictures decoded, and
    /// what they hold on the GPU.
    pub scrollback: u64,
    pub pictures: u64,
    pub gpu: u64,
}

/// About's Resources: every tab, then Flow's own process and the window's
/// GPU memory and frames a second.
#[derive(Debug, Clone, PartialEq)]
pub struct ResourcesView {
    pub tabs: Vec<TabUse>,
    pub flow: Option<crate::resources::Programs>,
    pub window_gpu: u64,
    pub fps: u32,
    /// The last second's frames' mean time on the GPU, in milliseconds,
    /// once one is read; and whether the GPU can say at all.
    pub gpu_ms: Option<f32>,
    pub gpu_timed: bool,
}

/// The new-tab picker: the shell and the theme chosen, and whether the
/// arrows move in the themes' list rather than the shells'.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TabPicker {
    pub shell: usize,
    pub theme: usize,
    pub in_themes: bool,
    /// The first shell and the first theme shown, where the window has no
    /// room for a whole list (the themes saved grow it): the drawing keeps
    /// each choice in view, so a list moves only as far as its choice takes
    /// it.
    pub tops: Cell<[usize; 2]>,
}

/// The first of a list's `len` rows to show `fit` of from, `top` before,
/// so that row `chosen` is among them, moving no further than that takes.
fn keep_in_view(top: usize, chosen: usize, fit: usize, len: usize) -> usize {
    let fit = fit.max(1);
    top.min(chosen).max((chosen + 1).saturating_sub(fit)).min(len.saturating_sub(fit))
}

/// A terminal tab as the title bar shows it.
pub struct TabLabel {
    pub label: String,
    /// A long command finished there while another tab was in front, and
    /// whether it failed.
    pub alert: Option<bool>,
    /// The group it's in, by its id, and the group's color, which marks
    /// its pill (`tab_groups`).
    pub group: Option<(u64, [u8; 3])>,
}

/// The tab sidebar as it's drawn (`tab_sidebar`): its rows, the one the
/// keys are on, the first shown, how far out it is, a row's menu, and a
/// name being typed in a row.
#[derive(Debug, Clone)]
pub struct SidebarView {
    pub rows: Vec<SidebarRow>,
    pub chosen: Option<usize>,
    pub top: usize,
    /// 0 in, 1 all the way out.
    pub out: f32,
    pub menu: Option<SidebarMenu>,
    pub naming: Option<(usize, crate::settings::TextField)>,
    /// The row of a group whose Close Group was pressed once, which a
    /// second press closes.
    pub closing: Option<usize>,
    /// How many tabs are picked together, where more than one is: New
    /// Group at the foot says so.
    pub picked: usize,
    /// How wide it is, in pixels (`window.tab_sidebar_width`).
    pub width: f32,
    /// Pinned beside the terminal, which then has the pointer outside it,
    /// and whether it has the keys (popped out, it always has).
    pub pinned: bool,
    pub keys: bool,
    /// A row being dragged, and where it would land.
    pub drag: Option<SidebarDrag>,
}

/// A row dragged in the sidebar (`tab_sidebar::drop_at`).
#[derive(Debug, Clone, PartialEq)]
pub struct SidebarDrag {
    /// Its place among the rows.
    pub row: Option<usize>,
    /// The group's header it would go into, by its place.
    pub onto: Option<usize>,
    /// The gap it would go into: above the row at this place (the rows'
    /// count, after the last), set in and in its color where it would join
    /// a group there.
    pub gap: Option<(usize, Option<[u8; 3]>)>,
}

/// A tab or a group being dragged, as the title bar and the pointer show
/// it (`tab_sidebar::Drop`).
#[derive(Debug, Clone, PartialEq)]
pub struct TabDrag {
    /// The tab dragged, by its place in the bar, whose pill is dimmed;
    /// none for a group.
    pub tab: Option<usize>,
    /// The title bar's gap it would land in: before the tab at this place
    /// (the tabs' count, after the last), and the color of the group it
    /// would join there.
    pub bar: Option<(usize, Option<[u8; 3]>)>,
    /// Its name, beside the pointer.
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct SidebarRow {
    pub level: usize,
    pub kind: SidebarKind,
    /// A tab picked with others (⌘-click, Shift-click).
    pub picked: bool,
}

#[derive(Debug, Clone)]
pub enum SidebarKind {
    Group { name: String, color: [u8; 3], count: usize, folded: bool },
    /// A tab, by its place in the title bar, its group's color, and its
    /// panes folded open or shut where it's split.
    Tab { tab: usize, name: String, line: crate::tab_sidebar::Line, front: bool, alert: Option<bool>, split: Option<bool>, color: Option<[u8; 3]> },
    Pane { name: String, line: crate::tab_sidebar::Line, front: bool },
}

/// A row's menu: its items' words, a color's swatch, and the one chosen.
#[derive(Debug, Clone)]
pub struct SidebarMenu {
    pub row: usize,
    pub items: Vec<(String, Option<[u8; 3]>)>,
    pub chosen: usize,
}

pub struct View<'a> {
    pub layout: &'a Layout,
    pub tabs: &'a [TabLabel],
    pub active_tab: usize,
    /// The shells and the themes a new tab can have, and the picker's choice
    /// while it's open.
    pub shells: &'a [ShellProfile],
    pub themes: &'a [String],
    pub tab_picker: Option<&'a TabPicker>,
    /// A tab's name being typed in its pill: the tab's place, and the field.
    pub renaming: Option<(usize, &'a crate::settings::TextField)>,
    /// The tab sidebar, while it's out.
    pub sidebar: Option<SidebarView>,
    /// A tab's menu opened from its pill (a right click), while the
    /// sidebar is in: the tab's place, and the menu.
    pub pill_menu: Option<(usize, SidebarMenu)>,
    /// A tab or a group being dragged, by its pill, its row or its header.
    pub tab_drag: Option<TabDrag>,
    /// Efficient mode is on, which its pill in the title bar shows.
    pub efficient: bool,
    /// It's on only because the computer runs on battery or saves power
    /// (`ui.efficient_on_battery`), which the pill says too.
    pub efficient_for: Option<crate::power::Power>,
    /// The window is kept on top of every other, which the title bar's
    /// on-top pill shows; none where it can't be (Wayland), and no pill.
    pub on_top: Option<bool>,
    /// The minutes left to work on its own, while the assistant may.
    pub own_left: Option<u64>,
    /// Flow runs as administrator or root.
    pub elevated: bool,
    /// How long the terminal has been recorded, while it is.
    pub recording: Option<std::time::Duration>,
    /// The session of the pane in front of the tab in front.
    pub session: Option<&'a Session>,
    /// The tab in front's panes, each where it's laid out; one where it
    /// isn't split.
    pub panes: Vec<PaneView<'a>>,
    /// The lines between the tab in front's panes (`Split::dividers`): each
    /// its gap, and whether it stands upright.
    pub dividers: Vec<(Rect, bool)>,
    /// The font in use (families, size in pixels, line height), which Flow's
    /// own words are drawn in, back to after a pane in another.
    pub font: (Vec<String>, f32, f32),
    pub ai: &'a AiPanel,
    pub key_dialog: Option<&'a KeyDialog>,
    pub has_key: bool,
    pub model: &'a str,
    /// Who answers the assistant's requests.
    pub provider: Provider,
    pub hover: (f32, f32),
    pub time: f32,
    pub cursor_pos: (f32, f32),
    /// The cursor is in the lit half of its blink (always, when it doesn't blink).
    pub cursor_lit: bool,
    pub toast: Option<&'a str>,
    /// Caps Lock's notice shows: a key was just typed into a pane with it on.
    pub caps_lock: bool,
    pub window_buttons: bool,
    pub maximized: bool,
    /// Flow's window has the keys; a window frame without them is ghosted.
    pub focused: bool,
    /// The system lets Flow send its window behind the others, which a
    /// window frame's depth gadget does (`Platform::sends_to_back`).
    pub sends_to_back: bool,
    /// A window frame's sizing gadget drags the window's corner just now
    /// (`frame_sizes`).
    pub sizes: bool,
    pub ripple: bool,
    /// The Glow setting: how strong glows are (1 is normal).
    pub glow: f32,
    pub settings: Option<SettingsView<'a>>,
    pub tokens_used: u64,
    /// 0 = no cap.
    pub token_cap: u64,
    pub about: Option<AboutView>,
    /// The card under the tab the pointer rests on, by its place.
    pub tab_card: Option<(usize, TabUse)>,
    /// About's Resources, over About.
    pub resources: Option<ResourcesView>,
    pub history: Option<&'a HistoryPanel>,
    /// The find bar over the terminal, and what it found in the tab.
    pub find: Option<&'a crate::find::Find>,
    /// The command palette, over everything.
    pub palette: Option<&'a crate::command_palette::Palette>,
    /// The exit code of the last command that failed in the tab in front,
    /// while its shell waits at its prompt again: Ask why is offered.
    pub ask_why: Option<i32>,
    /// The first file a program asked to show a model from, if any wait.
    pub model_ask: Option<&'a ModelAsk>,
    /// The first outside app's card, if any wait.
    pub mcp_card: Option<&'a McpCard>,
    /// The first of what MCP servers ask the user, if any wait.
    pub server_ask: Option<&'a ServerAsk>,
    /// Where the updater Flow shows stands, for the title bar's sign.
    pub update: Option<UpdateState>,
    /// Whether to check for updates automatically is being asked.
    pub update_ask: bool,
    /// Whether to restart into the Flow an update put in place is asked.
    pub restart_ask: Option<RestartAsk<'a>>,
    /// What the input method is composing, shown where typed text goes.
    pub preedit: Option<&'a Preedit>,
    /// A file dragged over the window: where it would land, lit, and what
    /// dropping it there does, in words.
    pub drop: Option<(Rect, String)>,
    /// How strongly the glass flashes for a bell (`sound.bell_flash`), 0..1.
    pub bell_flash: f32,
    /// A page of help over whatever it's about.
    pub help: Option<&'a crate::help::Help>,
    /// The link under the pointer (`crate::link`): its cells are underlined
    /// and where it goes is shown at the terminal's foot.
    pub link: Option<&'a crate::link::Link>,
}

/// A pane of the tab in front, as a frame draws it.
pub struct PaneView<'a> {
    pub session: &'a Session,
    /// Who it works for, where it isn't the user: a label at its top right.
    pub by: Option<PaneFor>,
    /// The window's layout with the pane's rectangle as its terminal, and
    /// its own font's cell.
    pub layout: Layout,
    /// Its theme's colors: the window's for the pane in front.
    pub ink: palette::Ink,
    /// Its theme's font (families, size in pixels, line height), where it
    /// isn't the one in front's.
    pub font: Option<(Vec<String>, f32, f32)>,
    /// It has the keys: its cursor glides and blinks, the find bar searched
    /// it, and the link under the pointer is its.
    pub focused: bool,
    /// Its place in its tab, which its pill's and scrollbar's hits name.
    pub index: usize,
    /// How much its scrollbar shows (0 none, 1 full; `scrollbar::Light`),
    /// and whether its thumb is held.
    pub scrollbar: f32,
    pub scrollbar_held: bool,
    /// The line Previous or Next Command went to (from the scrollback's
    /// top), and how lit it still is (1 at first, fading to 0).
    pub jumped: Option<(usize, f32)>,
}

/// What drawing a frame hands back to the app.
pub struct Drawn {
    /// What the pointer can hit; later entries sit on top.
    pub hits: Vec<(Rect, Hit)>,
    /// The caret where typed text goes: the terminal's cursor, the
    /// assistant's input, a preset's name or a Settings text row being
    /// typed in (your own theme's start screen or prompt). The input method
    /// opens its candidates beside it, and works only while there is one. None while
    /// keys work something else: Settings' rows, the shell picker, About, and
    /// the key dialog, whose key is plain letters an input method would only
    /// get in the way of.
    pub caret: Option<Rect>,
    /// The tab sidebar's first row shown, as the drawing kept the chosen
    /// one in view.
    pub sidebar_top: usize,
    /// Each tab's pill in the title bar, in their order, kept apart from
    /// the hits, which a popped-out sidebar clears, for where a tab dragged
    /// would land.
    pub pills: Vec<Rect>,
}

pub fn draw(c: &mut Canvas<'_>, v: &View<'_>) -> Drawn {
    c.glow_level = v.glow;
    let mut hits = Vec::new();
    let mut caret = None;
    // The cursor of the pane in front, which Caps Lock's notice keeps clear of.
    let mut cursor = None;
    let mut naming = None;
    let plus = draw_title(c, v, &mut hits, &mut naming);
    let pills: Vec<Rect> = (0..v.tabs.len()).map_while(|i| hits.iter().find(|(_, h)| *h == Hit::Tab(i)).map(|&(r, _)| r)).collect();
    if let Some(color) = palette::ink().border {
        draw_border(c, v, color);
    }
    if v.session.is_some() {
        if v.panes.len() > 1 {
            draw_dividers(c, v, &mut hits);
        }
        let mut switched = false;
        for pane in &v.panes {
            if let Some(font) = &pane.font {
                c.use_font(font);
                switched = true;
            } else if switched {
                c.use_font(&v.font);
                switched = false;
            }
            if let Some(frame) = pane.layout.frame {
                draw_frame(c, v, pane, frame);
                hits.push((frame.close_box(), Hit::FrameClose(pane.index)));
                if let Some((front, back)) = frame.gadgets() {
                    hits.push((front, Hit::FrameFront));
                    hits.push((back, Hit::FrameBack));
                }
                hits.push((frame.size_box(), Hit::FrameSize));
            }
            let at = draw_terminal(c, v, pane, &mut hits);
            if pane.focused {
                caret = at;
            }
        }
        if switched {
            c.use_font(&v.font);
        }
        cursor = caret;
        if let Some(link) = v.link {
            draw_link_target(c, v, link.uri());
        }
        if let Some(exit) = v.ask_why {
            draw_ask_why(c, v, exit, &mut hits);
        }
        if let Some(find) = v.find {
            let typed = draw_find(c, v, find, &mut hits);
            if find.focus && !v.ai.focus {
                caret = Some(typed);
            }
        }
    }
    if v.bell_flash > 0.0 {
        draw_bell_flash(c, v);
    }
    if let Some(panel) = v.layout.panel {
        caret = draw_panel(c, v, panel, &mut hits).or(caret);
    }
    // A tab's name being typed has the keys, whatever else has a caret.
    if naming.is_some() {
        caret = naming;
    }
    if let Some((r, words)) = &v.drop {
        draw_drop(c, v, *r, words);
    }
    if v.caps_lock && v.session.is_some() && v.settings.is_none() {
        draw_caps_lock(c, v, cursor);
    }
    // The tab sidebar over the terminal, taking the pointer and the keys as
    // the palette does: a click outside it closes it.
    let mut sidebar_top = 0;
    let mut beside = None;
    if let Some(sb) = &v.sidebar {
        // Pinned, it's beside the terminal, which keeps the pointer.
        if !sb.pinned {
            hits.clear();
        }
        let (at, top, card) = draw_sidebar(c, v, sb, &mut hits);
        (sidebar_top, beside) = (top, card);
        if at.is_some() || !sb.pinned {
            caret = at;
        }
    }
    if let Some(t) = v.toast.filter(|_| v.settings.is_none()) {
        draw_toast(c, v, t);
    }
    if let Some((i, u)) = v.tab_card.as_ref().filter(|_| v.tab_picker.is_none()) {
        // Under its pill, or beside its row in the sidebar.
        let row = v.sidebar.as_ref().and_then(|sb| sb.rows.iter().position(|r| matches!(r.kind, SidebarKind::Tab { tab, .. } if tab == *i)));
        let at = row.and_then(|row| hits.iter().find(|(_, h)| *h == Hit::SidebarRow(row))).map(|&(r, _)| (r, true));
        if let Some((r, in_sidebar)) = at.or_else(|| hits.iter().find(|(_, h)| *h == Hit::Tab(*i)).map(|&(r, _)| (r, false))) {
            match (in_sidebar, beside) {
                // Beside the card, level with the row.
                (true, Some(right)) => draw_tab_card(c, v, Rect::new(right + 8.0 * v.layout.scale, r.y - 6.0 * v.layout.scale, 0.0, 0.0), u),
                _ => draw_tab_card(c, v, r, u),
            }
        }
    }
    if let Some(picker) = v.tab_picker {
        draw_tab_picker(c, v, plus, picker, &mut hits);
        caret = None;
    }
    // A tab's menu, under its pill.
    if let Some((i, menu)) = &v.pill_menu {
        if let Some(&(pill, _)) = hits.iter().find(|(_, h)| *h == Hit::Tab(*i)) {
            draw_row_menu(c, v, menu, pill, true, &mut hits);
        }
    }
    // The name of a tab or a group being dragged, beside the pointer.
    if let Some(d) = &v.tab_drag {
        draw_drag_name(c, v, &d.name);
    }
    if let Some(panel) = v.history {
        hits.clear();
        caret = Some(draw_history(c, v, panel, &mut hits));
    }
    if let Some(palette) = v.palette {
        hits.clear();
        caret = Some(draw_palette(c, v, palette, &mut hits));
    }
    if let Some(sv) = &v.settings {
        hits.clear();
        caret = draw_settings(c, v, sv, &mut hits);
        // Over Settings, whose rows say things in toasts too (Your sizes'
        // note of what it left out, the Fit rows'), which its card would
        // otherwise hide.
        if let Some(t) = v.toast {
            draw_toast(c, v, t);
        }
    }
    if let Some(about) = &v.about {
        hits.clear();
        draw_about(c, v, about, &mut hits);
        caret = None;
    }
    if let Some(r) = &v.resources {
        hits.clear();
        draw_resources(c, v, r, &mut hits);
    }
    if let Some(dialog) = v.key_dialog {
        hits.clear();
        draw_key_dialog(c, v, dialog);
        caret = None;
    }
    if let Some(ask) = v.model_ask {
        hits.clear();
        draw_model_ask(c, v, ask, &mut hits);
        caret = None;
    }
    if let Some(card) = v.mcp_card.filter(|_| v.model_ask.is_none() && v.key_dialog.is_none()) {
        hits.clear();
        draw_mcp_card(c, v, card, &mut hits);
        caret = None;
    }
    if let Some(ask) = v.server_ask.filter(|_| v.model_ask.is_none() && v.key_dialog.is_none() && v.mcp_card.is_none()) {
        hits.clear();
        caret = draw_server_ask(c, v, ask, &mut hits);
    }
    if v.update_ask && v.model_ask.is_none() && v.key_dialog.is_none() {
        hits.clear();
        draw_update_ask(c, v, &mut hits);
        caret = None;
    }
    if let Some(ask) = v.restart_ask.as_ref().filter(|_| !v.update_ask && v.model_ask.is_none() && v.key_dialog.is_none()) {
        hits.clear();
        draw_restart_ask(c, v, ask, &mut hits);
        caret = None;
    }
    if let Some(help) = v.help {
        hits.clear();
        draw_help(c, v, help, &mut hits);
        caret = None;
    }
    Drawn { hits, caret, sidebar_top, pills }
}

/// A page of help over the screen it's about, as wide as the command
/// history's card: its title (with ← for going back once a link was
/// followed), its text laid out as the assistant's markdown is and scrolled
/// to `help.top`, its links to other pages in the accent color, and the keys
/// at its foot.
fn draw_help(c: &mut Canvas<'_>, v: &View<'_>, help: &crate::help::Help, hits: &mut Vec<(Rect, Hit)>) {
    let s = v.layout.scale;
    let (cw, ch) = (c.cell.width, c.cell.height);
    c.rect(0.0, 0.0, v.layout.width, v.layout.height, [0.0, 0.0, 0.0, 0.35], 0.0);
    let pad = 22.0 * s;
    let w = (820.0 * s).min(v.layout.width - 32.0 * s);
    let cols = (((w - pad * 2.0) / cw) as usize).max(10);
    let lines = help.page.laid_out(cols).lines;
    let hint: Vec<_> = text::wrap_plain(&crate::help::with_keys("help.footer"), cols, Style::Normal).into_iter().take(2).collect();
    let footer_h = ch * (hint.len() as f32 + 0.8);
    let head_h = ch * 2.2;
    let h = (v.layout.height - 48.0 * s).min(pad * 2.0 + head_h + lines.len() as f32 * ch + footer_h);
    let (x, y) = ((v.layout.width - w) * 0.5, (v.layout.height - h) * 0.5);
    c.glow(x, y, w, h, rgba(ACCENT, 0.2), 18.0 * s, 30.0 * s);
    c.glass(x, y, w, h, rgba(PANEL_TINT, 1.0), 18.0 * s);
    hits.push((Rect::new(x, y, w, h), Hit::HelpCard));

    // A button in the head: ← back, ✕ close.
    let button = |c: &mut Canvas<'_>, at: Rect, mark: &str, color: Rgba| {
        if at.contains(v.hover) {
            c.rect(at.x, at.y, at.w, at.h, rgba([255, 255, 255], 0.1), 6.0 * s);
        }
        c.text(at.x + (at.w - c.text_width(mark)) * 0.5, at.y + (at.h - ch) * 0.5, mark, color, false);
    };
    let tx = if help.can_go_back() {
        let back = Rect::new(x + pad - ch * 0.3, y + pad - ch * 0.2, ch * 1.4, ch * 1.4);
        button(c, back, "←", rgba(ACCENT, 1.0));
        hits.push((back, Hit::HelpBack));
        back.x + back.w + ch * 0.2
    } else {
        c.text(x + pad, y + pad, "? ", rgba(ACCENT, 1.0), true)
    };
    c.text(tx, y + pad, help.page.title(), rgba(FOREGROUND, 1.0), true);
    let close = Rect::new(x + w - pad - ch * 1.4, y + pad - ch * 0.2, ch * 1.4, ch * 1.4);
    button(c, close, "✕", rgba(FOREGROUND, 0.8));
    hits.push((close, Hit::HelpClose));

    // The text, kept from scrolling past its end, and scrolled just far
    // enough to show a link Tab chose.
    let ty = y + pad + head_h;
    let shown = (((y + h - pad - footer_h - ty) / ch) as usize).max(1);
    let mut top = help.top.get();
    if help.reveal.take() {
        if let Some((first, last)) = help.chosen.get().and_then(|i| text::lines_of(&lines, i)) {
            top = if first < top { first } else { top.max((last + 1).saturating_sub(shown)).min(first) };
        }
    }
    let top = top.min(lines.len().saturating_sub(shown));
    help.top.set(top);

    // Where the links shown are, measured first so that the one under the
    // pointer lights up whole, on each line it wraps onto.
    let mut links: Vec<(usize, Rect)> = Vec::new();
    for (k, line) in lines.iter().skip(top).take(shown).enumerate() {
        let mut lx = x + pad;
        for (span, style) in line {
            let width = c.text_width(span);
            if let Style::Link(i) = *style {
                let lead = c.text_width(&span[..span.len() - span.trim_start().len()]);
                let trail = c.text_width(&span[span.trim_end().len()..]);
                links.push((i, Rect::new(lx + lead, ty + k as f32 * ch, width - lead - trail, ch)));
            }
            lx += width;
        }
    }
    help.shown.set(links.first().zip(links.last()).map(|(first, last)| (first.0, last.0)));
    if help.chosen.get().is_some_and(|i| !links.iter().any(|&(link, _)| link == i)) {
        help.chosen.set(None);
    }
    let hovered = links.iter().find(|(_, r)| r.contains(v.hover)).map(|&(i, _)| i);
    for &(i, r) in &links {
        let chosen = help.chosen.get() == Some(i);
        if chosen {
            c.rect(r.x - 3.0 * s, r.y, r.w + 6.0 * s, r.h, rgba(ACCENT, 0.18), 4.0 * s);
        }
        if chosen || hovered == Some(i) {
            c.rect(r.x, r.y + c.cell.baseline + 2.0, r.w, 1.0_f32.max(s), rgba(ACCENT, 0.9), 0.0);
        }
        hits.push((r, Hit::HelpLink(i)));
    }
    let fg = rgba(FOREGROUND, 0.92);
    for (k, line) in lines.iter().skip(top).take(shown).enumerate() {
        let mut lx = x + pad;
        for (span, style) in line {
            let (color, bold) = style_color(*style, fg);
            lx = c.text(lx, ty + k as f32 * ch, span, color, bold);
        }
    }
    let more = |c: &mut Canvas<'_>, key: &str, at: f32| c.text(x + w - pad - c.text_width(t(key)), at, t(key), rgba(DIM, 0.8), false);
    if top > 0 {
        more(c, "settings.more_up", ty - ch * 1.1);
    }
    if top + shown < lines.len() {
        more(c, "settings.more_down", y + h - pad - footer_h - ch * 0.4);
    }

    let mut fy = y + h - pad - ch * hint.len() as f32;
    c.rect(x + pad, fy - ch * 0.5, w - pad * 2.0, 1.0, rgba([255, 255, 255], 0.08), 0.0);
    for line in &hint {
        c.text(x + pad, fy, &text::line_text(line), rgba(DIM, 0.75), false);
        fy += ch;
    }
}

/// The bell, seen: a wash over the terminal in the text's own color, light
/// on dark glass and dark on light, fading as it goes.
fn draw_bell_flash(c: &mut Canvas<'_>, v: &View<'_>) {
    let l = v.layout;
    let gap = l.cell_w * 0.5;
    let fill = rgba(palette::ink().text, 0.22 * v.bell_flash.min(1.0));
    c.rect(l.term.x - gap, l.term.y - gap * 0.5, l.cols as f32 * l.cell_w + gap * 2.0, l.rows as f32 * l.cell_h + gap, fill, 6.0 * l.scale);
}

/// The margin round the text in a color of its own, from the title bar down
/// to the window's (rounded) foot, leaving the text's area and a little
/// round it in the background.
fn draw_border(c: &mut Canvas<'_>, v: &View<'_>, color: [u8; 3]) {
    let l = v.layout;
    let gap = l.cell_w * 0.5;
    let inner = Rect::new(l.term.x - gap, l.term.y - gap * 0.5, l.cols as f32 * l.cell_w + gap * 2.0, l.rows as f32 * l.cell_h + gap);
    let (top, foot) = (l.title.h, inner.y + inner.h);
    let fill = rgba(color, 1.0);
    let round = 10.0 * l.scale;
    c.rect(0.0, top, l.width, inner.y - top, fill, 0.0);
    c.rect(0.0, top, inner.x, l.height - round - top, fill, 0.0);
    c.rect(inner.x + inner.w, top, l.width - inner.x - inner.w, l.height - round - top, fill, 0.0);
    c.rect(0.0, foot, l.width, l.height - foot, fill, round.min((l.height - foot) * 0.5));
}

/// The title bar's colors for the glass behind it: Flow's own on dark glass;
/// on light glass (a workstation's white, a teletype's paper), dark text (the
/// theme's own), darker accents and dark washes, so it reads there too.
#[derive(Debug, Clone, Copy)]
struct Chrome {
    fg: [u8; 3],
    wash: [u8; 3],
    accent: [u8; 3],
    accent2: [u8; 3],
    good: [u8; 3],
    warn: [u8; 3],
}

impl Chrome {
    /// `color` (one of Flow's own) as this chrome shows it.
    fn map(&self, color: [u8; 3]) -> [u8; 3] {
        match color {
            ACCENT => self.accent,
            ACCENT2 => self.accent2,
            GOOD => self.good,
            WARN => self.warn,
            c => c,
        }
    }
}

/// How light a color looks, 0 to 1.
fn lightness([r, g, b]: [u8; 3]) -> f32 {
    (0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32) / 255.0
}

fn chrome() -> Chrome {
    let ink = palette::ink();
    if lightness(ink.background) > 0.5 {
        let fg = if lightness(ink.text) < 0.5 { ink.text } else { [0x1A, 0x1A, 0x1A] };
        Chrome { fg, wash: [0, 0, 0], accent: [0x0B, 0x6E, 0x8A], accent2: [0x6A, 0x3F, 0xC4], good: [0x1E, 0x7A, 0x3E], warn: [0x9A, 0x5B, 0x00] }
    } else {
        Chrome { fg: FOREGROUND, wash: [255, 255, 255], accent: ACCENT, accent2: ACCENT2, good: GOOD, warn: WARN }
    }
}

fn pill(c: &mut Canvas<'_>, v: &View<'_>, x: f32, label: &str, active: bool, color: [u8; 3]) -> Rect {
    let s = v.layout.scale;
    let k = chrome();
    let color = k.map(color);
    let h = 24.0 * s;
    let y = (v.layout.title.h - h) * 0.5;
    let w = c.text_width(label) + 20.0 * s;
    let r = Rect::new(x, y, w, h);
    let hovered = r.contains(v.hover);
    if active {
        c.glow(r.x, r.y, r.w, r.h, rgba(color, 0.18), h * 0.5, 10.0 * s);
        c.rect(r.x, r.y, r.w, r.h, rgba(color, 0.20), h * 0.5);
        c.outline(r.x, r.y, r.w, r.h, rgba(color, 0.55), h * 0.5, 1.0);
    } else {
        c.rect(r.x, r.y, r.w, r.h, rgba(k.wash, if hovered { 0.10 } else { 0.04 }), h * 0.5);
    }
    let ty = y + (h - c.cell.height) * 0.5;
    let fg = if active { rgba(color, 1.0) } else { rgba(k.fg, if hovered { 0.95 } else { 0.7 }) };
    c.text(x + 10.0 * s, ty, label, fg, active);
    r
}

/// The title bar; returns where its + is, for the new-tab picker, and puts
/// in `naming` where the caret is while a tab's name is being typed in its
/// pill.
fn draw_title(c: &mut Canvas<'_>, v: &View<'_>, hits: &mut Vec<(Rect, Hit)>, naming: &mut Option<Rect>) -> Rect {
    let s = v.layout.scale;
    let k = chrome();
    let t = v.layout.title;
    let ty = (t.h - c.cell.height) * 0.5;
    // Leave room for macOS traffic lights.
    let mut x = if cfg!(target_os = "macos") { 80.0 * s } else { 16.0 * s };

    let pulse = 0.6 + 0.4 * (v.time * 1.6).sin();
    let dot = 8.0 * s;
    let dy = (t.h - dot) * 0.5;
    let logo_x = x;
    c.glow(x, dy, dot, dot, rgba(k.accent, 0.5 * pulse), dot, 8.0 * s);
    c.rect(x, dy, dot, dot, rgba(k.accent, 0.95), dot);
    x += dot + 8.0 * s;
    x = c.text(x, ty, APP_NAME, rgba(k.fg, 0.95), true);
    // The logo and name open About.
    let name = Rect::new(logo_x - 6.0 * s, 4.0 * s, x - logo_x + 12.0 * s, t.h - 8.0 * s);
    if name.contains(v.hover) {
        c.rect(name.x, name.y, name.w, name.h, rgba(k.wash, 0.06), 8.0 * s);
    }
    hits.push((name, Hit::About));
    x += 14.0 * s;
    // The tab sidebar's button: three lines, lit while it's out.
    let h = 24.0 * s;
    let r = Rect::new(x, (t.h - h) * 0.5, h, h);
    let (open, over) = (v.sidebar.is_some(), r.contains(v.hover));
    if open {
        c.rect(r.x, r.y, r.w, r.h, rgba(k.accent, 0.20), 6.0 * s);
        c.outline(r.x, r.y, r.w, r.h, rgba(k.accent, 0.55), 6.0 * s, 1.0);
    } else {
        c.rect(r.x, r.y, r.w, r.h, rgba(k.wash, if over { 0.10 } else { 0.04 }), 6.0 * s);
    }
    let line = if open { rgba(k.accent, 1.0) } else { rgba(k.fg, if over { 0.95 } else { 0.7 }) };
    let thick = (1.5 * s).round().max(1.0);
    for k in 0..3 {
        let y = (r.y + r.h * 0.5 + (k as f32 - 1.0) * 4.0 * s - thick * 0.5).round();
        c.rect(r.x + 7.0 * s, y, r.w - 14.0 * s, thick, line, thick * 0.5);
    }
    hits.push((r, Hit::TabSidebar));
    x += h + 10.0 * s;
    let tabs_x = x;

    // Right side, laid out from the right edge.
    let mut rx = t.w - 12.0 * s;
    if v.window_buttons {
        let bw = 40.0 * s;
        for (label, hit) in [("✕", Hit::Close), (if v.maximized { "❐" } else { "□" }, Hit::Maximize), ("—", Hit::Minimize)] {
            rx -= bw;
            let r = Rect::new(rx, 0.0, bw, t.h);
            if r.contains(v.hover) {
                let col = if hit == Hit::Close { rgba([0xE8, 0x3A, 0x4E], 0.85) } else { rgba(k.wash, 0.10) };
                c.rect(r.x + 4.0 * s, 6.0 * s, r.w - 8.0 * s, t.h - 12.0 * s, col, 6.0 * s);
            }
            let lw = c.text_width(label);
            c.text(r.x + (r.w - lw) * 0.5, ty, label, rgba(k.fg, 0.85), false);
            hits.push((r, hit));
        }
        rx -= 8.0 * s;
    }

    let gear = "⚙";
    rx -= c.text_width(gear) + 20.0 * s;
    let r = pill(c, v, rx, gear, v.settings.is_some(), ACCENT);
    hits.push((r, Hit::OpenSettings));
    rx -= 6.0 * s;

    // On top, lit while the window's kept on top of every other. It never
    // gives way, being the quickest way to turn that off: the pills to its
    // left and then the tabs give up the room.
    if let Some(on) = v.on_top {
        let label = i18n::t("title.on_top");
        rx -= c.text_width(label) + 20.0 * s;
        let r = pill(c, v, rx, label, on, ACCENT);
        hits.push((r, Hit::ToggleOnTop));
        rx -= 6.0 * s;
    }

    // A recording under way: its clock, lit, which a click stops. It's
    // outside the terminal area, so it isn't in the file.
    if let Some(at) = v.recording {
        let secs = at.as_secs();
        let label = i18n::tf("title.recording", &[("time", &format!("{}:{:02}", secs / 60, secs % 60))]);
        rx -= c.text_width(&label) + 20.0 * s;
        let r = pill(c, v, rx, &label, true, BAD);
        hits.push((r, Hit::StopRecording));
        rx -= 6.0 * s;
    }

    // Efficient mode (e-mode), lit while it's on, saying so when it's on for the
    // battery rather than the user's switch. Where the tabs would lose their
    // room, that gives way, and then the assistant's shortcut: Settings and
    // the toasts say why it's on, and the menus and help the shortcut.
    let pill_w = |label: &str| c.text_width(label) + 20.0 * s;
    let key_label = if v.has_key { format!("● {}", v.provider.name()) } else { i18n::t("title.add_key").to_string() };
    let mode = i18n::t(if v.own_left.is_some() {
        "title.own"
    } else if v.ai.auto_mode {
        "title.auto"
    } else {
        "title.guide"
    });
    let (key_w, mode_w) = (pill_w(&key_label), pill_w(mode));
    let admin = i18n::t("title.admin");
    let admin_w = pill_w(admin);
    let fits = |lean: &str, ask: &str| rx - pill_w(lean) - key_w - mode_w - pill_w(ask) - 32.0 * s >= tabs_x + 160.0 * s;
    let short = i18n::t("title.efficient");
    // The Ask pill says the assistant's key, as it's bound; with none, its
    // short form.
    let ask_with_keys = match crate::keys::bound().chords(crate::event::Command::ToggleAssistant).first() {
        Some(chord) => tf("title.ask", &[("keys", &chord.label(crate::keys::MAC))]),
        None => i18n::t("title.ask_short").to_string(),
    };
    let ask_with_keys = ask_with_keys.as_str();
    let why = v.efficient_for.map(|power| {
        i18n::t(match power {
            crate::power::Power::Battery => "title.efficient_battery",
            _ => "title.efficient_low_power",
        })
    });
    let lean = why.filter(|why| fits(why, ask_with_keys)).unwrap_or(short);
    let ask = if fits(lean, ask_with_keys) { ask_with_keys } else { i18n::t("title.ask_short") };
    let (lean_w, ask_w) = (pill_w(lean), pill_w(ask));
    rx -= lean_w;
    let r = pill(c, v, rx, lean, v.efficient, GOOD);
    hits.push((r, Hit::ToggleEfficient));
    rx -= 6.0 * s;

    rx -= key_w;
    let r = pill(c, v, rx, &key_label, !v.has_key, if v.has_key { GOOD } else { WARN });
    hits.push((r, Hit::KeyStatus));
    rx -= 6.0 * s;

    rx -= mode_w;
    let r = if v.own_left.is_some() { pill(c, v, rx, mode, true, GOOD) } else { pill(c, v, rx, mode, v.ai.auto_mode, ACCENT2) };
    hits.push((r, Hit::ToggleMode));
    rx -= 6.0 * s;
    // A Flow running as administrator or root says so, in amber.
    if v.elevated {
        rx -= admin_w;
        pill(c, v, rx, admin, true, WARN);
        rx -= 6.0 * s;
    }

    rx -= ask_w;
    let r = pill(c, v, rx, ask, v.ai.open, ACCENT);
    hits.push((r, Hit::ToggleAi));
    rx -= 14.0 * s;

    // A newer Flow found, or installed and waiting for a restart: where the
    // tabs still have room beside it, since About says so too.
    let update = match &v.update {
        Some(UpdateState::Available { .. } | UpdateState::Installing { .. } | UpdateState::ByHand { .. } | UpdateState::InTab { .. } | UpdateState::ByPackage { .. }) => Some(i18n::t("title.update")),
        Some(UpdateState::Installed { .. }) => Some(i18n::t("title.restart")),
        _ => None,
    };
    if let Some(label) = update {
        let w = c.text_width(label) + 20.0 * s;
        if rx - w - 8.0 * s >= tabs_x + 160.0 * s {
            rx -= w;
            let r = pill(c, v, rx, label, true, ACCENT2);
            hits.push((r, Hit::UpdatePill));
            rx -= 8.0 * s;
        }
    }

    let plus = draw_tabs(c, v, tabs_x, rx, hits, naming);

    // Hairline under the title bar.
    c.rect(14.0 * s, t.h - 1.0, t.w - 28.0 * s, 1.0, rgba(k.wash, 0.06), 0.0);
    plus
}

/// `text`, cut short with "…" to fit `room` pixels.
fn fit(c: &Canvas<'_>, text: &str, room: f32) -> String {
    if c.text_width(text) <= room {
        return text.to_string();
    }
    let mut cut = text.to_string();
    while !cut.is_empty() && c.text_width(&cut) + c.text_width("…") > room {
        cut.pop();
    }
    cut + "…"
}

/// The terminal tabs, the + that opens another and the … that opens the
/// new-tab picker, between `x` and `limit`. When they don't all fit, the
/// tabs share the room and their names are cut short. A tab whose long
/// command finished while it was behind glows, green or red, until it's
/// shown. A tab whose name is being typed is a field, as wide as its name
/// and room for more, and `naming` is where its caret is. Returns the ….
fn draw_tabs(c: &mut Canvas<'_>, v: &View<'_>, x: f32, limit: f32, hits: &mut Vec<(Rect, Hit)>, naming: &mut Option<Rect>) -> Rect {
    let s = v.layout.scale;
    let k = chrome();
    let h = 24.0 * s;
    let y = (v.layout.title.h - h) * 0.5;
    let ty = y + (h - c.cell.height) * 0.5;
    let gap = 6.0 * s;
    let plus_w = c.text_width("+") + c.text_width("…") + 40.0 * s + gap;
    // The last tab can't be closed from its ×: the window has its own.
    let closable = v.tabs.len() > 1;
    let close_w = if closable { 18.0 * s } else { 0.0 };
    let dot_w = 12.0 * s;
    // A name being typed: its field's width, with room for a few letters more.
    let typing = |i: usize| v.renaming.filter(|(at, _)| *at == i).map(|(_, f)| f);
    let field_w = |f: &crate::settings::TextField| (c.text_width(&f.text()) + 4.0 * c.cell.width).max(TAB_FIELD_LEAST * s) + 20.0 * s;
    // Each pill drawn, for where a tab dragged would land.
    let mut pills: Vec<Rect> = Vec::new();
    let natural: Vec<f32> = v
        .tabs
        .iter()
        .enumerate()
        .map(|(i, t)| match typing(i) {
            Some(f) => field_w(f),
            None => c.text_width(&t.label) + 20.0 * s + close_w + if t.alert.is_some() { dot_w } else { 0.0 },
        })
        .collect();
    let room = (limit - x - plus_w - gap * v.tabs.len() as f32).max(0.0);
    let share = if natural.iter().sum::<f32>() > room { room / v.tabs.len().max(1) as f32 } else { f32::MAX };
    let mut x = x;
    for (i, t) in v.tabs.iter().enumerate() {
        let least = if typing(i).is_some() { (TAB_FIELD_LEAST * s).min(room * 0.5) } else { 40.0 * s };
        let w = natural[i].min(share).max(least);
        let r = Rect::new(x, y, w, h);
        // A tab in a group: a bar in the group's color along the pill's
        // foot, running on under the gap to the next tab where that's in
        // the same group, so a group's tabs read as one.
        if let Some((group, color)) = t.group {
            let same = |j: Option<usize>| j.and_then(|j| v.tabs.get(j)).and_then(|n| n.group).is_some_and(|(g, _)| g == group);
            let inset = h * 0.4;
            let from = if same(i.checked_sub(1)) { r.x } else { r.x + inset };
            let to = if same(Some(i + 1)) { r.x + w + gap } else { r.x + w - inset };
            let thick = (2.0 * s).round().max(1.0);
            c.rect(from, r.y + h + 2.0 * s, (to - from).max(0.0), thick, rgba(color, 0.95), thick * 0.5);
        }
        if let Some(f) = typing(i) {
            *naming = Some(draw_tab_field(c, v, r, f));
            hits.push((r, Hit::Tab(i)));
            pills.push(r);
            x += w + gap;
            continue;
        }
        let active = i == v.active_tab;
        let hovered = r.contains(v.hover);
        let alert = t.alert.map(|failed| if failed { BAD } else { k.good });
        // Steady, not pulsing: a glowing tab shouldn't keep frames coming.
        // And whatever the Glow setting, since it's news.
        if let Some(col) = alert {
            c.steady_glow([r.x, r.y, r.w, r.h], rgba(col, 0.35), h * 0.5, 12.0 * s);
            c.rect(r.x, r.y, r.w, r.h, rgba(col, 0.12), h * 0.5);
        }
        if active {
            c.glow(r.x, r.y, r.w, r.h, rgba(k.accent, 0.18), h * 0.5, 10.0 * s);
            c.rect(r.x, r.y, r.w, r.h, rgba(k.accent, 0.20), h * 0.5);
            c.outline(r.x, r.y, r.w, r.h, rgba(k.accent, 0.55), h * 0.5, 1.0);
        } else {
            c.rect(r.x, r.y, r.w, r.h, rgba(k.wash, if hovered { 0.10 } else { 0.04 }), h * 0.5);
        }
        let mut tx = r.x + 10.0 * s;
        if let Some(col) = alert {
            let d = 6.0 * s;
            c.rect(tx, y + (h - d) * 0.5, d, d, rgba(col, 0.95), d);
            tx += dot_w;
        }
        let label = fit(c, &t.label, r.x + r.w - 10.0 * s - close_w - tx);
        let fg = if active { rgba(k.accent, 1.0) } else { rgba(k.fg, if hovered { 0.95 } else { 0.7 }) };
        c.text(tx, ty, &label, fg, active);
        hits.push((r, Hit::Tab(i)));
        // The pill dragged, dimmed where it was.
        if v.tab_drag.as_ref().is_some_and(|d| d.tab == Some(i)) {
            c.rect(r.x, r.y, r.w, r.h, rgba(PANEL_TINT, 0.6), h * 0.5);
        }
        pills.push(r);
        if closable && (hovered || active) && v.tab_drag.is_none() {
            let cr = Rect::new(r.x + r.w - close_w - 4.0 * s, y + 3.0 * s, close_w, h - 6.0 * s);
            let on = cr.contains(v.hover);
            if on {
                c.rect(cr.x, cr.y, cr.w, cr.h, rgba(k.wash, 0.14), cr.h * 0.5);
            }
            let xw = c.text_width("×");
            c.text(cr.x + (cr.w - xw) * 0.5, ty, "×", rgba(k.fg, if on { 0.95 } else { 0.5 }), false);
            hits.push((cr, Hit::CloseTab(i)));
        }
        x += w + gap;
    }
    // Where a tab dragged would land: a line in the gap, in the color of
    // the group it would join there, else the accent.
    if let Some((at, color)) = v.tab_drag.as_ref().and_then(|d| d.bar) {
        let gx = match pills.get(at) {
            Some(r) => Some(r.x - gap * 0.5),
            None => pills.last().map(|r| r.x + r.w + gap * 0.5),
        };
        if let Some(gx) = gx {
            let t = (3.0 * s).round().max(2.0);
            c.rect((gx - t * 0.5).round(), y - 3.0 * s, t, h + 6.0 * s, rgba(color.unwrap_or(ACCENT), 1.0), t * 0.5);
        }
    }
    let plus = pill(c, v, x, "+", false, ACCENT);
    hits.push((plus, Hit::NewTab));
    let more = pill(c, v, plus.x + plus.w + gap, "…", v.tab_picker.is_some(), ACCENT);
    hits.push((more, Hit::NewTabChoose));
    more
}

/// The least width of a tab whose name is being typed, in points.
const TAB_FIELD_LEAST: f32 = 110.0;

/// A tab whose name is being typed, in `r`: a field lit in the accent's
/// color, the name in it scrolled to keep the caret in view, what the
/// input method is composing among its letters. Returns the caret.
fn draw_tab_field(c: &mut Canvas<'_>, v: &View<'_>, r: Rect, f: &crate::settings::TextField) -> Rect {
    let s = v.layout.scale;
    let k = chrome();
    let ty = r.y + (r.h - c.cell.height) * 0.5;
    c.glow(r.x, r.y, r.w, r.h, rgba(k.accent, 0.22), r.h * 0.5, 10.0 * s);
    c.rect(r.x, r.y, r.w, r.h, rgba(k.wash, 0.10), r.h * 0.5);
    c.outline(r.x, r.y, r.w, r.h, rgba(k.accent, 0.9), r.h * 0.5, 1.5 * s);
    let tx = r.x + 10.0 * s;
    let (x, _) = input_line_in(c, (tx, ty), r.w - 20.0 * s, (&f.text(), f.caret().1), v.preedit, s, k.fg);
    // No caret while the input method converts.
    if v.preedit.is_none_or(|p| p.caret().is_some()) {
        c.rect(x - 0.5 * s, ty, 2.0 * s, c.cell.height, rgba(k.accent, 1.0), 0.0);
    }
    Rect::new(x, ty, c.cell.width, c.cell.height)
}

/// The new-tab picker, in a card under the …: the shells and the themes
/// side by side, the chosen one in each marked (brighter in the list the
/// arrows move in), and Open under them. Where the window has no room for
/// every row, each list shows what fits, around its choice, with a bar
/// saying where it is, and the buttons stay in view under them.
fn draw_tab_picker(c: &mut Canvas<'_>, v: &View<'_>, more: Rect, picker: &TabPicker, hits: &mut Vec<(Rect, Hit)>) {
    let s = v.layout.scale;
    let pad = 6.0 * s;
    let row_h = c.cell.height + 12.0 * s;
    let shells: Vec<&str> = v.shells.iter().map(|p| p.label.as_str()).collect();
    let themes: Vec<&str> = v.themes.iter().map(String::as_str).collect();
    let widest = |names: &[&str], title: &str| names.iter().map(|n| c.text_width(n)).fold(c.text_width(title), f32::max) + 40.0 * s;
    let (shell_title, theme_title, open, save) = (t("tab_picker.shell"), t("tab_picker.theme"), t("tab_picker.open"), t("tab_picker.save"));
    let shells_w = widest(&shells, shell_title);
    let mut themes_w = widest(&themes, theme_title);
    // Save as theme… and Open tab share the last row, each as wide as its words.
    let (save_w, open_w) = (c.text_width(save) + 32.0 * s, c.text_width(open) + 48.0 * s);
    themes_w = themes_w.max(save_w + open_w - shells_w);
    let w = shells_w + themes_w + pad * 3.0;
    let x = more.x.min(v.layout.width - w - 8.0 * s).max(8.0 * s);
    let y = more.y + more.h + 6.0 * s;
    // The rows the window has room for under the titles, above the buttons.
    let room = ((v.layout.height - y - 8.0 * s - pad * 3.0) / row_h - 2.0).floor().max(1.0) as usize;
    let rows = shells.len().max(themes.len()).min(room);
    let h = row_h * (rows + 2) as f32 + pad * 3.0;
    c.glow(x, y, w, h, rgba(ACCENT, 0.18), 12.0 * s, 20.0 * s);
    c.glass(x, y, w, h, rgba(PANEL_TINT, 1.0), 12.0 * s);
    let [shell_top, theme_top] = picker.tops.get();
    let tops = [keep_in_view(shell_top, picker.shell, rows, shells.len()), keep_in_view(theme_top, picker.theme, rows, themes.len())];
    picker.tops.set(tops);
    let mut list = |c: &mut Canvas<'_>, lx: f32, lw: f32, title: &str, names: &[&str], top: usize, chosen: usize, here: bool, hit: fn(usize) -> Hit| {
        c.text(lx + 12.0 * s, y + pad + (row_h - c.cell.height) * 0.5, title, rgba(FOREGROUND, 0.5), false);
        for (row, (i, name)) in names.iter().enumerate().skip(top).take(rows).enumerate() {
            let r = Rect::new(lx, y + pad + row_h * (row + 1) as f32, lw, row_h);
            let hovered = r.contains(v.hover);
            if i == chosen || hovered {
                let a = if i != chosen { 0.10 } else if here { 0.26 } else { 0.14 };
                c.rect(r.x, r.y, r.w, r.h, rgba(ACCENT, a), 8.0 * s);
            }
            let fg = if i == chosen || hovered { rgba(ACCENT, if here || i != chosen { 1.0 } else { 0.8 }) } else { rgba(FOREGROUND, 0.85) };
            c.text(r.x + 12.0 * s, r.y + (r.h - c.cell.height) * 0.5, name, fg, i == chosen && here);
            hits.push((r, hit(i)));
        }
        // Where the rows shown are in a list longer than them.
        if names.len() > rows {
            let (track_y, track_h) = (y + pad + row_h, row_h * rows as f32);
            let (from, part) = (top as f32 / names.len() as f32, rows as f32 / names.len() as f32);
            c.rect(lx + lw - 5.0 * s, track_y + track_h * from, 3.0 * s, track_h * part, rgba(FOREGROUND, 0.3), 1.5 * s);
        }
    };
    list(c, x + pad, shells_w, shell_title, &shells, tops[0], picker.shell, !picker.in_themes, Hit::TabPickerShell);
    list(c, x + pad * 2.0 + shells_w, themes_w, theme_title, &themes, tops[1], picker.theme, picker.in_themes, Hit::TabPickerTheme);
    let by = y + pad * 2.0 + row_h * (rows + 1) as f32;
    let r = Rect::new(x + pad, by, save_w, row_h);
    let on = r.contains(v.hover);
    c.rect(r.x, r.y, r.w, r.h, rgba([255, 255, 255], if on { 0.14 } else { 0.07 }), 8.0 * s);
    c.text(r.x + (r.w - c.text_width(save)) * 0.5, r.y + (r.h - c.cell.height) * 0.5, save, rgba(FOREGROUND, if on { 1.0 } else { 0.85 }), false);
    hits.push((r, Hit::TabPickerSave));
    let r = Rect::new(x + pad * 2.0 + save_w, by, w - pad * 3.0 - save_w, row_h);
    let on = r.contains(v.hover);
    c.rect(r.x, r.y, r.w, r.h, rgba(ACCENT, if on { 0.30 } else { 0.18 }), 8.0 * s);
    c.text(r.x + (r.w - c.text_width(open)) * 0.5, r.y + (r.h - c.cell.height) * 0.5, open, rgba(ACCENT, 1.0), true);
    hits.push((r, Hit::TabPickerOpen));
}

/// The grid, its cursor and what the input method is composing there.
/// Returns the cursor's cell while the terminal takes the keys.
/// The lines between a split tab's panes, in the gaps that part them.
/// The lines between a split tab's panes, each in the middle of its gap,
/// brighter under the pointer, and each a hit a few pixels wide that drags
/// it.
fn draw_dividers(c: &mut Canvas<'_>, v: &View<'_>, hits: &mut Vec<(Rect, Hit)>) {
    let s = v.layout.scale;
    let thick = 1.0_f32.max(s);
    let grab = (8.0 * s).max(thick);
    for (i, &(line, across)) in v.dividers.iter().enumerate() {
        let hit = if across {
            Rect::new(line.x + (line.w - grab) * 0.5, line.y, grab, line.h)
        } else {
            Rect::new(line.x, line.y + (line.h - grab) * 0.5, line.w, grab)
        };
        let alpha = if hit.contains(v.hover) { 0.7 } else { 0.28 };
        if across {
            c.rect((line.x + line.w * 0.5).round(), line.y, thick, line.h, rgba(ACCENT, alpha), 0.0);
        } else {
            c.rect(line.x, (line.y + line.h * 0.5).round(), line.w, thick, rgba(ACCENT, alpha), 0.0);
        }
        hits.push((hit, Hit::Divider(i, across)));
    }
}

/// The room a pane's scrollbar takes its track in: its text, less the find
/// bar at its top while that's over it (the pane in front, as the pill
/// goes), so a mark at the top shows below the bar.
pub fn scrollbar_room(term: Rect, find_over: bool, cell_h: f32, scale: f32) -> Rect {
    if !find_over {
        return term;
    }
    let top = find_bar_height(cell_h, scale) + 12.0 * scale;
    Rect::new(term.x, term.y + top, term.w, (term.h - top).max(0.0))
}

/// Where a split tab's pane has its ✕ ([`Hit::PaneClose`]): a cell tall and
/// as wide, at the top right of its text `term`, left of its scrollbar's
/// strip (`zone` wide, none where it shows no bar), `top` down (below the
/// find bar where that's over it).
/// Who a pane works for, where it isn't the user: the assistant's own pane,
/// one of its helpers', or an outside app's ([`crate::mcp`]), by its place in
/// the names Flow keeps ([`outside_name`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneFor {
    Assistant,
    Helper(u32),
    Outside(u32),
}

impl PaneFor {
    pub fn label(self) -> String {
        match self {
            PaneFor::Assistant => t("pane.assistant").to_string(),
            PaneFor::Helper(n) => tf("pane.helper", &[("n", &n)]),
            PaneFor::Outside(n) => outside_name(n),
        }
    }
}

/// The names of the outside apps Flow has opened panes for, which their
/// [`PaneFor::Outside`] label says.
static OUTSIDE_NAMES: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// The place of outside app `name` among them, kept once.
pub fn outside_slot(name: &str) -> u32 {
    let mut names = OUTSIDE_NAMES.lock().unwrap_or_else(|e| e.into_inner());
    match names.iter().position(|n| n == name) {
        Some(i) => i as u32,
        None => {
            names.push(name.to_string());
            names.len() as u32 - 1
        }
    }
}

fn outside_name(slot: u32) -> String {
    OUTSIDE_NAMES.lock().unwrap_or_else(|e| e.into_inner()).get(slot as usize).cloned().unwrap_or_default()
}

pub fn pane_close_rect(term: Rect, zone: f32, top: f32, cell_h: f32) -> Rect {
    Rect::new(term.x + term.w - zone - cell_h, term.y + top, cell_h, cell_h)
}

/// A shell window's frame round a pane ([`Frame`]), as the machine drew
/// one, on whole pixels: the outline in the pane's text color (the
/// machine's white), and along the top the title bar, white with the
/// background's color between its parts: a close box of Flow's own at its
/// left (a cross, not the machine's gadget), the tab's name in the pane's
/// own letters in a box of white, the letters in the background's color,
/// then three lines, the bar's top, middle and foot, to the depth gadgets
/// that bring the window in front of the others and send it behind them;
/// and in the bottom right corner the sizing gadget. A gadget under the pointer that would do something is
/// drawn as the machine drew one pressed, its colors turned over. A frame
/// whose pane hasn't the keys, or while Flow hasn't, has its bar ghosted,
/// as the machine's inactive windows had (`ghost`).
fn draw_frame(c: &mut Canvas<'_>, v: &View<'_>, pane: &PaneView<'_>, f: Frame) {
    let (o, line, ink) = (f.outer, f.line, &pane.ink);
    let (right, foot) = (o.x + o.w, o.y + o.h);
    // Pressed, the machine turned a gadget's colors over: white and black
    // change places, and its blue becomes its orange, the cursor's.
    let pen = |p: Pen, pressed: bool| -> Rgba {
        let rgb = match (p, pressed) {
            (Pen::White, false) | (Pen::Black, true) => ink.text,
            (Pen::Blue, false) => ink.background,
            (Pen::Black, false) | (Pen::White, true) => [0, 0, 0],
            (Pen::Blue, true) => ink.cursor,
        };
        rgba(rgb, 1.0)
    };
    let white = pen(Pen::White, false);
    // The outline's sides and foot; the bar's top is its first line.
    c.rect(o.x, o.y, line, o.h, white, 0.0);
    c.rect(right - line, o.y, line, o.h, white, 0.0);
    c.rect(o.x, foot - line, o.w, line, white, 0.0);

    let close = f.close_box();
    let pressed = close.contains(v.hover);
    c.rect(close.x, close.y, close.w, close.h, pen(Pen::White, pressed), 0.0);
    let unit = (close.w * 0.55 / 5.0).floor().max(1.0);
    let (cx, cy) = ((close.x + (close.w - 5.0 * unit) / 2.0).round(), (close.y + (close.h - 5.0 * unit) / 2.0).round());
    for k in 0..5 {
        let (a, b) = (k as f32 * unit, (4 - k) as f32 * unit);
        c.rect(cx + a, cy + a, unit, unit, pen(Pen::Blue, pressed), 0.0);
        c.rect(cx + b, cy + a, unit, unit, pen(Pen::Blue, pressed), 0.0);
    }

    let from = f.after_close();
    let (end, gadgets) = f.lines_end();
    let pad = (1.5 * f.unit()).round();
    // The name, cut where it would leave the lines less than a bar's width.
    let room = (end - from - 2.0 * pad - f.bar).max(0.0);
    let mut name: String = v.tabs.get(v.active_tab).map_or("", |t| t.label.as_str()).to_string();
    while !name.is_empty() && c.text_width(&name) > room {
        name.pop();
    }
    let named = if name.is_empty() {
        from
    } else {
        let w = (pad + c.text_width(&name) + pad).round();
        c.rect(from, o.y, w, f.bar, white, 0.0);
        c.text(from + pad, (o.y + (f.bar - pane.layout.cell_h) / 2.0).round(), &name, pen(Pen::Blue, false), false);
        from + w
    };
    if end > named {
        for (top, bottom) in f.stripes() {
            c.rect(named, top, end - named, bottom - top, white, 0.0);
        }
    }
    // The lines' white end before the gadgets, as tall as the bar.
    if gadgets > end {
        c.rect(end, o.y, gadgets - end, f.bar, white, 0.0);
    }
    if !(pane.focused && v.focused) {
        ghost(c, &f, from, named, 1..9, white);
        ghost(c, &f, named, end, 2..4, white);
        ghost(c, &f, named, end, 6..8, white);
    }

    let mut gadget = |at: Rect, pieces: &[Piece], acts: bool| {
        let pressed = acts && at.contains(v.hover);
        for p in pieces {
            let r = f.piece((at.x, at.y), p);
            c.rect(r.x, r.y, r.w, r.h, pen(p.4, pressed), 0.0);
        }
    };
    if let Some((front, back)) = f.gadgets() {
        gadget(front, &TO_FRONT, true);
        gadget(back, &TO_BACK, v.sends_to_back);
    }
    gadget(f.size_box(), &SIZING, v.sizes);
}

/// Whether a frame's sizing gadget drags the window's corner: not while the
/// window is maximized or full screen, its size the screen's. Where winit
/// can't start the system's drag (the Mac), Flow drags it itself
/// (`App::size_window_to`).
pub fn frame_sizes(maximized: bool, full_screen: bool) -> bool {
    !maximized && !full_screen
}

/// Ghosts the part of a frame's title bar from `x0` to `x1` on the
/// machine's `lines`, as it ghosted an inactive window's: a dot of white in
/// every four of its pixels, two further along on every other line, which
/// shows only on the background's color.
fn ghost(c: &mut Canvas<'_>, f: &Frame, x0: f32, x1: f32, lines: std::ops::Range<usize>, white: Rgba) {
    let (u, px) = (f.unit(), f.unit() / 2.0);
    if px <= 0.0 {
        return;
    }
    for l in lines {
        let along = if l % 2 == 0 { 3.0 } else { 1.0 };
        let mut k = (((x0 - f.outer.x) / px - along) / 4.0).ceil().max(0.0);
        loop {
            let x = f.outer.x + (along + 4.0 * k) * px;
            if x + px > x1 {
                break;
            }
            let r = Frame::edges(x, f.outer.y + l as f32 * u, x + px, f.outer.y + (l + 1) as f32 * u);
            c.rect(r.x, r.y, r.w, r.h, white, 0.0);
            k += 1.0;
        }
    }
}

fn draw_terminal(c: &mut Canvas<'_>, v: &View<'_>, pane: &PaneView<'_>, hits: &mut Vec<(Rect, Hit)>) -> Option<Rect> {
    let session = pane.session;
    let l = &pane.layout;
    let (cw, ch) = (l.cell_w, l.cell_h);
    let emu = session.emu.lock();
    let content = emu.term.renderable_content();
    let colors = content.colors;
    let offset = content.display_offset as i32;
    let selection = content.selection;
    let ripple = if v.ripple { 1.0 } else { 0.0 };
    let ink = pane.ink;
    let images = session.images.lock();
    // An animated picture shows the frame due now.
    let now = Instant::now();
    // The last placeholder cell read: its screen row, column and marks.
    let mut last_mark: Option<(i32, usize, graphics::Mark)> = None;
    // Each row's size: a double line's cells are twice as wide.
    let sizes: Vec<LineSize> = (0..l.rows).map(|row| line_size::on_screen(emu.term.grid(), row)).collect();
    let cells = Grid { x: l.term.x, y: l.term.y, cw, ch };
    // What the find bar found on the screen as it's scrolled, the one
    // chosen brighter.
    let found = v.find.filter(|_| pane.focused).map(|f| f.shown(-offset, l.rows as i32 - 1 - offset)).unwrap_or_default();

    for indexed in content.display_iter {
        let cell = indexed.cell;
        let point = indexed.point;
        let row = point.line.0 + offset;
        if row < 0 || row as usize >= l.rows || point.column.0 >= l.cols {
            continue;
        }
        if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
            continue;
        }
        let y = cells.y(row as f32);
        let bold = cell.flags.contains(Flags::BOLD);
        let dim = cell.flags.contains(Flags::DIM);
        let mut fg = palette::resolve(cell.fg, colors, ink, bold, dim);
        let mut bg = palette::resolve(cell.bg, colors, ink, false, false);
        let mut bg_default = palette::is_default_bg(cell.bg);
        if cell.flags.contains(Flags::INVERSE) {
            std::mem::swap(&mut fg, &mut bg);
            bg_default = false;
        }
        let selected = selection.is_some_and(|s| s.contains(point));
        let matched = found.iter().find(|(m, _)| m.contains(&point)).map(|&(_, current)| rgba(ACCENT2, if current { 0.62 } else { 0.3 }));
        // A double line shows the first half of its columns, twice as wide.
        let size = sizes[row as usize];
        if point.column.0 >= size.shown(l.cols) {
            continue;
        }
        let span = if size.doubled() { 2.0 } else { 1.0 };
        // Its first column, counted in single cells.
        let first = point.column.0 as f32 * span;
        let x = cells.x(first);
        if let Some(mut mark) = graphics::read_placeholder(cell) {
            // Marks a cell leaves out go on from the cell before, as the
            // protocol says.
            let before = last_mark
                .filter(|(r, col, m)| *r == row && col + 1 == point.column.0 && (m.id, m.placement) == (mark.id, mark.placement))
                .map(|(_, _, m)| m);
            let r = mark.row.or(before.and_then(|m| m.row)).unwrap_or(0);
            let col = mark.col.or(before.and_then(|m| m.col.map(|c| c + 1))).unwrap_or(0);
            (mark.row, mark.col) = (Some(r), Some(col));
            last_mark = Some((row, point.column.0, mark));
            if let Some((image, p)) = images.lookup(mark.id, mark.placement) {
                let frame = image.frame_at(now).map(|(frame, next)| (frame.clone(), next));
                let image = match frame {
                    Some((frame, next)) => {
                        c.changes_at(next);
                        frame
                    }
                    None => image,
                };
                if let Some((piece, uv)) = graphics::tile((image.width, image.height), &p, (cw, ch), r, col) {
                    // The cell's piece, from its top-left corner, drawn as
                    // its line's text is: on a double line twice as wide,
                    // and each half of a double height showing its half.
                    let within = [piece[0] - col as f32 * cw, piece[1] - r as f32 * ch, piece[2], piece[3]];
                    if let Some((rect, uv)) = CharCell::new(size, cells, first, span, row as f32).picture((cw, ch), within, uv) {
                        c.image_tile(&image, rect, uv, ripple);
                    }
                }
            }
            if selected {
                c.rect(x, y, cw * span, ch, rgba(ACCENT, 0.28), 0.0);
            } else if let Some(lit) = matched {
                c.rect(x, y, cw * span, ch, lit, 0.0);
            }
            continue;
        }
        let width = if cell.flags.contains(Flags::WIDE_CHAR) { 2.0 } else { 1.0 } * span;
        // Under the effects programs ask for, which glow over a game's own
        // ground as over its pictures.
        let edges = [x, y, cells.x(first + width), cells.y(row as f32 + 1.0)];
        if selected {
            fill_cell(c, edges, rgba(ACCENT, 0.28));
        } else if let Some(lit) = matched {
            fill_cell(c, edges, lit);
        } else if !bg_default {
            fill_cell(c, edges, rgba(bg, 0.88));
        }
        let at = CharCell::new(size, cells, first, width, row as f32);
        let marks = marks_of(cell);
        if (cell.c != ' ' || !marks.is_empty()) && !cell.flags.contains(Flags::HIDDEN) {
            draw_char(c, (cell.c, marks), at, rgba(fg, 1.0), (bold, cell.flags.contains(Flags::ITALIC)), ripple);
        }
        let thick = 1.0_f32.max(l.scale);
        // The link under the pointer, in the accent color over its own.
        if pane.focused && v.link.is_some_and(|link| link.covers(cell.hyperlink().as_ref(), point)) {
            at.line(c, c.cell.baseline + 2.0, thick, rgba(ACCENT, 1.0));
        } else if cell.flags.intersects(Flags::ALL_UNDERLINES) {
            at.line(c, c.cell.baseline + 2.0, thick, rgba(fg, 0.9));
        }
        if cell.flags.contains(Flags::STRIKEOUT) {
            at.line(c, ch * 0.5, thick, rgba(fg, 0.9));
        }
    }

    // Cursor: eased position, soft glow, pulse instead of blink. What the
    // input method is composing takes its place, at the cell it's headed for.
    let cursor = content.cursor;
    let focused_term = pane.focused && !v.ai.focus && v.key_dialog.is_none() && v.renaming.is_none();
    let row = (cursor.point.line.0 + offset).clamp(0, l.rows.saturating_sub(1) as i32);
    // On a double line, as wide as its cells.
    let cursor_size = sizes.get(row as usize).copied().unwrap_or_default();
    let cw = if cursor_size.doubled() { cw * 2.0 } else { cw };
    let cell = Rect::new(l.term.x + cursor.point.column.0 as f32 * cw, l.term.y + row as f32 * ch, cw, ch);
    let preedit = v.preedit.filter(|_| focused_term && offset == 0);
    if let Some(p) = preedit {
        if let Some(x) = draw_preedit(c, l, p, cell.x, cell.y) {
            c.rect(x, cell.y, 2.0 * l.scale, ch, rgba(ink.cursor, 1.0), 1.0);
        }
    } else if cursor.shape != CursorShape::Hidden && offset == 0 && (v.cursor_lit || !pane.focused) {
        // The pane in front's glides; the others' stand where they are.
        let (x, y) = if pane.focused { v.cursor_pos } else { (cell.x, cell.y) };
        let glow = 0.35 + 0.15 * (v.time * 3.0).sin();
        let col = rgba(ink.cursor, 1.0);
        // The glow takes the cursor's shape, so a bar or an underline doesn't
        // sit on a faint block.
        let solid = focused_term && cursor.shape != CursorShape::HollowBlock;
        let (gy, gw, gh) = match cursor.shape {
            CursorShape::Beam if solid => (y, 2.0 * l.scale, ch),
            CursorShape::Underline if solid => (y + ch - 2.0 * l.scale, cw, 2.0 * l.scale),
            _ => (y, cw, ch),
        };
        c.glow(x, gy, gw, gh, rgba(ink.cursor, glow * 0.6), 2.0, 10.0 * l.scale);
        match (cursor.shape, focused_term) {
            (_, false) | (CursorShape::HollowBlock, _) => c.outline(x, y, cw, ch, col, 2.0, 1.0_f32.max(l.scale)),
            (CursorShape::Beam, true) => c.rect(x, y, 2.0 * l.scale, ch, col, 1.0),
            (CursorShape::Underline, true) => c.rect(x, y + ch - 2.0 * l.scale, cw, 2.0 * l.scale, col, 1.0),
            _ => {
                c.rect(x, y, cw, ch, rgba(ink.cursor, 0.85), 2.0);
                // Redraw the glyph under a block cursor in the background color.
                let grid = emu.term.grid();
                let p = cursor.point;
                if p.line.0 >= 0 && (p.line.0 as usize) < l.rows && p.column.0 < l.cols {
                    let cell = &grid[Point::new(Line(p.line.0), Column(p.column.0))];
                    if cell.c != ' ' || !marks_of(cell).is_empty() {
                        // A grid of its own, from where the cursor is on its way.
                        // As wide as the character's own cells, so a square two
                        // cells wide is redrawn where it was drawn.
                        let span = if cursor_size.doubled() { 2.0 } else { 1.0 };
                        let cols = if cell.flags.contains(Flags::WIDE_CHAR) { 2.0 } else { 1.0 } * span;
                        let at = CharCell::new(cursor_size, Grid { x, y, cw: l.cell_w, ch }, 0.0, cols, 0.0);
                        draw_char(c, (cell.c, marks_of(cell)), at, rgba(ink.background, 1.0), (cell.flags.contains(Flags::BOLD), false), 0.0);
                    }
                }
            }
        }
    }
    let history = emu.term.grid().history_size();
    // The line Previous or Next Command went to, lit a moment across the
    // pane, so it's seen where the screen couldn't move to it.
    if let Some((line, lit)) = pane.jumped {
        let row = line as i64 - (history as i64 - offset as i64);
        if (0..l.rows as i64).contains(&row) {
            c.rect(l.term.x, l.term.y + row as f32 * ch, l.cols as f32 * l.cell_w, ch, rgba(ACCENT, 0.22 * lit), 0.0);
        }
    }
    // The scrollbar along the right edge, where it shows: a thumb that
    // widens under the pointer and while it's held, over a track then.
    let finding = v.find.filter(|_| pane.focused);
    let room = scrollbar_room(l.term, finding.is_some(), l.cell_h, l.scale);
    let bar = (pane.scrollbar > 0.0).then(|| crate::scrollbar::Bar::place(room, l.scale, history, l.rows, offset as usize)).flatten();
    if let Some(bar) = bar {
        let shown = pane.scrollbar;
        let lit = pane.scrollbar_held || bar.zone.contains(v.hover);
        let (track, thumb) = (bar.track, bar.thumb);
        let w = if lit { track.w } else { bar.thin };
        let x = track.x + track.w - w;
        if lit {
            c.rect(x, track.y, w, track.h, rgba(ink.text, 0.08 * shown), w * 0.5);
        }
        c.rect(x, thumb.y, w, thumb.h, rgba(ink.text, (if lit { 0.6 } else { 0.35 }) * shown), w * 0.5);
        // The commands run, each a short mark at the track's left where its
        // prompt is: a failed one in the error color, one still running
        // dim. Under the find bar's, which come next.
        let thick = (2.0 * l.scale).round().max(1.0);
        let mut last = f32::NAN;
        for (line, mark) in crate::marks::all(emu.term.grid()) {
            let y = bar.mark_y(line, history, l.rows).round();
            if y == last {
                continue;
            }
            last = y;
            let color = match mark {
                crate::marks::Mark::Done => ACCENT,
                crate::marks::Mark::Failed => BAD,
                crate::marks::Mark::Running => DIM,
            };
            c.rect(track.x - thick, y - thick * 0.5, track.w * 0.6 + thick, thick, rgba(color, 0.9 * shown.max(0.6)), 0.0);
        }
        // The find bar's matches, each a mark across the track where it is
        // in the scrollback, as the find bar lights them: the one chosen
        // brighter and taller. Marks on the same pixel row are drawn once.
        if let Some(f) = finding {
            let thick = (2.0 * l.scale).round().max(1.0);
            let mut last = f32::NAN;
            for (i, m) in f.matches.iter().enumerate() {
                let line = (history as i64 + m.start().line.0 as i64).max(0) as usize;
                let y = bar.mark_y(line, history, l.rows).round();
                let chosen = Some(i) == f.current;
                if y == last && !chosen {
                    continue;
                }
                last = y;
                let (h, a) = if chosen { (thick * 2.0, 1.0) } else { (thick, 0.8) };
                c.rect(track.x - thick, y - h * 0.5, track.w + thick, h, rgba(ACCENT2, a * shown.max(0.6)), 0.0);
            }
        }
        hits.push((bar.zone, Hit::Scrollbar(pane.index)));
    }
    // Scrolled back, a pill says how many lines are below, and takes you
    // back down to them.
    // Under the find bar, where it's open.
    let top = if v.find.is_some() && pane.focused { find_bar_height(c.cell.height, l.scale) + 12.0 * l.scale } else { 0.0 };
    // A split tab's pane under the pointer has a ✕ at its top right, left
    // of its scrollbar; one in a frame has the frame's close box instead.
    let zone = bar.map_or(0.0, |b| b.zone.w);
    let close = (v.panes.len() > 1 && l.frame.is_none() && l.term.contains(v.hover)).then(|| pane_close_rect(l.term, zone, top, ch));
    if let Some(r) = close {
        let lit = r.contains(v.hover);
        c.glass(r.x, r.y, r.w, r.h, rgba(PANEL_TINT, if lit { 0.95 } else { 0.8 }), r.h * 0.5);
        let xw = c.text_width("✕");
        c.text(r.x + (r.w - xw) * 0.5, r.y, "✕", if lit { rgba(FOREGROUND, 1.0) } else { rgba(DIM, 1.0) }, false);
        hits.push((r, Hit::PaneClose(pane.index)));
    }
    // Who the pane works for, where it isn't the user, at its top right.
    let by_room = match pane.by {
        Some(by) => {
            let label = by.label();
            let w = c.text_width(&label) + 12.0 * l.scale;
            let x = l.term.x + l.term.w - zone - close.map_or(0.0, |r| r.w + 6.0 * l.scale) - w;
            let y = l.term.y + top;
            c.glass(x, y, w, ch, rgba(PANEL_TINT, 0.85), ch * 0.5);
            c.text(x + 6.0 * l.scale, y, &label, rgba(ACCENT2, 1.0), false);
            w + 6.0 * l.scale
        }
        None => 0.0,
    };
    if offset > 0 {
        let label = if offset == 1 { t("terminal.scrolled_one").to_string() } else { tf("terminal.scrolled", &[("count", &offset)]) };
        let w = c.text_width(&label);
        let room = zone + close.map_or(0.0, |r| r.w + 6.0 * l.scale) + by_room;
        let x = l.term.x + l.term.w - room - w - 8.0 * l.scale;
        let y = l.term.y + top;
        let pill = Rect::new(x, y, w, ch);
        let hovered = pill.contains(v.hover);
        c.glass(x, y, w, ch, rgba(PANEL_TINT, if hovered { 0.95 } else { 0.8 }), ch * 0.5);
        c.text(x, y, &label, if hovered { rgba(FOREGROUND, 1.0) } else { rgba(DIM, 1.0) }, false);
        hits.push((pill, Hit::ScrollBottom(pane.index)));
    }
    // Programs that draw their own cursor hide the real one, but text typed
    // still goes where it is.
    focused_term.then_some(cell)
}

/// Fills a cell out to whole pixels as its ground (`Canvas::ground`), from
/// its edges (left, top, right, bottom: the grid's, `Grid`), so cells side
/// by side (a background, a band of teletext) meet on one pixel, with no
/// seam where their edges' soft pixels would overlap.
fn fill_cell(c: &mut Canvas<'_>, [left, top, right, bottom]: [f32; 4], color: Rgba) {
    let (x0, y0) = (left.round(), top.round());
    c.ground(x0, y0, right.round() - x0, bottom.round() - y0, color);
}

/// Where the terminal's cells' edges are: column `col`'s left (counted in
/// single cells) and row `row`'s top, from the grid's corner. Two cells
/// side by side (or one over the other) take the edge they share from the
/// same column (or row), worked out the same way for both, so it rounds
/// to the same pixel for each: their lines, blocks and grounds neither
/// part nor overlap there.
#[derive(Debug, Clone, Copy)]
struct Grid {
    x: f32,
    y: f32,
    cw: f32,
    ch: f32,
}

impl Grid {
    fn x(&self, col: f32) -> f32 {
        self.x + col * self.cw
    }

    fn y(&self, row: f32) -> f32 {
        self.y + row * self.ch
    }
}

/// Where a character goes on a line of its size (`crate::line_size`): the
/// part of the line's row it's drawn in and, on a double line, its cell
/// twice as wide (and maybe as tall), of which the row shows its part.
#[derive(Debug, Clone, Copy)]
struct CharCell {
    rect: Rect,
    /// The whole cell's edges (left, top, right, bottom): on a double
    /// line twice as wide, and twice as tall from the double height's top
    /// row, as `grid` has them.
    edges: [f32; 4],
    scaled: Option<Scaled>,
}

impl CharCell {
    /// The cell `cols` single cells wide from column `col` (in single
    /// cells) on row `row` of `grid`, a line of `size`.
    fn new(size: LineSize, grid: Grid, col: f32, cols: f32, row: f32) -> Self {
        let (x, y, h) = (grid.x(col), grid.y(row), grid.ch);
        // Whole pixels, so the halves of a double-height block meet.
        let rows = (y.round(), grid.y(row + 1.0).round());
        let (scaled, top, bottom) = match size {
            LineSize::Single => (None, row, row + 1.0),
            LineSize::Wide => (Some(Scaled { at: (x, y), scale: (2.0, 1.0), rows }), row, row + 1.0),
            LineSize::Top => (Some(Scaled { at: (x, y), scale: (2.0, 2.0), rows }), row, row + 2.0),
            LineSize::Bottom => (Some(Scaled { at: (x, y - h), scale: (2.0, 2.0), rows }), row - 1.0, row + 1.0),
        };
        let edges = [x, grid.y(top), grid.x(col + cols), grid.y(bottom)];
        Self { rect: Rect::new(x, y, cols * grid.cw, h), edges, scaled }
    }

    /// The whole cell (x, y, w, h), and on a double line the rows (top and
    /// bottom) its line shows of it.
    fn whole(&self) -> ([f32; 4], Option<(f32, f32)>) {
        let r = self.rect;
        match self.scaled {
            Some(s) => ([s.at.0, s.at.1, r.w, r.h * s.scale.1], Some(s.rows)),
            None => ([r.x, r.y, r.w, r.h], None),
        }
    }

    /// Where a piece of a picture in the cell goes, and the part of the
    /// picture it shows: `piece` is x, y, w, h from the cell's top left in a
    /// single cell `cell` big, `uv` its part of the picture. On a double
    /// line it's scaled with the cell, and cut to the rows the line shows of
    /// it, as a character is. None if none of it shows.
    fn picture(&self, (cw, ch): (f32, f32), [px, py, pw, ph]: [f32; 4], [u0, v0, u1, v1]: [f32; 4]) -> Option<([f32; 4], [f32; 4])> {
        let ([x, y, w, h], rows) = self.whole();
        let (kx, ky) = (w / cw, h / ch);
        let (top, tall) = (y + py * ky, ph * ky);
        let (y0, y1) = shown(top, tall, rows)?;
        let v = |at: f32| v0 + (at - top) / tall * (v1 - v0);
        Some(([x + px * kx, y0, pw * kx, y1 - y0], [u0, v(y0), u1, v(y1)]))
    }

    /// A line across the cell `down` from its top and `thick` (an
    /// underline, a strikeout), scaled with the character and drawn where
    /// the row shows it.
    fn line(&self, c: &mut Canvas<'_>, down: f32, thick: f32, color: Rgba) {
        let ([x, y, w, _], rows) = self.whole();
        let k = self.scaled.map_or(1.0, |s| s.scale.1);
        if let Some((y0, y1)) = shown(y + down * k, thick * k, rows) {
            c.rect(x, y0, w, y1 - y0, color, 0.0);
        }
    }
}

/// Of a span `h` tall from `y`, the part (top and bottom) within `rows`, or
/// all of it without; none if none shows.
fn shown(y: f32, h: f32, rows: Option<(f32, f32)>) -> Option<(f32, f32)> {
    let (top, bottom) = rows.unwrap_or((y, y + h));
    let (y0, y1) = (y.max(top), (y + h).min(bottom));
    (y1 > y0).then_some((y0, y1))
}

/// A cell's combining marks, drawn with its letter: none for a picture's
/// cell, whose marks say where in the picture it is.
fn marks_of(cell: &alacritty_terminal::term::cell::Cell) -> &[char] {
    match cell.zerowidth() {
        Some(marks) if cell.c != graphics::PLACEHOLDER => marks,
        _ => &[],
    }
}

/// Draws `chr` at `at`: a block graphic as rectangles of the cell's own
/// pixels (`crate::mosaic`), a line of box drawing or a square as strokes
/// of them (`crate::box_drawing`), both from the cell's edges as the grid
/// has them, anything else from the font with its combining `marks`
/// shaped on it (`crate::combining`), bigger on a double line. Blocks,
/// and a double-height character's halves from the font, don't ride the
/// liquid, so that they stay joined. Box drawing's strokes ride it, on a
/// double-height line too: each bends with it by its corners, as a
/// picture's pieces do, so the strokes that meet at a cell's edge, or
/// where a double height's halves meet, stay joined.
fn draw_char(c: &mut Canvas<'_>, (chr, marks): (char, &[char]), at: CharCell, color: Rgba, face: (bool, bool), ripple: f32) {
    let rows = at.scaled.map(|s| s.rows);
    if let Some(m) = mosaic::of(chr) {
        m.each_rect(at.edges, |[x, y, w, h]| {
            if let Some((y0, y1)) = shown(y, h, rows) {
                c.rect(x, y0, w, y1 - y0, color, 0.0);
            }
        });
        return;
    }
    if let Some(shape) = box_drawing::of(chr) {
        let scale = at.scaled.map_or([1.0, 1.0], |s| [s.scale.0, s.scale.1]);
        let pen = box_drawing::Pen { stroke: box_drawing::stroke(c.cell.width, c.cell.stem), x_height: c.cell.x_height, scale };
        shape.each_piece(at.edges, pen, |piece| c.stroke(&piece, rows, color, ripple));
        return;
    }
    let letter = (chr, c.marks(chr, marks));
    match at.scaled {
        None => c.letter(at.rect.x, at.rect.y, letter, color, face, ripple),
        Some(s) => c.glyph_scaled(letter, color, face, s, if s.scale.1 > 1.0 { 0.0 } else { ripple }),
    }
}

/// What the input method is composing, over the cells from (x, y) where it
/// will land: each character takes the cells it will once committed, on a
/// plate of the background that hides what's beneath, underlined (heavier
/// where it's being converted). Moved left if it would run off the grid.
/// Returns where its caret is, if it shows one.
fn draw_preedit(c: &mut Canvas<'_>, l: &Layout, p: &Preedit, x: f32, y: f32) -> Option<f32> {
    let (cw, ch) = (l.cell_w, l.cell_h);
    let ink = palette::ink();
    let marked = p.marked();
    let cells: usize = marked.iter().map(|&(chr, _)| chr.width().unwrap_or(0)).sum();
    let x = x.min(l.term.x + (l.cols.saturating_sub(cells)) as f32 * cw).max(l.term.x);
    c.rect(x, y, cells as f32 * cw, ch, rgba(ink.background, 0.94), 2.0);
    let caret = p.caret();
    let mut cx = x;
    let mut caret_x = None;
    let mut spans = Vec::new();
    for (i, &(chr, converting)) in marked.iter().enumerate() {
        if caret == Some(i) {
            caret_x = Some(cx);
        }
        let w = chr.width().unwrap_or(0) as f32 * cw;
        if w == 0.0 {
            continue;
        }
        c.glyph(cx, y, chr, rgba(ink.text, 1.0), false, false, 0.0);
        spans.push((cx, cx + w, Some(converting)));
        cx += w;
    }
    underline_composed(c, y + c.cell.baseline + 2.0, &spans, rgba(ink.text, 0.9), l.scale);
    caret_x.or(caret.map(|_| cx))
}

/// Underlines composed text: one stroke for each run of characters marked
/// alike, so no seams show between them, heavier where it's being converted.
/// `spans` are each character's left and right, and Some(true) if it's being
/// converted (None for text already typed, which isn't underlined).
fn underline_composed(c: &mut Canvas<'_>, y: f32, spans: &[(f32, f32, Option<bool>)], color: Rgba, s: f32) {
    let mut i = 0;
    while i < spans.len() {
        let (left, _, mark) = spans[i];
        let mut j = i;
        while j + 1 < spans.len() && spans[j + 1].2 == mark {
            j += 1;
        }
        if let Some(converting) = mark {
            let thick = if converting { 2.0 * s } else { 1.0_f32.max(s) };
            c.rect(left, y, spans[j].1 - left, thick, color, 0.0);
        }
        i = j + 1;
    }
}

/// `/model`'s list, while it's open, between `above` and `bottom` over the
/// input (and its chips): a line saying its keys, then each model Flow offers, numbered as
/// `/model` takes them, the one in use marked ●, the one chosen lit, and
/// another provider's saying it starts a new conversation; as many as
/// there's room for, around the one chosen. Returns its top.
fn draw_model_pick(c: &mut Canvas<'_>, v: &View<'_>, p: Rect, above: f32, bottom: f32, hits: &mut Vec<(Rect, Hit)>) -> f32 {
    let Some(chosen) = v.ai.model_pick else { return bottom };
    let s = v.layout.scale;
    let ch = c.cell.height;
    let list = crate::ai::provider::model_list(v.provider, v.ai.live_for(v.provider));
    let chosen = chosen.min(list.len().saturating_sub(1));
    let room = (((bottom - above - 16.0 * s) / ch).floor() as usize).saturating_sub(1).max(1);
    let shown = list.len().min(room);
    let first = crate::panel_input::Laid::follow(0, chosen, list.len(), shown);
    let h = (shown + 1) as f32 * ch + 12.0 * s;
    let card = Rect::new(p.x + 8.0 * s, bottom - h - 4.0 * s, p.w - 16.0 * s, h);
    c.rect(card.x, card.y, card.w, card.h, rgba([255, 255, 255], 0.05), 8.0 * s);
    c.outline(card.x, card.y, card.w, card.h, rgba(ACCENT, 0.35), 8.0 * s, 1.0);
    let x = card.x + 10.0 * s;
    let mut y = card.y + 6.0 * s;
    let title = c.text(x, y, t("panel.models_title"), rgba(FOREGROUND, 1.0), true);
    let mut keys = c.text(title + c.cell.width, y, t("panel.models_keys"), rgba(DIM, 0.85), false);
    // Where they don't all fit, which are in view.
    if shown < list.len() {
        let place = tf("panel.models_shown", &[("from", &(first + 1)), ("to", &(first + shown)), ("n", &list.len())]);
        keys = c.text(keys + c.cell.width, y, &format!("· {place}"), rgba(DIM, 0.85), false);
    }
    // The provider's own list, being asked for, or not come.
    let company = v.provider.company();
    let failed = v.ai.live.as_ref().is_some_and(|l| l.provider == v.provider && l.failed);
    let asked = if v.ai.models_asking {
        Some(tf("panel.models_asking", &[("company", &company)]))
    } else {
        failed.then(|| tf("panel.models_unanswered", &[("company", &company)]))
    };
    if let Some(asked) = asked {
        c.text(keys + c.cell.width, y, &format!("· {asked}"), rgba(if failed { WARN } else { DIM }, 0.85), false);
    }
    y += ch;
    for (i, m) in list.iter().enumerate().skip(first).take(shown) {
        let row = Rect::new(card.x + 4.0 * s, y, card.w - 8.0 * s, ch);
        let lit = i == chosen;
        if lit || row.contains(v.hover) {
            c.rect(row.x, row.y, row.w, row.h, rgba(ACCENT, if lit { 0.22 } else { 0.10 }), 4.0 * s);
        }
        let in_use = m.provider == v.provider && m.id == v.model;
        let mut tx = c.text(x, y, if in_use { "● " } else { "  " }, rgba(GOOD, 1.0), false);
        tx = c.text(tx, y, &format!("{}. {}", i + 1, m.full_name()), rgba(FOREGROUND, 1.0), lit);
        if m.name != m.id {
            tx = c.text(tx + c.cell.width, y, &m.id, rgba(DIM, 0.8), false);
        }
        let note = if m.provider != v.provider {
            Some(t("panel.models_new_conversation").to_string())
        } else if m.listed {
            Some(tf("panel.models_listed", &[("company", &company)]))
        } else {
            None
        };
        if let Some(note) = note {
            c.text(tx + c.cell.width, y, &format!("· {note}"), rgba(DIM, 0.7), false);
        }
        hits.push((row, Hit::ModelPick(i)));
        y += ch;
    }
    card.y - 4.0 * s
}

/// The panel's input at the foot of panel `p`: a box of the rows its text
/// wraps to, up to a third of the panel's height, scrolled to keep the
/// caret's row in view, with what's selected under the text and what the
/// input method is composing spliced in at the caret (`panel_input`).
/// Keeps where it all went for the keys and the mouse. Returns the top of
/// the chips above it and where the caret is, for the input method.
fn draw_input(c: &mut Canvas<'_>, v: &View<'_>, p: Rect, hits: &mut Vec<(Rect, Hit)>) -> (f32, Option<Rect>) {
    use crate::panel_input::{self, Laid};
    let ai = v.ai;
    let s = v.layout.scale;
    let (cw, ch) = (c.cell.width, c.cell.height);
    let (cell, em) = (c.cell.width, c.cell.font_size);
    let width = |chr: char| crate::text::advance(chr, cell, em);
    let (input_x, input_w) = (p.x + 8.0 * s, p.w - 16.0 * s);
    let text_x = input_x + 10.0 * s + c.text_width("› ");
    let attach = t("attach.button");
    let attach_x = input_x + input_w - 8.0 * s - small_button_w(c, attach, s);
    let room = ((attach_x - 8.0 * s) - text_x).max(4.0 * cw);

    // What's shown: the text, with what the input method composes at the
    // caret, each composed character marked Some(converting).
    let preedit = v.preedit.filter(|_| ai.focus);
    let marked = preedit.map(|p| p.marked());
    let typed = ai.input.chars();
    let at = ai.input.caret();
    let chars: std::borrow::Cow<'_, [char]> = match &marked {
        Some(marked) => typed[..at].iter().copied().chain(marked.iter().map(|&(chr, _)| chr)).chain(typed[at..].iter().copied()).collect::<Vec<_>>().into(),
        None => typed.into(),
    };
    let composed = marked.as_ref().map_or(at..at, |m| at..at + m.len());
    let caret_at = match (&marked, preedit) {
        (Some(m), Some(p)) => at + p.caret().unwrap_or(m.len()),
        _ => at,
    };
    let mut laid = ai.input_laid.borrow_mut();
    let fresh = marked.is_none() && laid.version == ai.input.version() && laid.room == room && (laid.cell, laid.em) == (cell, em) && !laid.rows.is_empty();
    let rows = if fresh { laid.rows.clone() } else { panel_input::rows(&chars, width, room) };
    let most = ((p.h / 3.0) / ch).floor().max(1.0) as usize;
    let shown = rows.len().min(most);
    let caret_row = panel_input::row_of(&rows, caret_at);
    let first = Laid::follow(laid.first, caret_row, rows.len(), shown);
    let input_h = shown as f32 * ch + 14.0 * s;
    let input = Rect::new(input_x, p.y + p.h - input_h - 8.0 * s, input_w, input_h);
    let top = input.y + 7.0 * s;
    (laid.room, laid.cell, laid.em, laid.first, laid.shown) = (room, cell, em, first, shown);
    (laid.left, laid.top, laid.row_h, laid.area) = (text_x, top, ch, input);
    if marked.is_none() {
        laid.version = ai.input.version();
        laid.rows = rows.clone();
    }
    drop(laid);

    c.rect(input.x, input.y, input.w, input.h, rgba([255, 255, 255], if ai.focus { 0.07 } else { 0.035 }), 10.0 * s);
    if ai.focus {
        c.outline(input.x, input.y, input.w, input.h, rgba(ACCENT, 0.45), 10.0 * s, 1.0);
    }
    if first == 0 {
        c.text(input_x + 10.0 * s, top, "› ", rgba(ACCENT, 1.0), true);
    }
    // Attach, at the input's right end, on its last row.
    small_button(c, v, (attach_x, top + (shown - 1) as f32 * ch), attach, DIM, Hit::Attach, hits);
    // The files to go with the next message, in rows of chips above it.
    let chips_top = draw_chips(c, v, Rect::new(input.x, p.y, input.w, input.y - p.y), hits);
    let along = |from: usize, to: usize| chars[from..to.max(from)].iter().map(|&chr| width(chr)).sum::<f32>();
    let blink = 0.55 + 0.45 * (v.time * 4.0).sin().abs();
    if chars.is_empty() {
        let plan = ai.awaiting.and_then(|i| ai.entries.get(i)).is_some_and(|e| matches!(e, Entry::Tool(card) if card.plan));
        let placeholder = if plan {
            t("panel.placeholder_plan").to_string()
        } else if ai.awaiting.is_some() {
            t("panel.placeholder_approve").to_string()
        } else if ai.busy {
            tf("panel.placeholder_busy", &[("name", &v.provider.name())])
        } else {
            t("panel.placeholder").to_string()
        };
        c.text(text_x, top, &placeholder, rgba(DIM, 0.6), false);
        if ai.focus {
            c.rect(text_x, top, 2.0 * s, ch, rgba(ACCENT, blink), 1.0);
            return (chips_top, Some(Rect::new(text_x, top, cw, ch)));
        }
        return (chips_top, None);
    }
    // What's selected, under the text; none while the input method
    // composes. A line break selected shows as half a cell after its row.
    let selection = ai.input.selection().filter(|_| marked.is_none());
    for (k, row) in rows.iter().enumerate().skip(first).take(shown) {
        let y = top + (k - first) as f32 * ch;
        if let Some(sel) = &selection {
            let (from, to) = (sel.start.max(row.start), sel.end.min(row.end));
            let tail = if row.broken && sel.start <= row.end && sel.end > row.end { cw * 0.5 } else { 0.0 };
            if to > from || tail > 0.0 {
                let x0 = text_x + along(row.start, from);
                let x1 = (text_x + along(row.start, to) + tail).min(text_x + room + cw);
                c.rect(x0, y, x1 - x0, ch, rgba(ACCENT, if ai.focus { 0.28 } else { 0.14 }), 2.0 * s);
            }
        }
        let mut cx = text_x;
        let mut spans = Vec::new();
        for (i, &chr) in chars.iter().enumerate().take(row.end).skip(row.start) {
            if cx > text_x + room {
                break;
            }
            let next = c.text(cx, y, chr.encode_utf8(&mut [0; 4]), rgba(FOREGROUND, 1.0), false);
            if composed.contains(&i) {
                let converting = marked.as_ref().is_some_and(|m| m[i - composed.start].1);
                spans.push((cx, next, Some(converting)));
            }
            cx = next;
        }
        underline_composed(c, y + c.cell.baseline + 2.0, &spans, rgba(FOREGROUND, 0.9), s);
    }
    if !ai.focus || !(first..first + shown).contains(&caret_row) {
        return (chips_top, None);
    }
    let y = top + (caret_row - first) as f32 * ch;
    let x = (text_x + along(rows[caret_row].start, caret_at)).min(text_x + room);
    // No caret while the input method converts.
    if preedit.is_none_or(|p| p.caret().is_some()) {
        c.rect(x, y, 2.0 * s, ch, rgba(ACCENT, blink), 1.0);
    }
    (chips_top, Some(Rect::new(x, y, cw, ch)))
}

/// A line of typed text from `at`, with what the input method is composing
/// spliced in at `caret` (in chars) and underlined, heavier where it's being
/// converted. It scrolls sideways to keep the caret within `room`. Returns
/// where the caret is drawn, and the character the line is drawn from.
fn input_line(c: &mut Canvas<'_>, at: (f32, f32), room: f32, typed: &str, caret: usize, preedit: Option<&Preedit>, s: f32) -> (f32, usize) {
    input_line_in(c, at, room, (typed, caret), preedit, s, FOREGROUND)
}

/// As [`input_line`], in `color`: the title bar's, which follows a light
/// theme as the glass cards don't.
fn input_line_in(c: &mut Canvas<'_>, at: (f32, f32), room: f32, (typed, caret): (&str, usize), preedit: Option<&Preedit>, s: f32, color: [u8; 3]) -> (f32, usize) {
    let (x, y) = at;
    // Each character, and whether it's composed: Some(true) if it's being converted.
    let mut chars: Vec<(char, Option<bool>)> = typed.chars().map(|chr| (chr, None)).collect();
    let mut shown = caret.min(chars.len());
    if let Some(p) = preedit {
        let marked = p.marked();
        let n = marked.len();
        chars.splice(shown..shown, marked.into_iter().map(|(chr, converting)| (chr, Some(converting))));
        shown += p.caret().unwrap_or(n);
    }
    let widths: Vec<f32> = chars.iter().map(|&(chr, _)| c.text_width(chr.encode_utf8(&mut [0; 4]))).collect();
    // Drop characters from the front until the caret fits.
    let mut start = 0;
    while start < shown && widths[start..shown].iter().sum::<f32>() + c.cell.width > room {
        start += 1;
    }
    let mut cx = x;
    let mut caret_x = None;
    let mut spans = Vec::new();
    for (i, &(chr, mark)) in chars.iter().enumerate().skip(start) {
        if i == shown {
            caret_x = Some(cx);
        }
        if cx + widths[i] > x + room {
            break;
        }
        let next = c.text(cx, y, chr.encode_utf8(&mut [0; 4]), rgba(color, 1.0), false);
        spans.push((cx, next, mark));
        cx = next;
    }
    underline_composed(c, y + c.cell.baseline + 2.0, &spans, rgba(color, 0.9), s);
    (caret_x.unwrap_or(cx), start)
}

/// Target pixel position of the terminal cursor (the app eases toward it),
/// on a double line among cells twice as wide.
pub fn cursor_target(layout: &Layout, session: &Session) -> (f32, f32) {
    let emu = session.emu.lock();
    let grid = emu.term.grid();
    let p = grid.cursor.point;
    let span = if line_size::of_row(&grid[p.line]).doubled() { 2.0 } else { 1.0 };
    (
        layout.term.x + p.column.0 as f32 * layout.cell_w * span,
        layout.term.y + p.line.0.max(0) as f32 * layout.cell_h,
    )
}

#[derive(Default)]
struct Row {
    spans: Vec<(String, Rgba, bool)>,
    card: Option<usize>,
    buttons: bool,
    /// The task offered's Carry on and Not now.
    resume: bool,
    /// A plan's Let it work and Ask each time, in place of Run, Edit and
    /// Skip.
    plan: bool,
    /// A call waiting for the browser: Try again and Give up.
    browser: bool,
    /// The assistant's reply it's a line of: its entry.
    reply: Option<usize>,
    /// The buttons under a block of code: its entry, the block, and
    /// whether it's commands to run.
    code: Option<(usize, usize, bool)>,
    /// A small Copy at the end of the row: a line of nothing but a span of
    /// code's, or a command card's.
    copy: Option<Hit>,
    /// How it goes on from the row above, where the wrap carried it down.
    wrap: Option<text::Wrapped>,
}

fn style_color(style: Style, base: Rgba) -> (Rgba, bool) {
    match style {
        Style::Normal => (base, false),
        Style::Bold => (base, true),
        Style::Code => (rgba(CODE, 1.0), false),
        Style::Dim => (rgba(DIM, 1.0), false),
        Style::Link(_) => (rgba(ACCENT, 1.0), false),
    }
}

fn push_lines(rows: &mut Vec<Row>, (lines, wraps): (Vec<text::Line>, Vec<Option<text::Wrapped>>), base: Rgba, prefix: Option<(&str, Rgba)>, card: Option<usize>) {
    // A line the wrap carried down copies without the prefix's room too.
    let lead = prefix.map_or(0, |(p, _)| p.chars().count());
    for (i, (line, wrap)) in lines.into_iter().zip(wraps).enumerate() {
        let mut spans = Vec::new();
        if let Some((p, col)) = prefix {
            if i == 0 {
                spans.push((p.to_string(), col, true));
            } else {
                spans.push((" ".repeat(p.chars().count()), col, false));
            }
        }
        for (s, st) in line {
            let (col, bold) = style_color(st, base);
            spans.push((s, col, bold));
        }
        let wrap = wrap.map(|w| text::Wrapped { indent: w.indent + lead, ..w });
        rows.push(Row { spans, card, wrap, ..Row::default() });
    }
}

/// The panel's rows. `room` is the columns a small Copy at the end of a row
/// takes, which a line of nothing but code and a command card's leave free.
fn layout_entries(ai: &AiPanel, cols: usize, now: Instant, time: f32, room: usize) -> Vec<Row> {
    let mut rows: Vec<Row> = Vec::new();
    let fg = rgba(FOREGROUND, 0.95);
    for (idx, entry) in ai.entries.iter().enumerate() {
        if !rows.is_empty() {
            rows.push(Row::default());
        }
        match entry {
            Entry::User(t) => {
                push_lines(&mut rows, text::wrap_plain_wraps(t, cols - 2, Style::Bold), rgba(ACCENT, 1.0), Some(("› ", rgba(ACCENT, 1.0))), None)
            }
            // Each block of code with its buttons under it, and each line of
            // nothing but a span of code with its Copy at its end. While the
            // reply streams in, its last line may yet go on past a span, so
            // it's one once a newline ends it.
            Entry::Assistant(t) => {
                let whole = !(ai.busy && idx + 1 == ai.entries.len());
                let (lines, blocks, wraps) = text::layout_markdown_wraps(t, cols, room, whole);
                for (i, (line, wrap)) in lines.into_iter().zip(wraps).enumerate() {
                    push_lines(&mut rows, (vec![line], vec![wrap]), fg, None, None);
                    rows.last_mut().unwrap().reply = Some(idx);
                    for (b, block) in blocks.iter().enumerate().filter(|(_, block)| block.lines.end == i + 1) {
                        if block.inline {
                            rows.last_mut().unwrap().copy = Some(Hit::CodeCopy(idx, b));
                        } else {
                            rows.push(Row { reply: Some(idx), code: Some((idx, b, block.is_shell())), ..Row::default() });
                        }
                    }
                }
            }
            Entry::Thinking { text: t, started, ended } => match ended {
                Some(end) => {
                    let secs = end.duration_since(*started).as_secs_f32();
                    rows.push(Row { spans: vec![(tf("panel.thought", &[("secs", &format!("{secs:.0}"))]), rgba(DIM, 0.8), false)], ..Row::default() });
                }
                None => {
                    let secs = now.duration_since(*started).as_secs();
                    rows.push(Row { spans: vec![(tf("panel.thinking", &[("secs", &secs)]), rgba(ACCENT2, 0.9), false)], ..Row::default() });
                    let (lines, wraps) = text::wrap_plain_wraps(t.trim(), cols - 2, Style::Dim);
                    let shown = lines.len().saturating_sub(3);
                    let mut wraps: Vec<_> = wraps.into_iter().skip(shown).collect();
                    // The first line shown goes on from none that is.
                    if let Some(first) = wraps.first_mut() {
                        *first = None;
                    }
                    push_lines(&mut rows, (lines.into_iter().skip(shown).collect(), wraps), rgba(DIM, 0.75), Some(("  ", rgba(DIM, 0.5))), None);
                }
            },
            Entry::Tool(card) => {
                let (icon, col) = match card.status {
                    CardStatus::Waiting => ("◆", rgba(WARN, 1.0)),
                    CardStatus::Queued => ("◇", rgba(DIM, 1.0)),
                    CardStatus::Running => ("◌", rgba(ACCENT, 1.0)),
                    CardStatus::InBrowser => ("↗", rgba(ACCENT2, 1.0)),
                    CardStatus::Done { ok: true } => ("✓", rgba(GOOD, 1.0)),
                    CardStatus::Done { ok: false } => ("✗", rgba(BAD, 1.0)),
                    CardStatus::Skipped => ("–", rgba(DIM, 1.0)),
                };
                // A command that ran has its Copy at the end of its line, one
                // waiting beside Run, Edit and Skip.
                let ran = card.command.is_some() && matches!(card.status, CardStatus::Done { .. });
                let width = if ran { (cols - 4).saturating_sub(room).max(cols / 2) } else { cols - 4 };
                push_lines(&mut rows, text::wrap_plain_wraps(&card.title, width, Style::Code), rgba(CODE, 1.0), Some((&format!(" {icon} "), col)), Some(idx));
                if ran {
                    rows.last_mut().unwrap().copy = Some(Hit::CardCopy(idx));
                } else if card.undo.is_some() && card.status == (CardStatus::Done { ok: true }) {
                    rows.last_mut().unwrap().copy = Some(Hit::CardUndo(idx));
                }
                let mut detail = card.detail.clone();
                if let Some(risk) = card.risk {
                    if !detail.is_empty() {
                        detail.push_str("  · ");
                    }
                    detail.push_str(risk.label());
                }
                if !detail.is_empty() {
                    push_lines(&mut rows, text::wrap_plain_wraps(&detail, cols - 4, Style::Normal), rgba(DIM, 1.0), Some(("   ", col)), Some(idx));
                }
                let status = match card.status {
                    CardStatus::Waiting => None,
                    CardStatus::Queued => Some(t("card.queued").to_string()),
                    CardStatus::Running => {
                        let secs = card.started.map(|s| now.duration_since(s).as_secs()).unwrap_or(0);
                        let spinner = ["◐", "◓", "◑", "◒"][(time * 6.0) as usize % 4];
                        // What it does now, where it says (a helper's command).
                        Some(match card.result.as_str() {
                            "" => tf("card.running", &[("spinner", &spinner), ("secs", &secs)]),
                            what => tf("card.running_what", &[("spinner", &spinner), ("what", &what), ("secs", &secs)]),
                        })
                    }
                    CardStatus::InBrowser | CardStatus::Done { .. } | CardStatus::Skipped => Some(card.result.clone()),
                };
                if let Some(st) = status {
                    push_lines(&mut rows, text::wrap_plain_wraps(&st, cols - 4, Style::Normal), col, Some(("   ", col)), Some(idx));
                }
                if card.status == CardStatus::Waiting {
                    let copy = card.command.is_some().then_some(Hit::CardCopy(idx));
                    rows.push(Row { card: Some(idx), buttons: true, plan: card.plan, copy, ..Row::default() });
                } else if card.status == CardStatus::InBrowser {
                    rows.push(Row { card: Some(idx), buttons: true, browser: true, ..Row::default() });
                }
            }
            Entry::Search(q) => rows.push(Row { spans: vec![(tf("panel.searching", &[("query", q)]), rgba(ACCENT2, 0.9), false)], ..Row::default() }),
            Entry::Notice(t) => push_lines(&mut rows, text::wrap_plain_wraps(t, cols, Style::Normal), rgba(WARN, 1.0), None, None),
            Entry::Error(t) => push_lines(&mut rows, text::wrap_plain_wraps(t, cols - 2, Style::Normal), rgba(BAD, 1.0), Some(("! ", rgba(BAD, 1.0))), None),
            Entry::Files(names) => {
                // Under the message they went with, not a message of their own.
                if rows.last().is_some_and(|r| r.spans.is_empty()) {
                    rows.pop();
                }
                push_lines(&mut rows, text::wrap_plain_wraps(&names.join(" · "), cols - 2, Style::Normal), rgba(ACCENT, 0.75), Some(("+ ", rgba(ACCENT, 0.75))), None)
            }
            // A task left unfinished: what it was, its progress and what
            // comes next, then Carry on and Not now.
            Entry::Resume { ask, summary, next, restart, own, steps, .. } => {
                let col = rgba(ACCENT2, 1.0);
                let title = tf(if *restart { "resume.after_restart" } else { "resume.unfinished" }, &[("ask", ask)]);
                push_lines(&mut rows, text::wrap_plain_wraps(&title, cols - 4, Style::Bold), col, Some((" ↻ ", col)), None);
                if !summary.is_empty() {
                    push_lines(&mut rows, text::wrap_plain_wraps(summary, cols - 4, Style::Normal), rgba(DIM, 1.0), Some(("   ", col)), None);
                }
                if !next.is_empty() {
                    let next = tf("resume.next", &[("next", next)]);
                    push_lines(&mut rows, text::wrap_plain_wraps(&next, cols - 4, Style::Normal), rgba(DIM, 1.0), Some(("   ", col)), None);
                }
                if let Some((done, n)) = steps {
                    let steps = tf("resume.steps", &[("done", done), ("n", n)]);
                    push_lines(&mut rows, text::wrap_plain_wraps(&steps, cols - 4, Style::Normal), rgba(DIM, 1.0), Some(("   ", col)), None);
                }
                if let Some(minutes) = own {
                    let own = tf("own.resume", &[("minutes", minutes)]);
                    push_lines(&mut rows, text::wrap_plain_wraps(&own, cols - 4, Style::Normal), rgba(GOOD, 1.0), Some(("   ", col)), None);
                }
                rows.push(Row { resume: true, ..Row::default() });
            }
        }
    }
    rows
}

/// How many of a task's steps the panel shows at once.
const STEPS_SHOWN: usize = 5;

/// Which of `steps` the panel shows, at most `most`: those round the one
/// working (else the first to come, else the last), and how many aren't.
pub fn steps_window(steps: &[(String, StepStatus)], most: usize) -> (std::ops::Range<usize>, usize) {
    if steps.len() <= most {
        return (0..steps.len(), 0);
    }
    let at = steps
        .iter()
        .position(|(_, st)| *st == StepStatus::Working)
        .or_else(|| steps.iter().position(|(_, st)| *st == StepStatus::Pending))
        .unwrap_or(steps.len() - 1);
    // One done before it, where there's one, the rest after.
    let start = at.saturating_sub(1).min(steps.len() - most);
    (start..start + most, steps.len() - most)
}

/// What screen readers hear of a task's steps: how many are done, and the
/// one working.
pub fn steps_spoken(steps: &[(String, StepStatus)]) -> Option<String> {
    if steps.is_empty() {
        return None;
    }
    let done = steps.iter().filter(|(_, st)| *st == StepStatus::Done).count();
    let now = steps.iter().find(|(_, st)| *st == StepStatus::Working).map_or("", |(text, _)| text.as_str());
    Some(tf("access.steps", &[("done", &done), ("n", &steps.len()), ("now", &now)]))
}

/// The assistant's panel: its conversation and input. Returns the input's
/// caret while the panel takes the keys.
fn draw_panel(c: &mut Canvas<'_>, v: &View<'_>, p: Rect, hits: &mut Vec<(Rect, Hit)>) -> Option<Rect> {
    let s = v.layout.scale;
    let ai = v.ai;
    let (cw, ch) = (c.cell.width, c.cell.height);
    let radius = 14.0 * s;
    c.steady_glow([p.x, p.y + 6.0 * s, p.w, p.h], rgba([0, 0, 0], 0.35), radius, 22.0 * s);
    if ai.busy {
        let a = 0.22 + 0.12 * (v.time * 2.2).sin();
        c.glow(p.x, p.y, p.w, p.h, rgba(ACCENT2, a), radius, 16.0 * s);
    }
    c.glass(p.x, p.y, p.w, p.h, rgba(PANEL_TINT, 0.72), radius);

    let pad = 14.0 * s;
    let inner_x = p.x + pad;
    let inner_w = p.w - pad * 2.0;
    let cols = ((inner_w / cw).floor() as usize).max(16);
    // A small Copy at a row's end takes its width and a space before it.
    let copy_room = ((small_button_w(c, t("panel.copy"), s) / cw).ceil() as usize) + 1;

    // Header
    let hy = p.y + 10.0 * s;
    let mut x = c.text(inner_x, hy, "✦ ", rgba(ACCENT, 1.0), true);
    x = c.text(x, hy, &tf("panel.name", &[("app", &APP_NAME)]), rgba(FOREGROUND, 1.0), true);
    let (mode, col) = match v.own_left {
        Some(minutes) => (tf("panel.own", &[("minutes", &minutes)]), GOOD),
        None if ai.auto_mode => (t("panel.auto").to_string(), ACCENT2),
        None => (t("panel.guide").to_string(), ACCENT),
    };
    x = c.text(x + cw, hy, &mode, rgba(col, 0.9), false);
    x = c.text(x + cw, hy, v.model, rgba(DIM, 0.7), false);
    // Tokens used, against the cap if there is one: amber from 80%, red at the cap.
    let (used, cap) = (v.tokens_used, v.token_cap);
    let (usage, col) = if cap > 0 {
        let frac = used as f32 / cap as f32;
        let col = if frac >= 1.0 { BAD } else if frac >= 0.8 { WARN } else { DIM };
        (tf("panel.tokens_of_cap", &[("used", &fmt_tokens(used)), ("cap", &fmt_tokens(cap))]), col)
    } else {
        (tf("panel.tokens", &[("used", &fmt_tokens(used))]), DIM)
    };
    c.text(x + cw, hy, &usage, rgba(col, if col == DIM { 0.7 } else { 1.0 }), false);
    let hint = if ai.busy {
        t("panel.hint_busy").to_string()
    } else if ai.focus {
        t(keys("panel.hint_focus_mac", "panel.hint_focus")).to_string()
    } else {
        tf("panel.hint_type", &[("keys", &crate::keys::label(crate::event::Command::ToggleAssistant))])
    };
    let hint = hint.as_str();
    let hw = c.text_width(hint);
    if hw < inner_w * 0.6 {
        c.text(p.x + p.w - pad - hw, hy, hint, rgba(DIM, 0.75), false);
    }
    if ai.busy {
        let sw = c.text_width(t("panel.stop")) + 12.0 * s;
        let r = Rect::new(p.x + p.w - pad - hw - sw - 10.0 * s, hy - 2.0 * s, sw, ch + 4.0 * s);
        hits.push((r, Hit::Stop));
    }

    let (chips_top, caret) = draw_input(c, v, p, hits);

    // The task's steps, pinned under the header, then the conversation,
    // bottom-aligned above the input.
    let mut top = hy + ch + 8.0 * s;
    if !ai.steps.is_empty() {
        let done = ai.steps.iter().filter(|(_, st)| *st == StepStatus::Done).count();
        c.text(inner_x, top, &tf("panel.steps", &[("done", &done), ("n", &ai.steps.len())]), rgba(DIM, 0.9), false);
        top += ch;
        let (shown, more) = steps_window(&ai.steps, STEPS_SHOWN);
        let room = cols.saturating_sub(5);
        for (text, status) in &ai.steps[shown] {
            let (mark, col, a) = match status {
                StepStatus::Done => ("✓", GOOD, 0.85),
                StepStatus::Working => ("◌", ACCENT, 1.0),
                StepStatus::Pending => ("○", DIM, 0.8),
                StepStatus::Skipped => ("–", DIM, 0.6),
            };
            let x = c.text(inner_x + cw, top, mark, rgba(col, a), false);
            let line: String = if text.chars().count() > room { text.chars().take(room.saturating_sub(1)).chain(['…']).collect() } else { text.clone() };
            let working = *status == StepStatus::Working;
            c.text(x + cw, top, &line, rgba(if working { FOREGROUND } else { DIM }, a), working);
            top += ch;
        }
        if more > 0 {
            c.text(inner_x + cw, top, &tf("panel.steps_more", &[("n", &more)]), rgba(DIM, 0.7), false);
            top += ch;
        }
        top += 6.0 * s;
    }
    // `/model`'s list over the input, leaving the conversation a row.
    let chips_top = draw_model_pick(c, v, p, top + ch, chips_top, hits);
    let bottom = chips_top - 6.0 * s;
    let visible_rows = ((bottom - top) / ch).floor().max(1.0) as usize;
    let now = Instant::now();
    let rows = if ai.entries.is_empty() {
        let mut r = Vec::new();
        let intro = if v.has_key {
            t("panel.intro").to_string()
        } else {
            tf("panel.intro_no_key", &[("company", &v.provider.company()), ("keys", &crate::keys::label(crate::event::Command::Settings))])
        };
        push_lines(&mut r, text::wrap_plain_wraps(&intro, cols, Style::Normal), rgba(DIM, 0.9), None, None);
        r
    } else {
        layout_entries(ai, cols, now, v.time, copy_room)
    };
    let total = rows.len();
    let max_scroll = total.saturating_sub(visible_rows);
    let scroll = ai.scroll.min(max_scroll);
    let end = total - scroll;
    let start = end.saturating_sub(visible_rows);
    let mut y = bottom - (end - start) as f32 * ch;

    // Card backgrounds first so text sits on top.
    let mut i = start;
    while i < end {
        if let Some(card) = rows[i].card {
            let mut j = i;
            while j + 1 < end && rows[j + 1].card == Some(card) {
                j += 1;
            }
            let cy = y + (i - start) as f32 * ch - 3.0 * s;
            let chh = (j - i + 1) as f32 * ch + 6.0 * s;
            let waiting = matches!(ai.entries.get(card), Some(Entry::Tool(t)) if t.status == CardStatus::Waiting);
            c.rect(inner_x - 6.0 * s, cy, inner_w + 12.0 * s, chh, rgba([255, 255, 255], 0.045), 8.0 * s);
            if waiting {
                c.outline(inner_x - 6.0 * s, cy, inner_w + 12.0 * s, chh, rgba(WARN, 0.5), 8.0 * s, 1.0);
            }
            i = j + 1;
        } else {
            i += 1;
        }
    }
    // What the mouse and Copy need, and the selection under the text.
    {
        let mut laid = ai.laid.borrow_mut();
        laid.cols = cols;
        laid.rows = rows.iter().map(|row| row.spans.iter().map(|(s, ..)| s.as_str()).collect()).collect();
        laid.wraps = rows.iter().map(|row| row.wrap.clone()).collect();
        laid.area = Rect::new(p.x, top, p.w, bottom - top);
        (laid.start, laid.top, laid.row_h) = (start, y, ch);
        laid.edges = rows[start..end]
            .iter()
            .map(|row| {
                let mut x = inner_x;
                let mut edges = vec![x];
                for ch in row.spans.iter().flat_map(|(s, ..)| s.chars()) {
                    x += c.text_width(ch.encode_utf8(&mut [0; 4]));
                    edges.push(x);
                }
                edges
            })
            .collect();
        if let Some((a, b)) = ai.selection {
            let (from, to) = if a <= b { (a, b) } else { (b, a) };
            for (i, edges) in laid.edges.iter().enumerate() {
                let row = start + i;
                if row < from.0 || row > to.0 {
                    continue;
                }
                let first = if row == from.0 { from.1 } else { 0 }.min(edges.len() - 1);
                let last = if row == to.0 { to.1 } else { edges.len() - 1 }.min(edges.len() - 1);
                if last > first {
                    c.rect(edges[first], y + i as f32 * ch, edges[last] - edges[first], ch, rgba(ACCENT, 0.28), 2.0 * s);
                }
            }
        }
    }
    // The reply under the pointer, whose Copy shows on a line in view.
    let hovered = (v.hover.0 >= p.x && v.hover.0 < p.x + p.w && v.hover.1 >= y && v.hover.1 < bottom)
        .then(|| rows.get(start + ((v.hover.1 - y) / ch) as usize))
        .flatten()
        .and_then(|row| row.reply);
    let reply_copy_at = reply_copy_row(&rows[start..end], hovered);
    let mut reply_copy = None;
    for (i, row) in rows[start..end].iter().enumerate() {
        let mut x = inner_x;
        if reply_copy_at == Some(i) {
            reply_copy = Some((hovered.unwrap(), y));
        }
        if let Some((entry, block, shell)) = row.code {
            // Small buttons, under the block's text.
            x += 2.0 * cw;
            let run = shell.then(|| (t("panel.run"), Hit::CodeRun(entry, block), GOOD));
            for (label, hit, col) in [(t("panel.copy"), Hit::CodeCopy(entry, block), ACCENT)].into_iter().chain(run) {
                x = small_button(c, v, (x, y), label, col, hit, hits) + 6.0 * s;
            }
        } else if row.buttons || row.resume {
            x += 3.0 * cw;
            let approval = [(t("card.run"), Hit::Approve, GOOD), (t("card.edit"), Hit::Edit, ACCENT), (t("card.skip"), Hit::Skip, DIM)];
            let resume = [(t("resume.carry_on"), Hit::ResumeOn, GOOD), (t("resume.later"), Hit::ResumeLater, DIM)];
            let plan = [(t("own.let"), Hit::Approve, GOOD), (t("own.each_time"), Hit::Skip, DIM)];
            let browser = [(t("card.try_again"), Hit::CardRetry, GOOD), (t("card.give_up"), Hit::CardGiveUp, DIM)];
            let buttons = if row.resume {
                &resume[..]
            } else if row.browser {
                &browser[..]
            } else if row.plan {
                &plan[..]
            } else {
                &approval[..]
            };
            for &(label, hit, col) in buttons {
                let w = c.text_width(label) + 16.0 * s;
                let r = Rect::new(x, y + 1.0 * s, w, ch - 2.0 * s);
                let hovered = r.contains(v.hover);
                c.rect(r.x, r.y, r.w, r.h, rgba(col, if hovered { 0.32 } else { 0.18 }), r.h * 0.5);
                c.text(r.x + 8.0 * s, y, label, rgba(col, 1.0), true);
                hits.push((r, hit));
                x += w + 8.0 * s;
            }
        } else {
            for (s_text, col, bold) in &row.spans {
                x = c.text(x, y, s_text, *col, *bold);
            }
            x += cw;
        }
        // After the row's text, or a waiting card's buttons, kept within
        // the panel where it's laid out narrower than its room.
        if let Some(hit) = row.copy {
            let label = t(if matches!(hit, Hit::CardUndo(_)) { "panel.undo" } else { "panel.copy" });
            let x = x.min(inner_x + inner_w - small_button_w(c, label, s));
            small_button(c, v, (x, y), label, ACCENT, hit, hits);
        }
        y += ch;
    }
    if let Some((entry, ry)) = reply_copy {
        let label = t("panel.copy_reply");
        let w = c.text_width(label) + 14.0 * s;
        let r = Rect::new(inner_x + inner_w - w, ry + 2.0 * s, w, ch - 4.0 * s);
        let lit = r.contains(v.hover);
        c.rect(r.x - 4.0 * s, r.y - 2.0 * s, r.w + 8.0 * s, r.h + 4.0 * s, rgba(PANEL_TINT, 0.95), r.h * 0.5);
        c.rect(r.x, r.y, r.w, r.h, rgba(ACCENT, if lit { 0.30 } else { 0.14 }), r.h * 0.5);
        c.text(r.x + 7.0 * s, ry, label, rgba(ACCENT, if lit { 1.0 } else { 0.85 }), false);
        hits.push((r, Hit::ReplyCopy(entry)));
    }
    if scroll > 0 {
        let label = tf("panel.more", &[("count", &scroll)]);
        let w = c.text_width(&label);
        c.text(p.x + p.w - pad - w, bottom - ch, &label, rgba(DIM, 0.9), false);
    }
    caret
}

/// Which of the rows in view takes Copy reply at its end, for the reply
/// `hovered`: its first without a Copy of its own, which Copy reply would
/// cover and take the clicks of, or none.
fn reply_copy_row(rows: &[Row], hovered: Option<usize>) -> Option<usize> {
    rows.iter().position(|row| row.reply.is_some() && row.reply == hovered && row.copy.is_none())
}

/// The width of a small button in the panel's text, as [`small_button`]
/// draws it.
/// The files to go with the next message, a chip each (`AiPanel::files`),
/// laid in rows from the foot of `room` (the panel above its input) up,
/// and those still being read, dim: a picture's thumbnail or its kind's
/// mark, its name cut to fit, its size, and a ✕. One that looks like a
/// secret is orange and asks for a click before it may go. Returns the
/// top of the chips, or the foot of `room` where there are none.
fn draw_chips(c: &mut Canvas<'_>, v: &View<'_>, room: Rect, hits: &mut Vec<(Rect, Hit)>) -> f32 {
    use crate::ai::files::Kind;
    let ai = v.ai;
    if ai.files.is_empty() && ai.reading.is_empty() {
        return room.y + room.h;
    }
    let s = v.layout.scale;
    let (cw, ch) = (c.cell.width, c.cell.height);
    let h = ch + 8.0 * s;
    let gap = 6.0 * s;
    // Each chip's parts, measured first, so the rows can be laid from the
    // foot.
    struct Chip {
        name: String,
        detail: String,
        mark: &'static str,
        thumb: Option<std::sync::Arc<crate::graphics::Image>>,
        secret: bool,
        reading: bool,
        w: f32,
    }
    let most_name = ((room.w * 0.5 / cw) as usize).max(8);
    let cut = |name: &str| if name.chars().count() > most_name { name.chars().take(most_name - 1).chain(['…']).collect() } else { name.to_string() };
    let mut chips: Vec<Chip> = Vec::new();
    for f in &ai.files {
        let secret = !f.ready();
        let detail = if secret { t("attach.secret").to_string() } else { f.shown_size() };
        let mark = match f.kind {
            Kind::Picture { .. } => "▣",
            Kind::Pdf { .. } => "▤",
            Kind::Text { .. } => "≡",
            Kind::Office { .. } => "▦",
        };
        chips.push(Chip { name: cut(&f.name), detail, mark, thumb: f.thumb.clone(), secret, reading: false, w: 0.0 });
    }
    for name in &ai.reading {
        chips.push(Chip { name: cut(name), detail: t("attach.reading").to_string(), mark: "…", thumb: None, secret: false, reading: true, w: 0.0 });
    }
    let pic = h - 6.0 * s;
    for chip in &mut chips {
        let lead = if chip.thumb.is_some() { pic + 6.0 * s } else { c.text_width(chip.mark) + 6.0 * s };
        chip.w = (8.0 * s + lead + c.text_width(&chip.name) + 8.0 * s + c.text_width(&chip.detail) + 8.0 * s + c.text_width("✕") + 8.0 * s).min(room.w);
    }
    // Rows, from the first chip down, then placed above the input.
    let mut rows: Vec<Vec<usize>> = vec![Vec::new()];
    let mut x = 0.0;
    for (i, chip) in chips.iter().enumerate() {
        if x > 0.0 && x + chip.w > room.w {
            rows.push(Vec::new());
            x = 0.0;
        }
        rows.last_mut().unwrap().push(i);
        x += chip.w + gap;
    }
    let top = room.y + room.h - gap - rows.len() as f32 * (h + gap);
    for (r, row) in rows.iter().enumerate() {
        let y = top + r as f32 * (h + gap);
        let mut x = room.x;
        for &i in row {
            let chip = &chips[i];
            let rect = Rect::new(x, y, chip.w, h);
            let col = if chip.secret { ORANGE } else { ACCENT };
            let lit = rect.contains(v.hover);
            c.rect(rect.x, rect.y, rect.w, rect.h, rgba(col, if lit { 0.22 } else { 0.12 }), h * 0.3);
            c.outline(rect.x, rect.y, rect.w, rect.h, rgba(col, if chip.secret { 0.7 } else { 0.35 }), h * 0.3, 1.0);
            let ty = y + (h - ch) * 0.5;
            let mut tx = x + 8.0 * s;
            match &chip.thumb {
                Some(thumb) => {
                    let (tw, th) = (thumb.width as f32, thumb.height as f32);
                    let k = pic / tw.max(th);
                    let (dw, dh) = (tw * k, th * k);
                    c.image_over(thumb, (tx + (pic - dw) * 0.5, y + 3.0 * s + (pic - dh) * 0.5, dw, dh), if chip.reading { 0.5 } else { 1.0 });
                    tx += pic + 6.0 * s;
                }
                None => tx = c.text(tx, ty, chip.mark, rgba(col, 0.9), false) + 6.0 * s,
            }
            let a = if chip.reading { 0.55 } else { 1.0 };
            tx = c.text(tx, ty, &chip.name, rgba(FOREGROUND, 0.95 * a), false) + 8.0 * s;
            c.text(tx, ty, &chip.detail, rgba(if chip.secret { ORANGE } else { DIM }, 0.9 * a), false);
            if !chip.reading {
                let xw = c.text_width("✕") + 8.0 * s;
                let close = Rect::new(rect.x + rect.w - xw - 4.0 * s, y, xw, h);
                let over = close.contains(v.hover);
                c.text(close.x + 4.0 * s, ty, "✕", rgba(if over { FOREGROUND } else { DIM }, 1.0), false);
                if chip.secret {
                    hits.push((Rect::new(rect.x, rect.y, close.x - rect.x, rect.h), Hit::AttachConfirm(i)));
                }
                hits.push((close, Hit::AttachRemove(i)));
            }
            x += chip.w + gap;
        }
    }
    top
}

/// Where a file dragged over the window would land, lit, with what
/// dropping it there does: the assistant's panel (or the room it opens
/// in), or the pane under the pointer.
fn draw_drop(c: &mut Canvas<'_>, v: &View<'_>, r: Rect, words: &str) {
    let s = v.layout.scale;
    let ch = c.cell.height;
    c.rect(r.x, r.y, r.w, r.h, rgba(ACCENT, 0.10), 12.0 * s);
    c.outline(r.x, r.y, r.w, r.h, rgba(ACCENT, 0.8), 12.0 * s, 2.0 * s);
    let w = c.text_width(words) + 24.0 * s;
    let (x, y) = (r.x + (r.w - w) * 0.5, r.y + (r.h - ch) * 0.5);
    // Solid, so what's under it doesn't show through its words.
    let pill = ch + 16.0 * s;
    c.rect(x, y - 8.0 * s, w, pill, rgba(PANEL_TINT, 0.97), pill * 0.5);
    c.outline(x, y - 8.0 * s, w, pill, rgba(ACCENT, 0.8), pill * 0.5, 1.0);
    c.text(x + 12.0 * s, y, words, rgba(FOREGROUND, 1.0), true);
}

fn small_button_w(c: &Canvas<'_>, label: &str, s: f32) -> f32 {
    c.text_width(label) + 14.0 * s
}

/// A small button on the panel's row at `y`, from `x`, lit under the
/// pointer, as Copy and Run under a block of code are. Returns its right
/// edge.
fn small_button(c: &mut Canvas<'_>, v: &View<'_>, (x, y): (f32, f32), label: &str, col: [u8; 3], hit: Hit, hits: &mut Vec<(Rect, Hit)>) -> f32 {
    let s = v.layout.scale;
    let ch = c.cell.height;
    let r = Rect::new(x, y + 2.0 * s, small_button_w(c, label, s), ch - 4.0 * s);
    let lit = r.contains(v.hover);
    c.rect(r.x, r.y, r.w, r.h, rgba(col, if lit { 0.30 } else { 0.14 }), r.h * 0.5);
    c.text(r.x + 7.0 * s, y, label, rgba(col, if lit { 1.0 } else { 0.85 }), false);
    hits.push((r, hit));
    r.x + r.w
}

/// Where the link under the pointer goes, and the click that opens it, in
/// a pill at the terminal's foot: at its left, or at its right while the
/// pointer is where the left one would be, as a browser shows a link's
/// address. A program can put any words over a link, so this is how to
/// tell where one goes before opening it.
fn draw_link_target(c: &mut Canvas<'_>, v: &View<'_>, uri: &str) {
    let (l, s) = (v.layout, v.layout.scale);
    let room = (l.term.w - 32.0 * s).max(c.cell.width * 8.0);
    // Cut to about what fits before measuring, as a link may be long.
    let most = (room / (c.cell.width * 0.5).max(1.0)) as usize;
    let words = match crate::link::target(uri) {
        Some(crate::link::Target::Reveal(path)) => {
            let path: String = path.display().to_string().chars().take(most).collect();
            tf(keys("link.reveals_mac", "link.reveals"), &[("path", &path)])
        }
        _ => tf(keys("link.opens_mac", "link.opens"), &[("uri", &uri.chars().take(most).collect::<String>())]),
    };
    let words = fit(c, &words, room - 20.0 * s);
    let (w, h) = (c.text_width(&words) + 20.0 * s, c.cell.height + 10.0 * s);
    let y = l.term.y + l.term.h - h - 6.0 * s;
    let left = l.term.x + 6.0 * s;
    let x = if Rect::new(left, y, w, h).contains(v.hover) { (l.term.x + l.term.w - w - 6.0 * s).max(left) } else { left };
    c.glass(x, y, w, h, rgba(PANEL_TINT, 0.9), h * 0.5);
    c.text(x + 10.0 * s, y + 5.0 * s, &words, rgba(FOREGROUND, 1.0), false);
}

fn draw_toast(c: &mut Canvas<'_>, v: &View<'_>, t: &str) {
    let s = v.layout.scale;
    let w = c.text_width(t) + 28.0 * s;
    let h = c.cell.height + 14.0 * s;
    let x = (v.layout.width - w) * 0.5;
    let y = v.layout.title.h + 10.0 * s;
    c.glow(x, y, w, h, rgba(ACCENT, 0.25), h * 0.5, 14.0 * s);
    c.glass(x, y, w, h, rgba(PANEL_TINT, 0.85), h * 0.5);
    c.text(x + 14.0 * s, y + 7.0 * s, t, rgba(FOREGROUND, 1.0), false);
}

/// Caps Lock's notice, in the middle of the terminal's foot: orange, its
/// glow steady whatever the Glow setting says, so it can't be missed. Over
/// the cursor's row it rises above it, leaving what's typed in view.
fn draw_caps_lock(c: &mut Canvas<'_>, v: &View<'_>, cursor: Option<Rect>) {
    let l = v.layout;
    let s = l.scale;
    let words = t("terminal.caps_lock");
    let (w, h) = (c.text_width(words) + 32.0 * s, c.cell.height + 16.0 * s);
    let x = l.term.x + (l.term.w - w) * 0.5;
    let mut y = l.term.y + l.term.h - h - 12.0 * s;
    if let Some(at) = cursor.filter(|r| r.y + r.h > y && r.y < y + h) {
        y = (at.y - h - 8.0 * s).max(l.term.y);
    }
    c.steady_glow([x, y, w, h], rgba(ORANGE, 0.7), h * 0.5, 22.0 * s);
    c.glass(x, y, w, h, rgba(PANEL_TINT, 0.92), h * 0.5);
    c.outline(x, y, w, h, rgba(ORANGE, 0.95), h * 0.5, 1.5 * s);
    c.text(x + 16.0 * s, y + 8.0 * s, words, rgba(ORANGE, 1.0), true);
}

fn draw_key_dialog(c: &mut Canvas<'_>, v: &View<'_>, d: &KeyDialog) {
    let s = v.layout.scale;
    let (cw, ch) = (c.cell.width, c.cell.height);
    // Dim everything behind the dialog.
    c.rect(0.0, 0.0, v.layout.width, v.layout.height, [0.0, 0.0, 0.0, 0.35], 0.0);
    let w = (640.0 * s).min(v.layout.width - 40.0 * s);
    let pad = 24.0 * s;
    let cols = ((w - pad * 2.0) / cw) as usize;
    let store = t(if cfg!(windows) {
        "key_dialog.store_windows"
    } else if cfg!(target_os = "macos") {
        "key_dialog.store_mac"
    } else {
        "key_dialog.store_other"
    });
    let p = v.provider;
    let company = p.company();
    let starts = match p {
        Provider::Gemini => String::new(),
        _ => tf("key_dialog.starts_with", &[("prefix", &p.key_prefix())]),
    };
    let body = match &d.server {
        Some(server) => tf("key_dialog.server_body", &[("server", server), ("store", &store)]),
        None => tf("key_dialog.body", &[("company", &company), ("starts", &starts), ("store", &store), ("console", &p.console())]),
    };
    let body = text::wrap_plain(&body, cols, Style::Normal);
    let (title, status) = d.title_and_status(p);
    let col = if d.checking {
        ACCENT
    } else if d.error.is_some() {
        BAD
    } else {
        DIM
    };
    let status = text::wrap_plain(&status, cols, Style::Normal);
    // Tall enough for the words, which run longer in some languages.
    let box_h = ch + 16.0 * s;
    let h = pad * 2.0 + ch * 1.6 + (body.len() + status.len()) as f32 * ch + ch * 1.2 + box_h;
    let x = (v.layout.width - w) * 0.5;
    let y = (v.layout.height - h) * 0.5;
    c.glow(x, y, w, h, rgba(ACCENT, 0.22), 18.0 * s, 30.0 * s);
    c.glass(x, y, w, h, rgba(PANEL_TINT, 1.0), 18.0 * s);
    let mut ty = y + pad;
    let tx = c.text(x + pad, ty, "✦ ", rgba(ACCENT, 1.0), true);
    c.text(tx, ty, &title, rgba(FOREGROUND, 1.0), true);
    ty += ch * 1.6;
    for line in body {
        c.text(x + pad, ty, &text::line_text(&line), rgba(DIM, 1.0), false);
        ty += ch;
    }
    ty += ch * 0.6;
    c.rect(x + pad, ty, w - pad * 2.0, box_h, rgba([255, 255, 255], 0.07), 10.0 * s);
    c.outline(x + pad, ty, w - pad * 2.0, box_h, rgba(ACCENT, 0.5), 10.0 * s, 1.0);
    let shown = if d.input.is_empty() {
        String::new()
    } else {
        crate::secrets::mask(&d.input)
    };
    let ix = x + pad + 12.0 * s;
    let iy = ty + 8.0 * s;
    if shown.is_empty() && d.server.is_none() {
        c.text(ix, iy, &format!("{}…", p.key_prefix()), rgba(DIM, 0.5), false);
    }
    let end = c.text(ix, iy, &shown, rgba(FOREGROUND, 1.0), false);
    let a = 0.55 + 0.45 * (v.time * 4.0).sin().abs();
    c.rect(if shown.is_empty() { ix } else { end }, iy, 2.0 * s, ch, rgba(ACCENT, a), 1.0);
    ty += box_h + ch * 0.6;
    for line in status {
        c.text(x + pad, ty, &text::line_text(&line), rgba(col, 1.0), false);
        ty += ch;
    }
}

/// An MCP server's question: its server at the head, its message, then a
/// form's fields (each its title, what it's for, its control and any
/// problem Send found) or a link's whole address, its host lit, and Send
/// (or Open) and Decline. The fields scroll to keep the one with the keys
/// in view. Hands back the caret of a field being typed in.
fn draw_server_ask(c: &mut Canvas<'_>, v: &View<'_>, a: &ServerAsk, hits: &mut Vec<(Rect, Hit)>) -> Option<Rect> {
    use crate::mcp::client::elicit::{Answer, Kind};
    let s = v.layout.scale;
    let (cw, ch) = (c.cell.width, c.cell.height);
    c.rect(0.0, 0.0, v.layout.width, v.layout.height, [0.0, 0.0, 0.0, 0.35], 0.0);
    let w = (680.0 * s).min(v.layout.width - 40.0 * s);
    let pad = 24.0 * s;
    let cols = ((w - pad * 2.0) / cw).max(8.0) as usize;
    let message: Vec<_> = text::wrap_plain(&a.message, cols, Style::Normal).into_iter().take(8).collect();
    let (row_h, box_h, button_h) = ((ch * 1.5).round(), ch + 12.0 * s, ch + 14.0 * s);
    let focus = a.focus.min(a.items() - 1);
    // Each field's height: its title (a switch's is on its own row), what
    // it's for, its control and a problem.
    let laid: Vec<(f32, Vec<String>)> = match &a.body {
        AskBody::Link(_) => Vec::new(),
        AskBody::Form { form, .. } => form
            .fields
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let about: Vec<String> = if f.description.trim().is_empty() { Vec::new() } else { text::wrap_plain(&f.description, cols, Style::Normal).iter().take(2).map(text::line_text).collect() };
                let control = match &f.kind {
                    Kind::Switch => row_h,
                    Kind::One(options) | Kind::Many { options, .. } => {
                        let width: f32 = options.iter().map(|o| c.text_width(&o.title) + cw * 4.0).sum();
                        row_h * (width / (w - pad * 2.0)).ceil().max(1.0)
                    }
                    _ => box_h,
                };
                let title = if matches!(f.kind, Kind::Switch) { 0.0 } else { ch };
                let problem = if a.problems.get(i).is_some_and(Option::is_some) { ch } else { 0.0 };
                (title + about.len() as f32 * ch + control + problem + ch * 0.5, about)
            })
            .collect(),
    };
    let url_lines: Vec<String> = match &a.body {
        AskBody::Link(link) => link.url.chars().collect::<Vec<_>>().chunks(cols.saturating_sub(2).max(8)).map(|c| c.iter().collect()).collect(),
        AskBody::Form { .. } => Vec::new(),
    };
    let warning = match &a.body {
        AskBody::Link(link) => link.warning(),
        AskBody::Form { .. } => None,
    };
    let fixed = pad * 2.0 + ch * 1.6 + message.len() as f32 * ch + ch * 1.2 + button_h + ch * 1.4;
    let link_h = if url_lines.is_empty() { 0.0 } else { url_lines.len() as f32 * ch + ch * 0.8 + if warning.is_some() { ch * 1.2 } else { 0.0 } };
    let room = (v.layout.height - 40.0 * s - fixed - link_h).max(row_h);
    // The fields shown: from the first that keeps the one with the keys in view.
    let target = focus.min(laid.len().saturating_sub(1));
    let mut first = 0;
    while first < target && laid[first..=target].iter().map(|(h, _)| h).sum::<f32>() > room {
        first += 1;
    }
    let mut shown = first;
    let mut fields_h = 0.0;
    while shown < laid.len() && fields_h + laid[shown].0 <= room.max(laid[first].0) {
        fields_h += laid[shown].0;
        shown += 1;
    }
    let h = fixed + link_h + fields_h;
    let (x, y) = ((v.layout.width - w) * 0.5, (v.layout.height - h) * 0.5);
    c.glow(x, y, w, h, rgba(WARN, 0.18), 18.0 * s, 30.0 * s);
    c.glass(x, y, w, h, rgba(PANEL_TINT, 1.0), 18.0 * s);
    hits.push((Rect::new(x, y, w, h), Hit::AskCard));
    let mut ty = y + pad;
    let tx = c.text(x + pad, ty, "◆ ", rgba(WARN, 1.0), true);
    c.text(tx, ty, &fit(c, &a.title(), x + w - pad - tx), rgba(FOREGROUND, 1.0), true);
    ty += ch * 1.6;
    for line in &message {
        c.text(x + pad, ty, &text::line_text(line), rgba(DIM, 1.0), false);
        ty += ch;
    }
    ty += ch * 0.6;
    let mut caret = None;
    let lit = |focused: bool| rgba(if focused { ACCENT } else { FOREGROUND }, if focused { 0.7 } else { 0.25 });
    match &a.body {
        AskBody::Link(link) => {
            let (before, host, _) = link.around_host();
            let (from, to) = (before.chars().count(), before.chars().count() + host.chars().count());
            c.rect(x + pad, ty - ch * 0.2, w - pad * 2.0, url_lines.len() as f32 * ch + ch * 0.4, rgba([255, 255, 255], 0.05), 8.0 * s);
            let mut at = 0;
            for line in &url_lines {
                let mut lx = x + pad + cw;
                for ch_ in line.chars() {
                    let in_host = (from..to).contains(&at);
                    lx = c.text(lx, ty, ch_.encode_utf8(&mut [0; 4]), rgba(if in_host { ACCENT } else { CODE }, 1.0), in_host);
                    at += 1;
                }
                ty += ch;
            }
            ty += ch * 0.8;
            if let Some(warning) = &warning {
                c.text(x + pad, ty, &fit(c, warning, w - pad * 2.0), rgba(BAD, 1.0), true);
                ty += ch * 1.2;
            }
        }
        AskBody::Form { form, answers, texts, picks } => {
            for i in first..shown {
                let (f, answer) = (&form.fields[i], &answers[i]);
                let focused = focus == i;
                let title = if f.required { format!("{} *", f.title) } else { f.title.clone() };
                if !matches!(f.kind, Kind::Switch) {
                    c.text(x + pad, ty, &fit(c, &title, w - pad * 2.0), rgba(if focused { FOREGROUND } else { DIM }, 1.0), focused);
                    ty += ch;
                }
                for line in &laid[i].1 {
                    c.text(x + pad, ty, line, rgba(DIM, 0.75), false);
                    ty += ch;
                }
                match (&f.kind, answer) {
                    (Kind::Switch, Answer::Switch(on)) => {
                        let r = Rect::new(x + pad - 8.0 * s, ty, w - pad * 2.0 + 16.0 * s, row_h);
                        if focused || r.contains(v.hover) {
                            c.rect(r.x, r.y, r.w, r.h, rgba([255, 255, 255], if focused { 0.08 } else { 0.05 }), 8.0 * s);
                        }
                        let my = ty + (row_h - ch) * 0.5;
                        let mx = c.text(x + pad, my, if *on { "☑" } else { "☐" }, rgba(if *on { ACCENT } else { DIM }, 1.0), false);
                        c.text(mx + cw, my, &fit(c, &title, x + w - pad - mx - cw), rgba(if *on || focused { FOREGROUND } else { DIM }, 1.0), focused);
                        hits.push((r, Hit::AskField(i as u8)));
                        ty += row_h;
                    }
                    (Kind::One(options) | Kind::Many { options, .. }, answer) => {
                        let (mut ox, mut oy) = (x + pad, ty);
                        for (k, o) in options.iter().enumerate() {
                            let on = match answer {
                                Answer::One(picked) => *picked == Some(k),
                                Answer::Many(on) => on.get(k).copied().unwrap_or(false),
                                _ => false,
                            };
                            let mark = match (answer, on) {
                                (Answer::One(_), true) => "●",
                                (Answer::One(_), false) => "○",
                                (_, true) => "☑",
                                _ => "☐",
                            };
                            let label = format!("{mark} {}", o.title);
                            let ow = c.text_width(&label) + cw * 2.0;
                            if ox + ow > x + w - pad && ox > x + pad {
                                (ox, oy) = (x + pad, oy + row_h);
                            }
                            let r = Rect::new(ox, oy + 2.0 * s, ow, row_h - 4.0 * s);
                            let here = focused && picks[i] == k;
                            c.rect(r.x, r.y, r.w, r.h, rgba(if on { ACCENT } else { FOREGROUND }, if on { 0.18 } else if r.contains(v.hover) { 0.1 } else { 0.05 }), r.h * 0.5);
                            if here {
                                c.outline(r.x, r.y, r.w, r.h, rgba(ACCENT, 0.8), r.h * 0.5, 1.5 * s);
                            }
                            c.text(ox + cw, oy + (row_h - ch) * 0.5, &label, rgba(if on { FOREGROUND } else { DIM }, 1.0), on);
                            hits.push((r, Hit::AskOption(i as u8, k as u8)));
                            ox += ow + cw;
                        }
                        ty = oy + row_h;
                    }
                    (_, Answer::Text(value)) => {
                        let r = Rect::new(x + pad, ty, w - pad * 2.0, box_h);
                        c.rect(r.x, r.y, r.w, r.h, rgba([255, 255, 255], 0.07), 8.0 * s);
                        c.outline(r.x, r.y, r.w, r.h, lit(focused), 8.0 * s, 1.0);
                        hits.push((r, Hit::AskField(i as u8)));
                        let (ix, iy) = (r.x + 10.0 * s, r.y + (box_h - ch) * 0.5);
                        // Keep the caret in view along a long line.
                        let chars: Vec<char> = value.chars().collect();
                        let fits = ((r.w - 20.0 * s) / cw).max(4.0) as usize;
                        let at = texts.get(i).map_or(chars.len(), |f| f.caret().1).min(chars.len());
                        let start = at.saturating_sub(fits.saturating_sub(1));
                        let seen: String = chars[start..chars.len().min(start + fits)].iter().collect();
                        c.text(ix, iy, &seen, rgba(FOREGROUND, 1.0), false);
                        if focused {
                            let before: String = chars[start..at].iter().collect();
                            let cx = ix + c.text_width(&before);
                            let blink = 0.55 + 0.45 * (v.time * 4.0).sin().abs();
                            c.rect(cx, iy, 2.0 * s, ch, rgba(ACCENT, blink), 1.0);
                            caret = Some(Rect::new(cx, iy, 2.0 * s, ch));
                        }
                        ty += box_h;
                    }
                    _ => {}
                }
                if let Some(why) = a.problems.get(i).cloned().flatten() {
                    c.text(x + pad, ty, &fit(c, &why, w - pad * 2.0), rgba(BAD, 1.0), false);
                    ty += ch;
                }
                ty += ch * 0.5;
            }
        }
    }
    ty += ch * 0.6;
    let (yes, no) = a.button_words();
    let mut bx = x + pad;
    let send = a.items() - 2;
    for (i, (label, hit, accent)) in [(yes, Hit::AskSend, true), (no, Hit::AskDecline, false)].into_iter().enumerate() {
        let bw = c.text_width(label) + 28.0 * s;
        let r = Rect::new(bx, ty, bw, button_h);
        let focused = focus == send + i;
        let color = if accent { ACCENT } else { FOREGROUND };
        c.rect(r.x, r.y, r.w, r.h, rgba(color, if r.contains(v.hover) || focused { 0.24 } else { 0.14 }), button_h * 0.5);
        c.outline(r.x, r.y, r.w, r.h, rgba(color, if focused { 0.9 } else { 0.5 }), button_h * 0.5, if focused { 2.0 * s } else { 1.0 });
        c.text(bx + 14.0 * s, ty + (button_h - ch) * 0.5, label, rgba(color, 1.0), accent);
        hits.push((r, hit));
        bx += bw + 10.0 * s;
    }
    let keys = match &a.body {
        AskBody::Link(_) => t("server_ask.keys_link"),
        AskBody::Form { .. } => t("server_ask.keys_form"),
    };
    c.text(x + pad, y + h - pad - ch, &fit(c, keys, w - pad * 2.0), rgba(DIM, 0.6), false);
    caret
}

/// Whether to read the file a program named, to show its 3D model: the tab,
/// the file, and Allow and Don't allow.
fn draw_model_ask(c: &mut Canvas<'_>, v: &View<'_>, a: &ModelAsk, hits: &mut Vec<(Rect, Hit)>) {
    let s = v.layout.scale;
    let (cw, ch) = (c.cell.width, c.cell.height);
    c.rect(0.0, 0.0, v.layout.width, v.layout.height, [0.0, 0.0, 0.0, 0.35], 0.0);
    let w = (640.0 * s).min(v.layout.width - 40.0 * s);
    let pad = 24.0 * s;
    let cols = ((w - pad * 2.0) / cw) as usize;
    let body = text::wrap_plain(&tf("model_ask.body", &[("tab", &a.tab)]), cols, Style::Normal);
    let path = a.ask.path.to_string_lossy();
    // The file's name matters most, so a long path keeps its end.
    let path: String = if path.chars().count() > cols { format!("…{}", path.chars().skip(path.chars().count() + 1 - cols).collect::<String>()) } else { path.into_owned() };
    let button_h = ch + 14.0 * s;
    let h = pad * 2.0 + ch * 1.6 + body.len() as f32 * ch + ch * 2.0 + button_h + ch * 1.4;
    let (x, y) = ((v.layout.width - w) * 0.5, (v.layout.height - h) * 0.5);
    c.glow(x, y, w, h, rgba(WARN, 0.18), 18.0 * s, 30.0 * s);
    c.glass(x, y, w, h, rgba(PANEL_TINT, 1.0), 18.0 * s);
    hits.push((Rect::new(x, y, w, h), Hit::ModelCard));
    let mut ty = y + pad;
    let tx = c.text(x + pad, ty, "◆ ", rgba(WARN, 1.0), true);
    c.text(tx, ty, t("model_ask.title"), rgba(FOREGROUND, 1.0), true);
    ty += ch * 1.6;
    for line in body {
        c.text(x + pad, ty, &text::line_text(&line), rgba(DIM, 1.0), false);
        ty += ch;
    }
    ty += ch * 0.4;
    c.text(x + pad, ty, &path, rgba(CODE, 1.0), false);
    ty += ch * 1.6;
    let mut bx = x + pad;
    for (label, hit, accent) in [(t("model_ask.allow"), Hit::ModelAllow, true), (t("model_ask.deny"), Hit::ModelDeny, false)] {
        let bw = c.text_width(label) + 28.0 * s;
        let r = Rect::new(bx, ty, bw, button_h);
        let hovered = r.contains(v.hover);
        let color = if accent { ACCENT } else { FOREGROUND };
        c.rect(r.x, r.y, r.w, r.h, rgba(color, if hovered { 0.24 } else { 0.14 }), button_h * 0.5);
        c.outline(r.x, r.y, r.w, r.h, rgba(color, 0.5), button_h * 0.5, 1.0);
        c.text(bx + 14.0 * s, ty + (button_h - ch) * 0.5, label, rgba(color, 1.0), accent);
        hits.push((r, hit));
        bx += bw + 10.0 * s;
    }
    c.text(x + pad, y + h - pad - ch, t("model_ask.keys"), rgba(DIM, 0.6), false);
}

/// An outside app's card: the first time it calls, what it may do, each
/// kind with a switch, and Allow and Deny; or a command it asks to run, with
/// Run and Skip. Over everything, as a program's question is.
fn draw_mcp_card(c: &mut Canvas<'_>, v: &View<'_>, card: &McpCard, hits: &mut Vec<(Rect, Hit)>) {
    use crate::mcp::tools::Kind;
    let s = v.layout.scale;
    let (cw, ch) = (c.cell.width, c.cell.height);
    c.rect(0.0, 0.0, v.layout.width, v.layout.height, [0.0, 0.0, 0.0, 0.35], 0.0);
    let w = (680.0 * s).min(v.layout.width - 40.0 * s);
    let pad = 24.0 * s;
    let cols = ((w - pad * 2.0) / cw) as usize;
    let host = if card.host.is_empty() { t("outside.host_unknown").to_string() } else { card.host.clone() };
    let (title, body, buttons) = match &card.ask {
        McpAsk::Use { .. } => (tf("outside.use_title", &[("app", &card.app)]), tf("outside.use_body", &[("app", &card.app), ("host", &host)]), [t("outside.allow"), t("outside.deny")]),
        McpAsk::Command { .. } => (tf("outside.command_title", &[("app", &card.app)]), tf("outside.command_body", &[("app", &card.app), ("host", &host)]), [t("outside.run"), t("outside.skip")]),
    };
    let body = text::wrap_plain(&body, cols, Style::Normal);
    let row_h = (ch * 1.5).round();
    let (extra, command) = match &card.ask {
        McpAsk::Use { .. } => (row_h * Kind::ALL.len() as f32 + ch * 0.4, Vec::new()),
        McpAsk::Command { command, risk } => {
            let lines: Vec<String> = command.lines().flat_map(|l| text::wrap_plain(l, cols.saturating_sub(2), Style::Normal)).map(|l| text::line_text(&l)).take(6).collect();
            let risky = *risk == crate::ai::tools::Risk::High;
            (lines.len() as f32 * ch + ch * 0.8 + if risky { ch * 1.2 } else { 0.0 }, lines)
        }
    };
    let button_h = ch + 14.0 * s;
    let h = pad * 2.0 + ch * 1.6 + body.len() as f32 * ch + ch * 0.6 + extra + ch * 0.6 + button_h + ch * 1.4;
    let (x, y) = ((v.layout.width - w) * 0.5, (v.layout.height - h) * 0.5);
    c.glow(x, y, w, h, rgba(WARN, 0.18), 18.0 * s, 30.0 * s);
    c.glass(x, y, w, h, rgba(PANEL_TINT, 1.0), 18.0 * s);
    hits.push((Rect::new(x, y, w, h), Hit::McpCard));
    let mut ty = y + pad;
    let tx = c.text(x + pad, ty, "◆ ", rgba(WARN, 1.0), true);
    c.text(tx, ty, &fit(c, &title, x + w - pad - tx), rgba(FOREGROUND, 1.0), true);
    ty += ch * 1.6;
    for line in body {
        c.text(x + pad, ty, &text::line_text(&line), rgba(DIM, 1.0), false);
        ty += ch;
    }
    ty += ch * 0.6;
    match &card.ask {
        McpAsk::Use { kinds } => {
            for (i, kind) in Kind::ALL.iter().enumerate() {
                let r = Rect::new(x + pad - 8.0 * s, ty, w - pad * 2.0 + 16.0 * s, row_h);
                if r.contains(v.hover) {
                    c.rect(r.x, r.y, r.w, r.h, rgba([255, 255, 255], 0.05), 8.0 * s);
                }
                let on = kinds[i];
                let mark = if on { "☑" } else { "☐" };
                let my = ty + (row_h - ch) * 0.5;
                let mx = c.text(x + pad, my, mark, rgba(if on { ACCENT } else { DIM }, 1.0), false);
                let words = t(&format!("outside.kind.{}", kind.key()));
                c.text(mx + cw, my, &fit(c, &format!("{}  {words}", i + 1), x + w - pad - mx - cw), rgba(if on { FOREGROUND } else { DIM }, 1.0), false);
                hits.push((r, Hit::McpKind(i as u8)));
                ty += row_h;
            }
            ty += ch * 0.4;
        }
        McpAsk::Command { risk, .. } => {
            c.rect(x + pad, ty - ch * 0.2, w - pad * 2.0, command.len() as f32 * ch + ch * 0.4, rgba([255, 255, 255], 0.05), 8.0 * s);
            for line in &command {
                c.text(x + pad + cw, ty, line, rgba(CODE, 1.0), false);
                ty += ch;
            }
            ty += ch * 0.8;
            if *risk == crate::ai::tools::Risk::High {
                c.text(x + pad, ty, t("outside.high_risk"), rgba(BAD, 1.0), true);
                ty += ch * 1.2;
            }
        }
    }
    ty += ch * 0.6;
    let mut bx = x + pad;
    for (i, label) in buttons.iter().enumerate() {
        let (hit, accent) = if i == 0 { (Hit::McpAllow, true) } else { (Hit::McpDeny, false) };
        let bw = c.text_width(label) + 28.0 * s;
        let r = Rect::new(bx, ty, bw, button_h);
        let hovered = r.contains(v.hover);
        let color = if accent { ACCENT } else { FOREGROUND };
        c.rect(r.x, r.y, r.w, r.h, rgba(color, if hovered { 0.24 } else { 0.14 }), button_h * 0.5);
        c.outline(r.x, r.y, r.w, r.h, rgba(color, 0.5), button_h * 0.5, 1.0);
        c.text(bx + 14.0 * s, ty + (button_h - ch) * 0.5, label, rgba(color, 1.0), accent);
        hits.push((r, hit));
        bx += bw + 10.0 * s;
    }
    let keys = match card.ask {
        McpAsk::Use { .. } => t("outside.use_keys"),
        McpAsk::Command { .. } => t("outside.command_keys"),
    };
    c.text(x + pad, y + h - pad - ch, keys, rgba(DIM, 0.6), false);
}

/// The command history's search: the words typed, the filters, and the
/// commands found, each with its folder, how it ended and when it last ran.
/// Returns the caret in the search box, for an input method.
/// The command palette, high over the middle of the window: what's typed,
/// then what matches it, best first, each with what kind of thing it is,
/// where it is (a setting's section) and its keys. Hands back its caret.
fn draw_palette(c: &mut Canvas<'_>, v: &View<'_>, p: &crate::command_palette::Palette, hits: &mut Vec<(Rect, Hit)>) -> Rect {
    let s = v.layout.scale;
    let ch = c.cell.height;
    c.rect(0.0, 0.0, v.layout.width, v.layout.height, [0.0, 0.0, 0.0, 0.3], 0.0);
    let pad = 16.0 * s;
    let row_h = (ch * 1.7).round();
    let w = (720.0 * s).min(v.layout.width - 32.0 * s);
    let box_h = ch + 16.0 * s;
    let room = (v.layout.height * 0.72 - box_h - pad * 3.0 - ch * 1.4).max(row_h);
    let visible = ((room / row_h) as usize).clamp(1, 12);
    let h = pad * 3.0 + box_h + row_h * visible as f32 + ch * 1.4;
    let (x, y) = ((v.layout.width - w) * 0.5, (v.layout.height * 0.12).max(v.layout.title.h + 8.0 * s));
    c.glow(x, y, w, h, rgba(ACCENT, 0.2), 18.0 * s, 30.0 * s);
    c.glass(x, y, w, h, rgba(PANEL_TINT, 1.0), 14.0 * s);
    hits.push((Rect::new(x, y, w, h), Hit::PaletteCard));
    // What's typed.
    let ty = y + pad;
    c.rect(x + pad, ty, w - pad * 2.0, box_h, rgba([255, 255, 255], 0.07), 10.0 * s);
    c.outline(x + pad, ty, w - pad * 2.0, box_h, rgba(ACCENT, 0.5), 10.0 * s, 1.0);
    let (ix, iy) = (x + pad + 12.0 * s, ty + 8.0 * s);
    let ix = c.text(ix, iy, "› ", rgba(ACCENT, 1.0), true);
    if p.query.is_empty() {
        c.text(ix, iy, t("palette.placeholder"), rgba(DIM, 0.55), false);
    }
    let mut shown = p.query.clone();
    while !shown.is_empty() && c.text_width(&shown) > x + w - pad * 2.0 - ix - c.cell.width {
        shown.remove(0);
    }
    let end = c.text(ix, iy, &shown, rgba(FOREGROUND, 1.0), false);
    let caret = Rect::new(end, iy, 2.0 * s, ch);
    let a = 0.55 + 0.45 * (v.time * 4.0).sin().abs();
    c.rect(caret.x, caret.y, caret.w, caret.h, rgba(ACCENT, a), 1.0);
    // What matches, the choice kept in view.
    let mut top = p.top.get().min(p.found.len().saturating_sub(1));
    if p.selected < top {
        top = p.selected;
    } else if p.selected >= top + visible {
        top = p.selected + 1 - visible;
    }
    p.top.set(top);
    let list_y = ty + box_h + pad * 0.6;
    if p.found.is_empty() {
        c.text(x + pad, list_y + (row_h - ch) * 0.5, t("palette.nothing"), rgba(DIM, 1.0), false);
    }
    for (row, &i) in p.found.iter().enumerate().skip(top).take(visible) {
        let item = &p.items[i];
        let r = Rect::new(x + pad * 0.5, list_y + (row - top) as f32 * row_h, w - pad, row_h - 2.0 * s);
        let chosen = row == p.selected;
        if chosen {
            c.rect(r.x, r.y, r.w, r.h, rgba(ACCENT, 0.16), 8.0 * s);
        } else if r.contains(v.hover) {
            c.rect(r.x, r.y, r.w, r.h, rgba([255, 255, 255], 0.05), 8.0 * s);
        }
        hits.push((r, Hit::PaletteItem(row)));
        let ly = r.y + (r.h - ch) * 0.5;
        // On the right: its keys, and what kind of thing it is.
        let mut right = r.x + r.w - 10.0 * s;
        if !item.keys.is_empty() {
            right -= c.text_width(&item.keys);
            c.text(right, ly, &item.keys, rgba(ACCENT, 0.8), false);
            right -= 14.0 * s;
        }
        let kind = item.kind.words();
        right -= c.text_width(kind);
        c.text(right, ly, kind, rgba(DIM, 0.8), false);
        // On the left: its name, and a setting's section.
        let room = right - 16.0 * s - (r.x + 10.0 * s);
        let label = if item.detail.is_empty() { item.label.clone() } else { format!("{} · {}", item.label, item.detail) };
        c.text(r.x + 10.0 * s, ly, &fit(c, &label, room), rgba(FOREGROUND, if chosen { 1.0 } else { 0.85 }), chosen);
    }
    c.text(x + pad, y + h - pad * 0.5 - ch, t("palette.hint"), rgba(DIM, 0.7), false);
    caret
}

/// `text`, cut short from its start with "…" to fit `room` pixels, as a
/// folder's end says more than its start.
fn fit_left(c: &Canvas<'_>, text: &str, room: f32) -> String {
    if c.text_width(text) <= room {
        return text.to_string();
    }
    let mut cut: String = text.to_string();
    while !cut.is_empty() && c.text_width(&cut) + c.text_width("…") > room {
        cut.remove(0);
    }
    format!("…{cut}")
}

/// The tab sidebar (`tab_sidebar`): a card of glass down the window's left
/// side, from under the title bar to the window's foot, slid out as far as
/// `sb.out` says, over the terminal. Its rows are each group's header (its
/// fold, its color, its name and how many tabs it holds), each tab (its
/// name and, under it, what it's doing), set in under its group with a
/// line in its color, and a split tab's panes under it where it's folded
/// open. The tab in front is lit as its pill is, the row the keys are on is
/// outlined, and that row and the one under the pointer show their … and
/// ×. The list scrolls to keep the chosen row in view, a bar at its edge
/// saying where it is. At its foot, New Tab and New Group; a row's menu
/// opens under its …. Hands back the caret of a name being typed in a
/// row, the first row shown, and the card's right edge.
fn draw_sidebar(c: &mut Canvas<'_>, v: &View<'_>, sb: &SidebarView, hits: &mut Vec<(Rect, Hit)>) -> (Option<Rect>, usize, Option<f32>) {
    let l = v.layout;
    let s = l.scale;
    let ch = c.cell.height;
    let w = sb.width;
    let margin = crate::tab_sidebar::MARGIN * s;
    let top_y = l.title.h + 4.0 * s;
    let h = (l.height - top_y - margin).max(ch * 4.0);
    let x = margin - (w + margin * 2.0) * (1.0 - sb.out);
    c.glow(x, top_y, w, h, rgba(ACCENT, 0.16 * sb.out), 14.0 * s, 26.0 * s);
    c.glass(x, top_y, w, h, rgba(PANEL_TINT, 1.0), 12.0 * s);
    hits.push((Rect::new(x, top_y, w, h), Hit::SidebarCard));
    let pad = 8.0 * s;

    // At its foot, New Tab and New Group, sharing the width, then its pin.
    let foot_h = ch + 12.0 * s;
    let foot_y = top_y + h - pad - foot_h;
    let pin = Rect::new(x + w - pad - foot_h, foot_y, foot_h, foot_h);
    // Each button as wide as its words and a little more, sharing what's
    // short where they don't both fit.
    let labels: Vec<(String, Hit)> = [("menu.new_tab", Hit::SidebarNewTab), ("menu.new_group", Hit::SidebarNewGroup)]
        .into_iter()
        .map(|(key, hit)| {
            let label = match hit {
                Hit::SidebarNewGroup if sb.picked > 1 => format!("{} · {}", t(key), sb.picked),
                _ => t(key).to_string(),
            };
            (label, hit)
        })
        .collect();
    let room = (pin.x - 6.0 * s - (x + pad) - 6.0 * s).max(0.0);
    let natural: Vec<f32> = labels.iter().map(|(l, _)| c.text_width(l) + 14.0 * s).collect();
    let share = (room / natural.iter().sum::<f32>()).min(1.0);
    let spare = room - natural.iter().sum::<f32>() * share;
    let mut bx = x + pad;
    for ((label, hit), want) in labels.into_iter().zip(natural) {
        let r = Rect::new(bx, foot_y, want * share + spare * 0.5, foot_h);
        bx += r.w + 6.0 * s;
        let over = r.contains(v.hover);
        c.rect(r.x, r.y, r.w, r.h, rgba([255, 255, 255], if over { 0.12 } else { 0.06 }), 8.0 * s);
        let label = fit(c, &label, r.w - 10.0 * s);
        c.text(r.x + (r.w - c.text_width(&label)) * 0.5, r.y + (foot_h - ch) * 0.5, &label, rgba(FOREGROUND, if over { 1.0 } else { 0.8 }), false);
        hits.push((r, hit));
    }
    // The pin: a pushpin leaning over, ready to pin it; pinned, upright and
    // pressed in to its collar, lit (the user's word, 2026-11-28).
    let over = pin.contains(v.hover);
    if sb.pinned {
        c.rect(pin.x, pin.y, pin.w, pin.h, rgba(ACCENT, 0.22), 8.0 * s);
        c.outline(pin.x, pin.y, pin.w, pin.h, rgba(ACCENT, 0.6), 8.0 * s, 1.0);
    } else {
        c.rect(pin.x, pin.y, pin.w, pin.h, rgba([255, 255, 255], if over { 0.12 } else { 0.06 }), 8.0 * s);
    }
    // Opaque, dimmed by its color, so its parts don't show through each
    // other where they meet.
    let ink = if sb.pinned {
        rgba(ACCENT, 1.0)
    } else {
        let (f, k) = (rgba(FOREGROUND, 1.0), if over { 1.0 } else { 0.75 });
        [f[0] * k, f[1] * k, f[2] * k, 1.0]
    };
    draw_pin(c, (pin.x + pin.w * 0.5, pin.y + pin.h * 0.5), sb.pinned, ink, s);
    hits.push((pin, Hit::SidebarPin));

    // The rows, from the first shown, the chosen one kept in view.
    let list_y = top_y + pad;
    let list_h = (foot_y - pad * 0.5 - list_y).max(0.0);
    let height = |r: &SidebarRow| match r.kind {
        SidebarKind::Group { .. } => (ch * 1.8).round(),
        _ => (ch * 2.9).round(),
    };
    let n = sb.rows.len();
    let mut top = sb.top.min(n.saturating_sub(1));
    if let Some(chosen) = sb.chosen.filter(|&k| k < n) {
        top = top.min(chosen);
        while top < chosen && sb.rows[top..=chosen].iter().map(height).sum::<f32>() > list_h {
            top += 1;
        }
    }
    // No room left unused at the foot while rows above are hidden.
    while top > 0 && sb.rows[top - 1..].iter().map(height).sum::<f32>() <= list_h {
        top -= 1;
    }
    let mut caret = None;
    let mut menu_at = None;
    let mut y = list_y;
    let mut shown = 0;
    let indent = 14.0 * s;
    // Each row drawn and where, for where a dragged row would land.
    let mut drawn: Vec<(usize, Rect)> = Vec::new();
    for (i, row) in sb.rows.iter().enumerate().skip(top) {
        let rh = height(row);
        if y + rh > list_y + list_h + 0.5 {
            break;
        }
        shown += 1;
        let r = Rect::new(x + pad * 0.5, y, w - pad, rh - 2.0 * s);
        y += rh;
        let chosen = sb.chosen == Some(i);
        let over = r.contains(v.hover);
        let front = matches!(row.kind, SidebarKind::Tab { front: true, .. } | SidebarKind::Pane { front: true, .. });
        let alert = match row.kind {
            SidebarKind::Tab { alert: Some(failed), .. } => Some(if failed { BAD } else { GOOD }),
            _ => None,
        };
        if let Some(col) = alert {
            c.steady_glow([r.x, r.y, r.w, r.h], rgba(col, 0.25), 8.0 * s, 10.0 * s);
            c.rect(r.x, r.y, r.w, r.h, rgba(col, 0.10), 8.0 * s);
        }
        if front {
            c.rect(r.x, r.y, r.w, r.h, rgba(ACCENT, 0.16), 8.0 * s);
        } else if over {
            c.rect(r.x, r.y, r.w, r.h, rgba([255, 255, 255], 0.05), 8.0 * s);
        }
        // The row the keys are on, while it has them.
        if chosen && sb.keys {
            c.outline(r.x, r.y, r.w, r.h, rgba(ACCENT, 0.6), 8.0 * s, 1.0);
        }
        // Picked with others: lit, with a bar at its left.
        if row.picked {
            c.rect(r.x, r.y, r.w, r.h, rgba(ACCENT2, 0.14), 8.0 * s);
            c.rect(r.x + 1.0 * s, r.y + 6.0 * s, (3.0 * s).round(), r.h - 12.0 * s, rgba(ACCENT2, 0.95), 1.5 * s);
        }
        hits.push((r, Hit::SidebarRow(i)));
        let left = r.x + 8.0 * s + row.level as f32 * indent;
        // A group's tab: a line in its color down the indent.
        if let SidebarKind::Tab { color: Some(col), .. } = row.kind {
            let lx = r.x + 8.0 * s + (row.level as f32 - 1.0) * indent + 3.0 * s;
            c.rect(lx, r.y + 4.0 * s, (2.0 * s).round().max(1.0), r.h - 8.0 * s, rgba(col, 0.9), 1.0 * s);
        }
        // Its × and …, on the right, where it's chosen or under the pointer.
        let mut right = r.x + r.w - 4.0 * s;
        let b = (ch * 1.25).round();
        let buttons = (over || (chosen && sb.keys)) && sb.drag.is_none() && !matches!(row.kind, SidebarKind::Pane { .. });
        let name_y = match row.kind {
            SidebarKind::Group { .. } => r.y + (r.h - ch) * 0.5,
            _ => r.y + 5.0 * s,
        };
        if buttons || sb.menu.as_ref().is_some_and(|m| m.row == i) {
            // Every row has its ×: the last tab's asks before it quits Flow.
            for (label, hit) in [("×", Hit::SidebarClose(i)), ("…", Hit::SidebarMore(i))] {
                right -= b;
                let br = Rect::new(right, name_y + (ch - b) * 0.5, b, b);
                if br.contains(v.hover) {
                    c.rect(br.x, br.y, br.w, br.h, rgba([255, 255, 255], 0.12), 6.0 * s);
                }
                c.text(br.x + (b - c.text_width(label)) * 0.5, name_y, label, rgba(FOREGROUND, 0.8), false);
                hits.push((br, hit));
                if matches!(hit, Hit::SidebarMore(_)) {
                    menu_at = menu_at.or(sb.menu.as_ref().filter(|m| m.row == i).map(|_| br));
                }
                right -= 2.0 * s;
            }
        }
        if sb.menu.as_ref().is_some_and(|m| m.row == i) && menu_at.is_none() {
            menu_at = Some(Rect::new(right - b, name_y, b, b));
        }
        // Its fold, where it has one: a group, or a split tab.
        let fold = match row.kind {
            SidebarKind::Group { folded, .. } => Some(!folded),
            SidebarKind::Tab { split, .. } => split,
            _ => None,
        };
        let mut tx = left;
        if let Some(open) = fold {
            let mark = if open { "▾" } else { "▸" };
            let fr = Rect::new(tx - 4.0 * s, name_y - 2.0 * s, c.text_width(mark) + 8.0 * s, ch + 4.0 * s);
            c.text(tx, name_y, mark, rgba(DIM, 1.0), false);
            hits.push((fr, Hit::SidebarFold(i)));
            tx += c.text_width(mark) + 6.0 * s;
        }
        // A name being typed in its row: a field where its name goes, after
        // a group's color and a tab's alert, which stay in view.
        if let Some((_, field)) = sb.naming.as_ref().filter(|(at, _)| *at == i) {
            let fx = match row.kind {
                SidebarKind::Group { .. } => tx + 18.0 * s,
                SidebarKind::Tab { alert: Some(_), .. } => tx + 12.0 * s,
                _ => tx,
            };
            let fr = Rect::new(fx - 4.0 * s, name_y - 3.0 * s, (right - fx).max(40.0 * s), ch + 6.0 * s);
            c.rect(fr.x, fr.y, fr.w, fr.h, rgba([255, 255, 255], 0.08), 6.0 * s);
            c.outline(fr.x, fr.y, fr.w, fr.h, rgba(ACCENT, 0.9), 6.0 * s, 1.5 * s);
            let (cx, _) = input_line_in(c, (fx, name_y), fr.w - 8.0 * s, (&field.text(), field.caret().1), v.preedit, s, FOREGROUND);
            if v.preedit.is_none_or(|p| p.caret().is_some()) {
                c.rect(cx - 0.5 * s, name_y, 2.0 * s, ch, rgba(ACCENT, 1.0), 0.0);
            }
            caret = Some(Rect::new(cx, name_y, c.cell.width, ch));
        }
        let naming = sb.naming.as_ref().is_some_and(|(at, _)| *at == i);
        let room = (right - 6.0 * s - tx).max(0.0);
        match &row.kind {
            SidebarKind::Group { name, color, count, .. } => {
                let d = 10.0 * s;
                c.rect(tx, name_y + (ch - d) * 0.5, d, d, rgba(*color, 0.95), 3.0 * s);
                let nx = tx + d + 8.0 * s;
                let count = count.to_string();
                let cw = if buttons { 0.0 } else { c.text_width(&count) + 6.0 * s };
                let closing = sb.closing == Some(i);
                if !naming {
                    c.text(nx, name_y, &fit(c, name, (room - (nx - tx) - cw).max(0.0)), rgba(if closing { BAD } else { FOREGROUND }, 1.0), true);
                }
                if !buttons {
                    c.text(r.x + r.w - 8.0 * s - c.text_width(&count), name_y, &count, rgba(DIM, 1.0), false);
                }
            }
            SidebarKind::Tab { name, line, front, .. } | SidebarKind::Pane { name, line, front, .. } => {
                let alert = match row.kind {
                    SidebarKind::Tab { alert, .. } => alert,
                    _ => None,
                };
                let mut nx = tx;
                if let Some(failed) = alert {
                    let d = 6.0 * s;
                    c.rect(nx, name_y + (ch - d) * 0.5, d, d, rgba(if failed { BAD } else { GOOD }, 0.95), d);
                    nx += d + 6.0 * s;
                }
                if !naming {
                    let fg = if *front { rgba(ACCENT, 1.0) } else { rgba(FOREGROUND, if over || chosen { 0.95 } else { 0.8 }) };
                    c.text(nx, name_y, &fit(c, name, (room - (nx - tx)).max(0.0)), fg, *front);
                }
                // What it's doing, on the line under its name.
                let ly = name_y + (ch * 1.3).round();
                let lroom = (r.x + r.w - 8.0 * s - tx).max(0.0);
                draw_sidebar_line(c, (tx, ly), lroom, line, s);
            }
        }
        drawn.push((i, r));
        if let Some(d) = &sb.drag {
            // The header a tab dragged would go into, lit in its color.
            if let (Some(_), SidebarKind::Group { color, .. }) = (d.onto.filter(|&k| k == i), &row.kind) {
                c.rect(r.x, r.y, r.w, r.h, rgba(*color, 0.16), 8.0 * s);
                c.outline(r.x, r.y, r.w, r.h, rgba(*color, 0.9), 8.0 * s, 1.5 * s);
            }
            // The row dragged, dimmed where it was.
            if d.row == Some(i) {
                c.rect(r.x, r.y, r.w, r.h, rgba(PANEL_TINT, 0.6), 8.0 * s);
            }
        }
    }
    if let Some(d) = &sb.drag {
        // The gap it would go into: a line, set in and in a group's color
        // where it would join that group, above the row it goes before.
        if let Some((at, color)) = d.gap {
            let y = drawn.iter().find(|(k, _)| *k == at).map(|(_, r)| r.y - 1.0 * s).or_else(|| drawn.last().filter(|(k, _)| k + 1 == at).map(|(_, r)| r.y + r.h + 1.0 * s));
            if let Some(y) = y {
                let col = color.unwrap_or(ACCENT);
                let t = (2.0 * s).round().max(1.0);
                let dot = (7.0 * s).round();
                let lx = x + pad * 0.5 + 8.0 * s + if color.is_some() { indent } else { 0.0 } + dot * 0.5;
                let rx = x + w - pad;
                c.rect(lx, (y - t * 0.5).round(), (rx - lx).max(0.0), t, rgba(col, 0.95), t * 0.5);
                c.outline(lx - dot, (y - dot * 0.5).round(), dot, dot, rgba(col, 0.95), dot * 0.5, t);
            }
        }
    }
    // Where the list is, where it doesn't all fit.
    if shown < n && n > 0 {
        let track_h = list_h;
        let thumb_h = (track_h * shown as f32 / n as f32).max(16.0 * s);
        let thumb_y = list_y + (track_h - thumb_h) * top as f32 / (n - shown).max(1) as f32;
        c.rect(x + w - 4.0 * s, thumb_y, 2.0 * s, thumb_h, rgba(FOREGROUND, 0.3), 1.0 * s);
    }
    // Its right edge, which a drag makes wider or narrower.
    hits.push((Rect::new(x + w - 3.0 * s, top_y + 10.0 * s, 6.0 * s, h - 20.0 * s), Hit::SidebarEdge));
    // A row's menu, under its … (over it where there's no room below).
    if let (Some(menu), Some(at)) = (&sb.menu, menu_at) {
        draw_row_menu(c, v, menu, at, false, hits);
    }
    (caret, top, Some(x + w))
}

/// The name of a tab or a group being dragged, in a chip of glass below
/// and right of the pointer, so the line where it would land stays in
/// view, kept in the window.
fn draw_drag_name(c: &mut Canvas<'_>, v: &View<'_>, name: &str) {
    let l = v.layout;
    let s = l.scale;
    let name = fit(c, name, 240.0 * s);
    let (w, h) = (c.text_width(&name) + 20.0 * s, c.cell.height + 10.0 * s);
    let x = (v.hover.0 + 12.0 * s).min(l.width - w - 4.0 * s).max(4.0 * s);
    let y = (v.hover.1 + 14.0 * s).clamp(4.0 * s, (l.height - h - 4.0 * s).max(4.0 * s));
    c.glow(x, y, w, h, rgba(ACCENT, 0.22), 8.0 * s, 16.0 * s);
    c.glass(x, y, w, h, rgba(PANEL_TINT, 1.0), 8.0 * s);
    c.text(x + 10.0 * s, y + 5.0 * s, &name, rgba(FOREGROUND, 1.0), true);
}

/// The sidebar's pin, a pushpin around `(cx, cy)`, in points of `s`
/// pixels: its head, its body, its collar and its needle, leaning over to
/// the right while it isn't pinned; pinned, upright and pressed in to its
/// collar, which sits on the board, its needle in it.
fn draw_pin(c: &mut Canvas<'_>, (cx, cy): (f32, f32), pinned: bool, ink: Rgba, s: f32) {
    // Each part a segment and its half thickness, the pin upright.
    let parts: [([f32; 2], [f32; 2], f32); 4] = if pinned {
        [([-3.0, -3.6], [3.0, -3.6], 1.6), ([0.0, -3.6], [0.0, 1.2], 2.2), ([-5.0, 2.4], [5.0, 2.4], 1.3), ([-7.0, 4.6], [7.0, 4.6], 0.6)]
    } else {
        [([-3.0, -6.5], [3.0, -6.5], 1.6), ([0.0, -6.5], [0.0, -1.5], 2.2), ([-5.0, -1.0], [5.0, -1.0], 1.3), ([0.0, -1.0], [0.0, 6.8], 0.7)]
    };
    let (sin, cos) = if pinned { (0.0, 1.0) } else { std::f32::consts::FRAC_PI_4.sin_cos() };
    let at = |[x, y]: [f32; 2]| [cx + (x * cos - y * sin) * s, cy + 0.4 * s + (x * sin + y * cos) * s];
    for (k, (a, b, half)) in parts.into_iter().enumerate() {
        // The board it's pinned to is fainter than the pin.
        let color = if pinned && k == 3 { [ink[0], ink[1], ink[2], ink[3] * 0.55] } else { ink };
        c.capsule(at(a), at(b), (half * s).max(0.6), color);
    }
}

/// A tab's or a group's menu (`tab_sidebar::Menu`), drawn by Flow: under
/// `at` (a row's …, or with `left` a tab's pill, its left edges lined up),
/// over it where there's no room below. A color item shows its swatch, and
/// the item the keys are on is lit. Where the window hasn't room for every
/// item (a group's themes, many saved), it shows as many as fit, keeping
/// the lit one in view, a bar at its edge saying where they are.
fn draw_row_menu(c: &mut Canvas<'_>, v: &View<'_>, menu: &SidebarMenu, at: Rect, left: bool, hits: &mut Vec<(Rect, Hit)>) {
    let l = v.layout;
    let s = l.scale;
    let ch = c.cell.height;
    let item_h = (ch * 1.6).round();
    let mw = menu.items.iter().map(|(words, swatch)| c.text_width(words) + if swatch.is_some() { 18.0 * s } else { 0.0 }).fold(0.0, f32::max) + 28.0 * s;
    let n = menu.items.len();
    let room = (l.height - l.title.h - 16.0 * s).max(item_h + 8.0 * s);
    let fit = (((room - 8.0 * s) / item_h).floor() as usize).clamp(1, n.max(1));
    let top = if menu.chosen >= fit { menu.chosen + 1 - fit } else { 0 };
    let mh = item_h * fit as f32 + 8.0 * s;
    let mx = if left { at.x } else { at.x + at.w - mw }.max(4.0 * s).min(l.width - mw - 4.0 * s);
    let below = at.y + at.h + 4.0 * s;
    let my = if below + mh <= l.height - 4.0 * s { below } else { (at.y - mh - 4.0 * s).max(l.title.h).min((l.height - mh - 4.0 * s).max(l.title.h)) };
    c.glow(mx, my, mw, mh, rgba(ACCENT, 0.18), 10.0 * s, 18.0 * s);
    c.glass(mx, my, mw, mh, rgba(PANEL_TINT, 1.0), 10.0 * s);
    hits.push((Rect::new(mx, my, mw, mh), Hit::SidebarCard));
    if fit < n {
        let track = mh - 12.0 * s;
        let thumb = (track * fit as f32 / n as f32).max(12.0 * s);
        let ty = my + 6.0 * s + (track - thumb) * top as f32 / (n - fit) as f32;
        c.rect(mx + mw - 4.0 * s, ty, 2.0 * s, thumb, rgba(FOREGROUND, 0.3), 1.0 * s);
    }
    for (k, (words, swatch)) in menu.items.iter().enumerate().skip(top).take(fit) {
        let r = Rect::new(mx + 4.0 * s, my + 4.0 * s + (k - top) as f32 * item_h, mw - 8.0 * s, item_h);
        let lit = k == menu.chosen || r.contains(v.hover);
        if lit {
            c.rect(r.x, r.y, r.w, r.h, rgba(ACCENT, if k == menu.chosen { 0.18 } else { 0.08 }), 6.0 * s);
        }
        let mut tx = r.x + 10.0 * s;
        let ty = r.y + (item_h - ch) * 0.5;
        if let Some(col) = swatch {
            let d = 10.0 * s;
            c.rect(tx, ty + (ch - d) * 0.5, d, d, rgba(*col, 0.95), 3.0 * s);
            tx += d + 8.0 * s;
        }
        c.text(tx, ty, words, rgba(FOREGROUND, if lit { 1.0 } else { 0.85 }), k == menu.chosen);
        hits.push((r, Hit::SidebarMenuItem(k)));
    }
}

/// What a tab or a pane is doing, on the line under its name in the
/// sidebar: a program's progress as a thin bar and its share, the command
/// running after a steady dot, how the last ended after a ✓ or ✗, or the
/// folder, cut from its start. None of it moves, so none keeps frames
/// coming.
fn draw_sidebar_line(c: &mut Canvas<'_>, (x, y): (f32, f32), room: f32, line: &crate::tab_sidebar::Line, s: f32) {
    use crate::tab_sidebar::Line;
    let ch = c.cell.height;
    match line {
        Line::Progress { percent, failed } => {
            let color = if *failed { BAD } else { ACCENT };
            let label = percent.map_or_else(|| "…".to_string(), |p| tf("sidebar.progress", &[("percent", &p)]));
            let lw = c.text_width(&label);
            let bar = (room - lw - 8.0 * s).max(0.0);
            let (bh, by) = ((3.0 * s).round().max(2.0), y + (ch - 3.0 * s) * 0.5);
            c.rect(x, by, bar, bh, rgba([255, 255, 255], 0.12), bh * 0.5);
            let share = percent.map_or(1.0, |p| p.min(100) as f32 / 100.0);
            c.rect(x, by, bar * share, bh, rgba(color, if percent.is_some() { 0.9 } else { 0.4 }), bh * 0.5);
            c.text(x + bar + 8.0 * s, y, &label, rgba(DIM, 1.0), false);
        }
        Line::Running(command) => {
            let d = 6.0 * s;
            c.rect(x, y + (ch - d) * 0.5, d, d, rgba(ACCENT, 0.95), d);
            c.text(x + d + 6.0 * s, y, &fit(c, command, (room - d - 6.0 * s).max(0.0)), rgba(FOREGROUND, 0.75), false);
        }
        Line::Ended { command, ok } => {
            let (mark, color) = if *ok { ("✓", GOOD) } else { ("✗", BAD) };
            // A fallback font may draw the mark wider than it measures.
            let mw = c.text_width(mark).max(c.cell.width * 1.5) + 6.0 * s;
            c.text(x, y, mark, rgba(color, 1.0), false);
            c.text(x + mw, y, &fit(c, command, (room - mw).max(0.0)), rgba(DIM, 1.0), false);
        }
        Line::Folder(folder) => {
            c.text(x, y, &fit_left(c, folder, room), rgba(DIM, 0.9), false);
        }
        Line::Nothing => {}
    }
}

/// Ask why: a pill at the terminal's foot, on the right, after a command
/// failed, which asks the assistant why.
fn draw_ask_why(c: &mut Canvas<'_>, v: &View<'_>, exit: i32, hits: &mut Vec<(Rect, Hit)>) {
    let l = v.layout;
    let s = l.scale;
    let ch = c.cell.height;
    let label = tf("ask_why.button", &[("exit", &exit)]);
    let (w, h) = (c.text_width(&label) + 22.0 * s, ch + 10.0 * s);
    let r = Rect::new(l.term.x + l.term.w - w - 8.0 * s, l.term.y + l.term.h - h - 6.0 * s, w, h);
    let over = r.contains(v.hover);
    c.glow(r.x, r.y, r.w, r.h, rgba(ACCENT2, if over { 0.35 } else { 0.18 }), h * 0.5, 12.0 * s);
    c.glass(r.x, r.y, r.w, r.h, rgba(PANEL_TINT, 0.92), h * 0.5);
    c.outline(r.x, r.y, r.w, r.h, rgba(ACCENT2, if over { 0.8 } else { 0.45 }), h * 0.5, 1.0);
    c.text(r.x + 11.0 * s, r.y + 5.0 * s, &label, rgba(FOREGROUND, if over { 1.0 } else { 0.85 }), false);
    hits.push((r, Hit::AskWhy));
}

/// How tall the find bar is.
fn find_bar_height(line: f32, scale: f32) -> f32 {
    line + 14.0 * scale
}

/// The find bar, at the terminal's top right: what's typed, how many it
/// found and which of them is chosen, and its `.*` (a pattern), ↑, ↓ and
/// ✕. Hands back where its caret is.
fn draw_find(c: &mut Canvas<'_>, v: &View<'_>, f: &crate::find::Find, hits: &mut Vec<(Rect, Hit)>) -> Rect {
    let l = v.layout;
    let s = l.scale;
    let ch = c.cell.height;
    let h = find_bar_height(ch, s);
    let w = (440.0 * s).min(l.term.w - 16.0 * s).max(160.0 * s);
    let (x, y) = (l.term.x + l.term.w - w - 8.0 * s, l.term.y + 6.0 * s);
    c.glow(x, y, w, h, rgba(ACCENT, if f.focus { 0.22 } else { 0.1 }), 10.0 * s, 16.0 * s);
    c.glass(x, y, w, h, rgba(PANEL_TINT, 0.96), 10.0 * s);
    if f.focus {
        c.outline(x, y, w, h, rgba(ACCENT, 0.5), 10.0 * s, 1.0);
    }
    hits.push((Rect::new(x, y, w, h), Hit::FindBar));
    let ty = y + (h - ch) * 0.5;
    // Its buttons, from the right.
    let b = ch * 1.25;
    let mut bx = x + w - 6.0 * s - b;
    for (label, hit) in [("✕", Hit::FindClose), ("↓", Hit::FindNext), ("↑", Hit::FindPrev)] {
        let r = Rect::new(bx, y + (h - b) * 0.5, b, b);
        if r.contains(v.hover) {
            c.rect(r.x, r.y, r.w, r.h, rgba([255, 255, 255], 0.1), 6.0 * s);
        }
        c.text(r.x + (b - c.text_width(label)) * 0.5, ty, label, rgba(FOREGROUND, if f.matches.is_empty() && hit != Hit::FindClose { 0.35 } else { 0.85 }), false);
        hits.push((r, hit));
        bx -= b + 2.0 * s;
    }
    // The pattern switch, lit while it's on.
    let label = ".*";
    let r = Rect::new(bx, y + (h - b) * 0.5, b, b);
    if f.pattern {
        c.rect(r.x, r.y, r.w, r.h, rgba(ACCENT, 0.28), 6.0 * s);
        c.outline(r.x, r.y, r.w, r.h, rgba(ACCENT, 0.7), 6.0 * s, 1.0);
    } else if r.contains(v.hover) {
        c.rect(r.x, r.y, r.w, r.h, rgba([255, 255, 255], 0.1), 6.0 * s);
    }
    c.text(r.x + (b - c.text_width(label)) * 0.5, ty, label, rgba(FOREGROUND, if f.pattern { 1.0 } else { 0.6 }), false);
    hits.push((r, Hit::FindPattern));
    bx -= b + 2.0 * s;
    // How many, and which: "3 of 17".
    let count = match (f.current, f.matches.len()) {
        _ if f.query.is_empty() => String::new(),
        _ if f.bad => t("find.bad").to_string(),
        (_, 0) => t("find.none").to_string(),
        (Some(i), n) => tf(if f.more { "find.count_more" } else { "find.count" }, &[("n", &(i + 1)), ("total", &n)]),
        (None, n) => n.to_string(),
    };
    let count_x = bx + b - 6.0 * s - c.text_width(&count);
    c.text(count_x, ty, &count, rgba(if f.matches.is_empty() && !f.query.is_empty() { WARN } else { DIM }, 1.0), false);
    // What's typed, its end shown where it's longer than the room.
    let ix = x + 12.0 * s;
    let room = (count_x - 10.0 * s - ix).max(0.0);
    if f.query.is_empty() {
        c.text(ix, ty, t(if f.pattern { "find.placeholder_pattern" } else { "find.placeholder" }), rgba(DIM, 0.6), false);
    }
    let mut shown = f.query.clone();
    while !shown.is_empty() && c.text_width(&shown) > room - c.cell.width {
        shown.remove(0);
    }
    let end = c.text(ix, ty, &shown, rgba(FOREGROUND, 1.0), false);
    let caret = Rect::new(if f.query.is_empty() { ix } else { end }, ty, 2.0 * s, ch);
    if f.focus && !v.ai.focus {
        let a = 0.55 + 0.45 * (v.time * 4.0).sin().abs();
        c.rect(caret.x, caret.y, caret.w, caret.h, rgba(ACCENT, a), 1.0);
    }
    caret
}

fn draw_history(c: &mut Canvas<'_>, v: &View<'_>, p: &HistoryPanel, hits: &mut Vec<(Rect, Hit)>) -> Rect {
    let s = v.layout.scale;
    let (cw, ch) = (c.cell.width, c.cell.height);
    c.rect(0.0, 0.0, v.layout.width, v.layout.height, [0.0, 0.0, 0.0, 0.35], 0.0);
    let pad = 22.0 * s;
    let row_h = (ch * 2.5).round();
    let w = (820.0 * s).min(v.layout.width - 32.0 * s);
    let h = (v.layout.height - 48.0 * s).min(ch * 6.0 + row_h * 9.0 + pad * 2.0);
    let (x, y) = ((v.layout.width - w) * 0.5, (v.layout.height - h) * 0.5);
    c.glow(x, y, w, h, rgba(ACCENT, 0.2), 18.0 * s, 30.0 * s);
    c.glass(x, y, w, h, rgba(PANEL_TINT, 1.0), 18.0 * s);
    hits.push((Rect::new(x, y, w, h), Hit::HistoryCard));

    let tx = c.text(x + pad, y + pad, "↺ ", rgba(ACCENT, 1.0), true);
    c.text(tx, y + pad, t("history.title"), rgba(FOREGROUND, 1.0), true);
    let close = Rect::new(x + w - pad - ch * 1.4, y + pad - ch * 0.2, ch * 1.4, ch * 1.4);
    if close.contains(v.hover) {
        c.rect(close.x, close.y, close.w, close.h, rgba([255, 255, 255], 0.1), 6.0 * s);
    }
    c.text(close.x + (close.w - c.text_width("✕")) * 0.5, close.y + (close.h - ch) * 0.5, "✕", rgba(FOREGROUND, 0.8), false);
    hits.push((close, Hit::HistoryClose));

    // The search box, as the key dialog's.
    let mut ty = y + pad + ch * 1.6;
    let box_h = ch + 16.0 * s;
    c.rect(x + pad, ty, w - pad * 2.0, box_h, rgba([255, 255, 255], 0.07), 10.0 * s);
    c.outline(x + pad, ty, w - pad * 2.0, box_h, rgba(ACCENT, 0.5), 10.0 * s, 1.0);
    let (ix, iy) = (x + pad + 12.0 * s, ty + 8.0 * s);
    let room = w - pad * 2.0 - 24.0 * s;
    if p.query.is_empty() {
        c.text(ix, iy, t("history.placeholder"), rgba(DIM, 0.5), false);
    }
    // A long search shows its end, where the typing is.
    let mut shown = p.query.clone();
    while !shown.is_empty() && c.text_width(&shown) > room - cw {
        shown.remove(0);
    }
    let end = c.text(ix, iy, &shown, rgba(FOREGROUND, 1.0), false);
    let caret = Rect::new(end, iy, 2.0 * s, ch);
    let a = 0.55 + 0.45 * (v.time * 4.0).sin().abs();
    c.rect(caret.x, caret.y, caret.w, caret.h, rgba(ACCENT, a), 1.0);
    ty += box_h + ch * 0.6;

    // The filters: everything, this folder, failures, this shell.
    let mut fx = x + pad;
    for (i, filter) in Filter::ALL.iter().enumerate() {
        let label = match filter {
            Filter::All => t("history.filter.all").to_string(),
            Filter::Folder => t("history.filter.folder").to_string(),
            Filter::Failed => t("history.filter.failed").to_string(),
            Filter::Shell => p.shell_label.clone(),
        };
        let pw = c.text_width(&label) + 20.0 * s;
        let r = Rect::new(fx, ty, pw, ch + 8.0 * s);
        let on = *filter == p.filter;
        if on {
            c.rect(r.x, r.y, r.w, r.h, rgba(ACCENT, 0.20), r.h * 0.5);
            c.outline(r.x, r.y, r.w, r.h, rgba(ACCENT, 0.55), r.h * 0.5, 1.0);
        } else {
            c.rect(r.x, r.y, r.w, r.h, rgba([255, 255, 255], if r.contains(v.hover) { 0.10 } else { 0.04 }), r.h * 0.5);
        }
        c.text(fx + 10.0 * s, ty + 4.0 * s, &label, if on { rgba(ACCENT, 1.0) } else { rgba(FOREGROUND, 0.75) }, on);
        hits.push((r, Hit::HistoryFilter(i)));
        fx += pw + 8.0 * s;
    }
    ty += ch + 8.0 * s + ch * 0.6;

    // The keys, or the word that a second press forgets everything found.
    let cols = ((w - pad * 2.0) / cw) as usize;
    let (hint, hint_color) = if p.forget_all_at.is_some() {
        let n = if p.found.len() >= crate::history::SHOWN { format!("{}+", p.found.len()) } else { p.found.len().to_string() };
        (tf("history.forget_all_again", &[("n", &n)]), WARN)
    } else {
        (t(keys("history.keys_mac", "history.keys")).to_string(), DIM)
    };
    let hint: Vec<_> = text::wrap_plain(&hint, cols, Style::Normal).into_iter().take(2).collect();
    let footer_h = ch * (hint.len() as f32 + 0.8);

    // The commands found, the selected one kept in view.
    let list_h = (y + h - pad - footer_h - ty).max(row_h);
    let visible = ((list_h / row_h) as usize).max(1);
    let mut top = p.top.get().min(p.found.len().saturating_sub(1));
    if p.selected < top {
        top = p.selected;
    } else if p.selected >= top + visible {
        top = p.selected + 1 - visible;
    }
    p.top.set(top);
    if p.found.is_empty() {
        let why = if p.off {
            tf("history.off", &[("keys", &crate::keys::label(crate::event::Command::Settings))])
        } else if p.query.is_empty() && p.filter == Filter::All {
            t("history.empty").to_string()
        } else {
            t("history.none").to_string()
        };
        let mut ly = ty + ch * 0.5;
        for line in text::wrap_plain(&why, cols, Style::Normal) {
            c.text(x + pad, ly, &text::line_text(&line), rgba(DIM, 1.0), false);
            ly += ch;
        }
    }
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0);
    let home = dirs::home_dir().map(|h| h.to_string_lossy().into_owned());
    let mut ry = ty;
    for (i, f) in p.found.iter().enumerate().skip(top).take(visible) {
        let r = Rect::new(x + pad - 10.0 * s, ry, w - pad * 2.0 + 20.0 * s, row_h - 4.0 * s);
        let selected = i == p.selected;
        if selected {
            c.rect(r.x, r.y, r.w, r.h, rgba([255, 255, 255], 0.06), 8.0 * s);
            c.outline(r.x, r.y, r.w, r.h, rgba(ACCENT, 0.35), 8.0 * s, 1.0);
        } else if r.contains(v.hover) {
            c.rect(r.x, r.y, r.w, r.h, rgba([255, 255, 255], 0.03), 8.0 * s);
        }
        hits.push((r, Hit::HistoryItem(i)));
        let line_y = ry + (row_h - 4.0 * s - ch * 2.0) * 0.5;
        let mut cx = x + pad;
        // What the assistant ran wears its mark.
        if f.assistant {
            cx = c.text(cx, line_y, "✦ ", rgba(ACCENT, 0.9), false);
        }
        let room = x + w - pad - cx;
        // Cut long before measuring: a command can run to thousands of letters.
        let one_line: String = f.command.replace("\r\n", " ↵ ").replace(['\n', '\r'], " ↵ ").chars().take((room / cw) as usize + 2).collect();
        c.text(cx, line_y, &fit(c, &one_line, room), rgba(CODE, if selected { 1.0 } else { 0.85 }), selected);
        // Under it: where, how it ended, when, and how often.
        let mut dx = x + pad;
        let dy = line_y + ch;
        if let Some(code) = f.exit {
            let (mark, color) = if code == 0 { ("✓".to_string(), GOOD) } else { (format!("✗ {code}"), BAD) };
            dx = c.text(dx, dy, &mark, rgba(color, 0.9), false) + cw;
        }
        let mut detail = vec![ago(now, f.started)];
        if f.runs > 1 {
            detail.push(tf("history.runs", &[("n", &f.runs.to_string())]));
        }
        if let Some(cwd) = &f.cwd {
            let short = match &home {
                Some(home) if cwd.starts_with(home.as_str()) && !home.is_empty() => format!("~{}", &cwd[home.len()..]),
                _ => cwd.clone(),
            };
            detail.push(short);
        }
        let detail = detail.join(" · ");
        c.text(dx, dy, &fit(c, &detail, x + w - pad - dx), rgba(DIM, 0.9), false);
        ry += row_h;
    }
    if top + visible < p.found.len() {
        c.text(x + w - pad - c.text_width(t("settings.more_down")), y + h - pad - footer_h - ch * 0.4, t("settings.more_down"), rgba(DIM, 0.8), false);
    }

    let mut fy = y + h - pad - ch * hint.len() as f32;
    c.rect(x + pad, fy - ch * 0.5, w - pad * 2.0, 1.0, rgba([255, 255, 255], 0.08), 0.0);
    for line in &hint {
        c.text(x + pad, fy, &text::line_text(line), rgba(hint_color, 0.75), false);
        fy += ch;
    }
    caret
}

/// How long ago `then` was, both in milliseconds since 1970 (or since
/// anything else, the same for both).
pub(crate) fn ago(now: i64, then: i64) -> String {
    let mins = (now - then).max(0) / 60_000;
    let (key, n) = match mins {
        0 => return t("history.ago.now").to_string(),
        1..=59 => ("history.ago.minutes", mins),
        60..=1439 => ("history.ago.hours", mins / 60),
        1440..=43_199 => ("history.ago.days", mins / 1440),
        43_200..=525_599 => ("history.ago.months", mins / 43_200),
        _ => ("history.ago.years", mins / 525_600),
    };
    tf(key, &[("n", &n.to_string())])
}

/// Row heights for the settings list, shared by drawing and scrolling.
struct SettingsGeometry {
    ch: f32,
    row_h: f32,
    header_h: f32,
}

/// The fewest and most lines a text box in Settings shows; past the most,
/// it shows the lines round the caret.
const TEXT_BOX_LINES: (usize, usize) = (3, 8);

/// How many lines the box for `text` shows.
fn box_lines(text: &str) -> usize {
    text.split('\n').count().clamp(TEXT_BOX_LINES.0, TEXT_BOX_LINES.1)
}

impl SettingsGeometry {
    fn new(ch: f32) -> Self {
        Self { ch, row_h: (ch * 1.75).round(), header_h: (ch * 1.7).round() }
    }

    fn height(&self, entry: &settings::Entry) -> f32 {
        match entry {
            settings::Entry::Header(_) => self.header_h,
            // A box of lines, half a line inside it above and below, and the
            // row's margin round it.
            settings::Entry::Setting(settings::Row { control: Control::Text { value, multiline: true, .. }, .. }) => {
                (box_lines(value) as f32 * self.ch + self.ch * 1.75).round()
            }
            // A line under the slider, saying what its value makes.
            settings::Entry::Setting(settings::Row { control: Control::Slider { note: Some(_), .. }, .. }) => self.row_h + self.ch.round(),
            settings::Entry::Setting(_) => self.row_h,
        }
    }
}

/// Where the settings list's entry `index` starts, in pixels below the
/// list's top, for cells `ch` tall: a snapshot scrolls there, so the list
/// starts on a row's edge.
pub fn settings_entry_top(entries: &[settings::Entry], index: usize, ch: f32) -> f32 {
    let geo = SettingsGeometry::new(ch);
    entries[..index.min(entries.len())].iter().map(|e| geo.height(e)).sum()
}

/// The settings card. Returns the caret of the box naming a preset, while
/// it's open, or of the text row being typed in.
/// The most lines of a chosen setting's hint Settings' footer shows.
const HINT_LINES: usize = 8;

fn draw_settings(c: &mut Canvas<'_>, v: &View<'_>, sv: &SettingsView<'_>, hits: &mut Vec<(Rect, Hit)>) -> Option<Rect> {
    let s = v.layout.scale;
    let (cw, ch) = (c.cell.width, c.cell.height);
    let geo = SettingsGeometry::new(ch);
    c.rect(0.0, 0.0, v.layout.width, v.layout.height, [0.0, 0.0, 0.0, 0.35], 0.0);

    let pad = 22.0 * s;
    // The title, then the row of tabs under it.
    let chip_h = ch * 1.5;
    let title_h = ch * 2.0 + chip_h + ch * 0.7;
    let w = (780.0 * s).min(v.layout.width - 32.0 * s);
    // The footer holds what the chosen setting does, whole (a skill's
    // description runs to a few lines), up to `HINT_LINES`, never less room
    // than two, so the card doesn't jump between short hints; the list gives
    // up what a longer one takes.
    let cols = ((w - pad * 2.0) / cw) as usize;
    let selected_hint = settings::rows(sv.entries).get(sv.menu.selected).map_or("", |r| r.hint.as_str());
    let mut hint_lines = text::wrap_plain(selected_hint, cols, Style::Normal);
    hint_lines.truncate(HINT_LINES);
    let footer_h = ch * (hint_lines.len().max(2) as f32 + 1.4);
    let content_h: f32 = sv.entries.iter().map(|e| geo.height(e)).sum();
    let h = (title_h + content_h + footer_h + pad * 2.0).min(v.layout.height - 48.0 * s);
    let x = (v.layout.width - w) * 0.5;
    let y = (v.layout.height - h) * 0.5;
    let card = Rect::new(x, y, w, h);
    c.glow(x, y, w, h, rgba(ACCENT, 0.2), 18.0 * s, 30.0 * s);
    c.glass(x, y, w, h, rgba(PANEL_TINT, 1.0), 18.0 * s);
    hits.push((card, Hit::SettingsCard));

    // The gear glyph comes from a fallback font and is wider than one cell.
    let tx = c.text(x + pad, y + pad, "⚙  ", rgba(ACCENT, 1.0), true);
    c.text(tx, y + pad, t("settings.title"), rgba(FOREGROUND, 1.0), true);
    let close = Rect::new(x + w - pad - ch * 1.4, y + pad - ch * 0.2, ch * 1.4, ch * 1.4);
    if close.contains(v.hover) {
        c.rect(close.x, close.y, close.w, close.h, rgba([255, 255, 255], 0.1), 6.0 * s);
    }
    let xw = c.text_width("✕");
    c.text(close.x + (close.w - xw) * 0.5, close.y + (close.h - ch) * 0.5, "✕", rgba(FOREGROUND, 0.8), false);
    hits.push((close, Hit::SettingsClose));
    // Collapse all and Expand all beside it, each dim where it would change
    // nothing, so one section can be opened alone; left out where the
    // title leaves no room.
    let (any_open, any_closed) = settings::sections_shown(sv.entries);
    let title_end = tx + c.text_width(t("settings.title")) + cw * 2.0;
    let ph = ch * 1.4;
    let mut bx = close.x - 10.0 * s;
    for (label, hit, useful) in [(t("settings.expand_all"), Hit::SettingsExpandAll, any_closed), (t("settings.collapse_all"), Hit::SettingsCollapseAll, any_open)] {
        let bw = c.text_width(label) + 20.0 * s;
        if bx - bw < title_end {
            break;
        }
        bx -= bw;
        let r = Rect::new(bx, close.y + (close.h - ph) * 0.5, bw, ph);
        let a = if !useful { 0.06 } else if r.contains(v.hover) { 0.30 } else { 0.16 };
        c.rect(r.x, r.y, r.w, r.h, rgba(ACCENT, a), ph * 0.5);
        c.text(r.x + 10.0 * s, r.y + (ph - ch) * 0.5, label, rgba(if useful { ACCENT } else { DIM }, if useful { 1.0 } else { 0.6 }), useful);
        hits.push((r, hit));
        bx -= 8.0 * s;
    }

    // The open tabs, the one it's on lit, then new tabs': whose look the
    // look's sections change. What the card has no room for is cut short,
    // left of the search field at the row's end.
    let strip_y = y + pad + ch * 2.0;
    let search_w = (cw * 26.0).min((w - pad * 2.0) * 0.45);
    let search = Rect::new(x + w - pad - search_w, strip_y, search_w, chip_h);
    let mut search_caret = None;
    {
        let typing = sv.menu.search.as_ref();
        let lit = typing.is_some() || search.contains(v.hover);
        c.rect(search.x, search.y, search.w, search.h, rgba([255, 255, 255], if lit { 0.07 } else { 0.04 }), 8.0 * s);
        c.outline(search.x, search.y, search.w, search.h, rgba(if typing.is_some() { ACCENT } else { DIM }, if typing.is_some() { 0.6 } else { 0.3 }), 8.0 * s, 1.0);
        let ty = search.y + (chip_h - ch) * 0.5;
        let room = search.w - cw * 2.0;
        match typing.filter(|s| !s.query.is_empty()) {
            // What's typed, its end in view, then the caret.
            Some(typed) => {
                let mut shown: &str = &typed.query;
                while c.text_width(shown) > room - cw {
                    let mut chars = shown.chars();
                    chars.next();
                    shown = chars.as_str();
                }
                let end = c.text(search.x + cw, ty, shown, rgba(FOREGROUND, 1.0), false);
                search_caret = Some(Rect::new(end, ty, 2.0 * s, ch));
            }
            None => {
                let find = crate::keys::label(crate::event::Command::Find);
                let words = tf("settings.search.placeholder", &[("keys", &find)]);
                c.text(search.x + cw, ty, &fit(c, &words, room), rgba(DIM, 0.8), false);
                if typing.is_some() {
                    search_caret = Some(Rect::new(search.x + cw, ty, 2.0 * s, ch));
                }
            }
        }
        if let Some(r) = search_caret {
            c.rect(r.x, r.y, r.w, r.h, rgba(ACCENT, 0.9), 0.0);
        }
        hits.push((search, Hit::SettingsSearch));
    }
    let mut sx = x + pad;
    let labels = sv.tabs.iter().map(String::as_str).chain([t("settings.on.new_tabs")]);
    for (k, label) in labels.enumerate() {
        let on = sv.on.map_or(k == sv.tabs.len(), |i| i == k);
        let chip_w = c.text_width(label) + cw * 2.0;
        if sx + chip_w > search.x - cw {
            c.text(sx, strip_y + (chip_h - ch) * 0.5, "…", rgba(DIM, 0.9), false);
            break;
        }
        let r = Rect::new(sx, strip_y, chip_w, chip_h);
        if on {
            c.rect(r.x, r.y, r.w, r.h, rgba(ACCENT, 0.18), 8.0 * s);
            c.outline(r.x, r.y, r.w, r.h, rgba(ACCENT, 0.5), 8.0 * s, 1.0);
        } else if r.contains(v.hover) {
            c.rect(r.x, r.y, r.w, r.h, rgba([255, 255, 255], 0.06), 8.0 * s);
        }
        let fg = if on { rgba(ACCENT, 1.0) } else { rgba(FOREGROUND, 0.7) };
        c.text(sx + cw, strip_y + (chip_h - ch) * 0.5, label, fg, on);
        hits.push((r, if k < sv.tabs.len() { Hit::SettingsOnTab(k) } else { Hit::SettingsOnNewTabs }));
        sx += chip_w + cw * 0.6;
    }

    // The list stays where it was scrolled. A row the keys select comes into
    // view, the list moving no further than that takes; the first row of a
    // section brings its header along.
    let view_top = y + pad + title_h;
    let view_h = (h - pad * 2.0 - title_h - footer_h).max(geo.row_h);
    let mut scroll = sv.menu.scroll.get();
    if sv.menu.shown.get() != Some(sv.menu.selected) {
        let mut acc = 0.0;
        let mut header_top = None;
        for (row_i, e) in sv.entries.iter().enumerate() {
            let top = acc;
            acc += geo.height(e);
            if row_i == sv.menu.selected {
                let top = header_top.unwrap_or(top);
                // Below, half a row stays clear for the "more" mark.
                let bottom = acc + geo.row_h * 0.5;
                scroll = settings::reveal(scroll, top, bottom, view_h);
            }
            header_top = matches!(e, settings::Entry::Header(_)).then_some(top);
        }
        sv.menu.shown.set(Some(sv.menu.selected));
    }
    let scroll = scroll.clamp(0.0, (content_h - view_h).max(0.0));
    sv.menu.scroll.set(scroll);

    let label_x = x + pad;
    let ctrl_x = x + w * 0.40;
    let ctrl_w = x + w - pad - ctrl_x;
    let mut ey = view_top - scroll;
    let mut row_i = 0;
    let mut caret = None;
    for e in sv.entries {
        let eh = geo.height(e);
        let visible = ey >= view_top - 0.5 && ey + eh <= view_top + view_h + 0.5;
        match e {
            // A section's header: a click, or Enter, opens or closes it, and
            // closed it says how many settings it holds.
            settings::Entry::Header(row) => {
                let i = row_i;
                row_i += 1;
                let selected = i == sv.menu.selected;
                if visible {
                    let r = Rect::new(label_x - 10.0 * s, ey + 2.0 * s, w - pad * 2.0 + 20.0 * s, eh - 4.0 * s);
                    if selected {
                        c.rect(r.x, r.y, r.w, r.h, rgba([255, 255, 255], 0.06), 8.0 * s);
                        c.outline(r.x, r.y, r.w, r.h, rgba(ACCENT, 0.35), 8.0 * s, 1.0);
                    } else if r.contains(v.hover) {
                        c.rect(r.x, r.y, r.w, r.h, rgba([255, 255, 255], 0.03), 8.0 * s);
                    }
                    hits.push((r, Hit::SettingToggle(i)));
                    let ty = ey + (eh - ch) * 0.5;
                    let (open, count, scope) = match row.control {
                        Control::Section { open, count, scope } => (open, count, scope),
                        _ => (true, 0, ""),
                    };
                    let mark_x = c.text(label_x, ty, if open { "▾" } else { "▸" }, rgba(ACCENT, 0.9), false);
                    let end = c.text(mark_x.max(label_x + cw) + cw * 0.5, ty, row.label, rgba(ACCENT, 0.9), true);
                    // Whose its settings are, dimly after its name.
                    let end = if scope.is_empty() { end } else { c.text(end + cw, ty, scope, rgba(DIM, 0.8), false) };
                    let mut line_end = x + w - pad;
                    if !open {
                        let n = tf("settings.section.count", &[("n", &count.to_string())]);
                        line_end -= c.text_width(&n);
                        c.text(line_end, ty, &n, rgba(DIM, 0.8), false);
                        line_end -= cw;
                    }
                    c.rect(end + cw, ty + ch * 0.55, (line_end - end - cw).max(0.0), 1.0, rgba([255, 255, 255], 0.08), 0.0);
                }
            }
            settings::Entry::Setting(row) => {
                let i = row_i;
                row_i += 1;
                let selected = i == sv.menu.selected;
                if visible {
                    let r = Rect::new(label_x - 10.0 * s, ey + 2.0 * s, w - pad * 2.0 + 20.0 * s, eh - 4.0 * s);
                    if selected {
                        c.rect(r.x, r.y, r.w, r.h, rgba([255, 255, 255], 0.06), 8.0 * s);
                        c.outline(r.x, r.y, r.w, r.h, rgba(ACCENT, 0.35), 8.0 * s, 1.0);
                    } else if r.contains(v.hover) {
                        c.rect(r.x, r.y, r.w, r.h, rgba([255, 255, 255], 0.03), 8.0 * s);
                    }
                    hits.push((r, Hit::SettingRow(i)));
                    // A slider's note goes under it, the row's first line
                    // keeping the label and the slider.
                    let note = match &row.control {
                        Control::Slider { note: Some(note), .. } => Some(note),
                        _ => None,
                    };
                    let line_h = if note.is_some() { geo.row_h } else { eh };
                    let area = Rect::new(ctrl_x, ey, ctrl_w, line_h);
                    // A tall row's label sits level with its first line.
                    let ty = if line_h > geo.row_h { ey + ch * 0.875 } else { ey + (line_h - ch) * 0.5 };
                    // Under the slider, from its start; too long for that,
                    // under the label, whose column is free on the row's
                    // second line.
                    if let Some(note) = note {
                        let end = ctrl_x + ctrl_w;
                        let nx = if c.text_width(note) <= ctrl_w { ctrl_x } else { label_x };
                        c.text(nx, ey + line_h - ch * 0.3, &fit(c, note, end - nx), rgba(DIM, 0.95), false);
                    }
                    // A row found by the search lights the words typed in
                    // its label and says its section after it.
                    match sv.menu.search.as_ref().filter(|s| s.finding()).and_then(|s| s.found.iter().find(|f| f.id == row.id)) {
                        Some(found) => {
                            let end = draw_lit(c, label_x, ty, row.label, &found.lit, rgba(FOREGROUND, if selected { 1.0 } else { 0.85 }), selected);
                            let room = ctrl_x - cw - (end + cw);
                            if !found.section.is_empty() && room > cw * 3.0 {
                                c.text(end + cw, ty, &fit(c, found.section, room), rgba(DIM, 0.8), false);
                            }
                        }
                        None => {
                            c.text(label_x, ty, row.label, rgba(FOREGROUND, if selected { 1.0 } else { 0.85 }), selected);
                        }
                    }
                    match &row.control {
                        Control::Text { value, multiline, empty } => {
                            let editing = sv.menu.editing.as_ref().filter(|e| e.id == row.id).map(|e| &e.field);
                            let field = TextRow { value, multiline: *multiline, empty, editing };
                            caret = draw_text_field(c, v, i, &field, area, hits).or(caret);
                        }
                        control => draw_control(c, v, i, control, area, hits),
                    }
                }
            }
        }
        ey += eh;
    }
    // Nothing found: says so where the rows would be.
    if let Some(s) = sv.menu.search.as_ref().filter(|s| s.finding() && sv.entries.is_empty()) {
        let none = tf("settings.search.none", &[("query", &s.query.trim())]);
        c.text(label_x, view_top + geo.row_h * 0.5, &fit(c, &none, w - pad * 2.0), rgba(DIM, 0.9), false);
    }
    let (up, down) = (t("settings.more_up"), t("settings.more_down"));
    let more_x = x + w - pad - c.text_width(down);
    if scroll > 0.5 {
        c.text(more_x, view_top - ch * 1.1, up, rgba(DIM, 0.8), false);
    }
    if scroll < content_h - view_h - 0.5 {
        c.text(more_x, view_top + view_h - ch * 0.2, down, rgba(DIM, 0.8), false);
    }

    // Footer: what the selected setting does, then the keys.
    let fy = y + h - pad - footer_h + ch * 0.4;
    c.rect(x + pad, fy - ch * 0.3, w - pad * 2.0, 1.0, rgba([255, 255, 255], 0.08), 0.0);
    let mut ly = fy + ch * 0.2;
    for line in &hint_lines {
        c.text(x + pad, ly, &text::line_text(line), rgba(DIM, 1.0), false);
        ly += ch;
    }
    // While a text row is typed in, the keys that edit it.
    let footer_keys = match &sv.menu.editing {
        Some(e) if e.field.multiline() => t(keys("settings.editing.keys_lines_mac", "settings.editing.keys_lines")),
        Some(_) => t(keys("settings.editing.keys_line_mac", "settings.editing.keys_line")),
        None if sv.menu.search.is_some() => t("settings.search.keys"),
        None => t("settings.keys"),
    };
    c.text(x + pad, y + h - pad - ch, footer_keys, rgba(DIM, 0.6), false);
    if let Some(p) = &sv.menu.picker {
        draw_color_picker(c, v, p, card, hits);
    }
    if let Some(g) = &sv.menu.gallery {
        draw_gallery(c, v, g, card, hits);
    }
    sv.menu.naming.as_ref().map(|naming| draw_name_box(c, v, naming, card, hits)).or(caret).or(search_caret)
}

/// Draws `text` at `x`, `y`, the byte ranges `lit` in the accent color,
/// and says where it ends.
fn draw_lit(c: &mut Canvas<'_>, x: f32, y: f32, text: &str, lit: &[std::ops::Range<usize>], color: [f32; 4], bold: bool) -> f32 {
    let mut at = 0;
    let mut end = x;
    for r in lit.iter().filter(|r| r.end <= text.len() && text.is_char_boundary(r.start) && text.is_char_boundary(r.end)) {
        if r.start < at {
            continue;
        }
        end = c.text(end, y, &text[at..r.start], color, bold);
        end = c.text(end, y, &text[r.start..r.end], rgba(ACCENT, 1.0), true);
        at = r.end;
    }
    c.text(end, y, &text[at..], color, bold)
}

/// The theme gallery over the settings: the packages of themes down the
/// left, each with where it stands; the chosen one's themes beside them;
/// and the chosen theme's look drawn from what its package says (its
/// screen in its colors with a few lines of its start screen and its
/// prompt, its sixteen colors, its font and tube), with Install, Upgrade
/// or Wear under it.
fn draw_gallery(c: &mut Canvas<'_>, v: &View<'_>, g: &crate::gallery::Gallery, over: Rect, hits: &mut Vec<(Rect, Hit)>) {
    use crate::gallery::{name_of, plain, Contents};
    let s = v.layout.scale;
    let (cw, ch) = (c.cell.width, c.cell.height);
    let pad = 18.0 * s;
    let w = (1000.0 * s).min(over.w - 24.0 * s);
    let h = (620.0 * s).min(over.h - 24.0 * s);
    let (x, y) = (over.x + (over.w - w) * 0.5, over.y + (over.h - h) * 0.5);
    c.rect(over.x, over.y, over.w, over.h, [0.0, 0.0, 0.0, 0.35], 18.0 * s);
    c.glow(x, y, w, h, rgba(ACCENT, 0.2), 16.0 * s, 26.0 * s);
    c.glass(x, y, w, h, rgba(PANEL_TINT, 1.0), 16.0 * s);
    hits.push((Rect::new(x, y, w, h), Hit::GalleryCard));
    c.text(x + pad, y + pad, t("gallery.title"), rgba(FOREGROUND, 1.0), true);
    let b = ch * 1.4;
    let close = Rect::new(x + w - pad - b, y + pad - (b - ch) * 0.5, b, b);
    if close.contains(v.hover) {
        c.rect(close.x, close.y, close.w, close.h, rgba([255, 255, 255], 0.1), 6.0 * s);
    }
    c.text(close.x + (b - c.text_width("✕")) * 0.5, y + pad, "✕", rgba(FOREGROUND, 0.8), false);
    hits.push((close, Hit::GalleryClose));

    let top = y + pad + ch * 2.4;
    let foot = y + h - pad - ch;
    let bottom = foot - ch * 0.8;
    let acts = g.acts();
    let first = acts.first().map(|a| a.label()).unwrap_or("—");
    c.text(x + pad, foot, &fit(c, &tf("gallery.hint", &[("act", &first)]), w - pad * 2.0), rgba(DIM, 0.6), false);

    // The packages.
    let list_w = (w * 0.29).max(cw * 21.0);
    let row = ch * 2.6;
    if g.packages.is_empty() {
        c.text(x + pad, top, &fit(c, t("gallery.none"), w - pad * 2.0), rgba(DIM, 0.9), false);
        let label = t("gallery.check");
        let ph = ch + 8.0 * s;
        let r = Rect::new(x + pad, top + ch * 1.8, c.text_width(label) + 24.0 * s, ph);
        c.rect(r.x, r.y, r.w, r.h, rgba(ACCENT, if r.contains(v.hover) { 0.30 } else { 0.16 }), ph * 0.5);
        c.text(r.x + 12.0 * s, r.y + 4.0 * s, label, rgba(ACCENT, 1.0), true);
        hits.push((r, Hit::GalleryCheck));
        return;
    }
    for (i, p) in g.packages.iter().enumerate() {
        let r = Rect::new(x + pad, top + i as f32 * row, list_w, row - 4.0 * s);
        if r.y + r.h > bottom {
            break;
        }
        let chosen = i == g.chosen;
        if chosen {
            c.rect(r.x, r.y, r.w, r.h, rgba(ACCENT, if g.on_themes { 0.10 } else { 0.20 }), 8.0 * s);
            if !g.on_themes {
                c.outline(r.x, r.y, r.w, r.h, rgba(ACCENT, 0.6), 8.0 * s, 1.0);
            }
        } else if r.contains(v.hover) {
            c.rect(r.x, r.y, r.w, r.h, rgba([255, 255, 255], 0.06), 8.0 * s);
        }
        c.text(r.x + 10.0 * s, r.y + 5.0 * s, &fit(c, &p.name, r.w - 20.0 * s), rgba(FOREGROUND, 1.0), chosen);
        let latest = p.latest.as_deref().unwrap_or("—");
        let state = match (&p.installed, &p.needs_flow) {
            (_, Some(flow)) => tf("gallery.needs_flow", &[("latest", &latest), ("flow", flow)]),
            (Some(have), None) if p.upgradable() => tf("gallery.newer", &[("version", have), ("latest", &latest)]),
            (Some(have), None) => tf("gallery.installed", &[("version", have)]),
            (None, None) => tf("gallery.available", &[("latest", &latest)]),
        };
        c.text(r.x + 10.0 * s, r.y + 5.0 * s + ch * 1.05, &fit(c, &state, r.w - 20.0 * s), rgba(DIM, 0.9), false);
        hits.push((r, Hit::GalleryPackage(i)));
    }

    // The chosen package's themes.
    let tx = x + pad + list_w + 12.0 * s;
    let tw = (w * 0.23).max(cw * 18.0);
    let theme_row = ch * 1.6;
    match g.package().and_then(|p| p.contents.as_ref()) {
        Some(Contents::Read(_)) => {
            for (i, theme) in g.package().map(|p| p.themes()).unwrap_or_default().iter().enumerate() {
                let r = Rect::new(tx, top + i as f32 * theme_row, tw, theme_row - 3.0 * s);
                if r.y + r.h > bottom {
                    break;
                }
                let chosen = i == g.theme;
                if chosen {
                    c.rect(r.x, r.y, r.w, r.h, rgba(ACCENT, if g.on_themes { 0.20 } else { 0.10 }), 7.0 * s);
                    if g.on_themes {
                        c.outline(r.x, r.y, r.w, r.h, rgba(ACCENT, 0.6), 7.0 * s, 1.0);
                    }
                } else if r.contains(v.hover) {
                    c.rect(r.x, r.y, r.w, r.h, rgba([255, 255, 255], 0.06), 7.0 * s);
                }
                c.text(r.x + 9.0 * s, r.y + (r.h - ch) * 0.5, &fit(c, name_of(theme), r.w - 18.0 * s), rgba(FOREGROUND, if chosen { 1.0 } else { 0.85 }), chosen);
                hits.push((r, Hit::GalleryTheme(i)));
            }
        }
        Some(Contents::Failed(why)) => {
            let words = tf("gallery.failed", &[("error", why)]);
            c.text(tx, top, &fit(c, &words, x + w - pad - tx), rgba(WARN, 1.0), false);
            return;
        }
        Some(Contents::Reading) | None => {
            c.text(tx, top, t("gallery.reading"), rgba(DIM, 0.9), false);
            return;
        }
    }

    // The chosen theme's look.
    let Some(theme) = g.theme() else { return };
    let color = |hex: &str, or: [u8; 3]| crate::render::palette::parse_hex(hex).unwrap_or(or);
    let (bg, fg, cursor) = (color(&theme.background, [8, 12, 22]), color(&theme.text, FOREGROUND), color(&theme.cursor, ACCENT));
    let px = tx + tw + 14.0 * s;
    let pw = x + w - pad - px;
    let info_h = ch * 1.2 * 4.0 + ch + 16.0 * s;
    // A screen's shape, where there's room.
    let ph = (bottom - top - info_h).min(pw * 0.78).max(ch * 4.0);
    let border = (!theme.border.is_empty()).then(|| color(&theme.border, bg));
    if let Some(edge) = border {
        c.rect(px, top, pw, ph, rgba(edge, 1.0), 8.0 * s);
    }
    let inset = if border.is_some() { 12.0 * s } else { 0.0 };
    let (sx, sy, sw, sh) = (px + inset, top + inset, pw - inset * 2.0, ph - inset * 2.0);
    c.rect(sx, sy, sw, sh, rgba(bg, 1.0), if border.is_some() { 3.0 * s } else { 8.0 * s });
    let mut ly = sy + 8.0 * s;
    let room = sw - 20.0 * s;
    // Its logo, where its package brings one, as a tab shows it above the
    // start screen.
    if let Some(logo) = g.logo() {
        let lh = (ch * 3.5).min(sh * 0.4);
        let lw = (lh * logo.width as f32 / logo.height.max(1) as f32).min(room);
        c.image_over(logo, (sx + 10.0 * s, ly, lw, lh), 1.0);
        ly += lh + ch * 0.4;
    }
    // In its own letters, where its package brings them or they're
    // installed, else Flow's.
    let letters = (theme.fonts.iter().cloned().chain(v.font.0.iter().cloned()).collect::<Vec<_>>(), v.font.1, v.font.2);
    c.use_font(&letters);
    let (tw2, th2) = (c.cell.width, c.cell.height);
    let line_h = th2 * 1.15;
    let lines: Vec<String> = theme.start.iter().map(|l| plain(l)).filter(|l| !l.trim().is_empty()).collect();
    for line in lines.iter().take(((sy + sh - ly - line_h * 2.0 - 8.0 * s) / line_h).max(0.0) as usize) {
        c.text(sx + 10.0 * s, ly, &fit(c, line, room), rgba(fg, 1.0), false);
        ly += line_h;
    }
    // Its prompt and cursor.
    let prompt = theme.prompt.as_deref().map(plain).filter(|p| !p.trim().is_empty()).unwrap_or_else(|| "$ ".into());
    ly += line_h * 0.4;
    let end = c.text(sx + 10.0 * s, ly, &fit(c, prompt.lines().last().unwrap_or("$"), room - tw2 * 2.0), rgba(fg, 1.0), false);
    let shape = theme.cursor_shape.as_str();
    let (cx, cy2, cw2, chh) = match shape {
        "bar" => (end, ly, 2.0 * s, th2),
        "underline" => (end, ly + th2 - 2.0 * s, tw2, 2.0 * s),
        _ => (end, ly, tw2, th2),
    };
    c.rect(cx, cy2, cw2, chh, rgba(cursor, 0.9), 0.0);
    c.use_font(&v.font);

    // Its sixteen colors (the dark eight over the bright, where one row
    // won't hold them), its font and its tube.
    let iy = top + ph + 12.0 * s;
    let sq = ch * 0.8;
    let gap = 4.0 * s;
    let ansi: Vec<[u8; 3]> = theme.ansi.iter().filter_map(|h| crate::render::palette::parse_hex(h)).collect();
    let label_end = c.text(px, iy, t("gallery.colors"), rgba(DIM, 0.9), false);
    if ansi.is_empty() {
        c.text(label_end + 10.0 * s, iy, "—", rgba(DIM, 0.9), false);
    }
    let per_row = if label_end + 10.0 * s + 16.0 * (sq + gap) <= px + pw { 16 } else { 8 };
    for (k, rgb) in ansi.iter().enumerate() {
        let qx = label_end + 10.0 * s + (k % per_row) as f32 * (sq + gap);
        let qy = iy + (ch - sq) * 0.5 + (k / per_row) as f32 * (sq + gap);
        c.rect(qx, qy, sq, sq, rgba(*rgb, 1.0), 3.0 * s);
    }
    let rows = if per_row == 16 || ansi.len() <= 8 { 1.0 } else { 2.0 };
    let font = theme.fonts.first().map(|f| tf("gallery.font", &[("font", f)])).unwrap_or_else(|| t("gallery.font_own").to_string());
    let tube = t(if theme.crt.is_empty() { "gallery.no_tube" } else { "gallery.tube" });
    let fy = iy + ch * 0.3 + rows * (sq + gap);
    c.text(px, fy, &fit(c, &font, pw), rgba(DIM, 0.9), false);
    c.text(px, fy + ch * 1.2, tube, rgba(DIM, 0.9), false);

    // What can be done, at the right.
    let bh = ch + 10.0 * s;
    let by = fy + ch * 2.7;
    let mut bx = px + pw;
    for (k, act) in acts.iter().enumerate().rev() {
        let label = act.label();
        let bw = c.text_width(label) + 26.0 * s;
        bx -= bw;
        let r = Rect::new(bx, by, bw, bh);
        let a = if r.contains(v.hover) { 0.32 } else { 0.18 };
        c.rect(r.x, r.y, r.w, r.h, rgba(ACCENT, a), bh * 0.5);
        c.text(r.x + 13.0 * s, by + (bh - ch) * 0.5, label, rgba(ACCENT, 1.0), true);
        hits.push((r, Hit::GalleryAct(k)));
        bx -= 8.0 * s;
    }
}

/// A text row as drawn: its text, whether it takes several lines, what it
/// says with nothing typed, and its field while it's being typed in.
struct TextRow<'a> {
    value: &'a str,
    multiline: bool,
    empty: &'a str,
    editing: Option<&'a settings::TextField>,
}

/// A text row's field in `area`: a line, or a box of several, showing the
/// text (or what an empty one means) and, while it's typed in, the caret
/// with what the input method is composing there. Long lines are cut short
/// but the caret's, which scrolls sideways to keep it in view, and a box of
/// more lines than it shows shows those round the caret. Returns the caret.
fn draw_text_field(c: &mut Canvas<'_>, v: &View<'_>, i: usize, row: &TextRow<'_>, area: Rect, hits: &mut Vec<(Rect, Hit)>) -> Option<Rect> {
    let s = v.layout.scale;
    let (cw, ch) = (c.cell.width, c.cell.height);
    let shown = if row.multiline { box_lines(row.value) } else { 1 };
    let fh = if row.multiline { shown as f32 * ch + ch } else { ch + 8.0 * s };
    let r = Rect::new(area.x, area.y + (area.h - fh) * 0.5, area.w, fh);
    let typing = row.editing.is_some();
    c.rect(r.x, r.y, r.w, r.h, rgba([255, 255, 255], if typing { 0.07 } else { 0.04 }), 6.0 * s);
    let edge = if typing {
        0.7
    } else if r.contains(v.hover) {
        0.35
    } else {
        0.18
    };
    c.outline(r.x, r.y, r.w, r.h, rgba(ACCENT, edge), 6.0 * s, 1.0);
    hits.push((r, Hit::SettingText(i)));
    let inset = 10.0 * s;
    let (tx, room) = (r.x + inset, r.w - inset * 2.0);
    let top = if row.multiline { r.y + ch * 0.5 } else { r.y + (fh - ch) * 0.5 };
    let Some(field) = row.editing else {
        if row.value.is_empty() {
            c.text(tx, top, row.empty, rgba(DIM, 0.6), false);
            return None;
        }
        let lines: Vec<&str> = row.value.split('\n').collect();
        // Lines past the box are counted at the end of its last one.
        let more = (lines.len() > shown).then(|| tf("settings.editing.more", &[("n", &(lines.len() - shown))]));
        for (k, line) in lines.iter().take(shown).enumerate() {
            let y = top + k as f32 * ch;
            let mut room = room;
            if let Some(more) = more.as_deref().filter(|_| k + 1 == shown) {
                let mw = c.text_width(more);
                c.text(r.x + r.w - inset - mw, y, more, rgba(DIM, 0.8), false);
                room -= mw + cw;
            }
            c.text(tx, y, &fit(c, line, room), rgba(FOREGROUND, 0.85), false);
            caret_hits(c, hits, (i, k, 0), line, r, (tx, y), room);
        }
        return None;
    };
    let (line, col) = field.caret();
    let lines = field.lines();
    let first = line.saturating_sub(shown - 1).min(lines.len().saturating_sub(shown));
    let mut caret = None;
    for (k, text) in lines.iter().enumerate().skip(first).take(shown) {
        let y = top + (k - first) as f32 * ch;
        if k != line {
            c.text(tx, y, &fit(c, text, room), rgba(FOREGROUND, 1.0), false);
            caret_hits(c, hits, (i, k, 0), text, r, (tx, y), room);
            continue;
        }
        let (x, from) = input_line(c, (tx, y), room, text, col, v.preedit, s);
        // While the input method composes, its letters sit among the line's.
        if v.preedit.is_none() {
            caret_hits(c, hits, (i, k, from), text, r, (tx, y), room);
        }
        // No caret while the input method converts. It sits on the boundary
        // between two letters, which it can be inside a line.
        if v.preedit.is_none_or(|p| p.caret().is_some()) {
            c.rect(x - 0.5 * s, y, 2.0 * s, ch, rgba(ACCENT, 1.0), 0.0);
        }
        caret = Some(Rect::new(x, y, cw, ch));
    }
    caret
}

/// Where a click on a line of text row `row`'s field puts the caret: a
/// character's left half before it and its right half after, the room left
/// of the text the first character shown, and the rest of the line its
/// end. `at` is where line `line` is drawn from, starting at character
/// `from`, within `room`; `field` is the whole field.
fn caret_hits(c: &Canvas<'_>, hits: &mut Vec<(Rect, Hit)>, (row, line, from): (usize, usize, usize), text: &str, field: Rect, at: (f32, f32), room: f32) {
    let ((x, y), h) = (at, c.cell.height);
    hits.push((Rect::new(field.x, y, x - field.x, h), Hit::SettingCaret(row, line, from)));
    hits.push((Rect::new(x, y, field.x + field.w - x, h), Hit::SettingCaret(row, line, text.chars().count())));
    let mut cx = x;
    for (col, ch) in text.chars().enumerate().skip(from) {
        let w = c.text_width(ch.encode_utf8(&mut [0; 4]));
        if cx + w > x + room {
            break;
        }
        hits.push((Rect::new(cx, y, w * 0.5, h), Hit::SettingCaret(row, line, col)));
        hits.push((Rect::new(cx + w * 0.5, y, w * 0.5, h), Hit::SettingCaret(row, line, col + 1)));
        cx += w;
    }
}

/// The box for naming a CRT preset or a theme, over the settings: the name
/// typed so far (or a placeholder), then Cancel and Save, and under them the
/// keys, or that a name already taken is saved over. Returns its caret.
fn draw_name_box(c: &mut Canvas<'_>, v: &View<'_>, naming: &settings::Naming, over: Rect, hits: &mut Vec<(Rect, Hit)>) -> Rect {
    let s = v.layout.scale;
    let ch = c.cell.height;
    let pad = 18.0 * s;
    let row_h = ch + 12.0 * s;
    let words = naming.words();
    let word = |part: &str| t(&format!("{words}.{part}"));
    let name = naming.name.as_str();
    let (save, cancel) = (word("save"), t("common.cancel"));
    let w = (460.0 * s).min(over.w - 24.0 * s);
    let h = pad * 2.0 + ch * 1.8 + row_h + 14.0 * s + row_h + 10.0 * s + ch;
    let x = over.x + (over.w - w) * 0.5;
    let y = over.y + (over.h - h) * 0.5;
    c.rect(over.x, over.y, over.w, over.h, [0.0, 0.0, 0.0, 0.35], 18.0 * s);
    c.glow(x, y, w, h, rgba(ACCENT, 0.2), 16.0 * s, 26.0 * s);
    c.glass(x, y, w, h, rgba(PANEL_TINT, 1.0), 16.0 * s);
    hits.push((Rect::new(x, y, w, h), Hit::NameCard));
    c.text(x + pad, y + pad, word("title"), rgba(FOREGROUND, 1.0), true);

    // The name, with its caret, or a placeholder until something is typed.
    let (fx, fw) = (x + pad, w - pad * 2.0);
    let fy = y + pad + ch * 1.8;
    c.rect(fx, fy, fw, row_h, rgba([255, 255, 255], 0.06), 6.0 * s);
    c.outline(fx, fy, fw, row_h, rgba(ACCENT, 0.6), 6.0 * s, 1.0);
    let ty = fy + (row_h - ch) * 0.5;
    let end = if name.is_empty() && v.preedit.is_none() {
        c.text(fx + 12.0 * s, ty, word("placeholder"), rgba(DIM, 0.6), false);
        fx + 12.0 * s
    } else {
        input_line(c, (fx + 12.0 * s, ty), fw - 24.0 * s, name, name.chars().count(), v.preedit, s).0
    };
    if v.preedit.is_none_or(|p| p.caret().is_some()) {
        c.rect(end + 1.0, ty, 2.0 * s, ch, rgba(ACCENT, 1.0), 0.0);
    }

    let by = fy + row_h + 14.0 * s;
    let mut bx = x + w - pad;
    for (label, hit, primary) in [(save, Hit::NameSave, true), (cancel, Hit::NameCancel, false)] {
        let bw = c.text_width(label) + 24.0 * s;
        bx -= bw;
        let r = Rect::new(bx, by, bw, row_h);
        let a = if r.contains(v.hover) { 0.30 } else { 0.16 };
        let col = if primary { ACCENT } else { [255, 255, 255] };
        c.rect(r.x, r.y, r.w, r.h, rgba(col, if primary { a } else { a * 0.5 }), row_h * 0.5);
        c.text(r.x + 12.0 * s, by + (row_h - ch) * 0.5, label, rgba(if primary { ACCENT } else { FOREGROUND }, 1.0), primary);
        hits.push((r, hit));
        bx -= 8.0 * s;
    }
    let (hint, color) = match naming.over() {
        true => (tf(&format!("{words}.over"), &[("name", &naming.chosen())]), rgba(ACCENT, 0.9)),
        false => (word("hint").to_string(), rgba(DIM, 0.7)),
    };
    c.text(fx, by + row_h + 10.0 * s, &fit(c, &hint, fw), color, false);
    Rect::new(end, ty, c.cell.width, ch)
}

/// The color picker over the settings: a field of saturation (across) and
/// brightness (up) at the picked hue, the hue bar, the color it opened on
/// beside the new one, the hex value, which can be typed, and Cancel and Done.
fn draw_color_picker(c: &mut Canvas<'_>, v: &View<'_>, p: &ColorPicker, over: Rect, hits: &mut Vec<(Rect, Hit)>) {
    let s = v.layout.scale;
    let ch = c.cell.height;
    let pad = 18.0 * s;
    let row_h = ch + 10.0 * s;
    let swatch = row_h * 1.3;
    let shown = p.shown();
    let hex_w = c.text_width("#000000") + 24.0 * s;
    let (done, cancel) = (t("common.done"), t("common.cancel"));
    let (done_w, cancel_w) = (c.text_width(done) + 24.0 * s, c.text_width(cancel) + 24.0 * s);
    let needed = pad * 2.0 + swatch * 2.0 + 10.0 * s + hex_w + 24.0 * s + cancel_w + 8.0 * s + done_w;
    let w = needed.max(460.0 * s).min(over.w - 24.0 * s);
    let (field_h, hue_h) = (190.0 * s, 16.0 * s);
    let title_h = ch * 1.8;
    let h = pad + title_h + field_h + 14.0 * s + hue_h + 18.0 * s + row_h + 12.0 * s + ch + pad;
    let x = over.x + (over.w - w) * 0.5;
    let y = over.y + (over.h - h) * 0.5;
    // The menu steps back while it's open.
    c.rect(over.x, over.y, over.w, over.h, [0.0, 0.0, 0.0, 0.35], 18.0 * s);
    c.glow(x, y, w, h, rgba(ACCENT, 0.2), 16.0 * s, 26.0 * s);
    c.glass(x, y, w, h, rgba(PANEL_TINT, 1.0), 16.0 * s);
    hits.push((Rect::new(x, y, w, h), Hit::PickerCard));
    c.text(x + pad, y + pad, p.label, rgba(FOREGROUND, 1.0), true);

    let [hue, sat, val] = p.hsv;
    let (fx, fw) = (x + pad, w - pad * 2.0);
    let fy = y + pad + title_h;
    c.color_field([fx, fy, fw, field_h], hue, 8.0 * s);
    hits.push((Rect::new(fx, fy, fw, field_h), Hit::PickerField));
    // Rings, dark under light, to show on any color.
    let ring = |c: &mut Canvas<'_>, r: Rect, radius: f32| {
        c.outline(r.x, r.y, r.w, r.h, [0.0, 0.0, 0.0, 0.45], radius, 3.0 * s);
        c.outline(r.x, r.y, r.w, r.h, [1.0, 1.0, 1.0, 1.0], radius, 1.5 * s);
    };
    let m = 14.0 * s;
    ring(c, Rect::new(fx + sat * fw - m * 0.5, fy + (1.0 - val) * field_h - m * 0.5, m, m), m);

    let hy = fy + field_h + 14.0 * s;
    c.hue_bar([fx, hy, fw, hue_h], hue_h * 0.5);
    // Taller than the bar, so it's easy to hit; across is what counts.
    hits.push((Rect::new(fx, hy - 5.0 * s, fw, hue_h + 10.0 * s), Hit::PickerHue));
    let knob = 8.0 * s;
    ring(c, Rect::new(fx + hue * fw - knob * 0.5, hy - 3.0 * s, knob, hue_h + 6.0 * s), knob * 0.5);

    // The color it opened on, the new one, and the hex value.
    let ry = hy + hue_h + 18.0 * s;
    let edge = rgba([255, 255, 255], 0.2);
    for (k, color) in [p.was, p.color()].into_iter().enumerate() {
        let sx = fx + k as f32 * (swatch + 2.0 * s);
        c.rect(sx, ry, swatch, row_h, rgba(color, 1.0), 6.0 * s);
        c.outline(sx, ry, swatch, row_h, edge, 6.0 * s, 1.0);
    }
    let hx = fx + swatch * 2.0 + 12.0 * s;
    c.rect(hx, ry, hex_w, row_h, rgba([255, 255, 255], 0.06), 6.0 * s);
    c.outline(hx, ry, hex_w, row_h, rgba(ACCENT, if p.typed.is_some() { 0.7 } else { 0.3 }), 6.0 * s, 1.0);
    let ty = ry + (row_h - ch) * 0.5;
    let end = c.text(hx + 12.0 * s, ty, &shown, rgba(FOREGROUND, 1.0), false);
    if p.typed.is_some() {
        c.rect(end + 1.0, ty, 2.0 * s, ch, rgba(ACCENT, 1.0), 0.0);
    }

    let mut bx = x + w - pad;
    for (label, hit, primary) in [(done, Hit::PickerDone, true), (cancel, Hit::PickerCancel, false)] {
        let bw = c.text_width(label) + 24.0 * s;
        bx -= bw;
        let r = Rect::new(bx, ry, bw, row_h);
        let a = if r.contains(v.hover) { 0.30 } else { 0.16 };
        let col = if primary { ACCENT } else { [255, 255, 255] };
        c.rect(r.x, r.y, r.w, r.h, rgba(col, if primary { a } else { a * 0.5 }), row_h * 0.5);
        c.text(r.x + 12.0 * s, ty, label, rgba(if primary { ACCENT } else { FOREGROUND }, 1.0), primary);
        hits.push((r, hit));
        bx -= 8.0 * s;
    }
    let hint = fit(c, t("picker.hint"), fw);
    c.text(fx, ry + row_h + 12.0 * s, &hint, rgba(DIM, 0.7), false);
}

fn draw_control(c: &mut Canvas<'_>, v: &View<'_>, i: usize, control: &Control, area: Rect, hits: &mut Vec<(Rect, Hit)>) {
    let s = v.layout.scale;
    let ch = c.cell.height;
    let cy = area.y + area.h * 0.5;
    let ty = area.y + (area.h - ch) * 0.5;
    match control {
        // Headers draw themselves (`draw_settings`).
        Control::Section { .. } => {}
        Control::Slider { value, min, max, text, .. } => {
            let text_w = c.cell.width * 15.0;
            let track_w = (area.w - text_w - 14.0 * s).max(40.0 * s);
            let frac = ((value - min) / (max - min)).clamp(0.0, 1.0);
            let th = 4.0 * s;
            c.rect(area.x, cy - th * 0.5, track_w, th, rgba([255, 255, 255], 0.14), th);
            c.rect(area.x, cy - th * 0.5, track_w * frac, th, rgba(ACCENT, 0.85), th);
            let knob = 14.0 * s;
            let kx = area.x + track_w * frac - knob * 0.5;
            c.glow(kx, cy - knob * 0.5, knob, knob, rgba(ACCENT, 0.35), knob, 8.0 * s);
            c.rect(kx, cy - knob * 0.5, knob, knob, rgba([0xF2, 0xFB, 0xFF], 1.0), knob);
            c.text(area.x + track_w + 14.0 * s, ty, text, rgba(FOREGROUND, 0.9), false);
            hits.push((Rect::new(area.x, area.y, track_w, area.h), Hit::SettingSlider(i)));
        }
        Control::Toggle(on) => {
            let (sw, sh) = (38.0 * s, 20.0 * s);
            let sy = cy - sh * 0.5;
            let fill = if *on { rgba(ACCENT, 0.85) } else { rgba([255, 255, 255], 0.14) };
            c.rect(area.x, sy, sw, sh, fill, sh);
            let knob = sh - 6.0 * s;
            let kx = if *on { area.x + sw - knob - 3.0 * s } else { area.x + 3.0 * s };
            c.rect(kx, sy + 3.0 * s, knob, knob, rgba([0xF2, 0xFB, 0xFF], 1.0), knob);
            let label = t(if *on { "common.on" } else { "common.off" });
            let end = c.text(area.x + sw + 10.0 * s, ty, label, rgba(FOREGROUND, 0.9), false);
            hits.push((Rect::new(area.x, area.y, end - area.x, area.h), Hit::SettingToggle(i)));
        }
        Control::Choice { options, index } => {
            let pad_x = 9.0 * s;
            let gap = 6.0 * s;
            let widths: Vec<f32> = options.iter().map(|o| c.text_width(o) + pad_x * 2.0).collect();
            let total = widths.iter().sum::<f32>() + gap * options.len().saturating_sub(1) as f32;
            let ph = ch + 6.0 * s;
            let py = cy - ph * 0.5;
            if total <= area.w {
                let mut px = area.x;
                for (k, (opt, ow)) in options.iter().zip(&widths).enumerate() {
                    let r = Rect::new(px, py, *ow, ph);
                    let active = k == *index;
                    let bg = if active {
                        rgba(ACCENT, 0.24)
                    } else if r.contains(v.hover) {
                        rgba([255, 255, 255], 0.10)
                    } else {
                        rgba([255, 255, 255], 0.045)
                    };
                    c.rect(r.x, r.y, r.w, r.h, bg, ph * 0.5);
                    if active {
                        c.outline(r.x, r.y, r.w, r.h, rgba(ACCENT, 0.6), ph * 0.5, 1.0);
                    }
                    let fg = if active { rgba(ACCENT, 1.0) } else { rgba(FOREGROUND, 0.75) };
                    c.text(r.x + pad_x, ty, opt, fg, active);
                    hits.push((r, Hit::SettingChoice(i, k)));
                    px += ow + gap;
                }
            } else {
                // Too many to show side by side: arrows around the current
                // value, stepping as ← and → do. None chosen (past the end,
                // as Fit window to's while the window holds another grid)
                // shows a dash, and the arrows go to the last and the first.
                let step = |n| settings::choice_index(*index, options.len(), settings::Change::Step(n));
                let (prev, next) = (step(-1), step(1));
                let aw = c.text_width("<") + pad_x * 2.0;
                let left = Rect::new(area.x, py, aw, ph);
                c.rect(left.x, left.y, left.w, left.h, rgba([255, 255, 255], 0.06), ph * 0.5);
                c.text(left.x + pad_x, ty, "<", rgba(FOREGROUND, 0.9), true);
                hits.push((left, Hit::SettingChoice(i, prev)));
                // The value gets the widest option's room, so the > stays put
                // while stepping; a value too long for the card (a font
                // family) is cut short to keep the > in it.
                let room = area.w - aw * 2.0 - pad_x * 4.0;
                let slot = (widths.iter().fold(0.0f32, |a, &w| a.max(w)) - pad_x * 2.0).min(room).max(0.0);
                let (cur, col) = match options.get(*index) {
                    Some(cur) => (cur.as_str(), rgba(ACCENT, 1.0)),
                    None => ("—", rgba(DIM, 0.9)),
                };
                c.text(left.x + left.w + pad_x, ty, &fit(c, cur, slot), col, true);
                let right = Rect::new(left.x + left.w + pad_x + slot + pad_x, py, aw, ph);
                c.rect(right.x, right.y, right.w, right.h, rgba([255, 255, 255], 0.06), ph * 0.5);
                c.text(right.x + pad_x, ty, ">", rgba(FOREGROUND, 0.9), true);
                hits.push((right, Hit::SettingChoice(i, next)));
            }
        }
        Control::Colors { options, index, none } => {
            // Round swatches with the one in use ringed; no color at all
            // (Phosphor's Off) an empty one.
            let d = ch * 0.85;
            let gap = 8.0 * s;
            let ring = 3.0 * s;
            let mut px = area.x + ring;
            for (k, (_, rgb)) in options.iter().enumerate() {
                let r = Rect::new(px, cy - d * 0.5, d, d);
                let active = k == *index;
                let empty = *none && k == 0;
                if active && !empty {
                    c.glow(r.x, r.y, r.w, r.h, rgba(*rgb, 0.45), d, 8.0 * s);
                }
                if empty {
                    c.outline(r.x, r.y, r.w, r.h, rgba(FOREGROUND, 0.6), d, 1.5 * s);
                } else {
                    c.rect(r.x, r.y, r.w, r.h, rgba(*rgb, 1.0), d);
                }
                // An edge, so the darkest show on the dark card.
                c.outline(r.x, r.y, r.w, r.h, rgba([255, 255, 255], 0.18), d, 1.0);
                if active || r.contains(v.hover) {
                    let a = if active { 0.9 } else { 0.4 };
                    c.outline(r.x - ring, r.y - ring, d + ring * 2.0, d + ring * 2.0, rgba([0xF2, 0xFB, 0xFF], a), d, 1.5 * s);
                }
                hits.push((Rect::new(r.x - gap * 0.5, area.y, d + gap, area.h), Hit::SettingChoice(i, k)));
                px += d + gap;
            }
            // The color in use, as #RRGGBB (or no color's name): a button
            // that opens the picker.
            let value = match options.get(*index) {
                Some((name, _)) if *none && *index == 0 => name.clone(),
                Some(&(_, rgb)) => palette::hex(rgb),
                None => String::new(),
            };
            let ph = ch + 6.0 * s;
            let r = Rect::new(px + 2.0 * s, cy - ph * 0.5, c.text_width(&value) + 20.0 * s, ph);
            let a = if r.contains(v.hover) { 0.30 } else { 0.16 };
            c.rect(r.x, r.y, r.w, r.h, rgba(ACCENT, a), ph * 0.5);
            c.text(r.x + 10.0 * s, ty, &value, rgba(ACCENT, 1.0), true);
            hits.push((r, Hit::SettingAction(i, 0)));
        }
        Control::Readout { text, action } => {
            let end = c.text(area.x, ty, text, rgba(FOREGROUND, 0.9), false);
            let ph = ch + 6.0 * s;
            let bw = c.text_width(action) + 20.0 * s;
            let r = Rect::new(end + 14.0 * s, cy - ph * 0.5, bw, ph);
            let a = if r.contains(v.hover) { 0.30 } else { 0.16 };
            c.rect(r.x, r.y, r.w, r.h, rgba(ACCENT, a), ph * 0.5);
            c.text(r.x + 10.0 * s, ty, action, rgba(ACCENT, 1.0), true);
            hits.push((r, Hit::SettingAction(i, 0)));
        }
        Control::Item { text, actions } => {
            // The buttons at the row's end, from the right, and the words in
            // the room left of them.
            let ph = ch + 6.0 * s;
            let mut px = area.x + area.w;
            for (k, label) in actions.iter().enumerate().rev() {
                let bw = c.text_width(label) + 20.0 * s;
                px -= bw;
                let r = Rect::new(px, cy - ph * 0.5, bw, ph);
                let col = if *label == t("settings.apps.remove") { BAD } else { ACCENT };
                let a = if r.contains(v.hover) { 0.30 } else { 0.16 };
                c.rect(r.x, r.y, r.w, r.h, rgba(col, a), ph * 0.5);
                c.text(r.x + 10.0 * s, ty, label, rgba(col, 1.0), true);
                hits.push((r, Hit::SettingAction(i, k)));
                px -= 8.0 * s;
            }
            let shown = fit(c, text, (px - area.x - 6.0 * s).max(0.0));
            c.text(area.x, ty, &shown, rgba(FOREGROUND, 0.9), false);
        }
        Control::Actions(labels) => {
            let mut px = area.x;
            let ph = ch + 6.0 * s;
            for (k, label) in labels.iter().enumerate() {
                let bw = c.text_width(label) + 20.0 * s;
                let r = Rect::new(px, cy - ph * 0.5, bw, ph);
                let col = if *label == t("settings.api_key.remove") { BAD } else { ACCENT };
                let a = if r.contains(v.hover) { 0.30 } else { 0.16 };
                c.rect(r.x, r.y, r.w, r.h, rgba(col, a), ph * 0.5);
                c.text(r.x + 10.0 * s, ty, label, rgba(col, 1.0), true);
                hits.push((r, Hit::SettingAction(i, k)));
                px += bw + 8.0 * s;
            }
        }
        // Drawn by draw_text_field, which hands back the caret.
        Control::Text { .. } => {}
    }
}

fn mix_rgb(a: [u8; 3], b: [u8; 3], t: f32, alpha: f32) -> Rgba {
    let (x, y) = (rgba(a, alpha), rgba(b, alpha));
    [x[0] + (y[0] - x[0]) * t, x[1] + (y[1] - x[1]) * t, x[2] + (y[2] - x[2]) * t, alpha]
}

/// A glowing head with a fading tail running around an ellipse.
#[allow(clippy::too_many_arguments)]
fn comet(c: &mut Canvas<'_>, cx: f32, cy: f32, r: f32, head: f32, color: [u8; 3], n: usize, s: f32, alpha: f32) {
    for k in 0..n {
        let f = k as f32 / n as f32;
        let ang = head - f * 1.7;
        let (px, py) = (cx + ang.cos() * r, cy + ang.sin() * r * 0.92);
        let size = (3.4 * (1.0 - f) + 0.8) * s;
        c.rect(px - size * 0.5, py - size * 0.5, size, size, rgba(color, (1.0 - f).powi(2) * 0.9 * alpha), size * 0.5);
    }
    let (hx, hy) = (cx + head.cos() * r, cy + head.sin() * r * 0.92);
    c.glow(hx - 4.0 * s, hy - 4.0 * s, 8.0 * s, 8.0 * s, rgba(color, 0.7 * alpha), 4.0 * s, 12.0 * s);
}

/// Sparks orbiting the icon on a tilted ring; `front` picks the half nearer to us.
fn sparks(c: &mut Canvas<'_>, cx: f32, cy: f32, t: f32, s: f32, alpha: f32, front: bool) {
    for i in 0..26 {
        let fi = i as f32;
        let h = |k: f32| ((fi * 12.9898 + k * 78.233).sin() * 43_758.547).fract().abs();
        let radius = (64.0 + h(1.0) * 52.0) * s;
        let speed = (0.3 + h(2.0) * 0.8) * if i % 2 == 0 { 1.0 } else { -1.0 };
        let ang = t * speed + h(3.0) * std::f32::consts::TAU;
        if (ang.sin() > 0.0) != front {
            continue;
        }
        let (px, py) = (cx + ang.cos() * radius, cy + ang.sin() * radius * 0.45);
        let size = (1.4 + h(4.0) * 2.6) * s * if front { 1.0 } else { 0.7 };
        let twinkle = 0.45 + 0.55 * (t * 3.0 + fi).sin().abs();
        let col = mix_rgb(ACCENT, ACCENT2, h(5.0), twinkle * alpha * if front { 1.0 } else { 0.55 });
        c.glow(px - size, py - size, size * 2.0, size * 2.0, [col[0], col[1], col[2], col[3] * 0.5], size, size * 2.5);
        c.rect(px - size * 0.5, py - size * 0.5, size, size, col, size * 0.5);
    }
}

fn draw_about(c: &mut Canvas<'_>, v: &View<'_>, a: &AboutView, hits: &mut Vec<(Rect, Hit)>) {
    let s = v.layout.scale;
    let t = v.time;
    let ch = c.cell.height;
    // Ease in: fade and rise into place.
    let k = (a.age / 0.45).clamp(0.0, 1.0);
    let intro = k * k * (3.0 - 2.0 * k);
    c.rect(0.0, 0.0, v.layout.width, v.layout.height, [0.0, 0.0, 0.0, 0.38 * intro], 0.0);
    let w = (560.0 * s).min(v.layout.width - 32.0 * s);
    // Where Flow shows the updater (Linux), a line says how it stands.
    let status = a.update.as_ref().and_then(|u| match u {
        UpdateState::Idle => None,
        UpdateState::Checking => Some((i18n::t("about.update.checking").to_string(), DIM)),
        UpdateState::UpToDate => Some((i18n::t("about.update.up_to_date").to_string(), GOOD)),
        UpdateState::Available { version } => Some((tf("about.update.available", &[("version", version)]), ACCENT)),
        UpdateState::Installing { version } => Some((tf("about.update.installing", &[("version", version)]), DIM)),
        UpdateState::Installed { version } => Some((tf("about.update.installed", &[("version", version)]), GOOD)),
        UpdateState::ByHand { version, file, .. } => Some((tf("update.deb", &[("version", version), ("file", file)]), ACCENT)),
        UpdateState::InTab { version, .. } => Some((tf("update.in_tab", &[("version", version)]), ACCENT)),
        UpdateState::ByPackage { version } => Some((tf("update.aur", &[("version", version)]), ACCENT)),
        UpdateState::Failed(error) => Some((error.clone(), WARN)),
    });
    let cols = ((w - 48.0 * s) / c.cell.width).max(10.0) as usize;
    let mut status: Vec<(String, [u8; 3])> = status
        .map(|(line, color)| text::wrap_plain(&line, cols, Style::Normal).iter().take(4).map(|l| (text::line_text(l), color)).collect())
        .unwrap_or_default();
    // A command to type goes below, on lines of its own: where it doesn't
    // fit, its file's path starts a line rather than breaking.
    if let Some(UpdateState::ByHand { command, .. }) = &a.update {
        let wrap = |s: &str| text::wrap_plain(s, cols, Style::Normal).iter().map(text::line_text).collect::<Vec<_>>();
        let mut lines = wrap(command);
        let path = if command.ends_with('\'') { command.rfind(" '") } else { command.rfind(' ') };
        if let (true, Some(at)) = (lines.len() > 1, path) {
            lines = wrap(&command[..at]);
            lines.extend(wrap(&command[at + 1..]));
        }
        status.extend(lines.into_iter().take(3).map(|l| (l, FOREGROUND)));
    }
    // The version's line, then a line of its links under it.
    let h = (470.0 * s + ch * 2.5 + status.len() as f32 * ch * 1.1).min(v.layout.height - 32.0 * s);
    let x = (v.layout.width - w) * 0.5;
    let y = (v.layout.height - h) * 0.5 + (1.0 - intro) * 24.0 * s;
    c.glow(x, y, w, h, rgba(ACCENT2, 0.25 * intro), 22.0 * s, 36.0 * s);
    c.glass(x, y, w, h, rgba(PANEL_TINT, intro.max(0.3)), 22.0 * s);
    hits.push((Rect::new(x, y, w, h), Hit::AboutCard));

    let close = Rect::new(x + w - 22.0 * s - ch * 1.4, y + 18.0 * s, ch * 1.4, ch * 1.4);
    if close.contains(v.hover) {
        c.rect(close.x, close.y, close.w, close.h, rgba([255, 255, 255], 0.1), 6.0 * s);
    }
    let xw = c.text_width("✕");
    c.text(close.x + (close.w - xw) * 0.5, close.y + (close.h - ch) * 0.5, "✕", rgba(FOREGROUND, 0.8 * intro), false);
    hits.push((close, Hit::AboutClose));

    // The cube rolls out its F in a pulsing halo; without it, the icon
    // floats there, circled by comets and orbiting sparks.
    let cx = x + w * 0.5;
    let pulse = 0.5 + 0.5 * (t * 2.0).sin();
    if let Some(cube) = &a.cube {
        let cy = y + 128.0 * s;
        c.glow(cx - 90.0 * s, cy - 70.0 * s, 180.0 * s, 140.0 * s, rgba(ACCENT, (0.16 + 0.10 * pulse) * intro), 70.0 * s, 56.0 * s);
        c.glow(cx - 60.0 * s, cy - 46.0 * s, 120.0 * s, 92.0 * s, rgba(ACCENT2, (0.12 + 0.08 * (1.0 - pulse)) * intro), 46.0 * s, 36.0 * s);
        let (cw, chh) = (ABOUT_CUBE.0 * s, ABOUT_CUBE.1 * s);
        c.image_over(cube, (cx - cw * 0.5, cy - chh * 0.5, cw, chh), intro);
    } else {
        let cy = y + 132.0 * s + (t * 1.4).sin() * 3.0 * s;
        c.glow(cx - 62.0 * s, cy - 62.0 * s, 124.0 * s, 124.0 * s, rgba(ACCENT, (0.20 + 0.12 * pulse) * intro), 62.0 * s, 48.0 * s);
        c.glow(cx - 42.0 * s, cy - 42.0 * s, 84.0 * s, 84.0 * s, rgba(ACCENT2, (0.16 + 0.10 * (1.0 - pulse)) * intro), 42.0 * s, 32.0 * s);
        sparks(c, cx, cy, t, s, intro, false);
        comet(c, cx, cy, 96.0 * s, t * 1.7, ACCENT, 42, s, intro);
        comet(c, cx, cy, 80.0 * s, -t * 1.25 + 2.0, ACCENT2, 30, s, intro);
        let icon = 112.0 * s;
        c.image(ICON_IMAGE, cx - icon * 0.5, cy - icon * 0.5, icon, icon, intro);
        sparks(c, cx, cy, t, s, intro, true);
    }

    // The name in large letters, rippling through the accent colors.
    let px = (44.0 * s).round();
    let cw = c.cell_width_at(px);
    let n = APP_NAME.chars().count() as f32;
    let tx = cx - cw * n * 0.5;
    let ty = y + 236.0 * s;
    for (i, letter) in APP_NAME.chars().enumerate() {
        let fi = i as f32;
        let col = mix_rgb(ACCENT, ACCENT2, 0.5 + 0.5 * (t * 1.6 + fi * 0.9).sin(), intro);
        let dy = (t * 2.2 + fi * 0.75).sin() * 3.0 * s;
        c.glyph_sized(tx + fi * cw, ty + dy, letter, px, col, true);
    }

    let mut line_y = ty + px * 1.45;
    let centered = |c: &mut Canvas<'_>, y: f32, text: &str, col: Rgba| {
        let tw = c.text_width(text);
        c.text(cx - tw * 0.5, y, text, col, false);
    };
    // The version (and on Linux the window system), and under it links to
    // its release's page, to Flow's keys and to what each tab uses: a line
    // of their own, so they stay inside the card whatever the language.
    let version = about_version(a.windowing);
    let ink = rgba(FOREGROUND, 0.9 * intro);
    // The window system's logo goes before its name, a space either side,
    // the line centered with it, glowing in its color so it shows on the
    // glass whatever the Glow setting.
    match (a.windowing.and_then(|name| Some((name, version.rfind(name)?))), &a.windowing_logo) {
        (Some((windowing, at)), Some(logo)) => {
            let (before, name) = version.split_at(at);
            let side = (ch * 0.9).round();
            let space = c.text_width(" ");
            let width = c.text_width(before) + side + space + c.text_width(name);
            let vx = c.text(cx - width * 0.5, line_y, before, ink, false);
            let ly = line_y + (ch - side) * 0.5;
            let glow = crate::logos::windowing_glow(windowing, FOREGROUND);
            c.steady_glow([vx, ly, side, side], rgba(glow, 0.45 * intro), side * 0.5, 12.0 * s);
            c.image_over(logo, (vx, ly, side, side), intro);
            c.text(vx + side + space, line_y, name, ink, false);
        }
        _ => centered(c, line_y, &version, ink),
    }
    line_y += ch * 1.3;
    let gap = "  ·  ";
    let links = [(i18n::t("about.release_notes"), Hit::AboutReleaseNotes), (i18n::t("about.keys"), Hit::AboutKeys), (i18n::t("resources.title"), Hit::AboutResources)];
    let width = links.iter().map(|(l, _)| c.text_width(l)).sum::<f32>() + c.text_width(gap) * (links.len() - 1) as f32;
    let mut vx = cx - width * 0.5;
    for (k, (label, hit)) in links.into_iter().enumerate() {
        if k > 0 {
            vx = c.text(vx, line_y, gap, rgba(DIM, 0.6 * intro), false);
        }
        let link = Rect::new(vx, line_y, c.text_width(label), ch);
        let hovered = link.contains(v.hover);
        vx = c.text(vx, line_y, label, rgba(ACCENT, (if hovered { 1.0 } else { 0.85 }) * intro), false);
        if hovered {
            c.rect(link.x, line_y + c.cell.baseline + 2.0, link.w, 1.0_f32.max(s), rgba(ACCENT, 0.9 * intro), 0.0);
        }
        hits.push((link, hit));
    }
    line_y += ch * 1.6;
    centered(c, line_y, i18n::t("about.tagline"), rgba(DIM, intro));
    line_y += ch * 1.2;
    // Who makes Flow, their name a link to their page, the words round it
    // as the language puts them.
    let credits = tf("about.credits", &[("name", &crate::MAKER)]);
    let (before, after) = credits.split_once(crate::MAKER).unwrap_or((credits.as_str(), ""));
    let name_w = c.text_width(crate::MAKER);
    let mut vx = cx - (c.text_width(before) + name_w + c.text_width(after)) * 0.5;
    vx = c.text(vx, line_y, before, rgba(DIM, intro), false);
    let link = Rect::new(vx, line_y, name_w, ch);
    let hovered = link.contains(v.hover);
    vx = c.text(vx, line_y, crate::MAKER, rgba(ACCENT, (if hovered { 1.0 } else { 0.85 }) * intro), false);
    if hovered {
        c.rect(link.x, line_y + c.cell.baseline + 2.0, link.w, 1.0_f32.max(s), rgba(ACCENT, 0.9 * intro), 0.0);
    }
    c.text(vx, line_y, after, rgba(DIM, intro), false);
    hits.push((link, Hit::AboutCredits));
    line_y += ch * 1.2;
    let credits = crate::platform::per_os(
        "Rust · wgpu · alacritty_terminal · cosmic-text · Sparkle",
        "Rust · wgpu · alacritty_terminal · cosmic-text · WinSparkle",
        "Rust · wgpu · alacritty_terminal · cosmic-text",
    );
    centered(c, line_y, credits, rgba(DIM, 0.6 * intro));
    line_y += ch * 1.7;
    for (line, color) in &status {
        centered(c, line_y, line, rgba(*color, intro));
        line_y += ch * 1.1;
    }

    // Buttons. Where Flow shows the updater, its button checks, installs
    // what was found, copies the command that does, or restarts into it;
    // while it works, it waits.
    let ph = ch + 10.0 * s;
    let by = y + h - 26.0 * s - ph;
    let update_label = match &a.update {
        Some(UpdateState::Checking) => Some(i18n::t("about.update.checking_button")),
        Some(UpdateState::Available { .. }) => Some(i18n::t("about.update.install")),
        Some(UpdateState::Installing { .. }) => Some(i18n::t("about.update.installing_button")),
        Some(UpdateState::Installed { .. }) => Some(i18n::t("about.update.restart")),
        Some(UpdateState::ByHand { .. }) if a.copied => Some(i18n::t("toast.copied")),
        Some(UpdateState::ByHand { .. }) => Some(i18n::t("about.update.copy_command")),
        Some(UpdateState::InTab { .. }) => Some(i18n::t("about.update.show_tab")),
        _ => a.can_update.then(|| i18n::t("about.check_updates")),
    };
    let waiting = matches!(a.update, Some(UpdateState::Checking | UpdateState::Installing { .. }));
    let mut labels: Vec<(&str, Hit)> = update_label.map(|label| (label, Hit::AboutUpdate)).into_iter().collect();
    // Restarting within Flow, unless an update waits for Flow to start
    // again, which only the updater's own button does.
    if !matches!(a.update, Some(UpdateState::Installed { .. })) {
        labels.push((i18n::t("about.restart"), Hit::AboutRestart));
    }
    labels.push((i18n::t("about.legal"), Hit::AboutLegal));
    labels.push((i18n::t("common.close"), Hit::AboutClose));
    let widths: Vec<f32> = labels.iter().map(|(l, _)| c.text_width(l) + 28.0 * s).collect();
    let total = widths.iter().sum::<f32>() + 12.0 * s * (labels.len() as f32 - 1.0);
    let mut bx = cx - total * 0.5;
    for ((label, hit), bw) in labels.into_iter().zip(widths) {
        let r = Rect::new(bx, by, bw, ph);
        let primary = hit == Hit::AboutUpdate && !waiting;
        let col = if primary { ACCENT } else { DIM };
        let alpha = if r.contains(v.hover) && !(waiting && hit == Hit::AboutUpdate) { 0.32 } else { 0.18 };
        c.rect(r.x, r.y, r.w, r.h, rgba(col, alpha * intro), ph * 0.5);
        let fg = if primary { rgba(ACCENT, intro) } else if hit == Hit::AboutUpdate { rgba(DIM, intro) } else { rgba(FOREGROUND, intro) };
        c.text(r.x + 14.0 * s, by + (ph - ch) * 0.5, label, fg, primary);
        hits.push((r, hit));
        bx += bw + 12.0 * s;
    }
}

/// A tab's programs as a line: how many, and the busiest; or that they're
/// being measured, or out of sight in WSL. `short` under the table's
/// Programs, which says the rest.
fn programs_line(u: &TabUse, short: bool) -> String {
    use crate::resources::Programs;
    match &u.programs {
        _ if u.wsl && short => "WSL".to_string(),
        _ if u.wsl => t("resources.wsl").to_string(),
        None if short => "…".to_string(),
        None => t("resources.measuring").to_string(),
        Some(Programs { count, busiest: Some(name), .. }) if short => format!("{count} · {name}"),
        Some(Programs { count, .. }) if short => count.to_string(),
        Some(Programs { count, busiest: Some(name), .. }) => tf("resources.programs_busiest", &[("count", count), ("name", name)]),
        Some(Programs { count, .. }) => tf("resources.programs", &[("count", count)]),
    }
}

/// The card under a tab the pointer rests on: its programs, their memory
/// and CPU, and what Flow holds for it.
fn draw_tab_card(c: &mut Canvas<'_>, v: &View<'_>, tab: Rect, u: &TabUse) {
    use crate::resources::{bytes, percent};
    let s = v.layout.scale;
    let ch = c.cell.height;
    let mut lines = vec![programs_line(u, false)];
    if let Some(p) = u.programs.as_ref().filter(|_| !u.wsl) {
        lines.push(tf("resources.memory_cpu", &[("memory", &bytes(p.memory)), ("cpu", &percent(p.cpu))]));
    }
    lines.push(tf("resources.held", &[("scrollback", &bytes(u.scrollback)), ("pictures", &bytes(u.pictures)), ("gpu", &bytes(u.gpu))]));
    let pad = 12.0 * s;
    let w = lines.iter().map(|l| c.text_width(l)).fold(0.0f32, f32::max) + pad * 2.0;
    let h = ch * 1.35 * lines.len() as f32 + pad * 2.0 - ch * 0.35;
    let x = tab.x.min(v.layout.width - w - 8.0 * s).max(8.0 * s);
    let y = tab.y + tab.h + 6.0 * s;
    c.glass(x, y, w, h, rgba(PANEL_TINT, 1.0), 10.0 * s);
    for (i, line) in lines.iter().enumerate() {
        let fg = if i == 0 { rgba(ACCENT, 1.0) } else { rgba(FOREGROUND, 0.85) };
        c.text(x + pad, y + pad + ch * 1.35 * i as f32, line, fg, i == 0);
    }
}

/// About's Resources: a row for each tab (its programs, their memory and
/// CPU, its scrollback, pictures and GPU memory), then Flow's own process
/// and the window's GPU memory, and the frames a second.
fn draw_resources(c: &mut Canvas<'_>, v: &View<'_>, r: &ResourcesView, hits: &mut Vec<(Rect, Hit)>) {
    use crate::resources::{bytes, percent};
    let s = v.layout.scale;
    let ch = c.cell.height;
    c.rect(0.0, 0.0, v.layout.width, v.layout.height, [0.0, 0.0, 0.0, 0.5], 0.0);
    let head: Vec<String> = ["tab", "programs", "memory", "cpu", "scrollback", "pictures", "gpu"].iter().map(|k| t(&format!("resources.column.{k}")).to_string()).collect();
    let dash = || "–".to_string();
    let mut rows: Vec<Vec<String>> = r
        .tabs
        .iter()
        .map(|u| {
            let p = u.programs.as_ref().filter(|_| !u.wsl);
            vec![
                u.label.clone(),
                programs_line(u, true),
                p.map_or_else(dash, |p| bytes(p.memory)),
                p.map_or_else(dash, |p| percent(p.cpu)),
                bytes(u.scrollback),
                bytes(u.pictures),
                bytes(u.gpu),
            ]
        })
        .collect();
    rows.push(vec![
        t("resources.flow").to_string(),
        String::new(),
        r.flow.as_ref().map_or_else(dash, |p| bytes(p.memory)),
        r.flow.as_ref().map_or_else(dash, |p| percent(p.cpu)),
        String::new(),
        String::new(),
        bytes(r.window_gpu),
    ]);
    let gap = 22.0 * s;
    let widths: Vec<f32> = (0..head.len()).map(|i| rows.iter().map(|row| c.text_width(&row[i])).fold(c.text_width(&head[i]), f32::max)).collect();
    let pad = 24.0 * s;
    let row_h = ch * 1.45;
    let note = match r.gpu_ms {
        // How long each took on the GPU, and so how much of the second it
        // was busy.
        Some(ms) => tf("resources.fps_gpu", &[("fps", &r.fps), ("ms", &format!("{ms:.1}")), ("busy", &percent(ms * r.fps as f32 / 1000.0))]),
        None if r.gpu_timed => tf("resources.fps", &[("fps", &r.fps)]),
        None => tf("resources.fps_untimed", &[("fps", &r.fps)]),
    };
    let w = (widths.iter().sum::<f32>() + gap * (widths.len() - 1) as f32 + pad * 2.0).max(420.0 * s).min(v.layout.width - 16.0 * s);
    let note = text::wrap_plain(&note, ((w - pad * 2.0) / c.cell.width).max(10.0) as usize, Style::Normal);
    let h = pad * 2.0 + ch * 1.8 + row_h * (rows.len() + 1) as f32 + ch * (0.6 + note.len() as f32);
    let (x, y) = ((v.layout.width - w) * 0.5, ((v.layout.height - h) * 0.5).max(8.0 * s));
    c.glow(x, y, w, h, rgba(ACCENT2, 0.16), 18.0 * s, 30.0 * s);
    c.glass(x, y, w, h, rgba(PANEL_TINT, 1.0), 18.0 * s);
    hits.push((Rect::new(x, y, w, h), Hit::ResourcesCard));
    c.text(x + pad, y + pad, t("resources.title"), rgba(ACCENT, 1.0), true);
    let close = Rect::new(x + w - pad - ch, y + pad - 4.0 * s, ch + 8.0 * s, ch + 8.0 * s);
    c.text(close.x + 4.0 * s, y + pad, "×", rgba(FOREGROUND, if close.contains(v.hover) { 0.95 } else { 0.5 }), false);
    hits.push((close, Hit::ResourcesClose));
    let mut ty = y + pad + ch * 1.8;
    let row = |c: &mut Canvas<'_>, cells: &[String], ty: f32, fg: Rgba, bold: bool| {
        let mut tx = x + pad;
        for (i, cell) in cells.iter().enumerate() {
            // The words to the left, the numbers to the right of their column.
            let left = i < 2;
            let cx = if left { tx } else { tx + widths[i] - c.text_width(cell) };
            c.text(cx, ty, cell, fg, bold);
            tx += widths[i] + gap;
        }
    };
    row(c, &head, ty, rgba(FOREGROUND, 0.5), false);
    for (i, cells) in rows.iter().enumerate() {
        ty += row_h;
        let flow = i + 1 == rows.len();
        row(c, cells, ty, if flow { rgba(ACCENT2, 0.95) } else { rgba(FOREGROUND, 0.9) }, false);
    }
    for (i, line) in note.iter().enumerate() {
        c.text(x + pad, ty + row_h * 1.2 + ch * i as f32, &text::line_text(line), rgba(DIM, 1.0), false);
    }
}

/// Asked once where Flow checks for updates itself (Linux): whether to
/// look for a newer release every day, as Sparkle and WinSparkle ask.
fn draw_update_ask(c: &mut Canvas<'_>, v: &View<'_>, hits: &mut Vec<(Rect, Hit)>) {
    let s = v.layout.scale;
    let (cw, ch) = (c.cell.width, c.cell.height);
    c.rect(0.0, 0.0, v.layout.width, v.layout.height, [0.0, 0.0, 0.0, 0.35], 0.0);
    let w = (640.0 * s).min(v.layout.width - 40.0 * s);
    let pad = 24.0 * s;
    let cols = ((w - pad * 2.0) / cw) as usize;
    let body = text::wrap_plain(t("update_ask.body"), cols, Style::Normal);
    let button_h = ch + 14.0 * s;
    let h = pad * 2.0 + ch * 1.6 + body.len() as f32 * ch + ch * 0.8 + button_h + ch * 1.4;
    let (x, y) = ((v.layout.width - w) * 0.5, (v.layout.height - h) * 0.5);
    c.glow(x, y, w, h, rgba(ACCENT2, 0.18), 18.0 * s, 30.0 * s);
    c.glass(x, y, w, h, rgba(PANEL_TINT, 1.0), 18.0 * s);
    hits.push((Rect::new(x, y, w, h), Hit::UpdateAskCard));
    let mut ty = y + pad;
    let tx = c.text(x + pad, ty, "↑ ", rgba(ACCENT2, 1.0), true);
    c.text(tx, ty, t("update_ask.title"), rgba(FOREGROUND, 1.0), true);
    ty += ch * 1.6;
    for line in body {
        c.text(x + pad, ty, &text::line_text(&line), rgba(DIM, 1.0), false);
        ty += ch;
    }
    ty += ch * 0.8;
    let mut bx = x + pad;
    for (label, hit, accent) in [(t("update_ask.yes"), Hit::UpdateAskYes, true), (t("update_ask.no"), Hit::UpdateAskNo, false)] {
        let bw = c.text_width(label) + 28.0 * s;
        let r = Rect::new(bx, ty, bw, button_h);
        let hovered = r.contains(v.hover);
        let color = if accent { ACCENT } else { FOREGROUND };
        c.rect(r.x, r.y, r.w, r.h, rgba(color, if hovered { 0.24 } else { 0.14 }), button_h * 0.5);
        c.outline(r.x, r.y, r.w, r.h, rgba(color, 0.5), button_h * 0.5, 1.0);
        c.text(bx + 14.0 * s, ty + (button_h - ch) * 0.5, label, rgba(color, 1.0), accent);
        hits.push((r, hit));
        bx += bw + 10.0 * s;
    }
    c.text(x + pad, y + h - pad - ch, t("update_ask.keys"), rgba(DIM, 0.6), false);
}

/// Asked once an update the user started has put a new Flow in place
/// (`RestartAsk`): restart into it now, Flow's tabs coming back as they
/// are, or later, from About or the title bar's pill.
fn draw_restart_ask(c: &mut Canvas<'_>, v: &View<'_>, ask: &RestartAsk<'_>, hits: &mut Vec<(Rect, Hit)>) {
    let s = v.layout.scale;
    let (cw, ch) = (c.cell.width, c.cell.height);
    c.rect(0.0, 0.0, v.layout.width, v.layout.height, [0.0, 0.0, 0.0, 0.35], 0.0);
    let w = (640.0 * s).min(v.layout.width - 40.0 * s);
    let pad = 24.0 * s;
    let cols = ((w - pad * 2.0) / cw) as usize;
    let title = tf("restart_ask.title", &[("version", &ask.version)]);
    let mut body: Vec<(String, [u8; 3])> = text::wrap_plain(t("restart_ask.body"), cols, Style::Normal).iter().map(|l| (text::line_text(l), DIM)).collect();
    if ask.busy {
        body.extend(text::wrap_plain(t("restart_ask.busy"), cols, Style::Normal).iter().map(|l| (text::line_text(l), WARN)));
    }
    let button_h = ch + 14.0 * s;
    let h = pad * 2.0 + ch * 1.6 + body.len() as f32 * ch + ch * 0.8 + button_h + ch * 1.4;
    let (x, y) = ((v.layout.width - w) * 0.5, (v.layout.height - h) * 0.5);
    c.glow(x, y, w, h, rgba(ACCENT2, 0.18), 18.0 * s, 30.0 * s);
    c.glass(x, y, w, h, rgba(PANEL_TINT, 1.0), 18.0 * s);
    hits.push((Rect::new(x, y, w, h), Hit::RestartAskCard));
    let mut ty = y + pad;
    let tx = c.text(x + pad, ty, "↻ ", rgba(ACCENT2, 1.0), true);
    c.text(tx, ty, &title, rgba(FOREGROUND, 1.0), true);
    ty += ch * 1.6;
    for (line, color) in body {
        c.text(x + pad, ty, &line, rgba(color, 1.0), false);
        ty += ch;
    }
    ty += ch * 0.8;
    let mut bx = x + pad;
    for (label, hit, accent) in [(t("restart_ask.now"), Hit::RestartAskNow, true), (t("restart_ask.later"), Hit::RestartAskLater, false)] {
        let bw = c.text_width(label) + 28.0 * s;
        let r = Rect::new(bx, ty, bw, button_h);
        let hovered = r.contains(v.hover);
        let color = if accent { ACCENT } else { FOREGROUND };
        c.rect(r.x, r.y, r.w, r.h, rgba(color, if hovered { 0.24 } else { 0.14 }), button_h * 0.5);
        c.outline(r.x, r.y, r.w, r.h, rgba(color, 0.5), button_h * 0.5, 1.0);
        c.text(bx + 14.0 * s, ty + (button_h - ch) * 0.5, label, rgba(color, 1.0), accent);
        hits.push((r, hit));
        bx += bw + 10.0 * s;
    }
    c.text(x + pad, y + h - pad - ch, t("restart_ask.keys"), rgba(DIM, 0.6), false);
}

#[cfg(test)]
mod steps_tests {
    use super::*;

    fn steps(statuses: &[StepStatus]) -> Vec<(String, StepStatus)> {
        statuses.iter().enumerate().map(|(i, st)| (format!("step {i}"), *st)).collect()
    }

    #[test]
    fn the_panel_shows_the_steps_round_the_one_working() {
        use StepStatus::*;
        assert_eq!(steps_window(&steps(&[Done, Working, Pending]), 5), (0..3, 0));
        let seven = steps(&[Done, Done, Done, Working, Pending, Pending, Pending]);
        assert_eq!(steps_window(&seven, 5), (2..7, 2), "one done before it, the rest after");
        assert_eq!(steps_window(&steps(&[Working, Pending, Pending, Pending, Pending, Pending, Pending]), 5), (0..5, 2));
        assert_eq!(steps_window(&steps(&[Done; 7]), 5), (2..7, 2), "all done: the last");
        assert_eq!(steps_window(&steps(&[Done, Done, Pending, Pending, Pending, Pending, Pending]), 5), (1..6, 2), "none working: the first to come");
        assert_eq!(steps_spoken(&[]), None);
        assert!(steps_spoken(&seven).unwrap().contains("step 3"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pinned_sidebars_room_moves_the_terminal_and_the_panel_right_the_title_bar_staying_whole() {
        let cell = (10.0, 20.0);
        let plain = Layout::compute(1200.0, 800.0, 1.0, cell, Around::new(true, false));
        let beside = Layout::compute(1200.0, 800.0, 1.0, cell, Around { left: 268.0, ..Around::new(true, false) });
        assert_eq!(beside.title, plain.title, "the title bar stays the window's whole width");
        assert_eq!((beside.term.x, beside.term.w), (plain.term.x + 268.0, plain.term.w - 268.0));
        assert_eq!((beside.cols, plain.cols), (90, 117));
        let (p, q) = (beside.panel.unwrap(), plain.panel.unwrap());
        assert_eq!((p.x, p.w), (q.x + 268.0, q.w - 268.0), "the panel beside it too");
        // Fit window to's window is wider by its room.
        let (w, h) = Layout::size_for(80, 24, 1.0, cell, Around::default());
        assert_eq!(Layout::size_for(80, 24, 1.0, cell, Around { left: 268.0, ..Around::default() }), (w + 268, h));
    }

    #[test]
    fn abouts_version_says_the_window_system_where_there_is_a_choice() {
        let version = crate::version::display();
        crate::i18n::set_language("en", &[]);
        assert_eq!(about_version(None), format!("Version {version}"));
        assert_eq!(about_version(Some("Wayland")), format!("Version {version} · Wayland"));
        crate::i18n::set_language("ja", &[]);
        assert_eq!(about_version(Some("X11")), format!("バージョン {version} · X11"));
    }

    #[test]
    fn a_shell_windows_frame_leaves_the_text_room_inside_it_on_whole_pixels() {
        let outer = Rect::new(20.4, 50.6, 801.3, 600.2);
        for scale in [1.0, 2.0] {
            let cell = (9.5 * scale, 19.0 * scale);
            let (f, room) = Frame::round(outer, cell, scale);
            assert_eq!((f.outer.x, f.outer.y, f.outer.w, f.outer.h), (20.0, 51.0, 801.0, 600.0));
            assert_eq!(f.line, 2.0 * scale);
            assert!(f.bar >= cell.1 + 2.0 * f.line - 0.5 && f.bar == f.bar.round(), "a cell tall, the outline more");
            // The text inside the outline and below the bar, a gap from each.
            assert!(room.x > f.outer.x + f.line && room.y > f.outer.y + f.bar);
            assert!(room.x + room.w < f.outer.x + f.outer.w - f.line && room.y + room.h < f.outer.y + f.outer.h - f.line);
            assert!([room.x, room.y].iter().all(|v| *v == v.round()));
            // A pane laid out in it holds what that room holds.
            let l = Layout::compute(900.0, 700.0, scale, cell, Around::default());
            let framed = Layout { frame: Some(f), ..l.within_cell(room, cell) };
            assert_eq!(framed.cols, (room.w / cell.0) as usize);
            assert!(framed.cols < l.cols && framed.rows < l.rows);
            assert_eq!(l.within_cell(l.term, cell).frame, None, "a pane's own layout has none of another's");
        }
    }

    #[test]
    fn a_panes_close_is_at_its_top_right_left_of_its_scrollbar_and_under_the_find_bar() {
        let term = Rect::new(400.0, 40.0, 380.0, 600.0);
        let r = pane_close_rect(term, 0.0, 0.0, 18.0);
        assert_eq!((r.x + r.w, r.y, r.w, r.h), (780.0, 40.0, 18.0, 18.0), "at the top right corner, a cell square");
        let r = pane_close_rect(term, 16.0, 0.0, 18.0);
        assert_eq!(r.x + r.w, 764.0, "left of the scrollbar's strip");
        let r = pane_close_rect(term, 16.0, 40.0, 18.0);
        assert_eq!(r.y, 80.0, "below the find bar");
        assert!(term.contains((r.x, r.y)) && term.contains((r.x + r.w - 0.1, r.y + r.h)));
    }

    #[test]
    fn a_frames_close_box_is_square_at_its_title_bars_left_above_the_text() {
        for scale in [1.0, 2.0] {
            let cell = (9.5 * scale, 19.0 * scale);
            let (f, room) = Frame::round(Rect::new(20.0, 50.0, 800.0, 600.0), cell, scale);
            let b = f.close_box();
            assert_eq!(b.w, b.h, "square");
            assert_eq!((b.x, b.y), (f.outer.x + f.line, f.outer.y), "inside the outline, at the left");
            assert_eq!(b.y + b.h, f.outer.y + f.bar, "as tall as the bar");
            assert!(b.y + b.h < room.y, "above the text, which a click there never reaches");
            assert!([b.x, b.y, b.w].iter().all(|v| *v == v.round()), "on whole pixels");
        }
    }

    /// A gadget's pieces painted on the machine's own pixels, a row for
    /// each of its lines: `#` its white, `.` its blue, `k` black.
    fn painted(pieces: &[Piece], w: usize, h: usize) -> Vec<String> {
        (0..h)
            .map(|l| {
                (0..w)
                    .map(|x| {
                        let (x, y) = (x as f32 + 0.5, l as f32 + 0.5);
                        match pieces.iter().rev().find(|p| p.0 <= x && x < p.2 && p.1 <= y && y < p.3).map(|p| p.4) {
                            Some(Pen::White) => '#',
                            Some(Pen::Blue) => '.',
                            Some(Pen::Black) => 'k',
                            None => ' ',
                        }
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn the_gadgets_are_the_machines_pixel_for_pixel() {
        // As WinUAE showed AmigaShell's window under Kickstart 1.3, a line
        // of it to a row.
        let front = [
            "######################",
            "##..............######",
            "##..##########..######",
            "##..##kkkkkkkkkkkkkk##",
            "##..##kkkkkkkkkkkkkk##",
            "##..##kkkkkkkkkkkkkk##",
            "##....kkkkkkkkkkkkkk##",
            "######kkkkkkkkkkkkkk##",
            "######kkkkkkkkkkkkkk##",
            "######################",
        ];
        let back = [
            "######################",
            "##kkkkkkkkkkkkkk######",
            "##kkkk..............##",
            "##kkkk..##########..##",
            "##kkkk..##########..##",
            "##kkkk..##########..##",
            "##kkkk..##########..##",
            "######..##########..##",
            "######..............##",
            "######################",
        ];
        let sizing = [
            "################",
            "##......########",
            "##..##..########",
            "##............##",
            "######..####..##",
            "######..####..##",
            "######..####..##",
            "######........##",
            "################",
        ];
        assert_eq!(painted(&TO_FRONT, 22, 10), front);
        assert_eq!(painted(&TO_BACK, 22, 10), back);
        assert_eq!(painted(&SIZING, 16, 9), sizing);
    }

    #[test]
    fn a_frames_title_bar_has_three_white_lines_as_far_apart_as_they_are_thick() {
        // The machine's own: a bar of 20 pixels, lines of 4, 4 apart.
        let (f, _) = Frame::round(Rect::new(20.0, 50.0, 800.0, 600.0), (8.0, 16.0), 1.0);
        assert_eq!(f.bar, 20.0);
        assert_eq!(f.stripes(), [(50.0, 54.0), (58.0, 62.0), (66.0, 70.0)]);
        for scale in [1.0, 1.5, 2.0] {
            let (f, _) = Frame::round(Rect::new(20.0, 50.0, 800.0, 600.0), (9.5 * scale, 19.0 * scale), scale);
            let s = f.stripes();
            assert_eq!((s[0].0, s[2].1), (f.outer.y, f.outer.y + f.bar), "the bar's top and its foot");
            assert!(s.iter().all(|(top, bottom)| bottom > top && *top == top.round()));
            assert!(s[0].1 < s[1].0 && s[1].1 < s[2].0, "a gap between each");
        }
    }

    #[test]
    fn a_frames_gadgets_sit_at_its_bars_right_and_its_sizing_gadget_in_its_corner_clear_of_the_text() {
        let (f, room) = Frame::round(Rect::new(20.0, 50.0, 800.0, 600.0), (8.0, 16.0), 1.0);
        let (front, back) = f.gadgets().expect("room for them");
        assert_eq!((front.w, front.h, back.w, back.h), (22.0, 20.0, 22.0, 20.0), "the machine's size");
        assert_eq!(back.x + back.w, 820.0 - f.line - 2.0, "a gap from the outline");
        assert_eq!(back.x - (front.x + front.w), 2.0, "a gap apart");
        assert_eq!(f.lines_end(), (front.x - 5.0, front.x - 2.0), "the lines' white end, a gap before them");
        let size = f.size_box();
        assert_eq!((size.x, size.y, size.w, size.h), (804.0, 632.0, 16.0, 18.0), "in the corner, over the outline");
        assert!(room.y + room.h <= size.y - f.line, "the text ends above it");
        for scale in [1.0, 1.25, 2.0] {
            let (f, room) = Frame::round(Rect::new(20.4, 50.6, 801.3, 600.2), (9.5 * scale, 19.0 * scale), scale);
            let (front, back) = f.gadgets().unwrap();
            let size = f.size_box();
            for r in [front, back, size] {
                assert!([r.x, r.y, r.w, r.h].iter().all(|v| *v == v.round()), "on whole pixels at {scale}");
            }
            assert_eq!((front.y, front.h), (f.outer.y, f.bar), "as tall as the bar");
            assert!(front.x > f.close_box().x + f.close_box().w + f.bar, "right of the close box and the lines");
            assert_eq!((size.x + size.w, size.y + size.h), (f.outer.x + f.outer.w, f.outer.y + f.outer.h));
            assert!(room.y + room.h < size.y && room.x + room.w < f.outer.x + f.outer.w - f.line);
        }
    }

    #[test]
    fn a_narrow_frame_leaves_its_gadgets_out_and_its_lines_run_to_the_outline() {
        let (f, _) = Frame::round(Rect::new(0.0, 0.0, 90.0, 300.0), (8.0, 16.0), 1.0);
        assert_eq!(f.gadgets(), None);
        assert_eq!(f.lines_end(), (90.0 - f.line, 90.0 - f.line));
        let (f, _) = Frame::round(Rect::new(0.0, 0.0, 200.0, 300.0), (8.0, 16.0), 1.0);
        assert!(f.gadgets().is_some(), "a pane of 25 columns has them");
    }

    #[test]
    fn a_list_cut_to_the_window_moves_only_as_far_as_its_choice_takes_it() {
        // Ten rows, four shown: down from the top, the list stays until the
        // choice passes its foot, then moves a row at a time.
        assert_eq!(keep_in_view(0, 3, 4, 10), 0);
        assert_eq!(keep_in_view(0, 4, 4, 10), 1);
        assert_eq!(keep_in_view(1, 9, 4, 10), 6);
        // Back up, it stays until the choice passes its head.
        assert_eq!(keep_in_view(6, 7, 4, 10), 6);
        assert_eq!(keep_in_view(6, 2, 4, 10), 2);
        // A list that fits, or a window with no room at all, still shows
        // the choice.
        assert_eq!(keep_in_view(3, 2, 12, 10), 0);
        assert_eq!(keep_in_view(0, 5, 0, 10), 5);
    }

    /// Three rows of ten-pixel characters, the second and third in view.
    fn laid() -> PanelLaid {
        let rows: Vec<String> = ["› free up space", "Run this:", "  du -sh ~"].map(String::from).to_vec();
        let edges = rows[1..].iter().map(|r| (0..=r.chars().count()).map(|i| 100.0 + 10.0 * i as f32).collect()).collect();
        PanelLaid { cols: 40, rows, wraps: Vec::new(), area: Rect::new(100.0, 50.0, 300.0, 40.0), start: 1, top: 50.0, row_h: 20.0, edges }
    }

    #[test]
    fn the_place_under_the_pointer_is_the_character_nearest_its_left_half() {
        let laid = laid();
        assert_eq!(laid.place_at((100.0, 55.0), false), Some((1, 0)));
        assert_eq!(laid.place_at((104.0, 55.0), false), Some((1, 0)), "the left half of the first character");
        assert_eq!(laid.place_at((114.0, 55.0), false), Some((1, 1)), "nearer the edge after it");
        assert_eq!(laid.place_at((116.0, 75.0), false), Some((2, 2)));
        assert_eq!(laid.place_at((390.0, 75.0), false), Some((2, 10)), "past the end, the end");
        assert_eq!(laid.place_at((116.0, 10.0), false), None, "above the conversation");
        assert_eq!(laid.place_at((116.0, 10.0), true), Some((1, 2)), "a drag that left it stays in view");
    }

    #[test]
    fn selected_text_runs_from_place_to_place_a_row_to_a_line() {
        let laid = laid();
        assert_eq!(laid.text_between((1, 4), (2, 10)), "this:\n  du -sh ~");
        assert_eq!(laid.text_between((2, 10), (1, 4)), "this:\n  du -sh ~", "dragged up");
        assert_eq!(laid.text_between((0, 2), (0, 6)), "free");
        let mut ai = AiPanel::new(false);
        *ai.laid.borrow_mut() = laid;
        ai.selection = Some(((2, 0), (2, 2)));
        assert_eq!(ai.selected_text(), None, "blanks alone are nothing to copy");
        ai.selection = Some(((2, 2), (2, 10)));
        assert_eq!(ai.selected_text().as_deref(), Some("du -sh ~"));
    }

    #[test]
    fn a_selection_copies_a_line_the_panel_wrapped_as_one() {
        // What went wrong: a command too long for the panel copied as one
        // command per row, and the shell ran it as several.
        let command = "sudo iptables -P FORWARD DROP && sudo iptables -D FORWARD -i eno1 -o wlp195s0 -j ACCEPT";
        let mut ai = AiPanel::new(false);
        ai.entries.push(Entry::User(format!("Tighten it: {command}")));
        ai.entries.push(Entry::Assistant(format!("Run this:\n```bash\n{command}\n```\nThen `netfilter-persistent save` keeps it.")));
        let rows = layout_entries(&ai, 30, Instant::now(), 0.0, 6);
        let laid = PanelLaid {
            rows: rows.iter().map(|row| row.spans.iter().map(|(s, ..)| s.as_str()).collect()).collect(),
            wraps: rows.iter().map(|row| row.wrap.clone()).collect(),
            ..PanelLaid::default()
        };
        assert!(laid.rows.len() > 10, "laid out over many rows: {:?}", laid.rows);
        let all = laid.text_between((0, 0), (laid.rows.len() - 1, usize::MAX));
        // The block's buttons are a row of their own, which copies as an
        // empty line, as it always did.
        assert_eq!(all, format!("› Tighten it: {command}\n\nRun this:\n  {command}\n\nThen netfilter-persistent save keeps it."));
        // From inside the block, down into a row the wrap made: no line
        // break, and not the two columns the block is set in.
        let first = laid.rows.iter().position(|row| row.starts_with("  sudo")).unwrap();
        assert_eq!(laid.text_between((first, 2), (first + 1, usize::MAX)), format!("{} {}", laid.rows[first].trim(), laid.rows[first + 1].trim()));
    }

    #[test]
    fn a_line_of_nothing_but_code_has_its_copy_at_its_end_and_a_fenced_block_its_buttons_under_it() {
        let mut ai = AiPanel::new(false);
        let reply = "Install it:\n`brew install ripgrep`\nIts settings are in\n`config.toml`\nand `rg` searches. Then:\n- `make`\n```bash\nls\n```";
        ai.entries.push(Entry::Assistant(reply.into()));
        let rows = layout_entries(&ai, 60, Instant::now(), 0.0, 6);
        // Each span alone on its line has a Copy at that line's end, command
        // or not, and the span among words none; the fenced block has a row
        // of Copy and Run under it: the entry, the block and whether it has
        // Run.
        let copies: Vec<_> = rows.iter().enumerate().filter_map(|(i, row)| row.copy.map(|hit| (i, hit))).collect();
        assert_eq!(copies, [(1, Hit::CodeCopy(0, 0)), (3, Hit::CodeCopy(0, 1)), (5, Hit::CodeCopy(0, 2))]);
        let buttons: Vec<_> = rows.iter().enumerate().filter_map(|(i, row)| row.code.map(|code| (i, code))).collect();
        assert_eq!(buttons, [(7, (0, 3, true))]);
        assert_eq!(rows.len(), 8);
    }

    #[test]
    fn a_span_ending_a_reply_still_coming_in_has_its_copy_once_its_line_ends() {
        let copies = |ai: &AiPanel| layout_entries(ai, 60, Instant::now(), 0.0, 6).iter().filter(|row| row.copy.is_some()).count();
        let mut ai = AiPanel::new(false);
        ai.busy = true;
        ai.push_text("Run:\n`make`");
        assert_eq!(copies(&ai), 0, "it may yet go on");
        ai.push_text("\n");
        assert_eq!(copies(&ai), 1);
        ai.push_text("`make test` checks it");
        assert_eq!(copies(&ai), 1, "the line after it isn't one, however it ends");
        // A reply done, or with more after it, has it however it ends.
        ai.entries = vec![Entry::Assistant("Run:\n`make`".into())];
        ai.busy = false;
        assert_eq!(copies(&ai), 1);
        ai.busy = true;
        ai.entries.push(Entry::Notice("…".into()));
        assert_eq!(copies(&ai), 1);
    }

    #[test]
    fn copy_reply_goes_on_the_replys_first_row_in_view_without_a_copy_of_its_own() {
        let mut ai = AiPanel::new(false);
        let path = "~/Library/Application Support/Flow/presets/rain.json";
        ai.entries.push(Entry::Assistant(format!("`{path}`\n- `{path}`\nThose two.")));
        let rows = layout_entries(&ai, 60, Instant::now(), 0.0, 6);
        // Each long span keeps its line, with its Copy at the end, which
        // Copy reply at the same end would cover.
        assert_eq!(rows.len(), 3);
        assert!(rows[0].copy.is_some() && rows[1].copy.is_some());
        assert_eq!(reply_copy_row(&rows, Some(0)), Some(2));
        // Scrolled so only those are in view, it has no row.
        assert_eq!(reply_copy_row(&rows[..2], Some(0)), None);
        assert_eq!(reply_copy_row(&rows, None), None);
    }

    #[test]
    fn a_commands_card_has_copy_once_it_ran_and_beside_run_while_it_waits() {
        let card = |status, command: Option<&str>| {
            Entry::Tool(ToolCard { title: "ls -la".into(), command: command.map(Into::into), detail: String::new(), risk: None, status, result: "exit 0".into(), started: None, plan: false, undo: None })
        };
        let mut ai = AiPanel::new(false);
        ai.entries = vec![
            card(CardStatus::Done { ok: true }, Some("ls -la")),
            card(CardStatus::Waiting, Some("rm old.dmg")),
            card(CardStatus::Done { ok: true }, None),
            card(CardStatus::Queued, Some("pwd")),
            card(CardStatus::Skipped, Some("pwd")),
        ];
        let rows = layout_entries(&ai, 60, Instant::now(), 0.0, 6);
        // On the command's line once it ran, on the row of Run, Edit and
        // Skip while it waits; none on a card that isn't a command's, or one
        // that hasn't run.
        let copies: Vec<_> = rows.iter().filter_map(|row| row.copy.map(|hit| (hit, row.buttons, line_text(row)))).collect();
        assert_eq!(copies, [(Hit::CardCopy(0), false, " ✓ ls -la".to_string()), (Hit::CardCopy(1), true, String::new())]);
    }

    /// A row's text as drawn.
    fn line_text(row: &Row) -> String {
        row.spans.iter().map(|(s, ..)| s.as_str()).collect()
    }

    #[test]
    fn a_pane_in_a_font_of_its_own_has_a_grid_of_its_cells() {
        let l = Layout::compute(1000.0, 700.0, 1.0, (10.0, 20.0), Around::default());
        let r = Rect::new(l.term.x, l.term.y, 400.0, 300.0);
        let mine = l.within(r);
        assert_eq!((mine.cols, mine.rows), (40, 15));
        let wider = l.within_cell(r, (8.0, 16.0));
        assert_eq!((wider.cols, wider.rows, wider.cell_w, wider.cell_h), (50, 18, 8.0, 16.0));
        assert_eq!(wider.term, r);
    }

    #[test]
    fn a_window_fitted_for_a_pane_holds_the_grid_in_that_pane() {
        let (scale, cell) = (2.0, (18.0, 38.0));
        // The left of two panes side by side, 6 pixels apart.
        let left = |l: &Layout| {
            let w = ((l.term.w - 6.0) / 2.0).floor();
            ((w / l.cell_w) as usize, l.rows)
        };
        let whole = Layout::size_for(80, 24, scale, cell, Around::default());
        let (w, h) = Layout::size_for_part(left, 80, 24, scale, cell, Around::default());
        assert!(w > whole.0 + 80 * 18 - 10, "about twice as wide: {w} for {}", whole.0);
        assert_eq!(h, whole.1, "as tall");
        let at = Layout::compute(w as f32, h as f32, scale, cell, Around::default());
        assert_eq!(left(&at), (80, 24));
        let narrower = Layout::compute(w as f32 - 1.0, h as f32, scale, cell, Around::default());
        assert!(left(&narrower).0 < 80, "the least that does");
    }

    #[test]
    fn the_text_size_for_a_grid_the_least_window_holds_too_much_of_holds_no_more() {
        // Menlo's cell at 15 points, two pixels to a point, and the least
        // window, 520 × 320 points: 54 columns, more than a home computer's.
        let (scale, window) = (2.0, (1040, 640));
        let cell_at = |size: f32| (18.06 * size / 15.0, 38.0 * size / 15.0);
        let grid = |size| {
            let l = Layout::compute(window.0 as f32, window.1 as f32, scale, cell_at(size), Around::default());
            (l.cols, l.rows)
        };
        assert_eq!(grid(15.0).0, 54);
        let size = Layout::text_size_within((40, 25), window, scale, cell_at(15.0), Around::default(), 15.0);
        assert!(grid(size).0 <= 40 && grid(size - 1.0).0 > 40, "{size}: {:?} {:?}", grid(size), grid(size - 1.0));
        // A grid it holds already needs no larger letters.
        assert!(Layout::text_size_within((80, 24), window, scale, cell_at(15.0), Around::default(), 15.0) <= 15.0);
    }

    #[test]
    fn the_window_sized_for_a_grid_is_the_smallest_that_holds_it() {
        // Fractional cells, at two pixels to a point, with and without the
        // panel and a border, whose margins grow with the window.
        let cell = (16.8125, 34.0);
        for (panel, border) in [(false, false), (true, false), (false, true), (true, true)] {
            for (cols, rows) in [(40, 25), (80, 24), (80, 25), (80, 34), (132, 43)] {
                let (w, h) = Layout::size_for(cols, rows, 2.0, cell, Around::new(panel, border));
                let at = |w: u32, h: u32| {
                    let l = Layout::compute(w as f32, h as f32, 2.0, cell, Around::new(panel, border));
                    (l.cols, l.rows)
                };
                assert_eq!(at(w, h), (cols, rows), "{cols} × {rows}, panel {panel}, border {border}");
                assert_eq!(at(w - 1, h).0, cols - 1, "a pixel narrower holds a column less");
                assert_eq!(at(w, h - 1).1, rows - 1, "a pixel shorter holds a row less");
            }
        }
    }

    #[test]
    fn a_short_window_with_the_panel_open_gives_the_terminal_what_room_there_is() {
        // Shorter than the panel's least and the terminal's room together.
        let l = Layout::compute(1040.0, 660.0, 2.0, (16.8, 34.0), Around::new(true, false));
        let panel = l.panel.unwrap();
        assert!(panel.h >= 240.0 && l.term.y + l.term.h <= panel.y, "{panel:?} {:?}", l.term);
    }

    #[test]
    fn the_pointer_on_a_double_line_falls_in_its_cells_twice_as_wide() {
        let l = Layout::compute(1000.0, 600.0, 1.0, (10.0, 20.0), Around::default());
        let at = |col: f32, row: f32| (l.term.x + col * 10.0, l.term.y + row * 20.0 + 5.0);
        assert_eq!(l.cell_on_line(at(7.7, 2.0), LineSize::Single), (7, 2, true));
        assert_eq!(l.cell_on_line(at(7.7, 2.0), LineSize::Wide), (3, 2, true), "columns 6 and 7 are the fourth cell");
        assert_eq!(l.cell_on_line(at(6.4, 3.0), LineSize::Bottom), (3, 3, false));
        // Past the half it shows, the last cell there.
        assert_eq!(l.cell_on_line(at(l.cols as f32 - 0.5, 0.0), LineSize::Top).0, l.cols / 2 - 1);
    }

    #[test]
    fn a_pictures_cell_on_a_double_line_is_drawn_as_its_text_is() {
        // Cells 10 × 20; the cell's piece is its right half, the picture's
        // middle tenth across and its top quarter down.
        let (cell, piece, uv) = ((10.0, 20.0), [5.0, 0.0, 5.0, 20.0], [0.45, 0.0, 0.55, 0.25]);
        let grid = Grid { x: 100.0, y: 40.0, cw: 10.0, ch: 20.0 };
        let at = |size| CharCell::new(size, grid, 0.0, if size == LineSize::Single { 1.0 } else { 2.0 }, 0.0).picture(cell, piece, uv);
        assert_eq!(at(LineSize::Single), Some(([105.0, 40.0, 5.0, 20.0], uv)));
        assert_eq!(at(LineSize::Wide), Some(([110.0, 40.0, 10.0, 20.0], uv)), "twice as wide");
        // Twice as tall too, and each half of the line shows half of it.
        assert_eq!(at(LineSize::Top), Some(([110.0, 40.0, 10.0, 20.0], [0.45, 0.0, 0.55, 0.125])));
        assert_eq!(at(LineSize::Bottom), Some(([110.0, 40.0, 10.0, 20.0], [0.45, 0.125, 0.55, 0.25])));
        // A piece only in the cell's bottom half shows only on the bottom line.
        let low = [0.0, 12.0, 10.0, 8.0];
        let at = |size| CharCell::new(size, grid, 0.0, 2.0, 0.0).picture(cell, low, uv);
        assert_eq!(at(LineSize::Top), None);
        assert_eq!(at(LineSize::Bottom), Some(([100.0, 44.0, 20.0, 16.0], uv)));
    }

    #[test]
    fn cells_side_by_side_share_the_edge_between_them_to_the_pixel() {
        // Cells 10.095… pixels wide from x 14: a cell's left edge plus its
        // width comes to 1447.5 at column 141, which rounds up, while
        // column 142's left edge is a step short of it and rounds down, so
        // the two would overlap by a pixel. Worked out from the column, the
        // edge they share is one number.
        let cw = f32::from_bits(0x4121_8568);
        let cells = Grid { x: 14.0, y: 3.3, cw, ch: 19.4 };
        let (x, next) = (cells.x(141.0), cells.x(142.0));
        assert_ne!((x + cw).round(), next.round(), "the sum this replaces");
        for size in [LineSize::Single, LineSize::Wide, LineSize::Top, LineSize::Bottom] {
            let span = if size.doubled() { 2.0 } else { 1.0 };
            for col in 0..300 {
                // A wide character's cell and the one after it, and the
                // cell below, whose top is this one's bottom.
                for width in [1.0, 2.0] {
                    let first = col as f32 * span;
                    let (here, after) = (CharCell::new(size, cells, first, width * span, 5.0), CharCell::new(size, cells, first + width * span, span, 5.0));
                    assert_eq!(here.edges[2], after.edges[0], "{size:?} column {col}");
                }
                let (here, below) = (CharCell::new(LineSize::Single, cells, col as f32, 1.0, 5.0), CharCell::new(LineSize::Single, cells, col as f32, 1.0, 6.0));
                assert_eq!(here.edges[3], below.edges[1]);
            }
        }
        // A double height's halves draw from the same doubled cell.
        let (top, bottom) = (CharCell::new(LineSize::Top, cells, 4.0, 2.0, 7.0), CharCell::new(LineSize::Bottom, cells, 4.0, 2.0, 8.0));
        assert_eq!(top.edges, bottom.edges);
    }

    #[test]
    fn a_composition_shows_its_caret_until_a_part_is_being_converted() {
        // "にほんご" as typed: the caret after the third kana (byte 9).
        let typing = Preedit { text: "にほんご".into(), cursor: Some((9, 9)) };
        assert_eq!(typing.caret(), Some(3));
        assert!(typing.marked().iter().all(|&(_, converting)| !converting));
        // Converted to "日本語", with "日本" the part being converted.
        let converting = Preedit { text: "日本語".into(), cursor: Some((0, 6)) };
        assert_eq!(converting.caret(), None);
        assert_eq!(converting.marked(), vec![('日', true), ('本', true), ('語', false)]);
        // Without a cursor, the caret is at the end.
        assert_eq!(Preedit { text: "かな".into(), cursor: None }.caret(), Some(2));
    }
}
