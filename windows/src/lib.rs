//! Flow on Windows: DWM glass (acrylic, mica, blur), WinSparkle updates, the
//! display language, the computer for the startup logo, a toast for a long
//! command done, the screen's work area for Fit window to, the drop-down
//! hotkey, the window sent back for a frame's depth gadget, one Flow per
//! program and config folder with an icon while it's put away, and the
//! Windows shells including the bundled BusyBox.
//! The installer, release scripts and vendored binaries live next to this
//! crate (`packaging/`, `scripts/`, `third_party/`; see RELEASING.md).
#![cfg(windows)]

mod elevate;
mod handles;
mod hardware;
mod hotkey;
mod identity;
mod instance;
mod languages;
pub mod mcp;
mod notify;
mod pipe;
mod processes;
mod shells;
mod start_once;
mod system;
mod tray;
mod updater;
mod window;

use flow_core::platform::Machine;
use flow_core::session::Waker;
use flow_core::shell::ShellProfile;
use flow_core::Platform;
use winit::window::{Window, WindowAttributes};

pub use handles::keep_std_handles;
pub use shells::bundled_busybox;

pub struct Windows;

/// What the `flow` binary hands the core on Windows.
pub static PLATFORM: Windows = Windows;

impl Platform for Windows {
    fn window_attributes(&self, attrs: WindowAttributes) -> WindowAttributes {
        use winit::platform::windows::WindowAttributesExtWindows;
        // DirectComposition draws the window; no GDI copy underneath.
        attrs.with_no_redirection_bitmap(true)
    }

    fn apply_backdrop(&self, window: &Window, kind: &str) {
        window::apply_backdrop(window, kind);
    }

    fn instance_descriptor(&self) -> wgpu::InstanceDescriptor {
        let mut desc = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
        // DirectComposition swapchains are the DX12 path that supports alpha.
        desc.backends = wgpu::Backends::DX12;
        desc.backend_options.dx12.presentation_system = wgpu::Dx12SwapchainKind::DxgiFromVisual;
        desc
    }

    fn languages(&self) -> Vec<String> {
        languages::preferred()
    }

    fn machine(&self) -> Option<Machine> {
        Some(system::machine())
    }

    fn hardware(&self) -> flow_core::hardware::Hardware {
        hardware::read()
    }

    fn power(&self) -> flow_core::power::Power {
        system::power()
    }

    fn processes(&self, root: u32) -> Vec<flow_core::resources::Process> {
        processes::processes(root)
    }

    fn machine_id(&self) -> Option<String> {
        identity::machine_id()
    }

    fn host_name(&self) -> Option<String> {
        std::env::var("COMPUTERNAME").ok().filter(|s| !s.trim().is_empty())
    }

    fn real_numbers(&self) -> flow_core::identity::RealNumbers {
        identity::real_numbers()
    }

    fn shells(&self) -> Vec<ShellProfile> {
        shells::shells()
    }

    /// The `games` folder beside flow.exe, where the installer and the
    /// portable zip put the games.
    fn games_dir(&self) -> Option<std::path::PathBuf> {
        let games = std::env::current_exe().ok()?.with_file_name("games");
        games.is_dir().then_some(games)
    }

    /// Flow's pipe for outside apps' `flow-mcp`, the user's alone.
    fn mcp_listen(&self, handler: flow_core::mcp::bridge::Handler) -> Result<Box<dyn flow_core::mcp::bridge::Listening>, String> {
        mcp::listen(handler)
    }

    /// ConPTY drops kitty's image uploads, so each tab's programs send them
    /// down a named pipe instead.
    fn open_pipe(&self, serve: flow_core::pipe::Serve) -> Option<Box<dyn flow_core::pipe::Pipe>> {
        pipe::open(serve)
    }

    fn show_error(&self, message: &str) {
        window::show_error(message);
    }

    fn beep(&self) -> bool {
        use windows_sys::Win32::System::Diagnostics::Debug::MessageBeep;
        use windows_sys::Win32::UI::WindowsAndMessaging::MB_OK;
        // SAFETY: plays the sound set for Windows' default beep; no pointers.
        unsafe { MessageBeep(MB_OK) != 0 }
    }

    fn pointer(&self, window: &Window) -> Option<(f64, f64)> {
        window::pointer(window)
    }

