//! The drop-down hotkey on Linux (`Platform::register_hotkey`): X11's
//! XGrabKey on the root window, through x11rb (which winit already takes),
//! on a connection of Flow's own, whose events a thread of its own waits
//! for. Wayland has no way for a program to take a key for the whole
//! desktop, and a grab through XWayland (Flow on X11 under a Wayland
//! desktop, which the X server's `XWAYLAND` extension says) sees keys only
//! while an X11 window has them: the hotkey would put the window away and
//! never hear the key that brings it back. So on either it says false, and
//! Settings says the hotkey can't be had.
//! Brought forward, the window asks the window manager for the keys again
//! as a pager would ([`bring_forward`]), and the same connection asks it to
//! send the window behind the others for the notanAmiga frame's depth
//! gadget ([`send_to_back`]).

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use flow_core::event::UserEvent;
use flow_core::hotkey::{Hotkey, Key};
use flow_core::session::Waker;
use x11rb::connection::Connection;
use winit::window::Window;
use x11rb::protocol::xproto::{AtomEnum, ClientMessageEvent, ConfigureWindowAux, ConnectionExt, EventMask, GrabMode, Keycode, Keysym, ModMask, StackMode};
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;

/// winit runs on Wayland, as `window_created` found.
pub static ON_WAYLAND: AtomicBool = AtomicBool::new(false);

/// Flow's own connection to the X server, and the root window grabs are on.
struct Display {
    conn: Arc<RustConnection>,
    root: u32,
    /// The server is XWayland, whose grabs hear keys only while one of its
    /// own windows has them.
    xwayland: bool,
}

/// Opened at the first hotkey, with the thread reading its events; none
/// where there's no X server to reach.
static DISPLAY: OnceLock<Option<Display>> = OnceLock::new();
/// Where presses go, and the grabs held now (the key code and the
/// modifiers grabbed with it), let go of before the next.
static SINK: Mutex<Option<Waker>> = Mutex::new(None);
static HELD: Mutex<Vec<(Keycode, ModMask)>> = Mutex::new(Vec::new());
/// The X server's time of the last press, which the window manager weighs
/// when the window asks for the keys.
static PRESSED_AT: AtomicU32 = AtomicU32::new(0);

/// Whether a hotkey can be had: on X11, an X server to reach that isn't
/// XWayland; never on Wayland. Asked once the window exists.
pub fn possible() -> bool {
    !ON_WAYLAND.load(Ordering::Relaxed) && DISPLAY.get_or_init(open).as_ref().is_some_and(|d| !d.xwayland)
}

/// Takes `hotkey` for the whole desktop in place of the one before; None
/// lets go of it. False on Wayland, without an X server, or where another
/// program has the key.
pub fn register(hotkey: Option<Hotkey>, waker: Waker) -> bool {
    if ON_WAYLAND.load(Ordering::Relaxed) {
        return hotkey.is_none();
    }
    if let Ok(mut sink) = SINK.lock() {
        *sink = Some(waker);
    }
    let Some(display) = DISPLAY.get_or_init(open).as_ref().filter(|d| !d.xwayland) else { return hotkey.is_none() };
    let conn = &*display.conn;
    let mut held = HELD.lock().unwrap_or_else(|e| e.into_inner());
    for (key, modifiers) in held.drain(..) {
        let _ = conn.ungrab_key(key, display.root, modifiers);
    }
    let _ = conn.flush();
    let Some(hotkey) = hotkey else { return true };
    let Some(key) = keycode(conn, keysym(hotkey.key)) else { return false };
    let mut modifiers = ModMask::from(0u16);
    for (on, m) in [(hotkey.ctrl, ModMask::CONTROL), (hotkey.alt, ModMask::M1), (hotkey.shift, ModMask::SHIFT), (hotkey.cmd, ModMask::M4)] {
        if on {
            modifiers |= m;
        }
    }
    // Caps Lock and Num Lock (Mod2) come with a press as modifiers, so the
    // key is grabbed with each of them on and off.
    for extra in [ModMask::from(0u16), ModMask::LOCK, ModMask::M2, ModMask::LOCK | ModMask::M2] {
        let with = modifiers | extra;
        let taken = conn.grab_key(false, display.root, with, key, GrabMode::ASYNC, GrabMode::ASYNC).map(|c| c.check().is_ok()).unwrap_or(false);
        if !taken {
            // Another program has it (BadAccess): let go of what was taken.
            for (key, modifiers) in held.drain(..) {
                let _ = conn.ungrab_key(key, display.root, modifiers);
            }
            let _ = conn.flush();
            return false;
        }
        held.push((key, with));
    }
    true
}

