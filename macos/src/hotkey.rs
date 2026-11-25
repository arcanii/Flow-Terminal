//! The drop-down hotkey on the Mac (`Platform::register_hotkey`), and the
//! window dropped down. The key is taken with Carbon's RegisterEventHotKey,
//! which AppKit still delivers to an app whichever app is in front, with no
//! permission to ask for (unlike watching every key, which needs
//! Accessibility's), through hand-written bindings: no crate has them. The
//! window drops over the top of the screen the pointer is on, below its
//! menu bar, on every Space while it's dropped down (in front of other
//! apps' windows only while Always on top keeps it there, as anywhere),
//! and AppKit doesn't keep it on the screen meanwhile, so it can slide
//! down from above (Flow's own `constrainFrameRect:toScreen:` on
//! winit's window class). With Over full-screen apps on, Flow is an
//! accessory app while it's down, which a full-screen app's Space lets in.
//! Putting it away hides Flow, so the app before it has the keys again,
//! and makes it a Regular app again.

use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::ptr::null_mut;
use std::sync::{Once, OnceLock};

use flow_core::event::UserEvent;
use flow_core::hotkey::{top_part, Hotkey, Key, Rect, Share};
use flow_core::session::Waker;
use objc2::runtime::{AnyClass, AnyObject, Imp, Sel};
use objc2::{sel, MainThreadMarker};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSEvent, NSScreen, NSWindowCollectionBehavior,
};
use objc2_foundation::NSRect;
use winit::window::Window;

use crate::window::ns_window;

type OSStatus = i32;
type EventTargetRef = *mut c_void;
type EventHandlerRef = *mut c_void;
type EventHandlerCallRef = *mut c_void;
type EventRef = *mut c_void;
type EventHotKeyRef = *mut c_void;
type EventHandlerUPP = extern "C" fn(EventHandlerCallRef, EventRef, *mut c_void) -> OSStatus;

#[repr(C)]
struct EventTypeSpec {
    event_class: u32,
    event_kind: u32,
}

#[repr(C)]
struct EventHotKeyID {
    signature: u32,
    id: u32,
}

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    fn GetApplicationEventTarget() -> EventTargetRef;
    fn InstallEventHandler(
        target: EventTargetRef,
        handler: EventHandlerUPP,
        num_types: u32,
        list: *const EventTypeSpec,
        user_data: *mut c_void,
        out_ref: *mut EventHandlerRef,
    ) -> OSStatus;
    fn RegisterEventHotKey(
        key_code: u32,
        modifiers: u32,
        id: EventHotKeyID,
        target: EventTargetRef,
        options: u32,
        out_ref: *mut EventHotKeyRef,
    ) -> OSStatus;
    fn UnregisterEventHotKey(hot_key: EventHotKeyRef) -> OSStatus;
}

/// `kEventClassKeyboard`, `kEventHotKeyPressed`, and Carbon's modifiers.
const KEYBOARD: u32 = u32::from_be_bytes(*b"keyb");
const HOT_KEY_PRESSED: u32 = 5;
const CMD: u32 = 1 << 8;
const SHIFT: u32 = 1 << 9;
const OPTION: u32 = 1 << 11;
const CONTROL: u32 = 1 << 12;

thread_local! {
    /// The hotkey taken, the handler installed, where a press goes, and the
    /// window's collection behavior from before it dropped down. All on the
    /// main thread, where Carbon calls the handler.
    static HELD: Cell<EventHotKeyRef> = const { Cell::new(null_mut()) };
    static HANDLER: Cell<bool> = const { Cell::new(false) };
    static SINK: RefCell<Option<Waker>> = const { RefCell::new(None) };
    static BEHAVIOR: Cell<Option<NSWindowCollectionBehavior>> = const { Cell::new(None) };
    /// Flow runs as an accessory app while the window is down, for Over
    /// full-screen apps.
    static ACCESSORY: Cell<bool> = const { Cell::new(false) };
}

extern "C" fn pressed(_call: EventHandlerCallRef, _event: EventRef, _data: *mut c_void) -> OSStatus {
    SINK.with(|s| {
        if let Some(waker) = s.borrow().as_ref() {
            waker.send(UserEvent::Hotkey);
        }
    });
    0
}

