//! The settings menu: which rows it shows and how a change maps onto the config.

use std::cell::Cell;
use std::rc::Rc;

use crate::ai::usage::{fmt_date, fmt_tokens, Usage};
use crate::ai::Provider;
use crate::config::{Config, CrtPreset, FontConfig, ThemeConfig};
use crate::event::Command;
use crate::i18n::{self, t, tf};
use crate::platform::{keys, per_os};
use crate::render::palette::{self, Rgb8};
use crate::shell::ShellProfile;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingId {
    /// A section's header, which opens and closes it.
    Section(Section),
    Theme,
    ThemeStart,
    ThemePrompt,
    /// Your own theme's start screen, typed in place, a line each.
    OwnStart,
    /// Your own theme's prompt, typed in place.
    OwnPrompt,
    /// How fast your own theme's start screen is typed out.
    OwnBaud,
    /// What your own theme's machine sounds like.
    OwnSounds,
    OwnFrame,
    /// Save as theme…, and Delete for the saved theme chosen.
    SavedTheme,
    /// Opens the theme gallery: the catalog's packages of themes.
    ThemeGallery,
    LiquidStyle,
    WaterView,
    Sunlight,
    Electricity,
    FlowSpeed,
    FlowAmount,
    /// How hard it rains, in place of Flow amount for the rain style.
    RainAmount,
    Ribbons,
    TextRipple,
    Opacity,
    Liquid,
    Reactions,
    ProgramEffects,
    Backdrop,
    Palette,
    Font,
    FontSize,
    /// Sizes the window to a machine's screen at the text size in use, once:
    /// nothing is saved.
    FitScreen,
    /// The text size that fills the window with a screen, leaving the
    /// window as it is.
    FitText,
    /// Sizes of the user's own for the Fit rows, typed in place
    /// (`window.sizes`).
    OwnSizes,
    Language,
    ScreenRefresh,
    /// Whether the window stays on top of every other (`window.on_top`).
    OnTop,
    /// The key taken system-wide that brings Flow's window forward and
    /// puts it away (`window.hotkey`).
    Hotkey,
    /// Whether the hotkey drops the window down over the top of the screen.
    DropDown,
    /// How much of the screen the window dropped down takes.
    DropDownHeight,
    /// Whether it slides down and back up.
    DropDownSlide,
    /// Whether it's put away once Flow loses the keys.
    DropDownHides,
    /// On the Mac, whether it drops over full-screen apps too.
    DropDownOverFullScreen,
    TextColor,
    /// The one color everything is shown in, as on a monochrome tube.
    Phosphor,
    CursorColor,
    CursorShape,
    CursorBlink,
    CursorGlide,
    CursorSpeed,
    Efficient,
    /// Efficient mode while the computer runs on battery or saves power.
    EfficientOnBattery,
    /// Theme details' switches: an old machine's font, its CRT, its colors,
    /// and each of its sounds.
    ThemeFont,
    ThemeCrt,
    ThemeColors,
    /// Theme details: an old machine's shell window frame round each pane.
    ThemeFrame,
    ThemeRealNumbers,
    ThemeSound(crate::looks::Part),
    BackgroundColor,
    Glow,
    StartupLogo,
    PresetFile,
    CrtPreset,
    Crt,
    Scanlines,
    Interlace,
    PhosphorGlow,
    PhosphorDecay,
    Grille,
    Vignette,
    Curvature,
    CrtSaved,
    Provider,
    Model,
    Effort,
    Mode,
    WebSearch,
    /// The assistant's memory kept and sent (`ai.memory`).
    /// Where its commands run (`ai.pane`).
    AiPane,
    /// The user's instructions file and skills folder, opened.
    Instructions,
    Skills,
    /// Its helpers (`ai.helpers`), and their model (`ai.helper_model`).
    Helpers,
    HelperModel,
    Memory,
    /// What it keeps: its facts opened in an editor, or all of it cleared.
    MemoryKept,
    /// Asking to work on its own (`ai.own`), and how long and how many
    /// tokens it may (`ai.own_minutes`, `ai.own_tokens`).
    Own,
    OwnMinutes,
    OwnTokens,
    TokenCap,
    TokenUsage,
    ApiKey,
    DefaultShell,
    /// The tabs open at quit opened again at the next start.
    ReopenTabs,
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    OptionAsMeta,
    MouseReporting,
    /// A click on the line typed at the prompt moves the cursor there
    /// (`shell.click_moves_cursor`).
    ClickMovesCursor,
    /// When each pane's scrollbar shows (`shell.scrollbar`).
    Scrollbar,
    /// How long a command runs before its end, while Flow isn't in front,
    /// posts the system's notification (`ui.notify_after`).
    NotifyAfter,
    /// How a recording of the terminal is encoded (`record.quality`).
    RecordQuality,
    /// Its size: the window's points or its every pixel (`record.size`).
    RecordSize,
    /// The seconds counted down before it starts (`record.countdown`).
    RecordCountdown,
    History,
    HistoryKept,
    Sound,
    SoundVolume,
    Bell,
    SoundCues,
    EffectSounds,
    ProgramSounds,
    LiquidSounds,
    MachineSounds,
    BellFlash,
    About,
    /// Closing every tab and opening a first one as at a start.
    Restart,
    /// Flow's own daily check for a newer release (Linux).
    UpdateChecks,
    /// Reading flowapp's catalog afresh at each start (`apps.check_at_start`).
    AppsAtStart,
    /// How often Flow reads flowapp's catalog by itself (`apps.check`).
    AppsCheck,
    /// When the catalog was read, with Check now.
    AppsCatalog,
    /// An app or package of themes, by its place in the list Settings was
    /// given (`Tally::apps`), with its buttons.
    App(u16),
    /// An installed package with skills: how many are on, and All on and
    /// All off, by its place in the packages Settings was given
    /// (`SkillsView::packages`).
    SkillPackage(u16),
    /// Whether the assistant reads one of the user's own skills, by its
    /// place in `SkillsView::skills`: on until switched off.
    Skill(u16),
    /// Whether it reads one of a package's skills, by its place there: off
    /// until turned on.
    PackageSkill(u16),
    /// Whether outside apps may connect (`mcp.enabled`).
    McpEnabled,
    /// `flow-mcp`'s path, and the lines that connect an app to it.
    McpConnect,
    /// An app remembered, by its place in `mcp.json`, with Forget.
    McpApp(u16),
    /// Whether that app may do a kind of thing (`mcp::tools::Kind::ALL`'s
    /// first three, by their places).
    McpKind(u16, u8),
    /// Whether it may run commands: never, asking, or without asking.
    McpCommands(u16),
    /// The assistant's MCP servers (`mcp-servers.json`): how many, with
    /// Add server… and Open.
    ServersFile,
    /// A server, by its place in `ServersView::servers`: how it stands,
    /// with Connect or Disconnect and Remove.
    Server(u16),
    /// Its command or URL, typed in place.
    ServerHow(u16),
    /// A remote one's sign-in: Sign in (Cancel while it runs) or Sign out,
    /// Client ID… and Token….
    ServerAuth(u16),
    /// Whether the assistant may read its resources (`ai.mcp_resources_on`).
    ServerResources(u16),
    /// How its calls are approved (`ai.mcp_approval`).
    ServerApproval(u16),
    /// How many of its tools are on, with All on and All off.
    ServerTools(u16),
    /// Whether the assistant is offered one of its tools, by the server's
    /// place and the tool's (`ai.mcp_tools_on`): off until turned on.
    ServerTool(u16, u16),
    /// Every key back as Flow has it (Settings' Keys).
    KeysReset,
    /// One of Flow's keys, by its place in `keys::TABLE`.
    Key(u8),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Control {
    /// `note` is a line under it, saying what its value makes.
    /// `step` is how far the keys move it (`slider_value`).
    Slider { value: f32, min: f32, max: f32, step: f32, text: String, note: Option<String> },
    Toggle(bool),
    /// Shared, so the rows built each frame don't copy a long list (fonts).
    Choice { options: Rc<[String]>, index: usize },
    /// Named colors, shown as swatches, then the one in use as #RRGGBB,
    /// which opens the color picker. With `none`, the first is no color at
    /// all (Phosphor's Off), an empty swatch whose value reads as its name.
    Colors { options: Vec<(String, Rgb8)>, index: usize, none: bool },
    Actions(Vec<&'static str>),
    /// A value to read, with one button next to it.
    Readout { text: String, action: &'static str },
    /// A line of words, then buttons at the row's end (an app's Install,
    /// Upgrade and Remove).
    Item { text: String, actions: Vec<&'static str> },
    /// Text typed in place: a line, or several in a box (`multiline`).
    /// Enter, or a click, starts typing in it (`SettingsMenu::editing`);
    /// `empty` says what it means with nothing typed.
    Text { value: String, multiline: bool, empty: &'static str },
    /// A section's header: whether it's open, how many settings it has,
    /// and whose they are: the tab Settings is on, new tabs, or all of Flow.
    Section { open: bool, count: usize, scope: &'static str },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub id: SettingId,
    pub label: &'static str,
    /// Shown at the bottom of the menu while the row is selected.
    pub hint: String,
    pub control: Control,
}

/// A section's header or one of its settings. Headers are rows too, so the
/// keys reach them (to open and close their sections) as they do settings.
#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
    Header(Row),
    Setting(Row),
}

/// Settings' sections, in their order. Each can be closed, hiding its
/// settings until it's opened again; which are closed is kept in the config
/// (`ui.closed_sections`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    General,
    Theme,
    /// What an old machine's theme brings, each part on or off.
    ThemeDetails,
    Liquid,
    Glass,
    Text,
    Crt,
    Assistant,
    /// The assistant's skills: the user's own and each installed
    /// package's, each with a switch (`ai::skills`).
    Skills,
    /// Outside apps using Flow through `flow-mcp` (`crate::mcp`): whether
    /// they may connect, how, and what each may do.
    OutsideApps,
    /// The MCP servers the assistant uses (`crate::mcp::client`): each one,
    /// how its calls are approved, and a switch for each of its tools.
    McpServers,
    Terminal,
    /// Flow's keys, each rebound to whatever key the user wants
    /// (`keys.rs`).
    Keys,
    /// How the terminal is recorded (`Command::Record`).
    Recording,
    /// flowapp's apps and packages of themes, and how Flow checks for newer
    /// ones.
    Apps,
    Sound,
    About,
}

impl Section {
    /// Every section, in Settings' order.
    pub const ALL: [Section; 17] = [
        Section::General,
        Section::Theme,
        Section::ThemeDetails,
        Section::Liquid,
        Section::Glass,
        Section::Text,
        Section::Crt,
        Section::Assistant,
        Section::Skills,
        Section::OutsideApps,
        Section::McpServers,
        Section::Terminal,
        Section::Keys,
        Section::Recording,
        Section::Apps,
        Section::Sound,
        Section::About,
    ];

    /// Its name in the config.
    pub fn id(self) -> &'static str {
        match self {
            Section::General => "general",
            Section::Theme => "theme",
            Section::ThemeDetails => "theme_details",
            Section::Liquid => "liquid",
            Section::Glass => "glass",
            Section::Text => "text",
            Section::Crt => "crt",
            Section::Assistant => "assistant",
            Section::Skills => "skills",
            Section::OutsideApps => "outside_apps",
            Section::McpServers => "mcp_servers",
            Section::Terminal => "terminal",
            Section::Keys => "keys",
            Section::Recording => "recording",
            Section::Apps => "apps",
            Section::Sound => "sound",
            Section::About => "about",
        }
    }

    pub fn title(self) -> &'static str {
        t(&format!("settings.header.{}", self.id()))
    }

    /// Its settings are a tab's whole look ([`crate::looks::TAB_KEYS`]),
    /// which Settings on a tab changes for it alone; the others are Flow's.
    pub fn follows_tab(self) -> bool {
        matches!(self, Section::Theme | Section::ThemeDetails | Section::Liquid | Section::Glass | Section::Text | Section::Crt)
    }

    /// Whether `cfg` shows its settings.
    pub fn open(self, cfg: &Config) -> bool {
        !cfg.ui.closed_sections.iter().any(|s| s == self.id())
    }

    /// Opens or closes it in `cfg`.
    pub fn set_open(self, cfg: &mut Config, open: bool) {
        cfg.ui.closed_sections.retain(|s| s != self.id());
        if !open {
            cfg.ui.closed_sections.push(self.id().to_string());
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Change {
    /// Slider dragged or clicked to this value.
    Set(f32),
    /// Keyboard nudge: slider steps, choice cycles, toggle flips.
    Step(i32),
    Toggle,
    Select(usize),
    Action(usize),
}

/// UI state of the open menu.
#[derive(Debug, Default)]
pub struct SettingsMenu {
    /// Index into the setting rows (headers don't count).
    pub selected: usize,
    /// How far the list is scrolled down, in pixels. Only the drawing knows
    /// the rows' heights, so it keeps this up to date, within bounds.
    pub scroll: Cell<f32>,
    /// The selected row the list last scrolled to show. When the keys select
    /// another, the list moves just far enough to show that one too.
    pub shown: Cell<Option<usize>>,
    /// Slider being dragged with the mouse: row, track x, track width.
    pub dragging: Option<(usize, f32, f32)>,
    pub dirty: bool,
    /// font.families as they were at the first Font pick, which every
    /// later pick starts from.
    pub font_families: Option<Vec<String>>,
    /// The color picker, open over the menu for one of the color rows.
    pub picker: Option<ColorPicker>,
    /// The box asking for a name for a CRT preset or a theme, while it's
    /// open.
    pub naming: Option<Naming>,
    /// The text row being typed in, while one is.
    pub editing: Option<Editing>,
    /// The theme gallery, open over the menu.
    pub gallery: Option<crate::gallery::Gallery>,
    /// It's on the look new tabs open with (the user's own config), not
    /// the tab in front's, which it's on otherwise: chosen in the row of
    /// tabs under its title.
    pub new_tabs: bool,
    /// The search field at the top, while it has the keys.
    pub search: Option<SettingsSearch>,
}

/// Settings' search (`settings_search.rs`): what's typed, every row's words
/// as the field opened, and the rows found, which the list shows in place
/// of the rest while anything's typed.
#[derive(Debug, Clone, Default)]
pub struct SettingsSearch {
    pub query: String,
    pub index: Rc<crate::settings_search::Index>,
    pub found: Vec<crate::settings_search::Found>,
}

impl SettingsSearch {
    /// Something's typed, so the list is what it finds.
    pub fn finding(&self) -> bool {
        !self.query.trim().is_empty()
    }

    /// Finds afresh what's typed now.
    pub fn refind(&mut self) {
        self.found = self.index.find(&self.query);
    }
}

/// The box asking for a name to save something under: the name typed so
/// far, what it saves, and the names already taken, which saving again
/// saves over.
#[derive(Debug, Clone)]
pub struct Naming {
    pub name: String,
    pub what: Named,
    pub taken: Vec<String>,
}

/// What a name is asked for.
#[derive(Debug, Clone)]
pub enum Named {
    /// The CRT's look as it's set.
    Crt,
    /// A theme: the config worn when Save as theme… was chosen, which may
    /// be a tab's own theme rather than what Settings shows.
    Theme(Box<Config>),
    /// An MCP server added in Settings' MCP servers.
    Server,
    /// The client ID registered for Flow with a server's sign-in, by the
    /// server's name; left empty, Flow registers itself.
    ClientId(String),
}

impl Naming {
    /// A box for the CRT's look, with the names of `cfg`'s presets taken.
    pub fn crt(cfg: &Config) -> Self {
        Naming { name: String::new(), what: Named::Crt, taken: cfg.crt.presets.iter().map(|p| p.name.clone()).collect() }
    }

    /// A box for the theme `worn` wears, with the names of `cfg`'s saved
    /// themes taken.
    pub fn theme(cfg: &Config, worn: Config) -> Self {
        Naming { name: String::new(), what: Named::Theme(Box::new(worn)), taken: cfg.themes.saved.iter().map(|t| t.name.clone()).collect() }
    }

    /// A box for a new MCP server's name, with the names of the servers
    /// there already, which it refuses.
    pub fn server(taken: Vec<String>) -> Self {
        Naming { name: String::new(), what: Named::Server, taken }
    }

    /// A box for server `server`'s client ID, holding the one typed before.
    pub fn client_id(server: &str, current: Option<&str>) -> Self {
        Naming { name: current.unwrap_or_default().to_string(), what: Named::ClientId(server.to_string()), taken: Vec::new() }
    }

    /// The name, as it's saved.
    pub fn chosen(&self) -> &str {
        self.name.trim()
    }

    /// Saving would save over what's kept under the name typed.
    pub fn over(&self) -> bool {
        self.taken.iter().any(|t| t == self.chosen())
    }

    /// The words for the box under `naming`: `crt_name` or `theme_name`.
    pub fn words(&self) -> &'static str {
        match self.what {
            Named::Crt => "crt_name",
            Named::Theme(_) => "theme_name",
            Named::Server => "server_name",
            Named::ClientId(_) => "client_id_name",
        }
    }
}

/// A text row being typed in: which one, its text and caret, and the theme
/// it types into (your own, or the saved one chosen) as it was before,
/// which Esc puts back.
#[derive(Debug, Clone)]
pub struct Editing {
    pub id: SettingId,
    pub field: TextField,
    /// What Esc puts back.
    pub was: Was,
}

/// What a text row held before it was typed in: your own theme, for its
/// start screen and prompt, or a value of its own.
#[derive(Debug, Clone)]
pub enum Was {
    Theme(ThemeConfig),
    Text(String),
}

/// Text typed into a Settings row, a line or several, and where the caret
/// is. It holds only the text and the caret, so the keys that edit it are
/// tested without drawing anything.
#[derive(Debug, Clone, PartialEq)]
pub struct TextField {
    lines: Vec<String>,
    /// The caret's line, and how many characters along it.
    line: usize,
    col: usize,
    /// The column ↑ and ↓ keep to, across lines too short for it.
    goal: Option<usize>,
    multiline: bool,
    /// The most characters a line takes.
    most: usize,
}

impl TextField {
    /// The most lines a field takes, and characters a line: a start screen
    /// is a screenful, and a paste of a whole file shouldn't hang the menu.
    pub const MAX_LINES: usize = 40;
    pub const MAX_CHARS: usize = 200;
    /// The most characters a long line takes: a command with its paths, a
    /// URL, an answer to a server's question.
    pub const LONG_CHARS: usize = 4000;

    /// A field holding `text` (lines parted by `\n` where it takes
    /// several), with the caret at its end.
    pub fn new(text: &str, multiline: bool) -> Self {
        let mut field = TextField { lines: vec![String::new()], line: 0, col: 0, goal: None, multiline, most: Self::MAX_CHARS };
        field.insert(text);
        field
    }

    /// A field of one line holding `text`, with the caret at its end, that
    /// takes at most `most` characters: a tab's name.
    pub fn line(text: &str, most: usize) -> Self {
        let mut field = TextField { lines: vec![String::new()], line: 0, col: 0, goal: None, multiline: false, most };
        field.insert(text);
        field
    }

    /// A field of one long line holding `text`, with the caret at its end.
    pub fn long(text: &str) -> Self {
        let mut field = TextField { lines: vec![String::new()], line: 0, col: 0, goal: None, multiline: false, most: Self::LONG_CHARS };
        field.insert(text);
        field
    }

    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    pub fn multiline(&self) -> bool {
        self.multiline
    }

    /// The caret: its line, and how many characters along it.
    pub fn caret(&self) -> (usize, usize) {
        (self.line, self.col)
    }

    fn len(&self, line: usize) -> usize {
        self.lines[line].chars().count()
    }

    /// Where character `col` of the caret's line starts, in bytes.
    fn byte(&self, col: usize) -> usize {
        let line = &self.lines[self.line];
        line.char_indices().nth(col).map_or(line.len(), |(i, _)| i)
    }

    /// Types `text` at the caret. A line break makes a new line where the
    /// field takes several and is left out where it doesn't; a tab is a
    /// space, and other control characters are left out.
    pub fn insert(&mut self, text: &str) {
        self.goal = None;
        for ch in text.replace("\r\n", "\n").chars() {
            match ch {
                '\n' | '\r' => self.new_line(),
                '\t' => self.put(' '),
                ch if ch.is_control() => {}
                ch => self.put(ch),
            }
        }
    }

    fn put(&mut self, ch: char) {
        if self.len(self.line) < self.most {
            let at = self.byte(self.col);
            self.lines[self.line].insert(at, ch);
            self.col += 1;
        }
    }

    /// Breaks the line at the caret, where the field takes several.
    pub fn new_line(&mut self) {
        self.goal = None;
        if self.multiline && self.lines.len() < Self::MAX_LINES {
            let at = self.byte(self.col);
            let rest = self.lines[self.line].split_off(at);
            self.lines.insert(self.line + 1, rest);
            self.line += 1;
            self.col = 0;
        }
    }

    /// Deletes the character before the caret; at a line's start, joins
    /// the line to the one above.
    pub fn backspace(&mut self) {
        self.goal = None;
        if self.col > 0 {
            let (start, end) = (self.byte(self.col - 1), self.byte(self.col));
            self.lines[self.line].replace_range(start..end, "");
            self.col -= 1;
        } else if self.line > 0 {
            let rest = self.lines.remove(self.line);
            self.line -= 1;
            self.col = self.len(self.line);
            self.lines[self.line].push_str(&rest);
        }
    }

    /// Deletes back to the line's start (⌘⌫), or at its start joins the
    /// line to the one above.
    pub fn backspace_line(&mut self) {
        if self.col == 0 {
            return self.backspace();
        }
        let at = self.byte(self.col);
        self.lines[self.line].replace_range(..at, "");
        self.col = 0;
        self.goal = None;
    }

    /// Deletes the character after the caret; at a line's end, joins the
    /// next line to it.
    pub fn delete(&mut self) {
        self.goal = None;
        if self.col < self.len(self.line) {
            let (start, end) = (self.byte(self.col), self.byte(self.col + 1));
            self.lines[self.line].replace_range(start..end, "");
        } else if self.line + 1 < self.lines.len() {
            let next = self.lines.remove(self.line + 1);
            self.lines[self.line].push_str(&next);
        }
    }

    pub fn left(&mut self) {
        self.goal = None;
        if self.col > 0 {
            self.col -= 1;
        } else if self.line > 0 {
            self.line -= 1;
            self.col = self.len(self.line);
        }
    }

    pub fn right(&mut self) {
        self.goal = None;
        if self.col < self.len(self.line) {
            self.col += 1;
        } else if self.line + 1 < self.lines.len() {
            self.line += 1;
            self.col = 0;
        }
    }

    /// To the start of the word before the caret (⌥← on the Mac, Ctrl+←
    /// elsewhere), past the spaces and marks between; from a line's start,
    /// to the start of the last word of the line above.
    pub fn word_left(&mut self) {
        self.goal = None;
        if self.col == 0 {
            if self.line == 0 {
                return;
            }
            self.left();
        }
        let chars: Vec<char> = self.lines[self.line].chars().collect();
        while self.col > 0 && !is_word(chars[self.col - 1]) {
            self.col -= 1;
        }
        while self.col > 0 && is_word(chars[self.col - 1]) {
            self.col -= 1;
        }
    }

    /// To the end of the word after the caret, as [`TextField::word_left`]
    /// goes back; from a line's end, to the end of the next line's first.
    pub fn word_right(&mut self) {
        self.goal = None;
        if self.col == self.len(self.line) {
            if self.line + 1 == self.lines.len() {
                return;
            }
            self.right();
        }
        let chars: Vec<char> = self.lines[self.line].chars().collect();
        while self.col < chars.len() && !is_word(chars[self.col]) {
            self.col += 1;
        }
        while self.col < chars.len() && is_word(chars[self.col]) {
            self.col += 1;
        }
    }

    /// Deletes back to where [`TextField::word_left`] goes (⌥⌫, or
    /// Ctrl+Backspace), within the line; at its start, joins it to the line
    /// above.
    pub fn backspace_word(&mut self) {
        if self.col == 0 {
            return self.backspace();
        }
        let end = self.col;
        self.word_left();
        let (start, end) = (self.byte(self.col), self.byte(end));
        self.lines[self.line].replace_range(start..end, "");
    }

    /// Deletes up to where [`TextField::word_right`] goes, within the line;
    /// at its end, joins the next line to it.
    pub fn delete_word(&mut self) {
        if self.col == self.len(self.line) {
            return self.delete();
        }
        let start = self.col;
        self.word_right();
        let (from, to) = (self.byte(start), self.byte(self.col));
        self.lines[self.line].replace_range(from..to, "");
        self.col = start;
    }

    /// Puts the caret at character `col` of line `line`, as a click does,
    /// kept within the text.
    pub fn place(&mut self, line: usize, col: usize) {
        self.goal = None;
        self.line = line.min(self.lines.len() - 1);
        self.col = col.min(self.len(self.line));
    }

    /// To the line above, at the column the caret had before it met a
    /// shorter line; from the first line, to its start.
    pub fn up(&mut self) {
        if self.line == 0 {
            return self.home();
        }
        let goal = *self.goal.get_or_insert(self.col);
        self.line -= 1;
        self.col = goal.min(self.len(self.line));
    }

    /// To the line below, as [`TextField::up`] goes up; from the last line,
    /// to its end.
    pub fn down(&mut self) {
        if self.line + 1 >= self.lines.len() {
            return self.end();
        }
        let goal = *self.goal.get_or_insert(self.col);
        self.line += 1;
        self.col = goal.min(self.len(self.line));
    }

    /// To the line's start.
    pub fn home(&mut self) {
        self.goal = None;
        self.col = 0;
    }

    /// To the line's end.
    pub fn end(&mut self) {
        self.goal = None;
        self.col = self.len(self.line);
    }

    /// To the start of the whole text.
    pub fn top(&mut self) {
        self.goal = None;
        (self.line, self.col) = (0, 0);
    }

    /// To the end of the whole text.
    pub fn bottom(&mut self) {
        self.goal = None;
        self.line = self.lines.len() - 1;
        self.col = self.len(self.line);
    }
}

/// Whether `ch` is part of a word for the word jumps: letters, digits and
/// `_`, in any script.
fn is_word(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

/// The text row `id` shows of `cfg`: your own theme's start screen (or
/// the saved theme's chosen, [`Config::worn_theme`]), its lines parted by
/// line breaks, or its prompt, with each new line in it written `\n`, as
/// it's typed.
pub fn row_text(cfg: &Config, id: SettingId) -> String {
    let theme = cfg.worn_theme();
    match id {
        SettingId::OwnStart => theme.start.join("\n"),
        SettingId::OwnPrompt => theme.prompt.replace('\n', "\\n"),
        SettingId::OwnSizes => cfg.window.sizes.clone(),
        _ => String::new(),
    }
}

/// Puts `text`, as typed in row `id`, in `cfg`: nothing typed is no start
/// screen, or the shell's own prompt.
pub fn set_row_text(cfg: &mut Config, id: SettingId, text: &str) {
    let theme = cfg.worn_theme_mut();
    match id {
        SettingId::OwnStart => theme.start = if text.is_empty() { Vec::new() } else { text.split('\n').map(String::from).collect() },
        SettingId::OwnPrompt => theme.prompt = text.replace("\\n", "\n"),
        SettingId::OwnSizes => cfg.window.sizes = text.to_string(),
        _ => {}
    }
}

/// Which part of the color picker a point is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerPart {
    /// Saturation across, brightness up.
    Field,
    Hue,
}

/// A color picker for one color row: a field of saturation and brightness
/// at one hue, a hue bar, and the hex value, which can be typed.
#[derive(Debug, Clone)]
pub struct ColorPicker {
    pub id: SettingId,
    pub label: &'static str,
    /// Hue (0..1), saturation and brightness as picked. They're kept rather
    /// than worked out from the color, since a gray or black has no hue of
    /// its own to come back to.
    pub hsv: [f32; 3],
    /// The color it opened on, which Cancel and Esc go back to.
    pub was: Rgb8,
    /// It opened on no color at all (Phosphor's Off), which Cancel and Esc
    /// go back to instead.
    pub was_none: bool,
    /// The color exactly, when it came as one (opened on, typed or pasted)
    /// rather than from the field, so it isn't moved a step by the trip
    /// through hue, saturation and brightness.
    exact: Option<Rgb8>,
    /// Hex digits typed so far, while a value is being typed.
    pub typed: Option<String>,
    /// The part being dragged and its rect (x, y, w, h).
    pub dragging: Option<(PickerPart, [f32; 4])>,
}

impl ColorPicker {
    pub fn new(id: SettingId, label: &'static str, color: Rgb8) -> Self {
        ColorPicker { id, label, hsv: palette::to_hsv(color), was: color, was_none: false, exact: Some(color), typed: None, dragging: None }
    }

    pub fn color(&self) -> Rgb8 {
        self.exact.unwrap_or_else(|| palette::from_hsv(self.hsv))
    }

    /// Picks from point `p` in `part`, laid out over `rect` (x, y, w, h).
    pub fn pick(&mut self, part: PickerPart, [x, y, w, h]: [f32; 4], p: (f32, f32)) {
        let across = ((p.0 - x) / w.max(1.0)).clamp(0.0, 1.0);
        match part {
            PickerPart::Field => {
                self.hsv[1] = across;
                self.hsv[2] = 1.0 - ((p.1 - y) / h.max(1.0)).clamp(0.0, 1.0);
            }
            // Short of 1, which is red again, so the marker stays at the end.
            PickerPart::Hue => self.hsv[0] = across.min(0.9999),
        }
        self.exact = None;
        self.typed = None;
    }

    /// Turns the hue by `turn` (a fraction of the circle) and the brightness
    /// by `brighter`, from the keyboard.
    pub fn nudge(&mut self, turn: f32, brighter: f32) {
        self.hsv[0] = (self.hsv[0] + turn).rem_euclid(1.0);
        self.hsv[2] = (self.hsv[2] + brighter).clamp(0.0, 1.0);
        self.exact = None;
        self.typed = None;
    }

    /// Takes a typed hex digit; true once six make a color, which is then
    /// picked. A seventh starts a new value.
    pub fn type_digit(&mut self, digit: char) -> bool {
        if !digit.is_ascii_hexdigit() {
            return false;
        }
        let typed = self.typed.get_or_insert_with(String::new);
        if typed.len() >= 6 {
            typed.clear();
        }
        typed.push(digit.to_ascii_uppercase());
        match palette::parse_hex(typed) {
            Some(c) => {
                self.set(c);
                true
            }
            None => false,
        }
    }

    pub fn backspace(&mut self) {
        let color = self.color();
        self.typed.get_or_insert_with(|| palette::hex(color)[1..].to_string()).pop();
    }

    /// Picks `c`, as typed in full or pasted.
    pub fn set(&mut self, c: Rgb8) {
        let hsv = palette::to_hsv(c);
        // A gray keeps the hue the picker was on.
        self.hsv = if hsv[1] > 0.0 { hsv } else { [self.hsv[0], 0.0, hsv[2]] };
        self.exact = Some(c);
        self.typed = self.typed.take().filter(|t| t.len() < 6);
    }

    /// The hex value as shown: what's typed so far, or the color.
    pub fn shown(&self) -> String {
        match &self.typed {
            Some(t) => format!("#{t}"),
            None => palette::hex(self.color()),
        }
    }
}

/// Where the list, scrolled down `scroll` pixels with `view_h` of it in
/// view, scrolls to show a row from `top` to `bottom`: nowhere if it shows
/// already, else just far enough.
pub fn reveal(scroll: f32, top: f32, bottom: f32, view_h: f32) -> f32 {
    if top < scroll {
        top
    } else {
        scroll.max(bottom - view_h)
    }
}

impl SettingsMenu {
    /// Puts `family` first in `font`, ahead of the families the menu began
    /// with, so stepping through fonts leaves only the last one picked
    /// rather than each one passed on the way.
    pub fn pick_font(&mut self, font: &mut FontConfig, family: &str) {
        font.families = self.font_families.get_or_insert_with(|| font.families.clone()).clone();
        font.prefer(family);
    }
}

pub const STYLES: [&str; 5] = ["clouds", "water", "oil", "rain", "none"];
pub const WATER_VIEWS: [&str; 2] = ["above", "below"];
/// Windows has three materials behind windows; macOS has one blur, and Linux
/// the one some compositors offer (KDE Plasma's).
pub const BACKDROPS: &[&str] = if cfg!(windows) { &["acrylic", "mica", "blur", "none"] } else { &["blur", "none"] };
pub const PALETTES: [&str; 4] = ["aurora", "ocean", "ember", "mono"];
pub const CURSOR_SHAPES: [&str; 3] = ["block", "underline", "bar"];
/// Startup logos: Flow's, the system's own (the distribution's on Linux,
/// Flow's where it has none to show), or none.
pub const LOGOS: [&str; 4] = ["flow", "system", "chip", "off"];
pub const EFFORTS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];
/// Guide, then auto, as the Mode row's index says.
pub const MODES: [&str; 2] = ["guide", "auto"];
/// What a program's bell does (`sound.bell`).
pub const BELLS: [&str; 3] = ["system", "flow", "off"];
/// The speeds your own theme's start screen is typed at, in baud (bits a
/// second, ten to a letter); 0 shows it at once.
pub const BAUDS: [u32; 7] = [0, 300, 1200, 2400, 4800, 9600, 19200];
/// What your own theme's machine can sound like (`looks::Sounds`).
pub const THEME_SOUNDS: [&str; 10] = ["quiet", "tube", "terminal", "disk", "fan", "floppy", "chime", "tones", "printer", "teletype"];

/// What the Font row shows when no font could be found at all.
pub const SYSTEM_FONT: &str = "System monospace";

/// The Font row's families, sorted, and the one the terminal draws with.
#[derive(Debug, Clone, Default)]
pub struct FontChoices {
    pub families: Rc<[String]>,
    pub current: usize,
}

/// Sorts and dedupes `installed` without regard to case, and makes sure the
/// family in use is among them so the row always shows it.
pub fn font_choices(installed: Vec<String>, in_use: Option<&str>) -> FontChoices {
    let in_use = in_use.unwrap_or(SYSTEM_FONT);
    let mut families = installed;
    families.push(in_use.to_string());
    families.sort_by_key(|f| f.to_lowercase());
    families.dedup_by(|a, b| a.to_lowercase() == b.to_lowercase());
    let key = in_use.to_lowercase();
    let current = families.iter().position(|f| f.to_lowercase() == key).unwrap_or(0);
    FontChoices { families: families.into(), current }
}

/// What the window holds at the text size in use, handed to the menu's
/// entries as the font choices are: the terminal's columns and rows, each
/// character's cell in the screen's pixels (as wide as the font's letters,
/// so often a fraction of one), and the display's scale, the pixels in a
/// point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextGrid {
    pub cols: usize,
    pub rows: usize,
    pub cell: (f32, f32),
    pub scale: f32,
}

impl TextGrid {
    /// "80 × 24 characters of 9 × 18 pixels"; on a scaled display the
    /// cell in points too, as the font's size is given.
    pub fn describe(&self) -> String {
        let (w, h) = self.cell;
        let (cols, rows) = (self.cols, self.rows);
        let (w, h, pw, ph) = (amount(w), amount(h), amount(w / self.scale), amount(h / self.scale));
        let mut args: Vec<(&str, &dyn std::fmt::Display)> = vec![("cols", &cols), ("rows", &rows), ("w", &w), ("h", &h)];
        if (self.scale - 1.0).abs() < 0.01 {
            return tf("settings.font_size.grid", &args);
        }
        args.extend([("pw", &pw as &dyn std::fmt::Display), ("ph", &ph)]);
        tf("settings.font_size.grid_scaled", &args)
    }
}

/// A size to a tenth, without the tenth when it's whole: 9, 8.4.
fn amount(v: f32) -> String {
    let tenths = (v * 10.0).round();
    if tenths % 10.0 == 0.0 { format!("{:.0}", tenths / 10.0) } else { format!("{:.1}", tenths / 10.0) }
}

/// The screens Fit window to offers, as columns and rows: a terminal's, the
/// PC's, a home computer's and teletext's. The home computer's comes last,
/// since the least window holds more than its 40 columns until the text is
/// about 20 points.
pub const SCREENS: [(usize, usize); 3] = [(80, 24), (80, 25), (40, 25)];
/// The least and most columns and rows a size of the user's own may have:
/// the layout's least grid, and more than any screen holds.
pub const SIZE_COLS: (usize, usize) = (10, 500);
pub const SIZE_ROWS: (usize, usize) = (3, 300);

/// Fit window to's screens for `cfg`: the worn machine's own first, where a
/// machine's theme is worn, then the rest of [`SCREENS`], then the user's
/// own sizes (`window.sizes`) not among them.
pub fn screens(cfg: &Config) -> Vec<(usize, usize)> {
    let own = match crate::looks::InUse::of(cfg) {
        crate::looks::InUse::Builtin(look) => look.screen,
        crate::looks::InUse::Own(..) => None,
    };
    let mut screens: Vec<(usize, usize)> = own.into_iter().collect();
    screens.extend(SCREENS.iter().filter(|s| own != Some(**s)));
    for size in parse_sizes(&cfg.window.sizes) {
        if !screens.contains(&size) {
            screens.push(size);
        }
    }
    screens
}

/// What Your sizes made of its text: the sizes it takes and what it left
/// out, which Enter says in a note (`App::finish_editing`).
#[derive(Debug, Default, PartialEq)]
pub struct SizesRead {
    /// The sizes taken, each once, in the order typed.
    pub sizes: Vec<(usize, usize)>,
    /// Sizes written as sizes but past [`SIZE_COLS`] or [`SIZE_ROWS`],
    /// each once, in the order typed.
    pub outside: Vec<(usize, usize)>,
    /// Whether a number made no size (`80x`, `x24`, `100`).
    pub stray: bool,
}

/// The sizes in `text`, as Your sizes takes them: each columns, then `x`,
/// `X`, `×` or `*`, then rows, spaces round the sign allowed, anything
/// between them (commas, words) passed over; each within [`SIZE_COLS`]
/// and [`SIZE_ROWS`], and each once, in the order typed.
pub fn parse_sizes(text: &str) -> Vec<(usize, usize)> {
    read_sizes(text).sizes
}

/// [`parse_sizes`], with what it left out: sizes too small or too large,
/// and numbers that made no size.
pub fn read_sizes(text: &str) -> SizesRead {
    let chars: Vec<char> = text.chars().collect();
    let number = |i: &mut usize| -> Option<usize> {
        let from = *i;
        while *i < chars.len() && chars[*i].is_ascii_digit() {
            *i += 1;
        }
        let digits: String = chars[from..*i].iter().collect();
        (!digits.is_empty()).then(|| digits.parse().unwrap_or(usize::MAX))
    };
    let skip_spaces = |i: &mut usize| {
        while *i < chars.len() && chars[*i] == ' ' {
            *i += 1;
        }
    };
    let mut read = SizesRead::default();
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let Some(cols) = number(&mut i) else { continue };
        let mut j = i;
        skip_spaces(&mut j);
        let mut size = None;
        if j < chars.len() && matches!(chars[j], 'x' | 'X' | '×' | '*') {
            j += 1;
            skip_spaces(&mut j);
            if let Some(rows) = number(&mut j) {
                i = j;
                size = Some((cols, rows));
            }
        }
        let Some(size) = size else {
            read.stray = true;
            continue;
        };
        let fits = (SIZE_COLS.0..=SIZE_COLS.1).contains(&size.0) && (SIZE_ROWS.0..=SIZE_ROWS.1).contains(&size.1);
        let list = if fits { &mut read.sizes } else { &mut read.outside };
        if !list.contains(&size) {
            list.push(size);
        }
    }
    read
}

/// The note Enter shows when Your sizes left something out (none when it
/// took everything): the sizes too small or too large, up to three, and
/// what a size may be.
pub fn sizes_note(read: &SizesRead) -> Option<String> {
    let least = sizes_text(&[(SIZE_COLS.0, SIZE_ROWS.0)]);
    let most = sizes_text(&[(SIZE_COLS.1, SIZE_ROWS.1)]);
    let mut sizes = sizes_text(&read.outside[..read.outside.len().min(3)]);
    if read.outside.len() > 3 {
        sizes.push_str(", …");
    }
    let args: [(&str, &dyn std::fmt::Display); 3] = [("sizes", &sizes), ("least", &least), ("most", &most)];
    match (read.outside.is_empty(), read.stray) {
        (true, false) => None,
        (false, false) => Some(tf("toast.sizes_outside", &args)),
        (true, true) => Some(t("toast.sizes_stray").to_string()),
        (false, true) => Some(tf("toast.sizes_both", &args)),
    }
}

/// Sizes as Your sizes keeps them once typed: `132x43, 40x24`.
pub fn sizes_text(sizes: &[(usize, usize)]) -> String {
    sizes.iter().map(|(cols, rows)| format!("{cols}x{rows}")).collect::<Vec<_>>().join(", ")
}

/// The Fit window to row: its screens as "80 × 24", the one the window
/// holds now chosen (none, past the end, where it holds another), and its
/// hint naming the machine whose screen comes first.
fn fit_row(cfg: &Config, grid: Option<TextGrid>) -> Entry {
    let screens = screens(cfg);
    let options: Vec<String> = screens.iter().map(|(cols, rows)| format!("{cols} × {rows}")).collect();
    let index = grid.and_then(|g| screens.iter().position(|&s| s == (g.cols, g.rows))).unwrap_or(options.len());
    let hint = match crate::looks::InUse::of(cfg) {
        crate::looks::InUse::Builtin(look) if look.screen.is_some() => {
            tf("settings.fit_screen.hint_machine", &[("machine", &words("theme", look.id))])
        }
        _ => words("fit_screen", "hint").to_string(),
    };
    row(SettingId::FitScreen, "fit_screen", hint, Control::Choice { options: options.into(), index })
}

/// The Fit text to row: Fit window to's screens, the other way round, the
/// one the window holds now chosen as there.
fn fit_text_row(cfg: &Config, grid: Option<TextGrid>) -> Entry {
    let screens = screens(cfg);
    let options: Vec<String> = screens.iter().map(|(cols, rows)| format!("{cols} × {rows}")).collect();
    let index = grid.and_then(|g| screens.iter().position(|&s| s == (g.cols, g.rows))).unwrap_or(options.len());
    row(SettingId::FitText, "fit_text", words("fit_text", "hint").to_string(), Control::Choice { options: options.into(), index })
}

/// The largest of Font size's whole sizes at which a window that holds
/// `holds(size)` columns and rows at each has room for `cols` × `rows`,
/// starting from `guess`; none where even the least size has too little.
/// `holds` grows with the window and shrinks as the text grows.
pub fn text_size_for((cols, rows): (usize, usize), guess: f32, mut holds: impl FnMut(f32) -> (usize, usize)) -> Option<f32> {
    let (least, most, _) = FONT_SIZE;
    let mut fits = |size: f32| {
        let (c, r) = holds(size);
        c >= cols && r >= rows
    };
    let mut size = guess.round().clamp(least, most);
    if fits(size) {
        while size < most && fits(size + 1.0) {
            size += 1.0;
        }
        return Some(size);
    }
    while size > least {
        size -= 1.0;
        if fits(size) {
            return Some(size);
        }
    }
    None
}

/// Screen refresh choices, as (frames a second, vsync): 0 keeps to the
/// display's own rate, and the last doesn't wait for the display at all.
pub const REFRESH_RATES: [(u32, bool); 7] = [(0, true), (30, true), (60, true), (120, true), (144, true), (240, true), (0, false)];

/// Token cap choices; 0 means no cap.
pub const TOKEN_CAPS: [u64; 8] = [0, 100_000, 250_000, 500_000, 1_000_000, 2_000_000, 5_000_000, 10_000_000];

pub const SUNLIGHT: (f32, f32, f32) = (0.0, 1.0, 0.05);
pub const ELECTRICITY: (f32, f32, f32) = (0.0, 1.0, 0.1);
pub const FLOW_SPEED: (f32, f32, f32) = (0.1, 2.0, 0.1);
/// The cursor's glide, times its rate as it always was.
pub const CURSOR_SPEED: (f32, f32, f32) = (0.5, 4.0, 0.5);
pub const FLOW_AMOUNT: (f32, f32, f32) = (0.0, 1.0, 0.1);
pub const RAIN_AMOUNT: (f32, f32, f32) = (0.0, 1.0, 0.05);
pub const RIBBONS: (f32, f32, f32) = (0.0, 2.0, 0.1);
pub const TEXT_RIPPLE: (f32, f32, f32) = (0.0, 6.0, 0.5);
pub const OPACITY: (f32, f32, f32) = (0.2, 1.0, 0.05);
pub const FONT_SIZE: (f32, f32, f32) = (8.0, 40.0, 1.0);
pub const GLOW: (f32, f32, f32) = (0.0, 2.0, 0.1);
pub const CRT: (f32, f32, f32) = (0.0, 1.0, 0.05);
pub const SCANLINES: (f32, f32, f32) = (0.0, 1.0, 0.05);
pub const PHOSPHOR_GLOW: (f32, f32, f32) = (0.0, 2.0, 0.1);
pub const PHOSPHOR_DECAY: (f32, f32, f32) = (0.0, 0.3, 0.01);
pub const GRILLE: (f32, f32, f32) = (0.0, 1.0, 0.05);
pub const VIGNETTE: (f32, f32, f32) = (0.0, 1.0, 0.05);
pub const CURVATURE: (f32, f32, f32) = (0.0, 1.0, 0.05);
pub const VOLUME: (f32, f32, f32) = (0.05, 1.0, 0.05);

/// The CRT looks Flow comes with: their words are under
/// settings.crt_preset in the language files, and Classic is the default's.
pub fn builtin_crt_presets() -> Vec<CrtPreset> {
    let look = |name: &str, strength, scanlines, interlace, glow, decay, grille, vignette, curvature| CrtPreset {
        name: name.to_string(),
        strength,
        scanlines,
        interlace,
        glow,
        decay,
        grille,
        vignette,
        curvature,
    };
    vec![
        look("subtle", 0.6, 0.3, false, 0.6, 0.03, 0.2, 0.3, 0.0),
        look("classic", 0.8, 0.5, true, 1.2, 0.06, 0.4, 0.6, 0.0),
        look("arcade", 1.0, 0.7, true, 1.6, 0.05, 0.6, 0.8, 0.6),
        look("phosphor", 0.9, 0.4, true, 1.5, 0.2, 0.1, 0.5, 0.3),
        look("television", 1.0, 0.6, true, 1.0, 0.08, 0.8, 0.9, 0.8),
        // A home computer's own monitor: not interlaced, so its 256 lines
        // stand still, a slot mask finer than a television's, and a
        // gentler curve.
        look("monitor", 0.9, 0.55, false, 0.9, 0.05, 0.5, 0.6, 0.4),
    ]
}

/// What the Preset row offers: Flow's looks, then the user's, each with the
/// name to show and whether it's one the user saved.
pub fn crt_presets(cfg: &Config) -> Vec<(String, CrtPreset, bool)> {
    let builtin = builtin_crt_presets().into_iter().map(|p| (t(&format!("settings.crt_preset.{}", p.name)).to_string(), p, false));
    let saved = cfg.crt.presets.iter().map(|p| (p.name.clone(), p.clone(), true));
    builtin.chain(saved).collect()
}

/// Which of [`crt_presets`] is the look set now, if any: the user's first,
/// so a look they saved shows by their name.
pub fn current_crt_preset(presets: &[(String, CrtPreset, bool)], crt: &crate::config::CrtConfig, strength: f32) -> Option<usize> {
    let matching = |user: bool| presets.iter().position(|(_, p, saved)| *saved == user && p.matches(crt, strength));
    matching(true).or_else(|| matching(false))
}

/// The colors the Text color and Cursor color rows offer. Frost is the
/// text's default and Cyan the cursor's.
pub const COLORS: [(&str, Rgb8); 7] = [
    ("Frost", palette::FOREGROUND),
    ("White", [0xFF, 0xFF, 0xFF]),
    ("Cyan", palette::CURSOR),
    ("Green", [0x7E, 0xE0, 0x9F]),
    ("Amber", [0xFF, 0xC8, 0x6E]),
    ("Pink", [0xFF, 0x9E, 0xC7]),
    ("Violet", [0xC5, 0x8B, 0xFF]),
];

/// The phosphors the Phosphor row offers after Off: the green and amber
/// screens' own, and a white tube's.
pub const PHOSPHORS: [(&str, Rgb8); 3] = [("Green", [0x41, 0xFF, 0x6E]), ("Amber", [0xFF, 0xB0, 0x00]), ("White", [0xE8, 0xF0, 0xF0])];

/// Dark tints the Background color row offers; Night is the default.
pub const BACKGROUNDS: [(&str, Rgb8); 7] = [
    ("Night", palette::BACKGROUND),
    ("Black", [0x00, 0x00, 0x00]),
    ("Navy", [0x0A, 0x16, 0x30]),
    ("Forest", [0x08, 0x1C, 0x14]),
    ("Plum", [0x1E, 0x0C, 0x26]),
    ("Ember", [0x26, 0x0E, 0x0A]),
    ("Graphite", [0x1E, 0x20, 0x24]),
];

/// A color row's options from `presets` and the one in use; any other
/// color, picked or set in the config file, joins them at the end.
pub fn color_options(presets: &[(&str, Rgb8)], current: Rgb8) -> (Vec<(String, Rgb8)>, usize) {
    let mut options: Vec<(String, Rgb8)> = presets.iter().map(|&(name, c)| (name.to_string(), c)).collect();
    let index = match presets.iter().position(|&(_, c)| c == current) {
        Some(i) => i,
        None => {
            options.push((palette::hex(current), current));
            options.len() - 1
        }
    };
    (options, index)
}

fn slider((min, max, step): (f32, f32, f32), value: f32, text: String) -> Control {
    Control::Slider { value: value.clamp(min, max), min, max, step, text, note: None }
}

/// A choice among `options`, shown in the words under
/// `settings.options.{group}` in the language files.
fn choice(group: &str, options: &[&str], current: &str) -> Control {
    Control::Choice {
        options: options.iter().map(|o| t(&format!("settings.options.{group}.{o}")).to_string()).collect(),
        index: options.iter().position(|o| *o == current).unwrap_or(0),
    }
}

/// Setting `name`'s words under `settings` in the language files: its
/// "label", "hint" and so on.
fn words(name: &str, part: &str) -> &'static str {
    t(&format!("settings.{name}.{part}"))
}

