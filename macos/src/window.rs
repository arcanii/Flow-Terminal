//! Window dressing on macOS: the traffic lights in Flow's title bar, the
//! title bar double-click, the Dock icon of unbundled builds, sending the
//! window behind the others and error alerts.

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{AnyThread, MainThreadMarker, Message};
use objc2_app_kit::{
    NSAlert, NSApplication, NSApplicationActivationOptions, NSApplicationActivationPolicy, NSImage, NSRunningApplication, NSView, NSWindow, NSWindowButton, NSWindowStyleMask,
};
use objc2_foundation::{ns_string, NSArray, NSData, NSDictionary, NSNumber, NSPoint, NSString, NSUserDefaults};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

pub fn ns_view(window: &Window) -> Option<Retained<NSView>> {
    let handle = window.window_handle().ok()?;
    let RawWindowHandle::AppKit(h) = handle.as_raw() else { return None };
    // SAFETY: winit's NSView stays alive as long as the window, and this runs
    // on the main thread like all of winit's window calls.
    let view: &NSView = unsafe { h.ns_view.cast().as_ref() };
    Some(view.retain())
}

pub(crate) fn ns_window(window: &Window) -> Option<Retained<NSWindow>> {
    ns_view(window)?.window()
}

/// Where the pointer is now, in the window's pixels from its view's top
/// left, as winit gives `CursorMoved`: asked of AppKit, since winit sends
/// none while a file is dragged over the window, for where a drop lands.
pub fn pointer(window: &Window) -> Option<(f64, f64)> {
    let view = ns_view(window)?;
    let ns_window = view.window()?;
    let in_window = ns_window.convertPointFromScreen(objc2_app_kit::NSEvent::mouseLocation());
    let at = view.convertPoint_fromView(in_window, None);
    let y = if view.isFlipped() { at.y } else { view.frame().size.height - at.y };
    let scale = window.scale_factor();
    Some((at.x * scale, y * scale))
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    /// The windows on screen, front to back, as a CFArray of CFDictionaries
    /// the caller owns, toll-free an NSArray of NSDictionaries.
    fn CGWindowListCopyWindowInfo(option: u32, relative_to_window: u32) -> *mut NSArray<NSDictionary<NSString, AnyObject>>;
}

/// `kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements`.
const WINDOWS_ON_SCREEN: u32 = 1 | 1 << 4;

/// Sends the window behind every other on the screen, as an old machine's
/// depth gadget sent its window back, and gives the keys to the app whose
/// window is then in front, as Windows' Alt+Esc does: Flow yields to it
/// (macOS 14 on), and it's activated with that window alone brought
/// forward. Where no other app has a window on screen, Flow keeps them.
pub fn send_to_back(window: &Window) -> bool {
    let Some(mtm) = MainThreadMarker::new() else { return false };
    let Some(win) = ns_window(window) else { return false };
    let next = app_in_front_of_the_rest();
    win.orderBack(None);
    if let Some(app) = next {
        if objc2::available!(macos = 14.0) {
            NSApplication::sharedApplication(mtm).yieldActivationToApplication(&app);
        }
        app.activateWithOptions(NSApplicationActivationOptions::empty());
    }
    true
}

/// The app of the window in front of all but Flow's: the first on screen,
/// front to back, at the normal level (layer 0, which leaves out the menu
/// bar, the Dock and floating panels), not see-through and not Flow's own.
/// It asks CoreGraphics for the windows' owners and levels alone, which
/// need no Screen Recording permission (their names would).
fn app_in_front_of_the_rest() -> Option<Retained<NSRunningApplication>> {
    // SAFETY: CoreGraphics hands back an array it made for the caller (a
    // "Copy" function), which `from_raw` takes over and releases; null is
    // none.
    let windows = unsafe { Retained::from_raw(CGWindowListCopyWindowInfo(WINDOWS_ON_SCREEN, 0)) }?;
    let own = std::process::id() as i32;
    let number = |w: &NSDictionary<NSString, AnyObject>, key: &NSString| w.objectForKey(key)?.downcast::<NSNumber>().ok();
    windows.iter().find_map(|w| {
        let pid = number(&w, ns_string!("kCGWindowOwnerPID"))?.intValue();
        let layer = number(&w, ns_string!("kCGWindowLayer"))?.intValue();
        let alpha = number(&w, ns_string!("kCGWindowAlpha")).map_or(1.0, |a| a.doubleValue());
        if pid == own || layer != 0 || alpha <= 0.0 {
            return None;
        }
        NSRunningApplication::runningApplicationWithProcessIdentifier(pid).filter(|app| app.activationPolicy() != NSApplicationActivationPolicy::Prohibited)
    })
}

/// A held key repeats, as in Terminal. Flow takes keys through the input
/// method so Japanese can be typed, and macOS would otherwise answer a held
/// letter with its accent picker. Registered as a default, so
/// `defaults write <Flow's bundle id> ApplePressAndHoldEnabled -bool true`
/// still brings the picker back. Set before the window and its input context
/// exist, in case they read it only once.
pub fn keep_key_repeat() {
    let no = NSNumber::new_bool(false);
    let values: [&AnyObject; 1] = [&no];
    let defaults = NSDictionary::from_slices(&[ns_string!("ApplePressAndHoldEnabled")], &values);
    // SAFETY: a dictionary of property-list values, as registerDefaults: takes.
    unsafe { NSUserDefaults::standardUserDefaults().registerDefaults(&defaults) };
}

