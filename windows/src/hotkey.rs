//! The drop-down hotkey on Windows (`Platform::register_hotkey`):
//! RegisterHotKey with no window, on a thread of its own, whose message
//! queue WM_HOTKEY then comes to (a hotkey without a window posts to the
//! thread that registered it), and each press sends `UserEvent::Hotkey`.
//! Another hotkey ends that thread, with its hotkey, and starts another.

use std::sync::{mpsc, Mutex};
use std::thread::JoinHandle;

use flow_core::event::UserEvent;
use flow_core::hotkey::{Hotkey, Key};
use flow_core::session::Waker;
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, VK_F1, VK_OEM_3, VK_SPACE,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetMessageW, PeekMessageW, PostThreadMessageW, MSG, PM_NOREMOVE, WM_HOTKEY, WM_QUIT};

/// The thread holding the hotkey: its id, which WM_QUIT is posted to, and
/// its handle, to wait for it to let go.
static HOLDER: Mutex<Option<(u32, JoinHandle<()>)>> = Mutex::new(None);

/// Takes `hotkey` for the whole system in place of the one before; None
/// lets go of it. False where another program has it.
pub fn register(hotkey: Option<Hotkey>, waker: Waker) -> bool {
    let mut holder = HOLDER.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((thread, handle)) = holder.take() {
        // SAFETY: posts to a thread of Flow's own, which has a queue.
        unsafe { PostThreadMessageW(thread, WM_QUIT, 0, 0) };
        let _ = handle.join();
    }
    let Some(hotkey) = hotkey else { return true };
    let modifiers = [(hotkey.ctrl, MOD_CONTROL), (hotkey.alt, MOD_ALT), (hotkey.shift, MOD_SHIFT), (hotkey.cmd, MOD_WIN)]
        .iter()
        .filter(|(on, _)| *on)
        .fold(MOD_NOREPEAT, |all, (_, m)| all | m);
    let key = virtual_key(hotkey.key) as u32;
    let (said, heard) = mpsc::channel();
    let started = std::thread::Builder::new().name("flow-hotkey".into()).spawn(move || {
        // SAFETY: the thread's own message queue, made first (PeekMessage)
        // so a WM_QUIT posted once its id is known finds it; a hotkey with
        // no window, let go of on the same thread.
        unsafe {
            let mut msg = MSG::default();
            PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_NOREMOVE);
            let taken = RegisterHotKey(std::ptr::null_mut(), 1, modifiers, key) != 0;
            let _ = said.send((GetCurrentThreadId(), taken));
            if !taken {
                return;
            }
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                if msg.message == WM_HOTKEY {
                    waker.send(UserEvent::Hotkey);
                }
            }
            UnregisterHotKey(std::ptr::null_mut(), 1);
        }
    });
    let Ok(handle) = started else { return false };
    match heard.recv() {
        Ok((thread, true)) => {
            *holder = Some((thread, handle));
            true
        }
        _ => {
            let _ = handle.join();
            false
        }
    }
}

/// Windows' virtual key for `key`; the key left of 1 is VK_OEM_3 on US
/// keyboards.
fn virtual_key(key: Key) -> u16 {
    match key {
        Key::Grave => VK_OEM_3,
        Key::Space => VK_SPACE,
        Key::F(n) => VK_F1 + (n.clamp(1, 12) as u16 - 1),
        Key::Letter(c) => c.to_ascii_uppercase() as u16,
        Key::Digit(d) => b'0' as u16 + d.min(9) as u16,
    }
}
