//! A long command done, told of by a toast in Windows' notifications: the
//! tab's name, the command and how it ended, its click bringing Flow forward
//! with that tab and pane in front, as the Mac's notification does.
//!
//! Windows shows a desktop app's toast only under an AppUserModelID it knows.
//! Flow registers its own for the user (`HKCU\Software\Classes\AppUserModelId`,
//! with its name and icon) rather than through a Start menu shortcut, so the
//! portable zip and a build run from its folder have toasts too, and the
//! shortcuts, and how the taskbar groups Flow's window with one pinned
//! before, stay as they were. Where a toast can't be shown (Windows turned
//! them off for Flow, or WinRT failed) the taskbar button flashes instead,
//! as it did before toasts.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use flow_core::event::UserEvent;
use flow_core::session::Waker;
use windows::core::HSTRING;
use windows::Data::Xml::Dom::XmlDocument;
use windows::Foundation::TypedEventHandler;
use windows::UI::Notifications::{NotificationSetting, ToastNotification, ToastNotificationManager};
use winit::window::Window;

/// The name Windows knows Flow's toasts by, as Linux's window is known.
pub const APP_USER_MODEL_ID: &str = "com.arcanii.Flow";

/// Where Windows reads the AppUserModelID's name and icon, under the user's
/// own classes.
const REGISTERED: &str = r"Software\Classes\AppUserModelId\com.arcanii.Flow";

/// The icon a toast wears, written beside the config for the registration
/// to name.
const ICON: &[u8] = include_bytes!("../../assets/icon/flow-256.png");

/// How many toasts are kept alive for their clicks: one a tab's pane, as a
/// pane's next takes its place.
const KEPT: usize = 16;

/// What hands a toast's click back to the app.
static WAKER: Mutex<Option<Waker>> = Mutex::new(None);

/// The toasts shown lately, held so their clicks still come while they're
/// up or in the notification center.
static SHOWN: Mutex<VecDeque<ToastNotification>> = Mutex::new(VecDeque::new());

/// Whether the taskbar button was set flashing and not yet stopped.
static FLASHING: AtomicBool = AtomicBool::new(false);

/// Keeps `waker` for the clicks, and registers Flow's toasts with Windows
/// (once a run, before any is shown).
pub fn prepare(waker: Waker) {
    if let Ok(mut kept) = WAKER.lock() {
        *kept = Some(waker);
    }
    if let Err(e) = register() {
        log::warn!("notifications: Flow's toasts not registered: {e}");
    }
}

/// Shows a toast titled `title` saying `body`, which a newer one under the
/// same `tag` (the same pane's next command) replaces; the taskbar button
/// flashes where none can be shown.
pub fn notify(window: &Window, title: &str, body: &str, tag: u64) {
    if let Err(e) = toast(title, body, tag) {
        log::info!("notifications: no toast ({e}), so the taskbar button flashes");
        // winit's informational attention is FlashWindowEx with FLASHW_TRAY
        // and FLASHW_TIMERNOFG: until the window is in front again.
        window.request_user_attention(Some(winit::window::UserAttentionType::Informational));
        FLASHING.store(true, Ordering::Relaxed);
    }
}

/// Stops the taskbar button's flashing as the window comes to the front.
/// FLASHW_TIMERNOFG was to stop it then, but the button went on flashing
/// after a click on it brought Flow forward; and winit asked to stop does
/// nothing while the window is the active one, as it is by then.
pub fn stop_flashing(window: &Window) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{FlashWindowEx, FLASHWINFO, FLASHW_STOP};
    if !FLASHING.swap(false, Ordering::Relaxed) {
        return;
    }
    let Some(hwnd) = crate::window::hwnd(window) else { return };
    let info = FLASHWINFO { cbSize: std::mem::size_of::<FLASHWINFO>() as u32, hwnd, dwFlags: FLASHW_STOP, uCount: 0, dwTimeout: 0 };
    unsafe { FlashWindowEx(&info) };
}

fn toast(title: &str, body: &str, tag: u64) -> Result<(), String> {
    let notifier = ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(APP_USER_MODEL_ID)).map_err(|e| e.to_string())?;
    // Turned off for Flow, for the user or by policy: the flash instead.
    // The notifier can't say for an AppUserModelID registered as Flow's is,
    // without a Start menu shortcut (it fails with ERROR_NOT_FOUND), so the
    // switches it would read are read from the registry then.
    match notifier.Setting() {
        Ok(setting) if setting != NotificationSetting::Enabled => return Err(format!("Windows has them off ({})", setting.0)),
        Ok(_) => {}
        Err(_) if toasts_off() => return Err("Windows' notifications are off, or Flow's are".into()),
        Err(_) => {}
    }
    let xml = XmlDocument::new().map_err(|e| e.to_string())?;
    xml.LoadXml(&HSTRING::from(content(title, body))).map_err(|e| e.to_string())?;
    let toast = ToastNotification::CreateToastNotification(&xml).map_err(|e| e.to_string())?;
    let _ = toast.SetTag(&HSTRING::from(tag.to_string()));
    let _ = toast.SetGroup(&HSTRING::from("commands"));
    // Comes on a thread of WinRT's own; the waker hands it to the app.
    let clicked = TypedEventHandler::new(move |_, _| {
        if let Some(waker) = WAKER.lock().ok().and_then(|w| w.clone()) {
            waker.send(UserEvent::NotificationClicked(tag));
        }
        Ok(())
    });
    toast.Activated(&clicked).map_err(|e| e.to_string())?;
    notifier.Show(&toast).map_err(|e| e.to_string())?;
    if let Ok(mut shown) = SHOWN.lock() {
        shown.push_back(toast);
        while shown.len() > KEPT {
            shown.pop_front();
        }
    }
    Ok(())
}

