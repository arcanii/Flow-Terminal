//! Flow's icon in the notification area while it's put away
//! (`Platform::away_icon`, docs/design/put-away.md). A put-away window is
//! hidden, with no taskbar button, so the icon shows Flow is still there:
//! a click brings it back (`UserEvent::AwayIconShow`), and its menu has
//! Show Flow and Quit Flow (`AwayIconQuit`). Its messages come to a window
//! of its own, never shown, on a `flow-tray` thread, made the first time
//! it's needed; not message-only, since that would never hear
//! `TaskbarCreated`, which says Explorer started again and the icon must
//! be added again.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{mpsc, Mutex};

use flow_core::event::UserEvent;
use flow_core::i18n::t;
use flow_core::session::Waker;
use windows_sys::Win32::Foundation::{HANDLE, HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NIM_SETVERSION, NINF_KEY,
    NIN_SELECT, NOTIFYICONDATAW, NOTIFYICON_VERSION_4,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, ChangeWindowMessageFilterEx, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DispatchMessageW,
    GetMessageW, GetSystemMetrics, LoadIconW, LoadImageW, PostMessageW, RegisterClassW, RegisterWindowMessageW, SetForegroundWindow,
    SetMenuDefaultItem, TrackPopupMenuEx, TranslateMessage, HICON, IDI_APPLICATION, IMAGE_ICON, LR_DEFAULTCOLOR, MF_STRING,
    MSG, MSGFLT_ALLOW, SM_CXSMICON, TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON, WM_APP, WM_CONTEXTMENU, WM_NULL, WNDCLASSW,
    WS_EX_TOOLWINDOW, WS_POPUP,
};

/// The message the notification area sends the icon's clicks as.
const MESSAGE: u32 = WM_APP + 1;
/// The icon's number for its window: Flow has the one.
const ID: u32 = 1;
/// The menu's items.
const SHOW: usize = 1;
const QUIT: usize = 2;
/// Enter or Space on the icon from the keyboard (`NIN_SELECT | NINF_KEY`).
const NIN_KEYSELECT: u32 = NIN_SELECT | NINF_KEY;

/// The waker `install_menus` was given, which the icon's clicks go through.
static WAKER: Mutex<Option<Waker>> = Mutex::new(None);
/// The icon's window and picture, once made, its words, and whether it's
/// in the notification area now.
static ICON: Mutex<Option<Icon>> = Mutex::new(None);
/// `TaskbarCreated`'s number, which Windows gives each program asking.
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

struct Icon {
    /// Handles as numbers, since a pointer isn't `Send`.
    window: usize,
    picture: usize,
    tip: [u16; 128],
    shown: bool,
}

pub fn prepare(waker: Waker) {
    *WAKER.lock().unwrap_or_else(|e| e.into_inner()) = Some(waker);
}

/// Shows the icon with `tip` over it, or changes its words if it shows.
pub fn show(tip: &str) {
    let mut icon = ICON.lock().unwrap_or_else(|e| e.into_inner());
    if icon.is_none() {
        *icon = make();
    }
    let Some(icon) = icon.as_mut() else { return };
    icon.tip = tip_of(tip);
    if icon.shown {
        call(NIM_MODIFY, icon);
    } else {
        add(icon);
    }
}

/// Takes the icon away, if it shows.
pub fn hide() {
    let mut icon = ICON.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(icon) = icon.as_mut().filter(|i| i.shown) {
        call(NIM_DELETE, icon);
        icon.shown = false;
    }
}

/// `tip` as the notification area takes it: at most 127 letters and the
/// ending nothing, cut short with … where it's longer.
fn tip_of(tip: &str) -> [u16; 128] {
    let mut words: Vec<u16> = tip.encode_utf16().collect();
    if words.len() > 127 {
        words.truncate(126);
        // A cut through a surrogate pair would leave half a letter.
        if words.last().is_some_and(|w| (0xd800..0xdc00).contains(w)) {
            words.pop();
        }
        words.push('…' as u16);
    }
    let mut out = [0u16; 128];
    out[..words.len()].copy_from_slice(&words);
    out
}

fn add(icon: &mut Icon) {
    icon.shown = call(NIM_ADD, icon);
    if icon.shown {
        // Version 4's clicks: NIN_SELECT, NIN_KEYSELECT and WM_CONTEXTMENU,
        // with where the menu goes.
        call(NIM_SETVERSION, icon);
    }
}

fn call(message: u32, icon: &Icon) -> bool {
    let mut data = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: icon.window as HWND,
        uID: ID,
        uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP,
        uCallbackMessage: MESSAGE,
        hIcon: icon.picture as HICON,
        szTip: icon.tip,
        ..Default::default()
    };
    data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
    // SAFETY: the data is filled in as the call reads it, for a window of
    // Flow's own.
    unsafe { Shell_NotifyIconW(message, &data) != 0 }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Makes the icon's window on a thread of its own, which then runs its