    fn caps_lock(&self) -> Option<bool> {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_CAPITAL};
        // SAFETY: reads the thread's keyboard state for one key; no
        // pointers. Its low bit is the key's toggle, Caps Lock being on.
        Some(unsafe { GetKeyState(VK_CAPITAL as i32) } & 1 != 0)
    }

    fn mark_running(&self) {
        window::mark_running();
    }

    fn updater_init(&self, waker: Waker) -> bool {
        updater::init(waker)
    }

    fn updater_available(&self) -> bool {
        updater::available()
    }

    /// The shell's own opener (`ShellExecuteExW`, as `elevate.rs` calls
    /// it) hands a web or mail address to its program, or one of Flow's own
    /// files, whose `file:` URLs only Flow makes, to its own
    /// (`link::for_the_opener`), the address one string of its own. Never
    /// Explorer's command line, which could read a quote and the commas
    /// after it in an address as switches of its own (`/root,…`), nor a
    /// path, which the opener would run. An elevated Flow opens nothing
    /// (`App::open_url`), as what it opened would run elevated too.
    fn open_url(&self, url: &str) -> bool {
        flow_core::link::for_the_opener(url) && shell_open(url)
    }

    /// Explorer with the file selected in its folder: `/select,"<path>"`
    /// written as it is (`raw_arg`), since Explorer doesn't read the quotes
    /// Rust would put round the whole of it. Only a drive's own path
    /// (`link::on_this_computer`), with no quote in it to end the one
    /// round it: what follows would be Explorer's switches.
    fn reveal(&self, path: &std::path::Path) -> bool {
        use std::os::windows::process::CommandExt;
        let path = path.display().to_string().replace('/', "\\");
        if !flow_core::link::on_this_computer(std::path::Path::new(&path)) || path.contains('"') {
            return false;
        }
        std::process::Command::new("explorer.exe").raw_arg(format!("/select,\"{path}\"")).spawn().is_ok()
    }

    fn start_once(&self, on: bool) -> bool {
        start_once::start_once(on)
    }

    fn elevated(&self) -> bool {
        elevate::elevated()
    }

    fn relaunch_elevated(&self, task: i64) -> bool {
        elevate::relaunch(task)
    }

    fn check_for_updates(&self) {
        updater::check_with_ui();
    }

    fn updater_cleanup(&self) {
        updater::cleanup();
    }

    /// Windows has no menu bar of Flow's; the waker is kept for a toast's
    /// click and the icon's while put away, and Flow's toasts registered.
    fn install_menus(&self, waker: Waker, _shells: &[String], _can_update: bool) {
        tray::prepare(waker.clone());
        notify::prepare(waker);
    }

    /// A toast in Windows' notifications, or the taskbar button flashing
    /// where Windows won't show one.
    fn notify(&self, window: &Window, title: &str, body: &str, tag: u64) {
        notify::notify(window, title, body, tag);
    }

    fn attended(&self, window: &Window) {
        notify::stop_flashing(window);
    }

    fn screen_room(&self, window: &Window) -> Option<(u32, u32)> {
        window::screen_room(window)
    }

    fn keep_on_screen(&self, window: &Window) {
        window::keep_on_screen(window);
    }

    fn set_dropped_down(&self, window: &Window, on: bool, _over_full_screen: bool) {
        window::set_fades(window, !on);
    }

    fn sends_to_back(&self) -> bool {
        true
    }

    fn send_to_back(&self, window: &Window) -> bool {
        window::send_to_back(window)
    }

    fn register_hotkey(&self, hotkey: Option<flow_core::hotkey::Hotkey>, waker: Waker) -> bool {
        hotkey::register(hotkey, waker)
    }

    /// The notification area's icon, since the hidden window has no
    /// taskbar button.
    fn away_icon(&self, tip: Option<&str>) {
        match tip {
            Some(tip) => tray::show(tip),
            None => tray::hide(),
        }
    }

    fn hand_to_running(&self, id: &str) -> flow_core::platform::Start {
        instance::hand_to_running(id)
    }

    fn listen_for_starts(&self, waker: Waker) {
        instance::listen(waker);
    }

    fn came_forward(&self) {
        instance::came_forward();
    }

    /// No icon is left behind: Windows keeps a gone program's until the
    /// pointer passes over it.
    fn exiting(&self) {
        tray::hide();
    }
}

/// Has the shell open `url` with its verb "open", as a click on it would:
/// the address handed whole to the program registered for its scheme, on
/// no command line of Flow's. False where the shell couldn't.
fn shell_open(url: &str) -> bool {
    use windows_sys::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW};
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let (verb, file) = (wide("open"), wide(url));
    // SAFETY: a zeroed SHELLEXECUTEINFOW is valid; its strings outlive the call.
    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOASYNC;
    info.lpVerb = verb.as_ptr();
    info.lpFile = file.as_ptr();
    info.nShow = SW_SHOWNORMAL;
    // SAFETY: `info` is filled in as the call reads it.
    let opened = unsafe { ShellExecuteExW(&mut info) } != 0;
    if !opened {
        log::warn!("couldn't open {url}: {}", std::io::Error::last_os_error());
    }
    opened
}