/// Takes `hotkey` for the whole system in place of the one before; None
/// lets go of it. False where it couldn't be taken.
pub fn register(hotkey: Option<Hotkey>, waker: Waker) -> bool {
    if MainThreadMarker::new().is_none() {
        return false;
    }
    SINK.with(|s| *s.borrow_mut() = Some(waker));
    let held = HELD.with(|h| h.replace(null_mut()));
    if !held.is_null() {
        // SAFETY: a hotkey RegisterEventHotKey gave, let go of once.
        unsafe { UnregisterEventHotKey(held) };
    }
    let Some(hotkey) = hotkey else { return true };
    let modifiers = [(hotkey.ctrl, CONTROL), (hotkey.alt, OPTION), (hotkey.shift, SHIFT), (hotkey.cmd, CMD)]
        .iter()
        .filter(|(on, _)| *on)
        .fold(0, |all, (_, m)| all | m);
    // SAFETY: Carbon's event calls, on the main thread, with pointers to
    // values that outlive them; the handler is a plain function for the
    // whole run.
    unsafe {
        let target = GetApplicationEventTarget();
        if !HANDLER.get() {
            let spec = EventTypeSpec { event_class: KEYBOARD, event_kind: HOT_KEY_PRESSED };
            let mut handler = null_mut();
            if InstallEventHandler(target, pressed, 1, &spec, null_mut(), &mut handler) != 0 {
                return false;
            }
            HANDLER.set(true);
        }
        let id = EventHotKeyID { signature: u32::from_be_bytes(*b"Flow"), id: 1 };
        let mut held = null_mut();
        if RegisterEventHotKey(key_code(hotkey.key), modifiers, id, target, 0, &mut held) != 0 || held.is_null() {
            return false;
        }
        HELD.set(held);
    }
    true
}

/// The Mac's virtual key code for `key` (`kVK_*`, by where the key is on a
/// US keyboard, so the key left of 1 is the same key elsewhere).
fn key_code(key: Key) -> u32 {
    match key {
        Key::Grave => 0x32,
        Key::Space => 0x31,
        Key::F(n) => [0x7a, 0x78, 0x63, 0x76, 0x60, 0x61, 0x62, 0x64, 0x65, 0x6d, 0x67, 0x6f][(n.clamp(1, 12) - 1) as usize],
        Key::Digit(d) => [0x1d, 0x12, 0x13, 0x14, 0x15, 0x17, 0x16, 0x1a, 0x1c, 0x19][d.min(9) as usize],
        Key::Letter(c) => {
            const LETTERS: [u32; 26] = [
                0x00, 0x0b, 0x08, 0x02, 0x0e, 0x03, 0x05, 0x04, 0x22, 0x26, 0x28, 0x25, 0x2e, 0x2d, 0x1f, 0x23, 0x0c, 0x0f,
                0x01, 0x11, 0x20, 0x09, 0x0d, 0x07, 0x10, 0x06,
            ];
            LETTERS[(c.to_ascii_lowercase() as u8).saturating_sub(b'a').min(25) as usize]
        }
    }
}

/// The top `share` of the room for windows (below the menu bar, beside the
/// Dock) of the screen the pointer is on, else the window's.
pub fn drop_down_rect(window: &Window, share: Share) -> Option<Rect> {
    let mtm = MainThreadMarker::new()?;
    let screens = NSScreen::screens(mtm);
    // AppKit's origin is the foot of the main screen, the one with the menu bar.
    let main = screens.firstObject()?.frame();
    let pointer = NSEvent::mouseLocation();
    let inside = |r: NSRect| {
        pointer.x >= r.origin.x && pointer.x < r.origin.x + r.size.width && pointer.y >= r.origin.y && pointer.y < r.origin.y + r.size.height
    };
    let room = match screens.iter().find(|s| inside(s.frame())) {
        Some(screen) => screen.visibleFrame(),
        None => ns_window(window)?.screen()?.visibleFrame(),
    };
    Some(top_part(flipped(room, main.size.height, window.scale_factor()), share))
}