/// Whether the user has turned toasts off, all of them (Settings → System →
/// Notifications) or Flow's alone (its row there), as the registry keeps
/// those switches: a 0 for either.
fn toasts_off() -> bool {
    let off = |subkey: &str, value: &str| dword(subkey, value) == Some(0);
    off(r"Software\Microsoft\Windows\CurrentVersion\PushNotifications", "ToastEnabled")
        || off(&format!(r"Software\Microsoft\Windows\CurrentVersion\Notifications\Settings\{APP_USER_MODEL_ID}"), "Enabled")
}

/// A DWORD under `subkey` of the user's registry, where there is one.
fn dword(subkey: &str, value: &str) -> Option<u32> {
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
    let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let (key, name) = (wide(subkey), wide(value));
    let (mut data, mut size) = (0u32, std::mem::size_of::<u32>() as u32);
    let read = unsafe { RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr(), RRF_RT_REG_DWORD, std::ptr::null_mut(), (&mut data as *mut u32).cast(), &mut size) };
    (read == ERROR_SUCCESS).then_some(data)
}

/// The toast's XML: its title and its line, each escaped.
fn content(title: &str, body: &str) -> String {
    format!(
        r#"<toast><visual><binding template="ToastGeneric"><text>{}</text><text>{}</text></binding></visual></toast>"#,
        escaped(title),
        escaped(body)
    )
}

/// `text` as XML's character data and attribute values take it.
fn escaped(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            // XML 1.0 has no place for the other control characters.
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => {}
            c => out.push(c),
        }
    }
    out
}

/// Names Flow's AppUserModelID for the user, with Flow's name and icon,
/// which Windows shows on each toast and lists in Settings' notifications.
fn register() -> Result<(), String> {
    let icon = flow_core::config_dir().join("flow-notification.png");
    if std::fs::read(&icon).ok().as_deref() != Some(ICON) {
        if let Some(dir) = icon.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        std::fs::write(&icon, ICON).map_err(|e| format!("{}: {e}", icon.display()))?;
    }
    set_values(REGISTERED, &[("DisplayName", flow_core::APP_NAME), ("IconUri", &icon.display().to_string())])
}

/// Sets string values under `subkey` of the user's registry, making the key
/// where it isn't.
pub(crate) fn set_values(subkey: &str, values: &[(&str, &str)]) -> Result<(), String> {
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ,
    };
    let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let mut key: HKEY = std::ptr::null_mut();
    let path = wide(subkey);
    let made = unsafe {
        RegCreateKeyExW(HKEY_CURRENT_USER, path.as_ptr(), 0, std::ptr::null(), REG_OPTION_NON_VOLATILE, KEY_SET_VALUE, std::ptr::null(), &mut key, std::ptr::null_mut())
    };
    if made != ERROR_SUCCESS {
        return Err(format!("HKCU\\{subkey}: error {made}"));
    }
    let mut result = Ok(());
    for (name, value) in values {
        let (name, data) = (wide(name), wide(value));
        let set = unsafe { RegSetValueExW(key, name.as_ptr(), 0, REG_SZ, data.as_ptr().cast(), (data.len() * 2) as u32) };
        if set != ERROR_SUCCESS && result.is_ok() {
            result = Err(format!("HKCU\\{subkey}: error {set}"));
        }
    }
    unsafe { RegCloseKey(key) };
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_toasts_words_are_escaped_as_xml() {
        assert_eq!(escaped(r#"a < b && "c" > 'd'"#), "a &lt; b &amp;&amp; &quot;c&quot; &gt; &apos;d&apos;");
        assert_eq!(escaped("tab\tline\nbell\x07"), "tab\tline\nbell");
        let xml = content("pwsh 2", "cargo build <release> finished");
        assert!(xml.contains("<text>pwsh 2</text><text>cargo build &lt;release&gt; finished</text>"), "{xml}");
    }

    #[test]
    fn the_users_notification_switch_is_read_from_the_registry() {
        // Every Windows 10 and 11 keeps it once notifications were ever set;
        // either way, reading it mustn't fail, and a key that isn't there
        // reads as none.
        let global = dword(r"Software\Microsoft\Windows\CurrentVersion\PushNotifications", "ToastEnabled");
        assert!(matches!(global, None | Some(0) | Some(1)), "{global:?}");
        assert_eq!(dword(r"Software\Flow\NoSuchKey", "Enabled"), None);
    }

    #[test]
    fn a_toast_is_read_by_windows_xml() {
        let xml = XmlDocument::new().unwrap();
        xml.LoadXml(&HSTRING::from(content("Flow & co", "it's \"done\" <0>"))).unwrap();
        let texts = xml.GetElementsByTagName(&HSTRING::from("text")).unwrap();
        assert_eq!(texts.Length().unwrap(), 2);
        assert_eq!(texts.Item(1).unwrap().InnerText().unwrap().to_string(), "it's \"done\" <0>");
    }
}
