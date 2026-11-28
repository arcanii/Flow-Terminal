//! What Flow shows, as the tree screen readers read (VoiceOver, Narrator,
//! Orca), through AccessKit and its winit adapter.
//!
//! Flow draws everything itself, on the GPU, so a reader sees nothing of it
//! unless told. `App::access_view` gathers what's on screen into an
//! [`AccessView`], plain data, and [`tree`] makes AccessKit's tree from that
//! alone, so it's tested here without a window. The tree also says what each
//! node a reader may act on does (a [`Target`]); [`what_to_do`] turns a
//! reader's request into a [`Do`], which the app does as the keys or a click
//! there would (`App::do_for_reader`). The app sends a tree only while a
//! reader has asked for one, and only when the view has changed
//! (`App::tell_readers`).

use std::collections::HashMap;

use accesskit::{Action, ActionData, ActionRequest, Live, Node, NodeId, Role, TextPosition, TextSelection, Toggled, TreeId, TreeInfo, TreeUpdate};

use crate::i18n::{t, tf};
use crate::settings::{self, Change, Control, Entry};
use crate::ui::Rect;

/// Everything on screen a reader reads, gathered from the app each frame
/// while a reader is on.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AccessView {
    /// The window's size in pixels, and its name: Flow and the tab in front.
    pub size: (f32, f32),
    pub title: String,
    /// The title bar: what comes before the tabs (Flow's name, which opens
    /// About), the tabs and which is in front, then its buttons.
    pub lead: Vec<Button>,
    pub tabs: Vec<TabItem>,
    pub active: usize,
    pub bar: Vec<Button>,
    /// The tab in front's panes, one where it isn't split.
    pub panes: Vec<Screen>,
    /// Each pane's pill while it's scrolled back, saying how many lines
    /// are below, which takes it back to the bottom.
    pub scrolled: Vec<Button>,
    /// What closes each pane that has a close, by its place in its tab: a
    /// theme's frame's close box, or a split tab's ✕, which shows only
    /// under the pointer, so a reader's press closes the pane itself.
    pub closes: Vec<(usize, Button)>,
    /// Ask why, at the terminal's foot after a command failed.
    pub ask_why: Option<Button>,
    pub panel: Option<Panel>,
    /// The assistant's last reply once it's done, said as it comes.
    pub reply: Option<String>,
    pub settings: Option<SettingsView>,
    /// What's open over the rest, the topmost last.
    pub dialogs: Vec<Dialog>,
    pub toast: Option<String>,
    /// Caps Lock's notice, while it shows: said at once, as it comes.
    pub caps_lock: Option<String>,
    /// What has the keys.
    pub focus: Focus,
}

/// A button, and whether it's on where it stays on or off.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Button {
    pub label: String,
    pub rect: Option<Rect>,
    pub on: Option<bool>,
}

impl Button {
    pub fn new(label: impl Into<String>, rect: Option<Rect>) -> Self {
        Button { label: spoken(&label.into()), rect, on: None }
    }

    pub fn toggle(label: impl Into<String>, rect: Option<Rect>, on: bool) -> Self {
        Button { on: Some(on), ..Button::new(label, rect) }
    }
}

/// A tab in the title bar (or in the row under Settings' title), and its ×.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TabItem {
    pub label: String,
    pub rect: Option<Rect>,
    pub close: Option<Rect>,
}

/// A pane's terminal: its rows in view and the cursor among them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Screen {
    pub label: String,
    /// The room its cells are in, and a cell's size.
    pub rect: Rect,
    pub cell: (f32, f32),
    pub rows: Vec<Row>,
    /// The cursor's row and how many letters come before it, while it
    /// shows in view.
    pub cursor: Option<(usize, usize)>,
}

/// A row of a terminal: its text, its letters, and whether it runs on into
/// the next (wrapped) rather than ending.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Row {
    pub text: String,
    pub letters: Vec<Letter>,
    pub wraps: bool,
}

/// A letter of a row: its bytes in the row's text (with any marks combined
/// on it), the column it starts in and the cells it takes.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Letter {
    pub bytes: u8,
    pub col: u16,
    pub cells: u8,
}

/// The assistant's panel: its conversation as laid out (`ui::PanelLaid`),
/// its input and its buttons.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Panel {
    pub label: String,
    pub rect: Rect,
    pub rows: Vec<String>,
    /// The first row in view, how many are, the first one's top, a row's
    /// height and the conversation's room.
    pub start: usize,
    pub in_view: usize,
    pub top: f32,
    pub row_h: f32,
    pub area: Rect,
    pub input: Field,
    pub buttons: Vec<Button>,
}

/// A field typed in: its lines and the caret's line and letter.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Field {
    pub label: String,
    pub placeholder: String,
    pub lines: Vec<String>,
    pub caret: (usize, usize),
    /// Where the selection began, the caret being its other end, if
    /// something is selected.
    pub anchor: Option<(usize, usize)>,
    pub multiline: bool,
    /// An API key, which Flow never shows: read as dots.
    pub secret: bool,
    /// A search box (Settings' search) rather than a plain field.
    pub search: bool,
    pub rect: Option<Rect>,
}

impl Field {
    /// A line of text with the caret at its end.
    pub fn line(label: impl Into<String>, placeholder: impl Into<String>, text: &str, rect: Option<Rect>) -> Self {
        Field { label: label.into(), placeholder: placeholder.into(), lines: vec![text.to_string()], caret: (0, text.chars().count()), rect, ..Field::default() }
    }

    /// `text`, its lines parted by `\n`, with the caret `caret` letters in.
    pub fn text(label: impl Into<String>, placeholder: impl Into<String>, text: &str, caret: usize, rect: Option<Rect>) -> Self {
        let lines: Vec<String> = text.split('\n').map(str::to_string).collect();
        let at = line_and_letter(&lines, caret);
        Field { label: label.into(), placeholder: placeholder.into(), multiline: lines.len() > 1, lines, caret: at, rect, ..Field::default() }
    }

    /// The field with what's selected from `anchor` letters in to the caret.
    pub fn selected_from(mut self, anchor: Option<usize>) -> Self {
        self.anchor = anchor.map(|a| line_and_letter(&self.lines, a));
        self
    }
}

/// The line and the letter along it that letter `at` of `lines`, parted by
/// line breaks, is at.
fn line_and_letter(lines: &[String], at: usize) -> (usize, usize) {
    let mut left = at;
    for (i, line) in lines.iter().enumerate() {
        let n = line.chars().count();
        if left <= n {
            return (i, left);
        }
        left -= n + 1;
    }
    (lines.len() - 1, lines.last().map_or(0, |l| l.chars().count()))
}

/// Settings, as a dialog: its rows as `settings::entries` builds them, each
/// row's room in view and its buttons' (`(row, button)`), the row selected,
/// and the field of the one being typed in.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SettingsView {
    pub title: String,
    pub rect: Option<Rect>,
    /// The row of tabs under its title: the open tabs and new tabs, and the
    /// one it's on.
    pub on: Vec<TabItem>,
    pub on_at: Option<usize>,
    pub buttons: Vec<Button>,
    pub entries: Vec<Entry>,
    pub rows: Vec<Option<Rect>>,
    pub actions: Vec<((usize, usize), Rect)>,
    pub selected: usize,
    pub editing: Option<Field>,
    /// The search field at the top (`settings_search.rs`).
    pub search: Field,
}

/// One of Flow's own screens over the rest.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Dialog {
    pub kind: DialogKind,
    pub title: String,
    pub rect: Option<Rect>,
    /// Its words, a paragraph each.
    pub text: Vec<String>,
    pub field: Option<Field>,
    pub lists: Vec<List>,
    pub buttons: Vec<Button>,
    /// The item chosen while the field has the keys (the palette's, the
    /// history's): the list and its place.
    pub active: Option<(usize, usize)>,
}

/// Which of Flow's screens a dialog is, which names its nodes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum DialogKind {
    #[default]
    Find = 1,
    Palette,
    History,
    Help,
    About,
    Resources,
    Key,
    Model,
    Update,
    Shells,
    Picker,
    Naming,
    Gallery,
    Restart,
    /// An outside app's card (`crate::mcp`).
    Outside,
    /// An MCP server's question (`crate::mcp::client::elicit`).
    ServerAsk,
    /// `/model`'s list of the assistant's models.
    AiModels,
    /// A tab's name being typed in its pill in the title bar.
    TabName,
    /// The tab sidebar (`tab_sidebar`): its rows as a tree, a row's menu.
    TabSidebar,
}

