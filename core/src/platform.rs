//! What each OS provides. The core never calls an OS API itself: the `flow`
//! binary passes [`crate::run`] the implementation for the OS it was built
//! for, from the `windows`, `macos` or `linux` crate.

use winit::window::{Window, WindowAttributes};

use crate::session::Waker;
use crate::shell::ShellProfile;

/// The computer, as the startup logo's lines tell it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Machine {
    /// The system and its version ("macOS 27.0").
    pub os: String,
    /// The processor's name ("Apple M3"); empty when unknown.
    pub chip: String,
    /// Memory in whole gigabytes; 0 when unknown.
    pub memory_gb: u64,
}

pub trait Platform: Sync {
    /// Adds this OS's window options (decorations, title bar, compositing)
    /// to the transparent, undecorated window the core asks for.
    fn window_attributes(&self, attrs: WindowAttributes) -> WindowAttributes {
        attrs
    }

    /// Puts a backdrop material behind the glass: "acrylic", "mica", "blur"
    /// or "none". Also called again when the setting changes.
    fn apply_backdrop(&self, window: &Window, kind: &str);

    /// The wgpu instance settings that give this OS a transparent surface.
    fn instance_descriptor(&self) -> wgpu::InstanceDescriptor {
        wgpu::InstanceDescriptor::new_without_display_handle_from_env()
    }

    /// The shells this machine has, in title-bar order.
    fn shells(&self) -> Vec<ShellProfile>;

    /// The system's languages, most wanted first ("ja-JP", "en-US"), for
    /// Flow's own when it follows the system. None known by default.
    fn languages(&self) -> Vec<String> {
        Vec::new()
    }

    /// Environment variables every shell starts with, on top of Flow's own
    /// (macOS sets a UTF-8 LANG: apps opened from the Finder get none).
    fn shell_env(&self) -> Vec<(String, String)> {
        Vec::new()
    }

    /// The folder of the programs that come with Flow (`flow-help`,
    /// `flow-settings` and `flowapp`; the games and the demos too until
    /// 0.1.33, which `flowapp` installs now), which Flow's shells get at the
    /// end of their PATH. None when Flow isn't installed with them, as under
    /// `cargo run`.
    fn games_dir(&self) -> Option<std::path::PathBuf> {
        None
    }

    /// Opens a named pipe for the programs in one tab to send images by,
    /// around the terminal (`crate::pipe`), and hands each program that
    /// connects to `serve` on a thread of its own, until the pipe is dropped.
    /// None where the terminal carries images itself, as on macOS and Linux.
    fn open_pipe(&self, _serve: crate::pipe::Serve) -> Option<Box<dyn crate::pipe::Pipe>> {
        None
    }

    /// Tells the user about a failure that keeps the window from opening.
    fn show_error(&self, message: &str);

    /// Plays the system's alert sound, for a program's bell. False where
    /// there's none, and Flow rings its own.
    fn beep(&self) -> bool {
        false
    }

    /// Whether Caps Lock is on, for its notice (`caps_lock.rs`), asked at
    /// each key typed into a pane and while the notice shows. None where
    /// the system doesn't say (Linux on Wayland), and Flow guesses from the
    /// letters typed.
    fn caps_lock(&self) -> Option<bool> {
        None
    }

    /// Marks Flow as running, for installers that wait until it closes.
    fn mark_running(&self) {}

    /// Settles how typing behaves, before the window and its input method
    /// exist.
    fn prepare_input(&self) {}

    /// The computer Flow runs on, for the startup logo's lines.
    fn machine(&self) -> Option<Machine> {
        None
    }

    /// The memory banks and the slots on the buses, for the Workstation's
    /// PROM to probe (`{banks}`, `{probes}`), from what any user may read,
    /// keeping only what names no one: the banks read once a run, the buses
    /// afresh each time, so a tab opened after a device is plugged in shows
    /// it. Empty where the OS says nothing, and then the PROM probes
    /// made-up ones.
    fn hardware(&self) -> crate::hardware::Hardware {
        Default::default()
    }

    /// The system's logo as premultiplied RGBA pixels with their width and
    /// height, for the "system" startup logo, where the OS crate has a
    /// picture of it (the distribution's on Linux). The core draws Apple's
    /// and Windows' itself.
    fn system_logo(&self) -> Option<(Vec<u8>, u32, u32)> {
        None
    }