/// A row labelled from `settings.{name}.label`.
fn row(id: SettingId, name: &str, hint: impl Into<String>, control: Control) -> Entry {
    Entry::Setting(Row { id, label: words(name, "label"), hint: hint.into(), control })
}

/// Theme details: an old machine's start screen, prompt, font, CRT and
/// colors, each a switch, then each sound it makes; for your own theme,
/// its sounds alone, its start screen and prompt being typed under Theme;
/// none for Flow's look.
fn theme_details_rows(cfg: &Config) -> Vec<Entry> {
    let in_use = crate::looks::InUse::of(cfg);
    if !in_use.machine() {
        return Vec::new();
    }
    let mut rows = Vec::new();
    if let crate::looks::InUse::Builtin(look) = in_use {
        rows.push(row(SettingId::ThemeStart, "theme_start", words("theme_start", "hint"), Control::Toggle(cfg.ui.look_start)));
        rows.push(row(SettingId::ThemePrompt, "theme_prompt", words("theme_prompt", "hint"), Control::Toggle(cfg.ui.look_prompt)));
        rows.push(row(SettingId::ThemeFont, "theme_font", words("theme_font", "hint"), Control::Toggle(cfg.ui.look_font)));
        // Its tube, where it was seen through one.
        if look.has_crt() {
            rows.push(row(SettingId::ThemeCrt, "theme_crt", words("theme_crt", "hint"), Control::Toggle(cfg.ui.look_crt)));
        }
        rows.push(row(SettingId::ThemeColors, "theme_colors", words("theme_colors", "hint"), Control::Toggle(cfg.ui.look_colors)));
        // Its shell window's frame, where its desktop drew one.
        if look.frame {
            rows.push(row(SettingId::ThemeFrame, "theme_frame", words("theme_frame", "hint"), Control::Toggle(cfg.ui.look_frame)));
        }
    }
    // A workstation's own numbers, where its start screen shows any.
    if in_use.shows_numbers() {
        rows.push(row(SettingId::ThemeRealNumbers, "theme_real_numbers", words("theme_real_numbers", "hint"), Control::Toggle(cfg.ui.look_real_numbers)));
    }
    for &part in in_use.sounds().parts() {
        let on = !cfg.sound.muted.contains(&part.key(in_use.id()));
        let label = t(&format!("settings.theme_sound.{}", part.id()));
        rows.push(Entry::Setting(Row { id: SettingId::ThemeSound(part), label, hint: words("theme_sound", "hint").into(), control: Control::Toggle(on) }));
    }
    rows
}

