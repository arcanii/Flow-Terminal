//! Administrator rights for the assistant's task: whether Flow runs
//! elevated (its process token's `TokenElevation`), and Flow opened again as
//! administrator for a task (`ShellExecuteExW`'s `runas`), its UAC prompt
//! the user's yes. Windows gives an elevated program a window of its own,
//! which a program that isn't can't drive, so the task carries on there
//! (`--carry-on`), pressed for in that window.

/// Whether this Flow runs elevated.
pub fn elevated() -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    let mut token: HANDLE = std::ptr::null_mut();
    // SAFETY: the current process's pseudo-handle; the token is closed below.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return false;
    }
    let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
    let mut len = 0u32;
    // SAFETY: `elevation` is as large as the size given.
    let ok = unsafe {
        GetTokenInformation(token, TokenElevation, (&mut elevation as *mut TOKEN_ELEVATION).cast(), std::mem::size_of::<TOKEN_ELEVATION>() as u32, &mut len)
    };
    // SAFETY: opened above.
    unsafe { CloseHandle(token) };
    ok != 0 && elevation.TokenIsElevated != 0
}

/// Opens flow.exe again as administrator with `--carry-on <task>`: false
/// where it couldn't, or the user said no to UAC's prompt.
pub fn relaunch(task: i64) -> bool {
    use windows_sys::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW};
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let Ok(exe) = std::env::current_exe() else { return false };
    let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let (verb, file, params) = (wide("runas"), wide(&exe.display().to_string()), wide(&format!("{} {task}", flow_core::CARRY_ON)));
    // SAFETY: a zeroed SHELLEXECUTEINFOW is valid; its strings outlive the call.
    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOASYNC;
    info.lpVerb = verb.as_ptr();
    info.lpFile = file.as_ptr();
    info.lpParameters = params.as_ptr();
    info.nShow = SW_SHOWNORMAL;
    // SAFETY: `info` is filled in as the call reads it.
    let opened = unsafe { ShellExecuteExW(&mut info) } != 0;
    if !opened {
        log::warn!("couldn't open Flow as administrator: {}", std::io::Error::last_os_error());
    }
    opened
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whether_flow_is_elevated_can_be_read() {
        // A test run as administrator or not, either way it reads.
        let _ = elevated();
    }
}