/// A list in a dialog, the one chosen in it, and what choosing does.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct List {
    pub label: String,
    pub chooser: Chooser,
    pub items: Vec<Item>,
    pub chosen: Option<usize>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Item {
    pub label: String,
    pub detail: String,
    pub rect: Option<Rect>,
    /// In a tree (the tab sidebar's), how far in it's set, from 0, whether
    /// it's folded open, where it folds, and whether it's picked with
    /// others, which selects it beside the one the keys are on.
    pub tree: Option<(usize, Option<bool>, bool)>,
}

/// The lists a reader can choose in, as the keys do.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Chooser {
    #[default]
    Palette,
    History,
    Shells,
    Themes,
    /// Help's links, which Tab chooses among.
    Links,
    Packages,
    PackageThemes,
    /// `/model`'s list of the assistant's models.
    Models,
    /// The tab sidebar's rows, a tree, and a row's menu.
    Sidebar,
    SidebarMenu,
}

/// What has the keys.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Focus {
    #[default]
    Window,
    /// A pane of the tab in front, by its place.
    Pane(usize),
    /// The assistant's input.
    Panel,
    /// Settings' row selected (being typed in, its field).
    Setting(usize),
    /// Settings' search field.
    SettingsSearch,
    Field(DialogKind),
    /// A dialog's list and the item in it.
    Item(DialogKind, usize, usize),
    Dialog(DialogKind),
}

/// What a node a reader may act on is, for [`what_to_do`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Target {
    /// A button: a click at its middle.
    Press(Rect),
    Pane(usize),
    Panel,
    /// Settings' row: Focus selects it, Click selects it and makes `click`
    /// (none: only selects), and a slider's or a choice's value moves.
    Setting { row: usize, click: Option<Change> },
    /// Settings' search field: Focus and Click give it the keys.
    SettingsSearch,
    /// A pane's close, by its place in its tab: Click closes it.
    ClosePane(usize),
    Field(DialogKind),
    /// An item in a list: Focus chooses it, Click is a click on it.
    Item { chooser: Chooser, index: usize, rect: Option<Rect> },
}

/// What each node a reader may act on is, by its id.
pub type Targets = HashMap<NodeId, Target>;

/// What the app does for a reader's request.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Do {
    /// A click at the middle of this, as the pointer's would be.
    Click(Rect),
    FocusPane(usize),
    FocusPanel,
    /// Selects Settings' row, and makes a change to it.
    Select(usize),
    Change(usize, Change),
    FocusSettingsSearch,
    ClosePane(usize),
    FocusField(DialogKind),
    Choose(Chooser, usize),
    /// Folds item `usize` of a tree open (true) or shut.
    Fold(Chooser, usize, bool),
}

/// What to do for `request`, on the node it names: none where the node
/// can't do it. Focus moves only what takes the keys, never pressing a
/// button, since a reader's cursor may move focus as it goes.
pub fn what_to_do(targets: &Targets, request: &ActionRequest) -> Option<Do> {
    let target = targets.get(&request.target_node)?;
    let value = || match &request.data {
        Some(ActionData::NumericValue(v)) => Some(*v as f32),
        Some(ActionData::Value(v)) => v.trim().trim_end_matches('%').parse().ok(),
        _ => None,
    };
    Some(match (*target, request.action) {
        (Target::Press(r), Action::Click) => Do::Click(r),
        (Target::Pane(p), Action::Focus | Action::Click) => Do::FocusPane(p),
        (Target::Panel, Action::Focus | Action::Click) => Do::FocusPanel,
        (Target::Setting { row, .. }, Action::Focus) => Do::Select(row),
        (Target::Setting { row, click }, Action::Click) => click.map_or(Do::Select(row), |c| Do::Change(row, c)),
        (Target::Setting { row, .. }, Action::Increment) => Do::Change(row, Change::Step(1)),
        (Target::Setting { row, .. }, Action::Decrement) => Do::Change(row, Change::Step(-1)),
        (Target::Setting { row, .. }, Action::SetValue) => Do::Change(row, Change::Set(value()?)),
        (Target::Field(kind), Action::Focus | Action::Click) => Do::FocusField(kind),
        (Target::SettingsSearch, Action::Focus | Action::Click) => Do::FocusSettingsSearch,
        (Target::ClosePane(p), Action::Click) => Do::ClosePane(p),
        (Target::Item { chooser, index, .. }, Action::Focus) => Do::Choose(chooser, index),
        (Target::Item { chooser, index, rect }, Action::Click) => rect.map_or(Do::Choose(chooser, index), Do::Click),
        (Target::Item { chooser, index, .. }, Action::Expand) => Do::Fold(chooser, index, true),
        (Target::Item { chooser, index, .. }, Action::Collapse) => Do::Fold(chooser, index, false),
        _ => return None,
    })
}

/// The most letters a text run holds: AccessKit counts a run's words in
/// bytes, so a longer line is several runs.
const RUN: usize = 250;

/// What each part of the tree is, which with its place names a node.
#[derive(Debug, Clone, Copy)]
enum Part {
    Window = 1,
    Bar,
    BarButton,
    Tabs,
    Tab,
    TabClose,
    Pane,
    PaneRun,
    Panel,
    Conversation,
    ConversationRun,
    Input,
    InputRun,
    PanelButton,
    AskWhy,
    Scrolled,
    FrameClose,
    Reply,
    Settings,
    SettingsOn,
    SettingsTab,
    SettingsButton,
    SettingsSearch,
    SettingsSearchRun,
    Setting,
    SettingChild,
    SettingRun,
    Dialog,
    DialogText,
    Field,
    FieldRun,
    List,
    ListItem,
    DialogButton,
    Toast,
    CapsLock,
}

/// A node's id: its part, and its place in it (`a`, then `b`).
fn id(part: Part, a: usize, b: usize) -> NodeId {
    NodeId(((part as u64) << 56) | ((a as u64 & 0xFF_FFFF) << 32) | (b as u64 & 0xFFFF_FFFF))
}

/// The id of the window's own node, the tree's root.
pub const ROOT: NodeId = NodeId((Part::Window as u64) << 56);

fn bounds(r: Rect) -> accesskit::Rect {
    accesskit::Rect { x0: r.x as f64, y0: r.y as f64, x1: (r.x + r.w) as f64, y1: (r.y + r.h) as f64 }
}

/// A button's or other label's words as spoken: the symbols Flow draws
/// beside its words (✦, ●, ⏎, ⌘, ↑, ⚙) left out, whatever the language's
/// own punctuation, and its dots between parts read as pauses.
pub fn spoken(label: &str) -> String {
    // Arrows, technical symbols (⌘ ⌃ ⏎), box drawing and blocks, shapes
    // (● ■ ▸), other symbols (⚙ ☰) and dingbats (✦ ✕).
    let drawn = |c: char| matches!(c as u32, 0x2190..=0x21FF | 0x2300..=0x23FF | 0x2500..=0x27BF | 0x2B00..=0x2BFF);
    let kept: String = label.chars().filter(|c| !drawn(*c)).map(|c| if c == '·' { ',' } else { c }).collect();
    let words: Vec<&str> = kept.split_whitespace().collect();
    words.join(" ").replace(" ,", ",").trim_matches(|c: char| c == ',' || c.is_whitespace()).to_string()
}

/// A reply's markdown as words to say: its fences, backticks, emphasis and
/// headings' marks left out.
pub fn plain_markdown(text: &str) -> String {
    let mut out = Vec::new();
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            continue;
        }
        let line = line.trim_start_matches('#').trim_start();
        out.push(line.replace("**", "").replace('`', ""));
    }
    out.join("\n").trim().to_string()
}

/// Where in a field's or a row's runs a letter is: the run it falls in and
/// its place there, for `chars` letters cut into runs of [`RUN`].
fn run_place(chars: usize, at: usize) -> (usize, usize) {
    let runs = chars.div_ceil(RUN).max(1);
    let run = (at / RUN).min(runs - 1);
    (run, at - run * RUN)
}