/// What a program may set `key` to, as Settings offers it (`flow-settings
/// --help`, and its `set` checked against it): one of a list of choices
/// (`{"one_of": [...]}`), a number in a slider's range (`{"from", "to",
/// "step"}`), or a color written `#RRGGBB` (`{"color": true}`, and
/// `"or_empty"` where empty is none); none where its kind says enough (a
/// switch, a name, a list).
pub fn takes(cfg: &Config, key: &str) -> Option<serde_json::Value> {
    use serde_json::json;
    let one_of = |choices: &[&str]| json!({ "one_of": choices });
    let range = |(from, to, step): (f32, f32, f32)| json!({ "from": from, "to": to, "step": step });
    Some(match key {
        "fx.style" => one_of(&STYLES),
        "fx.water_view" => one_of(&WATER_VIEWS),
        "fx.backdrop" => one_of(BACKDROPS),
        "fx.palette" => one_of(&PALETTES),
        "cursor.shape" => one_of(&CURSOR_SHAPES),
        "ui.logo" => one_of(&LOGOS),
        "sound.bell" => one_of(&BELLS),
        "shell.scrollbar" => one_of(&crate::scrollbar::MODES),
        "record.quality" => one_of(&crate::record::QUALITIES),
        "record.size" => one_of(&crate::record::SIZES),
        "record.countdown" => json!({ "one_of": crate::record::COUNTDOWNS }),
        "ui.language" => one_of(&languages()),
        // The built-in themes and your own, never a saved one's name.
        "ui.look" => json!({ "one_of": crate::looks::ids(cfg).into_iter().filter(|id| crate::looks::saved_name(id).is_none()).collect::<Vec<_>>() }),
        "window.hotkey" => one_of(crate::hotkey::CHOICES),
        "window.drop_down_height" => one_of(&crate::hotkey::HEIGHTS),
        "ui.notify_after" => json!({ "one_of": crate::notify::AFTER }),
        "window.refresh_rate" => {
            // The last 0 is vsync off, so the 0s aren't side by side.
            let mut rates: Vec<u32> = REFRESH_RATES.iter().map(|(rate, _)| *rate).collect();
            rates.sort_unstable();
            rates.dedup();
            json!({ "one_of": rates })
        }
        "fx.sun" => range(SUNLIGHT),
        "fx.electricity" => range(ELECTRICITY),
        "fx.speed" => range(FLOW_SPEED),
        "fx.ambient" => range(FLOW_AMOUNT),
        "fx.rain" => range(RAIN_AMOUNT),
        "fx.text_ripple" => range(TEXT_RIPPLE),
        "fx.caustics" => range(RIBBONS),
        "fx.opacity" => range(OPACITY),
        "fx.glow" => range(GLOW),
        "fx.crt" => range(CRT),
        "crt.scanlines" => range(SCANLINES),
        "crt.glow" => range(PHOSPHOR_GLOW),
        "crt.decay" => range(PHOSPHOR_DECAY),
        "crt.grille" => range(GRILLE),
        "crt.vignette" => range(VIGNETTE),
        "crt.curvature" => range(CURVATURE),
        "cursor.speed" => range(CURSOR_SPEED),
        "font.size" => range(FONT_SIZE),
        "sound.volume" => range(VOLUME),
        "colors.text" | "colors.cursor" | "colors.background" => json!({ "color": true }),
        "colors.border" | "colors.phosphor" => json!({ "color": true, "or_empty": true }),
        _ => return None,
    })
}

/// Whether `value` is what `takes` (from [`takes`]) allows: one of its
/// choices, a number in its range, or a color as `#RRGGBB`.
pub fn allows(takes: &serde_json::Value, value: &serde_json::Value) -> bool {
    if let Some(choices) = takes["one_of"].as_array() {
        return choices.contains(value);
    }
    if let (Some(from), Some(to)) = (takes["from"].as_f64(), takes["to"].as_f64()) {
        // Within a hair of the ends, as a slider's float steps may be.
        return value.as_f64().is_some_and(|v| v >= from - 1e-6 && v <= to + 1e-6);
    }
    if takes["color"] == serde_json::Value::Bool(true) {
        let text = value.as_str().unwrap_or_default();
        let hex = text.strip_prefix('#').is_some_and(|h| h.len() == 6 && h.chars().all(|c| c.is_ascii_hexdigit()));
        return hex || (text.is_empty() && takes["or_empty"] == serde_json::Value::Bool(true));
    }
    true
}

/// What the Language row offers: following the system, then each language.
pub fn languages() -> Vec<&'static str> {
    std::iter::once(i18n::AUTO).chain(i18n::codes()).collect()
}

fn percent(value: f32) -> String {
    format!("{:.0}%", value * 100.0)
}

/// How hard it rains, in words: from a drizzle through rain to a downpour,
/// or none.
pub fn rain_label(amount: f32) -> String {
    let how = match amount {
        a if a <= 0.0 => return t("common.off").to_string(),
        a if a < 0.3 => "drizzle",
        a if a < 0.55 => "rain",
        a if a < 0.8 => "heavy",
        _ => "downpour",
    };
    words("rain_amount", how).to_string()
}

pub fn speed_label(speed: f32) -> String {
    let feel = match speed {
        s if s < 0.35 => "very_slow",
        s if s < 0.75 => "slow",
        s if s < 1.25 => "normal",
        _ => "fast",
    };
    tf("settings.flow_speed.value", &[("speed", &format!("{speed:.1}")), ("feel", &words("flow_speed", feel))])
}

/// Labels for the Screen refresh choice, with a hand-set rate from the
/// config file added at the end when it isn't one of them.
pub fn refresh_options(rate: u32, vsync: bool) -> (Vec<String>, usize) {
    let label = |rate: u32, vsync: bool| match (rate, vsync) {
        (0, true) => words("screen_refresh", "display").to_string(),
        (0, false) => words("screen_refresh", "unlimited").to_string(),
        (rate, _) => tf("settings.screen_refresh.rate", &[("rate", &rate)]),
    };
    let mut labels: Vec<String> = REFRESH_RATES.iter().map(|&(r, v)| label(r, v)).collect();
    let index = match REFRESH_RATES.iter().position(|&choice| choice == (rate, vsync)) {
        Some(i) => i,
        None => {
            labels.push(label(rate, vsync));
            labels.len() - 1
        }
    };
    (labels, index)
}

/// Labels for the Hotkey choice, Off first, with one set otherwise (a key
/// pressed, or one written by hand in the config file, as it's written
/// there where Flow can't read it) added when it isn't one of them, and
/// Press a key… last, which is the one chosen while Settings waits for it
/// (`pressing`).
pub fn hotkey_options(hotkey: &str, pressing: bool) -> (Vec<String>, usize) {
    use crate::hotkey::{Hotkey, CHOICES};
    let label = |name: &str| match Hotkey::parse(name) {
        Some(key) => key.label(),
        None if name.trim().is_empty() => t("common.off").to_string(),
        None => name.to_string(),
    };
    let mut labels: Vec<String> = CHOICES.iter().map(|c| label(c)).collect();
    let set = Hotkey::parse(hotkey);
    let index = match CHOICES.iter().position(|c| Hotkey::parse(c) == set && (set.is_some() || hotkey.trim().is_empty())) {
        Some(i) => i,
        None => {
            labels.push(label(hotkey));
            labels.len() - 1
        }
    };
    labels.push(words("hotkey", "press").to_string());
    let index = if pressing { labels.len() - 1 } else { index };
    (labels, index)
}

/// Settings waits for the key that's to be the hotkey (the Hotkey row's
/// Press a key…), which the row's hint asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pressing {
    Waiting,
    /// A key Flow can't take was pressed, which the hint says.
    Refused,
    /// The system's key (⌘Q, ⌘Tab, Alt+F4) was pressed, which a hotkey
    /// would take from every app.
    System,
}

/// Labels for the Notify after choice, Off first, with a time set by hand
/// in the config file added at the end when it isn't one of them.
pub fn notify_options(after: u32) -> (Vec<String>, usize) {
    seconds_options(&crate::notify::AFTER, after)
}

/// Labels for a choice of seconds (`choices`, 0 as Off), with one set by
/// hand in the config added at the end where it isn't among them, and
/// which is `value`.
pub fn seconds_options(choices: &[u32], value: u32) -> (Vec<String>, usize) {
    let label = |secs: u32| if secs == 0 { t("common.off").to_string() } else { crate::notify::took(secs as f32) };
    let mut labels: Vec<String> = choices.iter().map(|&a| label(a)).collect();
    let index = match choices.iter().position(|&a| a == value) {
        Some(i) => i,
        None => {
            labels.push(label(value));
            labels.len() - 1
        }
    };
    (labels, index)
}

/// Labels for the token cap choice, with a hand-set cap from the config
/// file added at the end when it isn't one of the presets.
/// Where the assistant's commands run (`ai.pane`).
pub const AI_PANES: [&str; 2] = ["yours", "own"];

/// The minutes Settings offers the assistant to work on its own.
pub const OWN_MINUTES: [u32; 5] = [15, 30, 60, 120, 240];
/// And the tokens.
pub const OWN_TOKENS: [u64; 5] = [100_000, 250_000, 500_000, 1_000_000, 2_000_000];

/// Minutes as the interface says them: "45 min", "2 h", "1 h 30 min".
pub fn fmt_minutes(minutes: u64) -> String {
    match (minutes / 60, minutes % 60) {
        (0, m) => tf("own.minutes", &[("n", &m)]),
        (h, 0) => tf("own.hours", &[("n", &h)]),
        (h, m) => tf("own.hours_minutes", &[("h", &h), ("m", &m)]),
    }
}

/// A choice of `values`, each named by `label`, chosen where it's `value`;
/// one set by hand in the config, past them, stays as it is.
fn number_choice<T: PartialEq + Copy>(values: &[T], value: T, label: impl Fn(T) -> String) -> Control {
    let mut options: Vec<String> = values.iter().map(|&v| label(v)).collect();
    let index = values.iter().position(|&v| v == value).unwrap_or_else(|| {
        options.push(label(value));
        options.len() - 1
    });
    Control::Choice { options: options.into(), index }
}

pub fn token_cap_options(cap: u64) -> (Vec<String>, usize) {
    let mut labels: Vec<String> =
        TOKEN_CAPS.iter().map(|&c| if c == 0 { t("common.off").to_string() } else { fmt_tokens(c) }).collect();
    let index = match TOKEN_CAPS.iter().position(|&c| c == cap) {
        Some(i) => i,
        None => {
            labels.push(fmt_tokens(cap));
            labels.len() - 1
        }
    };
    (labels, index)
}

/// Labels for the Typed at choice, with a speed set by hand in the config
/// file added at the end when it isn't one of them.
pub fn baud_options(baud: u32) -> (Vec<String>, usize) {
    let label = |baud: u32| if baud == 0 { words("own_baud", "now").to_string() } else { baud.to_string() };
    let mut labels: Vec<String> = BAUDS.iter().map(|&b| label(b)).collect();
    let index = match BAUDS.iter().position(|&b| b == baud) {
        Some(i) => i,
        None => {
            labels.push(label(baud));
            labels.len() - 1
        }
    };
    (labels, index)
}

/// What the menu counts: the tokens used, since when (seconds since 1970),
/// and the commands the history keeps.
#[derive(Debug, Clone, Default)]
pub struct Tally {
    pub usage: Usage,
    pub since: u64,
    pub history_kept: usize,
    /// Flow checks for updates daily (Linux), as `updates.json` says.
    pub update_checks: bool,
    /// Efficient mode is on because the computer runs on battery or saves
    /// power, though its own switch is off.
    pub efficient_for: Option<crate::power::Power>,
    /// Settings is on a tab, whose whole look its look's sections change,
    /// rather than on new tabs'.
    pub on_tab: bool,
    /// That tab is split: the look's sections change its pane in front.
    pub on_pane: bool,
    /// The hotkey set couldn't be taken (`Platform::register_hotkey`).
    pub hotkey_failed: bool,
    /// No hotkey can be had here (`Platform::hotkey_possible`: Wayland,
    /// XWayland), which its row says in place of the choices.
    pub no_hotkey: bool,
    /// The window can't be kept on top here (`Platform::on_top_possible`:
    /// Wayland), which its row says in place of the switch.
    pub no_on_top: bool,
    /// flowapp's apps and packages of themes, as Settings' Apps lists them,
    /// worked out when Settings opened or something changed, not each frame.
    pub apps: std::rc::Rc<[crate::apps::shelf::Line]>,
    /// How long ago the catalog they're from was read: none where it never
    /// was.
    pub catalog_age: Option<std::time::Duration>,
    /// The catalog is being read now.
    pub apps_checking: bool,
    /// Settings waits for the hotkey to be pressed.
    pub hotkey_pressing: Option<Pressing>,
    /// Settings waits for a key to bind one of Flow's keys to.
    pub key_wait: Option<crate::keys::Wait>,
    /// How many facts and tasks the assistant's memory keeps.
    pub memory_kept: (usize, usize),
    /// How many lines the user's instructions have.
    pub instruction_lines: usize,
    /// The user's skills and the installed packages', each with its switch
    /// (`ai.skills_off`, `ai.skills_on`), worked out as `apps` is.
    pub skills: std::rc::Rc<SkillsView>,
    /// The outside apps remembered, and how to connect one.
    pub outside: std::rc::Rc<OutsideView>,
    /// The assistant's MCP servers, how each stands and its tools.
    pub servers: std::rc::Rc<ServersView>,
}

/// Settings' MCP servers: each server in `mcp-servers.json`, how it stands
/// and, once connected, its tools, worked out when Settings opens and as
/// they change, not each frame.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ServersView {
    pub servers: Vec<ServerRow>,
    /// What in the file didn't read.
    pub problems: Vec<String>,
    /// Where the file is.
    pub file: String,
}

/// A server as Settings shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct ServerRow {
    pub name: String,
    /// Its command or URL as it's typed (`How::line`); empty for one just
    /// added, not yet in the file.
    pub line: String,
    pub remote: bool,
    /// Switched on (`ai.mcp_servers_on`).
    pub on: bool,
    pub standing: ServerStanding,
    pub tools: Vec<ServerToolRow>,
    /// How many resources and templates it offers to be read, and prompts.
    pub resources: usize,
    pub prompts: Vec<String>,
    /// Connected with a sign-in of Flow's (OAuth), which Sign out forgets.
    pub signed_in: bool,
    /// The client ID the user typed for its sign-in, where they did.
    pub client_id: Option<String>,
}

/// How a server stands.
#[derive(Debug, Clone, PartialEq)]
pub enum ServerStanding {
    Off,
    Connecting,
    Connected,
    /// It couldn't be reached, and why.
    Failed(String),
    /// It needs the user to sign in.
    SignIn,
    /// Signing in, in the browser.
    SigningIn,
}

/// One of a connected server's tools.
#[derive(Debug, Clone, PartialEq)]
pub struct ServerToolRow {
    pub name: String,
    pub description: String,
    pub read_only: bool,
}

impl ServersView {
    /// Two servers made up, one connected with its tools, for Settings'
    /// picture of them.
    pub fn made_up() -> Self {
        let tool = |name: &str, description: &str, read_only| ServerToolRow { name: name.into(), description: description.into(), read_only };
        ServersView {
            servers: vec![
                ServerRow {
                    name: "notes".into(),
                    line: "npx -y @example/notes-mcp".into(),
                    remote: false,
                    on: true,
                    standing: ServerStanding::Connected,
                    tools: vec![
                        tool("find_notes", "Finds notes by their words.", true),
                        tool("read_note", "Reads a note whole.", true),
                        tool("add_note", "Adds a note to a notebook.", false),
                    ],
                    resources: 12,
                    prompts: vec!["summarize".into()],
                    signed_in: false,
                    client_id: None,
                },
                ServerRow {
                    name: "tracker".into(),
                    line: "https://tracker.example.com/mcp".into(),
                    remote: true,
                    on: true,
                    standing: ServerStanding::SignIn,
                    tools: Vec::new(),
                    resources: 0,
                    prompts: Vec::new(),
                    signed_in: false,
                    client_id: None,
                },
            ],
            problems: Vec::new(),
            file: "~/Library/Application Support/Flow/mcp-servers.json".into(),
        }
    }
}

/// How a server's calls may be approved, as Settings offers them
/// (`mcp::client::servers::Approval`'s keys).
pub const MCP_APPROVALS: [&str; 3] = ["ask", "read_only", "allowed"];

/// A button of a server's row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerAct {
    Connect,
    Disconnect,
    Remove,
    SignIn,
    CancelSignIn,
    SignOut,
    ClientId,
    Token,
}

impl ServerAct {
    pub fn label(self) -> &'static str {
        words(
            "mcp_server",
            match self {
                ServerAct::Connect => "connect",
                ServerAct::Disconnect => "disconnect",
                ServerAct::Remove => "remove",
                ServerAct::SignIn => "sign_in",
                ServerAct::CancelSignIn => "cancel_sign_in",
                ServerAct::SignOut => "sign_out",
                ServerAct::ClientId => "client_id",
                ServerAct::Token => "token",
            },
        )
    }
}

/// A server's buttons, in their order: Connect (Disconnect while it's
/// on), and Remove. One just added, with no command or URL yet, has only
/// Remove.
pub fn server_acts(server: &ServerRow) -> Vec<ServerAct> {
    let mut acts = Vec::new();
    if !server.line.is_empty() {
        acts.push(if server.on { ServerAct::Disconnect } else { ServerAct::Connect });
    }
    acts.push(ServerAct::Remove);
    acts
}

/// A remote server's sign-in buttons: Sign in (Cancel while it runs, Sign
/// out once signed in), Client ID… and Token….
pub fn server_auth_acts(server: &ServerRow) -> Vec<ServerAct> {
    let first = match server.standing {
        ServerStanding::SigningIn => ServerAct::CancelSignIn,
        _ if server.signed_in => ServerAct::SignOut,
        _ => ServerAct::SignIn,
    };
    vec![first, ServerAct::ClientId, ServerAct::Token]
}

/// Settings' Outside apps: what each app remembered may do (`mcp.json`),
/// which are connected now, and `flow-mcp`'s path, where it's found.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OutsideView {
    pub apps: Vec<crate::mcp::grants::Grant>,
    /// Flow listens now.
    pub listening: bool,
    pub flow_mcp: Option<std::path::PathBuf>,
}

impl OutsideView {
    /// Two apps made up, for Settings' picture of them.
    pub fn made_up() -> Self {
        use crate::mcp::grants::{Commands, Grant};
        OutsideView {
            apps: vec![
                Grant { name: "claude-code".into(), path: "/opt/homebrew/bin/claude".into(), read: true, tabs: true, settings: false, commands: Commands::Ask },
                Grant { name: "Claude".into(), path: "/Applications/Claude.app/Contents/MacOS/Claude".into(), read: true, tabs: false, settings: true, commands: Commands::Off },
            ],
            listening: true,
            flow_mcp: Some("/Applications/Flow.app/Contents/Helpers/flow-mcp".into()),
        }
    }
}

/// How an app may be allowed to run commands, as Settings offers them.
pub const MCP_COMMANDS: [&str; 3] = ["off", "ask", "allowed"];

/// A skill as Settings' Skills section shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct SkillRow {
    /// As the assistant knows it: its own name, or `<package>:<name>`.
    pub name: String,
    pub description: String,
    /// The package it came in; none for the user's own.
    pub from: Option<String>,
}

/// The user's own skills and each installed package's, for Settings'
/// Skills section, worked out when Settings opens, not each frame.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SkillsView {
    /// The user's own first, then each package's, package by package.
    pub skills: Vec<SkillRow>,
    /// The installed packages that bring skills that read, by name.
    pub packages: Vec<String>,
}

impl SkillsView {
    /// The skills in the user's folder `dir` and in each package installed
    /// under `root` (`apps::skill_dirs`), whether they're on or off.
    pub fn read(dir: Option<&std::path::Path>, root: &std::path::Path) -> Self {
        use crate::ai::skills;
        let mut view = SkillsView::default();
        if let Some(dir) = dir {
            view.skills.extend(skills::list(dir).into_iter().map(|s| SkillRow { name: s.name, description: s.description, from: None }));
        }
        for (package, folder) in crate::apps::skill_dirs(root) {
            let found = skills::list(&folder);
            if found.is_empty() {
                continue;
            }
            view.skills.extend(found.into_iter().map(|s| SkillRow { name: format!("{package}:{}", s.name), description: s.description, from: Some(package.clone()) }));
            view.packages.push(package);
        }
        view
    }

    /// The skills of `package`, with their places.
    pub fn of<'a>(&'a self, package: &'a str) -> impl Iterator<Item = (usize, &'a SkillRow)> {
        self.skills.iter().enumerate().filter(move |(_, s)| s.from.as_deref() == Some(package))
    }

    /// Made up for Settings' picture of them: four of the user's own, and
    /// two packages.
    pub fn made_up() -> Self {
        let row = |from: Option<&str>, name: &str, description: &str| SkillRow {
            name: from.map_or(name.to_string(), |p| format!("{p}:{name}")),
            description: description.into(),
            from: from.map(String::from),
        };
        SkillsView {
            skills: vec![
                row(None, "deploy-site", "Deploying the site: build, check the links, then copy it up."),
                row(None, "fix-wifi", "Bringing the Wi-Fi back after the laptop sleeps."),
                row(None, "release-notes", "Writing a release's notes from its commits, in the project's own words."),
                row(None, "windows-drivers", "Checking and updating drivers on Windows, a restore point first."),
                row(Some("flow-skills"), "flow-settings", "Seeing and changing Flow's settings from a shell or a script with flow-settings."),
                row(Some("flow-skills"), "flowapp", "Using flowapp: installing apps, putting a program first, making a package of skills."),
                row(Some("flow-skills"), "sdk-programs", "Writing a program that draws in Flow: effects, pictures and 3D models, with Flow's SDK."),
                row(Some("flow-skills"), "theme-package", "Making a package of themes for Flow, packing it and installing it with flowapp."),
                row(Some("flow-utils"), "tree", "Showing a folder as a tree with Flow-Utils' tree."),
            ],
            packages: vec!["flow-skills".into(), "flow-utils".into()],
        }
    }
}