/// One-time setup: no automatic tabs (Flow has one window), and Flow's icon
/// in the Dock when it runs as a bare binary.
pub fn setup(_window: &Window) {
    let Some(mtm) = MainThreadMarker::new() else { return };
    NSWindow::setAllowsAutomaticWindowTabbing(false, mtm);
    // `cargo run` starts a bare binary, which the Dock shows with a generic
    // icon. Flow.app has its own icon in the bundle.
    if NSRunningApplication::currentApplication().bundleIdentifier().is_none() {
        let data = NSData::with_bytes(include_bytes!("../../assets/icon/flow-512.png"));
        if let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) {
            // SAFETY: a valid image, set on the main thread.
            unsafe { NSApplication::sharedApplication(mtm).setApplicationIconImage(Some(&image)) };
        }
    }
}

/// Centers the close, minimize and zoom buttons in a title bar
/// `title_height` points tall, the way Electron's trafficLightPosition does:
/// the title bar container gets that height, so AppKit's own layout keeps the
/// buttons vertically centered, and the buttons move in from the edge by the
/// same margin. AppKit puts them back whenever it lays the title bar out again
/// (after resizing, leaving full screen, ...), so this runs with every frame;
/// it only changes anything when they have moved.
pub fn place_traffic_lights(window: &Window, title_height: f64) {
    let Some(win) = ns_window(window) else { return };
    if win.styleMask().contains(NSWindowStyleMask::FullScreen) {
        return;
    }
    let buttons = [NSWindowButton::CloseButton, NSWindowButton::MiniaturizeButton, NSWindowButton::ZoomButton]
        .map(|b| win.standardWindowButton(b));
    let [Some(close), Some(minimize), Some(zoom)] = buttons else { return };
    // SAFETY: plain view hierarchy access on the main thread.
    let (Some(bar), Some(content)) = (unsafe { close.superview() }, win.contentView()) else { return };
    // SAFETY: as above.
    let Some(container) = (unsafe { bar.superview() }) else { return };
    let mut frame = container.frame();
    let top = content.frame().size.height - title_height;
    if frame.size.height != title_height || frame.origin.y != top {
        frame.size.height = title_height;
        frame.origin.y = top;
        container.setFrame(frame);
    }

    let size = close.frame().size;
    let spacing = minimize.frame().origin.x - close.frame().origin.x;
    let margin = ((title_height - size.height) * 0.5).max(0.0);
    let y = if bar.isFlipped() { margin } else { bar.frame().size.height - margin - size.height };
    for (i, button) in [close, minimize, zoom].iter().enumerate() {
        let at = NSPoint::new(margin + spacing * i as f64, y);
        let now = button.frame().origin;
        if now.x != at.x || now.y != at.y {
            button.setFrameOrigin(at);
        }
    }
}

/// The room the window's screen leaves for windows, below the menu bar and
/// beside the Dock, in the screen's pixels.
pub fn screen_room(window: &Window) -> Option<(u32, u32)> {
    let room = ns_window(window)?.screen()?.visibleFrame();
    // The window's scale is its screen's.
    let scale = window.scale_factor();
    Some(((room.size.width * scale) as u32, (room.size.height * scale) as u32))
}

/// Moves the window back into its screen's room for windows where a new
/// size ran it off an edge, as little as that takes. Its title bar stays in
/// the room above all.
pub fn keep_on_screen(window: &Window) {
    let Some(win) = ns_window(window) else { return };
    let Some(screen) = win.screen() else { return };
    let (room, frame) = (screen.visibleFrame(), win.frame());
    let mut at = frame.origin;
    let right = room.origin.x + room.size.width;
    if at.x + frame.size.width > right {
        at.x = (right - frame.size.width).max(room.origin.x);
    }
    // AppKit's y runs up from the screen's foot.
    if at.y < room.origin.y {
        at.y = room.origin.y;
    }
    let top = room.origin.y + room.size.height;
    if at.y + frame.size.height > top {
        at.y = top - frame.size.height;
    }
    if at.x != frame.origin.x || at.y != frame.origin.y {
        win.setFrameOrigin(at);
    }
}

/// Double-click on the title bar: zoom, minimize or nothing, as set in
/// System Settings › Desktop & Dock.
pub fn title_double_click(window: &Window) {
    let Some(win) = ns_window(window) else { return };
    let action = NSUserDefaults::standardUserDefaults().stringForKey(ns_string!("AppleActionOnDoubleClick"));
    match action.map(|a| a.to_string()).as_deref() {
        Some("Minimize") => win.performMiniaturize(None),
        Some("None") => {}
        _ => win.performZoom(None),
    }
}

/// A blocking error alert, for when Flow can't start. Flow.app has no
/// console to print to.
pub fn show_error(message: &str) {
    eprintln!("{message}");
    let Some(mtm) = MainThreadMarker::new() else { return };
    let (title, detail) = message.split_once("\n\n").unwrap_or((message, ""));
    let alert = NSAlert::new(mtm);
    alert.setMessageText(&NSString::from_str(title.trim_end_matches(':')));
    alert.setInformativeText(&NSString::from_str(detail));
    alert.runModal();
}