/// Builds the nodes and what they do.
struct Build {
    nodes: Vec<(NodeId, Node)>,
    targets: Targets,
}

impl Build {
    fn push(&mut self, id: NodeId, node: Node) -> NodeId {
        self.nodes.push((id, node));
        id
    }

    fn target(&mut self, id: NodeId, target: Target) {
        self.targets.insert(id, target);
    }

    /// A button, which a reader can press.
    fn button(&mut self, id: NodeId, b: &Button) -> NodeId {
        let mut node = Node::new(Role::Button);
        node.set_label(b.label.clone());
        if let Some(on) = b.on {
            node.set_toggled(Toggled::from(on));
        }
        if let Some(r) = b.rect {
            node.set_bounds(bounds(r));
            node.add_action(Action::Click);
            self.target(id, Target::Press(r));
        }
        self.push(id, node)
    }

    /// Words to read, alone.
    fn label(&mut self, id: NodeId, words: &str) -> NodeId {
        let mut node = Node::new(Role::Label);
        node.set_value(words.to_string());
        self.push(id, node)
    }

    /// A line's text as runs of at most [`RUN`] letters, the last ending
    /// with a line break where `newline`. `letters` gives each letter's
    /// bytes, column and cells, which place it in `rect` by `cell_w`; with
    /// none, each `char` is a letter and the runs take the line's `rect`.
    /// Returns the runs' ids.
    fn runs(&mut self, run_id: impl Fn(usize) -> NodeId, text: &str, letters: Option<&[Letter]>, newline: bool, rect: Option<Rect>, cell_w: f32) -> Vec<NodeId> {
        let own: Vec<Letter>;
        let letters = match letters {
            Some(l) => l,
            None => {
                own = text.chars().map(|c| Letter { bytes: c.len_utf8() as u8, ..Letter::default() }).collect();
                &own
            }
        };
        let mut ids = Vec::new();
        let mut byte = 0;
        let pieces = letters.len().div_ceil(RUN).max(1);
        for piece in 0..pieces {
            let these = &letters[(piece * RUN).min(letters.len())..((piece + 1) * RUN).min(letters.len())];
            let len: usize = these.iter().map(|l| l.bytes as usize).sum();
            let mut value = text.get(byte..byte + len).unwrap_or("").to_string();
            byte += len;
            let mut lengths: Vec<u8> = these.iter().map(|l| l.bytes).collect();
            let last = piece + 1 == pieces;
            if last && newline {
                value.push('\n');
                lengths.push(1);
            }
            let mut node = Node::new(Role::TextRun);
            node.set_word_starts(word_starts(&value, &lengths));
            if let Some(r) = rect {
                if letters.iter().any(|l| l.cells > 0) {
                    // Each letter where its cells are, from the run's first.
                    let first = these.first().map_or(0, |l| l.col) as f32;
                    let end = these.last().map_or(first, |l| (l.col + l.cells as u16) as f32);
                    let mut positions: Vec<f32> = these.iter().map(|l| (l.col as f32 - first) * cell_w).collect();
                    let mut widths: Vec<f32> = these.iter().map(|l| l.cells as f32 * cell_w).collect();
                    if last && newline {
                        positions.push((end - first) * cell_w);
                        widths.push(0.0);
                    }
                    let x = r.x + first * cell_w;
                    node.set_bounds(bounds(Rect::new(x, r.y, ((end - first) * cell_w).max(cell_w), r.h)));
                    node.set_character_positions(positions);
                    node.set_character_widths(widths);
                } else {
                    node.set_bounds(bounds(r));
                }
            }
            node.set_character_lengths(lengths);
            node.set_value(value);
            ids.push(self.push(run_id(piece), node));
        }
        ids
    }

    /// A field typed in: a node of its role holding a run for each line,
    /// the caret its selection.
    fn field(&mut self, id: NodeId, run_id: impl Fn(usize, usize) -> NodeId, f: &Field, target: Option<Target>) -> NodeId {
        let role = if f.secret {
            Role::PasswordInput
        } else if f.search {
            Role::SearchInput
        } else if f.multiline {
            Role::MultilineTextInput
        } else {
            Role::TextInput
        };
        let mut node = Node::new(role);
        node.set_label(f.label.clone());
        if !f.placeholder.is_empty() {
            node.set_placeholder(f.placeholder.clone());
        }
        let lines: Vec<String> = match f.lines.is_empty() {
            true => vec![String::new()],
            false if f.secret => f.lines.iter().map(|l| "•".repeat(l.chars().count())).collect(),
            false => f.lines.clone(),
        };
        let mut children = Vec::new();
        let (mut caret, mut anchor) = (None, None);
        let (cl, cc) = (f.caret.0.min(lines.len() - 1), f.caret.1);
        let (al, ac) = f.anchor.map_or((usize::MAX, 0), |(l, c)| (l.min(lines.len() - 1), c));
        for (i, line) in lines.iter().enumerate() {
            let newline = i + 1 < lines.len();
            let runs = self.runs(|p| run_id(i, p), line, None, newline, None, 0.0);
            let n = line.chars().count();
            if i == cl {
                let (run, at) = run_place(n, cc.min(n));
                caret = Some(TextPosition { node: runs[run], character_index: at });
            }
            if i == al {
                let (run, at) = run_place(n, ac.min(n));
                anchor = Some(TextPosition { node: runs[run], character_index: at });
            }
            children.extend(runs);
        }
        node.set_children(children);
        if let Some(at) = caret {
            node.set_text_selection(TextSelection { anchor: anchor.unwrap_or(at), focus: at });
        }
        if let Some(r) = f.rect {
            node.set_bounds(bounds(r));
        }
        node.add_action(Action::Focus);
        if let Some(target) = target {
            self.target(id, target);
        }
        self.push(id, node)
    }
}

/// Where each word starts among a run's letters (`lengths` their bytes): a
/// letter that isn't a blank after one that is, and the run's first.
fn word_starts(text: &str, lengths: &[u8]) -> Vec<u8> {
    let mut starts = Vec::new();
    let mut byte = 0;
    let mut blank_before = true;
    for (i, &len) in lengths.iter().enumerate() {
        let blank = text.get(byte..).and_then(|s| s.chars().next()).is_none_or(char::is_whitespace);
        if !blank && blank_before && i > 0 {
            starts.push(i.min(255) as u8);
        }
        blank_before = blank;
        byte += len as usize;
    }
    starts
}