/// The menu's rows for the current config. `grid` is what the window
/// holds at the text size in use, where it's known.
pub fn entries(cfg: &Config, fonts: &FontChoices, grid: Option<TextGrid>, profiles: &[ShellProfile], auto_mode: bool, has_key: bool, tally: Tally) -> Vec<Entry> {
    let Tally { usage, since, history_kept, update_checks, efficient_for, on_tab, on_pane, hotkey_failed, no_hotkey, no_on_top, hotkey_pressing, apps, catalog_age, apps_checking, key_wait, memory_kept, instruction_lines, skills, outside, servers } = tally;
    let fx = &cfg.fx;
    let shell_index = profiles.iter().position(|p| p.id == cfg.shell.default).unwrap_or(0);
    let clouds = fx.style == "clouds" || !STYLES.contains(&fx.style.as_str());
    let water = fx.style == "water";
    let rain = fx.style == "rain";
    // With no liquid, nothing flows.
    let none = fx.style == "none";
    let languages = languages();
    let language_names = languages
        .iter()
        .map(|&code| if code == i18n::AUTO { words("language", "auto") } else { i18n::native_name(code) }.to_string())
        .collect();
    let mut general = vec![
        // On for the battery, it shows on, and says why.
        row(
            SettingId::Efficient,
            "efficient",
            words(
                "efficient",
                match efficient_for {
                    Some(crate::power::Power::Battery) => "hint_battery",
                    Some(_) => keys("hint_low_power_mac", "hint_low_power"),
                    None => "hint",
                },
            ),
            Control::Toggle(cfg.ui.efficient || efficient_for.is_some()),
        ),
        row(SettingId::EfficientOnBattery, "efficient_on_battery", words("efficient_on_battery", per_os("hint_mac", "hint_windows", "hint")), Control::Toggle(cfg.ui.efficient_on_battery)),
        row(
            SettingId::Language,
            "language",
            words("language", "hint"),
            Control::Choice { options: language_names, index: languages.iter().position(|c| *c == cfg.ui.language).unwrap_or(0) },
        ),
        row(SettingId::ScreenRefresh, "screen_refresh", words("screen_refresh", "hint"), {
            let (options, index) = refresh_options(cfg.window.refresh_rate, cfg.window.vsync);
            Control::Choice { options: options.into(), index }
        }),
    ];
    general.push(on_top_row(cfg, no_on_top));
    general.extend(hotkey_rows(cfg, hotkey_failed, hotkey_pressing, no_hotkey));
    let mut look = vec![
        row(SettingId::Theme, "theme", words("theme", "hint"), {
            // The built-in looks, the user's own, then the ones they saved.
            let looks = crate::looks::ids(cfg);
            let options: Vec<String> = looks.iter().map(|id| crate::looks::name(id)).collect();
            Control::Choice { options: options.into(), index: looks.iter().position(|id| *id == cfg.ui.look).unwrap_or(0) }
        }),
    ];
    // Your own theme's start screen and prompt are typed here, and with
    // nothing typed there's none, so they need no switches; a saved theme
    // chosen has its own here, which they change, leaving your own be.
    if let crate::looks::InUse::Own(_, theme) = crate::looks::InUse::of(cfg) {
        look.extend(own_rows(cfg, theme));
    }
    look.push(saved_theme_row(cfg));
    look.push(row(SettingId::ThemeGallery, "theme_gallery", words("theme_gallery", "hint"), Control::Actions(vec![words("theme_gallery", "browse")])));
    look.push(row(SettingId::CursorSpeed, "cursor_speed", words("cursor_speed", "hint"), slider(CURSOR_SPEED, cfg.cursor.speed, speed_label(cfg.cursor.speed))));
    let mut liquid = vec![row(SettingId::LiquidStyle, "liquid_style", words("liquid_style", "hint"), choice("style", &STYLES, &fx.style))];
    if water {
        liquid.push(row(SettingId::WaterView, "water_view", words("water_view", "hint"), choice("water_view", &WATER_VIEWS, &fx.water_view)));
        liquid.push(row(SettingId::Sunlight, "sunlight", words("sunlight", "hint"), slider(SUNLIGHT, fx.sun, percent(fx.sun))));
    }
    // The clouds' lightning, or the rain's storm outside.
    if clouds || rain {
        let text = if fx.electricity <= 0.0 { t("common.off").to_string() } else { percent(fx.electricity) };
        let hint = words("electricity", if rain { "hint_rain" } else { "hint" });
        liquid.push(row(SettingId::Electricity, "electricity", hint, slider(ELECTRICITY, fx.electricity, text)));
    }
    if !none {
        liquid.push(row(SettingId::FlowSpeed, "flow_speed", words("flow_speed", "hint"), slider(FLOW_SPEED, fx.speed, speed_label(fx.speed))));
        if rain {
            liquid.push(row(SettingId::RainAmount, "rain_amount", words("rain_amount", "hint"), slider(RAIN_AMOUNT, fx.rain, rain_label(fx.rain))));
        } else {
            liquid.push(row(SettingId::FlowAmount, "flow_amount", words("flow_amount", "hint"), slider(FLOW_AMOUNT, fx.ambient, percent(fx.ambient))));
        }
    }
    if clouds {
        liquid.push(row(SettingId::Ribbons, "ribbons", words("ribbons", "hint"), slider(RIBBONS, fx.caustics, percent(fx.caustics))));
    }
    if clouds || rain {
        let hint = words("palette", if rain { "hint_rain" } else { "hint" });
        liquid.push(row(SettingId::Palette, "palette", hint, choice("palette", &PALETTES, &fx.palette)));
    }
    if !none {
        liquid.push(row(
            SettingId::TextRipple,
            "text_ripple",
            words("text_ripple", "hint"),
            slider(TEXT_RIPPLE, fx.text_ripple, format!("{:.1} px", fx.text_ripple)),
        ));
    }
    if !none {
        liquid.push(row(SettingId::Liquid, "liquid", words("liquid", "hint"), Control::Toggle(fx.liquid)));
    }
    liquid.push(row(SettingId::Reactions, "reactions", words("reactions", "hint"), Control::Toggle(fx.reactions)));
    if !none {
        liquid.push(row(SettingId::ProgramEffects, "program_effects", words("program_effects", "hint"), Control::Toggle(fx.program_effects)));
    }
    let mut text = vec![
        row(
            SettingId::Font,
            "font",
            words("font", "hint"),
            Control::Choice { options: fonts.families.clone(), index: fonts.current },
        ),
        row(SettingId::FontSize, "font_size", tf("settings.font_size.hint", &[("bigger", &crate::keys::label(Command::BiggerText)), ("smaller", &crate::keys::label(Command::SmallerText))]), {
            let mut size = slider(FONT_SIZE, cfg.font.size, format!("{:.0} pt", cfg.font.size));
            // What the window holds at it, and each character's size.
            if let Control::Slider { note, .. } = &mut size {
                *note = grid.map(|g| g.describe());
            }
            size
        }),
        fit_row(cfg, grid),
        fit_text_row(cfg, grid),
        row(
            SettingId::OwnSizes,
            "own_sizes",
            words("own_sizes", "hint"),
            Control::Text { value: row_text(cfg, SettingId::OwnSizes), multiline: false, empty: words("own_sizes", "empty") },
        ),
    ];
    let ink = cfg.colors.ink();
    let colors = |presets: &[(&str, Rgb8)], current| {
        let (options, index) = color_options(presets, current);
        Control::Colors { options, index, none: false }
    };
    // Off, then the phosphors, and any other picked.
    let phosphor = {
        let (mut options, index) = color_options(&PHOSPHORS, ink.phosphor.unwrap_or(PHOSPHORS[0].1));
        options.insert(0, (t("common.off").to_string(), ink.background));
        Control::Colors { options, index: if ink.phosphor.is_some() { index + 1 } else { 0 }, none: true }
    };
    let glow = if fx.glow <= 0.0 { t("common.off").to_string() } else { percent(fx.glow) };
    text.extend([
        row(SettingId::TextColor, "text_color", words("text_color", "hint"), colors(&COLORS, ink.text)),
        row(SettingId::Phosphor, "phosphor", words("phosphor", "hint"), phosphor),
        row(SettingId::CursorColor, "cursor_color", words("cursor_color", "hint"), colors(&COLORS, ink.cursor)),
        row(SettingId::CursorShape, "cursor_shape", words("cursor_shape", "hint"), choice("cursor_shape", &CURSOR_SHAPES, &cfg.cursor.shape)),
        row(SettingId::CursorBlink, "cursor_blink", words("cursor_blink", "hint"), Control::Toggle(cfg.cursor.blink)),
        row(SettingId::CursorGlide, "cursor_glide", words("cursor_glide", "hint"), Control::Toggle(fx.cursor_glide)),
    ]);
    let glass = vec![
        row(SettingId::Opacity, "opacity", words("opacity", "hint"), slider(OPACITY, fx.opacity, percent(fx.opacity))),
        row(SettingId::Backdrop, "backdrop", words("backdrop", per_os("hint_mac", "hint", "hint_linux")), choice("backdrop", BACKDROPS, &fx.backdrop)),
        row(SettingId::BackgroundColor, "background", words("background", "hint"), colors(&BACKGROUNDS, ink.background)),
        row(SettingId::Glow, "glow", words("glow", "hint"), slider(GLOW, fx.glow, glow)),
    ];
    look.extend([
        row(SettingId::StartupLogo, "startup_logo", words("startup_logo", "hint"), {
            let label = |logo: &str| match logo {
                "system" => words("startup_logo", per_os("apple", "windows", "linux")).to_string(),
                "chip" => words("startup_logo", "chip").to_string(),
                "off" => t("common.off").to_string(),
                _ => words("startup_logo", "flow").to_string(),
            };
            let index = LOGOS.iter().position(|l| *l == cfg.ui.logo).unwrap_or(0);
            Control::Choice { options: LOGOS.iter().map(|l| label(l)).collect(), index }
        }),
        row(
            SettingId::PresetFile,
            "preset_file",
            words("preset_file", "hint"),
            Control::Actions(vec![words("preset_file", "save"), words("preset_file", "load")]),
        ),
    ]);
    let mut terminal = vec![row(
        SettingId::DefaultShell,
        "default_shell",
        tf("settings.default_shell.hint", &[("keys", &crate::keys::label(Command::NewTab))]),
        Control::Choice { options: profiles.iter().map(|p| p.label.clone()).collect(), index: shell_index },
    )];
    terminal.push(row(SettingId::ReopenTabs, "reopen_tabs", words("reopen_tabs", "hint"), Control::Toggle(cfg.shell.reopen_tabs)));
    if cfg!(target_os = "macos") {
        terminal.push(row(SettingId::OptionAsMeta, "option_as_meta", words("option_as_meta", "hint"), Control::Toggle(cfg.shell.option_as_meta)));
    }
    terminal.push(row(SettingId::MouseReporting, "mouse_reporting", words("mouse_reporting", "hint"), Control::Toggle(cfg.shell.mouse_reporting)));
    terminal.push(row(SettingId::ClickMovesCursor, "click_moves_cursor", words("click_moves_cursor", "hint"), Control::Toggle(cfg.shell.click_moves_cursor)));
    terminal.push(row(SettingId::Scrollbar, "scrollbar", words("scrollbar", "hint"), choice("scrollbar", &crate::scrollbar::MODES, &cfg.shell.scrollbar)));
    terminal.push(row(SettingId::NotifyAfter, "notify_after", words("notify_after", per_os("hint_mac", "hint_windows", "hint")), {
        let (options, index) = notify_options(cfg.ui.notify_after);
        Control::Choice { options: options.into(), index }
    }));
    let history_hint = tf(&format!("settings.history.{}", keys("hint_mac", "hint")), &[("keys", &crate::keys::label(Command::SearchHistory))]);
    terminal.push(row(SettingId::History, "history", history_hint, Control::Toggle(cfg.history.enabled)));
    terminal.push(row(
        SettingId::HistoryKept,
        "history_kept",
        words("history_kept", "hint"),
        Control::Readout { text: tf("settings.history_kept.value", &[("n", &history_kept.to_string())]), action: words("history_kept", "clear") },
    ));
    let recording = vec![
        row(SettingId::RecordQuality, "record_quality", words("record_quality", "hint"), choice("record_quality", &crate::record::QUALITIES, &cfg.record.quality)),
        row(SettingId::RecordSize, "record_size", words("record_size", "hint"), choice("record_size", &crate::record::SIZES, &cfg.record.size)),
        row(SettingId::RecordCountdown, "record_countdown", words("record_countdown", "hint"), {
            let (options, index) = seconds_options(&crate::record::COUNTDOWNS, cfg.record.countdown);
            Control::Choice { options: options.into(), index }
        }),
    ];
    let mut about = vec![
        Entry::Setting(Row {
            id: SettingId::About,
            label: crate::APP_NAME,
            hint: tf(per_os("settings.about.hint", "settings.about.hint", "settings.about.hint_linux"), &[("version", &crate::version::display())]),
            control: Control::Actions(vec![words("about", "about"), words("about", "check_updates")]),
        }),
        row(SettingId::Restart, "restart", words("restart", "hint"), Control::Actions(vec![words("restart", "button")])),
    ];
    // Sparkle and WinSparkle keep this setting themselves; on Linux, Flow does.
    if !cfg!(any(target_os = "macos", windows)) {
        about.push(row(SettingId::UpdateChecks, "update_checks", words("update_checks", "hint"), Control::Toggle(update_checks)));
    }
    let sections = [
        (Section::General, general),
        (Section::Theme, look),
        (Section::ThemeDetails, theme_details_rows(cfg)),
        (Section::Liquid, liquid),
        (Section::Glass, glass),
        (Section::Text, text),
        (Section::Crt, crt_rows(cfg)),
        (Section::Assistant, assistant_rows(cfg, auto_mode, has_key, usage, since, memory_kept, instruction_lines)),
        (Section::Skills, skills_rows(cfg, &skills)),
        (Section::OutsideApps, outside_rows(cfg, &outside)),
        (Section::McpServers, servers_rows(cfg, &servers)),
        (Section::Terminal, terminal),
        (Section::Keys, key_rows(key_wait.as_ref())),
        (Section::Recording, recording),
        (Section::Apps, apps_rows(cfg, &apps, catalog_age, apps_checking)),
        (Section::Sound, sound_rows(cfg)),
        (Section::About, about),
    ];
    // Each section's header, then its rows while it's open.
    let mut entries = Vec::new();
    // A section with nothing to set (Theme details under Flow's own look)
    // isn't shown.
    for (section, rows) in sections.into_iter().filter(|(_, rows)| !rows.is_empty()) {
        let open = section.open(cfg);
        // The look's sections are the tab's, or new tabs'; the rest are Flow's.
        let scope = match section.follows_tab() {
            false => t("settings.scope.all"),
            true if on_pane => t("settings.scope.pane"),
            true if on_tab => t("settings.scope.tab"),
            true => t("settings.scope.new_tabs"),
        };
        let control = Control::Section { open, count: rows.len(), scope };
        entries.push(Entry::Header(Row { id: SettingId::Section(section), label: section.title(), hint: t("settings.section.hint").into(), control }));
        if open {
            entries.extend(rows);
        }
    }
    entries
}

/// The key in config.toml a row sets, where it sets one, as the manual
/// writes it (`fx.crt`): what Settings' search finds it by too. None for a
/// row that only does something (Restart, Check now) or keeps its value
/// elsewhere (the API key, updates.json).
pub fn config_key(id: SettingId) -> Option<String> {
    use SettingId::*;
    let key = match id {
        Key(i) => return crate::keys::TABLE.get(i as usize).map(|e| format!("keys.{}", e.name)),
        KeysReset => "keys",
        Theme => "ui.look",
        ThemeStart => "ui.look_start",
        ThemePrompt => "ui.look_prompt",
        OwnStart => "theme.start",
        OwnPrompt => "theme.prompt",
        OwnBaud => "theme.baud",
        OwnSounds => "theme.sounds",
        OwnFrame => "theme.frame",
        SavedTheme => "themes.saved",
        LiquidStyle => "fx.style",
        WaterView => "fx.water_view",
        Sunlight => "fx.sun",
        Electricity => "fx.electricity",
        FlowSpeed => "fx.speed",
        FlowAmount => "fx.ambient",
        RainAmount => "fx.rain",
        Ribbons => "fx.caustics",
        TextRipple => "fx.text_ripple",
        Opacity => "fx.opacity",
        Liquid => "fx.liquid",
        Reactions => "fx.reactions",
        ProgramEffects => "fx.program_effects",
        Backdrop => "fx.backdrop",
        Palette => "fx.palette",
        Font => "font.families",
        FontSize => "font.size",
        OwnSizes => "window.sizes",
        Language => "ui.language",
        ScreenRefresh => "window.refresh_rate",
        OnTop => "window.on_top",
        Hotkey => "window.hotkey",
        DropDown => "window.drop_down",
        DropDownHeight => "window.drop_down_height",
        DropDownSlide => "window.drop_down_slide",
        DropDownHides => "window.drop_down_hides",
        DropDownOverFullScreen => "window.drop_down_over_full_screen",
        TextColor => "colors.text",
        Phosphor => "colors.phosphor",
        CursorColor => "colors.cursor",
        CursorShape => "cursor.shape",
        CursorBlink => "cursor.blink",
        CursorGlide => "fx.cursor_glide",
        CursorSpeed => "cursor.speed",
        Efficient => "ui.efficient",
        EfficientOnBattery => "ui.efficient_on_battery",
        ThemeFont => "ui.look_font",
        ThemeCrt => "ui.look_crt",
        ThemeColors => "ui.look_colors",
        ThemeFrame => "ui.look_frame",
        ThemeRealNumbers => "ui.look_real_numbers",
        ThemeSound(_) => "sound.muted",
        BackgroundColor => "colors.background",
        Glow => "fx.glow",
        StartupLogo => "ui.logo",
        Crt => "fx.crt",
        Scanlines => "crt.scanlines",
        Interlace => "crt.interlace",
        PhosphorGlow => "crt.glow",
        PhosphorDecay => "crt.decay",
        Grille => "crt.grille",
        Vignette => "crt.vignette",
        Curvature => "crt.curvature",
        CrtSaved => "crt.presets",
        Provider => "ai.provider",
        Model => "ai.model",
        Effort => "ai.effort",
        Mode => "ai.auto_mode",
        WebSearch => "ai.web_search",
        AiPane => "ai.pane",
        Helpers => "ai.helpers",
        HelperModel => "ai.helper_model",
        Memory => "ai.memory",
        Own => "ai.own",
        OwnMinutes => "ai.own_minutes",
        OwnTokens => "ai.own_tokens",
        TokenCap => "ai.token_cap",
        DefaultShell => "shell.default",
        ReopenTabs => "shell.reopen_tabs",
        OptionAsMeta => "shell.option_as_meta",
        MouseReporting => "shell.mouse_reporting",
        ClickMovesCursor => "shell.click_moves_cursor",
        Scrollbar => "shell.scrollbar",
        NotifyAfter => "ui.notify_after",
        RecordQuality => "record.quality",
        RecordSize => "record.size",
        RecordCountdown => "record.countdown",
        // Put together, so the words' test doesn't take it for one of
        // `history.*`'s words.
        History => concat!("history", ".enabled"),
        Sound => "sound.enabled",
        SoundVolume => "sound.volume",
        Bell => "sound.bell",
        SoundCues => "sound.cues",
        EffectSounds => "sound.effects",
        ProgramSounds => "sound.programs",
        LiquidSounds => "sound.liquid",
        MachineSounds => "sound.machines",
        BellFlash => "sound.bell_flash",
        AppsAtStart => "apps.check_at_start",
        AppsCheck => "apps.check",
        SkillPackage(_) | PackageSkill(_) => "ai.skills_on",
        McpEnabled => "mcp.enabled",
        McpConnect | McpApp(_) | McpKind(..) | McpCommands(_) => return None,
        Server(_) => "ai.mcp_servers_on",
        ServerApproval(_) => "ai.mcp_approval",
        ServerTools(_) | ServerTool(..) => "ai.mcp_tools_on",
        ServerResources(_) => "ai.mcp_resources_on",
        // Kept in mcp-servers.json and the credential store, not the config.
        ServersFile | ServerHow(_) | ServerAuth(_) => return None,
        Skill(_) => "ai.skills_off",
        Section(_) | ThemeGallery | FitScreen | FitText | PresetFile | CrtPreset | TokenUsage | ApiKey | MemoryKept | Instructions | Skills | HistoryKept | About | Restart | UpdateChecks | AppsCatalog | App(_) => return None,
    };
    Some(key.to_string())
}

/// The version whose Settings first has its Keys section.
const KEYS_SINCE: &str = "0.1.39";

/// Whether this is the first start, over a Flow from before Settings had
/// its Keys section, of one that has it: the version that ran last
/// (`updates.json`'s `ran`) older than both 0.1.39 and `this` (so a build
/// still called 0.1.38 over itself isn't), or none written where a config
/// was there already (a Flow before 0.1.35).
pub fn first_start_since_keys(ran: &str, this: &str, older_flow: bool) -> bool {
    if ran.trim().is_empty() { older_flow } else { crate::apps::newer(KEYS_SINCE, ran) && crate::apps::newer(this, ran) }
}

/// Closes Settings' Keys in a config from before it ([`first_start_since_keys`]),
/// as a new config has it: one from before lists the sections closed by
/// their names, without Keys, which would otherwise open with its forty
/// rows. True where it changed.
pub fn close_keys_section(config: &mut Config) -> bool {
    let open = Section::Keys.open(config);
    if open {
        Section::Keys.set_open(config, false);
    }
    open
}

/// What a key's row in Settings' Keys offers, as its buttons say, the
/// first what Enter does: Change, Add Key (a command's, for another key
/// beside those it has) and, once it's changed, Reset; while it waits,
/// Cancel; asked about another's key, Swap (two commands), Take It and
/// Keep Mine. Quit's on the Mac offers none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAct {
    Change,
    Add,
    Reset,
    Cancel,
    Swap,
    Take,
    Keep,
}

impl KeyAct {
    pub fn label(self) -> &'static str {
        t(match self {
            KeyAct::Change => "settings.key.change",
            KeyAct::Add => "settings.key.add",
            KeyAct::Reset => "settings.key.reset",
            KeyAct::Cancel => "settings.key.cancel",
            KeyAct::Swap => "settings.key.swap",
            KeyAct::Take => "settings.key.take",
            KeyAct::Keep => "settings.key.keep",
        })
    }
}

/// What entry `i`'s row offers, with `wait` as Settings waits.
pub fn key_acts(wait: Option<&crate::keys::Wait>, i: usize, bound: &crate::keys::Bindings) -> Vec<KeyAct> {
    use crate::keys::Wait;
    match wait.filter(|w| w.entry() == i) {
        Some(Wait::Taken { swap: true, .. }) => vec![KeyAct::Swap, KeyAct::Take, KeyAct::Keep],
        Some(Wait::Taken { .. }) => vec![KeyAct::Take, KeyAct::Keep],
        Some(_) => vec![KeyAct::Cancel],
        None if crate::keys::fixed(i, bound.mac()) => Vec::new(),
        None => {
            let mut acts = vec![KeyAct::Change];
            if matches!(crate::keys::TABLE[i].does, crate::keys::Does::Command(_)) {
                acts.push(KeyAct::Add);
            }
            if !bound.is_default(i) {
                acts.push(KeyAct::Reset);
            }
            acts
        }
    }
}

/// Settings' Keys: Reset All, then each of Flow's keys as it's bound, with
/// its default beside a changed one, and while Settings waits for a key,
/// what it waits for and why it refused one.
fn key_rows(wait: Option<&crate::keys::Wait>) -> Vec<Entry> {
    use crate::keys::{Does, Mode, Wait, Why, TABLE};
    let bound = crate::keys::bound();
    let mac = bound.mac();
    let mut rows = vec![row(SettingId::KeysReset, "keys_reset", words("keys_reset", "hint"), Control::Actions(vec![words("keys_reset", "button")]))];
    for (i, entry) in TABLE.iter().enumerate().take(u8::MAX as usize) {
        let here = wait.filter(|w| w.entry() == i);
        let adding = here.is_some_and(|w| w.mode() == Mode::Add);
        let press = if adding { per_os("hint_add_mac", "hint_add_windows", "hint_add_linux") } else { per_os("hint_press_mac", "hint_press_windows", "hint_press_linux") };
        let asks = words("key", if adding { "press_add" } else { "press" }).to_string();
        let family = match entry.does {
            Does::Tabs => Some(words("key", "hint_tabs")),
            Does::Panes => Some(words("key", "hint_panes")),
            Does::Command(_) => None,
        };
        let (text, hint) = match here {
            Some(Wait::Waiting(..)) => (asks, family.unwrap_or(words("key", press)).to_string()),
            Some(Wait::Refused(_, _, why)) => {
                let why = match why {
                    Why::Typing => keys("refused_typing_mac", "refused_typing"),
                    Why::Help => "refused_help",
                    Why::System => "refused_system",
                    Why::Hotkey => "refused_hotkey",
                    Why::Family => if entry.does == Does::Panes { "hint_panes" } else { "hint_tabs" },
                };
                (asks, words("key", why).to_string())
            }
            Some(Wait::Taken { keys, other, swap, .. }) => {
                let label = |k: &crate::keys::Keys| {
                    let mut b = (*bound).clone();
                    b.put(i, k.clone());
                    b.label_at(i)
                };
                let other = TABLE[*other].label();
                let text = tf("settings.key.taken", &[("key", &label(keys)), ("other", &other)]);
                let hint = tf(if *swap { "settings.key.hint_swap" } else { "settings.key.hint_take" }, &[("other", &other)]);
                (text, hint)
            }
            None => {
                let mut text = bound.label_at(i);
                if !bound.is_default(i) {
                    text = format!("{text}  ·  {}", tf("settings.key.default", &[("key", &bound.default_label(i))]));
                }
                let hint = if crate::keys::fixed(i, mac) { words("key", "hint_fixed") } else { words("key", "hint") };
                (text, hint.to_string())
            }
        };
        let control = Control::Item { text, actions: key_acts(wait, i, &bound).into_iter().map(KeyAct::label).collect() };
        rows.push(Entry::Setting(Row { id: SettingId::Key(i as u8), label: interned(&entry.label()), hint, control }));
    }
    rows
}

/// Settings' Skills: the user's folder of skills (Open) and a switch for
/// each of their own, on until switched off (`ai.skills_off`); then for each
/// installed package with skills, how many are on with All on and All off,
/// and a switch for each of them, off until turned on (`ai.skills_on`, as
/// `<package>:<name>`), so a package's whole inventory is seen at once and
/// installing it never changes what the assistant reads.
fn skills_rows(cfg: &Config, view: &SkillsView) -> Vec<Entry> {
    let own = view.skills.iter().filter(|s| s.from.is_none()).count();
    let mut rows = vec![row(
        SettingId::Skills,
        "skills",
        words("skills", "hint"),
        Control::Item { text: tf("settings.skills.value", &[("n", &own)]), actions: vec![words("skills", "open")] },
    )];
    let most = u16::MAX as usize;
    for (i, skill) in view.skills.iter().enumerate().take(most).filter(|(_, s)| s.from.is_none()) {
        let on = !cfg.ai.skills_off.contains(&skill.name);
        rows.push(Entry::Setting(Row { id: SettingId::Skill(i as u16), label: interned(&skill.name), hint: skill.description.clone(), control: Control::Toggle(on) }));
    }
    for (p, package) in view.packages.iter().enumerate().take(most) {
        let names: Vec<&str> = view.of(package).map(|(_, s)| s.name.split_once(':').map_or(s.name.as_str(), |(_, n)| n)).collect();
        let on = view.of(package).filter(|(_, s)| cfg.ai.skills_on.contains(&s.name)).count();
        let label = interned(&tf("settings.skill_package.label", &[("name", package)]));
        let hint = tf("settings.skill_package.hint", &[("name", package), ("skills", &names.join(", "))]);
        let control = Control::Item {
            text: tf("settings.skill_package.value", &[("on", &on), ("n", &names.len())]),
            actions: vec![words("skill_package", "all_on"), words("skill_package", "all_off")],
        };
        rows.push(Entry::Setting(Row { id: SettingId::SkillPackage(p as u16), label, hint, control }));
        for (i, skill) in view.of(package).filter(|(i, _)| *i < most) {
            let hint = tf("settings.skill.hint_from", &[("package", package), ("description", &skill.description)]);
            let on = cfg.ai.skills_on.contains(&skill.name);
            rows.push(Entry::Setting(Row { id: SettingId::PackageSkill(i as u16), label: interned(&skill.name), hint, control: Control::Toggle(on) }));
        }
    }
    rows
}

