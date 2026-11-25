//! The window on Linux: the name desktops know Flow by (which finds its
//! .desktop file and icon), the icon X11 shows, startup notification, the
//! window manager's menu and error dialogs.

use winit::dpi::PhysicalPosition;
use winit::platform::startup_notify::{self, WindowAttributesExtStartupNotify};
use winit::platform::wayland::WindowAttributesExtWayland;
use winit::window::{ActivationToken, Icon, Window, WindowAttributes};

/// Wayland's app id and X11's WM_CLASS: the name of Flow's .desktop file
/// (`packaging/com.arcanii.Flow.desktop`) and icon, as on macOS its bundle id.
pub const APP_ID: &str = "com.arcanii.Flow";

pub fn attributes(attrs: WindowAttributes) -> WindowAttributes {
    // The same setting for X11's WM_CLASS ("flow", APP_ID).
    let mut attrs = WindowAttributesExtWayland::with_name(attrs, APP_ID, "flow").with_window_icon(icon());
    // A desktop that starts Flow hands it a token for its window to take
    // focus with and end the launch's busy cursor: XDG_ACTIVATION_TOKEN on
    // Wayland, which winit runs on when it can, DESKTOP_STARTUP_ID on X11.
    // Launchers on Wayland set both; one that passes only a socket has had
    // WAYLAND_SOCKET taken away by now, so take either then.
    let var = |name| std::env::var(name).ok().filter(|t| !t.is_empty());
    let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some_and(|s| !s.is_empty());
    let token = if wayland {
        var("XDG_ACTIVATION_TOKEN").or_else(|| var("DESKTOP_STARTUP_ID"))
    } else {
        var("DESKTOP_STARTUP_ID").or_else(|| var("XDG_ACTIVATION_TOKEN"))
    };
    if let Some(token) = token {
        attrs = attrs.with_activation_token(ActivationToken::from_raw(token));
    }
    // Spent once the window opens, so the shells and what they start mustn't
    // take it up. No other thread reads the environment yet.
    startup_notify::reset_activation_token_env();
    attrs
}

/// Flow's icon for X11, which shows a window's own where no .desktop file
/// names one. Wayland takes it from the .desktop file.
fn icon() -> Option<Icon> {
    let bytes: &[u8] = include_bytes!("../../assets/icon/flow-128.png");
    let mut reader = png::Decoder::new(std::io::Cursor::new(bytes)).read_info().ok()?;
    let mut buf = vec![0u8; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        return None;
    }
    buf.truncate(info.buffer_size());
    Icon::from_rgba(buf, info.width, info.height).ok()
}

/// The window manager's menu for a right click on Flow's title bar. winit
/// opens it on Wayland; on X11 it has no way to, so this says false there.
pub fn show_window_menu(window: &Window, at: (f32, f32)) -> bool {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let wayland = window.window_handle().is_ok_and(|h| matches!(h.as_raw(), RawWindowHandle::Wayland(_)));
    if wayland {
        window.show_window_menu(PhysicalPosition::new(at.0 as f64, at.1 as f64));
    }
    wayland
}

/// A blocking error dialog, for when Flow can't start: one opened from the
/// desktop has no terminal to print to. rfd shows it with zenity, where
/// that's installed.
pub fn show_error(message: &str) {
    eprintln!("{message}");
    let (title, detail) = message.split_once("\n\n").unwrap_or((message, ""));
    let _ = rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title(flow_core::APP_NAME)
        .set_description(if detail.is_empty() { title.to_string() } else { format!("{}\n\n{detail}", title.trim_end_matches(':')) })
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}