/// Connects to the X server, and starts the thread that sends each press
/// of a key grabbed. A held key repeats as a release and a press at the
/// same moment, which isn't a press of its own.
fn open() -> Option<Display> {
    let (conn, screen) = x11rb::connect(None).map_err(|e| log::warn!("no X server for the hotkey: {e}")).ok()?;
    let root = conn.setup().roots.get(screen)?.root;
    let xwayland = conn.query_extension(b"XWAYLAND").ok().and_then(|c| c.reply().ok()).is_some_and(|r| r.present);
    if xwayland {
        log::info!("no hotkey under XWayland: its grabs hear keys only while an X11 window has them");
        return Some(Display { conn: Arc::new(conn), root, xwayland });
    }
    let conn = Arc::new(conn);
    let reading = conn.clone();
    let started = std::thread::Builder::new().name("flow-hotkey".into()).spawn(move || {
        let mut released = None;
        while let Ok(event) = reading.wait_for_event() {
            match event {
                Event::KeyRelease(e) => released = Some(e.time),
                Event::KeyPress(e) if released != Some(e.time) => {
                    PRESSED_AT.store(e.time, Ordering::Relaxed);
                    if let Some(waker) = SINK.lock().ok().and_then(|s| s.clone()) {
                        waker.send(UserEvent::Hotkey);
                    }
                }
                _ => {}
            }
        }
    });
    started.ok()?;
    Some(Display { conn, root, xwayland })
}

/// Brings the window forward with the keys (`Platform::bring_forward`).
/// winit asks the window manager for them as an application does, with no
/// time, which GNOME's and KDE's turn down while another window has the
/// keys, and not at all for a window still being shown again after the
/// hotkey put it away. So on X11 Flow asks once more, as a pager does
/// (EWMH's source 2), with the time of the press: the window then comes
/// forward with the keys, and the next press finds it in front and puts it
/// away. Only once a hotkey has been taken, since nothing else brings the
/// window forward on Linux.
pub fn bring_forward(window: &Window) {
    window.set_visible(true);
    window.set_minimized(false);
    window.focus_window();
    if ON_WAYLAND.load(Ordering::Relaxed) {
        return;
    }
    let Some(display) = DISPLAY.get().and_then(Option::as_ref) else { return };
    let Some(id) = x11_window(window) else { return };
    let conn = &*display.conn;
    let Some(active) = conn.intern_atom(false, b"_NET_ACTIVE_WINDOW").ok().and_then(|c| c.reply().ok()) else { return };
    let event = ClientMessageEvent::new(32, id, active.atom, [2, PRESSED_AT.load(Ordering::Relaxed), 0, 0, 0]);
    let _ = conn.send_event(false, display.root, EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY, event);
    let _ = conn.flush();
}

/// Whether the window can be sent behind the others
/// (`Platform::sends_to_back`): on X11, through the window manager,
/// XWayland's too; not on Wayland, which lets no program lower its window.
pub fn sends_to_back() -> bool {
    !ON_WAYLAND.load(Ordering::Relaxed) && DISPLAY.get_or_init(open).is_some()
}

