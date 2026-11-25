//! Flow on Linux and other Unix systems: the window's name and icon for the
//! desktop, startup notification, blur where the compositor offers it, the
//! window manager's menu, the primary selection, the distribution's logo,
//! Flow's own updater, desktop notifications of long commands done, the
//! drop-down hotkey, sending the window behind the others (the window
//! frame's depth gadget) and whether Caps Lock is on, all on X11, and the Unix
//! shells (zsh, bash, sh). The .desktop file
//! and the scripts that make, sign and install a release live next to this
//! crate (`packaging/`, `scripts/`; see README.md and RELEASING.md).
#![cfg(all(unix, not(target_os = "macos")))]

mod caps_lock;
mod hotkey;
mod notify;
mod selection;
mod system;
mod updater;
mod window;

use flow_core::platform::{Machine, UpdateState};
use flow_core::session::Waker;
use flow_core::shell::ShellProfile;
use flow_core::Platform;
use winit::window::{Window, WindowAttributes};

pub use window::APP_ID;

pub struct Linux;

/// Opens `url` in the desktop's browser, through `xdg-open`, or a folder
/// (`reveal`'s) in its file manager. Never a word starting with a dash,
/// which xdg-open would take for one of its options.
pub(crate) fn open_url(url: &str) -> bool {
    use std::process::{Command, Stdio};
    if url.is_empty() || url.starts_with('-') {
        return false;
    }
    let opened = Command::new("xdg-open").arg(url).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
    if let Err(err) = &opened {
        log::warn!("could not open {url}: {err}");
    }
    opened.is_ok()
}

/// Shows `path` selected in its folder: the desktop's file manager through
/// D-Bus (`org.freedesktop.FileManager1.ShowItems`, which Files, Dolphin,
/// Nemo and Thunar answer), else its folder opened with `xdg-open`.
pub(crate) fn reveal(path: &std::path::Path) -> bool {
    use std::process::{Command, Stdio};
    let mut uri = String::from("file://");
    for b in path.to_string_lossy().bytes() {
        if b.is_ascii_alphanumeric() || b"-._~/".contains(&b) {
            uri.push(b as char);
        } else {
            uri.push_str(&format!("%{b:02X}"));
        }
    }
    let shown = Command::new("dbus-send")
        .args(["--session", "--print-reply", "--dest=org.freedesktop.FileManager1", "--type=method_call", "/org/freedesktop/FileManager1", "org.freedesktop.FileManager1.ShowItems"])
        .arg(format!("array:string:{uri}"))
        .arg("string:")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    shown || path.parent().is_some_and(|folder| open_url(&folder.to_string_lossy()))
}

/// What the `flow` binary hands the core on Linux.
pub static PLATFORM: Linux = Linux;

impl Platform for Linux {
    fn window_attributes(&self, attrs: WindowAttributes) -> WindowAttributes {
        window::attributes(attrs)
    }

    fn apply_backdrop(&self, window: &Window, kind: &str) {
        // KDE Plasma blurs behind the window; other compositors ignore this.
        window.set_blur(kind != "none");
    }

    fn shells(&self) -> Vec<ShellProfile> {
        flow_core::shell::unix_profiles()
    }

    fn languages(&self) -> Vec<String> {
        system::languages()
    }

    fn shell_env(&self) -> Vec<(String, String)> {
        system::shell_env()
    }

    /// `lib/flow-terminal/games` beside the `bin` folder Flow is in, where
    /// the release's install.sh (and a package for /usr) puts the games.
    fn games_dir(&self) -> Option<std::path::PathBuf> {
        let exe = std::env::current_exe().ok()?;
        let games = exe.parent()?.parent()?.join("lib/flow-terminal/games");
        games.is_dir().then_some(games)
    }

    fn show_error(&self, message: &str) {
        window::show_error(message);
    }

    fn machine(&self) -> Option<Machine> {
        Some(system::machine())
    }

    fn hardware(&self) -> flow_core::hardware::Hardware {
        system::hardware()
    }

    /// From the power supplies in sysfs (`flow_core::power::linux`).
    fn power(&self) -> flow_core::power::Power {
        flow_core::power::linux(std::path::Path::new("/"))
    }

    fn processes(&self, root: u32) -> Vec<flow_core::resources::Process> {
        flow_core::resources::linux_live(root)
    }

