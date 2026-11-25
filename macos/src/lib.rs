//! Flow on macOS: blur behind the glass, the traffic lights in Flow's own
//! title bar, the menu bar and context menu, Sparkle updates, how the Mac
//! is powered, notifications of long commands done, the drop-down hotkey,
//! and the Unix shells (zsh, bash) with a UTF-8 locale. The app bundle, release scripts and
//! notes live next to this crate (`packaging/`, `scripts/`, RELEASING.md).
#![cfg(target_os = "macos")]

mod hardware;
mod hotkey;
mod identity;
mod insert;
mod mcp;
mod menu;
mod notify;
mod power;
mod processes;
mod shells;
mod updater;
mod window;

use flow_core::platform::{Machine, MenuState};
use flow_core::session::Waker;
use flow_core::shell::ShellProfile;
use flow_core::Platform;
use winit::window::{Window, WindowAttributes};

pub struct MacOs;

/// What the `flow` binary hands the core on macOS.
pub static PLATFORM: MacOs = MacOs;

impl Platform for MacOs {
    fn window_attributes(&self, attrs: WindowAttributes) -> WindowAttributes {
        use winit::platform::macos::WindowAttributesExtMacOS;
        // Native decorations for the traffic lights, under a see-through title bar.
        attrs
            .with_decorations(true)
            .with_titlebar_transparent(true)
            .with_fullsize_content_view(true)
            .with_title_hidden(true)
    }

    fn apply_backdrop(&self, window: &Window, kind: &str) {
        // Blurs whatever is behind the window without adding views over our layer.
        window.set_blur(kind != "none");
    }

    fn languages(&self) -> Vec<String> {
        shells::preferred_languages()
    }

    fn shells(&self) -> Vec<ShellProfile> {
        flow_core::shell::unix_profiles()
    }

    fn shell_env(&self) -> Vec<(String, String)> {
        shells::env()
    }

    /// Flow.app/Contents/Helpers, where release.sh puts the games.
    fn games_dir(&self) -> Option<std::path::PathBuf> {
        let exe = std::env::current_exe().ok()?;
        let helpers = exe.parent()?.parent()?.join("Helpers");
        helpers.is_dir().then_some(helpers)
    }

    fn show_error(&self, message: &str) {
        window::show_error(message);
    }

    fn beep(&self) -> bool {
        objc2_app_kit::NSBeep();
        true
    }

    fn caps_lock(&self) -> Option<bool> {
        // The keyboard's flags now, not an event's: read any time, on any
        // thread, with no permission asked.
        Some(objc2_app_kit::NSEvent::modifierFlags_class().contains(objc2_app_kit::NSEventModifierFlags::CapsLock))
    }

    fn prepare_input(&self) {
        window::keep_key_repeat();
    }

    fn machine(&self) -> Option<Machine> {
        let info = objc2_foundation::NSProcessInfo::processInfo();
        let v = info.operatingSystemVersion();
        let os = match v.patchVersion {
            0 => format!("macOS {}.{}", v.majorVersion, v.minorVersion),
            patch => format!("macOS {}.{}.{patch}", v.majorVersion, v.minorVersion),
        };
        let memory_gb = (info.physicalMemory() as f64 / (1u64 << 30) as f64).round() as u64;
        Some(Machine { os, chip: sysctl_text(c"machdep.cpu.brand_string").unwrap_or_default(), memory_gb })
    }

    fn hardware(&self) -> flow_core::hardware::Hardware {
        hardware::read()
    }

    fn power(&self) -> flow_core::power::Power {
        power::power()
    }

    fn processes(&self, root: u32) -> Vec<flow_core::resources::Process> {
        processes::processes(root)
    }

    fn mcp_listen(&self, handler: flow_core::mcp::bridge::Handler) -> Result<Box<dyn flow_core::mcp::bridge::Listening>, String> {
        mcp::listen(handler)
    }

    fn machine_id(&self) -> Option<String> {
        identity::machine_id()
    }