    /// How the computer is powered: on battery, saving power while plugged
    /// in (the Mac's Low Power Mode, Windows' energy saver), or plugged in,
    /// as it is where the system doesn't say. Efficient mode follows it
    /// where the settings say (`ui.efficient_on_battery`): read on a thread
    /// of its own, at the start, on focus and at most every 2 seconds.
    fn power(&self) -> crate::power::Power {
        crate::power::Power::Plugged
    }

    /// The OS's own id for this computer, which only seeds the workstations'
    /// made-up numbers after `identity` hashes it, so they're the same at
    /// every launch here and different on another computer. None where the
    /// OS has none.
    fn machine_id(&self) -> Option<String> {
        None
    }

    /// The computer's name, as a prompt may show it, for the word a
    /// recording gives as it starts. None where it can't be read.
    fn host_name(&self) -> Option<String> {
        None
    }

    /// The computer's own Ethernet address and serial number, read only
    /// while Theme details' Its real numbers is on.
    fn real_numbers(&self) -> crate::identity::RealNumbers {
        Default::default()
    }

    /// Process `root` and every process under it (a tab's shell and the
    /// programs it started), `root` first, for the card under a tab and
    /// About's Resources: read on a thread of its own, about once a second
    /// while one of them shows. Empty where the OS can't say.
    fn processes(&self, _root: u32) -> Vec<crate::resources::Process> {
        Vec::new()
    }

    /// The primary selection: on Linux, the text last selected anywhere,
    /// which a middle click pastes. None where the OS has no such thing.
    fn primary_selection(&self) -> Option<String> {
        None
    }

    /// Makes `text`, just selected with the mouse, the primary selection.
    fn set_primary_selection(&self, _text: &str) {}

    /// Opens the window manager's menu for the window (move, resize, always
    /// on top…) at `at` (window pixels), for a right click on Flow's own
    /// title bar. False if this OS has none for Flow to open.
    fn show_window_menu(&self, _window: &Window, _at: (f32, f32)) -> bool {
        false
    }

    /// One-time setup once the window exists, before it shows.
    fn window_created(&self, _window: &Window) {}