/// messages for good, and loads its picture: flow.exe's own icon
/// (resource 1, which the root build.rs embeds), else Windows' plain one.
fn make() -> Option<Icon> {
    let (said, heard) = mpsc::channel();
    let started = std::thread::Builder::new().name("flow-tray".into()).spawn(move || {
        // SAFETY: the class and window are this thread's own; the strings
        // outlive the calls, and `msg` is written by GetMessageW.
        unsafe {
            let instance = GetModuleHandleW(std::ptr::null());
            let class = wide("FlowAwayIcon");
            let mut kind: WNDCLASSW = std::mem::zeroed();
            kind.lpfnWndProc = Some(window_proc);
            kind.hInstance = instance;
            kind.lpszClassName = class.as_ptr();
            RegisterClassW(&kind);
            let title = wide("Flow");
            let window = CreateWindowExW(
                WS_EX_TOOLWINDOW,
                class.as_ptr(),
                title.as_ptr(),
                WS_POPUP,
                0,
                0,
                0,
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                instance,
                std::ptr::null(),
            );
            if window.is_null() {
                let _ = said.send(None);
                return;
            }
            let created = RegisterWindowMessageW(wide("TaskbarCreated").as_ptr());
            TASKBAR_CREATED.store(created, Ordering::Relaxed);
            // An administrator's Flow still hears Explorer's.
            ChangeWindowMessageFilterEx(window, created, MSGFLT_ALLOW, std::ptr::null_mut());
            let _ = said.send(Some(window as usize));
            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    });
    if started.is_err() {
        return None;
    }
    let window = heard.recv().ok().flatten()?;
    // SAFETY: loads an icon from this program's own resources, or the
    // system's; the size is the notification area's.
    let picture = unsafe {
        let size = GetSystemMetrics(SM_CXSMICON);
        let own: HANDLE = LoadImageW(GetModuleHandleW(std::ptr::null()), std::ptr::without_provenance(1), IMAGE_ICON, size, size, LR_DEFAULTCOLOR);
        if own.is_null() {
            LoadIconW(std::ptr::null_mut(), IDI_APPLICATION) as usize
        } else {
            own as usize
        }
    };
    Some(Icon { window, picture, tip: [0; 128], shown: false })
}

fn send(event: UserEvent) {
    if let Some(waker) = WAKER.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        waker.send(event);
    }
}

unsafe extern "system" fn window_proc(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if message == MESSAGE {
        match lparam as u32 & 0xffff {
            NIN_SELECT | NIN_KEYSELECT => send(UserEvent::AwayIconShow),
            WM_CONTEXTMENU => {
                // Version 4 gives where the menu goes in wparam.
                let x = (wparam & 0xffff) as u16 as i16 as i32;
                let y = ((wparam >> 16) & 0xffff) as u16 as i16 as i32;
                menu(window, x, y);
            }
            _ => {}
        }
        return 0;
    }
    let created = TASKBAR_CREATED.load(Ordering::Relaxed);
    if created != 0 && message == created {
        // Explorer started again with an empty notification area.
        let mut icon = ICON.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(icon) = icon.as_mut().filter(|i| i.shown) {
            add(icon);
        }
        return 0;
    }
    // SAFETY: Windows' own handling of everything else.
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}

/// The icon's menu at (x, y): Show Flow, the default, and Quit Flow, on
/// the icon's own thread, for its window.
fn menu(window: HWND, x: i32, y: i32) {
    let show = wide(t("tray.show"));
    let quit = wide(t("tray.quit"));
    // SAFETY: a menu made and destroyed here, its strings outliving it;
    // the window takes the keys first, as a notification icon's menu must
    // or it won't close when clicked away from, and the empty message
    // after is Windows' advice for the same.
    let chosen = unsafe {
        let menu = CreatePopupMenu();
        if menu.is_null() {
            return;
        }
        AppendMenuW(menu, MF_STRING, SHOW, show.as_ptr());
        AppendMenuW(menu, MF_STRING, QUIT, quit.as_ptr());
        SetMenuDefaultItem(menu, SHOW as u32, 0);
        SetForegroundWindow(window);
        let chosen = TrackPopupMenuEx(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_NONOTIFY, x, y, window, std::ptr::null());
        PostMessageW(window, WM_NULL, 0, 0);
        DestroyMenu(menu);
        chosen
    };
    match chosen as usize {
        SHOW => send(UserEvent::AwayIconShow),
        QUIT => send(UserEvent::AwayIconQuit),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tip_fits_the_notification_area() {
        let short = tip_of("Flow is put away");
        assert_eq!(String::from_utf16_lossy(&short[..16]), "Flow is put away");
        assert_eq!(short[16], 0);
        let long = tip_of(&"x".repeat(300));
        assert_eq!(long[126], '…' as u16);
        assert_eq!(long[127], 0, "it always ends with nothing");
        // A letter of two halves isn't cut through.
        let pairs = tip_of(&format!("{}{}", "x".repeat(125), "😀😀"));
        assert_eq!(pairs[125], '…' as u16);
        assert_eq!(pairs[126], 0);
    }
}