    fn host_name(&self) -> Option<String> {
        let mut buf = [0u8; 256];
        // SAFETY: gethostname writes at most the buffer's length, which is
        // passed, and the name is read only up to its end.
        if unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len()) } != 0 {
            return None;
        }
        let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        String::from_utf8(buf[..end].to_vec()).ok().filter(|s| !s.is_empty())
    }

    fn real_numbers(&self) -> flow_core::identity::RealNumbers {
        identity::real_numbers()
    }

    fn window_created(&self, window: &Window) {
        window::setup(window);
        insert::catch_inserted_text();
    }

    fn place_window_controls(&self, window: &Window, title_height: f64) {
        window::place_traffic_lights(window, title_height);
    }

    fn pointer(&self, window: &Window) -> Option<(f64, f64)> {
        window::pointer(window)
    }

    fn title_double_click(&self, window: &Window) {
        window::title_double_click(window);
    }

    fn screen_room(&self, window: &Window) -> Option<(u32, u32)> {
        window::screen_room(window)
    }

    fn keep_on_screen(&self, window: &Window) {
        window::keep_on_screen(window);
    }

    fn install_menus(&self, waker: Waker, shells: &[String], can_update: bool) {
        notify::keep_waker(waker.clone());
        menu::install_menu_bar(waker, shells, can_update);
    }

    fn notify(&self, window: &Window, title: &str, body: &str, tag: u64) {
        notify::notify(window, title, body, tag);
    }

    fn register_hotkey(&self, hotkey: Option<flow_core::hotkey::Hotkey>, waker: Waker) -> bool {
        hotkey::register(hotkey, waker)
    }

    fn drop_down_rect(&self, window: &Window, share: flow_core::hotkey::Share) -> Option<flow_core::hotkey::Rect> {
        hotkey::drop_down_rect(window, share)
    }

    fn screens(&self, window: &Window) -> Vec<flow_core::hotkey::Rect> {
        hotkey::screens(window)
    }

    fn set_dropped_down(&self, window: &Window, on: bool, over_full_screen: bool) {
        hotkey::set_dropped_down(window, on, over_full_screen);
    }

    fn bring_forward(&self, window: &Window) {
        hotkey::bring_forward(window);
    }

    fn put_away(&self, _window: &Window) {
        hotkey::put_away();
    }

    fn sends_to_back(&self) -> bool {
        true
    }

    fn send_to_back(&self, window: &Window) -> bool {
        window::send_to_back(window)
    }

    fn set_menu_state(&self, state: MenuState) {
        menu::set_menu_state(state);
    }

    fn show_context_menu(&self, window: &Window, at: (f32, f32), has_selection: bool) -> bool {
        menu::show_context_menu(window, at, has_selection)
    }

    fn set_option_as_meta(&self, window: &Window, meta: bool) {
        use winit::platform::macos::{OptionAsAlt, WindowExtMacOS};
        window.set_option_as_alt(if meta { OptionAsAlt::Both } else { OptionAsAlt::None });
    }

    fn updater_init(&self, _waker: Waker) -> bool {
        updater::init()
    }

    fn updater_available(&self) -> bool {
        updater::available()
    }

    /// A web or mail address, or a `file:` URL of Flow's own, which only
    /// Flow makes (`link::for_the_opener`): never another scheme, which
    /// NSWorkspace would hand to whichever app claims it.
    fn open_url(&self, url: &str) -> bool {
        flow_core::link::for_the_opener(url) && menu::open_url(url)
    }

    fn elevated(&self) -> bool {
        // SAFETY: geteuid has no preconditions.
        unsafe { libc::geteuid() == 0 }
    }

    /// A LaunchAgent that opens Flow at the next login, which every start
    /// deletes: Flow.app, opened as an app is, where Flow runs from its
    /// bundle, else the binary itself.
    fn start_once(&self, on: bool) -> bool {
        use flow_core::ai::memory;
        let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) else { return false };
        let path = memory::launch_agent_path(&home);
        if !on {
            return memory::put_or_remove(&path, None);
        }
        let Ok(exe) = std::env::current_exe() else { return false };
        let program = match exe.ancestors().find(|p| p.extension().is_some_and(|e| e == "app")) {
            Some(app) => vec!["/usr/bin/open".to_string(), app.display().to_string()],
            None => vec![exe.display().to_string()],
        };
        let program: Vec<&str> = program.iter().map(String::as_str).collect();
        memory::put_or_remove(&path, Some(&memory::launch_agent(&program)))
    }

    /// `open -R`: the Finder, the file selected in its folder.
    fn reveal(&self, path: &std::path::Path) -> bool {
        use std::process::{Command, Stdio};
        Command::new("/usr/bin/open").arg("-R").arg(path).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().is_ok()
    }

    fn check_for_updates(&self) {
        updater::check_with_ui();
    }
}

/// A text value of the kernel's settings (`sysctl -n <name>`).
fn sysctl_text(name: &std::ffi::CStr) -> Option<String> {
    let mut len: libc::size_t = 0;
    // SAFETY: a null buffer asks only for the length; the second call writes
    // at most `len` bytes into a buffer that long.
    unsafe {
        if libc::sysctlbyname(name.as_ptr(), std::ptr::null_mut(), &mut len, std::ptr::null_mut(), 0) != 0 || len == 0 {
            return None;
        }
        let mut buf = vec![0u8; len];
        if libc::sysctlbyname(name.as_ptr(), buf.as_mut_ptr().cast(), &mut len, std::ptr::null_mut(), 0) != 0 {
            return None;
        }
        buf.truncate(len);
        let text = String::from_utf8_lossy(&buf).trim_end_matches('\0').trim().to_string();
        (!text.is_empty()).then_some(text)
    }
}
