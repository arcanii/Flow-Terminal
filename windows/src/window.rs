//! Window dressing on Windows: the backdrop material behind the glass, DWM's
//! rounded corners and dark frame, the running mutex and error dialogs.

use winit::window::Window;

/// Applies the frosted backdrop behind the transparent window.
pub fn apply_backdrop(window: &Window, kind: &str) {
    use window_vibrancy::{apply_acrylic, apply_blur, apply_mica, clear_acrylic, clear_blur, clear_mica};
    windows_frame(window);
    // Called again when the setting changes, so remove whatever is applied.
    let _ = clear_acrylic(window);
    let _ = clear_mica(window);
    let _ = clear_blur(window);
    let result = match kind {
        "none" => Ok(()),
        "mica" => apply_mica(window, Some(true)),
        "blur" => apply_blur(window, Some((10, 14, 24, 120))),
        _ => apply_acrylic(window, Some((10, 14, 24, 110))).or_else(|_| apply_blur(window, Some((10, 14, 24, 120)))),
    };
    if let Err(err) = result {
        log::warn!("backdrop '{kind}' unavailable: {err}");
    }
}

/// Creates the named mutex the installer's AppMutex looks for, so setup and
/// updates know Flow is still running. It lives until the process exits.
pub fn mark_running() {
    unsafe {
        use windows_sys::Win32::System::Threading::CreateMutexW;
        let name: Vec<u16> = "Flow.Running\0".encode_utf16().collect();
        let _ = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
    }
}

/// Shows a blocking error dialog; release builds have no console to print to.
pub fn show_error(message: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};
    let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let text = wide(message);
    let title = wide(flow_core::APP_NAME);
    unsafe {
        MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), MB_OK | MB_ICONERROR);
    }
}

/// Where the pointer is now, in the window's client pixels, as winit gives
/// `CursorMoved`: asked of the system, since winit sends none while a file
/// is dragged over the window, for where a drop lands.
pub fn pointer(window: &Window) -> Option<(f64, f64)> {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::Graphics::Gdi::ScreenToClient;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;
    let hwnd = hwnd(window)?;
    let mut p = POINT { x: 0, y: 0 };
    // SAFETY: both write the point they're given, which outlives the calls,
    // and the window's handle is winit's own, alive with the window.
    let ok = unsafe { GetCursorPos(&mut p) != 0 && ScreenToClient(hwnd, &mut p) != 0 };
    ok.then_some((p.x as f64, p.y as f64))
}

/// The window's handle, for the calls winit doesn't make.
pub(crate) fn hwnd(window: &Window) -> Option<windows_sys::Win32::Foundation::HWND> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let handle = window.window_handle().ok()?;
    let RawWindowHandle::Win32(h) = handle.as_raw() else { return None };
    Some(h.hwnd.get() as windows_sys::Win32::Foundation::HWND)
}

/// Sends the window behind every other, as Alt+Esc does: the keys go to the
/// window below it (the first that's shown, not minimized, not cloaked, not
/// a tool window and owned by none, the desktop where there's none), and it
/// goes to the bottom.
pub fn send_to_back(window: &Window) -> bool {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetWindow, GetWindowLongW, IsIconic, IsWindowVisible, SetForegroundWindow, SetWindowPos, GWL_EXSTYLE, GW_HWNDNEXT, GW_OWNER, HWND_BOTTOM, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, WS_EX_NOACTIVATE,
        WS_EX_TOOLWINDOW,
    };
    let Some(hwnd) = hwnd(window) else { return false };
    // SAFETY: each call takes a window handle from the system's own list
    // (or winit's, alive with the window) and writes only the cloaked flag,
    // a u32 that outlives the call and is as big as it's told.
    unsafe {
        let takes_keys = |w: HWND| {
            let ex = GetWindowLongW(w, GWL_EXSTYLE) as u32;
            let mut cloaked = 0u32;
            let _ = DwmGetWindowAttribute(w, DWMWA_CLOAKED as u32, (&mut cloaked as *mut u32).cast(), std::mem::size_of::<u32>() as u32);
            IsWindowVisible(w) != 0 && IsIconic(w) == 0 && cloaked == 0 && ex & (WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE) == 0 && GetWindow(w, GW_OWNER).is_null()
        };
        let mut next = GetWindow(hwnd, GW_HWNDNEXT);
        while !next.is_null() && !takes_keys(next) {
            next = GetWindow(next, GW_HWNDNEXT);
        }
        if SetWindowPos(hwnd, HWND_BOTTOM, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE) == 0 {
            return false;
        }
        if !next.is_null() {
            SetForegroundWindow(next);
        }
        true
    }
}