    /// The window system Flow's window is on, where a system has more than
    /// one (Linux's Wayland or X11), for About's version line; None
    /// elsewhere.
    fn windowing(&self) -> Option<&'static str> {
        None
    }

    /// Keeps window controls the OS draws (the macOS traffic lights) placed
    /// in Flow's title bar, `title_height` points tall. Runs with every
    /// frame, so it must be cheap when nothing has moved.
    fn place_window_controls(&self, _window: &Window, _title_height: f64) {}

    /// A double-click on the title bar. The default toggles maximize.
    fn title_double_click(&self, window: &Window) {
        window.set_maximized(!window.is_maximized());
    }

    /// The room the window's screen leaves for windows (less its menu bar,
    /// Dock or taskbar), as width and height in the screen's pixels, for
    /// Settings' Fit window to: the Mac's visible frame, Windows' work
    /// area. None where the system doesn't say: Flow then takes the
    /// screen's whole size, less a taskbar's height.
    fn screen_room(&self, _window: &Window) -> Option<(u32, u32)> {
        None
    }

    /// Moves the window, just given a new size, back into its screen's room
    /// for windows where the size ran it off, its top-left corner staying
    /// where it can.
    fn keep_on_screen(&self, _window: &Window) {}

    /// Tells the user a long command has finished while Flow wasn't in
    /// front (`notify::wanted`): `title` is its tab's name and `body` how
    /// it ended. Where the system's notification can be clicked, the click
    /// comes back as `UserEvent::NotificationClicked(tag)`. The Mac posts
    /// one to Notification Center, Windows shows a toast (or flashes the
    /// taskbar button where it won't), Linux asks the desktop for one.
    /// Nothing by default.
    fn notify(&self, _window: &Window, _title: &str, _body: &str, _tag: u64) {}

    /// The user is back at the window: whatever `notify` started to catch
    /// their eye and is still at stops. Windows' taskbar button, flashed
    /// until the window comes to the front, has been seen to go on
    /// flashing once it has. Nothing by default.
    fn attended(&self, _window: &Window) {}

    /// Whether a hotkey can be had at all: not on Linux under Wayland,
    /// which lets no program take a key for the whole desktop, nor under
    /// XWayland, whose grab hears keys only while an X11 window has them.
    /// Where not, Settings says so in the Hotkey's place and none is taken.
    fn hotkey_possible(&self) -> bool {
        true
    }

    /// Whether the window can be kept on top of every other
    /// (`window.on_top`, winit's `set_window_level`): not on Linux under
    /// Wayland, which has no protocol for it (the window manager's own
    /// menu has Always on Top). Where not, Settings says so in its row and
    /// the title bar has no pill.
    fn on_top_possible(&self) -> bool {
        true
    }

    /// Takes `hotkey` system-wide, letting go of the one taken before, so
    /// that pressing it anywhere sends `UserEvent::Hotkey` through `waker`;
    /// None lets go of it alone. False where it couldn't be taken (another
    /// program has it, or the system has no such thing, as Wayland hasn't).
    fn register_hotkey(&self, _hotkey: Option<crate::hotkey::Hotkey>, _waker: Waker) -> bool {
        false
    }

    /// Where the hotkey drops the window down: the top `share` of the
    /// screen it's on (`window.drop_down_height`), in pixels, its origin
    /// where winit's window positions have theirs. None where there's no
    /// screen to say.
    fn drop_down_rect(&self, window: &Window, share: crate::hotkey::Share) -> Option<crate::hotkey::Rect> {
        let screen = window.current_monitor().or_else(|| window.primary_monitor())?;
        let (at, size) = (screen.position(), screen.size());
        Some(crate::hotkey::top_part(crate::hotkey::Rect { x: at.x, y: at.y, width: size.width, height: size.height }, share))
    }

    /// Every screen's rectangle, in the pixels `drop_down_rect` gives, for
    /// the slide down to tell whether one is above the screen it drops on.
    fn screens(&self, window: &Window) -> Vec<crate::hotkey::Rect> {
        let rect = |m: winit::monitor::MonitorHandle| {
            let (at, size) = (m.position(), m.size());
            crate::hotkey::Rect { x: at.x, y: at.y, width: size.width, height: size.height }
        };
        window.available_monitors().map(rect).collect()
    }

    /// The window has been dropped down (`on`), or is a window as others
    /// are again: on the Mac, dropped, it's on every Space, full-screen
    /// apps' among them, and with `over_full_screen`,
    /// `window.drop_down_over_full_screen`, an accessory app, which a
    /// full-screen app's Space lets in, until it's put away; on Windows,
    /// DWM's fades as it's shown and hidden are off, so it's gone the moment
    /// its slide up ends. Nothing on Linux. Its level isn't this: dropped
    /// down or not, it stays in front of other windows only while
    /// `window.on_top` says (`App::follow_on_top`), so the title bar's
    /// on-top tells the truth.
    fn set_dropped_down(&self, _window: &Window, _on: bool, _over_full_screen: bool) {}

    /// Brings the window forward for the hotkey or a notification's click,
    /// shown, unminimized and with the keys, in front of the app that has
    /// them.
    fn bring_forward(&self, window: &Window) {
        window.set_visible(true);
        window.set_minimized(false);
        window.focus_window();
    }

    /// Puts the window away for the hotkey, the app that had the keys
    /// before getting them back.
    fn put_away(&self, window: &Window) {
        window.set_visible(false);
    }

    /// While the window is put away (`Some`, with the words to show over
    /// it), an icon that shows Flow is still there and brings it back
    /// when clicked, `UserEvent::AwayIconShow`, its menu's Quit
    /// `AwayIconQuit`; `None` takes it away. Windows' notification area,
    /// whose hidden window has no taskbar button; nothing elsewhere: the
    /// Mac's Dock unhides Flow. Its clicks go through the waker
    /// `install_menus` was given.
    fn away_icon(&self, _tip: Option<&str>) {}

    /// First thing as Flow starts, for a start that takes part
    /// (`instance::takes_part`): whether this Flow (`id`,
    /// `instance::id`) is running already, and if so has it come forward
    /// in this start's place. Nothing by default: the Mac's Launch
    /// Services hands a second start to the running app itself.
    fn hand_to_running(&self, _id: &str) -> Start {
        Start::First
    }

    /// Hears later starts of this Flow, once `hand_to_running` said
    /// `First`, each a `UserEvent::StartedAgain`.
    fn listen_for_starts(&self, _waker: Waker) {}

    /// The window has come forward for a start: it's told, and quits.
    fn came_forward(&self) {}

    /// Whether `send_to_back` can, for a window frame's gadget to look
    /// pressed only where it does something. Not by default.
    fn sends_to_back(&self) -> bool {
        false
    }

    /// Sends the window behind every other window on the desktop, as an old
    /// machine's depth gadget sent its window back, the keys going to the
    /// window then in front. False where it couldn't; nothing by default.
    fn send_to_back(&self, _window: &Window) -> bool {
        false
    }

    /// Installs the app's menus; their commands arrive as
    /// `UserEvent::Command`. `shells` are the shell labels, in order, for
    /// the new-tab items.
    fn install_menus(&self, _waker: Waker, _shells: &[String], _can_update: bool) {}

    /// Keeps the menus' checkmarks and enabled items in step with the app.
    fn set_menu_state(&self, _state: MenuState) {}

    /// Opens a native context menu at `at` (window pixels). False if this
    /// OS has none, and a right click copies or pastes instead.
    fn show_context_menu(&self, _window: &Window, _at: (f32, f32), _has_selection: bool) -> bool {
        false
    }

    /// macOS: whether Option sends Esc+key (Meta) or types characters.
    fn set_option_as_meta(&self, _window: &Window, _meta: bool) {}

    /// Starts this OS's updater. False if this build has none; the
    /// "Check for Updates…" button is hidden then.
    fn updater_init(&self, _waker: Waker) -> bool {
        false
    }

    /// Whether `updater_init` succeeded.
    fn updater_available(&self) -> bool {
        false
    }

    /// Opens `url` in the program the system has for it: a web page in the
    /// default browser (About's link to the release's notes, a link a
    /// program wrote, a server's page to sign in on), a mail address in the
    /// mail app, one of Flow's own files in its editor. Each OS takes only
    /// what `link::for_the_opener` lets, so a path or another scheme, which
    /// the opener would run, never goes; its `file:` URLs are trusted, made
    /// only by Flow for its own files (`ai::memory::file_url`), so a link a
    /// program wrote must come through `link::target`. Callers go through
    /// `App::open_url`, which opens nothing while Flow runs as administrator.
    /// False if it couldn't be asked to.
    fn open_url(&self, _url: &str) -> bool {
        false
    }

    /// Shows the file at `path` in its folder, selected, as the Finder,
    /// Explorer or the file manager shows it, running nothing: a `file:`
    /// link a program wrote (`link::Target::Reveal`). False if it couldn't
    /// be asked to.
    fn reveal(&self, _path: &std::path::Path) -> bool {
        false
    }

    /// Has Flow start once by itself when the user next logs in (`on`), for
    /// a task the assistant left waiting for a restart, or takes that away,
    /// as every start does once it has done its work. False where it
    /// couldn't be arranged.
    fn start_once(&self, _on: bool) -> bool {
        false
    }

    /// Whether Flow runs as administrator or root.
    fn elevated(&self) -> bool {
        false
    }

    /// Listens for outside apps' `flow-mcp` ([`crate::mcp::bridge`]), each
    /// call handed to `handler`: the socket in Flow's config folder, or
    /// Windows' pipe, the user's alone. Dropped, it stops. An error says why
    /// it couldn't, such as another Flow listening already.
    fn mcp_listen(&self, _handler: crate::mcp::bridge::Handler) -> Result<Box<dyn crate::mcp::bridge::Listening>, String> {
        Err("outside apps can't connect to Flow on this system yet".into())
    }

    /// Opens Flow again as administrator, for the assistant's `task` to
    /// carry on there ([`crate::CARRY_ON`]); the system's prompt is the
    /// user's yes. False where it couldn't, or they said no. Windows only:
    /// elsewhere sudo gives a command root in the terminal.
    fn relaunch_elevated(&self, _task: i64) -> bool {
        false
    }

    /// "Check for Updates…": the updater shows its own progress and result,
    /// or (Linux) reports them through `update_state` for Flow to show.
    fn check_for_updates(&self) {}

    /// Where an updater with no windows of its own stands, for Flow to show
    /// in About (Linux's); None where the updater shows its own (Sparkle,
    /// WinSparkle). Changes arrive as `UserEvent::Update`.
    fn update_state(&self) -> Option<UpdateState> {
        None
    }

    /// Looks for a newer release unasked, as Flow's daily check does where
    /// Flow checks for itself: nothing changes unless there is one.
    fn check_for_updates_quietly(&self) {}

    /// Downloads, checks and installs the release `update_state` offers.
    fn install_update(&self) {}

    /// Starts the Flow just installed, for this one to quit. False if it
    /// couldn't be started.
    fn relaunch(&self) -> bool {
        false
    }

    /// Whether the program this Flow runs from has been replaced on disk
    /// since it started, as an update puts a new one in its place (Linux's
    /// updater, or apt for a .deb's), so a restart has to start that one
    /// rather than run this one again. False where it can't tell, or where
    /// the updater restarts Flow itself (Sparkle, WinSparkle).
    fn replaced(&self) -> bool {
        false
    }

    /// Shuts the updater down as Flow exits.
    fn updater_cleanup(&self) {}

    /// Where the pointer is now, in the window's pixels from its top left,
    /// asked of the system rather than taken from winit's events, which
    /// stop while a file is dragged over the window: where a dropped file
    /// lands (winit gives a drop no place). None where it can't be asked
    /// (Wayland, which winit gives no file drops anyway).
    fn pointer(&self, _window: &Window) -> Option<(f64, f64)> {
        None
    }

    /// Flow is exiting: lets go of what the OS crate holds, before the core
    /// drops its own (Linux's primary selection, whose clipboard must go
    /// first for the core's to hand the clipboard to a clipboard manager).
    fn exiting(&self) {}
}

