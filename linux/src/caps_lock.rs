//! Whether Caps Lock is on, for its notice (`Platform::caps_lock`): on
//! X11, the Lock modifier in the mask the X server gives with where the
//! pointer is (`QueryPointer`, the keys' state as well as the buttons'), on
//! a connection of Flow's own, opened at the first ask, as the hotkey has
//! one. Wayland gives a program its locked modifiers only with its own
//! keyboard's events (`wl_keyboard.modifiers`), which winit keeps to itself,
//! and XWayland's are only right while one of its windows has the keys; so
//! there it's none, and Flow guesses from the letters typed.
//!
//! The X server locks Caps Lock as its key goes down and unlocks it as the
//! key comes up, so a press that turns it off still finds it on: the notice
//! then goes as the key comes up, when `about_to_wait` asks again.

use std::sync::atomic::Ordering;
use std::sync::OnceLock;

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ConnectionExt, KeyButMask};
use x11rb::rust_connection::RustConnection;

/// Flow's own connection to the X server, and the root window it asks of.
struct Display {
    conn: RustConnection,
    root: u32,
}

/// Opened at the first ask; none where there's no X server to reach.
static DISPLAY: OnceLock<Option<Display>> = OnceLock::new();

/// Whether Caps Lock is on, where winit runs on X11 and its server answers.
pub fn on() -> Option<bool> {
    if crate::hotkey::ON_WAYLAND.load(Ordering::Relaxed) {
        return None;
    }
    let display = DISPLAY.get_or_init(open).as_ref()?;
    let reply = display.conn.query_pointer(display.root).ok()?.reply().ok()?;
    Some(locked(reply.mask))
}

/// Where the pointer is in X11 window `window`, in its pixels: asked of
/// the server, since winit sends no `CursorMoved` while a file is dragged
/// over the window, for where a drop lands. None on Wayland, where winit
/// gives no file drops anyway.
pub fn pointer(window: u32) -> Option<(f64, f64)> {
    if crate::hotkey::ON_WAYLAND.load(Ordering::Relaxed) {
        return None;
    }
    let display = DISPLAY.get_or_init(open).as_ref()?;
    let reply = display.conn.query_pointer(window).ok()?.reply().ok()?;
    reply.same_screen.then_some((reply.win_x as f64, reply.win_y as f64))
}

/// Whether a pointer's mask holds the Lock modifier, which Caps Lock sets.
fn locked(mask: KeyButMask) -> bool {
    mask.contains(KeyButMask::LOCK)
}

fn open() -> Option<Display> {
    let (conn, screen) = x11rb::connect(None).map_err(|e| log::warn!("no X server to ask of Caps Lock: {e}")).ok()?;
    let root = conn.setup().roots.get(screen)?.root;
    Some(Display { conn, root })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caps_lock_is_the_lock_modifier_whatever_else_is_held() {
        assert!(locked(KeyButMask::LOCK));
        assert!(locked(KeyButMask::LOCK | KeyButMask::SHIFT | KeyButMask::MOD2 | KeyButMask::BUTTON1));
        assert!(!locked(KeyButMask::SHIFT | KeyButMask::MOD2));
        assert!(!locked(KeyButMask::from(0u16)));
    }
}