/// The work area of the screen the window is most on: the screen less the
/// taskbar and anything else docked at its edges, in the virtual screen's
/// pixels, as winit gives the window's place.
fn work_area(window: &Window) -> Option<windows_sys::Win32::Foundation::RECT> {
    use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST};
    let hwnd = hwnd(window)?;
    unsafe {
        let screen = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        (GetMonitorInfoW(screen, &mut info) != 0).then_some(info.rcWork)
    }
}

/// The room the window's screen leaves for windows (its work area), in
/// pixels.
pub fn screen_room(window: &Window) -> Option<(u32, u32)> {
    let room = work_area(window)?;
    let (w, h) = (room.right - room.left, room.bottom - room.top);
    (w > 0 && h > 0).then_some((w as u32, h as u32))
}

/// Moves the window back into its screen's work area where a new size ran
/// it off the right or the bottom, as little as that takes, its top-left
/// corner staying in the area above all, as the Mac's does.
pub fn keep_on_screen(window: &Window) {
    let Some(room) = work_area(window) else { return };
    let Ok(at) = window.outer_position() else { return };
    let size = window.outer_size();
    let (w, h) = (size.width as i32, size.height as i32);
    let (mut x, mut y) = (at.x, at.y);
    if x + w > room.right {
        x = (room.right - w).max(room.left);
    }
    if y + h > room.bottom {
        y = room.bottom - h;
    }
    y = y.max(room.top);
    if (x, y) != (at.x, at.y) {
        window.set_outer_position(winit::dpi::PhysicalPosition::new(x, y));
    }
}

/// Turns DWM's fades as the window is shown and hidden off while it's
/// dropped down (`on`), and back on after. The drop-down slides in and out
/// itself, and once up out of sight it's hidden and put back where it
/// drops (`App::tend_drop_down`): DWM fades a hidden window out over a
/// moment, wherever the window is meanwhile, so it showed at the drop-down's
/// place, the slide up ending in a jump down.
pub fn set_fades(window: &Window, on: bool) {
    use windows_sys::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_TRANSITIONS_FORCEDISABLED};
    let Some(hwnd) = hwnd(window) else { return };
    let off: i32 = (!on).into();
    // SAFETY: the window's handle is winit's own, alive with the window, and
    // the attribute is a BOOL read from a value that outlives the call.
    unsafe {
        DwmSetWindowAttribute(hwnd, DWMWA_TRANSITIONS_FORCEDISABLED as u32, (&off as *const i32).cast(), std::mem::size_of::<i32>() as u32);
    }
}

fn windows_frame(window: &Window) {
    use windows_sys::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWA_WINDOW_CORNER_PREFERENCE,
        DWMWCP_ROUND,
    };
    let Some(hwnd) = hwnd(window) else { return };
    unsafe {
        let dark: i32 = 1;
        DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE as u32, &dark as *const _ as _, 4);
        let corner = DWMWCP_ROUND;
        DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE as u32, &corner as *const _ as _, 4);
        // We draw our own hairline edge; hide the system one.
        let none: u32 = 0xFFFF_FFFE;
        DwmSetWindowAttribute(hwnd, DWMWA_BORDER_COLOR as u32, &none as *const _ as _, 4);
    }
}