/// What a start of Flow found (`Platform::hand_to_running`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Start {
    /// No other start of this Flow runs: this one runs, and hears later
    /// starts.
    First,
    /// The Flow running came forward in this start's place, which quits.
    Handed,
    /// Another start runs but didn't answer: this one runs beside it, as
    /// two Flows always could.
    Beside,
}

/// Where an updater that Flow shows stands (`Platform::update_state`).
#[derive(Debug, Clone, PartialEq)]
pub enum UpdateState {
    /// Nothing asked yet, or a quiet check that found nothing.
    Idle,
    Checking,
    UpToDate,
    /// A newer release, as "0.1.24 (build 160)".
    Available { version: String },
    Installing { version: String },
    /// In place where Flow runs from: the next start is the new version.
    Installed { version: String },
    /// A newer release this copy can't put in place itself, as a .deb's
    /// Flow is dpkg's to replace: the file to download from its page and
    /// the command that installs it, to show and copy.
    ByHand { version: String, file: String, command: String },
    /// A .deb's Flow with the new .deb downloaded and checked with Flow's
    /// key: the command that installs it with apt, which Flow runs in a tab
    /// of its own, where sudo asks for the password
    /// (`App::run_update_in_tab`). The updater goes on to `Installed` once
    /// the new Flow is in place.
    InTab { version: String, command: String },
    /// A newer release for a package manager to install, as an AUR
    /// helper updates the Flow pacman put in /usr: About says so, with
    /// nothing for Flow itself to install.
    ByPackage { version: String },
    /// What went wrong, in the user's language.
    Failed(String),
}

/// What menus need to know to show checkmarks and greyed-out items.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MenuState {
    pub auto_mode: bool,
    pub has_key: bool,
    pub has_selection: bool,
    /// How many terminal tabs are open.
    pub tabs: usize,
    /// Efficient mode is on.
    pub efficient: bool,
    /// The window is kept on top of every other (`window.on_top`).
    pub on_top: bool,
    /// The terminal is being recorded.
    pub recording: bool,
    /// Settings waits for a key to bind (its Keys section), so the menus
    /// give up theirs.
    pub capturing: bool,
}

/// A shortcut as people on this platform write it: `mac` on macOS, `other`
/// elsewhere. For hints and messages.
pub const fn keys(mac: &'static str, other: &'static str) -> &'static str {
    if cfg!(target_os = "macos") {
        mac
    } else {
        other
    }
}

/// Words that differ on each OS: `mac` on macOS, `windows` on Windows and
/// `linux` on Linux and the other Unix systems.
pub const fn per_os(mac: &'static str, windows: &'static str, linux: &'static str) -> &'static str {
    if cfg!(target_os = "macos") {
        mac
    } else if cfg!(windows) {
        windows
    } else {
        linux
    }
}