/// The tree for `v`, whole (each update gives every node, which the
/// adapters compare with the last), and what its nodes do.
pub fn tree(v: &AccessView) -> (TreeUpdate, Targets) {
    let mut b = Build { nodes: Vec::new(), targets: HashMap::new() };
    let mut children = Vec::new();

    // The title bar: Flow's name, the tabs, then its buttons.
    let mut bar = Vec::new();
    for (i, button) in v.lead.iter().enumerate() {
        bar.push(b.button(id(Part::BarButton, 0, i), button));
    }
    let mut tabs = Vec::new();
    for (i, tab) in v.tabs.iter().enumerate() {
        tabs.push(tab_node(&mut b, id(Part::Tab, 0, i), id(Part::TabClose, 0, i), tab, i == v.active));
    }
    let mut tab_list = Node::new(Role::TabList);
    tab_list.set_label(t("access.tabs"));
    tab_list.set_children(tabs);
    bar.push(b.push(id(Part::Tabs, 0, 0), tab_list));
    for (i, button) in v.bar.iter().enumerate() {
        bar.push(b.button(id(Part::BarButton, 1, i), button));
    }
    let mut title_bar = Node::new(Role::Toolbar);
    title_bar.set_label(t("access.title_bar"));
    title_bar.set_children(bar);
    children.push(b.push(id(Part::Bar, 0, 0), title_bar));

    // The terminal: each pane's rows in view, the cursor the caret.
    for (p, screen) in v.panes.iter().enumerate() {
        children.push(pane_node(&mut b, p, screen, v.panes.len()));
    }
    for (i, button) in v.scrolled.iter().enumerate() {
        children.push(b.button(id(Part::Scrolled, 0, i), button));
    }
    for (i, (pane, button)) in v.closes.iter().enumerate() {
        let close = id(Part::FrameClose, 0, i);
        let mut node = Node::new(Role::Button);
        node.set_label(button.label.clone());
        if let Some(r) = button.rect {
            node.set_bounds(bounds(r));
        }
        node.add_action(Action::Click);
        b.target(close, Target::ClosePane(*pane));
        children.push(b.push(close, node));
    }
    if let Some(button) = &v.ask_why {
        children.push(b.button(id(Part::AskWhy, 0, 0), button));
    }
    if let Some(panel) = &v.panel {
        children.push(panel_node(&mut b, panel));
    }
    if let Some(reply) = v.reply.as_ref().filter(|r| !r.is_empty()) {
        let mut node = Node::new(Role::Status);
        node.set_label(reply.clone());
        node.set_live(Live::Polite);
        children.push(b.push(id(Part::Reply, 0, 0), node));
    }
    if let Some(s) = &v.settings {
        children.push(settings_node(&mut b, s));
    }
    for d in &v.dialogs {
        children.push(dialog_node(&mut b, d));
    }
    if let Some(toast) = v.toast.as_ref().filter(|t| !t.is_empty()) {
        let mut node = Node::new(Role::Status);
        node.set_label(spoken(toast));
        node.set_live(Live::Polite);
        children.push(b.push(id(Part::Toast, 0, 0), node));
    }
    if let Some(caps) = &v.caps_lock {
        // Assertive, unlike a toast: it warns of what's being typed.
        let mut node = Node::new(Role::Alert);
        node.set_label(spoken(caps));
        node.set_live(Live::Assertive);
        children.push(b.push(id(Part::CapsLock, 0, 0), node));
    }

    let mut window = Node::new(Role::Window);
    window.set_label(v.title.clone());
    window.set_bounds(bounds(Rect::new(0.0, 0.0, v.size.0, v.size.1)));
    window.set_children(children);
    b.push(ROOT, window);

    let focus = focus_id(v);
    let focus = if b.nodes.iter().any(|(n, _)| *n == focus) { focus } else { ROOT };
    let mut info = TreeInfo::new(ROOT);
    info.toolkit_name = Some(crate::APP_NAME.to_string());
    info.toolkit_version = Some(crate::version::VERSION.to_string());
    (TreeUpdate { nodes: b.nodes, tree: Some(info), tree_id: TreeId::ROOT, focus }, b.targets)
}

/// The node with the keys, as `v.focus` says.
fn focus_id(v: &AccessView) -> NodeId {
    match v.focus {
        Focus::Window => ROOT,
        Focus::Pane(p) => id(Part::Pane, p, 0),
        Focus::Panel => id(Part::Input, 0, 0),
        Focus::Setting(i) => id(Part::Setting, i, 0),
        Focus::SettingsSearch => id(Part::SettingsSearch, 0, 0),
        Focus::Field(kind) => id(Part::Field, kind as usize, 0),
        Focus::Item(kind, list, i) => id(Part::ListItem, kind as usize, (list << 20) | i),
        Focus::Dialog(kind) => id(Part::Dialog, kind as usize, 0),
    }
}

fn tab_node(b: &mut Build, tab_id: NodeId, close_id: NodeId, tab: &TabItem, selected: bool) -> NodeId {
    let mut node = Node::new(Role::Tab);
    node.set_label(spoken(&tab.label));
    node.set_selected(selected);
    if let Some(r) = tab.rect {
        node.set_bounds(bounds(r));
        node.add_action(Action::Click);
        b.target(tab_id, Target::Press(r));
    }
    if let Some(r) = tab.close {
        let close = b.button(close_id, &Button::new(t("menu.close_tab"), Some(r)));
        node.set_children(vec![close]);
    }
    b.push(tab_id, node)
}

fn pane_node(b: &mut Build, p: usize, s: &Screen, panes: usize) -> NodeId {
    let pane = id(Part::Pane, p, 0);
    let mut node = Node::new(Role::Terminal);
    let label = if panes > 1 { tf("access.pane", &[("tab", &s.label), ("n", &(p + 1)), ("count", &panes)]) } else { s.label.clone() };
    node.set_label(label);
    node.set_bounds(bounds(s.rect));
    node.add_action(Action::Focus);
    b.target(pane, Target::Pane(p));
    let mut children = Vec::new();
    let mut caret = None;
    for (r, row) in s.rows.iter().enumerate() {
        let newline = !row.wraps && r + 1 < s.rows.len();
        let rect = Rect::new(s.rect.x, s.rect.y + r as f32 * s.cell.1, s.rect.w, s.cell.1);
        let runs = b.runs(|piece| id(Part::PaneRun, p, (r << 8) | piece), &row.text, Some(&row.letters), newline, Some(rect), s.cell.0);
        if let Some((_, cc)) = s.cursor.filter(|(cr, _)| *cr == r) {
            let (run, at) = run_place(row.letters.len(), cc.min(row.letters.len()));
            caret = Some(TextPosition { node: runs[run], character_index: at });
        }
        children.extend(runs);
    }
    node.set_children(children);
    if let Some(at) = caret {
        node.set_text_selection(TextSelection { anchor: at, focus: at });
    }
    b.push(pane, node)
}

fn panel_node(b: &mut Build, p: &Panel) -> NodeId {
    let panel = id(Part::Panel, 0, 0);
    let mut node = Node::new(Role::Region);
    node.set_label(p.label.clone());
    node.set_bounds(bounds(p.rect));
    let mut children = Vec::new();
    // The conversation as laid out, a run for each row, those in view where
    // they're drawn.
    let mut talk = Node::new(Role::Document);
    talk.set_label(t("access.conversation"));
    talk.set_bounds(bounds(p.area));
    let mut runs = Vec::new();
    for (r, row) in p.rows.iter().enumerate() {
        let rect = (r >= p.start && r < p.start + p.in_view).then(|| Rect::new(p.area.x, p.top + (r - p.start) as f32 * p.row_h, p.area.w, p.row_h));
        runs.extend(b.runs(|piece| id(Part::ConversationRun, 0, (r << 8) | piece), row, None, r + 1 < p.rows.len(), rect, 0.0));
    }
    if !runs.is_empty() {
        talk.set_children(runs);
        children.push(b.push(id(Part::Conversation, 0, 0), talk));
    }
    for (i, button) in p.buttons.iter().enumerate() {
        children.push(b.button(id(Part::PanelButton, 0, i), button));
    }
    children.push(b.field(id(Part::Input, 0, 0), |line, piece| id(Part::InputRun, line, piece), &p.input, Some(Target::Panel)));
    node.set_children(children);
    b.push(panel, node)
}

fn settings_node(b: &mut Build, s: &SettingsView) -> NodeId {
    let mut node = Node::new(Role::Dialog);
    node.set_label(s.title.clone());
    node.set_modal();
    if let Some(r) = s.rect {
        node.set_bounds(bounds(r));
    }
    let mut children = Vec::new();
    if !s.on.is_empty() {
        let tabs: Vec<NodeId> = s.on.iter().enumerate().map(|(i, tab)| tab_node(b, id(Part::SettingsTab, 0, i), id(Part::SettingsTab, 1, i), tab, Some(i) == s.on_at)).collect();
        let mut list = Node::new(Role::TabList);
        list.set_label(t("access.settings_on"));
        list.set_children(tabs);
        children.push(b.push(id(Part::SettingsOn, 0, 0), list));
    }
    for (i, button) in s.buttons.iter().enumerate() {
        children.push(b.button(id(Part::SettingsButton, 0, i), button));
    }
    children.push(b.field(id(Part::SettingsSearch, 0, 0), |line, piece| id(Part::SettingsSearchRun, line, piece), &s.search, Some(Target::SettingsSearch)));
    for (i, entry) in s.entries.iter().enumerate() {
        let editing = s.editing.as_ref().filter(|_| i == s.selected);
        children.push(setting_node(b, s, i, entry, editing));
    }
    node.set_children(children);
    b.push(id(Part::Settings, 0, 0), node)
}