    /// The socket in Flow's config folder, its peers read from `/proc`.
    fn mcp_listen(&self, handler: flow_core::mcp::bridge::Handler) -> Result<Box<dyn flow_core::mcp::bridge::Listening>, String> {
        use flow_core::mcp::bridge;
        bridge::listen_unix(&bridge::address(), bridge::linux_peer, handler).map(|l| Box::new(l) as Box<dyn bridge::Listening>)
    }

    fn machine_id(&self) -> Option<String> {
        flow_core::identity::linux_machine_id(std::path::Path::new("/"))
    }

    fn host_name(&self) -> Option<String> {
        std::fs::read_to_string("/proc/sys/kernel/hostname").ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
    }

    fn real_numbers(&self) -> flow_core::identity::RealNumbers {
        flow_core::identity::linux_real(std::path::Path::new("/"))
    }

    fn system_logo(&self) -> Option<(Vec<u8>, u32, u32)> {
        system::logo()
    }

    fn primary_selection(&self) -> Option<String> {
        selection::get()
    }

    fn set_primary_selection(&self, text: &str) {
        selection::set(text);
    }

    fn show_window_menu(&self, window: &Window, at: (f32, f32)) -> bool {
        window::show_window_menu(window, at)
    }

    fn exiting(&self) {
        selection::release();
    }

    /// Whether winit runs on Wayland, where no hotkey can be taken.
    fn window_created(&self, window: &Window) {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        let wayland = window.window_handle().is_ok_and(|h| matches!(h.as_raw(), RawWindowHandle::Wayland(_)));
        hotkey::ON_WAYLAND.store(wayland, std::sync::atomic::Ordering::Relaxed);
    }

    /// Wayland or X11, as `window_created` found.
    fn windowing(&self) -> Option<&'static str> {
        Some(if hotkey::ON_WAYLAND.load(std::sync::atomic::Ordering::Relaxed) { "Wayland" } else { "X11" })
    }

    fn notify(&self, window: &Window, title: &str, body: &str, _tag: u64) {
        notify::notify(window, title, body);
    }

    fn caps_lock(&self) -> Option<bool> {
        caps_lock::on()
    }

    fn pointer(&self, window: &Window) -> Option<(f64, f64)> {
        caps_lock::pointer(hotkey::x11_window(window)?)
    }

    fn hotkey_possible(&self) -> bool {
        hotkey::possible()
    }

    fn on_top_possible(&self) -> bool {
        // An X11 window's _NET_WM_STATE_ABOVE, which XWayland's window
        // manager honors too; Wayland has nothing for it.
        !hotkey::ON_WAYLAND.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn register_hotkey(&self, hotkey: Option<flow_core::hotkey::Hotkey>, waker: Waker) -> bool {
        hotkey::register(hotkey, waker)
    }

    fn bring_forward(&self, window: &Window) {
        hotkey::bring_forward(window);
    }

    fn sends_to_back(&self) -> bool {
        hotkey::sends_to_back()
    }

    fn send_to_back(&self, window: &Window) -> bool {
        hotkey::send_to_back(window)
    }

    fn updater_init(&self, waker: Waker) -> bool {
        updater::init(waker)
    }

    fn updater_available(&self) -> bool {
        updater::available()
    }

    fn reveal(&self, path: &std::path::Path) -> bool {
        reveal(path)
    }

    fn elevated(&self) -> bool {
        // SAFETY: geteuid has no preconditions.
        unsafe { libc::geteuid() == 0 }
    }

    /// An XDG autostart entry starting Flow at the next login, which every
    /// start deletes.
    fn start_once(&self, on: bool) -> bool {
        use flow_core::ai::memory;
        let Some(config) = dirs::config_dir() else { return false };
        let path = memory::autostart_path(&config);
        if !on {
            return memory::put_or_remove(&path, None);
        }
        let Ok(exe) = std::env::current_exe() else { return false };
        memory::put_or_remove(&path, Some(&memory::autostart_entry(&exe.display().to_string())))
    }

    /// A web or mail address, or a `file:` URL of Flow's own, which only
    /// Flow makes (`link::for_the_opener`): never a path or another scheme,
    /// which xdg-open would hand to whichever program claims it.
    fn open_url(&self, url: &str) -> bool {
        flow_core::link::for_the_opener(url) && open_url(url)
    }

    fn check_for_updates(&self) {
        updater::check(false);
    }

    fn update_state(&self) -> Option<UpdateState> {
        updater::state()
    }

    fn check_for_updates_quietly(&self) {
        updater::check(true);
    }

    fn install_update(&self) {
        updater::install();
    }

    fn relaunch(&self) -> bool {
        updater::relaunch()
    }

    fn replaced(&self) -> bool {
        updater::replaced()
    }
}
