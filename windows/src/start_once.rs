//! Flow started once at the next login, for a task the assistant left
//! waiting for a restart: a value in the user's `RunOnce` key, which
//! Windows takes away as it runs it. Every start takes it away too, in case
//! Flow was opened before the restart came.

/// The user's key of programs to run once at the next login.
const RUN_ONCE: &str = r"Software\Microsoft\Windows\CurrentVersion\RunOnce";
/// Flow's value in it.
const NAME: &str = "Flow";

/// Puts flow.exe in `RunOnce` (`on`), or takes it out: whether it's as
/// asked, a value that isn't there being out already.
pub fn start_once(on: bool) -> bool {
    if on {
        let Ok(exe) = std::env::current_exe() else { return false };
        let command = format!("\"{}\"", exe.display());
        return crate::notify::set_values(RUN_ONCE, &[(NAME, &command)]).map_err(|e| log::warn!("can't start once at login: {e}")).is_ok();
    }
    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    use windows_sys::Win32::System::Registry::{RegDeleteKeyValueW, HKEY_CURRENT_USER};
    let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let (key, name) = (wide(RUN_ONCE), wide(NAME));
    let gone = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr()) };
    gone == ERROR_SUCCESS || gone == ERROR_FILE_NOT_FOUND
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flow_goes_in_run_once_and_comes_out_again() {
        // Out first, so a value left from before doesn't count; out again
        // at the end, so the PC doesn't start Flow at its next login.
        assert!(start_once(false));
        assert!(start_once(true));
        assert!(start_once(false));
        assert!(start_once(false), "a value that isn't there is out already");
    }
}