/// Every screen's whole frame, menu bar and all, in the pixels
/// `drop_down_rect` gives (at the window's scale, which winit's monitor
/// positions aren't when screens' scales differ).
pub fn screens(window: &Window) -> Vec<Rect> {
    let Some(mtm) = MainThreadMarker::new() else { return Vec::new() };
    let screens = NSScreen::screens(mtm);
    let Some(main) = screens.firstObject().map(|s| s.frame()) else { return Vec::new() };
    screens.iter().map(|s| flipped(s.frame(), main.size.height, window.scale_factor())).collect()
}

/// AppKit's rectangle `r`, in points up from the foot of a main screen
/// `main_height` tall, as winit has window positions: in pixels at
/// `scale`, down from its top. winit turns them back into points at the
/// window's scale, which is why that's the scale used, whichever screen it
/// goes to.
fn flipped(r: NSRect, main_height: f64, scale: f64) -> Rect {
    let top = main_height - (r.origin.y + r.size.height);
    let px = |points: f64| (points * scale).round();
    Rect { x: px(r.origin.x) as i32, y: px(top) as i32, width: px(r.size.width) as u32, height: px(r.size.height) as u32 }
}

/// Dropped down, the window joins every Space, full-screen apps' too,
/// where AppKit leaves it to Flow; with `over_full_screen`, Flow is an
/// accessory app meanwhile. After, it's as it was. Its level is left to
/// `window.on_top` (`App::follow_on_top`), dropped down or not: it floats
/// over other apps' windows only while that's on, as the pill says.
pub fn set_dropped_down(window: &Window, on: bool, over_full_screen: bool) {
    let Some(win) = ns_window(window) else { return };
    leave_placing_to_flow();
    UNCONSTRAINED.with(|u| u.set(on));
    set_accessory(on && over_full_screen);
    let was = BEHAVIOR.with(Cell::get);
    match (on, was) {
        (true, _) => {
            let before = was.unwrap_or_else(|| win.collectionBehavior());
            BEHAVIOR.with(|b| b.set(Some(before)));
            // Joining every Space can't be had with moving to the one in use,
            // nor an auxiliary with a full screen of its own: AppKit refuses
            // both pairs.
            let other = NSWindowCollectionBehavior::MoveToActiveSpace | NSWindowCollectionBehavior::FullScreenPrimary;
            let each = NSWindowCollectionBehavior::CanJoinAllSpaces | NSWindowCollectionBehavior::FullScreenAuxiliary;
            win.setCollectionBehavior(before.difference(other).union(each));
        }
        (false, Some(before)) => {
            BEHAVIOR.with(|b| b.set(None));
            win.setCollectionBehavior(before);
        }
        (false, None) => {}
    }
}

/// Brought forward: Flow shown again if the hotkey hid it, made the app in
/// front, and its window given the keys.
pub fn bring_forward(window: &Window) {
    let Some(mtm) = MainThreadMarker::new() else { return };
    let app = NSApplication::sharedApplication(mtm);
    app.unhide(None);
    if let Some(win) = ns_window(window) {
        if win.isMiniaturized() {
            win.deminiaturize(None);
        }
        #[allow(deprecated)]
        app.activateIgnoringOtherApps(true);
        win.makeKeyAndOrderFront(None);
    }
}

/// Put away: Flow hidden, which gives the keys back to the app that had
/// them before it, and a Regular app again, with its Dock icon and menu
/// bar, if it was an accessory while down.
pub fn put_away() {
    if let Some(mtm) = MainThreadMarker::new() {
        NSApplication::sharedApplication(mtm).hide(None);
    }
    set_accessory(false);
}

/// Flow becomes an accessory app (no Dock icon or menu bar, which lets its
/// window into a full-screen app's Space), or a Regular one again.
fn set_accessory(on: bool) {
    let Some(mtm) = MainThreadMarker::new() else { return };
    if ACCESSORY.with(Cell::get) == on {
        return;
    }
    let policy = if on { NSApplicationActivationPolicy::Accessory } else { NSApplicationActivationPolicy::Regular };
    if NSApplication::sharedApplication(mtm).setActivationPolicy(policy) {
        ACCESSORY.with(|a| a.set(on));
    } else {
        log::warn!("macOS wouldn't make Flow {} app", if on { "an accessory" } else { "a regular" });
    }
}

