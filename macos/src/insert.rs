//! Text macOS puts into the window without a keystroke, such as an emoji from
//! Emoji & Symbols, or dictation. winit takes inserted text only while an
//! input method is composing and drops the rest, so Flow catches that itself
//! and has it typed like an input method's (`UserEvent::Insert`).
//!
//! It swaps in its own `insertText:replacementRange:` and `keyDown:` on
//! winit's view class, each calling winit's own first: text inserted while a
//! key is handled is that key's, which winit sends as a key, so only text
//! from elsewhere is passed on.

use std::cell::Cell;
use std::sync::{Once, OnceLock};

use flow_core::event::UserEvent;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, Imp, Sel};
use objc2::{msg_send, sel};
use objc2_foundation::{NSRange, NSString};

type InsertText = unsafe extern "C-unwind" fn(&AnyObject, Sel, &AnyObject, NSRange);
type KeyDown = unsafe extern "C-unwind" fn(&AnyObject, Sel, &AnyObject);

/// winit's own methods, called first from Flow's.
static INSERT_TEXT: OnceLock<Imp> = OnceLock::new();
static KEY_DOWN: OnceLock<Imp> = OnceLock::new();

thread_local! {
    /// winit's view is handling a key: text inserted now belongs to it.
    static IN_KEY: Cell<bool> = const { Cell::new(false) };
}

/// Starts catching inserted text, once winit's view class exists (after the
/// window is made). Leaves winit alone if its class or methods aren't the
/// ones this was written for.
pub fn catch_inserted_text() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let Some(class) = AnyClass::get(c"WinitView") else { return };
        // Only methods the class defines itself: swapping one it inherits
        // would change NSView for every view in the app.
        let own = |sel: Sel| class.instance_methods().iter().any(|m| m.name() == sel);
        let (insert, key) = (sel!(insertText:replacementRange:), sel!(keyDown:));
        if !own(insert) || !own(key) {
            log::warn!("winit's view isn't as expected; emoji from Emoji & Symbols won't be typed");
            return;
        }
        // SAFETY: both functions have the signatures of the methods they
        // replace, and the old ones are kept to be called with the same
        // arguments.
        unsafe {
            if let Some(m) = class.instance_method(insert) {
                let _ = INSERT_TEXT.set(m.set_implementation(std::mem::transmute::<InsertText, Imp>(insert_text)));
            }
            if let Some(m) = class.instance_method(key) {
                let _ = KEY_DOWN.set(m.set_implementation(std::mem::transmute::<KeyDown, Imp>(key_down)));
            }
        }
    });
}

unsafe extern "C-unwind" fn key_down(this: &AnyObject, cmd: Sel, event: &AnyObject) {
    let was = IN_KEY.replace(true);
    if let Some(&imp) = KEY_DOWN.get() {
        // SAFETY: winit's keyDown:, with its own arguments.
        unsafe { std::mem::transmute::<Imp, KeyDown>(imp)(this, cmd, event) };
    }
    IN_KEY.set(was);
}

unsafe extern "C-unwind" fn insert_text(this: &AnyObject, cmd: Sel, text: &AnyObject, range: NSRange) {
    // SAFETY: winit's view answers hasMarkedText (NSTextInputClient).
    let composing: bool = unsafe { msg_send![this, hasMarkedText] };
    if let Some(&imp) = INSERT_TEXT.get() {
        // SAFETY: winit's insertText:replacementRange:, with its own arguments.
        unsafe { std::mem::transmute::<Imp, InsertText>(imp)(this, cmd, text, range) };
    }
    // An input method's text winit took itself, and a key's it sends as a key.
    if composing || IN_KEY.get() {
        return;
    }
    let typed: String = plain_text(text).chars().filter(|c| !c.is_control()).collect();
    if !typed.is_empty() {
        crate::menu::send(UserEvent::Insert(typed));
    }
}

/// The text of an NSString or NSAttributedString, which is what AppKit sends.
fn plain_text(text: &AnyObject) -> String {
    // SAFETY: every object answers isKindOfClass:.
    let attributed = AnyClass::get(c"NSAttributedString").is_some_and(|class| unsafe { msg_send![text, isKindOfClass: class] });
    if attributed {
        // SAFETY: an NSAttributedString's `string` is an NSString.
        let string: Retained<NSString> = unsafe { msg_send![text, string] };
        return string.to_string();
    }
    match text.downcast_ref::<NSString>() {
        Some(string) => string.to_string(),
        None => String::new(),
    }
}
