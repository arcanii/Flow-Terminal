//! The primary selection: what was last selected anywhere, which a middle
//! click pastes. Flow's own clipboard (`Ctrl+Shift+C`) is the core's.

use std::sync::Mutex;

use arboard::{Clipboard, GetExtLinux, LinuxClipboardKind, SetExtLinux};

/// Kept while Flow runs: the program that owns the selection serves it to
/// the others, so it would go with this. Let go as Flow exits ([`release`]).
static CLIPBOARD: Mutex<Option<Clipboard>> = Mutex::new(None);

fn with<T>(f: impl FnOnce(&mut Clipboard) -> Option<T>) -> Option<T> {
    let mut held = CLIPBOARD.lock().unwrap_or_else(|e| e.into_inner());
    if held.is_none() {
        *held = Clipboard::new().map_err(|e| log::warn!("no primary selection: {e}")).ok();
    }
    f(held.as_mut()?)
}

pub fn get() -> Option<String> {
    with(|c| c.get().clipboard(LinuxClipboardKind::Primary).text().ok()).filter(|t| !t.is_empty())
}

pub fn set(text: &str) {
    with(|c| c.set().clipboard(LinuxClipboardKind::Primary).text(text.to_owned()).ok());
}

/// Drops the clipboard as Flow exits, before the core's own. arboard's
/// clipboards share one connection, and only the last to go hands what was
/// copied to the clipboard manager, so this one mustn't outlive the core's.
pub fn release() {
    CLIPBOARD.lock().unwrap_or_else(|e| e.into_inner()).take();
}