thread_local! {
    /// While the window is dropped down, AppKit leaves where it goes to
    /// Flow.
    static UNCONSTRAINED: Cell<bool> = const { Cell::new(false) };
}

/// NSWindow's own `constrainFrameRect:toScreen:`, which Flow's calls when
/// the window isn't dropped down.
static CONSTRAIN: OnceLock<Imp> = OnceLock::new();

type Constrain = unsafe extern "C-unwind" fn(&AnyObject, Sel, NSRect, *mut AnyObject) -> NSRect;

/// AppKit keeps a titled window's top on its screen as the window is put
/// on screen (`constrainFrameRect:toScreen:`), which would move a window
/// about to slide down from above to where it lands first. So winit's
/// window class gets a `constrainFrameRect:toScreen:` of its own, which
/// leaves the frame as it is while the window is dropped down and is
/// NSWindow's otherwise. Leaves winit alone where its class already has
/// one or isn't there.
fn leave_placing_to_flow() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let constrain = sel!(constrainFrameRect:toScreen:);
        let (Some(class), Some(window)) = (AnyClass::get(c"WinitWindow"), AnyClass::get(c"NSWindow")) else { return };
        if class.instance_methods().iter().any(|m| m.name() == constrain) {
            return;
        }
        let Some(own) = window.instance_method(constrain) else { return };
        let _ = CONSTRAIN.set(own.implementation());
        // SAFETY: the function has the signature of the method it
        // overrides, whose type encoding it's added with, on a subclass of
        // NSWindow's that doesn't define it.
        unsafe {
            let types = objc2::ffi::method_getTypeEncoding(own);
            let imp = std::mem::transmute::<Constrain, Imp>(constrain_frame);
            if !objc2::ffi::class_addMethod(class as *const AnyClass as *mut AnyClass, constrain, imp, types).as_bool() {
                log::warn!("couldn't leave the window's place to Flow; it may not slide down from above");
            }
        }
    });
}

/// Flow's `constrainFrameRect:toScreen:` on winit's window.
unsafe extern "C-unwind" fn constrain_frame(this: &AnyObject, cmd: Sel, frame: NSRect, screen: *mut AnyObject) -> NSRect {
    if UNCONSTRAINED.with(Cell::get) {
        return frame;
    }
    match CONSTRAIN.get() {
        // SAFETY: NSWindow's own implementation of this very method.
        Some(&own) => unsafe { std::mem::transmute::<Imp, Constrain>(own)(this, cmd, frame, screen) },
        None => frame,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use objc2_foundation::{NSPoint, NSSize};

    #[test]
    fn appkit_s_room_for_windows_becomes_winit_s_pixels_from_the_top() {
        // A main screen 1117 points tall at 2x, its menu bar 33 points.
        let room = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(1728.0, 1084.0));
        assert_eq!(flipped(room, 1117.0, 2.0), Rect { x: 0, y: 66, width: 3456, height: 2168 });
        assert_eq!(top_part(flipped(room, 1117.0, 2.0), Share::HALF).height, 1084);
        // A screen above and left of it, its menu bar too.
        let above = NSRect::new(NSPoint::new(-1920.0, 1117.0), NSSize::new(1920.0, 1055.0));
        assert_eq!(flipped(above, 1117.0, 1.0), Rect { x: -1920, y: -1055, width: 1920, height: 1055 });
    }

    #[test]
    fn every_key_flow_names_has_a_key_code_of_its_own() {
        let mut keys: Vec<Key> = vec![Key::Grave, Key::Space];
        keys.extend((1..=12).map(Key::F));
        keys.extend((0..=9).map(Key::Digit));
        keys.extend(('a'..='z').map(Key::Letter));
        let mut codes: Vec<u32> = keys.iter().map(|&k| key_code(k)).collect();
        codes.sort();
        codes.dedup();
        assert_eq!(codes.len(), keys.len());
        assert_eq!((key_code(Key::Grave), key_code(Key::Space), key_code(Key::F(12))), (0x32, 0x31, 0x6f));
    }
}
