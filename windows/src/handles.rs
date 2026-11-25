//! A program keeping its own standard handles to itself when it starts one
//! that outlives it: `flow-mcp` starting Flow, the `flow` command starting
//! a guest Flow.

use std::os::windows::io::AsRawHandle;

use windows_sys::Win32::Foundation::{SetHandleInformation, HANDLE_FLAG_INHERIT};

/// Stops this program's standard input, output and error from going to
/// the programs it starts. Windows hands a child every handle marked
/// inheritable, whatever its own standard handles are set to, and these
/// come marked so from whatever started this program (an MCP app's pipes,
/// a script capturing what it prints): a Flow started from here held them
/// open as long as it ran, so the app reading them never saw them end.
/// This program goes on using them as before.
pub fn keep_std_handles() {
    let handles = [std::io::stdin().as_raw_handle(), std::io::stdout().as_raw_handle(), std::io::stderr().as_raw_handle()];
    for handle in handles {
        if !handle.is_null() {
            // SAFETY: a handle of this process's own, or none, which the
            // call refuses; failing changes nothing.
            unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT, 0) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::Foundation::GetHandleInformation;

    #[test]
    fn this_programs_own_standard_handles_stop_being_handed_on() {
        keep_std_handles();
        for handle in [std::io::stdin().as_raw_handle(), std::io::stdout().as_raw_handle(), std::io::stderr().as_raw_handle()] {
            let mut flags = 0;
            // SAFETY: reads a flag of this process's own handle.
            if !handle.is_null() && unsafe { GetHandleInformation(handle, &mut flags) } != 0 {
                assert_eq!(flags & HANDLE_FLAG_INHERIT, 0);
            }
        }
    }
}