/// Sends the window behind every other (`Platform::send_to_back`), as an
/// old machine's depth gadget sent its window back. The window manager
/// does the stacking, so Flow asks it as a window asks (ConfigureWindow's
/// stack mode, which reaches the manager as a request) and as a pager does
/// (EWMH's `_NET_RESTACK_WINDOW`), then gives the keys to the window that's
/// then in front, as a pager would. Under XWayland the X server lists none
/// of the Wayland windows, so which one is in front can't be known there,
/// and the keys are the window manager's to give.
pub fn send_to_back(window: &Window) -> bool {
    if ON_WAYLAND.load(Ordering::Relaxed) {
        return false;
    }
    let Some(display) = DISPLAY.get_or_init(open).as_ref() else { return false };
    let Some(id) = x11_window(window) else { return false };
    let conn = &*display.conn;
    let next = if display.xwayland { None } else { in_front_of_the_rest(conn, display.root, id) };
    let to_manager = EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY;
    let lowered = conn.configure_window(id, &ConfigureWindowAux::new().stack_mode(StackMode::BELOW)).is_ok();
    if let Some(restack) = atom(conn, b"_NET_RESTACK_WINDOW") {
        // A pager's request (source 2), with no sibling: to the bottom.
        let event = ClientMessageEvent::new(32, id, restack, [2, 0, u32::from(StackMode::BELOW), 0, 0]);
        let _ = conn.send_event(false, display.root, to_manager, event);
    }
    if let (Some(next), Some(active)) = (next, atom(conn, b"_NET_ACTIVE_WINDOW")) {
        // A pager's again, with no time of its own (CurrentTime), Flow's
        // window the one that had the keys.
        let event = ClientMessageEvent::new(32, next, active, [2, 0, id, 0, 0]);
        let _ = conn.send_event(false, display.root, to_manager, event);
    }
    lowered && conn.flush().is_ok()
}

/// What the window manager says of a window it lists, for finding the one
/// in front of the rest.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Listed {
    id: u32,
    /// Its desktop (`_NET_WM_DESKTOP`), where it says; [`ALL_DESKTOPS`] for
    /// one on every desktop.
    desktop: Option<u32>,
    /// Minimized (`_NET_WM_STATE_HIDDEN`).
    hidden: bool,
    /// An application's own window, normal or a dialog, or one that names
    /// no type, not a dock's, a panel's or the desktop's.
    app: bool,
}

/// `_NET_WM_DESKTOP` for a window on every desktop.
const ALL_DESKTOPS: u32 = 0xFFFF_FFFF;

/// The first window of `top_down`, listed from the top of the stack down,
/// that would take the keys once `mine` is behind the rest: an
/// application's, shown and on the desktop in view (`desktop`, where the
/// manager says).
fn first_in_front(top_down: impl IntoIterator<Item = Listed>, mine: u32, desktop: Option<u32>) -> Option<u32> {
    let on_view = |w: &Listed| match (w.desktop, desktop) {
        (Some(d), Some(now)) => d == now || d == ALL_DESKTOPS,
        _ => true,
    };
    top_down.into_iter().find(|w| w.id != mine && w.app && !w.hidden && on_view(w)).map(|w| w.id)
}

/// The window in front of the rest but `mine`, from the window manager's
/// list of its windows from the bottom up (`_NET_CLIENT_LIST_STACKING`),
/// each one's facts read only until one is found.
fn in_front_of_the_rest(conn: &RustConnection, root: u32, mine: u32) -> Option<u32> {
    let stacking = property(conn, root, atom(conn, b"_NET_CLIENT_LIST_STACKING")?)?;
    let desktop = atom(conn, b"_NET_CURRENT_DESKTOP").and_then(|a| property(conn, root, a)).and_then(|v| v.first().copied());
    let (state, hidden) = (atom(conn, b"_NET_WM_STATE"), atom(conn, b"_NET_WM_STATE_HIDDEN"));
    let kind = atom(conn, b"_NET_WM_WINDOW_TYPE");
    let apps = [atom(conn, b"_NET_WM_WINDOW_TYPE_NORMAL"), atom(conn, b"_NET_WM_WINDOW_TYPE_DIALOG")];
    let on_desktop = atom(conn, b"_NET_WM_DESKTOP");
    let read = |id: u32| {
        let of = |a: Option<u32>| a.and_then(|a| property(conn, id, a)).unwrap_or_default();
        let kinds = of(kind);
        Listed {
            id,
            desktop: of(on_desktop).first().copied(),
            hidden: hidden.is_some_and(|h| of(state).contains(&h)),
            app: kinds.is_empty() || kinds.iter().any(|k| apps.contains(&Some(*k))),
        }
    };
    first_in_front(stacking.iter().rev().filter(|&&id| id != mine).map(|&id| read(id)), mine, desktop)
}