/// Settings' Outside apps: whether they may connect, `flow-mcp`'s path with
/// the lines that connect Claude Code and a JSON config to it, then each
/// app remembered (Forget), what it may do, a switch each, and how it may
/// run commands.
fn outside_rows(cfg: &Config, view: &OutsideView) -> Vec<Entry> {
    use crate::mcp::grants::Commands;
    let mut rows = vec![row(SettingId::McpEnabled, "mcp_enabled", words("mcp_enabled", "hint"), Control::Toggle(cfg.mcp.enabled))];
    let (text, actions) = match &view.flow_mcp {
        Some(path) => (path.display().to_string(), vec![words("mcp_connect", "copy_code"), words("mcp_connect", "copy_json")]),
        None => (words("mcp_connect", "none").to_string(), Vec::new()),
    };
    let hint = tf("settings.mcp_connect.hint", &[("path", &view.flow_mcp.as_ref().map_or_else(|| words("mcp_connect", "none").to_string(), |p| p.display().to_string()))]);
    rows.push(row(SettingId::McpConnect, "mcp_connect", hint, Control::Item { text, actions }));
    for (i, app) in view.apps.iter().enumerate().take(u16::MAX as usize) {
        let i = i as u16;
        let path = if app.path.is_empty() { t("outside.host_unknown").to_string() } else { app.path.clone() };
        let hint = tf("settings.mcp_app.hint", &[("app", &app.name), ("path", &path)]);
        let text = words("mcp_app", if view.listening { "remembered" } else { "off" }).to_string();
        rows.push(Entry::Setting(Row { id: SettingId::McpApp(i), label: interned(&app.name), hint, control: Control::Item { text, actions: vec![words("mcp_app", "forget")] } }));
        for (k, key, on) in [(0u8, "read", app.read), (1, "tabs", app.tabs), (2, "settings", app.settings)] {
            let hint = tf(&format!("settings.mcp_kind.hint_{key}"), &[("app", &app.name)]);
            rows.push(Entry::Setting(Row { id: SettingId::McpKind(i, k), label: words("mcp_kind", key), hint, control: Control::Toggle(on) }));
        }
        let current = match app.commands {
            Commands::Off => "off",
            Commands::Ask => "ask",
            Commands::Allowed => "allowed",
        };
        let hint = tf("settings.mcp_commands.hint", &[("app", &app.name)]);
        rows.push(Entry::Setting(Row { id: SettingId::McpCommands(i), label: words("mcp_commands", "label"), hint, control: choice("mcp_commands", &MCP_COMMANDS, current) }));
    }
    rows
}

/// Settings' MCP servers: `mcp-servers.json` (Add server… and Open), then
/// each server in it: how it stands, with Connect or Disconnect, Token… and
/// Remove; its command or URL, typed in place; how its calls are approved;
/// and once connected, how many of its tools are on (All on, All off) and a
/// switch for each, off until turned on, so connecting a server never
/// changes what the assistant may do.
fn servers_rows(cfg: &Config, view: &ServersView) -> Vec<Entry> {
    let mut hint = tf("settings.mcp_servers.hint", &[("file", &view.file)]);
    let text = if view.problems.is_empty() {
        tf(if view.servers.len() == 1 { "settings.mcp_servers.value_one" } else { "settings.mcp_servers.value" }, &[("n", &view.servers.len())])
    } else {
        hint.push('\n');
        hint.push_str(&view.problems.join("\n"));
        tf("settings.mcp_servers.value_problems", &[("n", &view.servers.len()), ("problems", &view.problems.len())])
    };
    let mut rows = vec![row(SettingId::ServersFile, "mcp_servers", hint, Control::Item { text, actions: vec![words("mcp_servers", "add"), words("mcp_servers", "open")] })];
    let most = u16::MAX as usize;
    for (i, server) in view.servers.iter().enumerate().take(most) {
        let i = i as u16;
        let name = server.name.as_str();
        let on: Vec<bool> = server.tools.iter().map(|t| cfg.ai.mcp_tools_on.contains(&format!("{name}:{}", t.name))).collect();
        let lit = on.iter().filter(|o| **o).count();
        let (text, mut hint) = match &server.standing {
            _ if server.line.is_empty() => (words("mcp_server", "new").to_string(), tf("settings.mcp_server.hint_new", &[("server", &name)])),
            ServerStanding::Off => (words("mcp_server", "off").to_string(), tf("settings.mcp_server.hint", &[("server", &name)])),
            ServerStanding::Connecting => (words("mcp_server", "connecting").to_string(), tf("settings.mcp_server.hint", &[("server", &name)])),
            ServerStanding::Connected => (
                tf("settings.mcp_server.tools", &[("on", &lit), ("n", &server.tools.len())]),
                tf("settings.mcp_server.hint", &[("server", &name)]),
            ),
            ServerStanding::Failed(why) => (
                words("mcp_server", "failed").to_string(),
                tf("settings.mcp_server.hint_failed", &[("server", &name), ("why", why)]),
            ),
            ServerStanding::SignIn => (words("mcp_server", "needs_sign_in").to_string(), tf("settings.mcp_server.hint_sign_in", &[("server", &name)])),
            ServerStanding::SigningIn => (words("mcp_server", "signing_in").to_string(), tf("settings.mcp_server.hint_signing_in", &[("server", &name)])),
        };
        // Its prompts are said in its hint; its resources, in their row's.
        if server.standing == ServerStanding::Connected && !server.prompts.is_empty() {
            let listed: Vec<String> = server.prompts.iter().map(|p| format!("/{name}:{p}")).collect();
            hint.push(' ');
            hint.push_str(&tf("settings.mcp_server.hint_prompts", &[("prompts", &listed.join(", "))]));
        }
        let actions = server_acts(server).into_iter().map(ServerAct::label).collect();
        rows.push(Entry::Setting(Row { id: SettingId::Server(i), label: interned(name), hint, control: Control::Item { text, actions } }));
        let control = Control::Text { value: server.line.clone(), multiline: false, empty: words("mcp_server_how", "empty") };
        rows.push(Entry::Setting(Row { id: SettingId::ServerHow(i), label: words("mcp_server_how", "label"), hint: tf("settings.mcp_server_how.hint", &[("server", &name)]), control }));
        if server.remote && !server.line.is_empty() {
            let text = match server.standing {
                ServerStanding::SigningIn => words("mcp_server_auth", "signing_in"),
                ServerStanding::SignIn => words("mcp_server_auth", "needed"),
                _ if server.signed_in => words("mcp_server_auth", "signed_in"),
                _ => words("mcp_server_auth", "none"),
            };
            let redirect = format!("http://127.0.0.1:{}/callback", crate::mcp::client::oauth::CALLBACK_PORT);
            let mut hint = tf("settings.mcp_server_auth.hint", &[("server", &name), ("redirect", &redirect)]);
            if let Some(id) = &server.client_id {
                hint.push(' ');
                hint.push_str(&tf("settings.mcp_server_auth.hint_client", &[("id", id)]));
            }
            let actions = server_auth_acts(server).into_iter().map(ServerAct::label).collect();
            rows.push(Entry::Setting(Row { id: SettingId::ServerAuth(i), label: words("mcp_server_auth", "label"), hint, control: Control::Item { text: text.to_string(), actions } }));
        }
        if server.resources > 0 {
            let on = cfg.ai.mcp_resources_on.iter().any(|s| s == name);
            let hint = tf("settings.mcp_server_resources.hint", &[("server", &name), ("n", &server.resources)]);
            rows.push(Entry::Setting(Row { id: SettingId::ServerResources(i), label: words("mcp_server_resources", "label"), hint, control: Control::Toggle(on) }));
        }
        let current = cfg.ai.mcp_approval.get(name).map_or("ask", |k| k.as_str());
        let hint = tf("settings.mcp_approval.hint", &[("server", &name)]);
        rows.push(Entry::Setting(Row { id: SettingId::ServerApproval(i), label: words("mcp_approval", "label"), hint, control: choice("mcp_approval", &MCP_APPROVALS, current) }));
        if server.tools.is_empty() {
            continue;
        }
        let names: Vec<&str> = server.tools.iter().map(|t| t.name.as_str()).collect();
        let control = Control::Item {
            text: tf("settings.mcp_server_tools.value", &[("on", &lit), ("n", &names.len())]),
            actions: vec![words("mcp_server_tools", "all_on"), words("mcp_server_tools", "all_off")],
        };
        let hint = tf("settings.mcp_server_tools.hint", &[("server", &name)]);
        rows.push(Entry::Setting(Row { id: SettingId::ServerTools(i), label: words("mcp_server_tools", "label"), hint, control }));
        for (k, tool) in server.tools.iter().enumerate().take(most) {
            let description = if tool.description.trim().is_empty() { words("mcp_server_tool", "no_description").to_string() } else { tool.description.clone() };
            let key = if tool.read_only { "hint_read_only" } else { "hint" };
            let hint = tf(&format!("settings.mcp_server_tool.{key}"), &[("server", &name), ("description", &description)]);
            let label = interned(&format!("  {}", tool.name));
            rows.push(Entry::Setting(Row { id: SettingId::ServerTool(i, k as u16), label, hint, control: Control::Toggle(on[k]) }));
        }
    }
    rows
}

/// How often Settings' Check apps can be: never by itself, or once the
/// catalog's copy is that old (`apps::shelf::check_every`).
pub const APPS_CHECK: [&str; 3] = ["off", "daily", "weekly"];

/// Settings' Apps: whether Flow reads flowapp's catalog at each start and
/// how often by itself, when it was read (with Check now), then a row for
/// each app and package of themes, its buttons doing what `flowapp` would.
fn apps_rows(cfg: &Config, apps: &[crate::apps::shelf::Line], age: Option<std::time::Duration>, checking: bool) -> Vec<Entry> {
    let mut rows = vec![
        row(SettingId::AppsAtStart, "apps_at_start", words("apps_at_start", "hint"), Control::Toggle(cfg.apps.check_at_start)),
        row(SettingId::AppsCheck, "apps_check", words("apps_check", "hint"), choice("apps_check", &APPS_CHECK, &cfg.apps.check)),
    ];
    let read = match age {
        _ if checking => t("settings.apps_catalog.reading").to_string(),
        None => t("settings.apps_catalog.never").to_string(),
        Some(age) => tf("settings.apps_catalog.read", &[("ago", &crate::ui::ago(age.as_millis().min(i64::MAX as u128) as i64, 0))]),
    };
    rows.push(row(SettingId::AppsCatalog, "apps_catalog", words("apps_catalog", "hint"), Control::Readout { text: read, action: words("apps_catalog", "check") }));
    for (i, line) in apps.iter().enumerate().take(u16::MAX as usize) {
        let control = Control::Item { text: line.text.clone(), actions: line.acts.iter().map(|a| a.label()).collect() };
        rows.push(Entry::Setting(Row { id: SettingId::App(i as u16), label: interned(&line.name), hint: line.hint.clone(), control }));
    }
    rows
}

/// `name` as text that lasts as long as Flow does, for an app's row's
/// label: each name is kept once, however often the rows are built.
fn interned(name: &str) -> &'static str {
    use std::collections::BTreeSet;
    use std::sync::Mutex;
    static NAMES: Mutex<BTreeSet<&'static str>> = Mutex::new(BTreeSet::new());
    let mut names = NAMES.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(kept) = names.get(name) {
        return kept;
    }
    let kept: &'static str = Box::leak(name.to_string().into_boxed_str());
    names.insert(kept);
    kept
}

/// Always on top: a switch, or where the window can't be kept on top
/// (`none_here`, Wayland), the words that say the window manager's menu
/// does it, with nothing to choose.
fn on_top_row(cfg: &Config, none_here: bool) -> Entry {
    if none_here {
        let text = words("on_top", "none_here").to_string();
        return row(SettingId::OnTop, "on_top", words("on_top", "hint_none_here"), Control::Item { text, actions: Vec::new() });
    }
    row(SettingId::OnTop, "on_top", words("on_top", "hint"), Control::Toggle(cfg.window.on_top))
}

/// Hotkey, and while one is set, Drop down: whether it drops the window
/// over the top of the screen, and while it does, how much of it, whether
/// it slides, whether it's put away when Flow loses the keys, and on the
/// Mac whether it drops over full-screen apps. The Hotkey row's hint says
/// where the hotkey couldn't be taken (`failed`), and asks for a key while
/// Settings waits for one (`pressing`).
fn hotkey_rows(cfg: &Config, failed: bool, pressing: Option<Pressing>, none_here: bool) -> Vec<Entry> {
    // Where none can be had, the row says so and why, with nothing to
    // choose, and the drop-down's rows, which follow the hotkey, go.
    if none_here {
        let text = words("hotkey", "none_here").to_string();
        return vec![row(SettingId::Hotkey, "hotkey", words("hotkey", "hint_none_here"), Control::Item { text, actions: Vec::new() })];
    }
    let w = &cfg.window;
    let set = !w.hotkey.trim().is_empty();
    let hint = match pressing {
        Some(Pressing::Waiting) => words("hotkey", per_os("hint_press_mac", "hint_press_windows", "hint_press_linux")),
        Some(Pressing::Refused) => words("hotkey", per_os("hint_refused_mac", "hint_refused_windows", "hint_refused_linux")),
        Some(Pressing::System) => words("key", "refused_system"),
        None if failed && set => words("hotkey", per_os("hint_failed", "hint_failed", "hint_failed_linux")),
        None => words("hotkey", "hint"),
    };
    let (options, index) = hotkey_options(&w.hotkey, pressing.is_some());
    let mut rows = vec![row(SettingId::Hotkey, "hotkey", hint, Control::Choice { options: options.into(), index })];
    if !set {
        return rows;
    }
    rows.push(row(SettingId::DropDown, "drop_down", words("drop_down", keys("hint_mac", "hint")), Control::Toggle(w.drop_down)));
    if w.drop_down {
        let heights = crate::hotkey::HEIGHTS.iter().map(|h| words("drop_down_height", h).to_string()).collect();
        let named = crate::hotkey::Share::named(&w.drop_down_height);
        let index = crate::hotkey::HEIGHTS.iter().position(|h| crate::hotkey::Share::named(h) == named).unwrap_or(1);
        rows.push(row(SettingId::DropDownHeight, "drop_down_height", words("drop_down_height", "hint"), Control::Choice { options: heights, index }));
        let slide_hint = words("drop_down_slide", per_os("hint", "hint", "hint_linux"));
        rows.push(row(SettingId::DropDownSlide, "drop_down_slide", slide_hint, Control::Toggle(w.drop_down_slide)));
        rows.push(row(SettingId::DropDownHides, "drop_down_hides", words("drop_down_hides", "hint"), Control::Toggle(w.drop_down_hides)));
        if cfg!(target_os = "macos") {
            let over = Control::Toggle(w.drop_down_over_full_screen);
            rows.push(row(SettingId::DropDownOverFullScreen, "drop_down_over_full_screen", words("drop_down_over_full_screen", "hint"), over));
        }
    }
    rows
}

/// Save as theme…, which names what's worn and keeps it beside the
/// built-in themes, and while one saved is chosen, Export…, which writes it
/// as a theme package, and Delete.
fn saved_theme_row(cfg: &Config) -> Entry {
    let chosen = crate::looks::saved_name(&cfg.ui.look).is_some_and(|name| cfg.themes.named(name).is_some());
    let mut actions = vec![words("saved_theme", "save")];
    if chosen {
        actions.extend([words("saved_theme", "export"), words("saved_theme", "delete")]);
    }
    row(SettingId::SavedTheme, "saved_theme", words("saved_theme", "hint"), Control::Actions(actions))
}

/// Your own theme's rows, under Theme, or the saved theme's chosen: its
/// start screen and prompt, typed in place, how fast the start screen is
/// typed out, and its machine's sounds.
fn own_rows(cfg: &Config, theme: &ThemeConfig) -> [Entry; 5] {
    let text = |id: SettingId, name: &str, multiline: bool| {
        row(id, name, words(name, "hint"), Control::Text { value: row_text(cfg, id), multiline, empty: words(name, "empty") })
    };
    let (bauds, baud) = baud_options(theme.baud);
    let sounds = crate::looks::Sounds::named(&theme.sounds).name();
    [
        text(SettingId::OwnStart, "own_start", true),
        text(SettingId::OwnPrompt, "own_prompt", false),
        row(SettingId::OwnBaud, "own_baud", words("own_baud", "hint"), Control::Choice { options: bauds.into(), index: baud }),
        row(SettingId::OwnSounds, "own_sounds", words("own_sounds", "hint"), choice("own_sounds", &THEME_SOUNDS, sounds)),
        row(SettingId::OwnFrame, "own_frame", words("own_frame", "hint"), Control::Toggle(theme.frame)),
    ]
}

/// The Sound section: Flow's own sounds, and while they're on how loud and
/// which; the bell, which can be the system's whether they're on or not.
fn sound_rows(cfg: &Config) -> Vec<Entry> {
    let s = &cfg.sound;
    let mut rows = vec![row(SettingId::Sound, "sound", words("sound", "hint"), Control::Toggle(s.enabled))];
    if s.enabled {
        rows.push(row(SettingId::SoundVolume, "sound_volume", words("sound_volume", "hint"), slider(VOLUME, s.volume, percent(s.volume))));
    }
    rows.push(row(SettingId::Bell, "bell", words("bell", "hint"), choice("bell", &BELLS, &s.bell)));
    rows.push(row(SettingId::BellFlash, "bell_flash", words("bell_flash", "hint"), Control::Toggle(s.bell_flash)));
    if s.enabled {
        rows.extend([
            row(SettingId::SoundCues, "sound_cues", words("sound_cues", "hint"), Control::Toggle(s.cues)),
            row(SettingId::EffectSounds, "effect_sounds", words("effect_sounds", "hint"), Control::Toggle(s.effects)),
            row(SettingId::ProgramSounds, "program_sounds", words("program_sounds", "hint"), Control::Toggle(s.programs)),
            row(SettingId::LiquidSounds, "liquid_sounds", words("liquid_sounds", "hint"), Control::Toggle(s.liquid)),
            row(SettingId::MachineSounds, "machine_sounds", words("machine_sounds", "hint"), Control::Toggle(s.machines)),
        ]);
    }
    rows
}

/// The CRT section: a preset to start from, how strong it is, and, while
/// it's on, each of its parts and the user's own presets.
fn crt_rows(cfg: &Config) -> Vec<Entry> {
    let (crt, strength) = (&cfg.crt, cfg.fx.crt);
    let presets = crt_presets(cfg);
    let current = current_crt_preset(&presets, crt, strength);
    let mut options: Vec<String> = presets.iter().map(|(name, _, _)| name.clone()).collect();
    let index = current.unwrap_or_else(|| {
        options.push(words("crt_preset", "custom").to_string());
        options.len() - 1
    });
    let off_or = |v: f32, text: String| if v <= 0.0 { t("common.off").to_string() } else { text };
    let mut rows = vec![
        row(SettingId::CrtPreset, "crt_preset", words("crt_preset", "hint"), Control::Choice { options: options.into(), index }),
        row(SettingId::Crt, "crt", words("crt", "hint"), slider(CRT, strength, off_or(strength, percent(strength)))),
    ];
    if strength > 0.0 {
        let ms = tf("settings.phosphor_decay.value", &[("ms", &format!("{:.0}", crt.decay * 1000.0))]);
        rows.extend([
            row(SettingId::Scanlines, "scanlines", words("scanlines", "hint"), slider(SCANLINES, crt.scanlines, off_or(crt.scanlines, percent(crt.scanlines)))),
            row(SettingId::Interlace, "interlace", words("interlace", "hint"), Control::Toggle(crt.interlace)),
            row(SettingId::PhosphorGlow, "phosphor_glow", words("phosphor_glow", "hint"), slider(PHOSPHOR_GLOW, crt.glow, off_or(crt.glow, percent(crt.glow)))),
            row(SettingId::PhosphorDecay, "phosphor_decay", words("phosphor_decay", "hint"), slider(PHOSPHOR_DECAY, crt.decay, off_or(crt.decay, ms))),
            row(SettingId::Grille, "grille", words("grille", "hint"), slider(GRILLE, crt.grille, off_or(crt.grille, percent(crt.grille)))),
            row(SettingId::Vignette, "vignette", words("vignette", "hint"), slider(VIGNETTE, crt.vignette, off_or(crt.vignette, percent(crt.vignette)))),
            row(SettingId::Curvature, "curvature", words("curvature", "hint"), slider(CURVATURE, crt.curvature, off_or(crt.curvature, percent(crt.curvature)))),
        ]);
        let saved_in_use = current.is_some_and(|i| presets[i].2);
        let actions = if saved_in_use {
            vec![words("crt_saved", "save"), words("crt_saved", "delete")]
        } else {
            vec![words("crt_saved", "save")]
        };
        rows.push(row(SettingId::CrtSaved, "crt_saved", words("crt_saved", "hint"), Control::Actions(actions)));
    }
    rows
}

/// The Assistant section: who answers, how, and with which key.
fn assistant_rows(cfg: &Config, auto_mode: bool, has_key: bool, usage: Usage, since: u64, (facts, tasks): (usize, usize), lines: usize) -> Vec<Entry> {
    let provider = cfg.ai.provider();
    let known = provider.models();
    let mut models: Vec<&str> = known.iter().map(|(_, label)| *label).collect();
    let model_index = match known.iter().position(|(id, _)| *id == cfg.ai.active_model()) {
        Some(i) => i,
        None => {
            // Keep a model set by hand in the config file selectable.
            models.push(cfg.ai.active_model());
            models.len() - 1
        }
    };
    let (cap_options, cap_index) = token_cap_options(cfg.ai.token_cap);
    let cap = cfg.ai.token_cap;
    let usage_text = if cap > 0 {
        tf("settings.token_usage.value_of_cap", &[("used", &fmt_tokens(usage.total())), ("cap", &fmt_tokens(cap))])
    } else {
        tf("settings.token_usage.value", &[("used", &fmt_tokens(usage.total()))])
    };
    let usage_hint = tf(
        "settings.token_usage.hint",
        &[
            ("date", &fmt_date(since)),
            ("input", &fmt_tokens(usage.input)),
            ("output", &fmt_tokens(usage.output)),
            ("reads", &fmt_tokens(usage.cache_read)),
            ("writes", &fmt_tokens(usage.cache_write)),
            ("requests", &usage.requests),
        ],
    );
    let model_hint = words("model", &format!("hint_{}", provider.id()));
    let effort_hint = words("effort", if provider == Provider::Gemini { "hint_gemini" } else { "hint" });
    let company = provider.company();
    let key_hint = tf(if has_key { "settings.api_key.hint" } else { "settings.api_key.hint_none" }, &[("company", &company)]);
    let mut rows = vec![
        row(
            SettingId::Provider,
            "provider",
            words("provider", "hint"),
            Control::Choice {
                options: Provider::ALL.iter().map(|p| p.name().to_string()).collect(),
                index: Provider::ALL.iter().position(|p| *p == provider).unwrap_or(0),
            },
        ),
        row(
            SettingId::Model,
            "model",
            model_hint,
            Control::Choice { options: models.into_iter().map(String::from).collect(), index: model_index },
        ),
        row(SettingId::Effort, "effort", effort_hint, choice("effort", &EFFORTS, &cfg.ai.effort)),
        row(SettingId::Mode, "mode", words("mode", "hint"), choice("mode", &MODES, if auto_mode { "auto" } else { "guide" })),
    ];
    if provider.has_web_search() {
        rows.push(row(SettingId::WebSearch, "web_search", words("web_search", "hint"), Control::Toggle(cfg.ai.web_search)));
    }
    rows.extend([
        row(SettingId::AiPane, "ai_pane", words("ai_pane", "hint"), choice("ai_pane", &AI_PANES, &cfg.ai.pane)),
        row(SettingId::Helpers, "helpers", words("helpers", "hint"), Control::Toggle(cfg.ai.helpers)),
        row(SettingId::HelperModel, "helper_model", words("helper_model", "hint"), {
            let mut options = vec![words("helper_model", "same").to_string()];
            options.extend(known.iter().map(|(_, label)| label.to_string()));
            let index = known.iter().position(|(id, _)| *id == cfg.ai.helper_model).map_or(0, |i| i + 1);
            Control::Choice { options: options.into(), index }
        }),
        row(SettingId::Memory, "memory", words("memory", "hint"), Control::Toggle(cfg.ai.memory)),
        row(
            SettingId::MemoryKept,
            "memory_kept",
            words("memory_kept", "hint"),
            Control::Item {
                text: tf("settings.memory_kept.value", &[("facts", &facts), ("tasks", &tasks)]),
                actions: vec![words("memory_kept", "open"), words("memory_kept", "clear")],
            },
        ),
        row(
            SettingId::Instructions,
            "instructions",
            words("instructions", "hint"),
            Control::Item {
                text: if lines == 0 { words("instructions", "none").to_string() } else { tf("settings.instructions.value", &[("n", &lines)]) },
                actions: vec![words("instructions", "open")],
            },
        ),
        row(SettingId::Own, "own", words("own", "hint"), Control::Toggle(cfg.ai.own)),
        row(SettingId::OwnMinutes, "own_minutes", words("own_minutes", "hint"), number_choice(&OWN_MINUTES, cfg.ai.own_minutes, |m| fmt_minutes(m as u64))),
        row(SettingId::OwnTokens, "own_tokens", words("own_tokens", "hint"), number_choice(&OWN_TOKENS, cfg.ai.own_tokens, fmt_tokens)),
        row(SettingId::TokenCap, "token_cap", words("token_cap", "hint"), Control::Choice { options: cap_options.into(), index: cap_index }),
        row(
            SettingId::TokenUsage,
            "token_usage",
            usage_hint,
            Control::Readout { text: usage_text, action: words("token_usage", "clear") },
        ),
        row(
            SettingId::ApiKey,
            "api_key",
            key_hint,
            Control::Actions(if has_key {
                vec![words("api_key", "change"), words("api_key", "remove")]
            } else {
                vec![words("api_key", "add")]
            }),
        ),
    ]);
    rows
}

/// The rows the keys move through, headers and settings, in display order.
pub fn rows(entries: &[Entry]) -> Vec<&Row> {
    entries.iter().map(|e| match e {
        Entry::Header(r) | Entry::Setting(r) => r,
    }).collect()
}

/// The section a setting is in, with every section open; none for a
/// header's own id.
/// Opens every section (`open`), or closes them all, as Settings' Expand
/// all and Collapse all do: only the headers show then, to open the one
/// wanted.
pub fn open_all(cfg: &mut Config, open: bool) {
    for section in Section::ALL {
        section.set_open(cfg, open);
    }
}