/// A row of Settings, by its control.
fn setting_node(b: &mut Build, s: &SettingsView, i: usize, entry: &Entry, editing: Option<&Field>) -> NodeId {
    let row_id = id(Part::Setting, i, 0);
    let row = match entry {
        Entry::Header(r) | Entry::Setting(r) => r,
    };
    let rect = s.rows.get(i).copied().flatten();
    if let Some(field) = editing {
        let field = Field { label: row.label.to_string(), rect: field.rect.or(rect), ..field.clone() };
        return b.field(row_id, |line, piece| id(Part::SettingRun, i, (line << 8) | piece), &field, Some(Target::Setting { row: i, click: None }));
    }
    let select = |click| Target::Setting { row: i, click };
    let (mut node, target) = match &row.control {
        Control::Section { open, count, scope } => {
            let mut node = Node::new(Role::Heading);
            node.set_level(2);
            node.set_label(if scope.is_empty() { row.label.to_string() } else { format!("{}, {scope}", row.label) });
            node.set_expanded(*open);
            if !*open {
                node.set_description(tf("settings.section.count", &[("n", count)]));
            }
            (node, select(Some(Change::Toggle)))
        }
        Control::Toggle(on) => {
            let mut node = Node::new(Role::CheckBox);
            node.set_toggled(Toggled::from(*on));
            (node, select(Some(Change::Toggle)))
        }
        Control::Slider { value, min, max, step, text, note } => {
            let mut node = Node::new(Role::Slider);
            node.set_numeric_value(*value as f64);
            node.set_min_numeric_value(*min as f64);
            node.set_max_numeric_value(*max as f64);
            node.set_numeric_value_step(*step as f64);
            node.set_value(match note {
                Some(note) => format!("{text}, {note}"),
                None => text.clone(),
            });
            node.add_action(Action::Increment);
            node.add_action(Action::Decrement);
            node.add_action(Action::SetValue);
            (node, select(None))
        }
        Control::Choice { options, index } => {
            let mut node = Node::new(Role::ComboBox);
            node.set_value(options.get(*index).cloned().unwrap_or_default());
            node.add_action(Action::Increment);
            node.add_action(Action::Decrement);
            (node, select(Some(Change::Step(1))))
        }
        Control::Colors { options, index, .. } => {
            let mut node = Node::new(Role::ComboBox);
            node.set_value(options.get(*index).map(|(name, _)| name.clone()).unwrap_or_default());
            node.add_action(Action::Increment);
            node.add_action(Action::Decrement);
            // Enter or a click opens the color picker.
            (node, select(Some(Change::Action(0))))
        }
        Control::Actions(labels) => {
            let mut node = Node::new(Role::Group);
            node.set_children(setting_buttons(b, s, i, labels.iter().map(|l| l.to_string())));
            (node, select(None))
        }
        Control::Readout { text, action } => {
            let mut node = Node::new(Role::Group);
            let words = b.label(id(Part::SettingChild, i, 0), text);
            let mut parts = vec![words];
            parts.extend(setting_buttons(b, s, i, [action.to_string()].into_iter()));
            node.set_children(parts);
            (node, select(None))
        }
        Control::Item { text, actions } => {
            let mut node = Node::new(Role::Group);
            let words = b.label(id(Part::SettingChild, i, 0), text);
            let mut parts = vec![words];
            parts.extend(setting_buttons(b, s, i, actions.iter().map(|a| a.to_string())));
            node.set_children(parts);
            (node, select(None))
        }
        Control::Text { value, multiline, empty } => {
            let mut node = Node::new(if *multiline { Role::MultilineTextInput } else { Role::TextInput });
            node.set_value(value.clone());
            node.set_placeholder(empty.to_string());
            // Enter or a click starts typing in it.
            (node, select(Some(Change::Action(0))))
        }
    };
    if !matches!(row.control, Control::Section { .. }) {
        node.set_label(row.label.to_string());
        if !row.hint.is_empty() {
            node.set_description(row.hint.clone());
        }
    }
    if let Some(r) = rect {
        node.set_bounds(bounds(r));
    }
    node.add_action(Action::Focus);
    node.add_action(Action::Click);
    b.target(row_id, target);
    b.push(row_id, node)
}

/// A Settings row's buttons, which make its actions.
fn setting_buttons(b: &mut Build, s: &SettingsView, i: usize, labels: impl Iterator<Item = String>) -> Vec<NodeId> {
    labels
        .enumerate()
        .map(|(k, label)| {
            let button_id = id(Part::SettingChild, i, k + 1);
            let rect = s.actions.iter().find(|(at, _)| *at == (i, k)).map(|(_, r)| *r);
            let mut node = Node::new(Role::Button);
            node.set_label(spoken(&label));
            if let Some(r) = rect {
                node.set_bounds(bounds(r));
            }
            node.add_action(Action::Click);
            b.target(button_id, Target::Setting { row: i, click: Some(Change::Action(k)) });
            b.push(button_id, node)
        })
        .collect()
}

fn dialog_node(b: &mut Build, d: &Dialog) -> NodeId {
    let k = d.kind as usize;
    let dialog = id(Part::Dialog, k, 0);
    let mut node = Node::new(Role::Dialog);
    node.set_label(d.title.clone());
    node.set_modal();
    if let Some(r) = d.rect {
        node.set_bounds(bounds(r));
    }
    let mut children = Vec::new();
    for (i, words) in d.text.iter().enumerate().filter(|(_, w)| !w.is_empty()) {
        children.push(b.label(id(Part::DialogText, k, i), words));
    }
    if let Some(f) = &d.field {
        let field = b.field(id(Part::Field, k, 0), |line, piece| id(Part::FieldRun, k, (line << 8) | piece), f, Some(Target::Field(d.kind)));
        if let Some((list, i)) = d.active {
            if d.lists.get(list).is_some_and(|l| i < l.items.len()) {
                if let Some((_, node)) = b.nodes.iter_mut().rev().find(|(n, _)| *n == field) {
                    node.set_active_descendant(id(Part::ListItem, k, (list << 20) | i));
                }
            }
        }
        children.push(field);
    }
    for (l, list) in d.lists.iter().enumerate() {
        let links = list.chooser == Chooser::Links;
        // What a list and its items are to a reader: a tree's, a menu's,
        // links, or a list box's options.
        let (role, item_role) = match list.chooser {
            Chooser::Links => (Role::List, Role::Link),
            Chooser::Sidebar => (Role::Tree, Role::TreeItem),
            Chooser::SidebarMenu => (Role::Menu, Role::MenuItem),
            _ => (Role::ListBox, Role::ListBoxOption),
        };
        let mut items = Vec::new();
        for (i, item) in list.items.iter().enumerate() {
            let item_id = id(Part::ListItem, k, (l << 20) | i);
            let mut node = Node::new(item_role);
            node.set_label(spoken(&item.label));
            if !item.detail.is_empty() {
                node.set_description(item.detail.clone());
            }
            if !links && item_role != Role::MenuItem {
                node.set_selected(Some(i) == list.chosen || item.tree.is_some_and(|(_, _, picked)| picked));
            }
            if let Some((level, expanded, _)) = item.tree {
                node.set_level(level + 1);
                if let Some(open) = expanded {
                    node.set_expanded(open);
                    node.add_action(if open { Action::Collapse } else { Action::Expand });
                }
            }
            if let Some(r) = item.rect {
                node.set_bounds(bounds(r));
            }
            node.add_action(Action::Focus);
            node.add_action(Action::Click);
            b.target(item_id, Target::Item { chooser: list.chooser, index: i, rect: item.rect });
            items.push(b.push(item_id, node));
        }
        let mut node = Node::new(role);
        node.set_label(list.label.clone());
        // The tab sidebar's tabs are picked several at once.
        if list.chooser == Chooser::Sidebar {
            node.set_multiselectable();
        }
        node.set_children(items);
        children.push(b.push(id(Part::List, k, l), node));
    }
    for (i, button) in d.buttons.iter().enumerate() {
        children.push(b.button(id(Part::DialogButton, k, i), button));
    }
    node.set_children(children);
    b.push(dialog, node)
}

/// The words a dialog's button or a title bar's button has where it draws
/// a symbol alone, or words with keys beside them.
pub fn words(key: &str) -> String {
    spoken(t(key))
}

/// Each Settings row's room, by the row, and each of its buttons', by the
/// row and the button.
pub type Rooms = (Vec<Option<Rect>>, Vec<((usize, usize), Rect)>);

