//! Events delivered to the UI thread through winit's event loop proxy.

use crate::ai::AiEvent;

pub type SessionId = u32;

#[derive(Debug)]
pub enum UserEvent {
    /// New output (or a state change) in a terminal session.
    Wake,
    ChildExited(SessionId),
    /// OSC 52 clipboard write from a program in the terminal.
    Clipboard(String),
    Ai(AiEvent),
    /// One of the assistant's helpers, by its number.
    Helper(u32, AiEvent),
    /// The updater wants the app closed so it can install a new version.
    Quit,
    /// The updater Flow shows (Linux's) has moved on: see
    /// `Platform::update_state`.
    Update,
    /// A menu command (the macOS menu bar and context menu).
    Command(Command),
    /// Text the system put in without a keystroke (an emoji from macOS's
    /// Emoji & Symbols, dictation): typed where typing goes.
    Insert(String),
    /// A 3D model the user allowed from a file couldn't be shown, and why.
    ModelFailed(String),
    /// How the computer is powered, read on a thread of its own, for
    /// efficient mode to follow (`Platform::power`).
    Power(crate::power::Power),
    /// Each tab's programs and Flow's own process, read on a thread of its
    /// own for what each tab uses (`Platform::processes`).
    Usage(Vec<(SessionId, crate::resources::Reading)>, crate::resources::Reading),
    /// The games an older Flow carried, installed by `flowapp`'s code on
    /// the first start after an update (`App::keep_carried_apps`), and
    /// those something installed was in the way of.
    AppsKept(crate::apps::CarriedKept),
    /// An app Settings' Apps installed, upgraded or removed on a thread of
    /// its own: the version installed, or why it failed.
    AppsDone { name: String, doing: crate::apps::shelf::Doing, result: Result<String, String> },
    /// A provider's own list of models, asked for on a thread of its own
    /// as `/model` opened (`ai::models::fetch`), or why it didn't come.
    ModelsListed { provider: crate::ai::provider::Provider, result: Result<Vec<crate::ai::models::Listed>, String> },
    /// A recording of the terminal written to the file the save panel
    /// named, on a thread of its own, or why it wasn't; no file named, the
    /// panel was cancelled and the recording is let go.
    RecordingSaved { path: Option<std::path::PathBuf>, result: Result<(), String> },
    /// flowapp's catalog read afresh (`App::check_apps`): the apps
    /// installed that have a newer version this Flow can take, or why it
    /// didn't read. `asked`: Check now asked for it.
    AppsChecked { newer: Result<Vec<String>, String>, asked: bool },
    /// A package of themes the gallery asked about, read on a thread of its
    /// own (fetched and checked where it isn't installed): its themes, or
    /// why not.
    GalleryRead { name: String, themes: Result<crate::gallery::Preview, String> },
    /// A file read for the assistant on a thread of its own
    /// (`ai::files::read`), by the name its chip showed while it was read,
    /// or why it couldn't be, in words.
    FileRead { name: String, read: Result<Box<crate::ai::files::Attached>, String> },
    /// The system's notification of a long command done was clicked: the
    /// tag `Platform::notify` was given, the id of the session it ran in.
    NotificationClicked(u64),
    /// The drop-down hotkey was pressed, anywhere (`Platform::register_hotkey`).
    Hotkey,
    /// This Flow was started again (`Platform::listen_for_starts`): it
    /// comes forward in that start's place.
    StartedAgain,
    /// The icon of a Flow put away was clicked (`Platform::away_icon`):
    /// it comes back.
    AwayIconShow,
    /// The icon's menu's Quit.
    AwayIconQuit,
    /// A screen reader asks for the tree of what Flow shows, asks a node of
    /// it to do something, or lets it go (`crate::access`).
    Access(accesskit_winit::Event),
    /// An outside app's call through `flow-mcp`, or its going
    /// (`crate::mcp::bridge`).
    Mcp(crate::mcp::bridge::Event),
    /// What one of the MCP servers the assistant uses said
    /// (`crate::mcp::client`).
    McpServer(crate::mcp::client::ServerEvent),
}

impl From<accesskit_winit::Event> for UserEvent {
    fn from(event: accesskit_winit::Event) -> Self {
        UserEvent::Access(event)
    }
}

/// What menus can ask for (on macOS, the menu bar and the context menu).
/// Other platforms reach the same actions through keys and the title bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    About,
    CheckForUpdates,
    Settings,
    Quit,
    /// Close every tab and open a first one as Flow does when it starts.
    Restart,
    /// Start the tab in front afresh: its shell again, in its place.
    RestartTab,
    /// Efficient mode on or off.
    ToggleEfficient,
    /// The window kept on top of every other, or not (`window.on_top`).
    ToggleOnTop,
    /// Start recording the terminal, or stop the recording under way.
    Record,
    /// Save as theme…: name what's worn now and keep it beside the
    /// built-in themes.
    SaveTheme,
    Copy,
    Paste,
    SelectAll,
    /// Clear the scrollback and the screen, like Terminal's Clear to Start.
    ClearBuffer,
    /// The pane in front back to the scrollback's foot, or its top.
    ScrollToBottom,
    ScrollToTop,
    /// The pane in front's scrollback put at the command before, or after,
    /// the one last gone to (`crate::marks`).
    PreviousCommand,
    NextCommand,
    /// Open the command history's search.
    SearchHistory,
    /// Open the find bar over the terminal, or give it the keys again.
    Find,
    /// The find bar's next match, older (up), and the one before, newer.
    FindNext,
    FindPrevious,
    /// Open the command palette, everything Flow can do found by name.
    Palette,
    /// Ask the assistant why the last command that failed did.
    AskWhy,
    BiggerText,
    SmallerText,
    ActualSize,
    ToggleAssistant,
    /// Open and focus the assistant panel (never closes it).
    OpenAssistant,
    NewConversation,
    GuideMode,
    AutoMode,
    ChangeKey,
    ForgetKey,
    /// Open a tab with the default shell.
    NewTab,
    /// Open a tab with the shell profile at this index.
    NewTabWith(usize),
    /// ⌘W: the pane in front of a split tab, else the tab.
    CloseTab,
    /// Split the pane in front in two side by side (⌘D), or one above the
    /// other (⇧⌘D).
    SplitRight,
    SplitDown,
    NextTab,
    PreviousTab,
    /// Open help for the screen in front, or Flow's own over the terminal.
    Help,
    /// Open the help page at this index of `help::Page::ALL`.
    HelpPage(usize),
}