/// The atom named `name`, where the server has one.
fn atom(conn: &RustConnection, name: &[u8]) -> Option<u32> {
    conn.intern_atom(false, name).ok()?.reply().ok().map(|r| r.atom)
}

/// A property of `window` read as 32-bit values (windows, atoms, numbers),
/// where it has it.
fn property(conn: &RustConnection, window: u32, name: u32) -> Option<Vec<u32>> {
    let reply = conn.get_property(false, window, name, AtomEnum::ANY, 0, 4096).ok()?.reply().ok()?;
    let values = reply.value32()?.collect();
    Some(values)
}

/// The X11 window `window` is, where winit runs on X11.
pub(crate) fn x11_window(window: &Window) -> Option<u32> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match window.window_handle().ok()?.as_raw() {
        RawWindowHandle::Xlib(h) => u32::try_from(h.window).ok(),
        RawWindowHandle::Xcb(h) => Some(h.window.get()),
        _ => None,
    }
}

/// The key code the keyboard has `keysym` on, if it has it.
fn keycode(conn: &RustConnection, keysym: Keysym) -> Option<Keycode> {
    let (min, max) = (conn.setup().min_keycode, conn.setup().max_keycode);
    let map = conn.get_keyboard_mapping(min, (max - min).saturating_add(1)).ok()?.reply().ok()?;
    let per = (map.keysyms_per_keycode as usize).max(1);
    map.keysyms.chunks(per).position(|syms| syms.contains(&keysym)).map(|i| min + i as u8)
}

/// X11's keysym for `key`.
fn keysym(key: Key) -> Keysym {
    match key {
        Key::Grave => 0x60,
        Key::Space => 0x20,
        Key::F(n) => 0xffbe + (n.clamp(1, 12) as u32 - 1),
        Key::Letter(c) => c.to_ascii_lowercase() as u32,
        Key::Digit(d) => 0x30 + d.min(9) as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(id: u32) -> Listed {
        Listed { id, desktop: Some(0), hidden: false, app: true }
    }

    #[test]
    fn sent_back_the_keys_go_to_the_highest_window_that_would_take_them() {
        // From the top down: Flow's own, a dock, a minimized window, one on
        // another desktop, then the one that's shown.
        let top_down = [app(1), Listed { app: false, ..app(2) }, Listed { hidden: true, ..app(3) }, Listed { desktop: Some(1), ..app(4) }, app(5), app(6)];
        assert_eq!(first_in_front(top_down, 1, Some(0)), Some(5));
    }

    #[test]
    fn a_window_on_every_desktop_or_on_none_said_counts_as_on_the_one_in_view() {
        assert_eq!(first_in_front([Listed { desktop: Some(ALL_DESKTOPS), ..app(7) }], 1, Some(2)), Some(7));
        assert_eq!(first_in_front([Listed { desktop: None, ..app(8) }], 1, Some(2)), Some(8));
        assert_eq!(first_in_front([Listed { desktop: Some(3), ..app(9) }], 1, None), Some(9));
    }

    #[test]
    fn with_no_other_window_shown_the_keys_go_nowhere_new() {
        assert_eq!(first_in_front([app(1), Listed { hidden: true, ..app(2) }], 1, Some(0)), None);
    }
}