/// Settings' rows' rooms in `hits` as `ui::draw_settings` left them (a
/// header's is its toggle's, a row's its own), by the row, and its buttons'.
pub fn settings_rooms(entries: &[Entry], hits: &[(Rect, crate::ui::Hit)]) -> Rooms {
    use crate::ui::Hit;
    let rows = settings::rows(entries)
        .iter()
        .enumerate()
        .map(|(i, _)| {
            let header = matches!(entries.get(i), Some(Entry::Header(_)));
            hits.iter().find(|(_, h)| if header { *h == Hit::SettingToggle(i) } else { *h == Hit::SettingRow(i) }).map(|(r, _)| *r)
        })
        .collect();
    let actions = hits
        .iter()
        .filter_map(|(r, h)| match h {
            Hit::SettingAction(i, k) => Some(((*i, *k), *r)),
            _ => None,
        })
        .collect();
    (rows, actions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{Row as SettingRow, Section, SettingId};
    use accesskit_consumer::Tree;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Option<Rect> {
        Some(Rect::new(x, y, w, h))
    }

    /// A row of plain letters, one cell each.
    fn row(text: &str, wraps: bool) -> Row {
        Row { text: text.to_string(), letters: text.chars().enumerate().map(|(i, c)| Letter { bytes: c.len_utf8() as u8, col: i as u16, cells: 1 }).collect(), wraps }
    }

    fn setting(label: &'static str, control: Control) -> Entry {
        Entry::Setting(SettingRow { id: SettingId::Opacity, label, hint: format!("{label}'s hint"), control })
    }

    /// Two tabs, a terminal of three rows (the first wrapping into the
    /// second) with the cursor after the prompt, and a toast.
    fn view() -> AccessView {
        AccessView {
            size: (800.0, 600.0),
            title: "zsh — Flow".into(),
            lead: vec![Button::new("Flow", rect(10.0, 0.0, 60.0, 40.0))],
            tabs: vec![
                TabItem { label: "zsh".into(), rect: rect(100.0, 0.0, 80.0, 40.0), close: rect(160.0, 10.0, 16.0, 16.0) },
                TabItem { label: "bash · Green screen".into(), rect: rect(190.0, 0.0, 80.0, 40.0), close: None },
            ],
            active: 0,
            bar: vec![Button::new("+", rect(280.0, 0.0, 20.0, 40.0)), Button::toggle("✦ Ask", rect(500.0, 0.0, 60.0, 40.0), false)],
            panes: vec![Screen {
                label: "zsh".into(),
                rect: Rect::new(14.0, 42.0, 772.0, 54.0),
                cell: (8.0, 18.0),
                rows: vec![row("a long line that w", true), row("raps", false), row("~ % ", false)],
                cursor: Some((2, 4)),
            }],
            toast: Some("Copied".into()),
            focus: Focus::Pane(0),
            ..AccessView::default()
        }
    }

    fn read(update: TreeUpdate) -> Tree {
        Tree::new(update, true)
    }

    fn node<'a>(tree: &'a Tree, id: NodeId) -> accesskit_consumer::NodeRef<'a> {
        tree.state().node_by_tree_local_id(id, TreeId::ROOT).unwrap_or_else(|| panic!("no node {id:?}"))
    }

    #[test]
    fn the_terminal_reads_as_its_rows_with_the_cursor_as_the_caret() {
        let (update, targets) = tree(&view());
        let got = read(update);
        let pane = node(&got, id(Part::Pane, 0, 0));
        assert_eq!(pane.role(), Role::Terminal);
        assert_eq!(pane.label().as_deref(), Some("zsh"));
        // The wrapped row runs on into the next without a break.
        assert_eq!(pane.document_range().text(), "a long line that wraps\n~ % ");
        let caret = pane.text_selection_focus().unwrap();
        assert_eq!(caret.inner_node().id(), node(&got, id(Part::PaneRun, 0, 2 << 8)).id());
        assert_eq!(caret.to_global_usv_index(), "a long line that wraps\n~ % ".chars().count());
        assert_eq!(got.state().focus().map(|f| f.id()), Some(pane.id()));
        // Each row is where its cells are drawn.
        let second = node(&got, id(Part::PaneRun, 0, 1 << 8)).bounding_box().unwrap();
        assert_eq!((second.x0, second.y0, second.x1, second.y1), (14.0, 60.0, 14.0 + 4.0 * 8.0, 78.0));
        assert_eq!(targets.get(&id(Part::Pane, 0, 0)), Some(&Target::Pane(0)));
    }

    #[test]
    fn the_tabs_are_a_tab_list_with_the_one_in_front_selected() {
        let (update, targets) = tree(&view());
        let got = read(update);
        let tabs = node(&got, id(Part::Tabs, 0, 0));
        assert_eq!(tabs.role(), Role::TabList);
        let first = node(&got, id(Part::Tab, 0, 0));
        let second = node(&got, id(Part::Tab, 0, 1));
        assert_eq!((first.is_selected(), second.is_selected()), (Some(true), Some(false)));
        // The middle dot between the shell and its theme reads as a pause.
        assert_eq!(second.label().as_deref(), Some("bash, Green screen"));
        assert_eq!(targets.get(&id(Part::Tab, 0, 1)), Some(&Target::Press(Rect::new(190.0, 0.0, 80.0, 40.0))));
        assert_eq!(node(&got, id(Part::TabClose, 0, 0)).label().as_deref(), Some(t("menu.close_tab")));
        // The symbol beside a button's words isn't read.
        let ask = node(&got, id(Part::BarButton, 1, 1));
        assert_eq!((ask.label().as_deref(), ask.toggled()), (Some("Ask"), Some(Toggled::False)));
    }

    #[test]
    fn a_toast_is_a_polite_live_region() {
        let (update, _) = tree(&view());
        let got = read(update);
        let toast = node(&got, id(Part::Toast, 0, 0));
        assert_eq!((toast.role(), toast.live(), toast.label().as_deref()), (Role::Status, Live::Polite, Some("Copied")));
        // No toast, no region.
        let (update, _) = tree_of(AccessView { toast: None, ..view() });
        assert!(read(update).state().node_by_tree_local_id(id(Part::Toast, 0, 0), TreeId::ROOT).is_none());
    }

    #[test]
    fn caps_locks_notice_is_an_assertive_alert_said_without_its_symbol() {
        let (update, _) = tree_of(AccessView { caps_lock: Some("⇪ Caps Lock is on".into()), ..view() });
        let got = read(update);
        let caps = node(&got, id(Part::CapsLock, 0, 0));
        assert_eq!((caps.role(), caps.live(), caps.label().as_deref()), (Role::Alert, Live::Assertive, Some("Caps Lock is on")));
        let (update, _) = tree(&view());
        assert!(read(update).state().node_by_tree_local_id(id(Part::CapsLock, 0, 0), TreeId::ROOT).is_none());
    }

    fn tree_of(v: AccessView) -> (TreeUpdate, Targets) {
        tree(&v)
    }

    #[test]
    fn a_pane_scrolled_back_has_a_button_back_to_the_bottom() {
        let pill = Rect::new(600.0, 42.0, 150.0, 18.0);
        let (update, targets) = tree_of(AccessView { scrolled: vec![Button::new("↓ 21 lines below, back to the bottom", Some(pill))], ..view() });
        let got = read(update);
        let button = node(&got, id(Part::Scrolled, 0, 0));
        assert_eq!((button.role(), button.label().as_deref()), (Role::Button, Some("21 lines below, back to the bottom")));
        assert_eq!(targets.get(&id(Part::Scrolled, 0, 0)), Some(&Target::Press(pill)));
        // At the bottom, none.
        let (update, _) = tree(&view());
        assert!(read(update).state().node_by_tree_local_id(id(Part::Scrolled, 0, 0), TreeId::ROOT).is_none());
    }

    #[test]
    fn each_settings_control_is_its_role_and_the_selected_row_has_the_keys() {
        let entries = vec![
            Entry::Header(SettingRow { id: SettingId::Section(Section::Glass), label: "Glass", hint: String::new(), control: Control::Section { open: true, count: 3, scope: "this tab" } }),
            setting("Opacity", Control::Slider { value: 0.55, min: 0.2, max: 1.0, step: 0.05, text: "55%".into(), note: None }),
            setting("Glow", Control::Toggle(true)),
            setting("Style", Control::Choice { options: vec!["Clouds".to_string(), "Rain".to_string()].into(), index: 1 }),
            setting("Text color", Control::Colors { options: vec![("Ice".to_string(), [1, 2, 3])], index: 0, none: false }),
            setting("API key", Control::Actions(vec!["Change", "Remove"])),
            setting("Prompt", Control::Text { value: "$ ".into(), multiline: false, empty: "none" }),
        ];
        let settings = SettingsView {
            title: "Settings".into(),
            entries,
            rows: vec![rect(0.0, 100.0, 400.0, 30.0), rect(0.0, 130.0, 400.0, 30.0), None, None, None, None, None],
            actions: vec![((5, 1), Rect::new(300.0, 300.0, 60.0, 20.0))],
            selected: 1,
            ..SettingsView::default()
        };
        let (update, targets) = tree(&AccessView { settings: Some(settings.clone()), focus: Focus::Setting(1), ..view() });
        let got = read(update);
        let header = node(&got, id(Part::Setting, 0, 0));
        assert_eq!((header.role(), header.label().as_deref(), header.data().is_expanded()), (Role::Heading, Some("Glass, this tab"), Some(true)));
        let slider = node(&got, id(Part::Setting, 1, 0));
        assert_eq!(slider.role(), Role::Slider);
        assert_eq!((slider.numeric_value(), slider.data().min_numeric_value(), slider.data().max_numeric_value(), slider.numeric_value_step()), (Some(0.550000011920929), Some(0.20000000298023224), Some(1.0), Some(0.05000000074505806)));
        assert_eq!(slider.value().as_deref(), Some("55%"));
        assert_eq!(got.state().focus().map(|f| f.id()), Some(slider.id()));
        let toggle = node(&got, id(Part::Setting, 2, 0));
        assert_eq!((toggle.role(), toggle.toggled()), (Role::CheckBox, Some(Toggled::True)));
        let choice = node(&got, id(Part::Setting, 3, 0));
        assert_eq!((choice.role(), choice.value().as_deref()), (Role::ComboBox, Some("Rain")));
        let colors = node(&got, id(Part::Setting, 4, 0));
        assert_eq!((colors.role(), colors.value().as_deref()), (Role::ComboBox, Some("Ice")));
        let remove = node(&got, id(Part::SettingChild, 5, 2));
        assert_eq!((remove.role(), remove.label().as_deref()), (Role::Button, Some("Remove")));
        let text = node(&got, id(Part::Setting, 6, 0));
        assert_eq!((text.role(), text.label().as_deref()), (Role::TextInput, Some("Prompt")));
        // What each does.
        assert_eq!(targets[&id(Part::Setting, 0, 0)], Target::Setting { row: 0, click: Some(Change::Toggle) });
        assert_eq!(targets[&id(Part::SettingChild, 5, 2)], Target::Setting { row: 5, click: Some(Change::Action(1)) });
        assert_eq!(targets[&id(Part::Setting, 6, 0)], Target::Setting { row: 6, click: Some(Change::Action(0)) });
        // Typed in, the row is a field with its caret.
        let typing = SettingsView { selected: 6, editing: Some(Field::line("", "", "$> ", None)), ..settings };
        let (update, _) = tree(&AccessView { settings: Some(typing), focus: Focus::Setting(6), ..view() });
        let got = read(update);
        let field = node(&got, id(Part::Setting, 6, 0));
        assert_eq!((field.role(), field.document_range().text()), (Role::TextInput, "$> ".to_string()));
        assert_eq!(field.text_selection_focus().unwrap().to_global_usv_index(), 3);
    }

    #[test]
    fn settings_search_field_is_a_search_box_that_takes_the_keys_when_asked() {
        let search = Field { search: true, ..Field::line("Search settings", "Search  ⌘F", "glow", rect(400.0, 60.0, 200.0, 24.0)) };
        let settings = SettingsView { title: "Settings".into(), search, ..SettingsView::default() };
        let (update, targets) = tree(&AccessView { settings: Some(settings), focus: Focus::SettingsSearch, ..view() });
        assert_eq!(update.focus, id(Part::SettingsSearch, 0, 0));
        let got = read(update);
        let field = node(&got, id(Part::SettingsSearch, 0, 0));
        assert_eq!((field.role(), field.label().as_deref(), field.document_range().text()), (Role::SearchInput, Some("Search settings"), "glow".to_string()));
        let ask = |action| what_to_do(&targets, &ActionRequest { action, target_tree: TreeId::ROOT, target_node: id(Part::SettingsSearch, 0, 0), data: None });
        assert_eq!(ask(Action::Focus), Some(Do::FocusSettingsSearch));
        assert_eq!(ask(Action::Click), Some(Do::FocusSettingsSearch));
    }

    #[test]
    fn every_pane_with_a_close_has_a_button_that_closes_it_shown_or_not() {
        let closes = vec![(0, Button::new("Close Pane", None)), (1, Button::new("Close Pane", rect(700.0, 40.0, 18.0, 18.0)))];
        let (update, targets) = tree(&AccessView { closes, ..view() });
        let got = read(update);
        for i in 0..2 {
            let button = node(&got, id(Part::FrameClose, 0, i));
            assert_eq!((button.role(), button.label().as_deref()), (Role::Button, Some("Close Pane")));
            let press = what_to_do(&targets, &ActionRequest { action: Action::Click, target_tree: TreeId::ROOT, target_node: id(Part::FrameClose, 0, i), data: None });
            assert_eq!(press, Some(Do::ClosePane(i)), "the pane itself, not a click where the ✕ may not be");
        }
    }

    #[test]
    fn a_readers_requests_do_what_the_keys_and_clicks_do() {
        let ask = |targets: &Targets, node: NodeId, action: Action, data: Option<ActionData>| what_to_do(targets, &ActionRequest { action, target_tree: TreeId::ROOT, target_node: node, data });
        let entries = vec![setting("Opacity", Control::Slider { value: 0.5, min: 0.2, max: 1.0, step: 0.05, text: "50%".into(), note: None })];
        let settings = SettingsView { entries, rows: vec![None], ..SettingsView::default() };
        let palette = Dialog {
            kind: DialogKind::Palette,
            title: "Palette".into(),
            field: Some(Field::line("Find", "", "new", None)),
            lists: vec![List { label: "Found".into(), chooser: Chooser::Palette, items: vec![Item { tree: None, label: "New Tab".into(), detail: String::new(), rect: rect(10.0, 10.0, 100.0, 20.0) }], chosen: Some(0) }],
            active: Some((0, 0)),
            ..Dialog::default()
        };
        let (update, targets) = tree(&AccessView { settings: Some(settings), dialogs: vec![palette], focus: Focus::Field(DialogKind::Palette), ..view() });
        let got = read(update);
        // A button is pressed at its middle; Focus never presses it.
        let tab = id(Part::Tab, 0, 1);
        assert_eq!(ask(&targets, tab, Action::Click, None), Some(Do::Click(Rect::new(190.0, 0.0, 80.0, 40.0))));
        assert_eq!(ask(&targets, tab, Action::Focus, None), None);
        assert_eq!(ask(&targets, id(Part::Pane, 0, 0), Action::Focus, None), Some(Do::FocusPane(0)));
        // A slider moves through Settings' own change.
        let slider = id(Part::Setting, 0, 0);
        assert_eq!(ask(&targets, slider, Action::Increment, None), Some(Do::Change(0, Change::Step(1))));
        assert_eq!(ask(&targets, slider, Action::Decrement, None), Some(Do::Change(0, Change::Step(-1))));
        assert_eq!(ask(&targets, slider, Action::SetValue, Some(ActionData::NumericValue(0.75))), Some(Do::Change(0, Change::Set(0.75))));
        assert_eq!(ask(&targets, slider, Action::Focus, None), Some(Do::Select(0)));
        // An item is chosen by Focus and clicked by Click; the field keeps
        // the keys with the item chosen as its active descendant.
        let item = id(Part::ListItem, DialogKind::Palette as usize, 0);
        assert_eq!(ask(&targets, item, Action::Focus, None), Some(Do::Choose(Chooser::Palette, 0)));
        assert_eq!(ask(&targets, item, Action::Click, None), Some(Do::Click(Rect::new(10.0, 10.0, 100.0, 20.0))));
        assert_eq!(got.state().focus().map(|f| f.id()), Some(node(&got, item).id()));
        // A node that does nothing, and one that's gone.
        assert_eq!(ask(&targets, ROOT, Action::Click, None), None);
        assert_eq!(ask(&targets, id(Part::Tab, 0, 9), Action::Click, None), None);
    }

    #[test]
    fn the_tab_sidebar_is_a_tree_whose_groups_fold_and_a_rows_menu_is_a_menu() {
        let ask = |targets: &Targets, node: NodeId, action: Action| what_to_do(targets, &ActionRequest { action, target_tree: TreeId::ROOT, target_node: node, data: None });
        let item = |label: &str, detail: &str, tree: (usize, Option<bool>, bool)| Item { label: label.into(), detail: detail.into(), rect: rect(0.0, 0.0, 200.0, 30.0), tree: Some(tree) };
        let rows = vec![item("Web, 2 tabs", "", (0, Some(true), false)), item("zsh 1", "~/src", (1, None, false)), item("build", "running cargo build", (1, Some(false), true))];
        let sidebar = Dialog {
            kind: DialogKind::TabSidebar,
            title: "Tabs".into(),
            lists: vec![
                List { label: "Tabs".into(), chooser: Chooser::Sidebar, items: rows, chosen: Some(1) },
                List { label: "More".into(), chooser: Chooser::SidebarMenu, items: vec![Item { label: "Rename".into(), ..Item::default() }], chosen: Some(0) },
            ],
            ..Dialog::default()
        };
        let (update, targets) = tree(&AccessView { dialogs: vec![sidebar], focus: Focus::Item(DialogKind::TabSidebar, 0, 1), ..view() });
        let got = read(update);
        let k = DialogKind::TabSidebar as usize;
        let row = |i: usize| id(Part::ListItem, k, i);
        assert_eq!(node(&got, id(Part::List, k, 0)).role(), Role::Tree);
        assert!(node(&got, id(Part::List, k, 0)).is_multiselectable());
        assert_eq!(node(&got, row(2)).is_selected(), Some(true), "picked with others");
        assert_eq!(node(&got, row(0)).is_selected(), Some(false));
        let group = node(&got, row(0));
        assert_eq!((group.role(), group.level(), group.data().is_expanded()), (Role::TreeItem, Some(1), Some(true)));
        let tab = node(&got, row(1));
        assert_eq!((tab.level(), tab.description().as_deref(), tab.is_selected(), tab.data().is_expanded()), (Some(2), Some("~/src"), Some(true), None));
        assert_eq!(got.state().focus().map(|f| f.id()), Some(tab.id()), "the row the keys are on");
        // An open group collapses, a folded split tab expands, and choosing
        // a row moves the keys there.
        assert_eq!(ask(&targets, row(0), Action::Collapse), Some(Do::Fold(Chooser::Sidebar, 0, false)));
        assert_eq!(ask(&targets, row(2), Action::Expand), Some(Do::Fold(Chooser::Sidebar, 2, true)));
        assert_eq!(ask(&targets, row(1), Action::Focus), Some(Do::Choose(Chooser::Sidebar, 1)));
        // Its menu is a menu of items.
        assert_eq!(node(&got, id(Part::List, k, 1)).role(), Role::Menu);
        assert_eq!(node(&got, id(Part::ListItem, k, 1 << 20)).role(), Role::MenuItem);
    }

    #[test]
    fn a_long_row_is_several_runs_and_the_caret_lands_in_the_right_one() {
        let long = "x".repeat(RUN + 10);
        let v = AccessView { panes: vec![Screen { label: "zsh".into(), rect: Rect::new(0.0, 0.0, 4000.0, 18.0), cell: (8.0, 18.0), rows: vec![row(&long, false)], cursor: Some((0, RUN + 3)) }], ..view() };
        let (update, _) = tree(&v);
        let got = read(update);
        let pane = node(&got, id(Part::Pane, 0, 0));
        assert_eq!(pane.document_range().text(), long);
        let caret = pane.text_selection_focus().unwrap();
        assert_eq!(caret.inner_node().id(), node(&got, id(Part::PaneRun, 0, 1)).id());
        assert_eq!(caret.to_global_usv_index(), RUN + 3);
    }

    #[test]
    fn a_wide_letter_and_marks_on_a_letter_are_one_letter_each() {
        // 日 takes two cells, é is e and a combining acute.
        let text = "日e\u{301}x";
        let letters = vec![Letter { bytes: 3, col: 0, cells: 2 }, Letter { bytes: 3, col: 2, cells: 1 }, Letter { bytes: 1, col: 3, cells: 1 }];
        let v = AccessView { panes: vec![Screen { label: "zsh".into(), rect: Rect::new(0.0, 0.0, 80.0, 18.0), cell: (8.0, 18.0), rows: vec![Row { text: text.into(), letters, wraps: false }], cursor: Some((0, 2)) }], ..view() };
        let (update, _) = tree(&v);
        let got = read(update);
        let run = node(&got, id(Part::PaneRun, 0, 0));
        assert_eq!(run.data().character_lengths(), &[3, 3, 1]);
        assert_eq!(run.data().character_positions(), Some(&[0.0, 16.0, 24.0][..]));
        assert_eq!(run.data().character_widths(), Some(&[16.0, 8.0, 8.0][..]));
        // Two letters before the caret: 日 and é.
        let pane = node(&got, id(Part::Pane, 0, 0));
        let caret = pane.text_selection_focus().unwrap();
        assert_eq!(caret.to_global_usv_index(), 1 + 2);
    }

    #[test]
    fn dialogs_are_named_and_their_fields_and_buttons_are_there() {
        let key = Dialog {
            kind: DialogKind::Key,
            title: "Connect Claude".into(),
            text: vec!["Paste your key.".into()],
            field: Some(Field { label: "API key".into(), secret: true, ..Field::line("", "", "sk-ant-123", None) }),
            buttons: vec![Button::new("Close", rect(1.0, 2.0, 3.0, 4.0))],
            ..Dialog::default()
        };
        let help = Dialog {
            kind: DialogKind::Help,
            title: "Find in the scrollback".into(),
            lists: vec![List { label: "Pages".into(), chooser: Chooser::Links, items: vec![Item { label: "Flow".into(), ..Item::default() }], chosen: Some(0) }],
            ..Dialog::default()
        };
        let (update, _) = tree(&AccessView { dialogs: vec![key, help], focus: Focus::Field(DialogKind::Key), ..view() });
        let got = read(update);
        let dialog = node(&got, id(Part::Dialog, DialogKind::Key as usize, 0));
        assert_eq!((dialog.role(), dialog.label().as_deref()), (Role::Dialog, Some("Connect Claude")));
        let field = node(&got, id(Part::Field, DialogKind::Key as usize, 0));
        assert_eq!(field.role(), Role::PasswordInput);
        // The key itself is never read out.
        assert!(!field.document_range().text().contains("sk-ant"));
        assert_eq!(got.state().focus().map(|f| f.id()), Some(field.id()));
        let link = node(&got, id(Part::ListItem, DialogKind::Help as usize, 0));
        assert_eq!((link.role(), link.label().as_deref()), (Role::Link, Some("Flow")));
    }

    #[test]
    fn the_focus_falls_back_to_the_window_when_its_node_is_gone() {
        let (update, _) = tree(&AccessView { focus: Focus::Setting(4), ..view() });
        assert_eq!(update.focus, ROOT);
        read(update);
    }

    #[test]
    fn spoken_words_leave_out_drawn_symbols() {
        assert_eq!(spoken("✦ Ask"), "Ask");
        assert_eq!(spoken("○ Add API key"), "Add API key");
        assert_eq!(spoken("✦ Ask why · exit 1"), "Ask why, exit 1");
        assert_eq!(spoken("Run ⏎"), "Run");
        assert_eq!(spoken("Save as theme…"), "Save as theme…");
        assert_eq!(spoken("⚙"), "");
        // Another language's own punctuation stays.
        assert_eq!(spoken("コマンド・パレット（設定）、"), "コマンド・パレット（設定）、");
        assert_eq!(plain_markdown("# Done\nRun `ls`:\n```sh\nls -la\n```\n**ok**"), "Done\nRun ls:\nls -la\nok");
    }
}
