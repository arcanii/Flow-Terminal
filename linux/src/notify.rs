//! A long command done, on Linux (`Platform::notify`): a desktop
//! notification through org.freedesktop.Notifications, which every
//! desktop's notification daemon answers, and the window marked as wanting
//! attention, as the taskbar or dock shows it. The notification is asked
//! for with the desktop's own command-line tools, on a thread of its own so
//! the window never waits: gdbus first, which writes the call's hints as
//! the daemon takes them; then notify-send; then dbus-send, which every
//! desktop has but which can't write an empty a{sv}, so daemons that check
//! the call's signature (GNOME's, KDE's) may turn it down. A click isn't
//! followed: that would take watching D-Bus for its signal.

use std::process::{Command, Stdio};

use winit::window::{UserAttentionType, Window};

use crate::window::APP_ID;

pub fn notify(window: &Window, title: &str, body: &str) {
    window.request_user_attention(Some(UserAttentionType::Informational));
    let (title, body) = (title.to_string(), body.to_string());
    let asking = std::thread::Builder::new().name("flow-notify".into()).spawn(move || {
        if !(gdbus(&title, &body) || notify_send(&title, &body) || dbus_send(&title, &body)) {
            log::info!("no desktop notification: gdbus, notify-send and dbus-send couldn't ask for one");
        }
    });
    if let Err(err) = asking {
        log::warn!("could not ask for a notification: {err}");
    }
}

/// Whether `command` ran and said it worked, saying nothing itself.
fn ran(command: &mut Command) -> bool {
    command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success())
}

fn gdbus(title: &str, body: &str) -> bool {
    ran(Command::new("gdbus")
        .args(["call", "--session", "--dest", "org.freedesktop.Notifications", "--object-path", "/org/freedesktop/Notifications"])
        .args(["--method", "org.freedesktop.Notifications.Notify"])
        // The timeout, -1, would read as an option without the `--`.
        .arg("--")
        .args([quoted(flow_core::APP_NAME), "0".into(), quoted(APP_ID), quoted(title), quoted(body), "[]".into(), "{}".into(), "-1".into()]))
}

fn notify_send(title: &str, body: &str) -> bool {
    ran(Command::new("notify-send").arg(format!("--app-name={}", flow_core::APP_NAME)).arg(format!("--icon={APP_ID}")).arg("--").arg(title).arg(body))
}

fn dbus_send(title: &str, body: &str) -> bool {
    ran(Command::new("dbus-send")
        .args(["--session", "--print-reply", "--type=method_call", "--dest=org.freedesktop.Notifications"])
        .args(["/org/freedesktop/Notifications", "org.freedesktop.Notifications.Notify"])
        .arg(format!("string:{}", flow_core::APP_NAME))
        .arg("uint32:0")
        .arg(format!("string:{APP_ID}"))
        .arg(format!("string:{title}"))
        .arg(format!("string:{body}"))
        .args(["array:string:", "dict:string:string:", "int32:-1"]))
}

/// `text` as a string in GVariant's text form, which gdbus reads its
/// arguments in: in single quotes, with backslashes and quotes escaped.
fn quoted(text: &str) -> String {
    format!("'{}'", text.replace('\\', "\\\\").replace('\'', "\\'"))
}