/// Whether `entries` has a section open, and one closed: what Collapse
/// all and Expand all would each change.
pub fn sections_shown(entries: &[Entry]) -> (bool, bool) {
    let opened = |want: bool| entries.iter().any(|e| matches!(e, Entry::Header(Row { control: Control::Section { open, .. }, .. }) if *open == want));
    (opened(true), opened(false))
}

pub fn section_of(entries: &[Entry], id: SettingId) -> Option<Section> {
    let mut section = None;
    for e in entries {
        match e {
            Entry::Header(Row { id: SettingId::Section(s), .. }) => section = Some(*s),
            Entry::Setting(r) if r.id == id => return section,
            _ => {}
        }
    }
    None
}

/// New slider value for a change, snapped to the step and clamped.
pub fn slider_value(current: f32, change: Change, (min, max, step): (f32, f32, f32)) -> f32 {
    let v = match change {
        Change::Set(v) => v,
        Change::Step(n) => current + n as f32 * step,
        _ => current,
    };
    let snapped = min + ((v - min) / step).round() * step;
    // Round away float noise so 0.1 steps save as 0.5, not 0.50000006.
    ((snapped.clamp(min, max)) * 1000.0).round() / 1000.0
}

/// New choice index for a change, wrapping around. A choice showing none
/// (Fit window to, while the window holds another grid, is past its end)
/// steps to its first, or back to its last.
pub fn choice_index(current: usize, len: usize, change: Change) -> usize {
    if len == 0 {
        return 0;
    }
    if current >= len {
        return match change {
            Change::Select(i) => i.min(len - 1),
            Change::Step(n) if n < 0 => len - 1,
            Change::Step(_) | Change::Toggle => 0,
            _ => current,
        };
    }
    match change {
        Change::Select(i) => i.min(len - 1),
        Change::Step(n) => (current as i64 + n as i64).rem_euclid(len as i64) as usize,
        Change::Toggle => (current + 1) % len,
        _ => current,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slider_snaps_and_clamps() {
        assert_eq!(slider_value(1.0, Change::Step(-1), FLOW_SPEED), 0.9);
        assert_eq!(slider_value(0.1, Change::Step(-3), FLOW_SPEED), 0.1);
        assert_eq!(slider_value(1.0, Change::Set(0.537), FLOW_SPEED), 0.5);
        assert_eq!(slider_value(1.0, Change::Set(9.0), FLOW_SPEED), 2.0);
    }

    #[test]
    fn choices_wrap() {
        assert_eq!(choice_index(0, 4, Change::Step(-1)), 3);
        assert_eq!(choice_index(3, 4, Change::Step(1)), 0);
        assert_eq!(choice_index(1, 4, Change::Select(9)), 3);
    }

    #[test]
    fn a_choice_showing_none_steps_to_its_first_or_back_to_its_last() {
        assert_eq!(choice_index(3, 3, Change::Step(1)), 0);
        assert_eq!(choice_index(3, 3, Change::Step(-1)), 2);
        assert_eq!(choice_index(3, 3, Change::Toggle), 0);
        assert_eq!(choice_index(3, 3, Change::Select(1)), 1);
    }

    #[test]
    fn the_font_size_row_says_what_the_window_holds_and_each_characters_size() {
        let note = |grid| match row_of_with(&Config::default(), SettingId::FontSize, grid).control {
            Control::Slider { note, .. } => note,
            _ => panic!("a slider"),
        };
        let grid = TextGrid { cols: 80, rows: 24, cell: (9.0, 18.0), scale: 1.0 };
        assert_eq!(note(Some(grid)).as_deref(), Some("80 × 24 characters of 9 × 18 pixels"));
        // On a display of two pixels to a point, the cell in both, to a tenth.
        let grid = TextGrid { cols: 132, rows: 43, cell: (16.8125, 34.0), scale: 2.0 };
        assert_eq!(note(Some(grid)).as_deref(), Some("132 × 43 characters of 16.8 × 34 px (8.4 × 17 pt)"));
        assert_eq!(note(None), None, "nothing said before the window is laid out");
    }

    #[test]
    fn fit_window_offers_the_worn_machines_screen_first_and_chooses_the_one_the_window_holds() {
        let mut cfg = Config::default();
        assert_eq!(screens(&cfg), SCREENS, "Flow's look has no screen of its own");
        let choice = |cfg: &Config, grid| match row_of_with(cfg, SettingId::FitScreen, grid).control {
            Control::Choice { options, index } => (options.to_vec(), index),
            _ => panic!("a choice"),
        };
        let held = |cols, rows| Some(TextGrid { cols, rows, cell: (9.0, 18.0), scale: 1.0 });
        assert_eq!(choice(&cfg, held(80, 24)), (vec!["80 × 24".to_string(), "80 × 25".into(), "40 × 25".into()], 0));
        assert_eq!(choice(&cfg, held(100, 30)).1, 3, "none chosen while the window holds another");
        crate::looks::find("workstation").apply(&mut cfg);
        assert_eq!(screens(&cfg), [(80, 34), (80, 24), (80, 25), (40, 25)]);
        assert!(row_of_with(&cfg, SettingId::FitScreen, None).hint.contains("Workstation"));
        assert_eq!(choice(&cfg, held(80, 34)).1, 0);
        // A machine whose screen is one of them has it first, once.
        crate::looks::find("pc").apply(&mut cfg);
        assert_eq!(screens(&cfg), [(80, 25), (80, 24), (40, 25)]);
        cfg.ui.look = crate::looks::OWN.into();
        assert_eq!(screens(&cfg), SCREENS, "your own theme has none");
    }

    #[test]
    fn fit_text_finds_the_largest_whole_size_that_holds_the_screen() {
        // A window of 800 × 490 pixels, cells 0.6 by 1.2 of the size, as a
        // font's grow with it near enough.
        let holds = |size: f32| ((800.0 / (size * 0.6)) as usize, (490.0 / (size * 1.2)) as usize);
        // 80 columns need cells of 10 px at most, so 16 pt; 24 rows and 25
        // fit at 16 but not at 17.
        for guess in [8.0, 15.0, 16.0, 30.0] {
            assert_eq!(text_size_for((80, 24), guess, holds), Some(16.0), "from {guess}");
        }
        assert_eq!(text_size_for((40, 25), 15.0, holds), Some(16.0), "the rows hold it back");
        assert_eq!(text_size_for((40, 10), 15.0, holds), Some(33.0));
        // Past the largest size, the largest.
        assert_eq!(text_size_for((10, 5), 15.0, holds), Some(FONT_SIZE.1));
        // Where even the least is too big, none.
        assert_eq!(text_size_for((200, 60), 15.0, holds), None);
        let cfg = Config::default();
        let row = row_of_with(&cfg, SettingId::FitText, Some(TextGrid { cols: 80, rows: 25, cell: (9.0, 18.0), scale: 1.0 }));
        assert!(matches!(row.control, Control::Choice { index: 1, .. }));
    }

    #[test]
    fn every_rows_key_in_the_config_is_one_the_config_has() {
        let mut cfg = Config::default();
        cfg.ui.closed_sections.clear();
        cfg.themes.saved.push(Default::default());
        cfg.keys.insert("new_tab".into(), crate::keys::KeySetting::One("cmd+n".into()));
        let tree = serde_json::to_value(&cfg).unwrap();
        let all = entries(&cfg, &FontChoices::default(), None, &[], false, true, Tally::default());
        let mut named = 0;
        for row in rows(&all) {
            let Some(key) = config_key(row.id) else { continue };
            named += 1;
            // A key's row names its own key under [keys], which a config
            // holds only once it's changed.
            let path = if key.split('.').next() == Some("keys") { "keys".to_string() } else { key.clone() };
            let mut at = &tree;
            for part in path.split('.') {
                at = at.get(part).unwrap_or_else(|| panic!("{:?}'s {key} isn't in the config", row.id));
            }
        }
        assert!(named > 80, "{named}");
    }

    #[test]
    fn the_keys_section_is_closed_once_for_a_config_from_before_it() {
        // (what ran last, this Flow, a config there before) → whether it's the first start since.
        for (ran, this, older, first) in [
            ("0.1.38", "0.1.39", true, true),
            ("0.1.37.4", "0.1.40", true, true),
            ("", "0.1.39", true, true),
            ("", "0.1.39", false, false),
            ("0.1.39", "0.1.39", true, false),
            ("0.1.39", "0.1.40", true, false),
            // A build still called 0.1.38, over 0.1.38: not yet.
            ("0.1.38", "0.1.38", true, false),
        ] {
            assert_eq!(first_start_since_keys(ran, this, older), first, "{ran:?} {this} {older}");
        }
        // A config from before lists the sections it closed, without Keys.
        let mut cfg = Config::default();
        cfg.ui.closed_sections = vec!["crt".into(), "sound".into()];
        assert!(Section::Keys.open(&cfg));
        assert!(close_keys_section(&mut cfg));
        assert!(!Section::Keys.open(&cfg) && !Section::Crt.open(&cfg));
        assert!(!close_keys_section(&mut cfg), "closed already");
    }

    #[test]
    fn a_keys_row_says_its_key_its_default_once_changed_and_what_it_waits_for() {
        use crate::keys::{Bindings, Chord, Keys, Wait, TABLE};
        i18n::set_language("en", &[]);
        let mut cfg = Config::default();
        Section::Keys.set_open(&mut cfg, true);
        let at = |name: &str| TABLE.iter().position(|e| e.name == name).unwrap();
        let (new_tab, find) = (at("new_tab"), at("find"));
        let mut keys = Bindings::defaults(true);
        keys.put(new_tab, Keys::Chords(vec![Chord::parse("cmd+n").unwrap()]));
        crate::keys::set(keys);
        let tally = Tally { key_wait: Some(Wait::Waiting(find, crate::keys::Mode::Change)), ..Tally::default() };
        let all = entries(&cfg, &FontChoices::default(), None, &[], false, true, tally);
        let row = |i: usize| rows(&all).into_iter().find(|r| r.id == SettingId::Key(i as u8)).unwrap().clone();
        let Control::Item { text, actions } = row(new_tab).control else { panic!() };
        assert_eq!(text, "⌘N  ·  default ⌘T");
        assert_eq!(actions, ["Change", "Add Key", "Reset"]);
        let Control::Item { text, actions } = row(find).control else { panic!() };
        assert_eq!((text.as_str(), actions), ("Press a key…", vec!["Cancel"]));
        let Control::Item { text, actions } = row(at("copy")).control else { panic!() };
        assert_eq!((text.as_str(), actions), ("⌘C", vec!["Change", "Add Key"]));
        // A family has one set of modifiers: nothing to add.
        let Control::Item { actions, .. } = row(at("go_to_tab")).control else { panic!() };
        assert_eq!(actions, ["Change"]);
        // Waiting to add, it says so.
        let adding = Tally { key_wait: Some(Wait::Waiting(find, crate::keys::Mode::Add)), ..Tally::default() };
        let more = entries(&cfg, &FontChoices::default(), None, &[], false, true, adding);
        let found = rows(&more).into_iter().find(|r| r.id == SettingId::Key(find as u8)).unwrap().clone();
        let Control::Item { text, actions } = found.control else { panic!() };
        assert_eq!((text.as_str(), actions), ("Press a key to add…", vec!["Cancel"]));
        assert!(found.hint.contains("besides the keys it has"), "{}", found.hint);
        // Quit's on the Mac is the system's: nothing to press.
        let Control::Item { actions, .. } = row(at("quit")).control else { panic!() };
        assert!(actions.is_empty());
        assert!(rows(&all).iter().any(|r| r.id == SettingId::KeysReset));
        crate::keys::set(Bindings::defaults(true));
    }

    #[test]
    fn the_hints_of_font_size_fit_window_and_efficient_mode_fit_the_footers_two_lines() {
        // The footer shows two lines of a hint, which in the window Fit
        // window to makes for 80 × 24 at the default size hold 74 cells.
        let keys = [
            "settings.font_size.hint",
            "settings.fit_screen.hint",
            "settings.fit_text.hint",
            "settings.own_sizes.hint",
            "settings.efficient.hint",
            "settings.efficient.hint_battery",
            "settings.efficient.hint_low_power",
            "settings.efficient.hint_low_power_mac",
            "settings.efficient_on_battery.hint",
            "settings.efficient_on_battery.hint_mac",
            "settings.efficient_on_battery.hint_windows",
        ];
        for language in ["en", "ja"] {
            i18n::set_language(language, &[]);
            let machine = tf("settings.fit_screen.hint_machine", &[("machine", &words("theme", "graphics"))]);
            for hint in keys.iter().map(|k| t(k).to_string()).chain([machine]) {
                assert!(crate::text::wrap_plain(&hint, 74, crate::text::Style::Normal).len() <= 2, "{language}: {hint}");
            }
        }
        i18n::set_language("en", &[]);
    }

    #[test]
    fn every_row_has_a_hint() {
        let cfg = Config::default();
        let entries = entries(&cfg, &FontChoices::default(), None, &[], false, false, Tally::default());
        let rows = rows(&entries);
        assert!(rows.iter().any(|r| r.id == SettingId::FlowSpeed));
        assert!(rows.iter().all(|r| !r.hint.is_empty()));
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn font_choices_are_sorted_once_each_ignoring_case() {
        let fonts = font_choices(names(&["PT Mono", "menlo", "Andale Mono", "Menlo", "andale mono"]), Some("Menlo"));
        assert_eq!(fonts.families[..], ["Andale Mono", "menlo", "PT Mono"]);
        assert_eq!(fonts.current, 1);
    }

    #[test]
    fn the_font_in_use_is_offered_even_when_not_installed_as_monospaced() {
        let fonts = font_choices(names(&["Menlo", "PT Mono"]), Some("Monaco"));
        assert_eq!(fonts.families[..], ["Menlo", "Monaco", "PT Mono"]);
        assert_eq!(fonts.families[fonts.current], "Monaco");
    }

    #[test]
    fn with_no_fonts_found_the_row_still_shows_one() {
        let fonts = font_choices(Vec::new(), None);
        assert_eq!((&fonts.families[..], fonts.current), ([SYSTEM_FONT.to_string()].as_slice(), 0));
        let fonts = font_choices(Vec::new(), Some("Menlo"));
        assert_eq!((&fonts.families[..], fonts.current), (["Menlo".to_string()].as_slice(), 0));
        // Stepping a one-font row stays on it.
        assert_eq!(choice_index(fonts.current, fonts.families.len(), Change::Step(1)), 0);
    }

    #[test]
    fn browsing_fonts_leaves_only_the_last_pick_ahead_of_the_families_the_menu_began_with() {
        let mut menu = SettingsMenu::default();
        let mut font = FontConfig { families: names(&["SF Mono", "Menlo", "Monaco"]), ..FontConfig::default() };
        for pick in ["Monaco", "Osaka", "PT Mono", "Andale Mono"] {
            menu.pick_font(&mut font, pick);
        }
        assert_eq!(font.families, ["Andale Mono", "SF Mono", "Menlo", "Monaco"]);
        menu.pick_font(&mut font, "menlo");
        assert_eq!(font.families, ["menlo", "SF Mono", "Monaco"]);
        // The next time the menu opens, the last pick is kept as a fallback.
        let mut menu = SettingsMenu::default();
        menu.pick_font(&mut font, "Courier New");
        assert_eq!(font.families, ["Courier New", "menlo", "SF Mono", "Monaco"]);
    }

    #[test]
    fn the_font_row_sits_before_font_size_and_shows_the_family() {
        let cfg = Config::default();
        let fonts = font_choices(names(&["Courier New", "Menlo"]), Some("Menlo"));
        let entries = entries(&cfg, &fonts, None, &[], false, false, Tally::default());
        let rows = rows(&entries);
        let at = rows.iter().position(|r| r.id == SettingId::Font).unwrap();
        assert_eq!(rows[at + 1].id, SettingId::FontSize);
        match &rows[at].control {
            Control::Choice { options, index } => assert_eq!(options[*index], "Menlo"),
            other => panic!("font row is {other:?}"),
        }
    }

    #[test]
    fn the_model_row_offers_the_provider_s_models_and_gemini_gets_no_web_search() {
        let model_row = |cfg: &Config| {
            let entries = entries(cfg, &FontChoices::default(), None, &[], false, true, Tally::default());
            let rows = rows(&entries);
            let web_search = rows.iter().any(|r| r.id == SettingId::WebSearch);
            let provider = rows.iter().position(|r| r.id == SettingId::Provider).unwrap();
            assert_eq!(rows[provider + 1].id, SettingId::Model);
            match &rows[provider + 1].control {
                Control::Choice { options, index } => (options.to_vec(), *index, web_search),
                other => panic!("model row is {other:?}"),
            }
        };
        let mut cfg = Config::default();
        let (options, index, web_search) = model_row(&cfg);
        assert_eq!((options[index].as_str(), web_search), ("Opus 5.5", true));
        cfg.ai.provider = "openai".into();
        let (options, index, web_search) = model_row(&cfg);
        assert_eq!((options[index].as_str(), options.len(), web_search), ("6.1 Sol", 3, true));
        cfg.ai.provider = "gemini".into();
        cfg.ai.gemini_model = "gemini-9-test".into();
        let (options, index, web_search) = model_row(&cfg);
        // A model set by hand stays selectable.
        assert_eq!((options[index].as_str(), options.len(), web_search), ("gemini-9-test", 4, false));
        cfg.ai.set_model("gemini-3.8-flash");
        assert_eq!((cfg.ai.gemini_model.as_str(), cfg.ai.model.as_str()), ("gemini-3.8-flash", "claude-opus-5-5"));
    }

    /// The row `id` in the rows for `cfg`, its options, the one chosen and
    /// its hint; none where it isn't shown.
    fn choice_row(cfg: &Config, id: SettingId, tally: Tally) -> Option<(Vec<String>, usize, String)> {
        let entries = entries(cfg, &FontChoices::default(), None, &[], false, true, tally);
        let row = rows(&entries).into_iter().find(|r| r.id == id)?;
        match &row.control {
            Control::Choice { options, index } => Some((options.to_vec(), *index, row.hint.clone())),
            Control::Toggle(on) => Some((Vec::new(), *on as usize, row.hint.clone())),
            other => panic!("{id:?} is {other:?}"),
        }
    }

    #[test]
    fn the_hotkey_row_offers_off_and_four_keys_and_shows_the_one_set() {
        crate::i18n::set_language("en", &[]);
        let mut cfg = Config::default();
        let (options, index, _) = choice_row(&cfg, SettingId::Hotkey, Tally::default()).unwrap();
        let keys: &[&str] = if cfg!(target_os = "macos") {
            &["Off", "⌃`", "⌥Space", "F12", "⌃Space", "Press a key…"]
        } else if cfg!(windows) {
            // Windows keeps F12 for debuggers.
            &["Off", "Ctrl+`", "Alt+Space", "Ctrl+Space", "Press a key…"]
        } else {
            &["Off", "Ctrl+`", "Alt+Space", "F12", "Ctrl+Space", "Press a key…"]
        };
        assert_eq!((options.as_slice(), index), (&keys.iter().map(|k| k.to_string()).collect::<Vec<_>>()[..], 0));
        // Drop down shows only once there's a hotkey to drop the window.
        assert!(choice_row(&cfg, SettingId::DropDown, Tally::default()).is_none());
        // Each choice, set as Settings sets it, reads back as chosen.
        for (i, name) in crate::hotkey::CHOICES.iter().enumerate() {
            cfg.window.hotkey = name.to_string();
            assert_eq!(choice_row(&cfg, SettingId::Hotkey, Tally::default()).unwrap().1, i, "{name}");
        }
        assert_eq!(choice_row(&cfg, SettingId::DropDown, Tally::default()).unwrap().1, 1, "on by default");
        // One written by hand, in another case, is the same choice; one
        // Flow can't read stays as it's written, past the choices.
        cfg.window.hotkey = "Alt+Space".into();
        assert_eq!(choice_row(&cfg, SettingId::Hotkey, Tally::default()).unwrap().1, 2);
        let past = crate::hotkey::CHOICES.len() + 2;
        cfg.window.hotkey = "hyper+q".into();
        let (options, index, _) = choice_row(&cfg, SettingId::Hotkey, Tally::default()).unwrap();
        assert_eq!((options.len(), options[index].as_str()), (past, "hyper+q"));
        // A key pressed that isn't one of them shows as the system writes it.
        cfg.window.hotkey = "ctrl+shift+t".into();
        let (options, index, _) = choice_row(&cfg, SettingId::Hotkey, Tally::default()).unwrap();
        let mac = cfg!(target_os = "macos");
        assert_eq!((options.len(), options[index].as_str()), (past, if mac { "⌃⇧T" } else { "Ctrl+Shift+T" }));
        // F12 set before, on Windows, shows past them as one pressed would.
        if cfg!(windows) {
            cfg.window.hotkey = "f12".into();
            let (options, index, _) = choice_row(&cfg, SettingId::Hotkey, Tally::default()).unwrap();
            assert_eq!((options.len(), options[index].as_str()), (past, "F12"));
        }
    }

    #[test]
    fn waiting_for_a_key_to_be_pressed_chooses_press_a_key_and_asks_for_one() {
        crate::i18n::set_language("en", &[]);
        let mut cfg = Config::default();
        cfg.window.hotkey = "f12".into();
        let waiting = Tally { hotkey_pressing: Some(Pressing::Waiting), ..Tally::default() };
        let refused = Tally { hotkey_pressing: Some(Pressing::Refused), ..Tally::default() };
        let (_, _, plain) = choice_row(&cfg, SettingId::Hotkey, Tally::default()).unwrap();
        let (options, index, asking) = choice_row(&cfg, SettingId::Hotkey, waiting).unwrap();
        assert_eq!((index, options[index].as_str()), (options.len() - 1, "Press a key…"));
        let (_, _, saying) = choice_row(&cfg, SettingId::Hotkey, refused).unwrap();
        assert!(asking.starts_with("Press the key") && saying.starts_with("Flow can't take that key"), "{asking} / {saying}");
        assert_ne!(plain, asking);
        // It asks over a hotkey that couldn't be taken, too.
        let failed = Tally { hotkey_failed: true, hotkey_pressing: Some(Pressing::Waiting), ..Tally::default() };
        assert_eq!(choice_row(&cfg, SettingId::Hotkey, failed).unwrap().2, asking);
    }

    #[test]
    fn the_drop_down_s_rows_show_while_it_drops_and_read_its_config() {
        crate::i18n::set_language("en", &[]);
        let ids = [SettingId::DropDownHeight, SettingId::DropDownSlide, SettingId::DropDownHides, SettingId::DropDownOverFullScreen];
        let shown = |cfg: &Config| ids.iter().filter(|&&id| choice_row(cfg, id, Tally::default()).is_some()).count();
        let mut cfg = Config::default();
        assert_eq!(shown(&cfg), 0, "no hotkey, nothing drops");
        cfg.window.hotkey = "ctrl+`".into();
        // Over full-screen apps is the Mac's alone.
        assert_eq!(shown(&cfg), if cfg!(target_os = "macos") { 4 } else { 3 });
        let (heights, index, _) = choice_row(&cfg, SettingId::DropDownHeight, Tally::default()).unwrap();
        assert_eq!((heights.as_slice(), index), (&["A third", "Half", "Two thirds", "All"].map(String::from)[..], 1));
        for (i, name) in crate::hotkey::HEIGHTS.iter().enumerate() {
            cfg.window.drop_down_height = name.to_string();
            assert_eq!(choice_row(&cfg, SettingId::DropDownHeight, Tally::default()).unwrap().1, i, "{name}");
        }
        // One Flow doesn't know is half, as it drops.
        cfg.window.drop_down_height = "most".into();
        assert_eq!(choice_row(&cfg, SettingId::DropDownHeight, Tally::default()).unwrap().1, 1);
        // On at first: Slide; off: Put away when unfocused, Over full-screen apps.
        assert_eq!(choice_row(&cfg, SettingId::DropDownSlide, Tally::default()).unwrap().1, 1);
        assert_eq!(choice_row(&cfg, SettingId::DropDownHides, Tally::default()).unwrap().1, 0);
        cfg.window.drop_down_hides = true;
        assert_eq!(choice_row(&cfg, SettingId::DropDownHides, Tally::default()).unwrap().1, 1);
        if cfg!(target_os = "macos") {
            assert_eq!(choice_row(&cfg, SettingId::DropDownOverFullScreen, Tally::default()).unwrap().1, 0);
        }
        // Coming forward where it was, it doesn't drop: no height, no slide.
        cfg.window.drop_down = false;
        assert_eq!(shown(&cfg), 0);
        assert!(choice_row(&cfg, SettingId::DropDown, Tally::default()).is_some());
    }

    #[test]
    fn a_hotkey_that_couldnt_be_taken_says_so_in_its_hint() {
        crate::i18n::set_language("en", &[]);
        let mut cfg = Config::default();
        cfg.window.hotkey = "f12".into();
        let taken = Tally { hotkey_failed: true, ..Tally::default() };
        let (_, _, fine) = choice_row(&cfg, SettingId::Hotkey, Tally::default()).unwrap();
        let (_, _, failed) = choice_row(&cfg, SettingId::Hotkey, taken.clone()).unwrap();
        assert_ne!(fine, failed);
        // With none set there's nothing to have failed.
        cfg.window.hotkey.clear();
        assert_eq!(choice_row(&cfg, SettingId::Hotkey, taken).unwrap().2, fine);
    }

    #[test]
    fn where_no_hotkey_can_be_had_its_row_says_so_and_the_drop_down_goes() {
        crate::i18n::set_language("en", &[]);
        let mut cfg = Config::default();
        cfg.window.hotkey = "f12".into();
        cfg.window.drop_down = true;
        let ids = |tally: Tally| rows(&entries(&cfg, &FontChoices::default(), None, &[], false, true, tally)).into_iter().map(|r| r.id).collect::<Vec<_>>();
        assert!(ids(Tally::default()).contains(&SettingId::DropDown));
        let none = Tally { no_hotkey: true, ..Tally::default() };
        let shown = ids(none.clone());
        assert!(shown.contains(&SettingId::Hotkey) && !shown.contains(&SettingId::DropDown) && !shown.contains(&SettingId::DropDownHeight));
        let entries = entries(&cfg, &FontChoices::default(), None, &[], false, true, none);
        let row = rows(&entries).into_iter().find(|r| r.id == SettingId::Hotkey).unwrap();
        let Control::Item { text, actions } = &row.control else { panic!("{:?}", row.control) };
        assert_eq!((text.as_str(), actions.is_empty()), ("Not available on Wayland", true));
        assert!(row.hint.contains("XWayland"), "{}", row.hint);
    }

    #[test]
    fn always_on_top_is_a_switch_off_at_first_and_where_it_cant_be_had_its_row_says_so() {
        crate::i18n::set_language("en", &[]);
        let mut cfg = Config::default();
        let row_of = |cfg: &Config, tally: Tally| rows(&entries(cfg, &FontChoices::default(), None, &[], false, true, tally)).into_iter().find(|r| r.id == SettingId::OnTop).cloned().unwrap();
        assert!(matches!(row_of(&cfg, Tally::default()).control, Control::Toggle(false)));
        cfg.window.on_top = true;
        let on = row_of(&cfg, Tally::default());
        assert!(matches!(on.control, Control::Toggle(true)));
        assert_eq!((on.label, config_key(SettingId::OnTop).as_deref()), ("Always on top", Some("window.on_top")));
        let none = row_of(&cfg, Tally { no_on_top: true, ..Tally::default() });
        let Control::Item { text, actions } = &none.control else { panic!("{:?}", none.control) };
        assert_eq!((text.as_str(), actions.is_empty()), ("Not available on Wayland", true));
        assert!(none.hint.contains("right-click Flow's title bar"), "{}", none.hint);
    }

    #[test]
    fn the_notify_row_offers_off_to_five_minutes_with_half_a_minute_at_first() {
        crate::i18n::set_language("en", &[]);
        let mut cfg = Config::default();
        let (options, index, _) = choice_row(&cfg, SettingId::NotifyAfter, Tally::default()).unwrap();
        assert_eq!((options.as_slice(), index), (&["Off", "10 s", "30 s", "1 min", "5 min"].map(String::from)[..], 2));
        for (i, &after) in crate::notify::AFTER.iter().enumerate() {
            cfg.ui.notify_after = after;
            assert_eq!(choice_row(&cfg, SettingId::NotifyAfter, Tally::default()).unwrap().1, i);
        }
        cfg.ui.notify_after = 90;
        let (options, index, _) = choice_row(&cfg, SettingId::NotifyAfter, Tally::default()).unwrap();
        assert_eq!((options.len(), options[index].as_str()), (6, "1 min 30 s"));
    }

    #[test]
    fn programs_may_set_the_hotkey_and_the_notification_to_what_settings_offers() {
        let cfg = Config::default();
        let hotkey = takes(&cfg, "window.hotkey").unwrap();
        assert!(allows(&hotkey, &serde_json::json!("ctrl+`")) && allows(&hotkey, &serde_json::json!("")));
        assert!(!allows(&hotkey, &serde_json::json!("cmd+q")));
        let height = takes(&cfg, "window.drop_down_height").unwrap();
        assert!(crate::hotkey::HEIGHTS.iter().all(|h| allows(&height, &serde_json::json!(h))));
        assert!(!allows(&height, &serde_json::json!("most")));
        let after = takes(&cfg, "ui.notify_after").unwrap();
        assert!(allows(&after, &serde_json::json!(60)) && !allows(&after, &serde_json::json!(45)));
    }

    #[test]
    fn with_no_liquid_the_rows_about_its_flow_are_gone() {
        let ids = |style: &str| {
            let mut cfg = Config::default();
            cfg.fx.style = style.into();
            let entries = entries(&cfg, &FontChoices::default(), None, &[], false, true, Tally::default());
            rows(&entries).iter().map(|r| r.id).collect::<Vec<_>>()
        };
        let flowing = [SettingId::FlowSpeed, SettingId::FlowAmount, SettingId::TextRipple, SettingId::Liquid, SettingId::ProgramEffects];
        let clouds = ids("clouds");
        assert!(flowing.iter().all(|id| clouds.contains(id)));
        // The rain has how hard it rains in place of the flow's amount.
        let rain = ids("rain");
        assert!(rain.contains(&SettingId::RainAmount) && !rain.contains(&SettingId::FlowAmount) && !clouds.contains(&SettingId::RainAmount));
        let none = ids("none");
        assert!(flowing.iter().all(|id| !none.contains(id)), "{none:?}");
        assert!(!none.contains(&SettingId::Palette) && !none.contains(&SettingId::Electricity));
        // The reactions show on plain glass too.
        let kept = [SettingId::LiquidStyle, SettingId::Opacity, SettingId::Reactions, SettingId::Backdrop, SettingId::Font];
        assert!(kept.iter().all(|id| none.contains(id)));
    }

    #[test]
    fn a_row_comes_into_view_with_the_list_moving_no_further_than_it_must() {
        // In view: the list stays put.
        assert_eq!(reveal(100.0, 150.0, 200.0, 300.0), 100.0);
        // Below: it ends up at the bottom.
        assert_eq!(reveal(0.0, 400.0, 450.0, 300.0), 150.0);
        // Above: it ends up at the top.
        assert_eq!(reveal(200.0, 50.0, 100.0, 300.0), 50.0);
    }

    #[test]
    fn screen_refresh_starts_on_the_display_and_keeps_a_hand_set_rate() {
        let (labels, i) = refresh_options(0, true);
        assert_eq!((labels[i].as_str(), labels.len()), ("Display", REFRESH_RATES.len()));
        let (labels, i) = refresh_options(0, false);
        assert_eq!(labels[i], "Unlimited");
        let (labels, i) = refresh_options(120, true);
        assert_eq!(labels[i], "120 Hz");
        let (labels, i) = refresh_options(90, true);
        assert_eq!((labels[i].as_str(), labels.len()), ("90 Hz", REFRESH_RATES.len() + 1));
    }

    #[test]
    fn token_cap_choices_keep_custom_values() {
        let (labels, i) = token_cap_options(0);
        assert_eq!((labels[i].as_str(), labels.len()), ("Off", TOKEN_CAPS.len()));
        let (labels, i) = token_cap_options(1_000_000);
        assert_eq!(labels[i], "1M");
        let (labels, i) = token_cap_options(750_000);
        assert_eq!((labels[i].as_str(), labels.len()), ("750k", TOKEN_CAPS.len() + 1));
    }

    #[test]
    fn the_color_rows_start_on_their_defaults_and_keep_any_other_color() {
        let mut cfg = Config::default();
        let colors = |cfg: &Config| {
            let entries = entries(cfg, &FontChoices::default(), None, &[], false, true, Tally::default());
            rows(&entries)
                .iter()
                .filter_map(|r| match &r.control {
                    Control::Colors { options, index, .. } => Some((r.id, options[*index].0.clone(), options.len())),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        // The glass's tint comes first, under Glass, then the text's.
        assert_eq!(
            colors(&cfg),
            [
                (SettingId::BackgroundColor, "Night".to_string(), BACKGROUNDS.len()),
                (SettingId::TextColor, "Frost".to_string(), COLORS.len()),
                (SettingId::Phosphor, "Off".to_string(), PHOSPHORS.len() + 1),
                (SettingId::CursorColor, "Cyan".to_string(), COLORS.len())
            ]
        );
        cfg.colors.text = "#ffc86e".into();
        cfg.colors.cursor = "#123456".into();
        cfg.colors.background = "000000".into();
        cfg.colors.phosphor = Some("#FFB000".into());
        assert_eq!(
            colors(&cfg),
            [
                (SettingId::BackgroundColor, "Black".to_string(), BACKGROUNDS.len()),
                (SettingId::TextColor, "Amber".to_string(), COLORS.len()),
                (SettingId::Phosphor, "Amber".to_string(), PHOSPHORS.len() + 1),
                (SettingId::CursorColor, "#123456".to_string(), COLORS.len() + 1)
            ]
        );
        // One that doesn't parse shows, and draws, as the default; a
        // phosphor of its own joins the end, and one that doesn't parse is
        // none at all.
        cfg.colors.text = "teal".into();
        cfg.colors.phosphor = Some("#204060".into());
        assert_eq!((colors(&cfg)[1].1.as_str(), colors(&cfg)[2].clone()), ("Frost", (SettingId::Phosphor, "#204060".to_string(), PHOSPHORS.len() + 2)));
        cfg.colors.phosphor = Some("teal".into());
        assert_eq!(colors(&cfg)[2].1, "Off");
    }

    #[test]
    fn programs_may_set_a_phosphor_or_none() {
        let takes = takes(&Config::default(), "colors.phosphor").unwrap();
        for (value, allowed) in [("#41FF6E", true), ("", true), ("green", false)] {
            assert_eq!(allows(&takes, &serde_json::json!(value)), allowed, "{value}");
        }
    }

    #[test]
    fn the_picker_picks_from_its_field_and_hue_bar() {
        let mut p = ColorPicker::new(SettingId::TextColor, "Text color", [0xFF, 0x00, 0x00]);
        let field = [100.0, 50.0, 200.0, 100.0];
        // Top right of the field: the hue at full strength.
        p.pick(PickerPart::Field, field, (300.0, 50.0));
        assert_eq!(p.color(), [0xFF, 0x00, 0x00]);
        // Left is unsaturated, bottom is black, and past the edges clamps.
        p.pick(PickerPart::Field, field, (0.0, 50.0));
        assert_eq!(p.color(), [0xFF, 0xFF, 0xFF]);
        p.pick(PickerPart::Field, field, (250.0, 900.0));
        assert_eq!(p.color(), [0, 0, 0]);
        // Black keeps the hue: back up the field, it's red again.
        p.pick(PickerPart::Field, field, (300.0, 50.0));
        p.pick(PickerPart::Hue, [100.0, 160.0, 300.0, 10.0], (250.0, 165.0));
        assert_eq!(p.color(), [0x00, 0xFF, 0xFF]);
        p.nudge(0.5, 0.0);
        assert_eq!(p.color(), [0xFF, 0x00, 0x00]);
        assert_eq!(p.shown(), "#FF0000");
    }

    #[test]
    fn a_typed_hex_value_is_picked_once_it_has_six_digits() {
        let mut p = ColorPicker::new(SettingId::CursorColor, "Cursor color", palette::CURSOR);
        for d in "12ab".chars() {
            assert!(!p.type_digit(d));
        }
        assert_eq!((p.shown().as_str(), p.color()), ("#12AB", palette::CURSOR));
        p.backspace();
        assert!(!p.type_digit('x') && !p.type_digit('c') && !p.type_digit('d'));
        assert!(p.type_digit('f'));
        assert_eq!((p.shown().as_str(), p.color()), ("#12ACDF", [0x12, 0xAC, 0xDF]));
        // The next digit starts a new value; backspace on a picked color edits it.
        assert!(!p.type_digit('8'));
        assert_eq!(p.shown(), "#8");
        p.typed = None;
        p.backspace();
        assert_eq!(p.shown(), "#12ACD");
        // A gray typed in keeps the picker's hue for when it's saturated again.
        let hue = p.hsv[0];
        p.set([0x80, 0x80, 0x80]);
        assert_eq!((p.hsv[0], p.color()), (hue, [0x80, 0x80, 0x80]));
    }

    #[test]
    fn the_menu_has_words_for_every_row_in_every_language() {
        for code in i18n::codes() {
            i18n::set_language(code, &[]);
            for style in STYLES {
                for provider in Provider::ALL {
                    for (has_key, crt) in [(true, 0.0), (false, 0.8)] {
                        let mut cfg = Config::default();
                        cfg.fx.style = style.into();
                        cfg.ai.provider = provider.id().into();
                        // With the CRT on and a saved look in use, every
                        // part of it shows, and Delete too; an old machine's
                        // switches, or your own theme's rows.
                        cfg.ui.look = if has_key { crate::looks::OWN } else { "green" }.into();
                        cfg.fx.crt = crt;
                        cfg.crt.presets.push(cfg.crt.preset("Mine", crt));
                        let entries = entries(&cfg, &FontChoices::default(), None, &[], false, has_key, Tally::default());
                        assert!(rows(&entries).iter().all(|r| !r.label.is_empty() && !r.hint.is_empty()));
                    }
                }
            }
            assert!(i18n::missed().is_empty(), "{code}: {:?}", i18n::missed());
        }
        // In Japanese, with the language names in their own words.
        i18n::set_language("ja", &[]);
        let entries = entries(&Config::default(), &FontChoices::default(), None, &[], false, true, Tally::default());
        let Some(Entry::Header(first)) = entries.first() else { panic!() };
        let language = rows(&entries).into_iter().find(|r| r.id == SettingId::Language).unwrap();
        let Control::Choice { options, index } = &language.control else { panic!() };
        assert_eq!((first.label, language.label), ("一般", "言語"));
        assert_eq!((options.to_vec(), *index), (vec!["自動".to_string(), "English".into(), "日本語".into()], 0));
        assert_eq!(speed_label(0.5), "0.5× 遅い");
    }

    #[test]
    fn collapse_all_leaves_the_headers_and_expand_all_every_setting() {
        let mut cfg = Config::default();
        let every = |cfg: &Config| entries(cfg, &FontChoices::default(), None, &[], false, true, Tally::default());
        assert_eq!(sections_shown(&every(&cfg)), (true, true), "some open, some closed at first");
        open_all(&mut cfg, false);
        let closed = every(&cfg);
        assert!(closed.iter().all(|e| matches!(e, Entry::Header(_))), "only the headers");
        assert_eq!(sections_shown(&closed), (false, true));
        open_all(&mut cfg, true);
        let open = every(&cfg);
        assert!(cfg.ui.closed_sections.is_empty());
        assert_eq!(sections_shown(&open), (true, false));
        assert!(open.len() > closed.len() * 4);
        // Every section is one Settings shows, in its order.
        let headers: Vec<SettingId> = open.iter().filter_map(|e| if let Entry::Header(r) = e { Some(r.id) } else { None }).collect();
        let all: Vec<SettingId> = Section::ALL.iter().map(|s| SettingId::Section(*s)).filter(|id| headers.contains(id)).collect();
        assert_eq!(headers, all);
    }

    #[test]
    fn sizes_typed_are_read_however_they_are_written() {
        assert_eq!(parse_sizes("132x43, 40x24"), [(132, 43), (40, 24)]);
        assert_eq!(parse_sizes("132 × 43; 40 X 24 and 100*30"), [(132, 43), (40, 24), (100, 30)]);
        // Too small, too large, half written or twice: passed over.
        assert_eq!(parse_sizes("5x5, 132x43, 900x40, 80x, x24, 132x43"), [(132, 43)]);
        assert_eq!(parse_sizes(""), []);
        assert_eq!(sizes_text(&parse_sizes("132 × 43;40X24")), "132x43, 40x24");
    }

    #[test]
    fn what_your_sizes_leaves_out_is_kept_for_its_note() {
        // Too small and too large, each once, in the order typed.
        let read = read_sizes("5x5, 132x43, 900x40, 5 x 5, 40x2");
        assert_eq!(read, SizesRead { sizes: vec![(132, 43)], outside: vec![(5, 5), (900, 40), (40, 2)], stray: false });
        // Half written, or a number alone: no size, but something left out.
        for typed in ["80x", "x24", "100", "132x43 and 7"] {
            let read = read_sizes(typed);
            assert!(read.stray && read.outside.is_empty(), "{typed}");
        }
        // Words, commas and a size twice leave nothing out.
        let read = read_sizes("132 x 43; and 40×24, 132x43");
        assert_eq!(read.sizes, [(132, 43), (40, 24)]);
        assert!(read.outside.is_empty() && !read.stray);
        assert_eq!(read_sizes(""), SizesRead::default());
        // The edges themselves fit.
        assert_eq!(read_sizes("10x3, 500x300").sizes, [(SIZE_COLS.0, SIZE_ROWS.0), (SIZE_COLS.1, SIZE_ROWS.1)]);
        assert_eq!(read_sizes("9x3, 10x2, 501x300, 500x301").outside.len(), 4);
    }

    #[test]
    fn your_sizes_note_says_what_was_left_out_and_why() {
        i18n::set_language("en", &[]);
        assert_eq!(sizes_note(&read_sizes("132x43, 40x24")), None);
        assert_eq!(sizes_note(&read_sizes("100 * 30; 5x5")).as_deref(), Some("Left out 5x5: a size runs from 10x3 to 500x300"));
        assert_eq!(sizes_note(&read_sizes("80x")).as_deref(), Some("Left out what isn't a size: type columns × rows, as 132x43"));
        let both = sizes_note(&read_sizes("5x5, 1x1, 2x2, 4x4, 80x")).unwrap();
        assert_eq!(both, "Left out 5x5, 1x1, 2x2, …, and what isn't columns × rows: a size runs from 10x3 to 500x300");
    }

    #[test]
    fn your_sizes_come_after_the_fit_rows_screens_without_repeating_them() {
        let mut cfg = Config::default();
        cfg.window.sizes = "132x43, 80x24, 40x24".into();
        assert_eq!(screens(&cfg), [(80, 24), (80, 25), (40, 25), (132, 43), (40, 24)]);
        // Both Fit rows offer them, the window's grid chosen where it's one.
        let grid = Some(TextGrid { cols: 132, rows: 43, cell: (9.0, 18.0), scale: 1.0 });
        for id in [SettingId::FitScreen, SettingId::FitText] {
            let Control::Choice { options, index } = row_of_with(&cfg, id, grid).control else { panic!("a choice") };
            assert_eq!((options.len(), index), (5, 3), "{id:?}");
            assert_eq!(options[3], "132 × 43");
        }
        // Typed in place, under the Fit rows.
        assert_eq!(row_text(&cfg, SettingId::OwnSizes), "132x43, 80x24, 40x24");
        set_row_text(&mut cfg, SettingId::OwnSizes, "100x30");
        assert_eq!(screens(&cfg).last(), Some(&(100, 30)));
        let open = entries(&cfg, &FontChoices::default(), None, &[], false, true, Tally::default());
        assert_eq!(section_of(&open, SettingId::OwnSizes), section_of(&open, SettingId::FitText));
    }

    #[test]
    fn the_recordings_countdown_offers_its_seconds_and_keeps_one_set_by_hand() {
        crate::i18n::set_language("en", &[]);
        let (labels, index) = seconds_options(&crate::record::COUNTDOWNS, 3);
        assert_eq!((labels.len(), index), (4, 1));
        assert_eq!(labels[0], t("common.off"));
        // A countdown written in the config by hand is shown as it is.
        let (labels, index) = seconds_options(&crate::record::COUNTDOWNS, 7);
        assert_eq!((labels.len(), index), (5, 4));
        // The Recording section has it, its choice where the config says.
        let mut cfg = Config::default();
        cfg.record.countdown = 5;
        let open = entries(&cfg, &FontChoices::default(), None, &[], false, true, Tally::default());
        assert_eq!(section_of(&open, SettingId::RecordCountdown), Some(Section::Recording));
        assert_eq!(choice_row(&cfg, SettingId::RecordCountdown, Tally::default()).unwrap().1, 2);
        assert_eq!(takes(&cfg, "record.countdown"), Some(serde_json::json!({ "one_of": [0, 3, 5, 10] })));
    }

    #[test]
    fn a_closed_section_keeps_its_header_and_hides_its_settings() {
        let mut cfg = Config::default();
        let headers = |cfg: &Config| -> Vec<(SettingId, bool, usize)> {
            entries(cfg, &FontChoices::default(), None, &[], false, true, Tally::default())
                .into_iter()
                .filter_map(|e| match e {
                    Entry::Header(Row { id, control: Control::Section { open, count, .. }, .. }) => Some((id, open, count)),
                    _ => None,
                })
                .collect()
        };
        // Every section has a header, the CRT's, the keys', the apps' and
        // the sounds' closed at first.
        let first = headers(&cfg);
        assert_eq!(first.len(), 16);
        assert_eq!(
            first.iter().filter(|(_, open, _)| !open).map(|(id, ..)| *id).collect::<Vec<_>>(),
            [SettingId::Section(Section::Crt), SettingId::Section(Section::Keys), SettingId::Section(Section::Apps), SettingId::Section(Section::Sound)]
        );
        // Skills holds the folder's row alone while there are none, and MCP
        // servers the file's.
        assert!(first.iter().all(|(id, _, count)| *count >= 2 || [SettingId::Section(Section::Skills), SettingId::Section(Section::McpServers)].contains(id)));
        assert!(first.iter().any(|(id, ..)| *id == SettingId::Section(Section::OutsideApps)));
        // Closed, the Theme section's rows go and its header says how many.
        let all = entries(&cfg, &FontChoices::default(), None, &[], false, true, Tally::default()).len();
        Section::Theme.set_open(&mut cfg, false);
        let theme = headers(&cfg).into_iter().find(|(id, ..)| *id == SettingId::Section(Section::Theme)).unwrap();
        let fewer = entries(&cfg, &FontChoices::default(), None, &[], false, true, Tally::default());
        assert_eq!((theme.1, fewer.len()), (false, all - theme.2));
        assert!(rows(&fewer).iter().all(|r| r.id != SettingId::Theme));
        // Kept by name, and opened again.
        assert!(cfg.ui.closed_sections.contains(&"theme".to_string()));
        Section::Theme.set_open(&mut cfg, true);
        assert!(Section::Theme.open(&cfg) && !cfg.ui.closed_sections.contains(&"theme".to_string()));
        // A setting's section is found with every section open.
        cfg.ui.closed_sections.clear();
        let open = entries(&cfg, &FontChoices::default(), None, &[], false, true, Tally::default());
        assert_eq!(section_of(&open, SettingId::Scanlines), None, "the CRT's parts show only while it's on");
        assert_eq!(section_of(&open, SettingId::Bell), Some(Section::Sound));
        assert_eq!(section_of(&open, SettingId::ApiKey), Some(Section::Assistant));
    }

    #[test]
    fn the_skills_section_lists_every_skill_a_packages_off_until_turned_on() {
        crate::i18n::set_language("en", &[]);
        let mut cfg = Config::default();
        open_all(&mut cfg, true);
        let tally = || Tally { skills: SkillsView::made_up().into(), ..Tally::default() };
        let shown = |cfg: &Config| -> Vec<(&'static str, Control)> {
            let all = entries(cfg, &FontChoices::default(), None, &[], false, true, tally());
            assert_eq!(section_of(&all, SettingId::Skills), Some(Section::Skills), "a section of its own");
            rows(&all)
                .into_iter()
                .filter(|r| matches!(r.id, SettingId::Skill(_) | SettingId::SkillPackage(_) | SettingId::PackageSkill(_)))
                .map(|r| (r.label, r.control.clone()))
                .collect()
        };
        // Every skill there is, the user's own on and each package's off,
        // from the start: nothing hidden behind a switch, and nothing on
        // that wasn't turned on.
        let first = shown(&cfg);
        let labels: Vec<&str> = first.iter().map(|(l, _)| *l).collect();
        assert_eq!(
            labels,
            [
                "deploy-site",
                "fix-wifi",
                "release-notes",
                "windows-drivers",
                "Skills from flow-skills",
                "flow-skills:flow-settings",
                "flow-skills:flowapp",
                "flow-skills:sdk-programs",
                "flow-skills:theme-package",
                "Skills from flow-utils",
                "flow-utils:tree",
            ]
        );
        let control = |rows: &[(&str, Control)], label: &str| rows.iter().find(|(l, _)| *l == label).map(|(_, c)| c.clone()).unwrap();
        assert_eq!(control(&first, "deploy-site"), Control::Toggle(true));
        assert_eq!(control(&first, "flow-skills:flowapp"), Control::Toggle(false));
        let Control::Item { text, actions } = control(&first, "Skills from flow-skills") else { panic!() };
        assert_eq!((text.as_str(), actions.as_slice()), ("0 of 4 on", ["All on", "All off"].as_slice()));
        // Each turned on or off by the name the assistant knows it by.
        cfg.ai.skills_on = vec!["flow-skills:flowapp".into(), "flow-skills:theme-package".into()];
        cfg.ai.skills_off = vec!["fix-wifi".into()];
        let then = shown(&cfg);
        assert_eq!(control(&then, "fix-wifi"), Control::Toggle(false));
        assert_eq!(control(&then, "flow-skills:flowapp"), Control::Toggle(true));
        assert_eq!(control(&then, "flow-skills:sdk-programs"), Control::Toggle(false));
        let Control::Item { text, .. } = control(&then, "Skills from flow-skills") else { panic!() };
        assert_eq!(text, "2 of 4 on");
        // A package's skill's hint says where it came from.
        let all = entries(&cfg, &FontChoices::default(), None, &[], false, true, tally());
        let flowapp = rows(&all).into_iter().find(|r| r.label == "flow-skills:flowapp").unwrap();
        assert!(flowapp.hint.starts_with("From flow-skills:") && flowapp.hint.contains("Using flowapp"), "{}", flowapp.hint);
        assert_eq!(config_key(SettingId::Skill(0)).as_deref(), Some("ai.skills_off"));
        assert_eq!(config_key(SettingId::PackageSkill(0)).as_deref(), Some("ai.skills_on"));
        assert_eq!(config_key(SettingId::SkillPackage(0)).as_deref(), Some("ai.skills_on"));
        // With no skills at all, the section is the folder's row alone.
        let none = entries(&cfg, &FontChoices::default(), None, &[], false, true, Tally::default());
        assert!(!rows(&none).iter().any(|r| matches!(r.id, SettingId::Skill(_) | SettingId::SkillPackage(_) | SettingId::PackageSkill(_))));
    }

    #[test]
    fn outside_apps_has_its_switch_how_to_connect_and_each_app_remembered() {
        crate::i18n::set_language("en", &[]);
        let mut cfg = Config::default();
        open_all(&mut cfg, true);
        let rows_with = |cfg: &Config, view: OutsideView| -> Vec<(SettingId, &'static str, Control)> {
            let all = entries(cfg, &FontChoices::default(), None, &[], false, true, Tally { outside: view.into(), ..Tally::default() });
            assert_eq!(section_of(&all, SettingId::McpEnabled), Some(Section::OutsideApps));
            rows(&all)
                .into_iter()
                .filter(|r| matches!(r.id, SettingId::McpEnabled | SettingId::McpConnect | SettingId::McpApp(_) | SettingId::McpKind(..) | SettingId::McpCommands(_)))
                .map(|r| (r.id, r.label, r.control.clone()))
                .collect()
        };
        // Off at first, and with no app remembered, the switch and the path.
        let none = rows_with(&cfg, OutsideView::default());
        assert_eq!(none.len(), 2);
        assert_eq!(none[0].2, Control::Toggle(false), "off until turned on");
        assert!(matches!(&none[1].2, Control::Item { actions, .. } if actions.is_empty()), "no flow-mcp, nothing to copy");
        assert_eq!(config_key(SettingId::McpEnabled).as_deref(), Some("mcp.enabled"));
        // Each app: its row, then a switch for each kind and how it runs commands.
        cfg.mcp.enabled = true;
        let shown = rows_with(&cfg, OutsideView::made_up());
        assert_eq!(shown[0].2, Control::Toggle(true));
        assert!(matches!(&shown[1].2, Control::Item { text, actions } if text.ends_with("flow-mcp") && actions.len() == 2));
        let first: Vec<SettingId> = shown[2..7].iter().map(|r| r.0).collect();
        assert_eq!(first, [SettingId::McpApp(0), SettingId::McpKind(0, 0), SettingId::McpKind(0, 1), SettingId::McpKind(0, 2), SettingId::McpCommands(0)]);
        assert_eq!(shown[2].1, "claude-code");
        assert_eq!([&shown[3].2, &shown[4].2, &shown[5].2], [&Control::Toggle(true), &Control::Toggle(true), &Control::Toggle(false)]);
        assert!(matches!(&shown[6].2, Control::Choice { index: 1, options } if options.len() == 3), "commands asked each time");
        assert_eq!(shown.len(), 2 + 5 * 2);
        // What each app may do is kept apart from config.toml.
        assert_eq!(config_key(SettingId::McpKind(0, 0)), None);
    }

    /// MCP servers: the file's row, then each server's: how it stands, its
    /// command or URL, its approval, and once connected its tools, each off
    /// until turned on.
    #[test]
    fn mcp_servers_lists_each_server_and_its_tools_each_off_until_turned_on() {
        crate::i18n::set_language("en", &[]);
        let mut cfg = Config::default();
        open_all(&mut cfg, true);
        let rows_with = |cfg: &Config, view: ServersView| -> Vec<(SettingId, &'static str, Control)> {
            let all = entries(cfg, &FontChoices::default(), None, &[], false, true, Tally { servers: view.into(), ..Tally::default() });
            assert_eq!(section_of(&all, SettingId::ServersFile), Some(Section::McpServers));
            rows(&all)
                .into_iter()
                .filter(|r| {
                    use SettingId::*;
                    matches!(r.id, ServersFile | Server(_) | ServerHow(_) | ServerAuth(_) | ServerResources(_) | ServerApproval(_) | ServerTools(_) | ServerTool(..))
                })
                .map(|r| (r.id, r.label, r.control.clone()))
                .collect()
        };
        // None in the file: its row alone, with Add server… and Open.
        let none = rows_with(&cfg, ServersView::default());
        assert_eq!(none.len(), 1);
        assert!(matches!(&none[0].2, Control::Item { actions, .. } if actions == &["Add server…", "Open file"]));
        // The first server connected with three tools, none on, and
        // resources and a prompt: its row, its command typed in place, its
        // resources' switch, Ask each time, then its tools. The second, a
        // remote one waiting for a sign-in, has its sign-in's row.
        let shown = rows_with(&cfg, ServersView::made_up());
        let ids: Vec<SettingId> = shown.iter().map(|r| r.0).collect();
        assert_eq!(
            ids,
            [
                SettingId::ServersFile,
                SettingId::Server(0),
                SettingId::ServerHow(0),
                SettingId::ServerResources(0),
                SettingId::ServerApproval(0),
                SettingId::ServerTools(0),
                SettingId::ServerTool(0, 0),
                SettingId::ServerTool(0, 1),
                SettingId::ServerTool(0, 2),
                SettingId::Server(1),
                SettingId::ServerHow(1),
                SettingId::ServerAuth(1),
                SettingId::ServerApproval(1),
            ]
        );
        assert!(matches!(&shown[1].2, Control::Item { text, actions } if text == "0 of 3 tools on" && actions == &["Disconnect", "Remove"]));
        assert!(matches!(&shown[2].2, Control::Text { value, multiline: false, .. } if value == "npx -y @example/notes-mcp"));
        assert_eq!(shown[3].2, Control::Toggle(false), "its resources off until turned on");
        assert!(matches!(&shown[4].2, Control::Choice { index: 0, options } if options.len() == 3), "asked each time at first");
        assert!(shown[6..9].iter().all(|r| r.2 == Control::Toggle(false)), "each tool off until turned on");
        // The remote one says it needs a sign-in, and its sign-in's row
        // has Sign in, Client ID… and Token….
        assert!(matches!(&shown[9].2, Control::Item { text, actions } if text == "needs you to sign in" && actions == &["Disconnect", "Remove"]));
        assert!(matches!(&shown[11].2, Control::Item { text, actions } if text == "needed" && actions == &["Sign in", "Client ID…", "Token…"]));
        // Turned on, a tool's switch is on and the count says so; the
        // approval is the one chosen; its resources are on.
        cfg.ai.mcp_tools_on = vec!["notes:read_note".into()];
        cfg.ai.mcp_approval.insert("notes".into(), "read_only".into());
        cfg.ai.mcp_resources_on = vec!["notes".into()];
        let on = rows_with(&cfg, ServersView::made_up());
        assert_eq!(on[7].2, Control::Toggle(true));
        assert!(matches!(&on[5].2, Control::Item { text, .. } if text == "1 of 3 on"));
        assert!(matches!(&on[4].2, Control::Choice { index: 1, .. }));
        assert_eq!(on[3].2, Control::Toggle(true));
        // Signed in, its first button is Sign out; signing in, Cancel.
        let mut view = ServersView::made_up();
        view.servers[1].signed_in = true;
        view.servers[1].standing = ServerStanding::Connected;
        assert_eq!(server_auth_acts(&view.servers[1])[0], ServerAct::SignOut);
        view.servers[1].standing = ServerStanding::SigningIn;
        assert_eq!(server_auth_acts(&view.servers[1])[0], ServerAct::CancelSignIn);
        assert_eq!(config_key(SettingId::ServerTool(0, 0)).as_deref(), Some("ai.mcp_tools_on"));
        assert_eq!(config_key(SettingId::Server(0)).as_deref(), Some("ai.mcp_servers_on"));
        assert_eq!(config_key(SettingId::ServerResources(0)).as_deref(), Some("ai.mcp_resources_on"));
        // The command and the sign-in are kept in mcp-servers.json and the
        // credential store, not the config.
        assert_eq!(config_key(SettingId::ServerHow(0)), None);
        assert_eq!(config_key(SettingId::ServerAuth(0)), None);
    }

    #[test]
    fn crt_presets_apply_and_name_the_look_in_use() {
        let mut cfg = Config::default();
        Section::Crt.set_open(&mut cfg, true);
        let presets = crt_presets(&cfg);
        let name = |cfg: &Config| {
            let presets = crt_presets(cfg);
            current_crt_preset(&presets, &cfg.crt, cfg.fx.crt).map(|i| presets[i].0.clone())
        };
        // The default parts at 80% are Classic.
        cfg.fx.crt = 0.8;
        assert_eq!(name(&cfg).as_deref(), Some("Classic"));
        // Picking one takes on its parts and strength.
        let arcade = &presets.iter().find(|(_, p, _)| p.name == "arcade").unwrap().1;
        cfg.crt.apply(arcade);
        cfg.fx.crt = arcade.strength;
        assert_eq!((name(&cfg).as_deref(), cfg.crt.curvature), (Some("Arcade"), 0.6));
        // Changed by hand, it's Custom, shown after the presets.
        cfg.crt.scanlines = 0.33;
        assert_eq!(name(&cfg), None);
        let entries = entries(&cfg, &FontChoices::default(), None, &[], false, true, Tally::default());
        let preset_row = rows(&entries).into_iter().find(|r| r.id == SettingId::CrtPreset).unwrap();
        let Control::Choice { options, index } = &preset_row.control else { panic!() };
        assert_eq!((options[*index].as_str(), options.len()), ("Custom", builtin_crt_presets().len() + 1));
        // Saved under a name, it's theirs, ahead of a built-in look the same.
        cfg.crt.presets.push(cfg.crt.preset("Amber terminal", cfg.fx.crt));
        assert_eq!(name(&cfg).as_deref(), Some("Amber terminal"));
        cfg.crt.apply(arcade);
        cfg.crt.presets.push(cfg.crt.preset("Also arcade", cfg.fx.crt));
        assert_eq!(name(&cfg).as_deref(), Some("Also arcade"));
    }

    /// Every row `cfg` gives, its sections all open.
    fn ids(cfg: &Config) -> Vec<SettingId> {
        let mut cfg = cfg.clone();
        cfg.ui.closed_sections.clear();
        let entries = entries(&cfg, &FontChoices::default(), None, &[], false, true, Tally::default());
        rows(&entries).iter().map(|r| r.id).collect()
    }

    fn row_of(cfg: &Config, id: SettingId) -> Row {
        row_of_with(cfg, id, None)
    }

    /// `id`'s row while the window holds `grid`.
    fn row_of_with(cfg: &Config, id: SettingId, grid: Option<TextGrid>) -> Row {
        let mut cfg = cfg.clone();
        cfg.ui.closed_sections.clear();
        let entries = entries(&cfg, &FontChoices::default(), grid, &[], false, true, Tally::default());
        rows(&entries).into_iter().find(|r| r.id == id).cloned().unwrap_or_else(|| panic!("no {id:?} row"))
    }

    #[test]
    fn theme_details_offer_a_machines_parts_and_only_the_sounds_it_makes() {
        use crate::looks::Part;
        let mut cfg = Config::default();
        assert!(!ids(&cfg).contains(&SettingId::Section(Section::ThemeDetails)), "Flow's look has none");
        crate::looks::find("pc").apply(&mut cfg);
        let pc = ids(&cfg);
        for id in [SettingId::ThemeFont, SettingId::ThemeCrt, SettingId::ThemeColors, SettingId::ThemeSound(Part::Fan), SettingId::ThemeSound(Part::Hum)] {
            assert!(pc.contains(&id), "{id:?}");
        }
        assert!(!pc.contains(&SettingId::ThemeSound(Part::Punch)), "a PC has no tape punch");
        // The fan turned off on the PC, and the teletype's sounds left alone.
        cfg.sound.muted.push(Part::Fan.key("pc"));
        assert!(matches!(row_of(&cfg, SettingId::ThemeSound(Part::Fan)).control, Control::Toggle(false)));
        assert!(matches!(row_of(&cfg, SettingId::ThemeSound(Part::Hum)).control, Control::Toggle(true)));
        crate::looks::find("teletype").apply(&mut cfg);
        assert!(matches!(row_of(&cfg, SettingId::ThemeSound(Part::Punch)).control, Control::Toggle(true)));
        // No tube to turn off where the machine had none.
        crate::looks::find("workstation").apply(&mut cfg);
        assert!(!ids(&cfg).contains(&SettingId::ThemeCrt) && ids(&cfg).contains(&SettingId::ThemeColors));
        // It starts closed, out of the way.
        assert!(!Section::ThemeDetails.open(&Config::default()));
        // A window frame where the machine's desktop drew one alone.
        assert!(!ids(&cfg).contains(&SettingId::ThemeFrame));
        crate::looks::find("amiga").apply(&mut cfg);
        assert!(matches!(row_of(&cfg, SettingId::ThemeFrame).control, Control::Toggle(true)));
    }

    #[test]
    fn your_own_theme_shows_its_start_screen_prompt_speed_sounds_and_frame_in_place_of_the_switches() {
        let mut cfg = Config::default();
        let own = [SettingId::OwnStart, SettingId::OwnPrompt, SettingId::OwnBaud, SettingId::OwnSounds, SettingId::OwnFrame];
        let switches = [SettingId::ThemeStart, SettingId::ThemePrompt];
        let flow = ids(&cfg);
        assert!(own.iter().chain(&switches).all(|id| !flow.contains(id)), "Flow's look has neither");
        crate::looks::find("green").apply(&mut cfg);
        let green = ids(&cfg);
        let at = green.iter().position(|id| *id == SettingId::Theme).unwrap();
        let details = green.iter().position(|id| *id == SettingId::Section(Section::ThemeDetails)).unwrap();
        assert_eq!(green[details + 1..details + 3], switches, "an old machine's switches open its details");
        assert!(own.iter().all(|id| !green.contains(id)));
        // Your own, begun from the green screen: its rows right under Theme.
        cfg.theme = crate::looks::find("green").as_theme();
        cfg.ui.look = crate::looks::OWN.into();
        let mine = ids(&cfg);
        assert_eq!(mine[at + 1..at + 6], own);
        assert!(switches.iter().all(|id| !mine.contains(id)));
        let Control::Text { value, multiline: true, .. } = row_of(&cfg, SettingId::OwnStart).control else { panic!("a box of lines") };
        assert_eq!(value, "FLOW-11 MINICOMPUTER\n{memory} OF CORE\n\nBOOTING TIME-SHARING SYSTEM ...\n");
        let Control::Text { value, multiline: false, .. } = row_of(&cfg, SettingId::OwnPrompt).control else { panic!("a line") };
        assert_eq!(value, "$ ");
        let Control::Choice { options, index } = row_of(&cfg, SettingId::OwnBaud).control else { panic!() };
        assert_eq!((options[index].as_str(), options[0].as_str(), options.len()), ("1200", "At once", BAUDS.len()));
        let Control::Choice { options, index } = row_of(&cfg, SettingId::OwnSounds).control else { panic!() };
        assert_eq!((options[index].as_str(), options.len()), ("Video terminal", THEME_SOUNDS.len()), "the green screen's");
        // A sound Flow doesn't know is quiet, and a speed set by hand stays.
        cfg.theme.sounds = "organ".into();
        cfg.theme.baud = 110;
        let Control::Choice { options, index } = row_of(&cfg, SettingId::OwnSounds).control else { panic!() };
        assert_eq!(options[index], "Quiet");
        let Control::Choice { options, index } = row_of(&cfg, SettingId::OwnBaud).control else { panic!() };
        assert_eq!((options[index].as_str(), options.len()), ("110", BAUDS.len() + 1));
        // No frame from the green screen, whose desktop drew none; the
        // notanAmiga's from its.
        assert!(matches!(row_of(&cfg, SettingId::OwnFrame).control, Control::Toggle(false)));
        cfg.theme = crate::looks::find("amiga").as_theme();
        assert!(matches!(row_of(&cfg, SettingId::OwnFrame).control, Control::Toggle(true)));
    }

    #[test]
    fn saved_themes_come_after_the_built_in_ones_and_the_one_chosen_can_be_deleted() {
        use crate::looks::{save_as, wear, LOOKS};
        let mut cfg = Config::default();
        let actions = |cfg: &Config| match row_of(cfg, SettingId::SavedTheme).control {
            Control::Actions(actions) => actions,
            _ => panic!("buttons"),
        };
        assert_eq!(actions(&cfg), ["Save as theme…"], "nothing saved to delete");
        let mut night = cfg.clone();
        night.fx.style = "rain".into();
        cfg.themes.save(save_as(&night, "Night"));
        cfg.themes.save(save_as(&Config::default(), "Day"));
        // Chosen in Settings, it's worn and shown chosen, after your own,
        // with its start screen and prompt to type as your own's are.
        let cfg = wear(&cfg, "saved:Night");
        let Control::Choice { options, index } = row_of(&cfg, SettingId::Theme).control else { panic!() };
        assert_eq!((options.len(), options[LOOKS.len()].as_str(), options[index].as_str(), index), (LOOKS.len() + 3, "Your own", "Night", LOOKS.len() + 1));
        assert_eq!(cfg.fx.style, "rain");
        assert_eq!(actions(&cfg), ["Save as theme…", "Export…", "Delete"]);
        let rows = ids(&cfg);
        let at = rows.iter().position(|id| *id == SettingId::Theme).unwrap();
        assert_eq!(rows[at + 1..at + 7], [SettingId::OwnStart, SettingId::OwnPrompt, SettingId::OwnBaud, SettingId::OwnSounds, SettingId::OwnFrame, SettingId::SavedTheme]);
        // What's typed there goes into it, and your own stays as it was.
        let mut cfg = cfg;
        cfg.theme.prompt = "mine> ".into();
        assert_eq!(row_text(&cfg, SettingId::OwnPrompt), "");
        set_row_text(&mut cfg, SettingId::OwnPrompt, "night> ");
        assert_eq!((cfg.themes.named("Night").unwrap().theme.prompt.as_str(), cfg.theme.prompt.as_str()), ("night> ", "mine> "));
        let Control::Text { value, .. } = row_of(&cfg, SettingId::OwnPrompt).control else { panic!() };
        assert_eq!(value, "night> ");
        // A name taken is saved over, which the box says.
        let mut naming = Naming::theme(&cfg, cfg.clone());
        assert_eq!((naming.words(), Naming::crt(&cfg).words()), ("theme_name", "crt_name"));
        naming.name = " Night ".into();
        assert!(naming.over() && naming.chosen() == "Night");
        naming.name = "Dusk".into();
        assert!(!naming.over());
    }

    #[test]
    fn what_is_typed_in_the_text_rows_goes_into_your_own_theme() {
        let mut cfg = Config::default();
        set_row_text(&mut cfg, SettingId::OwnStart, "^MY MACHINE\n{memory} FREE\n");
        assert_eq!(cfg.theme.start, ["^MY MACHINE", "{memory} FREE", ""]);
        assert_eq!(row_text(&cfg, SettingId::OwnStart), "^MY MACHINE\n{memory} FREE\n");
        // Nothing typed is no start screen at all.
        set_row_text(&mut cfg, SettingId::OwnStart, "");
        assert!(cfg.theme.start.is_empty());
        // A prompt is typed on one line, its new lines as \n.
        set_row_text(&mut cfg, SettingId::OwnPrompt, "READY.\\n{n}> ");
        assert_eq!(cfg.theme.prompt, "READY.\n{n}> ");
        assert_eq!(row_text(&cfg, SettingId::OwnPrompt), "READY.\\n{n}> ");
        // A backslash of its own stays one.
        set_row_text(&mut cfg, SettingId::OwnPrompt, "C:\\>");
        assert_eq!((cfg.theme.prompt.as_str(), row_text(&cfg, SettingId::OwnPrompt).as_str()), ("C:\\>", "C:\\>"));
        // Other rows hold no text.
        assert_eq!(row_text(&cfg, SettingId::Font), "");
    }

    #[test]
    fn a_preset_carries_the_theme_typed_in_settings_and_brings_it_back_whole() {
        let mut cfg = Config::default();
        cfg.ui.look = crate::looks::OWN.into();
        let mut field = TextField::new("", true);
        field.insert("^FLOW SYSTEM ONE\n\nREADY FOR {shell}");
        set_row_text(&mut cfg, SettingId::OwnStart, &field.text());
        set_row_text(&mut cfg, SettingId::OwnPrompt, "{cwd}\\n> ");
        cfg.theme.baud = BAUDS[2];
        cfg.theme.sounds = THEME_SOUNDS[7].into();
        let json = cfg.preset_json();
        let mut back = Config::default();
        back.apply_preset_json(&json).unwrap();
        assert_eq!(back.theme, cfg.theme);
        assert_eq!(back.ui.look, crate::looks::OWN, "a start screen of its own chooses your own theme");
        assert_eq!(row_text(&back, SettingId::OwnStart), "^FLOW SYSTEM ONE\n\nREADY FOR {shell}");
        assert_eq!(row_text(&back, SettingId::OwnPrompt), "{cwd}\\n> ");
        assert_eq!(back.theme.prompt, "{cwd}\n> ");
    }

    #[test]
    fn word_jumps_pass_spaces_and_marks_then_a_word_and_cross_lines() {
        let mut f = TextField::new("{cwd} is here\nREADY.", true);
        f.place(0, 13);
        f.word_left();
        assert_eq!(f.caret(), (0, 9));
        f.word_left();
        assert_eq!(f.caret(), (0, 6));
        f.word_left();
        assert_eq!(f.caret(), (0, 1), "past the space and the brace, to the start of cwd");
        f.word_left();
        assert_eq!(f.caret(), (0, 0));
        f.word_left();
        assert_eq!(f.caret(), (0, 0), "nothing before the first line");
        f.word_right();
        assert_eq!(f.caret(), (0, 4));
        f.place(0, 13);
        f.word_right();
        assert_eq!(f.caret(), (1, 5), "to the end of the next line's first word");
        f.word_left();
        f.word_left();
        assert_eq!(f.caret(), (0, 9), "from a line's start, to the last word above");
        f.place(1, 6);
        f.word_right();
        assert_eq!(f.caret(), (1, 6), "nothing after the last line");
    }

    #[test]
    fn word_deletes_keep_to_the_line_and_join_lines_at_its_ends() {
        let mut f = TextField::new("ONE TWO, THREE\nFOUR", true);
        f.place(0, 7);
        f.backspace_word();
        assert_eq!((f.text().as_str(), f.caret()), ("ONE , THREE\nFOUR", (0, 4)));
        f.delete_word();
        assert_eq!((f.text().as_str(), f.caret()), ("ONE \nFOUR", (0, 4)), "the comma, the space and THREE");
        f.delete_word();
        assert_eq!((f.text().as_str(), f.caret()), ("ONE FOUR", (0, 4)), "at the end, the next line comes up");
        f.place(0, 0);
        f.backspace_word();
        assert_eq!((f.text().as_str(), f.caret()), ("ONE FOUR", (0, 0)));
    }

    #[test]
    fn a_click_puts_the_caret_within_the_text() {
        let mut f = TextField::new("AB\nCDEF", true);
        f.place(0, 1);
        assert_eq!(f.caret(), (0, 1));
        f.place(0, 9);
        assert_eq!(f.caret(), (0, 2), "past a line's end is its end");
        f.place(7, 3);
        assert_eq!(f.caret(), (1, 3), "below the last line is in it");
        f.up();
        f.down();
        assert_eq!(f.caret(), (1, 3), "↑ and ↓ keep to the column clicked");
    }

    #[test]
    fn typing_goes_in_at_the_caret_and_backspace_and_delete_join_lines() {
        let mut f = TextField::new("HELLO", true);
        assert_eq!(f.caret(), (0, 5));
        f.left();
        f.left();
        f.insert("é");
        assert_eq!((f.text().as_str(), f.caret()), ("HELéLO", (0, 4)));
        f.backspace();
        f.new_line();
        assert_eq!((f.text().as_str(), f.caret()), ("HEL\nLO", (1, 0)));
        // At a line's start, Backspace joins it to the line above.
        f.backspace();
        assert_eq!((f.text().as_str(), f.caret()), ("HELLO", (0, 3)));
        f.new_line();
        f.up();
        f.end();
        // At a line's end, Delete brings the next line up.
        f.delete();
        assert_eq!((f.text().as_str(), f.caret()), ("HELLO", (0, 3)));
        f.delete();
        assert_eq!(f.text(), "HELO");
        f.backspace_line();
        assert_eq!((f.text().as_str(), f.caret()), ("O", (0, 0)));
    }

    #[test]
    fn enter_makes_a_new_line_only_in_a_field_of_several() {
        let mut lines = TextField::new("A", true);
        let mut line = TextField::new("A", false);
        for f in [&mut lines, &mut line] {
            f.new_line();
            f.insert("B\r\nC\tD\u{7}");
        }
        assert_eq!((lines.text().as_str(), lines.lines().len()), ("A\nB\nC D", 3));
        assert_eq!((line.text().as_str(), line.lines().len()), ("ABC D", 1));
        assert!(TextField::new("one\ntwo", false).lines().len() == 1);
        // A long line, as a command with its paths is, takes more.
        assert_eq!(TextField::long(&"x".repeat(500)).text().len(), 500);
    }

    #[test]
    fn the_arrows_cross_lines_and_up_and_down_keep_their_column() {
        let mut f = TextField::new("FLOW-11 MINICOMPUTER\n\nBOOTING", true);
        f.top();
        assert_eq!(f.caret(), (0, 0));
        f.end();
        // Down through an empty line and on, the column comes back.
        f.down();
        assert_eq!(f.caret(), (1, 0));
        f.down();
        assert_eq!(f.caret(), (2, 7));
        f.up();
        f.up();
        assert_eq!(f.caret(), (0, 20));
        // Past the first line's start goes nowhere, and ← wraps to the line before.
        f.up();
        assert_eq!(f.caret(), (0, 0));
        f.left();
        assert_eq!(f.caret(), (0, 0));
        f.down();
        f.left();
        assert_eq!(f.caret(), (0, 20));
        f.right();
        f.right();
        assert_eq!(f.caret(), (2, 0));
        f.down();
        assert_eq!(f.caret(), (2, 7), "below the last line is its end");
        f.home();
        assert_eq!(f.caret(), (2, 0));
        f.bottom();
        assert_eq!(f.caret(), (2, 7));
    }

    #[test]
    fn a_field_holds_no_more_than_a_screenful() {
        let mut f = TextField::new(&"x".repeat(500), true);
        assert_eq!(f.lines()[0].len(), TextField::MAX_CHARS);
        f.insert(&"\n".repeat(100));
        assert_eq!(f.lines().len(), TextField::MAX_LINES);
    }

    #[test]
    fn rain_labels_go_from_a_drizzle_to_a_downpour() {
        let said: Vec<String> = [0.0, 0.15, 0.4, 0.6, 1.0].iter().map(|&a| rain_label(a)).collect();
        assert_eq!(said, ["Off", "Drizzle", "Rain", "Heavy rain", "Downpour"]);
    }

    #[test]
    fn speed_labels() {
        assert_eq!(speed_label(0.2), "0.2× very slow");
        assert_eq!(speed_label(0.5), "0.5× slow");
        assert_eq!(speed_label(1.0), "1.0× normal");
    }
}
